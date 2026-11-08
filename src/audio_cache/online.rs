// SPDX-License-Identifier: GPL-3.0-or-later
//! Fresh playback admission layered over a separately selected cache policy.
use super::*;

const ACTIVE_WRITE_LIMIT: Duration = Duration::from_secs(30 * 60);
const SEALED_WRITE_LIMIT: Duration = Duration::from_secs(30);

/// Production coordinator. A delegate can provide an independently licensed
/// admission or a private HTTP candidate; the latter still needs actual response
/// policy before publication and fresh representation validation before a hit.
pub struct OnlineStreamAuthority {
    permission: Arc<dyn StorageAuthority>,
    state: Mutex<OnlineState>,
}
#[derive(Default)]
struct OnlineState {
    sequence: u64,
    pending: Option<(ResourceIdentity, bool)>,
    current: Option<(StorageGrant, StorageGrant)>, // presented admission, delegate grant
    writes: BTreeMap<u64, RegisteredWrite>,
    consumed_write: Option<StorageGrant>,
}

#[derive(Clone, Copy)]
struct RegisteredWrite {
    presented: StorageGrant,
    permission: StorageGrant,
    deadline: Instant,
    sealed: bool,
}

fn same_grant(left: &StorageGrant, right: &StorageGrant) -> bool {
    left.identity == right.identity
        && left.revision == right.revision
        && left.content == right.content
        && left.basis == right.basis
}

fn same_context(left: &ResourceIdentity, right: &ResourceIdentity) -> bool {
    left.account == right.account
        && left.namespace == right.namespace
        && left.admission == right.admission
}

impl Default for OnlineStreamAuthority {
    fn default() -> Self {
        Self::with_storage_permission(Arc::new(MissingStorageContract))
    }
}
impl OnlineStreamAuthority {
    /// Select a local cache policy independently from playback access. A private
    /// HTTP candidate still requires actual response and representation checks;
    /// an independent license policy retains its own rights constraints.
    pub fn with_storage_permission(permission: Arc<dyn StorageAuthority>) -> Self {
        Self {
            permission,
            state: Mutex::new(OnlineState::default()),
        }
    }

    fn registered_write(
        &self,
        candidate: &StorageGrant,
        allow_retired_unsealed: bool,
    ) -> Result<RegisteredWrite> {
        let mut state = self.state.lock().map_err(|_| CacheError::Unavailable)?;
        let registered = state
            .writes
            .get(&candidate.revision)
            .filter(|registered| same_grant(&registered.presented, candidate))
            .copied()
            .ok_or(CacheError::Revoked)?;
        if registered.deadline <= Instant::now() {
            state.writes.remove(&candidate.revision);
            if state
                .current
                .is_some_and(|(current, _)| same_grant(&current, candidate))
            {
                state.consumed_write = Some(*candidate);
            }
            return Err(CacheError::Expired);
        }
        if !registered.sealed
            && !allow_retired_unsealed
            && !state
                .current
                .is_some_and(|(current, _)| same_grant(&current, candidate))
        {
            return Err(CacheError::Revoked);
        }
        Ok(registered)
    }
}
impl StorageAuthority for OnlineStreamAuthority {
    fn private_http(&self) -> bool {
        self.permission.private_http()
    }
    fn begin_online(&self, identity: ResourceIdentity) -> Result<OnlineCheck> {
        let mut state = self.state.lock().map_err(|_| CacheError::Unavailable)?;
        // A new playback attempt still retires every old hit and incomplete
        // write check. Only registered writes in the same account/lease can
        // subsequently be sealed by the owner's actual completed capture.
        let now = Instant::now();
        state.writes.retain(|_, registered| {
            registered.deadline > now && same_context(&registered.presented.identity, &identity)
        });
        state.current = None;
        state.pending = None;
        state.consumed_write = None;
        state.sequence = state.sequence.checked_add(1).ok_or(CacheError::Limit)?;
        state.pending = Some((identity, false));
        Ok(OnlineCheck {
            identity,
            sequence: state.sequence,
        })
    }
    fn prepare_online(&self, check: OnlineCheck) -> Result<VerifiedOnline> {
        {
            let mut state = self.state.lock().map_err(|_| CacheError::Unavailable)?;
            if state.sequence != check.sequence || state.pending != Some((check.identity, false)) {
                return Err(CacheError::Cancelled);
            }
            state.pending = Some((check.identity, true));
        }
        // Slow permission checks never hold a mutex needed by GTK fencing.
        let permission = self.permission.authorize(&check.identity)?;
        if permission.identity != check.identity {
            return Err(CacheError::Cancelled);
        }
        self.permission.check(&permission)?;
        Ok(VerifiedOnline { check, permission })
    }
    fn check_storage_prepared(&self, prepared: &VerifiedOnline) -> Result<()> {
        self.permission.check(&prepared.permission)?;
        let state = self.state.lock().map_err(|_| CacheError::Unavailable)?;
        if state.sequence == prepared.check.sequence
            && state.pending == Some((prepared.check.identity, true))
        {
            Ok(())
        } else {
            Err(CacheError::Cancelled)
        }
    }
    fn commit_online(&self, prepared: VerifiedOnline) -> Result<()> {
        let VerifiedOnline { check, permission } = prepared;
        self.permission.check(&permission)?; // memory-only revocation lease
        let mut state = self.state.lock().map_err(|_| CacheError::Unavailable)?;
        if state.sequence != check.sequence || state.pending != Some((check.identity, true)) {
            return Err(CacheError::Cancelled);
        }
        state.pending = None;
        state.current = Some((
            StorageGrant {
                identity: check.identity,
                revision: check.sequence,
                content: permission.content,
                basis: permission.basis,
            },
            permission,
        ));
        Ok(())
    }
    fn authorize(&self, identity: &ResourceIdentity) -> Result<StorageGrant> {
        let (presented, permission) = {
            let state = self.state.lock().map_err(|_| CacheError::Unavailable)?;
            state
                .current
                .filter(|(grant, _)| grant.identity == *identity)
                .ok_or(CacheError::NoGrant)?
        };
        self.permission.check(&permission)?;
        self.check(&presented)?;
        Ok(presented)
    }
    fn check(&self, grant: &StorageGrant) -> Result<()> {
        let permission = {
            let state = self.state.lock().map_err(|_| CacheError::Unavailable)?;
            state
                .current
                .filter(|(current, _)| {
                    current.identity == grant.identity
                        && current.revision == grant.revision
                        && current.content == grant.content
                        && current.basis == grant.basis
                })
                .map(|(_, permission)| permission)
                .ok_or(CacheError::Revoked)?
        };
        self.permission.check(&permission)?;
        let state = self.state.lock().map_err(|_| CacheError::Unavailable)?;
        if state.current.is_some_and(|(current, _)| {
            current.identity == grant.identity
                && current.revision == grant.revision
                && current.content == grant.content
                && current.basis == grant.basis
        }) {
            Ok(())
        } else {
            Err(CacheError::Revoked)
        }
    }

