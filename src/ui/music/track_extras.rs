// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded lyrics and related tracks in separate modes of the existing pane.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use gtk::{gio, glib};

use super::{Controller, MusicError, Operation, Track, icon_button};
use crate::{
    artwork::ArtworkLoader,
    lyrics::Lyrics,
    music::{
        interactions::{InteractionError, TrackReaction},
        track_extras::{LyricsRequest, SimilarTracks},
    },
    playback::PlaybackState,
};

const RELATED_PAGE: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ticket {
    selection: u64,
    request: u64,
}

#[derive(Clone, Copy)]
pub(super) enum RelatedAction {
    Play,
    Next,
    Enqueue,
    Details,
    Like,
    Dislike,
}

#[derive(Clone)]
pub(super) enum ExtrasAction {
    Lyrics {
        track: Track,
        ticket: Ticket,
        request: LyricsRequest,
    },
    Similar {
        track: Track,
        ticket: Ticket,
    },
    Related {
        track: Track,
        ticket: Ticket,
        action: RelatedAction,
    },
}

// Both roots belong to the same TrackPane stack. Signal closures hold Weak;
// lyrics, recommendations and copy state disappear with that pane/session.
struct Inner {
    widget: gtk::Box,
    lyrics_widget: gtk::Box,
    action: Rc<dyn Fn(ExtrasAction)>,
    selected: RefCell<Option<Track>>,
    epoch: Cell<u64>,
    info_active: Cell<bool>,
    lyrics_active: Cell<bool>,
    visible_scheduled: Cell<bool>,
    lyrics_serial: Cell<u64>,
    similar_serial: Cell<u64>,
    lyrics_started: Cell<bool>,
    similar_started: Cell<bool>,
    lyrics_loading: Cell<bool>,
    similar_loading: Cell<bool>,
    lyrics_request: Cell<LyricsRequest>,
    lyrics: RefCell<Option<Lyrics>>,
    synchronized: Cell<bool>,
    active: Cell<Option<u32>>,
    following: Cell<bool>,
    follow_revision: Cell<u64>,
    scroll_animation: RefCell<Option<adw::TimedAnimation>>,
    visible_lines: RefCell<Vec<(u32, glib::WeakRef<gtk::Label>)>>,
    lyrics_title: gtk::Label,
    lyrics_artist: gtk::Label,
    lyrics_status: gtk::Label,
    lyrics_spinner: gtk::Spinner,
    source: gtk::Label,
    lyrics_retry: gtk::Button,
    external: gtk::Button,
    external_action: gio::SimpleAction,
    external_consent: RefCell<Option<adw::AlertDialog>>,
    copy: gio::SimpleAction,
    #[cfg(test)]
    lyrics_actions: gtk::MenuButton,
    follow: gtk::Button,
    lyrics_model: gtk::StringList,
    lyrics_list: gtk::ListView,
    lyrics_scroll: gtk::ScrolledWindow,
    similar_status: gtk::Label,
    similar_spinner: gtk::Spinner,
    similar_retry: gtk::Button,
    similar_rows: gtk::ListBox,
    similar_more: gtk::Button,
    related: RefCell<Vec<Track>>,
    shown: Cell<usize>,
    loader: RefCell<Option<ArtworkLoader>>,
}

pub(super) struct ExtrasView {
    /// Normal Track Info contains recommendations, never the song text.
    pub widget: gtk::Box,
    pub(super) lyrics_widget: gtk::Box,
    inner: Rc<Inner>,
}

