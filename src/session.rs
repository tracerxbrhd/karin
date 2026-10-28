// SPDX-License-Identifier: GPL-3.0-or-later
//! Ephemeral session credentials. No token file or production credential store.
//!
//! The optional developer Device Flow has no bundled application identity. It
//! is separate from Karin's own PKCE diagnostic and absent from default builds.

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use zeroize::Zeroizing;

/// A process-memory-only token. Never use its contents in UI, logs or diagnostics.
#[derive(Clone)]
pub struct AccessToken {
    value: Zeroizing<String>,
    expires_at: Option<Instant>,
    invalidated: Arc<AtomicBool>,
}

impl AccessToken {
    pub(crate) fn expose(&self) -> &str {
        &self.value
    }

    /// Unknown provider expiry is not an invented successful refresh lifecycle.
    /// The API still rejects expired/revoked credentials independently.
    pub fn is_expired(&self) -> bool {
        self.invalidated.load(Ordering::Acquire)
            || self.expires_at.is_some_and(|end| Instant::now() >= end)
    }

    /// Local cancellation across worker clones; this does not revoke a provider grant.
    pub(crate) fn invalidate(&self) {
        self.invalidated.store(true, Ordering::Release);
    }

    #[cfg(any(test, feature = "experimental-device-auth"))]
    fn new(value: Zeroizing<String>, expires_at: Option<Instant>) -> Self {
        Self {
            value,
            expires_at,
            invalidated: Arc::new(AtomicBool::new(false)),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(value: &str, lifetime: std::time::Duration) -> Self {
        Self::new(
            Zeroizing::new(value.to_owned()),
            Some(Instant::now() + lifetime),
        )
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessToken([redacted])")
    }
}

#[cfg(feature = "experimental-device-auth")]
mod device;
#[cfg(feature = "experimental-device-auth")]
pub use device::{DeviceAuthClient, DeviceChallenge, DevicePoll, SessionError};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn token_debug_never_contains_credentials_and_expiry_is_observed() {
        let token = AccessToken::for_test("synthetic-private-token", Duration::ZERO);
        assert!(token.is_expired());
        assert_eq!(token.expose(), "synthetic-private-token");
        assert_eq!(format!("{token:?}"), "AccessToken([redacted])");
        let unknown_expiry =
            AccessToken::new(Zeroizing::new("synthetic-private-token".into()), None);
        assert!(!unknown_expiry.is_expired());
        let worker = unknown_expiry.clone();
        unknown_expiry.invalidate();
        assert!(
            worker.is_expired(),
            "Logout invalidates queued worker clones locally"
        );
    }
}
