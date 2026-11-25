// SPDX-License-Identifier: GPL-3.0-or-later
//! Application updates use a transport independent of the music account.

pub(crate) mod flatpak;
mod releases;

pub(crate) use releases::{
    CheckError, CheckResult, Deployment, DeploymentStatus, KEY_PIN, Release, channel_configured,
    check,
};
