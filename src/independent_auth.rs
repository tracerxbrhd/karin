// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in, owner-local login and protected restoration using Karin's own OAuth
//! registration. See ADR-0037. Ordinary sign-in does not call this module.

use std::{fmt, path::PathBuf, time::Duration};

use crate::{
    auth::{AuthAttempt, AuthError, BrowserMode},
    credentials::{Cancellation, CredentialStore, IndependentCredentialGuard, StorageError},
    music::{MusicError, MusicService},
    oauth::{self, ErrorCategory, OAuthClient, ProbeResult},
    session::AccessToken,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Validated,
    Remembered,
    Restored,
    LoggedOut,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Validated => "Independent Music account access validated for this run.",
            Self::Remembered => {
                "Independent Music account access validated and saved to protected storage."
            }
            Self::Restored => "Independent Music account access restored and revalidated.",
            Self::LoggedOut => "Independent local session cleared.",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Authorization(#[from] AuthError),
    #[error("OAuth / Yandex ID: {0}")]
    OAuth(ProbeResult),
    #[error("Music account validation: {}", music_error(.0))]
    Music(#[from] MusicError),
    #[error("Yandex ID and Music returned different accounts.")]
    AccountMismatch,
    #[error("No independent session is saved.")]
    NoSavedSession,
    #[error("Previous credential change unconfirmed. Retry --independent-auth logout first.")]
    PendingStorage,
    #[error("Independent protected storage: {0}")]
    Storage(#[from] StorageError),
    #[error("Cannot open the system browser.")]
    Browser,
}

fn music_error(error: &MusicError) -> String {
    match (error.http_status(), error) {
        (Some(status), _)
            if !matches!(
                error,
                MusicError::Http(_) | MusicError::SearchResponse { .. }
            ) =>
        {
            format!("{error} (HTTP {status})")
        }
        _ => error.to_string(),
    }
}

impl From<ProbeResult> for Error {
    fn from(probe: ProbeResult) -> Self {
        Self::OAuth(probe)
    }
}

/// Run synchronously in the explicit CLI command, outside the GTK lifecycle.
/// Reports contain only stage outcomes; they never expose the admitted token.
pub struct IndependentAuth {
    oauth: OAuthClient,
    music: MusicService,
    store: CredentialStore,
    state_directory: PathBuf,
}

impl IndependentAuth {
    pub fn new() -> Result<Self, Error> {
        let oauth = OAuthClient::new().map_err(|error| ProbeResult {
            http_status: None,
            error: Some(error),
        })?;
        Ok(Self {
            oauth,
            music: MusicService::new()?,
            store: CredentialStore::independent_auth(),
            state_directory: CredentialStore::independent_auth_directory(),
        })
    }

    pub fn login(
        &mut self,
        client_id: &str,
        registered_port: u16,
        remember: bool,
        cancel: &Cancellation,
        open_browser: impl FnOnce(&str) -> Result<(), Error>,
    ) -> Result<Outcome, Error> {
        request_budget(cancel, None)?;
        let attempt = AuthAttempt::new(client_id, registered_port, BrowserMode::System)?;
        self.login_with_attempt(&attempt, client_id, remember, cancel, open_browser)
    }

    fn login_with_attempt(
        &mut self,
        attempt: &AuthAttempt,
        client_id: &str,
        remember: bool,
        cancel: &Cancellation,
        open_browser: impl FnOnce(&str) -> Result<(), Error>,
    ) -> Result<Outcome, Error> {
        let guard = remember.then(|| self.lock_for_read(cancel)).transpose()?;
        request_budget(cancel, Some(attempt))?;
        open_browser(attempt.authorization_url().as_str())?;
        let code = attempt.receive_callback_until(|| cancel.is_cancelled())?;
        let remaining = request_budget(cancel, Some(attempt))?;
        let token = self.oauth.exchange_loopback(
            client_id,
            attempt.verifier(),
            code.expose(),
            remaining,
        )?;
        drop(code);
        self.validate(client_id, &token, cancel, Some(attempt))?;

        let Some(guard) = guard else {
            return Ok(Outcome::Validated);
        };
        // A crash, timeout or cancellation after publication is ambiguous. A
        // protected credential cannot be restored until the write is verified
        // and this durable reservation has been removed.
        guard.mark_pending(cancel)?;
        self.store.save_validated(&token, cancel)?;
        request_budget(cancel, Some(attempt))?;
        if token.is_expired() {
            return Err(AuthError::TokenExpired.into());
        }
        guard.clear_pending()?;
        Ok(Outcome::Remembered)
    }

    pub fn restore(&mut self, client_id: &str, cancel: &Cancellation) -> Result<Outcome, Error> {
        request_budget(cancel, None)?;
        if !oauth::valid_client_id(client_id) {
            return Err(ProbeResult {
                http_status: None,
                error: Some(ErrorCategory::InvalidInput),
            }
            .into());
        }
        let _guard = self.lock_for_read(cancel)?;
        let token = self.store.load(cancel)?.ok_or(Error::NoSavedSession)?;
        self.validate(client_id, &token, cancel, None)?;
        Ok(Outcome::Restored)
    }

    pub fn logout(cancel: &Cancellation) -> Result<Outcome, Error> {
        forget(
            &CredentialStore::independent_auth(),
            &CredentialStore::independent_auth_directory(),
            cancel,
        )
    }

    fn lock_for_read(&self, cancel: &Cancellation) -> Result<IndependentCredentialGuard, Error> {
        let guard = IndependentCredentialGuard::acquire_at(&self.state_directory, cancel)?;
        if guard.pending()? {
            return Err(Error::PendingStorage);
        }
        Ok(guard)
    }

    fn validate(
        &self,
        client_id: &str,
        token: &AccessToken,
        cancel: &Cancellation,
        attempt: Option<&AuthAttempt>,
    ) -> Result<(), Error> {
        let remaining = request_budget(cancel, attempt)?;
        let uid = self.oauth.identify(client_id, token, remaining)?;
        request_budget(cancel, attempt)?;
        let outcome = self
            .music
            .account(token)
            .and_then(|_| self.music.account_matches(token, uid));
        // This CLI never hands personal state or an authenticated service to
        // the normal application. Each restore starts with fresh checks.
        self.music.clear_session();
        request_budget(cancel, attempt)?;
        if !outcome? {
            return Err(Error::AccountMismatch);
        }
        Ok(())
    }
}

fn forget(
    store: &CredentialStore,
    directory: &std::path::Path,
    cancel: &Cancellation,
) -> Result<Outcome, Error> {
    request_budget(cancel, None)?;
    let guard = IndependentCredentialGuard::acquire_at(directory, cancel)?;
    guard.mark_pending(cancel)?;
    store.delete(cancel)?;
    guard.clear_pending()?;
    Ok(Outcome::LoggedOut)
}

fn request_budget(cancel: &Cancellation, attempt: Option<&AuthAttempt>) -> Result<Duration, Error> {
    if cancel.is_cancelled() {
        return Err(AuthError::Cancelled.into());
    }
    let remaining = attempt.map_or(Duration::MAX, AuthAttempt::time_remaining);
    if remaining.is_zero() {
        return Err(AuthError::Expired.into());
    }
    Ok(remaining)
}

#[cfg(test)]
mod tests;
