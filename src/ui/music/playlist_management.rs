// SPDX-License-Identifier: GPL-3.0-or-later
//! Native presentation of revision-bound playlist edits. The service owns every
//! remote contract; this module never constructs URLs, diffs or optimistic data.

use super::*;
use crate::music::{
    PlaylistId,
    interactions::InteractionError,
    playlist_creation::PlaylistVisibility,
    playlist_management::{EditablePlaylist, PlaylistChange, PlaylistMutationOutcome},
};

#[derive(Default)]
pub(super) struct State {
    dialog: Option<adw::Dialog>,
    confirmation: Option<adw::AlertDialog>,
    popover: Option<gtk::Popover>,
    epoch: u64,
    reading: bool,
    detail_anchor: glib::WeakRef<gtk::Button>,
    detail_id: Option<PlaylistId>,
}

#[derive(Clone)]
enum Mode {
    Editor(PlaylistId),
    Picker(Track),
}

struct ViewState {
    generation: u64,
    epoch: u64,
    open: Cell<bool>,
    busy: Cell<bool>,
    uncertain: Cell<bool>,
    detail_view: Cell<u64>,
    mode: Mode,
    snapshot: RefCell<Option<EditablePlaylist>>,
    reconciliation: RefCell<Option<PlaylistId>>,
    completed: Cell<bool>,
    waiting_other: Cell<bool>,
    page: Cell<usize>,
    choices: RefCell<Vec<Playlist>>,
    detail_anchor: RefCell<glib::WeakRef<gtk::Button>>,
    presenting: Cell<bool>,
    presentation_epoch: Cell<u64>,
}

#[derive(Clone)]
struct Form {
    controller: std::rc::Weak<Controller>,
    state: Rc<ViewState>,
    dialog: glib::WeakRef<adw::Dialog>,
    controls: glib::WeakRef<gtk::Box>,
    content: glib::WeakRef<gtk::Box>,
    message: glib::WeakRef<gtk::Label>,
    refresh: glib::WeakRef<gtk::Button>,
    spinner: glib::WeakRef<gtk::Spinner>,
    title: glib::WeakRef<adw::EntryRow>,
    rename: glib::WeakRef<gtk::Button>,
    visibility: glib::WeakRef<adw::ComboRow>,
}

impl Form {
    fn retire_row_actions(&self) -> u64 {
        let epoch = self.state.presentation_epoch.get().wrapping_add(1);
        self.state.presentation_epoch.set(epoch);
        epoch
    }

    fn alive(&self, this: &Controller) -> bool {
        let model = this.model.borrow();
        if !self.state.open.get()
            || model.closed
            || !model.account_valid
            || model.generation != self.state.generation
        {
            return false;
        }
        let state = this.playlist_management.borrow();
        state.epoch == self.state.epoch && self.dialog.upgrade().as_ref() == state.dialog.as_ref()
    }

    fn message(&self, value: &str, error: bool) {
        if let Some(label) = self.message.upgrade() {
            label.set_text(value);
            if error {
                label.add_css_class("error");
                label.remove_css_class("dim-label");
            } else {
                label.remove_css_class("error");
                label.add_css_class("dim-label");
            }
        }
    }

    fn sync(&self) {
        let Some(this) = self.controller.upgrade() else {
            return;
        };
        let reading = this.playlist_management.borrow().reading;
        let blocked = self.state.busy.get() || reading || this.mutation_busy.get();
        let other_busy = this.mutation_busy.get() && !self.state.busy.get();
        if other_busy && !self.state.uncertain.get() {
            self.state.waiting_other.set(true);
            self.message("Другая правка библиотеки ещё выполняется. Действия станут доступны после её завершения.", false);
        } else if self.state.waiting_other.replace(false) && !self.state.uncertain.get() {
            self.message(
                "Можно продолжить. Каждое изменение проверяется на сервере.",
                false,
            );
        }
        let available = self.state.snapshot.borrow().is_some();
        if let Some(controls) = self.controls.upgrade() {
            controls.set_sensitive(!blocked && !self.state.uncertain.get());
        }
        if let Some(rename) = self.rename.upgrade() {
            let changed = self.title.upgrade().is_some_and(|title| {
                valid_title(&title.text())
                    && self
                        .state
                        .snapshot
                        .borrow()
                        .as_ref()
                        .is_some_and(|snapshot| snapshot.playlist.title != title.text().trim())
            });
            rename.set_sensitive(available && changed);
        }
        if let Some(visibility) = self.visibility.upgrade() {
            visibility.set_sensitive(
                self.state
                    .snapshot
                    .borrow()
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.visibility.is_some()),
            );
        }
        if let Some(refresh) = self.refresh.upgrade() {
            refresh.set_sensitive(!blocked && !self.state.completed.get());
        }
        if let Some(spinner) = self.spinner.upgrade() {
            spinner.set_spinning(blocked);
            spinner.set_visible(blocked);
        }
    }

    fn close(&self) {
        self.state.open.set(false);
        if let Some(dialog) = self.dialog.upgrade() {
            dialog.close();
        }
    }
}