impl ExtrasView {
    pub(super) fn new(action: Rc<dyn Fn(ExtrasAction)>) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 10);
        widget.set_widget_name("music-track-extras");
        let similar_heading = wrap("Похожие треки", "heading");
        similar_heading.set_widget_name("music-similar-heading");
        widget.append(&similar_heading);
        let (similar_status, similar_spinner, similar_retry) =
            feedback(&widget, "Рекомендации Яндекс Музыки", "music-similar");
        let similar_rows = super::list();
        similar_rows.set_widget_name("music-similar-rows");
        similar_rows.remove_css_class("boxed-list");
        similar_rows.add_css_class("music-track-list");
        widget.append(&similar_rows);
        let similar_more = gtk::Button::with_label("Показать ещё");
        similar_more.set_widget_name("music-similar-more");
        similar_more.set_halign(gtk::Align::Start);
        similar_more.set_visible(false);
        widget.append(&similar_more);

        let lyrics_widget = gtk::Box::new(gtk::Orientation::Vertical, 8);
        lyrics_widget.set_widget_name("music-lyrics-view");
        lyrics_widget.set_vexpand(true);
        super::super::margins(&lyrics_widget, 16);
        lyrics_widget.set_margin_top(8);
        let lyrics_title = wrap("", "title-3");
        lyrics_title.set_widget_name("music-lyrics-title");
        lyrics_title.set_lines(2);
        lyrics_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        lyrics_widget.append(&lyrics_title);
        let lyrics_artist = wrap("", "dim-label");
        lyrics_artist.set_widget_name("music-lyrics-artist");
        lyrics_artist.set_lines(1);
        lyrics_artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
        lyrics_widget.append(&lyrics_artist);
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let source = wrap("", "caption");
        source.set_widget_name("music-lyrics-source");
        source.add_css_class("dim-label");
        source.set_hexpand(true);
        source.set_visible(false);
        toolbar.append(&source);
        let lyrics_actions = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .build();
        lyrics_actions.set_widget_name("music-lyrics-actions");
        lyrics_actions.set_tooltip_text(Some("Действия с текстом"));
        lyrics_actions.update_property(&[gtk::accessible::Property::Label("Действия с текстом")]);
        lyrics_actions.add_css_class("flat");
        let actions = gio::SimpleActionGroup::new();
        let copy = gio::SimpleAction::new("copy", None);
        copy.set_enabled(false);
        actions.add_action(&copy);
        let external_action = gio::SimpleAction::new("external", None);
        external_action.set_enabled(false);
        actions.add_action(&external_action);
        lyrics_widget.insert_action_group("lyrics", Some(&actions));
        let menu = gio::Menu::new();
        menu.append(Some("Копировать текст"), Some("lyrics.copy"));
        menu.append(Some("Искать в LRCLIB…"), Some("lyrics.external"));
        lyrics_actions.set_menu_model(Some(&menu));
        toolbar.append(&lyrics_actions);
        lyrics_widget.append(&toolbar);
        let (lyrics_status, lyrics_spinner, lyrics_retry) = feedback(
            &lyrics_widget,
            "Выберите композицию, чтобы загрузить текст.",
            "music-lyrics",
        );
        let factory = gtk::SignalListItemFactory::new();
        let lyrics_model = gtk::StringList::new(&[]);
        let selection = gtk::NoSelection::new(Some(lyrics_model.clone()));
        let lyrics_list = gtk::ListView::new(Some(selection), Some(factory.clone()));
        lyrics_list.set_widget_name("music-lyrics-lines");
        lyrics_list.add_css_class("karin-lyrics-lines");
        lyrics_list.set_single_click_activate(false);
        let lyrics_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .hexpand(true)
            .child(&lyrics_list)
            .build();
        lyrics_scroll.set_widget_name("music-lyrics-scroll");
        lyrics_scroll.set_visible(false);
        lyrics_widget.append(&lyrics_scroll);
        let follow = gtk::Button::with_label("Вернуться к текущей строке");
        follow.set_widget_name("music-lyrics-follow");
        follow.set_halign(gtk::Align::Center);
        follow.set_visible(false);
        lyrics_widget.append(&follow);
        let external = gtk::Button::with_label("Искать в LRCLIB");
        external.set_widget_name("music-lyrics-external");
        external.set_tooltip_text(Some("Найти текст и караоке во внешнем источнике"));
        external.set_halign(gtk::Align::Start);
        external.add_css_class("flat");
        external.set_sensitive(false);
        lyrics_widget.append(&external);

        let inner = Rc::new(Inner {
            widget: widget.clone(),
            lyrics_widget: lyrics_widget.clone(),
            action,
            selected: RefCell::new(None),
            epoch: Cell::new(0),
            info_active: Cell::new(false),
            lyrics_active: Cell::new(false),
            visible_scheduled: Cell::new(false),
            lyrics_serial: Cell::new(0),
            similar_serial: Cell::new(0),
            lyrics_started: Cell::new(false),
            similar_started: Cell::new(false),
            lyrics_loading: Cell::new(false),
            similar_loading: Cell::new(false),
            lyrics_request: Cell::new(LyricsRequest::Yandex),
            lyrics: RefCell::new(None),
            synchronized: Cell::new(false),
            active: Cell::new(None),
            following: Cell::new(true),
            follow_revision: Cell::new(0),
            scroll_animation: RefCell::new(None),
            visible_lines: RefCell::new(Vec::new()),
            lyrics_title,
            lyrics_artist,
            lyrics_status,
            lyrics_spinner,
            source,
            lyrics_retry,
            external,
            external_action,
            external_consent: RefCell::new(None),
            copy,
            #[cfg(test)]
            lyrics_actions,
            follow,
            lyrics_model,
            lyrics_list,
            lyrics_scroll,
            similar_status,
            similar_spinner,
            similar_retry,
            similar_rows,
            similar_more,
            related: RefCell::new(Vec::new()),
            shown: Cell::new(0),
            loader: RefCell::new(None),
        });
        factory.connect_setup(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let label = wrap("", "body");
            label.set_selectable(true);
            label.set_margin_start(2);
            label.set_margin_end(2);
            item.set_child(Some(&label));
        });
        factory.connect_bind({
            let weak = Rc::downgrade(&inner);
            move |_, item| {
                let (Some(this), Some(item)) =
                    (weak.upgrade(), item.downcast_ref::<gtk::ListItem>())
                else {
                    return;
                };
                let (Some(value), Some(label)) = (
                    item.item().and_downcast::<gtk::StringObject>(),
                    item.child().and_downcast::<gtk::Label>(),
                ) else {
                    return;
                };
                label.set_text(&value.string());
                style_line(
                    &label,
                    item.position(),
                    this.active.get(),
                    this.synchronized.get(),
                );
                let mut visible = this.visible_lines.borrow_mut();
                visible.retain(|(_, old)| old.upgrade().is_some_and(|old| old != label));
                visible.push((item.position(), label.downgrade()));
            }
        });
        factory.connect_unbind({
            let weak = Rc::downgrade(&inner);
            move |_, item| {
                let Some(label) = item
                    .downcast_ref::<gtk::ListItem>()
                    .and_then(|item| item.child())
                    .and_downcast::<gtk::Label>()
                else {
                    return;
                };
                if let Some(this) = weak.upgrade() {
                    this.visible_lines
                        .borrow_mut()
                        .retain(|(_, old)| old.upgrade().is_some_and(|old| old != label));
                }
                label.set_text("");
                style_line(&label, 0, None, false);
            }
        });
        connect(&inner);
        Self {
            widget,
            lyrics_widget,
            inner,
        }
    }

    pub(super) fn set_modes(&self, info_visible: bool, lyrics_visible: bool) {
        let info_changed = self.inner.info_active.replace(info_visible) != info_visible;
        let lyrics_changed = self.inner.lyrics_active.replace(lyrics_visible) != lyrics_visible;
        if !lyrics_visible {
            self.inner.stop_scroll_animation();
            self.inner.dismiss_external_consent();
        }
        if info_changed || lyrics_changed {
            schedule_visible(&self.inner);
        }
    }

    pub(super) fn set_track(&self, track: Option<&Track>, loader: Option<&ArtworkLoader>) {
        let this = &self.inner;
        if this.selected.borrow().as_ref() == track {
            return;
        }
        this.stop_scroll_animation();
        this.dismiss_external_consent();
        this.epoch.set(this.epoch.get().wrapping_add(1));
        this.selected.replace(track.cloned());
        this.loader.replace(loader.cloned());
        this.lyrics_started.set(false);
        this.similar_started.set(false);
        this.lyrics_loading.set(false);
        this.similar_loading.set(false);
        this.lyrics.borrow_mut().take();
        this.synchronized.set(false);
        this.active.set(None);
        this.following.set(true);
        this.follow.set_visible(false);
        this.copy.set_enabled(false);
        this.source.set_text("");
        this.source.set_visible(false);
        this.lyrics_model
            .splice(0, this.lyrics_model.n_items(), &[]);
        this.lyrics_scroll.set_visible(false);
        this.lyrics_retry.set_visible(false);
        this.similar_retry.set_visible(false);
        this.lyrics_spinner.stop();
        this.lyrics_spinner.set_visible(false);
        this.similar_spinner.stop();
        this.similar_spinner.set_visible(false);
        this.external.set_sensitive(track.is_some());
        this.external_action.set_enabled(track.is_some());
        this.related.borrow_mut().clear();
        this.shown.set(0);
        super::clear_list(&this.similar_rows);
        this.similar_more.set_visible(false);
        this.lyrics_title
            .set_text(track.map_or("", |track| track.title.as_str()));
        this.lyrics_title
            .set_tooltip_text(track.map(|track| track.title.as_str()));
        let artist = track
            .map(|track| track.artists.join(", "))
            .unwrap_or_default();
        this.lyrics_artist.set_text(&artist);
        this.lyrics_artist
            .set_tooltip_text((!artist.is_empty()).then_some(artist.as_str()));
        this.lyrics_status
            .set_text("Загрузка текста из Яндекс Музыки…");
        this.similar_status.set_text("Рекомендации Яндекс Музыки");
        this.widget.set_visible(track.is_some());
        schedule_visible(this);
    }

    pub(super) fn lyrics_is_current(&self, ticket: Ticket) -> bool {
        self.inner.epoch.get() == ticket.selection
            && self.inner.lyrics_serial.get() == ticket.request
            && self.inner.selected.borrow().is_some()
    }
    pub(super) fn similar_is_current(&self, ticket: Ticket) -> bool {
        self.inner.epoch.get() == ticket.selection
            && self.inner.similar_serial.get() == ticket.request
            && self.inner.selected.borrow().is_some()
    }

    pub(super) fn show_lyrics(&self, ticket: Ticket, result: Result<Option<Lyrics>, MusicError>) {
        if !self.lyrics_is_current(ticket) {
            return;
        }
        let this = &self.inner;
        this.lyrics_loading.set(false);
        this.lyrics_spinner.stop();
        this.lyrics_spinner.set_visible(false);
        this.external.set_sensitive(true);
        this.external_action.set_enabled(true);
        this.lyrics_retry.set_visible(result.is_err());
        match result {
            Ok(Some(lyrics)) => {
                let synchronized = !lyrics.synchronized.is_empty();
                let keep_manual = this.synchronized.get() && !this.following.get();
                this.lyrics_status.set_text(if synchronized {
                    "Караоке"
                } else {
                    "Без синхронизации"
                });
                this.source
                    .set_text(&format!("Источник: {}", lyrics.source.label()));
                this.source.set_visible(true);
                this.stop_scroll_animation();
                this.active.set(None);
                this.synchronized.set(synchronized);
                let lines = if synchronized {
                    lyrics
                        .synchronized
                        .iter()
                        .map(|line| line.text.as_str())
                        .collect::<Vec<_>>()
                } else {
                    lyrics
                        .plain
                        .split("\n\n")
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .collect::<Vec<_>>()
                };
                this.lyrics_model
                    .splice(0, this.lyrics_model.n_items(), &lines);
                this.lyrics_scroll.set_visible(true);
                this.lyrics.replace(Some(lyrics));
                this.copy.set_enabled(true);
                this.following.set(!keep_manual);
                this.follow.set_visible(synchronized && keep_manual);
                if !keep_manual {
                    this.lyrics_scroll.vadjustment().set_value(0.0);
                }
            }
            Ok(None) => {
                if this.lyrics_request.get() == LyricsRequest::Yandex {
                    this.lyrics.borrow_mut().take();
                    this.synchronized.set(false);
                    this.active.set(None);
                    this.lyrics_model
                        .splice(0, this.lyrics_model.n_items(), &[]);
                    this.lyrics_scroll.set_visible(false);
                    this.source.set_visible(false);
                    this.follow.set_visible(false);
                    this.copy.set_enabled(false);
                }
                this.lyrics_status
                    .set_text(if this.lyrics.borrow().is_some() {
                        "Другой текст не найден. Оставлен показанный текст."
                    } else {
                        "Текст для этой композиции не найден."
                    });
            }
            Err(error) => this.lyrics_status.set_text(extras_error(error)),
        }
    }

    pub(super) fn show_similar(&self, ticket: Ticket, result: Result<SimilarTracks, MusicError>) {
        if !self.similar_is_current(ticket) {
            return;
        }
        let this = &self.inner;
        this.similar_loading.set(false);
        this.similar_spinner.stop();
        this.similar_spinner.set_visible(false);
        this.similar_retry.set_visible(result.is_err());
        match result {
            Ok(similar) => {
                this.related.replace(similar.tracks);
                let count = this.related.borrow().len();
                this.similar_status.set_text(if count == 0 {
                    "Для этой композиции нет доступных рекомендаций."
                } else if similar.omitted > 0 {
                    "Рекомендации Яндекс Музыки · часть недоступна."
                } else {
                    "Рекомендации Яндекс Музыки"
                });
                this.shown.set(0);
                super::clear_list(&this.similar_rows);
                this.append_related();
            }
            Err(error) => this.similar_status.set_text(extras_error(error)),
        }
    }

    pub(super) fn update_position(&self, position_ms: Option<u64>) {
        let this = &self.inner;
        let next = position_ms
            .and_then(|position| this.lyrics.borrow().as_ref()?.active_line(position))
            .map(|line| line as u32);
        let old = this.active.replace(next);
        if old == next {
            return;
        }
        this.visible_lines.borrow_mut().retain(|(position, label)| {
            if let Some(label) = label.upgrade() {
                style_line(&label, *position, next, this.synchronized.get());
                true
            } else {
                false
            }
        });
        // A seek updates the line immediately; ordinary adjacent line movement
        // animates only GTK scroll position, never an independent lyric clock.
        let jump = old
            .zip(next)
            .is_none_or(|(old, next)| old.abs_diff(next) > 3);
        this.follow_active(jump);
    }

    fn defer_unmapped_request(&self, action: &ExtrasAction) {
        let this = &self.inner;
        match action {
            ExtrasAction::Lyrics { ticket, .. } if self.lyrics_is_current(*ticket) => {
                this.lyrics_started.set(false);
                this.lyrics_loading.set(false);
                this.lyrics_spinner.stop();
                this.lyrics_spinner.set_visible(false);
                this.external.set_sensitive(true);
                this.external_action.set_enabled(true);
            }
            ExtrasAction::Similar { ticket, .. } if self.similar_is_current(*ticket) => {
                this.similar_started.set(false);
                this.similar_loading.set(false);
                this.similar_spinner.stop();
                this.similar_spinner.set_visible(false);
            }
            _ => {}
        }
    }
}

