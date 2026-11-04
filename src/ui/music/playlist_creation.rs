// SPDX-License-Identifier: GPL-3.0-or-later
//! One deliberate native playlist write, with server confirmation and no replay.

use super::*;
use crate::music::{interactions::InteractionError, playlist_creation::PlaylistVisibility};

struct State {
    generation: u64,
    open: Cell<bool>,
    busy: Cell<bool>,
    uncertain: Cell<bool>,
    waiting_other: Cell<bool>,
}

#[derive(Clone)]
struct Form {
    controller: std::rc::Weak<Controller>,
    state: Rc<State>,
    dialog: glib::WeakRef<adw::Dialog>,
    title: glib::WeakRef<adw::EntryRow>,
    visibility: glib::WeakRef<adw::ComboRow>,
    create: glib::WeakRef<gtk::Button>,
    cancel: glib::WeakRef<gtk::Button>,
    refresh: glib::WeakRef<gtk::Button>,
    progress: glib::WeakRef<gtk::Spinner>,
    message: glib::WeakRef<gtk::Label>,
}

impl Form {
    fn alive(&self, this: &Controller) -> bool {
        let model = this.model.borrow();
        if !self.state.open.get()
            || model.closed
            || !model.account_valid
            || model.generation != self.state.generation
        {
            return false;
        }
        let Some(dialog) = self.dialog.upgrade() else {
            return false;
        };
        dialog.parent().is_some() && this.playlist_dialog.borrow().as_ref() == Some(&dialog)
    }

    fn sync(&self) {
        let busy = self.state.busy.get();
        let this = self.controller.upgrade();
        let supported = this
            .as_ref()
            .is_some_and(|this| this.can_create_playlist.get());
        let other_busy = this.as_ref().is_some_and(|this| this.mutation_busy.get()) && !busy;
        if !supported {
            self.state.uncertain.set(true);
            self.message(
                "Создание плейлистов сейчас недоступно для этого аккаунта.",
                true,
            );
            if let Some(refresh) = self.refresh.upgrade() {
                refresh.set_visible(true);
            }
        } else if other_busy {
            self.state.waiting_other.set(true);
            self.message("Другая правка библиотеки ещё выполняется. Создание станет доступно после её завершения.",false);
        } else if self.state.waiting_other.replace(false) && !self.state.uncertain.get() {
            self.message(
                "Плейлист появится в вашей библиотеке после проверки результата.",
                false,
            );
        }
        let valid = self
            .title
            .upgrade()
            .is_some_and(|title| valid_title(&title.text()));
        if let Some(button) = self.create.upgrade() {
            button.set_sensitive(
                self.state.open.get()
                    && !busy
                    && !other_busy
                    && supported
                    && !self.state.uncertain.get()
                    && valid,
            );
        }
        if let Some(title) = self.title.upgrade() {
            title.set_sensitive(!busy && !self.state.uncertain.get());
        }
        if let Some(visibility) = self.visibility.upgrade() {
            visibility.set_sensitive(!busy && !self.state.uncertain.get());
        }
        if let Some(refresh) = self.refresh.upgrade() {
            refresh.set_sensitive(!busy);
        }
        if let Some(spinner) = self.progress.upgrade() {
            spinner.set_visible(busy);
            spinner.set_spinning(busy);
        }
        if let Some(cancel) = self.cancel.upgrade() {
            cancel.set_label(if busy {
                "Закрыть"
            } else {
                "Отмена"
            });
            cancel.set_tooltip_text(
                busy.then_some("Создание уже началось. Закрытие окна не отменяет его."),
            );
        }
    }

    fn message(&self, value: &str, error: bool) {
        if error
            && self.state.uncertain.get()
            && let Some(dialog) = self.dialog.upgrade()
        {
            dialog.set_content_height(420);
        }
        if let Some(label) = self.message.upgrade() {
            label.set_text(value);
            if error {
                label.remove_css_class("dim-label");
                label.add_css_class("error");
            } else {
                label.remove_css_class("error");
                label.add_css_class("dim-label");
            }
        }
    }

    fn close(&self) {
        self.state.open.set(false);
        self.sync();
        if let Some(dialog) = self.dialog.upgrade()
            && dialog.parent().is_some()
        {
            dialog.close();
        }
    }
}

