// SPDX-License-Identifier: GPL-3.0-or-later
//! Karin's technical foundation. UI and external transports meet at application services.

pub mod api;
pub mod application;
pub mod application_fixture;
pub mod artwork;
pub mod audio;
// Production storage service; Music persistence requires a reviewed grant.
pub mod audio_cache;
pub mod auth;
pub mod cache;
pub mod cache_budget;
pub mod capabilities;
pub mod catalog_cache;
pub mod config;
pub mod credentials;
mod discord_presence;
pub mod domain;
pub mod independent_auth;
pub mod lyrics;
pub mod mpris;
pub mod music;
pub mod oauth;
pub mod platform;
pub mod playback;
pub(crate) mod playback_session;
pub mod queue;
pub mod session;
pub mod session_lifecycle;
pub mod settings;
pub mod stream;
pub mod ui;
pub(crate) mod updates;

#[doc(hidden)]
pub use updates::flatpak::{accept_restart, finish_restart};
