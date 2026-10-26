// SPDX-License-Identifier: GPL-3.0-or-later
//! Evidence-backed Music capabilities and the future permitted-stream boundary.
//!
//! Yandex ID's documented public-client flow is distinct from permission to use
//! Yandex Music. Anonymous HTTP status, generic profile permission and a working
//! local audio engine cannot promote this capability to Music authentication.

use std::fmt;

use thiserror::Error;

/// The currently established Music authentication capability. There is no
/// enabled sign-in flow until Music-specific authorization is established.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthenticationCapability {
    AuthenticationUnavailable(AuthenticationUnavailable),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum AuthenticationUnavailable {
    #[error(
        "Music-specific authorization and permitted stream access for Karin are not established."
    )]
    MusicGrantNotEstablished,
}

impl AuthenticationUnavailable {
    /// An owner action, not a request for credentials or an account login.
    pub const fn next_action(self) -> &'static str {
        match self {
            Self::MusicGrantNotEstablished => {
                "Validate the owner-registered Karin-owned public Linux client with the opt-in local PKCE diagnostic; confirm Music API acceptance and integration/playback rights separately."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MusicCapabilities {
    pub authentication: AuthenticationCapability,
}

impl MusicCapabilities {
    /// Local, evidence-backed policy. This performs no HTTP request and does not
    /// accept a connection report or a generic Yandex ID token as proof.
    pub const fn current() -> Self {
        Self {
            authentication: AuthenticationCapability::AuthenticationUnavailable(
                AuthenticationUnavailable::MusicGrantNotEstablished,
            ),
        }
    }
}

/// An opaque future service identifier. This only imposes storage/safety bounds;
/// it does not assert a Yandex Music identifier schema or catalog availability.
#[derive(Clone, PartialEq, Eq)]
pub struct ServiceTrackId(String);

impl ServiceTrackId {
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidServiceTrackId> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(InvalidServiceTrackId);
        }
        Ok(Self(value))
    }

    /// Intended for a future permitted adapter, not diagnostic/log output.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ServiceTrackId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ServiceTrackId(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("The service track identifier is empty, contains control characters or exceeds 256 bytes.")]
pub struct InvalidServiceTrackId;

/// A future verified transport-to-player handoff, deliberately unconstructible
/// in Gate A. There is no authorized resolver or arbitrary-URI constructor.
///
/// Concrete fields and a guarded constructor require a separately established
/// permission, entitlement and stream-delivery contract. Local developer media
/// belongs to the independent playback engine, not to this type.
#[derive(Debug)]
pub enum PermittedStream {}

/// Future service boundary only: no production implementation is enabled.
///
/// A future implementation must establish Music-specific authorization,
/// integration permission and content entitlement before producing a stream.
/// It must keep transport credentials and delivery details out of GTK and
/// redact errors. A generic Yandex ID token is not that evidence.
pub trait StreamResolver: Send + Sync {
    fn resolve_permitted_stream(
        &self,
        track: &ServiceTrackId,
    ) -> Result<PermittedStream, StreamResolutionError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum StreamResolutionError {
    #[error(transparent)]
    AuthenticationUnavailable(#[from] AuthenticationUnavailable),
    #[error("The service restricts access to this content.")]
    AccessRestricted,
    #[error("The requested content is unavailable.")]
    ContentUnavailable,
    #[error("The service could not be reached.")]
    Transport,
    #[error("The service returned an unsupported stream response.")]
    InvalidResponse,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_identifiers_preserve_value_without_leaking_it_in_debug() {
        let id = ServiceTrackId::new("private-example:album").expect("bounded opaque ID");
        assert_eq!(id.as_str(), "private-example:album");
        assert_eq!(format!("{id:?}"), "ServiceTrackId(<redacted>)");
        assert!(!format!("{id:?}").contains(id.as_str()));
    }

    #[test]
    fn invalid_identifiers_return_a_fixed_error_without_echoing_input() {
        for invalid in ["", " \t", "private\nidentifier", "private\0identifier"] {
            assert_eq!(ServiceTrackId::new(invalid), Err(InvalidServiceTrackId));
        }
        assert_eq!(
            ServiceTrackId::new("x".repeat(257)),
            Err(InvalidServiceTrackId)
        );
        assert!(ServiceTrackId::new("x".repeat(256)).is_ok());
        assert!(!InvalidServiceTrackId.to_string().contains("private"));
    }

    #[test]
    fn unavailable_error_preserves_the_actionable_music_specific_blocker() {
        let blocker = AuthenticationUnavailable::MusicGrantNotEstablished;
        let error = StreamResolutionError::from(blocker);
        assert_eq!(
            error,
            StreamResolutionError::AuthenticationUnavailable(blocker)
        );
        assert!(error.to_string().contains("Music-specific"));
        assert!(
            blocker
                .next_action()
                .contains("Karin-owned public Linux client")
        );
    }
}
