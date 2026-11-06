// SPDX-License-Identifier: GPL-3.0-or-later
//! Fresh online admission layered over independently established storage rights.
use super::*;

/// Production coordinator, with no service permission by default. The delegate
/// owns storage entitlement; online resolution can only narrow it. Neither the
/// local attempt sequence nor a cached file is a provider storage grant.
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
    /// The delegate must establish applicable storage/reuse permission separately
    /// from streaming. Currently Karin has no such Yandex service integration.
    pub fn with_storage_permission(permission: Arc<dyn StorageAuthority>) -> Self {
        Self {
            permission,
            state: Mutex::new(OnlineState::default()),
        }
    }
}
impl StorageAuthority for OnlineStreamAuthority {
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
        self.permission.check(&prepared.permission)
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
        }) {
            Ok(())
        } else {
            Err(CacheError::Revoked)
        }
    }
}

#[cfg(test)]
mod tests;
