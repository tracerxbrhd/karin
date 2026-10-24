//! Application boundary: UI callers receive domain values, never HTTP responses.

use std::sync::Arc;

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
}
