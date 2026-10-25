// SPDX-License-Identifier: GPL-3.0-or-later
//! Application boundary: UI callers receive domain values, never HTTP responses.

use std::sync::Arc;

use crate::capabilities::MusicCapabilities;
use crate::domain::{ConnectionReport, ServiceError};

/// Implementations may block. Invoke this interface on a worker, never the GTK
/// main thread. Production calls have bounded network timeouts.
pub trait MusicApi: Send + Sync {
    fn check_connection(&self) -> Result<ConnectionReport, ServiceError>;
}

#[derive(Clone)]
pub struct ConnectionService {
    api: Arc<dyn MusicApi>,
}

impl ConnectionService {
    pub fn new(api: impl MusicApi + 'static) -> Self {
        Self { api: Arc::new(api) }
    }

    /// Read established Music capabilities without a request or user data.
    /// Anonymous reachability and generic ID permission do not enable sign-in.
    pub fn capabilities(&self) -> MusicCapabilities {
        MusicCapabilities::current()
    }

    /// Check anonymous service availability. This does not authenticate or imply
    /// that a subscription, music metadata or audio streams are accessible.
    pub fn check_connection(&self) -> Result<ConnectionReport, ServiceError> {
        self.api.check_connection()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::capabilities::{AuthenticationCapability, AuthenticationUnavailable};
    use crate::domain::{AccountAccess, ConnectionState};

    struct StubApi {
        calls: Arc<AtomicUsize>,
        response: Result<ConnectionReport, ServiceError>,
    }

    impl MusicApi for StubApi {
        fn check_connection(&self) -> Result<ConnectionReport, ServiceError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.response.clone()
        }
    }

    #[test]
    fn service_preserves_access_restrictions_and_shared_adapter() {
        let calls = Arc::new(AtomicUsize::new(0));
        let report = ConnectionReport {
            access: AccountAccess::Forbidden,
            http_status: 403,
            service_available: None,
        };
        let service = ConnectionService::new(StubApi {
            calls: calls.clone(),
            response: Ok(report.clone()),
        });
        let mut state = ConnectionState::default();
        assert!(state.begin_check());
        assert!(state.complete(service.clone().check_connection()));
        assert_eq!(state, ConnectionState::Available(report));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn network_failure_remains_an_error_in_the_ui_model() {
        let service = ConnectionService::new(StubApi {
            calls: Arc::new(AtomicUsize::new(0)),
            response: Err(ServiceError::Network),
        });
        let mut state = ConnectionState::default();
        assert!(state.begin_check());
        assert!(state.complete(service.check_connection()));
        assert_eq!(state, ConnectionState::Failed(ServiceError::Network));
    }

    #[test]
    fn capability_read_is_local_and_anonymous_success_does_not_enable_music_authentication() {
        let calls = Arc::new(AtomicUsize::new(0));
        let service = ConnectionService::new(StubApi {
            calls: calls.clone(),
            response: Ok(ConnectionReport {
                access: AccountAccess::Anonymous,
                http_status: 200,
                service_available: Some(true),
            }),
        });
        let unavailable = AuthenticationCapability::AuthenticationUnavailable(
            AuthenticationUnavailable::MusicGrantNotEstablished,
        );

        assert_eq!(service.capabilities().authentication, unavailable);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let report = service.check_connection().expect("anonymous HTTP 200");
        assert_eq!(report.access, AccountAccess::Anonymous);
        assert_eq!(report.service_available, Some(true));
        assert_eq!(service.capabilities().authentication, unavailable);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