impl Inner {
    fn request_lyrics(self: &Rc<Self>, request: LyricsRequest, force: bool) {
        if !self.lyrics_active.get()
            || !self.lyrics_widget.is_mapped()
            || self.lyrics_loading.get()
            || (!force && self.lyrics_started.get())
        {
            return;
        }
        let Some(track) = self.selected.borrow().clone() else {
            return;
        };
        self.lyrics_started.set(true);
        self.lyrics_loading.set(true);
        self.lyrics_request.set(request);
        self.lyrics_serial
            .set(self.lyrics_serial.get().wrapping_add(1));
        self.lyrics_status.set_text("Загрузка текста…");
        self.lyrics_retry.set_visible(false);
        self.lyrics_spinner.set_visible(true);
        self.lyrics_spinner.start();
        self.external.set_sensitive(false);
        self.external_action.set_enabled(false);
        (self.action)(ExtrasAction::Lyrics {
            track,
            ticket: Ticket {
                selection: self.epoch.get(),
                request: self.lyrics_serial.get(),
            },
            request,
        });
    }

    fn request_similar(self: &Rc<Self>, force: bool) {
        if !self.info_active.get()
            || !self.widget.is_mapped()
            || self.similar_loading.get()
            || (!force && self.similar_started.get())
        {
            return;
        }
        let Some(track) = self.selected.borrow().clone() else {
            return;
        };
        self.similar_started.set(true);
        self.similar_loading.set(true);
        self.similar_serial
            .set(self.similar_serial.get().wrapping_add(1));
        self.similar_status.set_text("Загрузка рекомендаций…");
        self.similar_retry.set_visible(false);
        self.similar_spinner.set_visible(true);
        self.similar_spinner.start();
        (self.action)(ExtrasAction::Similar {
            track,
            ticket: Ticket {
                selection: self.epoch.get(),
                request: self.similar_serial.get(),
            },
        });
    }

