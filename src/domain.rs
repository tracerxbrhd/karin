// SPDX-License-Identifier: GPL-3.0-or-later
//! Transport-independent connection state. A reachable API never implies authentication.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountAccess {
    /// The API returned an anonymous account status, not a signed-in account.
    Anonymous,
    AuthorizationRequired,
    Forbidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionReport {
    pub access: AccountAccess,
    pub http_status: u16,
    /// The service's explicit availability flag; absent for HTTP access errors.
    pub service_available: Option<bool>,
}

/// Safe, bounded error categories. Raw server bodies, request URLs and credentials
/// are intentionally excluded from displayable errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServiceError {
    #[error("The service could not be reached. Check the network connection.")]
    Network,
    #[error("The service request timed out.")]
    Timeout,
    #[error("The service returned HTTP {0}.")]
    Http(u16),
    #[error("The service returned an unsupported account status response.")]
    InvalidResponse,
    #[error("The service response exceeded the diagnostic size limit.")]
    ResponseTooLarge,
    #[error("The HTTP client could not be initialized.")]
    Configuration,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    NotChecked,
    Checking,
    Available(ConnectionReport),
    Failed(ServiceError),
}

impl ConnectionState {
    /// Return false for duplicate checks, so only one request can be in flight.
    pub fn begin_check(&mut self) -> bool {
        if matches!(self, Self::Checking) {
            return false;
        }
        *self = Self::Checking;
        true
    }

    /// Apply an outstanding check's result. Unsolicited completions are ignored.
    pub fn complete(&mut self, result: Result<ConnectionReport, ServiceError>) -> bool {
        if !matches!(self, Self::Checking) {
            return false;
        }
        *self = match result {
            Ok(report) => Self::Available(report),
            Err(error) => Self::Failed(error),
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anonymous_report() -> ConnectionReport {
        ConnectionReport {
            access: AccountAccess::Anonymous,
            http_status: 200,
            service_available: Some(false),
        }
    }

    #[test]
    fn duplicate_checks_do_not_replace_the_pending_state() {
        let mut state = ConnectionState::default();
        assert!(state.begin_check());
        assert!(!state.begin_check());
        assert_eq!(state, ConnectionState::Checking);
        assert!(state.complete(Ok(anonymous_report())));
        assert_eq!(state, ConnectionState::Available(anonymous_report()));
    }

    #[test]
    fn errors_can_be_retried_without_claiming_authorization() {
        let mut state = ConnectionState::default();
        assert!(state.begin_check());
        assert!(state.complete(Err(ServiceError::Timeout)));
        assert_eq!(state, ConnectionState::Failed(ServiceError::Timeout));
        assert!(state.begin_check());
        assert!(state.complete(Ok(anonymous_report())));
        assert!(matches!(
            state,
            ConnectionState::Available(ConnectionReport {
                access: AccountAccess::Anonymous,
                service_available: Some(false),
                ..
            })
        ));
    }

    #[test]
    fn late_completions_do_not_change_idle_state() {
        let mut state = ConnectionState::default();
        assert!(!state.complete(Ok(anonymous_report())));
        assert_eq!(state, ConnectionState::NotChecked);
    }
}
