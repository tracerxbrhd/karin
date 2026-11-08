// SPDX-License-Identifier: GPL-3.0-or-later
//! Fresh playback admission layered over a separately selected cache policy.
use super::*;

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
}
impl StorageAuthority for OnlineStreamAuthority {
    fn private_http(&self) -> bool {
        self.permission.private_http()
    }
    fn begin_online(&self, identity: ResourceIdentity) -> Result<OnlineCheck> {
        let mut state = self.state.lock().map_err(|_| CacheError::Unavailable)?;
        state.current = None;
        state.pending = None;
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
}

#[cfg(test)]
mod tests;
