// SPDX-License-Identifier: GPL-3.0-or-later
//! Moves the existing information surface between two presentations.

use super::{
    Controller, MusicPane,
    mini::MiniPlayback,
    now_playing::{Action, NowPlaying, Section},
    workspace::{InfoHost, PanelMode},
};
use adw::prelude::*;
use gtk::{gdk, glib};
use std::cell::Cell;
use std::rc::Rc;

pub(super) struct Host {
    view: NowPlaying,
    shell: glib::WeakRef<gtk::Stack>,
    section: Cell<Option<Section>>,
    inspected_current: Cell<bool>,
}

impl Host {
    pub(super) fn new(action: impl Fn(Action) + 'static) -> Self {
        Self {
            view: NowPlaying::new(action),
            shell: glib::WeakRef::new(),
            section: Cell::new(None),
            inspected_current: Cell::new(false),
        }
    }

    pub(super) fn is_active(&self) -> bool {
        self.section.get().is_some()
    }

    pub(super) fn reactions_host(&self) -> &gtk::Box {
        &self.view.reactions_slot
    }

    pub(super) fn inspects_current(&self) -> bool {
        self.is_active() && self.inspected_current.get()
    }

    pub(super) fn panel_mode(&self) -> Option<PanelMode> {
        self.section.get().map(|section| match section {
            Section::Player => PanelMode::Closed,
            Section::Info => PanelMode::TrackInfo,
            Section::Lyrics => PanelMode::Lyrics,
            Section::Queue => PanelMode::Queue,
        })
    }
}

impl MusicPane {
    pub(in crate::ui) fn attach_now_playing(&self, shell: &gtk::Stack) {
        let host = &self.controller.now_playing;
        host.shell.set(Some(shell));
        shell.add_named(&host.view.widget, Some("now-playing"));
        let weak = Rc::downgrade(&self.controller);
        let extent = Cell::new(None);
        host.view.panel_host.add_tick_callback(move |widget, _| {
            let Some(controller) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let measured = (widget.width(), widget.height());
            if extent.replace(Some(measured)) != Some(measured) {
                controller.sync_now_playing_panel();
            }
            glib::ControlFlow::Continue
        });
        if let Some(window) = self.controller.window.upgrade() {
            let keys = gtk::EventControllerKey::new();
            let weak = Rc::downgrade(&self.controller);
            keys.connect_key_pressed(move |_, key, _, modifiers| {
                if key == gdk::Key::Escape
                    && (modifiers & gtk::accelerator_get_default_mod_mask()).is_empty()
                    && let Some(controller) = weak.upgrade()
                    && controller
                        .window
                        .upgrade()
                        .is_some_and(|window| window.visible_dialog().is_none())
                    && controller.close_now_playing(true)
                {
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            window.add_controller(keys);
        }
    }
}

impl Controller {
    pub(super) fn effective_panel_mode(&self) -> PanelMode {
        self.now_playing
            .panel_mode()
            .unwrap_or_else(|| self.panel_mode.get())
    }

    pub(super) fn open_now_playing(&self) {
        let host = &self.now_playing;
        if host.is_active() || !self.music_surface_visible.get() {
            return;
        }
        let Some(snapshot) = self.mini_snapshot().filter(|state| state.title.is_some()) else {
            return;
        };
        let Some(shell) = host.shell.upgrade() else {
            return;
        };
        let Some(side) = self
            .info_host
            .borrow()
            .as_ref()
            .and_then(InfoHost::revealer)
        else {
            return;
        };
        if let Some(window) = self.window.upgrade() {
            gtk::prelude::GtkWindowExt::set_focus(&window, gtk::Widget::NONE);
        }
        host.inspected_current.set(false);
        host.section.set(Some(Section::Player));
        self.track_pane.menu.popdown();
        side.set_surface(None);
        self.track_pane.set_embedded(true);
        host.view.panel_host.append(&self.track_pane.widget);
        host.view.set_section(Section::Player);
        host.view
            .render(&snapshot, self.now_playing_status().as_deref());
        self.sync_now_playing_panel();
        shell.set_visible_child_name("now-playing");
        self.sync_music_back();
        host.view.focus_back();
    }

    pub(super) fn close_now_playing(&self, return_focus: bool) -> bool {
        let host = &self.now_playing;
        if host.section.take().is_none() {
            return false;
        }
        if let Some(window) = self.window.upgrade() {
            gtk::prelude::GtkWindowExt::set_focus(&window, gtk::Widget::NONE);
        }
        self.track_pane.menu.popdown();
        host.view.panel_host.remove(&self.track_pane.widget);
        self.track_pane.set_embedded(false);
        if let Some(side) = self
            .info_host
            .borrow()
            .as_ref()
            .and_then(InfoHost::revealer)
        {
            side.set_surface(Some(self.track_pane.widget.upcast_ref()));
        }
        self.sync_track_pane();
        if let Some(shell) = host.shell.upgrade() {
            shell.set_visible_child_name("workspace");
        }
        host.view.render(&Default::default(), None);
        self.sync_music_back();
        if return_focus && self.widgets.current_art_button.is_mapped() {
            self.widgets.current_art_button.grab_focus();
        }
        true
    }

    pub(super) fn render_now_playing(&self) {
        if self.now_playing.is_active() {
            if let Some(snapshot) = self.mini_snapshot().filter(|state| state.title.is_some()) {
                self.now_playing
                    .view
                    .render(&snapshot, self.now_playing_status().as_deref());
            } else {
                self.close_now_playing(false);
            }
        }
    }

    fn now_playing_status(&self) -> Option<glib::GString> {
        let status = self.widgets.player_status.text();
        (self.widgets.player_status.is_visible() && !status.is_empty()).then_some(status)
    }

    pub(super) fn show_now_playing_section(&self, section: Section) {
        if !self.now_playing.is_active() || self.now_playing.section.get() == Some(section) {
            return;
        }
        self.track_pane.menu.popdown();
        if matches!(section, Section::Info | Section::Lyrics) {
            // Keep this target while its details are hidden so switching
            // tabs doesn't discard already-loaded lyrics and similar tracks.
            self.now_playing.inspected_current.set(true);
        }
        self.now_playing.section.set(Some(section));
        self.now_playing.view.set_section(section);
        self.sync_track_pane();
        self.sync_track_extras_position();
    }

    pub(super) fn sync_now_playing_panel(&self) {
        let Some(mode) = self.now_playing.panel_mode() else {
            return;
        };
        self.track_pane
            .widget
            .set_visible(mode != PanelMode::Closed);
        if mode != PanelMode::Closed {
            self.track_pane.set_mode(mode);
            let panel = &self.now_playing.view.panel_host;
            if panel.width() > 0 && panel.height() > 0 {
                self.track_pane
                    .set_available_size(panel.width(), panel.height());
            }
        }
        self.track_pane
            .extras
            .set_modes(mode == PanelMode::TrackInfo, mode == PanelMode::Lyrics);
        self.track_pane
            .extras
            .set_lyrics_metadata_visible(self.now_playing.view.is_compact());
    }

    pub(super) fn handle_now_playing_action(self: &Rc<Self>, action: Action) {
        if self.now_playing.is_active() {
            match action {
                Action::Back => {
                    self.close_now_playing(true);
                }
                Action::Section(section) => self.show_now_playing_section(section),
                Action::Playback(command) => MiniPlayback::new(self).command(command),
            }
        }
    }
}