    fn dismiss_external_consent(&self) {
        let dialog = self.external_consent.borrow_mut().take();
        if let Some(dialog) = dialog {
            dialog.close();
        }
    }

    fn request_external_consent(self: &Rc<Self>) {
        if !self.lyrics_active.get()
            || self.lyrics_loading.get()
            || self.external_consent.borrow().is_some()
            || self.selected.borrow().is_none()
        {
            return;
        }
        let Some(root) = self.lyrics_widget.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let epoch = self.epoch.get();
        let dialog = adw::AlertDialog::new(
            Some("Искать текст в LRCLIB?"),
            Some(
                "Во внешний сервис LRCLIB будут переданы название композиции, исполнитель, альбом и длительность. Токены, cookies и данные аккаунта не передаются.",
            ),
        );
        dialog.add_responses(&[("cancel", "Отмена"), ("search", "Искать")]);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("search", adw::ResponseAppearance::Suggested);
        dialog.connect_response(None, {
            let weak = Rc::downgrade(self);
            move |dialog, response| {
                let Some(this) = weak.upgrade() else {
                    return;
                };
                if this.external_consent.borrow().as_ref() != Some(dialog) {
                    return;
                }
                this.external_consent.borrow_mut().take();
                if response == "search" && this.epoch.get() == epoch && this.lyrics_active.get() {
                    this.request_lyrics(LyricsRequest::External, true);
                }
            }
        });
        self.external_consent.replace(Some(dialog.clone()));
        dialog.present(Some(&root));
    }

