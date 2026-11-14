// SPDX-License-Identifier: GPL-3.0-or-later
//! Karin's technical foundation. UI and external transports meet at application services.

pub mod api;
pub mod application;
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
pub mod domain;
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
