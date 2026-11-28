// SPDX-License-Identifier: GPL-3.0-or-later
//! Window lifetime and user intent for the desktop tray.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use adw::prelude::*;
use gtk::{gio, glib};

use super::tray::Tray;

pub(super) struct WindowTray {
    window: glib::WeakRef<adw::ApplicationWindow>,
    tray: RefCell<Option<Tray>>,
    hidden_hold: RefCell<Option<gio::ApplicationHoldGuard>>,
    quitting: Cell<bool>,
}

impl WindowTray {
    pub(super) fn attach(window: &adw::ApplicationWindow) -> Rc<Self> {
        let owner = Rc::new(Self {
            window: window.downgrade(),
            tray: RefCell::new(None),
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

    pub(super) fn set_enabled(self: &Rc<Self>, enabled: bool) {
        if enabled == self.tray.borrow().is_some() {
            return;
        }
        if !enabled {
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
            move |token| {
                if let Some(owner) = activate.upgrade()
                    && let Some(window) = owner.window.upgrade()
                    && (!window.is_active() || !owner.hide(&window))
                {
                    owner.present(token);
                }
            },
            move |token| {
                if let Some(owner) = show.upgrade() {
                    owner.present(token);
                }
            },
            move || {
                if let Some(owner) = quit.upgrade() {
                    owner.quit();
                }
            },
            move |available| {
                if !available && let Some(owner) = availability.upgrade() {
                    owner.restore_if_hidden();
                }
            },
        )));
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

    fn present(&self, token: Option<&str>) {
        if !self.quitting.get()
            && let Some(window) = self.window.upgrade()
        {
            if let Some(token) = token {
                window.set_startup_id(token);
            }
            window.present();
        }
    }

    fn restore_if_hidden(&self) {
        let hidden = self.hidden_hold.borrow().is_some();
        if hidden {
            self.present(None);
        }
    }

    fn quit(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        if self.quitting.replace(true) {
            return;
        }
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