    fn stop_scroll_animation(&self) {
        self.follow_revision
            .set(self.follow_revision.get().wrapping_add(1));
        let animation = self.scroll_animation.borrow_mut().take();
        if let Some(animation) = animation
            && animation.state() == adw::AnimationState::Playing
        {
            animation.pause();
        }
    }

    fn follow_active(self: &Rc<Self>, immediate: bool) {
        self.stop_scroll_animation();
        if !self.following.get() || !self.lyrics_active.get() || !self.lyrics_list.is_mapped() {
            return;
        }
        let Some(position) = self.active.get() else {
            return;
        };
        let revision = self.follow_revision.get();
        // ListView prebinds rows outside the viewport. A bound Label may still
        // be unmapped with a 0×0 allocation; its CSS bounds are not a usable
        // row position. Materialize it before measuring the scroll target.
        if !self.visible_lines.borrow().iter().any(|(index, label)| {
            *index == position
                && label.upgrade().is_some_and(|label| {
                    label.is_mapped() && label.width() > 0 && label.height() > 0
                })
        }) {
            self.lyrics_list
                .scroll_to(position, gtk::ListScrollFlags::NONE, None);
        }
        let weak = Rc::downgrade(self);
        let frames = Cell::new(0);
        self.lyrics_list.add_tick_callback(move |_, _| {
            let Some(this) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if this.follow_revision.get() != revision
                || !this.following.get()
                || !this.lyrics_active.get()
            {
                return glib::ControlFlow::Break;
            }
            frames.set(frames.get() + 1);
            let label = this
                .visible_lines
                .borrow()
                .iter()
                .find(|(index, _)| *index == position)
                .and_then(|(_, label)| label.upgrade())
                .filter(|label| label.is_mapped() && label.width() > 0 && label.height() > 0);
            let bounds = label
                .and_then(|label| label.compute_bounds(&this.lyrics_scroll))
                .filter(|bounds| bounds.height() > 0.0);
            if let Some(bounds) = bounds.filter(|_| this.lyrics_scroll.height() > 0) {
                let adjustment = this.lyrics_scroll.vadjustment();
                let center = f64::from(bounds.y() + bounds.height() / 2.0);
                let target = (adjustment.value() + center - adjustment.page_size() / 2.0).clamp(
                    adjustment.lower(),
                    (adjustment.upper() - adjustment.page_size()).max(adjustment.lower()),
                );
                if (target - adjustment.value()).abs() > 1.0 {
                    if immediate {
                        adjustment.set_value(target);
                    } else {
                        let weak = Rc::downgrade(&this);
                        let target_callback = adw::CallbackAnimationTarget::new(move |value| {
                            if let Some(this) = weak.upgrade() {
                                this.lyrics_scroll.vadjustment().set_value(value);
                            }
                        });
                        let animation = adw::TimedAnimation::new(
                            &this.lyrics_scroll,
                            adjustment.value(),
                            target,
                            180,
                            target_callback,
                        );
                        animation.set_easing(adw::Easing::EaseOutCubic);
                        animation.play();
                        this.scroll_animation.replace(Some(animation));
                    }
                }
                return glib::ControlFlow::Break;
            }
            if frames.get() >= 4 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    fn suspend_following(&self) {
        if self.synchronized.get() {
            self.stop_scroll_animation();
            self.following.set(false);
            self.follow.set_visible(true);
        }
    }
    fn append_related(self: &Rc<Self>) {
        let start = self.shown.get();
        let selected = self.related.borrow();
        let end = selected.len().min(start + RELATED_PAGE);
        for track in &selected[start..end] {
            let (row, body) =
                super::catalog_layout::catalog_item(&track.title, &track.artists.join(", "));
            row.set_widget_name("music-similar-track");
            row.set_activatable(track.available);
            let art = super::cover_widget(self.loader.borrow().as_ref(), track.cover.as_ref(), 40);
            body.prepend(&art);
            let ticket = Ticket {
                selection: self.epoch.get(),
                request: self.similar_serial.get(),
            };
            let group = gio::SimpleActionGroup::new();
            let activate = gio::SimpleAction::new("activate", None);
            activate.connect_activate({
                let weak = Rc::downgrade(self);
                let track = track.clone();
                move |_, _| {
                    if let Some(this) = weak.upgrade() {
                        (this.action)(ExtrasAction::Related {
                            track: track.clone(),
                            ticket,
                            action: RelatedAction::Play,
                        });
                    }
                }
            });
            group.add_action(&activate);
            row.insert_action_group("catalog-row", Some(&group));
            let menu = gtk::MenuButton::builder()
                .icon_name("view-more-symbolic")
                .build();
            menu.set_widget_name("music-similar-actions");
            menu.set_tooltip_text(Some("Действия с похожим треком"));
            menu.update_property(&[gtk::accessible::Property::Label(
                "Действия с похожим треком",
            )]);
            menu.add_css_class("flat");
            let popover = gtk::Popover::new();
            let actions = gtk::Box::new(gtk::Orientation::Vertical, 4);
            super::super::margins(&actions, 6);
            for (label, action) in [
                ("Воспроизвести", RelatedAction::Play),
                ("Воспроизвести следующим", RelatedAction::Next),
                ("Добавить в очередь", RelatedAction::Enqueue),
                ("Отметить: нравится", RelatedAction::Like),
                ("Отметить: не нравится", RelatedAction::Dislike),
                ("Сведения о треке", RelatedAction::Details),
            ] {
                let button = gtk::Button::with_label(label);
                button.add_css_class("flat");
                if matches!(action, RelatedAction::Play | RelatedAction::Next) {
                    button.set_sensitive(track.available);
                }
                let weak = Rc::downgrade(self);
                let track = track.clone();
                let close = popover.downgrade();
                button.connect_clicked(move |_| {
                    if let Some(popover) = close.upgrade() {
                        popover.popdown();
                    }
                    if let Some(this) = weak.upgrade() {
                        (this.action)(ExtrasAction::Related {
                            track: track.clone(),
                            ticket,
                            action,
                        });
                    }
                });
                actions.append(&button);
            }
            popover.set_child(Some(&actions));
            menu.set_popover(Some(&popover));
            body.append(&menu);
            self.similar_rows.append(&row);
        }
        self.shown.set(end);
        self.similar_more.set_visible(end < selected.len());
    }
}

fn connect(this: &Rc<Inner>) {
    for widget in [&this.widget, &this.lyrics_widget] {
        let weak = Rc::downgrade(this);
        widget.connect_map(move |_| {
            if let Some(this) = weak.upgrade() {
                schedule_visible(&this);
            }
        });
    }
    this.lyrics_retry.connect_clicked({
        let weak = Rc::downgrade(this);
        move |_| {
            if let Some(this) = weak.upgrade() {
                this.request_lyrics(this.lyrics_request.get(), true);
            }
        }
    });
    this.external.connect_clicked({
        let weak = Rc::downgrade(this);
        move |_| {
            if let Some(this) = weak.upgrade() {
                this.request_external_consent();
            }
        }
    });
    this.external_action.connect_activate({
        let weak = Rc::downgrade(this);
        move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.request_external_consent();
            }
        }
    });
    this.copy.connect_activate({
        let weak = Rc::downgrade(this);
        move |_, _| {
            if let Some(this) = weak.upgrade()
                && let Some(lyrics) = this.lyrics.borrow().as_ref()
            {
                this.lyrics_widget.clipboard().set_text(&lyrics.plain);
            }
        }
    });
    this.similar_retry.connect_clicked({
        let weak = Rc::downgrade(this);
        move |_| {
            if let Some(this) = weak.upgrade() {
                this.request_similar(true);
            }
        }
    });
    this.similar_more.connect_clicked({
        let weak = Rc::downgrade(this);
        move |_| {
            if let Some(this) = weak.upgrade() {
                this.append_related();
            }
        }
    });
    this.follow.connect_clicked({
        let weak = Rc::downgrade(this);
        move |_| {
            if let Some(this) = weak.upgrade() {
                this.following.set(true);
                this.follow.set_visible(false);
                this.follow_active(false);
            }
        }
    });
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    scroll.connect_scroll({
        let weak = Rc::downgrade(this);
        move |_, x, y| {
            if (x != 0.0 || y != 0.0)
                && let Some(this) = weak.upgrade()
            {
                this.suspend_following();
            }
            glib::Propagation::Proceed
        }
    });
    this.lyrics_scroll.add_controller(scroll);
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed({
        let weak = Rc::downgrade(this);
        move |_, key, _, modifiers| {
            if matches!(
                key,
                gtk::gdk::Key::Up
                    | gtk::gdk::Key::Down
                    | gtk::gdk::Key::Page_Up
                    | gtk::gdk::Key::Page_Down
                    | gtk::gdk::Key::Home
                    | gtk::gdk::Key::End
                    | gtk::gdk::Key::space
            ) && !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
                && let Some(this) = weak.upgrade()
            {
                this.suspend_following();
            }
            glib::Propagation::Proceed
        }
    });
    this.lyrics_scroll.add_controller(keys);
    let drag = gtk::GestureDrag::new();
    drag.set_propagation_phase(gtk::PropagationPhase::Capture);
    drag.connect_drag_update({
        let weak = Rc::downgrade(this);
        move |_, _, _| {
            if let Some(this) = weak.upgrade() {
                this.suspend_following();
            }
        }
    });
    this.lyrics_scroll.add_controller(drag);
    let scrollbar_click = gtk::GestureClick::new();
    scrollbar_click.set_button(1);
    scrollbar_click.set_propagation_phase(gtk::PropagationPhase::Capture);
    scrollbar_click.connect_pressed({
        let weak = Rc::downgrade(this);
        move |_, _, x, y| {
            let Some(this) = weak.upgrade() else { return };
            let scrollbar = this.lyrics_scroll.vscrollbar();
            let target = this.lyrics_scroll.pick(x, y, gtk::PickFlags::DEFAULT);
            if target.is_some_and(|target| target == scrollbar || target.is_ancestor(&scrollbar)) {
                this.suspend_following();
            }
        }
    });
    this.lyrics_scroll.add_controller(scrollbar_click);
    this.lyrics_scroll.vadjustment().connect_page_size_notify({
        let weak = Rc::downgrade(this);
        move |_| {
            if let Some(this) = weak.upgrade() {
                this.follow_active(true);
            }
        }
    });
}

fn schedule_visible(this: &Rc<Inner>) {
    if this.visible_scheduled.replace(true) {
        return;
    }
    let weak = Rc::downgrade(this);
    glib::idle_add_local_once(move || {
        let Some(this) = weak.upgrade() else { return };
        this.visible_scheduled.set(false);
        if this.lyrics_active.get() && this.lyrics_widget.is_mapped() {
            this.request_lyrics(LyricsRequest::Yandex, false);
            this.follow_active(true);
        }
        if this.info_active.get() && this.widget.is_mapped() {
            this.request_similar(false);
        }
    });
}

fn style_line(label: &gtk::Label, position: u32, active: Option<u32>, synchronized: bool) {
    for class in [
        "karin-lyrics-line",
        "karin-lyrics-stanza",
        "lyrics-active",
        "lyrics-past",
        "lyrics-future",
    ] {
        label.remove_css_class(class);
    }
    if synchronized {
        label.add_css_class("karin-lyrics-line");
        let (class, description) = if active == Some(position) {
            ("lyrics-active", "Текущая строка")
        } else if active.is_some_and(|active| position < active) {
            ("lyrics-past", "Прошедшая строка")
        } else {
            ("lyrics-future", "Следующая строка")
        };
        label.add_css_class(class);
        label.update_property(&[gtk::accessible::Property::Description(description)]);
    } else {
        label.add_css_class("karin-lyrics-stanza");
        label.update_property(&[gtk::accessible::Property::Description("Без синхронизации")]);
    }
}

fn feedback(
    parent: &gtk::Box,
    initial: &str,
    name: &str,
) -> (gtk::Label, gtk::Spinner, gtk::Button) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let spinner = gtk::Spinner::new();
    spinner.set_visible(false);
    row.append(&spinner);
    let label = wrap(initial, "caption");
    label.set_widget_name(&format!("{name}-status"));
    label.set_hexpand(true);
    row.append(&label);
    parent.append(&row);
    let retry = icon_button("view-refresh-symbolic", "Повторить загрузку");
    retry.set_widget_name(&format!("{name}-retry"));
    retry.set_halign(gtk::Align::Start);
    retry.set_visible(false);
    parent.append(&retry);
    (label, spinner, retry)
}

