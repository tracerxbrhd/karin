// SPDX-License-Identifier: GPL-3.0-or-later
//! Generation-safe persistence work. Music account validation remains the
//! independent authorization boundary; a loaded token alone is never a login.
use crate::{
    credentials::{BackendKind, Cancellation, CredentialStore, StorageError},
    session::AccessToken,
};
use std::{
    fmt,
    sync::{
        Arc, Mutex, MutexGuard, TryLockError, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PersistenceState {
    #[default]
    Unknown,
    Probing,
    Ready(BackendKind),
    Restoring,
    /// Credential found, still waiting for independent Music validation.
    AwaitingValidation,
    Saving,
    Remembered,
    Removing,
    MemoryOnly,
    Failed(StorageError),
    DeleteFailed(StorageError),
}

#[derive(Clone)]
pub struct PersistenceTicket {
    generation: u64,
    cancellation: Cancellation,
    owner: Weak<Inner>,
}
impl PersistenceTicket {
    pub fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }
}
impl fmt::Debug for PersistenceTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PersistenceTicket")
            .field("generation", &self.generation)
            .field("cancelled", &self.cancellation.is_cancelled())
            .finish()
    }
}

#[derive(Clone)]
pub struct SessionPersistence {
    inner: Arc<Inner>,
}
struct Inner {
    store: CredentialStore,
    serial: Mutex<()>,
    generation: AtomicU64,
    active: Mutex<Option<Cancellation>>,
    state: Mutex<PersistenceState>,
}
impl fmt::Debug for SessionPersistence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionPersistence")
            .field("state", &self.state())
            .finish()
    }
}
impl SessionPersistence {
    pub fn new(store: CredentialStore) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                serial: Mutex::new(()),
                generation: AtomicU64::new(0),
                active: Mutex::new(None),
                state: Mutex::new(PersistenceState::Unknown),
            }),
        }
    }
    /// Start the newest intent before scheduling it on a worker. Older waiting
    /// work is discarded, and a running operation receives cancellation.
    pub fn begin(&self) -> PersistenceTicket {
        let mut active = self.inner.active.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(previous) = active.take() {
            previous.cancel();
        }
        let generation = self
            .inner
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let cancellation = Cancellation::new();
        *active = Some(cancellation.clone());
        PersistenceTicket {
            generation,
            cancellation,
            owner: Arc::downgrade(&self.inner),
        }
    }
    pub fn cancel_pending(&self) {
        let mut active = self.inner.active.lock().unwrap_or_else(|p| p.into_inner());
        self.inner.generation.fetch_add(1, Ordering::AcqRel);
        if let Some(previous) = active.take() {
            previous.cancel();
        }
    }
    pub fn is_current(&self, ticket: &PersistenceTicket) -> bool {
        ticket
            .owner
            .upgrade()
            .is_some_and(|owner| Arc::ptr_eq(&owner, &self.inner))
            && ticket.generation == self.inner.generation.load(Ordering::Acquire)
            && !ticket.cancellation.is_cancelled()
    }
    pub fn state(&self) -> PersistenceState {
        *self.inner.state.lock().unwrap_or_else(|p| p.into_inner())
    }
    fn check(&self, ticket: &PersistenceTicket) -> Result<(), StorageError> {
        if self.is_current(ticket) {
            Ok(())
        } else {
            Err(StorageError::Cancelled)
        }
    }
    fn state_if_current(&self, ticket: &PersistenceTicket, value: PersistenceState) {
        let mut state = self.inner.state.lock().unwrap_or_else(|p| p.into_inner());
        if self.is_current(ticket) {
            *state = value;
        }
    }
    fn acquire(&self, ticket: &PersistenceTicket) -> Result<MutexGuard<'_, ()>, StorageError> {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            self.check(ticket)?;
            match self.inner.serial.try_lock() {
                Ok(guard) => {
                    self.check(ticket)?;
                    return Ok(guard);
                }
                Err(TryLockError::Poisoned(_)) => return Err(StorageError::Unconfirmed),
                Err(TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        return Err(StorageError::Timeout);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }
    pub fn probe(&self, ticket: &PersistenceTicket) -> Result<BackendKind, StorageError> {
        let _serial = self.acquire(ticket)?;
        self.state_if_current(ticket, PersistenceState::Probing);
        let result = self.inner.store.probe(ticket.cancellation());
        self.check(ticket)?;
        self.state_if_current(
            ticket,
            match result {
                Ok(kind) => PersistenceState::Ready(kind),
                Err(error) => PersistenceState::Failed(error),
            },
        );
        result
    }
    /// Returned credentials must be independently accepted by MusicService.
    pub fn load(&self, ticket: &PersistenceTicket) -> Result<Option<AccessToken>, StorageError> {
        let _serial = self.acquire(ticket)?;
        self.state_if_current(ticket, PersistenceState::Restoring);
        let result = self.inner.store.load(ticket.cancellation());
        self.check(ticket)?;
        self.state_if_current(
            ticket,
            match &result {
                Ok(Some(_)) => PersistenceState::AwaitingValidation,
                Ok(None) => PersistenceState::MemoryOnly,
                Err(error) => PersistenceState::Failed(*error),
            },
        );
        result
    }
    /// Called only after successful Music account validation, never merely
    /// after decryption, Yandex ID success, cached metadata or token issuance.
    pub fn accept_restored(&self, ticket: &PersistenceTicket) -> Result<(), StorageError> {
        self.check(ticket)?;
        self.state_if_current(ticket, PersistenceState::Remembered);
        Ok(())
    }
    pub fn save_validated(
        &self,
        ticket: &PersistenceTicket,
        token: &AccessToken,
    ) -> Result<(), StorageError> {
        let _serial = self.acquire(ticket)?;
        self.state_if_current(ticket, PersistenceState::Saving);
        let result = self
            .inner
            .store
            .save_validated(token, ticket.cancellation());
        // The server may have committed CreateItem (or rename) before a late
        // cancellation was observed. Compensate while holding the IO lock, so
        // neither logout nor a newer save can be overtaken by this write.
        if !self.is_current(ticket) || token.is_expired() {
            let cleanup = self.inner.store.delete(&Cancellation::new());
            if self.is_current(ticket) {
                self.state_if_current(
                    ticket,
                    if cleanup.is_ok() {
                        PersistenceState::MemoryOnly
                    } else {
                        PersistenceState::DeleteFailed(StorageError::Unconfirmed)
                    },
                );
            }
            return if cleanup.is_ok() {
                Err(StorageError::Cancelled)
            } else {
                Err(StorageError::Unconfirmed)
            };
        }
        self.state_if_current(
            ticket,
            match result {
                Ok(()) => PersistenceState::Remembered,
                Err(error) => PersistenceState::Failed(error),
            },
        );
        result
    }
    /// Persist a durable forget-pending barrier in application settings BEFORE
    /// scheduling this operation. Keep it on errors/unknown results, including
    /// restart; only confirmed success may clear that barrier.
    pub fn delete(&self, ticket: &PersistenceTicket) -> Result<(), StorageError> {
        let _serial = self.acquire(ticket)?;
        self.state_if_current(ticket, PersistenceState::Removing);
        let result = self.inner.store.delete(ticket.cancellation());
        self.check(ticket)?;
        self.state_if_current(
            ticket,
            match result {
                Ok(()) => PersistenceState::MemoryOnly,
                Err(error) => PersistenceState::DeleteFailed(error),
            },
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::CredentialBackend;
    use std::{sync::mpsc, thread};
    use zeroize::Zeroizing;
    #[derive(Default)]
    struct Memory {
        value: Mutex<Option<AccessToken>>,
        fail_delete: Mutex<bool>,
    }
    impl CredentialBackend for Arc<Memory> {
        fn probe(&self, c: &Cancellation) -> Result<BackendKind, StorageError> {
            c.check()?;
            Ok(BackendKind::SecretService)
        }
        fn load(&self, c: &Cancellation) -> Result<Option<AccessToken>, StorageError> {
            c.check()?;
            Ok(self.value.lock().unwrap().clone())
        }
        fn save_validated(
            &self,
            token: &AccessToken,
            c: &Cancellation,
        ) -> Result<(), StorageError> {
            c.check()?;
            *self.value.lock().unwrap() = Some(AccessToken::new(
                Zeroizing::new(token.expose().to_owned()),
                None,
            ));
            Ok(())
        }
        fn delete(&self, c: &Cancellation) -> Result<(), StorageError> {
            c.check()?;
            if *self.fail_delete.lock().unwrap() {
                return Err(StorageError::Unconfirmed);
            }
            *self.value.lock().unwrap() = None;
            Ok(())
        }
    }
    fn token() -> AccessToken {
        AccessToken::new(Zeroizing::new("synthetic-persistent-token".into()), None)
    }
    #[test]
    fn restore_requires_separate_validation_and_delete_failure_remains_explicit() {
        let backend = Arc::new(Memory::default());
        let manager = SessionPersistence::new(CredentialStore::with_backend(backend.clone()));
        let ticket = manager.begin();
        assert!(manager.load(&ticket).unwrap().is_none());
        assert_eq!(manager.state(), PersistenceState::MemoryOnly);
        manager.save_validated(&ticket, &token()).unwrap();
        assert_eq!(manager.state(), PersistenceState::Remembered);
        let restored = manager.begin();
        assert!(manager.load(&restored).unwrap().is_some());
        assert_eq!(manager.state(), PersistenceState::AwaitingValidation);
        manager.accept_restored(&restored).unwrap();
        assert_eq!(manager.state(), PersistenceState::Remembered);
        *backend.fail_delete.lock().unwrap() = true;
        let deleting = manager.begin();
        assert_eq!(manager.delete(&deleting), Err(StorageError::Unconfirmed));
        assert_eq!(
            manager.state(),
            PersistenceState::DeleteFailed(StorageError::Unconfirmed)
        );
        assert!(backend.value.lock().unwrap().is_some());
        *backend.fail_delete.lock().unwrap() = false;
        let retry = manager.begin();
        manager.delete(&retry).unwrap();
        assert_eq!(manager.state(), PersistenceState::MemoryOnly);
        assert_eq!(
            manager.accept_restored(&restored),
            Err(StorageError::Cancelled)
        );
    }
    #[test]
    fn cancellation_and_expiry_do_not_write_a_credential() {
        let backend = Arc::new(Memory::default());
        let manager = SessionPersistence::new(CredentialStore::with_backend(backend.clone()));
        let old = manager.begin();
        manager.cancel_pending();
        assert_eq!(
            manager.save_validated(&old, &token()),
            Err(StorageError::Cancelled)
        );
        assert!(backend.value.lock().unwrap().is_none());
        let invalid = token();
        invalid.invalidate();
        let ticket = manager.begin();
        assert_eq!(
            manager.save_validated(&ticket, &invalid),
            Err(StorageError::Cancelled)
        );
        assert!(backend.value.lock().unwrap().is_none());
        assert_eq!(manager.state(), PersistenceState::MemoryOnly);
    }
    #[test]
    fn tickets_are_bound_to_the_exact_manager() {
        let store = CredentialStore::with_backend(Arc::new(Memory::default()));
        let first = SessionPersistence::new(store.clone());
        let second = SessionPersistence::new(store);
        let foreign = first.begin();
        let own = second.begin();
        assert!(!second.is_current(&foreign));
        assert!(second.is_current(&own));
        assert_eq!(second.probe(&foreign), Err(StorageError::Cancelled));
        assert_eq!(second.state(), PersistenceState::Unknown);
    }
    struct LateSave {
        value: Mutex<Option<AccessToken>>,
        entered: mpsc::SyncSender<()>,
        release: Mutex<mpsc::Receiver<()>>,
    }
    impl CredentialBackend for Arc<LateSave> {
        fn probe(&self, _: &Cancellation) -> Result<BackendKind, StorageError> {
            Ok(BackendKind::SecretService)
        }
        fn load(&self, _: &Cancellation) -> Result<Option<AccessToken>, StorageError> {
            Ok(self.value.lock().unwrap().clone())
        }
        fn save_validated(&self, t: &AccessToken, _: &Cancellation) -> Result<(), StorageError> {
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
            *self.value.lock().unwrap() = Some(t.clone());
            Ok(())
        }
        fn delete(&self, _: &Cancellation) -> Result<(), StorageError> {
            *self.value.lock().unwrap() = None;
            Ok(())
        }
    }
    #[test]
    fn a_committed_late_save_is_compensated_before_logout_or_newer_work() {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let backend = Arc::new(LateSave {
            value: Mutex::new(None),
            entered: entered_tx,
            release: Mutex::new(release_rx),
        });
        let manager = SessionPersistence::new(CredentialStore::with_backend(backend.clone()));
        let old = manager.begin();
        let worker = manager.clone();
        let saved = thread::spawn(move || worker.save_validated(&old, &token()));
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let logout = manager.begin();
        let worker = manager.clone();
        let deleted = thread::spawn(move || worker.delete(&logout));
        release_tx.send(()).unwrap();
        assert_eq!(saved.join().unwrap(), Err(StorageError::Cancelled));
        assert_eq!(deleted.join().unwrap(), Ok(()));
        assert!(backend.value.lock().unwrap().is_none());
        assert_eq!(manager.state(), PersistenceState::MemoryOnly);
    }
    #[test]
    fn token_invalidated_during_current_save_settles_after_compensation() {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let backend = Arc::new(LateSave {
            value: Mutex::new(None),
            entered: entered_tx,
            release: Mutex::new(release_rx),
        });
        let manager = SessionPersistence::new(CredentialStore::with_backend(backend.clone()));
        let ticket = manager.begin();
        let worker = manager.clone();
        let saving = token();
        let transport = saving.clone();
        let saved = thread::spawn(move || worker.save_validated(&ticket, &saving));
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        transport.invalidate();
        release_tx.send(()).unwrap();
        assert_eq!(saved.join().unwrap(), Err(StorageError::Cancelled));
        assert!(backend.value.lock().unwrap().is_none());
        assert_eq!(manager.state(), PersistenceState::MemoryOnly);
    }
}