impl Controller {
    /// One shared native editor/picker. Session teardown also retires a nested
    /// confirmation, so an old dialog can never apply to a replacement account.
    pub(super) fn retire_playlist_management(&self) {
        let (dialog, confirmation, popover) = {
            let mut state = self.playlist_management.borrow_mut();
            state.epoch = state.epoch.wrapping_add(1);
            (
                state.dialog.take(),
                state.confirmation.take(),
                state.popover.take(),
            )
        };
        if let Some(confirmation) = confirmation {
            confirmation.force_close();
        }
        if let Some(dialog) = dialog {
            dialog.force_close();
        }
        if let Some(popover) = popover {
            popover.popdown();
            if popover.parent().is_some() {
                popover.unparent();
            }
        }
    }

    pub(super) fn append_playlist_management_action(self: &Rc<Self>, playlist: &Playlist) {
        // The current validated account establishes presentation eligibility.
        // The service checks ownership again before loading any editor or write.
        let owned = {
            let model = self.model.borrow();
            self.service
                .as_ref()
                .zip(model.token.as_ref())
                .is_some_and(|(service, token)| {
                    service.owns_playlist(token, &playlist.id).unwrap_or(false)
                })
        };
        if !owned {
            return;
        }
        let button = gtk::Button::with_label("Управление плейлистом");
        button.set_widget_name("music-playlist-manage");
        self.playlist_management.borrow_mut().detail_anchor = button.downgrade();
        self.playlist_management.borrow_mut().detail_id = Some(playlist.id.clone());
        let weak = Rc::downgrade(self);
        let playlist = playlist.clone();
        let generation = self.model.borrow().generation;
        button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade().filter(|this| {
                let model = this.model.borrow();
                model.generation == generation && model.account_valid && !model.closed
            }) {
                this.present_playlist_editor(playlist.clone());
            }
        });
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.append(&button);
        self.place_detail_actions(&actions);
    }

    pub(super) fn present_add_to_playlist(self: &Rc<Self>, track: Track) {
        self.present_playlist_management(Mode::Picker(track));
    }

    pub(super) fn present_playlist_editor(self: &Rc<Self>, playlist: Playlist) {
        self.present_playlist_management(Mode::Editor(playlist.id));
    }

    fn present_playlist_management(self: &Rc<Self>, mode: Mode) {
        let (generation, detail_view) = {
            let model = self.model.borrow();
            if model.closed || !model.account_valid || model.token.is_none() {
                return;
            }
            (model.generation, model.view_generation)
        };
        if let Some(dialog) = self.playlist_management.borrow().dialog.as_ref() {
            if let Some(window) = self.window.upgrade() {
                dialog.present(Some(&window));
            }
            return;
        }
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let picker = matches!(mode, Mode::Picker(_));
        let dialog = adw::Dialog::builder()
            .title(if picker {
                "Добавить в плейлист"
            } else {
                "Управление плейлистом"
            })
            .content_width(560)
            .content_height(620)
            .build();
        dialog.set_widget_name("music-playlist-manage-dialog");
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        let close = gtk::Button::with_label("Закрыть");
        close.set_widget_name("music-playlist-manage-close");
        header.pack_start(&close);
        toolbar.add_top_bar(&header);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
        super::super::margins(&body, 18);
        let message = text(
            if picker {
                "Выберите свой плейлист. Добавление выполняется один раз и проверяется на сервере."
            } else {
                "Загружаем актуальную версию и все позиции плейлиста…"
            },
            "dim-label",
        );
        message.set_widget_name("music-playlist-manage-message");
        message.set_wrap(true);
        message.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        message.set_width_chars(1);
        message.set_hexpand(true);
        body.append(&message);
        let spinner = gtk::Spinner::new();
        spinner.set_halign(gtk::Align::Start);
        body.append(&spinner);
        let refresh = gtk::Button::with_label(if picker {
            "Обновить мои плейлисты"
        } else {
            "Обновить с сервера"
        });
        refresh.set_widget_name("music-playlist-manage-refresh");
        refresh.set_halign(gtk::Align::Start);
        body.append(&refresh);
        let controls = gtk::Box::new(gtk::Orientation::Vertical, 14);
        controls.set_sensitive(false);
        body.append(&controls);
        let title = adw::EntryRow::new();
        title.set_title("Название плейлиста");
        title.set_widget_name("music-playlist-manage-rename-title");
        let rename = gtk::Button::with_label("Сохранить название");
        rename.set_widget_name("music-playlist-manage-rename");
        rename.set_halign(gtk::Align::Start);
        let visibility = adw::ComboRow::new();
        visibility.set_widget_name("music-playlist-manage-visibility");
        visibility.set_title("Кому доступен плейлист");
        visibility.set_model(Some(&gtk::StringList::new(&[
            "Только я",
            "Все",
            "Неизвестно",
        ])));
        visibility.set_selected(2);
        if !picker {
            let group = adw::PreferencesGroup::new();
            group.add(&title);
            group.add(&visibility);
            controls.append(&group);
            controls.append(&rename);
        }
        // Native composite preference rows stay mounted for this dialog's
        // lifetime. Rebuild only occurrence/picker content after readback;
        // disposing/reparenting their PreferencesGroup loses native row state.
        let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
        content.set_widget_name("music-playlist-manage-content");
        controls.append(&content);
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&body)
            .build();
        toolbar.set_content(Some(&scroller));
        dialog.set_child(Some(&toolbar));
        dialog.set_default_widget(Some(&close));
        dialog.set_focus(Some(&close));
        let epoch = {
            let mut state = self.playlist_management.borrow_mut();
            state.epoch = state.epoch.wrapping_add(1);
            state.dialog = Some(dialog.clone());
            state.epoch
        };
        let detail_anchor = {
            let state = self.playlist_management.borrow();
            match &mode {
                Mode::Editor(id) if state.detail_id.as_ref() == Some(id) => {
                    state.detail_anchor.clone()
                }
                _ => glib::WeakRef::new(),
            }
        };
        let form = Form {
            controller: Rc::downgrade(self),
            state: Rc::new(ViewState {
                generation,
                epoch,
                open: Cell::new(true),
                busy: Cell::new(false),
                uncertain: Cell::new(false),
                detail_view: Cell::new(detail_view),
                mode,
                snapshot: RefCell::new(None),
                reconciliation: RefCell::new(None),
                completed: Cell::new(false),
                waiting_other: Cell::new(false),
                page: Cell::new(0),
                choices: RefCell::new(Vec::new()),
                detail_anchor: RefCell::new(detail_anchor),
                presenting: Cell::new(false),
                presentation_epoch: Cell::new(0),
            }),
            dialog: dialog.downgrade(),
            controls: controls.downgrade(),
            content: content.downgrade(),
            message: message.downgrade(),
            refresh: refresh.downgrade(),
            spinner: spinner.downgrade(),
            title: title.downgrade(),
            rename: rename.downgrade(),
            visibility: visibility.downgrade(),
        };
        close.connect_clicked({
            let form = form.clone();
            move |_| form.close()
        });
        refresh.connect_clicked({
            let form = form.clone();
            let weak = Rc::downgrade(self);
            move |_| {
                if let Some(this) = weak.upgrade() {
                    this.read_playlist_management(form.clone());
                }
            }
        });
        title.connect_changed({
            let form = form.clone();
            move |title| {
                if !form.state.busy.get()
                    && !form.state.uncertain.get()
                    && !form.state.presenting.get()
                {
                    if valid_title(&title.text()) {
                        form.message(
                            "Название можно сохранить. Каждое изменение проверяется на сервере.",
                            false,
                        );
                    } else {
                        form.message(
                            "Введите название от 1 до 200 символов без переносов строк.",
                            true,
                        );
                    }
                }
                form.sync();
            }
        });
        rename.connect_clicked({
            let form = form.clone();
            let weak = Rc::downgrade(self);
            move |_| {
                if let (Some(this), Some(title)) = (weak.upgrade(), form.title.upgrade()) {
                    if valid_title(&title.text()) {
                        this.submit_playlist_change(
                            form.clone(),
                            PlaylistChange::Rename(title.text().trim().to_owned()),
                        );
                    } else {
                        form.message(
                            "Введите название от 1 до 200 символов без переносов строк.",
                            true,
                        );
                    }
                }
            }
        });
        visibility.connect_selected_notify({
            let form = form.clone();
            let weak = Rc::downgrade(self);
            move |row| {
                if form.state.presenting.get() {
                    return;
                }
                let desired = match row.selected() {
                    0 => Some(PlaylistVisibility::Private),
                    1 => Some(PlaylistVisibility::Public),
                    _ => None,
                };
                row.set_subtitle(match desired {
                    Some(PlaylistVisibility::Private) => "Личный плейлист виден только вам.",
                    Some(PlaylistVisibility::Public) => {
                        "Открытый плейлист доступен другим пользователям."
                    }
                    None => "Сервис не сообщил доступ. Значение не подставляется автоматически.",
                });
                let current = form
                    .state
                    .snapshot
                    .borrow()
                    .as_ref()
                    .and_then(|snapshot| snapshot.visibility);
                if current.is_some()
                    && desired.is_some()
                    && current != desired
                    && let Some(this) = weak.upgrade()
                {
                    this.submit_playlist_change(
                        form.clone(),
                        PlaylistChange::Visibility(desired.expect("checked selection")),
                    );
                }
            }
        });
        dialog.connect_closed({
            let weak = Rc::downgrade(self);
            let state = form.state.clone();
            move |closed| {
                state.open.set(false);
                if let Some(this) = weak.upgrade() {
                    let confirmation = {
                        let mut current = this.playlist_management.borrow_mut();
                        if current.dialog.as_ref() != Some(closed) {
                            return;
                        }
                        current.dialog.take();
                        current.confirmation.take()
                    };
                    if let Some(confirmation) = confirmation {
                        confirmation.force_close();
                    }
                }
            }
        });
        dialog.present(Some(&window));
        self.read_playlist_management(form.clone());
        glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
            let Some(this) = form.controller.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !form.alive(&this) {
                return glib::ControlFlow::Break;
            }
            form.sync();
            glib::ControlFlow::Continue
        });
    }

    fn read_playlist_management(self: &Rc<Self>, form: Form) {
        if !form.alive(self)
            || form.state.busy.get()
            || self.mutation_busy.get()
            || form.state.completed.get()
        {
            return;
        }
        if self.playlist_management.borrow().reading {
            form.message(
                "Предыдущее чтение ещё выполняется. Нажмите «Обновить» после его завершения.",
                false,
            );
            return;
        }
        let (Some(service), Some(app), Some(token)) = (
            self.service.clone(),
            self.app.upgrade(),
            self.model.borrow().token.clone(),
        ) else {
            return;
        };
        if token.is_expired() {
            self.show_error(MusicError::Unauthenticated);
            return;
        }
        self.playlist_management.borrow_mut().reading = true;
        form.message("Загружаем актуальные данные с сервера…", false);
        form.sync();
        let mode = form.state.mode.clone();
        let reconcile = form.state.uncertain.get();
        let reconciliation = form.state.reconciliation.borrow().clone();
        let hold = app.hold();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let _hold = hold;
            let result = gio::spawn_blocking(move || {
                if let Some(id) = reconciliation {
                    return service
                        .reconcile_playlist(&token, &id)
                        .map(ReadResult::Snapshot);
                }
                match mode {
                    Mode::Picker(_) => service
                        .playlist_edit_choices(&token)
                        .map(ReadResult::Playlists)
                        .map_err(Into::into),
                    Mode::Editor(id) => {
                        if reconcile {
                            service
                                .reconcile_playlist(&token, &id)
                                .map(ReadResult::Snapshot)
                        } else {
                            service
                                .playlist_edit_state(&token, &id)
                                .map(|state| ReadResult::Snapshot(Some(state)))
                        }
                    }
                }
            })
            .await
            .unwrap_or(Err(MusicError::Configuration.into()));
            let Some(this) = weak.upgrade() else { return };
            this.playlist_management.borrow_mut().reading = false;
            if !form.alive(&this) {
                return;
            }
            match result {
                Ok(ReadResult::Playlists(playlists)) => {
                    this.render_playlist_picker(&form, playlists)
                }
                Ok(ReadResult::Snapshot(Some(snapshot))) => {
                    if reconcile {
                        this.refresh_after_playlist_change(&form, Some(snapshot.playlist.clone()));
                    }
                    if matches!(form.state.mode, Mode::Picker(_)) {
                        // An ambiguous insertion is reconciled by an exact GET,
                        // never by replaying it or interpreting a summary as proof.
                        if let Some(controls) = form.content.upgrade() {
                            clear_box(&controls);
                            controls.append(&text(&snapshot.playlist.title, "heading"));
                            for entry in snapshot.entries.iter().take(50) {
                                controls.append(&text(
                                    entry
                                        .track
                                        .as_ref()
                                        .map(|track| track.title.as_str())
                                        .unwrap_or("Нет сведений о треке"),
                                    "body",
                                ));
                            }
                            if snapshot.entries.len() > 50 {
                                controls.append(&text("Показаны первые 50 позиций. Полный состав доступен в разделе «Плейлисты».", "dim-label"));
                            }
                        }
                        form.state.completed.set(true);
                        form.message("Актуальный состав прочитан с сервера. Проверьте список: повторная отправка не выполнялась. Можно закрыть окно.", false);
                    } else {
                        form.state.uncertain.set(false);
                        this.render_playlist_editor(&form, snapshot);
                        form.message(
                            "Данные обновлены. Каждое изменение проверяется на сервере.",
                            false,
                        );
                    }
                }
                Ok(ReadResult::Snapshot(None)) => {
                    form.state.snapshot.borrow_mut().take();
                    form.state.uncertain.set(true);
                    if let Some(controls) = form.content.upgrade() {
                        clear_box(&controls);
                    }
                    if let Some(parent) = form.controls.upgrade() {
                        parent.set_visible(false);
                    }
                    form.message(
                        "Плейлист удалён. Текущее воспроизведение и очередь сохранены.",
                        false,
                    );
                    this.refresh_after_playlist_change(&form, None);
                }
                Err(error) => {
                    form.state.uncertain.set(true);
                    form.message(edit_message(error), true);
                    if let InteractionError::Music(
                        error @ (MusicError::AccessDenied | MusicError::Unauthenticated),
                    ) = error
                    {
                        this.show_error(error);
                    }
                }
            }
            form.sync();
        });
    }

    fn render_playlist_picker(self: &Rc<Self>, form: &Form, playlists: Vec<Playlist>) {
        form.state.choices.replace(playlists);
        form.state.page.set(0);
        self.render_playlist_picker_page(form);
    }

    fn render_playlist_picker_page(self: &Rc<Self>, form: &Form) {
        let Some(controls) = form.content.upgrade() else {
            return;
        };
        let presentation = form.retire_row_actions();
        clear_box(&controls);
        if let Some(parent) = form.controls.upgrade() {
            parent.set_visible(true);
        }
        form.state.uncertain.set(false);
        let count = form.state.choices.borrow().len();
        if count == 0 {
            form.message(
                "У вас пока нет плейлистов. Сначала создайте плейлист в разделе «Плейлисты».",
                false,
            );
            return;
        }
        form.message("Выберите свой плейлист. Будет добавлена одна новая позиция; уже имеющиеся треки сохранятся.", false);
        let list = list();
        list.set_widget_name("music-playlist-manage-picker");
        let page = form.state.page.get().min(count.saturating_sub(1) / 50);
        form.state.page.set(page);
        for playlist in form
            .state
            .choices
            .borrow()
            .iter()
            .skip(page * 50)
            .take(50)
            .cloned()
        {
            let button = gtk::Button::new();
            button.set_widget_name("music-playlist-manage-pick");
            button.add_css_class("flat");
            let labels = gtk::Box::new(gtk::Orientation::Vertical, 4);
            labels.append(&text(&playlist.title, "heading"));
            labels.append(&text(&track_count_text(playlist.track_count), "dim-label"));
            super::super::margins(&labels, 8);
            button.set_child(Some(&labels));
            let weak = Rc::downgrade(self);
            let form = form.clone();
            button.connect_clicked(move |_| {
                if form.state.presentation_epoch.get() != presentation {
                    return;
                }
                if let Some(this) = weak.upgrade() {
                    this.insert_into_playlist(form.clone(), playlist.clone());
                }
            });
            list.append(&button);
        }
        controls.append(&list);
        if count > 50 {
            let pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let previous = gtk::Button::with_label("Назад");
            previous.set_sensitive(page > 0);
            let next = gtk::Button::with_label("Далее");
            next.set_sensitive((page + 1) * 50 < count);
            for (button, target) in [(&previous, page.saturating_sub(1)), (&next, page + 1)] {
                let weak = Rc::downgrade(self);
                let form = form.clone();
                button.connect_clicked(move |_| {
                    let Some(this) = weak.upgrade() else { return };
                    if !form.alive(&this)
                        || form.state.busy.get()
                        || form.state.uncertain.get()
                        || form.state.presentation_epoch.get() != presentation
                    {
                        return;
                    }
                    form.state.page.set(target);
                    this.render_playlist_picker_page(&form);
                    form.sync();
                });
            }
            pages.append(&previous);
            pages.append(&text(
                &format!(
                    "{}–{} из {count}",
                    page * 50 + 1,
                    ((page + 1) * 50).min(count)
                ),
                "dim-label",
            ));
            pages.append(&next);
            controls.append(&pages);
        }
    }

    fn insert_into_playlist(self: &Rc<Self>, form: Form, playlist: Playlist) {
        if !form.alive(self)
            || form.state.busy.get()
            || form.state.uncertain.get()
            || self.playlist_management.borrow().reading
        {
            return;
        }
        let Mode::Picker(track) = &form.state.mode else {
            return;
        };
        if self.mutation_busy.replace(true) {
            form.message("Другая правка библиотеки ещё выполняется. Добавление не началось; повторите действие после её завершения.", false);
            return;
        }
        let selected = track.clone();
        *form.state.reconciliation.borrow_mut() = Some(playlist.id.clone());
        form.state.busy.set(true);
        form.message(
            "Проверяем актуальную версию, добавляем и подтверждаем трек…",
            false,
        );
        form.sync();
        let cache = self.persistent_cache();
        self.invalidate_creation_reads();
        self.run(
            Operation::PlaylistMutation,
            move |service, token| {
                let result =
                    service
                        .playlist_edit_state(&token, &playlist.id)
                        .and_then(|snapshot| {
                            let at = snapshot.entries.len();
                            service.edit_playlist(
                                &token,
                                &snapshot,
                                PlaylistChange::Insert {
                                    track: selected,
                                    at,
                                },
                            )
                        });
                invalidate_cache(&service, &token, cache);
                Ok(result)
            },
            move |this, result| this.finish_playlist_change(form, result),
        );
    }

    fn render_playlist_editor(self: &Rc<Self>, form: &Form, snapshot: EditablePlaylist) {
        let Some(controls) = form.content.upgrade() else {
            return;
        };
        let Some(title) = form.title.upgrade() else {
            return;
        };
        let Some(visibility) = form.visibility.upgrade() else {
            return;
        };
        let presentation = form.retire_row_actions();
        clear_box(&controls);
        if let Some(parent) = form.controls.upgrade() {
            parent.set_visible(true);
        }
        // Store the confirmed snapshot before updating controls. The selected
        // signal must not interpret server readback as a deliberate new write.
        let current_visibility = snapshot.visibility;
        let count = snapshot.entries.len();
        let page = form.state.page.get().min(count.saturating_sub(1) / 50);
        form.state.page.set(page);
        let name = snapshot.playlist.title.clone();
        *form.state.snapshot.borrow_mut() = Some(snapshot);
        form.state.presenting.set(true);
        title.set_text(&name);
        visibility.set_model(Some(&gtk::StringList::new(
            if current_visibility.is_some() {
                &["Только я", "Все"]
            } else {
                &["Неизвестно"]
            },
        )));
        visibility.set_selected(match current_visibility {
            Some(PlaylistVisibility::Public) => 1,
            _ => 0,
        });
        visibility.set_subtitle(match current_visibility {
            Some(PlaylistVisibility::Private) => "Личный плейлист виден только вам.",
            Some(PlaylistVisibility::Public) => "Открытый плейлист доступен другим пользователям.",
            None => "Сервис не сообщил доступ. Значение не подставляется автоматически.",
        });
        form.state.presenting.set(false);
        let tracks = list();
        tracks.set_widget_name("music-playlist-manage-tracks");
        let snapshot = form.state.snapshot.borrow();
        for entry in snapshot
            .as_ref()
            .expect("assigned snapshot")
            .entries
            .iter()
            .skip(page * 50)
            .take(50)
        {
            let row = adw::ActionRow::new();
            row.set_widget_name("music-playlist-manage-occurrence");
            row.set_use_markup(false);
            row.set_title(
                entry
                    .track
                    .as_ref()
                    .map(|track| track.title.as_str())
                    .unwrap_or("Нет сведений о треке"),
            );
            let credits = entry
                .track
                .as_ref()
                .map(|track| track.artists.join(", "))
                .unwrap_or_else(|| "Метаданные недоступны".to_owned());
            row.set_subtitle(&format!("{} · {credits}", entry.position + 1));
            row.set_title_lines(2);
            row.set_subtitle_lines(1);
            for (name, icon, label, change, enabled) in [
                (
                    "music-playlist-manage-move-up",
                    "go-up-symbolic",
                    "Переместить выше",
                    PlaylistChange::Move {
                        from: entry.position,
                        to: entry.position.saturating_sub(1),
                    },
                    entry.position > 0 && entry.album_id.is_some(),
                ),
                (
                    "music-playlist-manage-move-down",
                    "go-down-symbolic",
                    "Переместить ниже",
                    PlaylistChange::Move {
                        from: entry.position,
                        to: entry.position + 1,
                    },
                    entry.position + 1 < count && entry.album_id.is_some(),
                ),
                (
                    "music-playlist-manage-remove",
                    "list-remove-symbolic",
                    "Удалить эту позицию из плейлиста",
                    PlaylistChange::Remove {
                        position: entry.position,
                    },
                    true,
                ),
            ] {
                let button = icon_button(icon, label);
                button.set_widget_name(name);
                button.set_sensitive(enabled);
                if entry.album_id.is_none() && name != "music-playlist-manage-remove" {
                    button.set_tooltip_text(Some("Перемещение недоступно: сервис не сообщил идентификатор альбома этой позиции."));
                }
                button.set_valign(gtk::Align::Center);
                button.add_css_class("flat");
                let weak = Rc::downgrade(self);
                let form = form.clone();
                button.connect_clicked(move |_| {
                    if form.state.presentation_epoch.get() != presentation {
                        return;
                    }
                    if let Some(this) = weak.upgrade() {
                        this.submit_playlist_change(form.clone(), change.clone());
                    }
                });
                row.add_suffix(&button);
            }
            tracks.append(&row);
        }
        drop(snapshot);
        controls.append(&text(
            &format!("Состав · {}", track_count_text(count as u64)),
            "heading",
        ));
        controls.append(&tracks);
        if count > 50 {
            let pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let previous = gtk::Button::with_label("Назад");
            previous.set_widget_name("music-playlist-manage-previous");
            previous.set_sensitive(page > 0);
            let next = gtk::Button::with_label("Далее");
            next.set_widget_name("music-playlist-manage-next");
            next.set_sensitive((page + 1) * 50 < count);
            for (button, target) in [(&previous, page.saturating_sub(1)), (&next, page + 1)] {
                let weak = Rc::downgrade(self);
                let form = form.clone();
                button.connect_clicked(move |_| {
                    let Some(this) = weak.upgrade() else { return };
                    if !form.alive(&this)
                        || form.state.busy.get()
                        || form.state.uncertain.get()
                        || form.state.presentation_epoch.get() != presentation
                    {
                        return;
                    }
                    let snapshot = form.state.snapshot.borrow().clone();
                    if let Some(snapshot) = snapshot {
                        form.state.page.set(target);
                        this.render_playlist_editor(&form, snapshot);
                        form.sync();
                    }
                });
            }
            pages.append(&previous);
            pages.append(&text(
                &format!(
                    "{}–{} из {count}",
                    page * 50 + 1,
                    ((page + 1) * 50).min(count)
                ),
                "dim-label",
            ));
            pages.append(&next);
            controls.append(&pages);
        }
        let delete = gtk::Button::with_label("Удалить плейлист…");
        delete.set_widget_name("music-playlist-manage-delete");
        delete.add_css_class("destructive-action");
        delete.set_halign(gtk::Align::Start);
        let weak = Rc::downgrade(self);
        let form = form.clone();
        delete.connect_clicked(move |_| {
            if form.state.presentation_epoch.get() != presentation {
                return;
            }
            if let Some(this) = weak.upgrade() {
                this.confirm_playlist_delete(form.clone());
            }
        });
        controls.append(&delete);
    }

    fn confirm_playlist_delete(self: &Rc<Self>, form: Form) {
        if !form.alive(self)
            || form.state.busy.get()
            || form.state.uncertain.get()
            || self.mutation_busy.get()
        {
            return;
        }
        let title = form
            .state
            .snapshot
            .borrow()
            .as_ref()
            .map(|snapshot| snapshot.playlist.title.clone());
        let (Some(title), Some(host)) = (title, form.dialog.upgrade()) else {
            return;
        };
        if self.playlist_management.borrow().confirmation.is_some() {
            return;
        }
        let confirmation = adw::AlertDialog::new(
            Some("Удалить плейлист?"),
            Some(&format!(
                "Плейлист «{title}» будет удалён из аккаунта. Текущее воспроизведение и очередь продолжатся."
            )),
        );
        confirmation.set_body_use_markup(false);
        confirmation.set_widget_name("music-playlist-manage-delete-confirmation");
        confirmation.add_responses(&[("cancel", "Отмена"), ("delete", "Удалить плейлист")]);
        confirmation.set_default_response(Some("cancel"));
        confirmation.set_close_response("cancel");
        confirmation.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        let weak = Rc::downgrade(self);
        let presentation = form.state.presentation_epoch.get();
        confirmation.connect_response(None, move |dialog, response| {
            let Some(this) = weak.upgrade() else { return };
            {
                let mut state = this.playlist_management.borrow_mut();
                if state.confirmation.as_ref() != Some(dialog) {
                    return;
                }
                state.confirmation.take();
            }
            if response == "delete"
                && form.alive(&this)
                && form.state.presentation_epoch.get() == presentation
            {
                this.submit_playlist_change(form.clone(), PlaylistChange::Delete);
            }
        });
        self.playlist_management.borrow_mut().confirmation = Some(confirmation.clone());
        confirmation.present(Some(&host));
    }

    fn submit_playlist_change(self: &Rc<Self>, form: Form, change: PlaylistChange) {
        if !form.alive(self)
            || form.state.busy.get()
            || form.state.uncertain.get()
            || self.playlist_management.borrow().reading
        {
            return;
        }
        let Some(snapshot) = form.state.snapshot.borrow().clone() else {
            return;
        };
        if self.mutation_busy.replace(true) {
            form.message("Другая правка библиотеки ещё выполняется. Изменение не началось; повторите действие после её завершения.", false);
            return;
        }
        form.state.busy.set(true);
        form.message("Отправляем одно изменение и проверяем результат…", false);
        form.sync();
        let cache = self.persistent_cache();
        self.invalidate_creation_reads();
        self.run(
            Operation::PlaylistMutation,
            move |service, token| {
                let result = service.edit_playlist(&token, &snapshot, change);
                invalidate_cache(&service, &token, cache);
                Ok(result)
            },
            move |this, result| this.finish_playlist_change(form, result),
        );
    }

    fn finish_playlist_change(
        self: &Rc<Self>,
        form: Form,
        result: Result<Result<PlaylistMutationOutcome, InteractionError>, MusicError>,
    ) {
        self.mutation_busy.set(false);
        self.invalidate_creation_reads();
        form.state.busy.set(false);
        let result = result.unwrap_or_else(|error| Err(error.into()));
        // A deliberate close only retires presentation. A completed write must
        // still refresh a surviving, matching collection view; it must never
        // reopen its modal or alter the immutable active playback context.
        match &result {
            Ok(PlaylistMutationOutcome::Confirmed(snapshot)) => {
                self.refresh_after_playlist_change(&form, Some(snapshot.playlist.clone()));
            }
            Ok(PlaylistMutationOutcome::Deleted) => {
                self.refresh_after_playlist_change(&form, None);
            }
            Err(_) => {}
        }
        if !form.alive(self) {
            return;
        }
        match result {
            Ok(PlaylistMutationOutcome::Confirmed(snapshot)) => {
                if matches!(form.state.mode, Mode::Picker(_)) {
                    form.state.uncertain.set(true);
                    form.state.completed.set(true);
                    if let Some(controls) = form.content.upgrade() {
                        clear_box(&controls);
                    }
                    form.message("Трек добавлен. Состав плейлиста подтверждён на сервере. Можно закрыть это окно.", false);
                } else {
                    self.render_playlist_editor(&form, *snapshot);
                    form.message("Изменение подтверждено на сервере.", false);
                }
            }
            Ok(PlaylistMutationOutcome::Deleted) => {
                form.state.snapshot.borrow_mut().take();
                form.state.uncertain.set(true);
                if let Some(controls) = form.content.upgrade() {
                    clear_box(&controls);
                }
                if let Some(parent) = form.controls.upgrade() {
                    parent.set_visible(false);
                }
                form.message(
                    "Плейлист удалён. Текущее воспроизведение и очередь сохранены.",
                    false,
                );
            }
            Err(error) => {
                form.state.uncertain.set(true);
                form.message(edit_message(error), true);
                if let InteractionError::Music(
                    error @ (MusicError::AccessDenied | MusicError::Unauthenticated),
                ) = error
                {
                    self.show_error(error);
                }
            }
        }
        form.sync();
    }

    fn refresh_after_playlist_change(self: &Rc<Self>, form: &Form, playlist: Option<Playlist>) {
        let unchanged = self.model.borrow().view_generation == form.state.detail_view.get()
            && form
                .state
                .detail_anchor
                .borrow()
                .upgrade()
                .is_some_and(|anchor| anchor.root().is_some());
        let visible = self.widgets.stack.visible_child_name();
        if unchanged
            && matches!(form.state.mode, Mode::Editor(_))
            && visible.as_deref() == Some("detail")
        {
            if let Some(playlist) = playlist {
                self.open_playlist_policy(playlist, 0, true);
                form.state
                    .detail_view
                    .set(self.model.borrow().view_generation);
                *form.state.detail_anchor.borrow_mut() =
                    self.playlist_management.borrow().detail_anchor.clone();
            } else {
                self.back_from_detail();
                self.widgets.stack.set_visible_child_name("playlists");
                self.widgets.library_mode.set_selected(1);
                self.load_library_refresh(0);
            }
        } else if visible.as_deref() == Some("playlists") {
            self.load_library_refresh(0);
        }
    }

    pub(super) fn connect_playlist_management(self: &Rc<Self>) {
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3);
        gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        gesture.connect_pressed(move |gesture, _, _, _| {
            if let Some(this) = weak.upgrade() {
                this.present_current_playlist_menu();
            }
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        self.widgets.current_art_button.add_controller(gesture);
        let keyboard = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        keyboard.connect_key_pressed(move |_, key, _, modifiers| {
            if key == gtk::gdk::Key::Menu
                || (key == gtk::gdk::Key::F10
                    && modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK))
            {
                if let Some(this) = weak.upgrade() {
                    this.present_current_playlist_menu();
                }
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.widgets.current_art_button.add_controller(keyboard);
    }

    fn present_current_playlist_menu(self: &Rc<Self>) {
        let track = {
            let model = self.model.borrow();
            if !model.account_valid || model.closed {
                return;
            }
            model.queue.current().cloned()
        };
        let Some(track) = track else { return };
        let previous = self.playlist_management.borrow_mut().popover.take();
        if let Some(previous) = previous {
            previous.popdown();
            if previous.parent().is_some() {
                previous.unparent();
            }
        }
        let popover = gtk::Popover::new();
        popover.set_parent(&self.widgets.current_art_button);
        let button = gtk::Button::with_label("Добавить в плейлист");
        button.set_widget_name("music-current-add-playlist");
        button.add_css_class("flat");
        popover.set_child(Some(&button));
        let weak = Rc::downgrade(self);
        let close = popover.downgrade();
        let generation = self.model.borrow().generation;
        button.connect_clicked(move |_| {
            if let Some(close) = close.upgrade() {
                close.popdown();
            }
            if let Some(this) = weak
                .upgrade()
                .filter(|this| this.model.borrow().generation == generation)
            {
                this.present_add_to_playlist(track.clone());
            }
        });
        self.playlist_management.borrow_mut().popover = Some(popover.clone());
        popover.popup();
    }

    #[cfg(test)]
    pub(super) fn playlist_management_dialog_for_test(&self) -> Option<adw::Dialog> {
        self.playlist_management.borrow().dialog.clone()
    }

    #[cfg(test)]
    pub(super) fn playlist_management_confirmation_for_test(&self) -> Option<adw::AlertDialog> {
        self.playlist_management.borrow().confirmation.clone()
    }
}

enum ReadResult {
    Playlists(Vec<Playlist>),
    Snapshot(Option<EditablePlaylist>),
}

fn valid_title(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 1024
        && value.chars().count() <= 200
        && !value.chars().any(char::is_control)
}

fn invalidate_cache(
    service: &MusicService,
    token: &AccessToken,
    cache: Option<crate::cache::Cache>,
) {
    if !token.is_expired()
        && service.validate_session(token).is_ok()
        && let Some(cache) = cache
    {
        let _ = cache.invalidate_metadata();
    }
}

fn edit_message(error: InteractionError) -> &'static str {
    match error {
        InteractionError::Conflict => {
            "Плейлист изменился на другом устройстве. Нажмите «Обновить с сервера», проверьте новый состав и повторите действие самостоятельно."
        }
        InteractionError::WriteOutcomeUnknown => {
            "Изменение могло быть принято, но надёжный ответ не получен. Нажмите «Обновить с сервера». Повторная отправка не выполняется."
        }
        InteractionError::ConfirmationUnavailable => {
            "Изменение отправлено, но результат пока не подтверждён. Нажмите «Обновить с сервера»: выполняется только чтение."
        }
        InteractionError::ConfirmationMismatch => {
            "Сервис не подтвердил ожидаемый состав или свойства. Обновите данные перед новой правкой."
        }
        InteractionError::Music(MusicError::InvalidInput) => {
            "Редактирование недоступно: нужны ваш плейлист, подтверждённая версия и полный список позиций. Для названия используйте от 1 до 200 символов."
        }
        InteractionError::Music(error) if matches!(error.http_status(), Some(403 | 451)) => {
            "Изменение этого плейлиста недоступно для текущего аккаунта. Можно обновить данные; воспроизведение продолжает работать."
        }
        InteractionError::Music(error) => super::music_error_message(error).1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_title_validation_counts_unicode_characters() {
        assert!(valid_title(&"я".repeat(200)));
        assert!(!valid_title(&"я".repeat(201)));
        assert!(!valid_title(" \n "));
        assert!(!valid_title("Имя\nплейлиста"));
        assert!(valid_title("  Мой плейлист 🎶  "));
    }
}