fn wrap(value: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(value));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_width_chars(1);
    label.add_css_class(class);
    label
}

fn extras_error(error: MusicError) -> &'static str {
    match error {
        MusicError::AccessDenied | MusicError::Unauthenticated => {
            "Войдите снова, чтобы получить данные композиции."
        }
        MusicError::Forbidden | MusicError::Restricted | MusicError::MissingScopes => {
            "Сервис не предоставил доступ к этим данным."
        }
        MusicError::Network | MusicError::Timeout => {
            "Не удалось загрузить данные. Проверьте соединение и повторите."
        }
        MusicError::RateLimited => "Сервис просит подождать. Повторите загрузку позже.",
        _ => "Не удалось обработать данные композиции. Можно повторить загрузку.",
    }
}

impl Controller {
    pub(super) fn handle_track_extras_action(self: &Rc<Self>, action: ExtrasAction) {
        let ready = {
            let model = self.model.borrow();
            model.account_valid && !model.closed
        };
        let mode = self.panel_mode.get();
        let appropriate = match &action {
            ExtrasAction::Lyrics { .. } => mode == super::workspace::PanelMode::Lyrics,
            ExtrasAction::Similar { .. } | ExtrasAction::Related { .. } => {
                mode == super::workspace::PanelMode::TrackInfo
            }
        };
        if !ready || !appropriate {
            self.track_pane.extras.defer_unmapped_request(&action);
            return;
        }
        match action {
            ExtrasAction::Lyrics {
                track,
                ticket,
                request,
            } => {
                if !self.track_pane.extras.lyrics_is_current(ticket) {
                    return;
                }
                self.run(
                    Operation::Lyrics,
                    move |service, token| service.lyrics(&token, &track, request),
                    move |this, result| {
                        if !this.track_pane.extras.lyrics_is_current(ticket) {
                            return;
                        }
                        if let Err(
                            error @ (MusicError::AccessDenied | MusicError::Unauthenticated),
                        ) = result
                        {
                            this.show_error(error);
                        } else {
                            this.track_pane.extras.show_lyrics(ticket, result);
                            this.sync_track_extras_position();
                        }
                    },
                );
            }
            ExtrasAction::Similar { track, ticket } => {
                if !self.track_pane.extras.similar_is_current(ticket) {
                    return;
                }
                self.run(
                    Operation::Similar,
                    move |service, token| service.similar_tracks(&token, &track.id),
                    move |this, result| {
                        if !this.track_pane.extras.similar_is_current(ticket) {
                            return;
                        }
                        if let Err(
                            error @ (MusicError::AccessDenied | MusicError::Unauthenticated),
                        ) = result
                        {
                            this.show_error(error);
                        } else {
                            this.track_pane.extras.show_similar(ticket, result);
                        }
                    },
                );
            }
            ExtrasAction::Related {
                track,
                ticket,
                action,
            } => {
                if !self.track_pane.extras.similar_is_current(ticket) {
                    return;
                }
                match action {
                    RelatedAction::Play if track.available => self.play_music_track(track),
                    RelatedAction::Play => {}
                    RelatedAction::Next if track.available => self.enqueue_next(track),
                    RelatedAction::Next => {}
                    RelatedAction::Enqueue => self.enqueue(track),
                    RelatedAction::Details => self.open_loaded_track_info(track, None),
                    RelatedAction::Like => self.mark_related(track, ticket, TrackReaction::Like),
                    RelatedAction::Dislike => {
                        self.mark_related(track, ticket, TrackReaction::Dislike)
                    }
                }
            }
        }
    }