    fn retain_write(&self, grant: &StorageGrant) -> Result<()> {
        // Registration is possible only while this exact fresh playback
        // admission is current. The copied delegate is rechecked independently
        // at every later write/finalization checkpoint.
        self.check(grant)?;
        let mut state = self.state.lock().map_err(|_| CacheError::Unavailable)?;
        let permission = state
            .current
            .filter(|(current, _)| same_grant(current, grant))
            .map(|(_, permission)| permission)
            .ok_or(CacheError::Revoked)?;
        if state
            .consumed_write
            .is_some_and(|consumed| same_grant(&consumed, grant))
        {
            return Err(CacheError::Revoked);
        }
        let now = Instant::now();
        if let Some(prior) = state.writes.get(&grant.revision) {
            if prior.deadline <= now {
                state.writes.remove(&grant.revision);
                state.consumed_write = Some(*grant);
                return Err(CacheError::Expired);
            }
            return Err(CacheError::Duplicate);
        }
        state
            .writes
            .retain(|_, registered| registered.deadline > now);
        if state.writes.len() >= MAX_REQUESTS {
            return Err(CacheError::Limit);
        }
        let deadline = now
            .checked_add(ACTIVE_WRITE_LIMIT)
            .ok_or(CacheError::Limit)?;
        state.writes.insert(
            grant.revision,
            RegisteredWrite {
                presented: *grant,
                permission,
                deadline,
                sealed: false,
            },
        );
        Ok(())
    }

    fn check_write(&self, grant: &StorageGrant) -> Result<()> {
        let registered = self.registered_write(grant, false)?;
        self.permission.check(&registered.permission)?;
        self.registered_write(grant, false).map(|_| ())
    }

    fn seal_write(&self, grant: &StorageGrant) -> Result<()> {
        // The owning Request supplies the completed-and-played capture proof.
        // Registration predates that event, so noticing it after next-track
        // admission cannot lose a physically complete resource. This method
        // never makes the old grant current for playback or cache reuse.
        let registered = self.registered_write(grant, true)?;
        self.permission.check(&registered.permission)?;
        let mut state = self.state.lock().map_err(|_| CacheError::Unavailable)?;
        let registered = state
            .writes
            .get_mut(&grant.revision)
            .filter(|registered| same_grant(&registered.presented, grant))
            .ok_or(CacheError::Revoked)?;
        let now = Instant::now();
        if registered.deadline <= now {
            return Err(CacheError::Expired);
        }
        if !registered.sealed {
            registered.deadline = registered.deadline.min(
                now.checked_add(SEALED_WRITE_LIMIT)
                    .ok_or(CacheError::Limit)?,
            );
            registered.sealed = true;
        }
        Ok(())
    }

    fn release_write(&self, grant: &StorageGrant) {
        if let Ok(mut state) = self.state.lock() {
            if state
                .writes
                .get(&grant.revision)
                .is_some_and(|registered| same_grant(&registered.presented, grant))
            {
                state.writes.remove(&grant.revision);
            }
            if state
                .current
                .is_some_and(|(current, _)| same_grant(&current, grant))
            {
                // A request is a one-use operation. Releasing it cannot renew
                // its finite lifetime without another fresh online attempt.
                state.consumed_write = Some(*grant);
            }
        }
    }

    fn revoke_writes(&self, identity: &ResourceIdentity, sequence: u64) {
        if let Ok(mut state) = self.state.lock() {
            if state.sequence != sequence
                || !(state
                    .pending
                    .is_some_and(|(pending, _)| pending == *identity)
                    || state.current.is_some_and(|(current, _)| {
                        current.identity == *identity && current.revision == sequence
                    }))
            {
                return;
            }
            state
                .writes
                .retain(|_, registered| registered.presented.identity != *identity);
            if let Some((current, _)) = state.current
                && current.identity == *identity
            {
                state.consumed_write = Some(current);
            }
        }
    }
}

#[cfg(test)]
mod tests;
