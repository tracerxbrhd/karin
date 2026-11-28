// SPDX-License-Identifier: GPL-3.0-or-later
//! Window lifetime and user intent for the desktop tray.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use adw::prelude::*;
use gtk::{gdk, gio, glib};

#[path = "tray_window/position.rs"]
mod position;

pub(super) use position::MINI_WIDTH;
use position::Positioning;

use super::{
    mini_player::{Action, MiniPlayer},
    music::{MiniCommand, MiniPlayback},
    tray::{Interaction, Tray},
};

const SCROLL_UNITS_PER_STEP: f64 = 120.0;

pub(super) struct WindowTray {
    window: glib::WeakRef<adw::ApplicationWindow>,
    tray: RefCell<Option<Tray>>,
    playback: RefCell<Option<MiniPlayback>>,
    mini: RefCell<Option<MiniPlayer>>,
    positioning: Positioning,
    mini_refresh: RefCell<Option<glib::SourceId>>,
    hidden_hold: RefCell<Option<gio::ApplicationHoldGuard>>,
    quitting: Cell<bool>,
}

impl WindowTray {
    pub(super) fn attach(window: &adw::ApplicationWindow) -> Rc<Self> {
        let owner = Rc::new(Self {
            window: window.downgrade(),
            tray: RefCell::new(None),
            playback: RefCell::new(None),
            mini: RefCell::new(None),
            positioning: Positioning::new(),
            mini_refresh: RefCell::new(None),
            hidden_hold: RefCell::new(None),
            quitting: Cell::new(false),
        });
        // Install before the playback and persistence shutdown handlers.
        let weak = Rc::downgrade(&owner);
        window.connect_close_request(move |window| {
            if let Some(owner) = weak.upgrade() {
                if owner.hide(window) {
                    return glib::Propagation::Stop;
                }
                if window.dialogs().n_items() == 0 {
                    owner.discard_mini();
                    owner.hidden_hold.borrow_mut().take();
                }
            }
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(&owner);
        window.connect_visible_notify(move |window| {
            if window.is_visible()
                && let Some(owner) = weak.upgrade()
            {
                owner.hidden_hold.borrow_mut().take();
            }
        });
        let show = gio::SimpleAction::new("show-main", None);
        let weak = Rc::downgrade(&owner);
        show.connect_activate(move |_, _| {
            if let Some(owner) = weak.upgrade() {
                owner.present_main(None);
            }
        });
        window.add_action(&show);
        let quit = gio::SimpleAction::new("quit", None);
        let weak = Rc::downgrade(&owner);
        quit.connect_activate(move |_, _| {
            if let Some(owner) = weak.upgrade() {
                owner.quit();
            }
        });
        window.add_action(&quit);
        owner
    }

    pub(super) fn attach_playback(&self, playback: MiniPlayback) {
        self.playback.replace(Some(playback));
    }

    pub(super) fn set_enabled(self: &Rc<Self>, enabled: bool) {
        if enabled == self.tray.borrow().is_some() {
            return;
        }
        if !enabled {
            self.discard_mini();
            self.restore_if_hidden();
            let tray = self.tray.borrow_mut().take();
            drop(tray);
            return;
        }

        let activate = Rc::downgrade(self);
        let show = activate.clone();
        let quit = activate.clone();
        let availability = activate.clone();
        self.tray.replace(Some(Tray::new(
            move |interaction| {
                if let Some(owner) = activate.upgrade() {
                    owner.handle_tray_interaction(interaction);
                }
            },
            move |token| {
                if let Some(owner) = show.upgrade() {
                    owner.present_main(token);
                }
            },
            move || {
                if let Some(owner) = quit.upgrade() {
                    owner.quit();
                }
            },
            move |available| {
                if !available && let Some(owner) = availability.upgrade() {
                    owner.discard_mini();
                    owner.restore_if_hidden();
                }
            },
        )));
    }

    fn handle_tray_interaction(self: &Rc<Self>, interaction: Interaction<'_>) {
        let command = match interaction {
            Interaction::Activate(point, token) => return self.toggle_mini(point, token),
            Interaction::PlayPause => MiniCommand::PlayPause,
            Interaction::Scroll(delta) => {
                MiniCommand::VolumeSteps(f64::from(delta) / SCROLL_UNITS_PER_STEP)
            }
        };
        let playback = self.playback.borrow().clone();
        if let Some(playback) = playback {
            playback.command(command);
            self.render_mini();
        }
    }

    fn hide(&self, window: &adw::ApplicationWindow) -> bool {
        if self.quitting.get()
            || !window.is_visible()
            || window.dialogs().n_items() != 0
            || !self.tray.borrow().as_ref().is_some_and(Tray::is_available)
        {
            return false;
        }
        let Some(app) = window.application() else {
            return false;
        };
        self.hidden_hold.replace(Some(app.hold()));
        window.set_visible(false);
        true
    }

    fn present_main(&self, token: Option<&str>) {
        if !self.quitting.get()
            && let Some(window) = self.window.upgrade()
        {
            present(&window, token);
            self.hide_mini();
        }
    }

    fn restore_if_hidden(&self) {
        let hidden = self.hidden_hold.borrow().is_some();
        if hidden {
            self.present_main(None);
        }
    }

    fn toggle_mini(self: &Rc<Self>, point: (i32, i32), token: Option<&str>) {
        if self.quitting.get() {
            return;
        }
        let Some(main) = self.window.upgrade() else {
            return;
        };
        let display = gtk::prelude::WidgetExt::display(&main);
        let placement = self.positioning.placement(&display, point);
        let target_display = placement.display().unwrap_or(display);
        if let Some(window) = self.mini_window() {
            let minimized = window
                .surface()
                .and_downcast::<gdk::Toplevel>()
                .is_some_and(|surface| surface.state().contains(gdk::ToplevelState::MINIMIZED));
            if window.is_visible() && !minimized {
                window.set_visible(false);
                return;
            }
            let same_role = self.mini.borrow().as_ref().is_some_and(|mini| {
                mini.is_anchored == placement.is_anchored()
                    && gtk::prelude::WidgetExt::display(&window) == target_display
            });
            if same_role {
                placement.present(&window, token);
                return;
            }
            self.discard_mini();
        }
        let Some(app) = main.application().and_downcast::<adw::Application>() else {
            return;
        };
        let Some(playback) = self.playback.borrow().clone() else {
            return;
        };
        let weak = Rc::downgrade(self);
        let mini = MiniPlayer::new(
            &app,
            &target_display,
            placement.is_anchored(),
            move |action| match action {
                Action::Playback(command) => playback.command(command),
                Action::OpenMain => {
                    if let Some(owner) = weak.upgrade() {
                        owner.present_main(None);
                    }
                }
            },
        );
        let window = mini.window.clone();
        placement.initialize(&window);
        window.set_hide_on_close(true);
        let weak = Rc::downgrade(self);
        window.connect_map(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.start_mini_refresh();
            }
        });
        let weak = Rc::downgrade(self);
        window.connect_unmap(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.stop_mini_refresh();
            }
        });
        self.mini.replace(Some(mini));
        placement.present(&window, token);
    }

    fn mini_window(&self) -> Option<adw::ApplicationWindow> {
        self.mini.borrow().as_ref().map(|mini| mini.window.clone())
    }

    fn hide_mini(&self) {
        if let Some(window) = self.mini_window() {
            window.set_visible(false);
        }
    }

    fn discard_mini(&self) {
        self.stop_mini_refresh();
        if let Some(mini) = self.mini.take() {
            mini.window.destroy();
        }
    }

    fn start_mini_refresh(self: &Rc<Self>) {
        self.stop_mini_refresh();
        if let Some(playback) = self.playback.borrow().as_ref()
            && let Some(mini) = self.mini.borrow().as_ref()
        {
            playback.set_reactions_host(Some(&mini.reactions_slot));
        }
        self.render_mini();
        let weak = Rc::downgrade(self);
        self.mini_refresh.replace(Some(glib::timeout_add_local(
            Duration::from_millis(150),
            move || {
                let Some(owner) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                owner.render_mini();
                glib::ControlFlow::Continue
            },
        )));
    }

    fn stop_mini_refresh(&self) {
        if let Some(playback) = self.playback.borrow().as_ref() {
            playback.set_reactions_host(None);
        }
        if let Some(source) = self.mini_refresh.take() {
            source.remove();
        }
    }

    fn render_mini(&self) {
        if let Some(snapshot) = self
            .playback
            .borrow()
            .as_ref()
            .and_then(MiniPlayback::snapshot)
            && let Some(mini) = self.mini.borrow().as_ref()
        {
            mini.render(&snapshot);
        }
    }

    fn quit(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        if self.quitting.replace(true) {
            return;
        }
        self.hide_mini();
        if window.dialogs().n_items() == 0 {
            window.close();
            self.quitting.set(false);
            return;
        }

        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            // Respect a modal that declines to close, such as update handoff.
            // A dialog can remain attached until its closing animation ends.
            let mut requested = None;
            for _ in 0..120 {
                if window.dialogs().n_items() == 0 {
                    window.close();
                    break;
                }
                if let Some(dialog) = window.visible_dialog()
                    && requested.as_ref() != Some(&dialog)
                {
                    dialog.close();
                    requested = Some(dialog);
                }
                glib::timeout_future(Duration::from_millis(25)).await;
            }
            if let Some(owner) = weak.upgrade() {
                owner.quitting.set(false);
            }
        });
    }
}

impl Drop for WindowTray {
    fn drop(&mut self) {
        self.discard_mini();
    }
}

fn present(window: &adw::ApplicationWindow, token: Option<&str>) {
    if let Some(token) = token {
        window.set_startup_id(token);
    }
    window.present();
}