    fn mark_related(self: &Rc<Self>, track: Track, ticket: Ticket, kind: TrackReaction) {
        if self.mutation_busy.replace(true) {
            self.track_pane
                .extras
                .inner
                .similar_status
                .set_text("Другая правка библиотеки ещё выполняется.");
            return;
        }
        let id = track.id;
        let playback_target = self
            .model
            .borrow()
            .queue
            .current()
            .is_some_and(|track| track.id == id)
            .then(|| self.reaction_playback_target())
            .flatten();
        let views = self.retire_detail_likes(&id);
        self.track_pane
            .extras
            .inner
            .similar_status
            .set_text("Сохраняем и проверяем отметку…");
        for operation in [Operation::Library, Operation::InitialLibrary] {
            self.requests.borrow_mut().invalidate(operation);
        }
        self.invalidate_liked_feeds();
        let cache = self.persistent_cache();
        self.run(Operation::Mutation, move |service, token| {
            let result = service.set_track_reaction(&token, &id, kind, true);
            if !token.is_expired() && service.cache_scope(&token).is_ok()
                && let Some(cache) = cache { let _ = cache.invalidate_metadata(); }
            Ok(result)
        }, move |this, result| {
            this.mutation_busy.set(false);
            let result = result.unwrap_or_else(|error| Err(InteractionError::Music(error)));
            views.finish(result.map(|state| state.liked), true);
            this.current_like_key.borrow_mut().take();
            this.sync_current_like();
            this.model.borrow_mut().liked = None;
            this.render_home_preview();
            if let Err(InteractionError::Music(error @ (MusicError::AccessDenied | MusicError::Unauthenticated))) = result {
                this.show_error(error);
                return;
            }
            if this.track_pane.extras.similar_is_current(ticket) {
                this.track_pane.extras.inner.similar_status.set_text(if result.is_ok() {
                    "Отметка подтверждена Яндекс Музыкой."
                } else { "Изменение не подтверждено. Обновите библиотеку перед новым действием; запись не повторяется." });
            }
            if super::reactions::confirmed_dislike_advances(
                kind,
                true,
                result.map(super::reactions::ReactionRead::Both),
                playback_target.as_ref(),
                this.reaction_playback_target().as_ref(),
            ) {
                this.skip_music(true, false);
            }
        });
    }

    /// Existing render/tick caller passes the sole native engine's position.
    /// Inspecting another track, Stop/EOS/logout never highlights a false line.
    pub(super) fn sync_track_extras_position(&self) {
        let selected = self.track_pane.selected();
        let current = self.model.borrow().queue.current().cloned();
        let same = self.information_follows_current_playback()
            && selected
                .as_ref()
                .zip(current.as_ref())
                .is_some_and(|(selected, current)| selected.id == current.id);
        let position = if same {
            self.music_snapshot()
                .filter(|snapshot| {
                    matches!(
                        snapshot.state,
                        PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ready
                    )
                })
                .and_then(|snapshot| snapshot.position)
                .map(|position| position.as_millis().min(u128::from(u64::MAX)) as u64)
        } else {
            None
        };
        self.track_pane.extras.update_position(position);
    }
}

#[cfg(test)]
pub(in crate::ui::music) mod tests;