impl Controller {
    pub(super) fn present_create_playlist(self: &Rc<Self>) {
        {
            let model = self.model.borrow();
            if model.closed || !model.account_valid || !self.can_create_playlist.get() {
                return;
            }
        }
        if let Some(existing) = self.playlist_dialog.borrow().as_ref() {
            if let Some(window) = self.window.upgrade() {
                existing.present(Some(&window));
            }
            return;
        }
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let dialog = adw::Dialog::builder()
            .title("Новый плейлист")
            .content_width(420)
            .content_height(360)
            .build();
        dialog.set_widget_name("music-create-playlist-dialog");
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        header.set_show_start_title_buttons(false);
        header.set_show_end_title_buttons(false);
        let cancel = gtk::Button::with_label("Отмена");
        cancel.set_widget_name("music-create-playlist-cancel");
        let create = gtk::Button::with_label("Создать");
        create.set_widget_name("music-create-playlist-submit");
        create.add_css_class("suggested-action");
        create.set_sensitive(false);
        header.pack_start(&cancel);
        header.pack_end(&create);
        toolbar.add_top_bar(&header);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
        super::super::margins(&body, 18);
        let group = adw::PreferencesGroup::new();
        let title = adw::EntryRow::new();
        title.set_title("Название");
        title.set_widget_name("music-create-playlist-title");
        group.add(&title);
        let visibility = adw::ComboRow::new();
        visibility.set_widget_name("music-create-playlist-visibility");
        visibility.set_title("Доступ");
        visibility.set_model(Some(&gtk::StringList::new(&["Только я", "Все"])));
        visibility.set_selected(0);
        visibility.set_subtitle("Личный плейлист виден только вам.");
        visibility.set_title_lines(0);
        visibility.set_subtitle_lines(0);
        visibility.connect_selected_notify(|row| {
            row.set_subtitle(if row.selected() == 0 {
                "Личный плейлист виден только вам."
            } else {
                "Открытый плейлист доступен другим пользователям."
            });
        });
        group.add(&visibility);
        body.append(&group);
        let progress = gtk::Spinner::new();
        progress.set_widget_name("music-create-playlist-progress");
        progress.set_halign(gtk::Align::Start);
        progress.set_visible(false);
        body.append(&progress);
        let message = text(
            "Плейлист появится в вашей библиотеке после проверки результата.",
            "dim-label",
        );
        message.set_widget_name("music-create-playlist-message");
        message.set_wrap(true);
        message.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        message.set_width_chars(1);
        message.set_hexpand(true);
        body.append(&message);
        let refresh = gtk::Button::with_label("Обновить мои плейлисты");
        refresh.set_widget_name("music-create-playlist-refresh");
        refresh.set_halign(gtk::Align::Start);
        refresh.set_visible(false);
        body.append(&refresh);
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .child(&body)
            .build();
        toolbar.set_content(Some(&scroller));
        dialog.set_child(Some(&toolbar));
        dialog.set_default_widget(Some(&cancel));
        dialog.set_focus(Some(&title));

        let form = Form {
            controller: Rc::downgrade(self),
            state: Rc::new(State {
                generation: self.model.borrow().generation,
                open: Cell::new(true),
                busy: Cell::new(false),
                uncertain: Cell::new(false),
                waiting_other: Cell::new(false),
            }),
            dialog: dialog.downgrade(),
            title: title.downgrade(),
            visibility: visibility.downgrade(),
            create: create.downgrade(),
            cancel: cancel.downgrade(),
            refresh: refresh.downgrade(),
            progress: progress.downgrade(),
            message: message.downgrade(),
        };
        title.connect_changed({
            let form = form.clone();
            move |entry| {
                form.sync();
                if !entry.text().is_empty() && !valid_title(&entry.text()) {
                    form.message(
                        "Введите название от 1 до 200 символов без переносов строк.",
                        true,
                    );
                } else if !form.state.busy.get() && !form.state.uncertain.get() {
                    form.message(
                        "Плейлист появится в вашей библиотеке после проверки результата.",
                        false,
                    );
                }
            }
        });
        cancel.connect_clicked({
            let form = form.clone();
            move |_| form.close()
        });
        create.connect_clicked({
            let weak = Rc::downgrade(self);
            let form = form.clone();
            move |_| {
                if let Some(this) = weak.upgrade() {
                    this.submit_create_playlist(form.clone());
                }
            }
        });
        refresh.connect_clicked({
            let weak = Rc::downgrade(self);
            let form = form.clone();
            move |_| {
                let Some(this) = weak.upgrade() else { return };
                if !form.alive(&this) || form.state.busy.get() || !form.state.uncertain.get() {
                    return;
                }
                form.close();
                this.widgets.stack.set_visible_child_name("playlists");
                this.widgets.library_mode.set_selected(1);
                this.load_library_refresh(0);
            }
        });
        dialog.connect_closed({
            let weak = Rc::downgrade(self);
            let state = form.state.clone();
            move |closed| {
                state.open.set(false);
                if let Some(this) = weak.upgrade() {
                    let current = this.playlist_dialog.borrow().as_ref() == Some(closed);
                    if current {
                        this.playlist_dialog.borrow_mut().take();
                    }
                }
            }
        });
        *self.playlist_dialog.borrow_mut() = Some(dialog.clone());
        form.sync();
        dialog.present(Some(&window));
        // A different confirmed library write can finish while this dialog is
        // open. Keep the one dialog's controls in sync without queuing a POST
        // or holding GTK/controller ownership after it closes.
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

    fn submit_create_playlist(self: &Rc<Self>, form: Form) {
        if !form.alive(self) || form.state.busy.get() || form.state.uncertain.get() {
            return;
        }
        let Some(title) = form.title.upgrade() else {
            return;
        };
        let Some(visibility) = form.visibility.upgrade() else {
            return;
        };
        if !valid_title(&title.text()) || !self.can_create_playlist.get() {
            return;
        }
        let visibility = match visibility.selected() {
            0 => PlaylistVisibility::Private,
            1 => PlaylistVisibility::Public,
            _ => {
                form.message("Выберите, кому будет доступен плейлист.", true);
                return;
            }
        };
        if self.mutation_busy.replace(true) {
            form.message("Другая правка библиотеки ещё выполняется. Создание не началось; повторите действие после её завершения.", false);
            return;
        }
        let title = title.text().trim().to_owned();
        let page = self.model.borrow().library_page;
        let view = self.model.borrow().view_generation;
        let mode = self.widgets.library_mode.selected();
        self.invalidate_creation_reads();
        form.state.busy.set(true);
        form.message("Создаём и проверяем плейлист…", false);
        form.sync();
        let cache = self.persistent_cache();
        self.run(
            Operation::Mutation,
            move |service, token| {
                let result = service.create_playlist(&token, &title, visibility);
                if !token.is_expired()
                    && service.validate_session(&token).is_ok()
                    && let Some(cache) = cache
                {
                    // Even a timeout can follow an accepted remote write.
                    // Old leases cannot resurrect pre-write metadata.
                    let _ = cache.invalidate_metadata();
                }
                Ok(result)
            },
            move |this, result| {
                this.mutation_busy.set(false);
                this.invalidate_creation_reads();
                let result = result.unwrap_or_else(|error| Err(InteractionError::Music(error)));
                form.state.busy.set(false);
                if let Err(InteractionError::Music(
                    error @ (MusicError::AccessDenied | MusicError::Unauthenticated),
                )) = result {
                    this.show_error(error);
                    return;
                }
                if matches!(result,Err(InteractionError::Music(error)) if matches!(error.http_status(),Some(403|451))) {
                    this.can_create_playlist.set(false);
                    this.playlist_grid.hide_create();
                }
                if !form.alive(this) {
                    // The remote write can complete after a deliberate close.
                    // Invalidation above still happens; no old dialog reopens.
                    return;
                }
                match result {
                    Ok(_) => {
                        form.close();
                        let same_view = {
                            let model = this.model.borrow();
                            model.view_generation == view && model.library_page == page
                        };
                        if same_view
                            && mode != 0
                            && this.widgets.library_mode.selected() != 0
                            && this.widgets.stack.visible_child_name().as_deref() == Some("playlists")
                        {
                            this.load_library_refresh(0);
                        }
                    }
                    Err(error) => {
                        let forbidden = matches!(error, InteractionError::Music(e) if matches!(e.http_status(),Some(403|451)));
                        if forbidden {
                            this.can_create_playlist.set(false);
                        }
                        form.state.uncertain.set(forbidden || matches!(error,
                            InteractionError::Conflict | InteractionError::WriteOutcomeUnknown
                            | InteractionError::ConfirmationUnavailable
                            | InteractionError::ConfirmationMismatch));
                        form.message(creation_message(error), true);
                        if let Some(refresh) = form.refresh.upgrade() {
                            refresh.set_visible(form.state.uncertain.get());
                        }
                        form.sync();
                    }
                }
            },
        );
    }

    fn invalidate_creation_reads(self: &Rc<Self>) {
        self.interrupt_feed_reads(Operation::Library);
        let mut requests = self.requests.borrow_mut();
        requests.invalidate(Operation::Library);
        requests.invalidate(Operation::InitialLibrary);
        requests.invalidate(Operation::PlaylistSections);
        requests.invalidate(Operation::OwnedPlaylistsMore);
        requests.invalidate(Operation::LikedPlaylistsMore);
        drop(requests);
        self.playlist_refreshing.set([false, false]);
        let mut model = self.model.borrow_mut();
        model.liked = None;
        model.playlists = None;
        model.liked_playlists = None;
        drop(model);
        self.render_home_preview();
    }
}

fn valid_title(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.chars().count() <= 200
        && value.len() <= 1024
        && !value.chars().any(char::is_control)
}

fn creation_message(error: InteractionError) -> &'static str {
    match error {
        InteractionError::WriteOutcomeUnknown => {
            "Создание могло завершиться, но надёжный ответ не получен. Обновите мои плейлисты; повторно создавать плейлист автоматически не будем."
        }
        InteractionError::ConfirmationUnavailable => {
            "Плейлист отправлен на создание, но результат не удалось проверить. Обновите мои плейлисты, чтобы увидеть актуальное состояние."
        }
        InteractionError::ConfirmationMismatch => {
            "Сервис не подтвердил ожидаемые название, доступ или владельца. Обновите мои плейлисты перед новой попыткой."
        }
        InteractionError::Conflict => {
            "Библиотека изменилась. Обновите мои плейлисты перед новой попыткой."
        }
        InteractionError::Music(error) if matches!(error.http_status(), Some(403 | 451)) => {
            "Создание плейлистов сейчас недоступно для этого аккаунта. Остальная библиотека продолжает работать."
        }
        InteractionError::Music(MusicError::InvalidInput) => {
            "Проверьте название: от 1 до 200 символов без переносов строк."
        }
        InteractionError::Music(error) => super::music_error_message(error).1,
    }
}

#[cfg(test)]
mod tests;
