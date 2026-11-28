// SPDX-License-Identifier: GPL-3.0-or-later
//! The mini player reads and controls the existing Music controller.

use std::{
    rc::{Rc, Weak},
    time::Duration,
};

use gtk::gdk;

use super::Controller;
use crate::queue::RepeatMode;

#[derive(Clone, Copy)]
pub(in crate::ui) enum MiniCommand {
    Previous,
    PlayPause,
    Next,
    Seek(Duration),
    Volume(f64),
    VolumeSteps(f64),
    Shuffle(bool),
    Repeat,
}

#[derive(Clone, Default)]
pub(in crate::ui) struct MiniSnapshot {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub cover: Option<gdk::Texture>,
    pub position: Option<Duration>,
    pub duration: Option<Duration>,
    pub volume: f64,
    pub playing: bool,
    pub loading: bool,
    pub can_previous: bool,
    pub can_play_pause: bool,
    pub can_next: bool,
    pub can_seek: bool,
    pub can_volume: bool,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub can_change_order: bool,
}

#[derive(Clone)]
pub(in crate::ui) struct MiniPlayback(Weak<Controller>);

impl MiniPlayback {
    pub(super) fn new(controller: &Rc<Controller>) -> Self {
        Self(Rc::downgrade(controller))
    }

    pub(in crate::ui) fn snapshot(&self) -> Option<MiniSnapshot> {
        self.0.upgrade()?.mini_snapshot()
    }

    pub(in crate::ui) fn set_reactions_host(&self, host: Option<&gtk::Box>) {
        if let Some(controller) = self.0.upgrade() {
            controller.set_mini_reactions_host(host);
        }
    }

    pub(in crate::ui) fn command(&self, command: MiniCommand) {
        let Some(controller) = self.0.upgrade() else {
            return;
        };
        let Some(snapshot) = controller.mini_snapshot() else {
            return;
        };
        match command {
            MiniCommand::Previous if snapshot.can_previous => controller.skip_music(false, false),
            MiniCommand::PlayPause if snapshot.can_play_pause => controller.toggle_music(),
            MiniCommand::Next if snapshot.can_next => controller.skip_music(true, false),
            MiniCommand::Seek(position)
                if snapshot.can_seek
                    && snapshot
                        .duration
                        .is_some_and(|duration| position <= duration) =>
            {
                controller.seek_music(position);
            }
            MiniCommand::Volume(volume) if snapshot.can_volume => {
                controller.set_music_volume(volume)
            }
            MiniCommand::VolumeSteps(steps) if snapshot.can_volume => {
                let volume =
                    (snapshot.volume + steps * super::volume_scroll::VOLUME_STEP).clamp(0.0, 1.0);
                if volume != snapshot.volume {
                    controller.set_music_volume(volume);
                }
            }
            MiniCommand::Shuffle(enabled) if snapshot.can_change_order => {
                controller.set_music_shuffle(enabled);
            }
            MiniCommand::Repeat if snapshot.can_change_order => {
                controller.set_music_repeat(snapshot.repeat.next_mode());
            }
            _ => {}
        }
    }
}
