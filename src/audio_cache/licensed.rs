// SPDX-License-Identifier: GPL-3.0-or-later
//! Independently reviewed permission for exact, openly licensed media bytes.
//!
//! This policy consumes trusted rightsholder input supplied by the application
//! assembly. It never interprets a streaming success, API display field, HTTP
//! cache directive or unverified `license` string as a permission. In particular
//! the Yandex production assembly has no such reviewed resource declarations.

use super::*;

#[derive(Clone, Copy)]
enum License {
    Cc0_1_0,
}

/// A reviewed rightsholder declaration, narrowed to one account/track and one
/// exact resource. The declaration is not inferred from the bytes' availability.
#[derive(Clone, Copy)]
pub struct LicensedResource {
    account: Key,
    track: ResourceKey,
    content: ContentClaim,
    license: License,
}
impl LicensedResource {
    /// Only trusted assembly code may supply this declaration after confirming
    /// that the rightsholder applied CC0-1.0 to these exact original bytes.
    pub fn cc0(
        scope: &crate::cache::AccountScope,
        track: ResourceKey,
        sha256: [u8; 32],
        bytes: u64,
    ) -> Result<Self> {
        if bytes == 0 || bytes > MAX_AUDIO as u64 {
            return Err(CacheError::InvalidInput);
        }
        Ok(Self {
            account: scope.audio_cache_key(),
            track,
            content: ContentClaim { sha256, bytes },
            license: License::Cc0_1_0,
        })
    }
    fn key(self) -> Key {
        ResourceIdentity {
            account: self.account,
            namespace: 1,
            track: self.track,
            admission: [0; 32],
        }
        .key()
    }
    fn applies(self, identity: &ResourceIdentity) -> bool {
        matches!(self.license, License::Cc0_1_0)
            && identity.account == self.account
            && identity.namespace == 1
            && identity.track == self.track
    }
}
impl fmt::Debug for LicensedResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LicensedAudioResource([opaque])")
    }
}

struct Permission {
    resource: LicensedResource,
    revision: u64,
    withdrawn: bool,
}

/// Memory-only, bounded permission registry for reviewed resources. It does not
/// resolve sources or perform network/GTK work. Wrap it in OnlineStreamAuthority
/// to additionally require the application's fresh account/resource admission.
/// No CLI setting or Yandex account metadata populates this registry.
pub struct LicensedResourceAuthority {
    resources: Mutex<BTreeMap<Key, Permission>>,
}
impl LicensedResourceAuthority {
    pub fn new(resources: Vec<LicensedResource>) -> Result<Self> {
        if resources.len() > MAX_ENTRIES {
            return Err(CacheError::Limit);
        }
        let mut permissions = BTreeMap::new();
        for resource in resources {
            if permissions
                .insert(
                    resource.key(),
                    Permission {
                        resource,
                        revision: 1,
                        withdrawn: false,
                    },
                )
                .is_some()
            {
                return Err(CacheError::InvalidInput);
            }
        }
        Ok(Self {
            resources: Mutex::new(permissions),
        })
    }

    /// Replace reviewed bytes or reinstate application admission. This does not
    /// claim that an already valid CC0 dedication can legally be revoked.
    pub fn replace(&self, resource: LicensedResource) -> Result<()> {
        let mut permissions = self.resources.lock().map_err(|_| CacheError::Unavailable)?;
        let key = resource.key();
        if permissions.len() >= MAX_ENTRIES && !permissions.contains_key(&key) {
            return Err(CacheError::Limit);
        }
        let revision = permissions
            .get(&key)
            .map_or(Some(1), |prior| prior.revision.checked_add(1))
            .ok_or(CacheError::Limit)?;
        permissions.insert(
            key,
            Permission {
                resource,
                revision,
                withdrawn: false,
            },
        );
        Ok(())
    }

    /// Withdraw application admission, for example after a false declaration is
    /// discovered. Existing requests must not publish after this change.
    pub fn revoke(&self, scope: &crate::cache::AccountScope, track: ResourceKey) -> Result<()> {
        let identity = ResourceIdentity {
            account: scope.audio_cache_key(),
            namespace: 1,
            track,
            admission: [0; 32],
        };
        let mut permissions = self.resources.lock().map_err(|_| CacheError::Unavailable)?;
        let permission = permissions
            .get_mut(&identity.key())
            .ok_or(CacheError::NoGrant)?;
        // Withdrawal stays fail-closed even if the opaque revision is exhausted.
        permission.withdrawn = true;
        permission.revision = permission
            .revision
            .checked_add(1)
            .ok_or(CacheError::Limit)?;
        Ok(())
    }
}
impl StorageAuthority for LicensedResourceAuthority {
    fn authorize(&self, identity: &ResourceIdentity) -> Result<StorageGrant> {
        let permissions = self.resources.lock().map_err(|_| CacheError::Unavailable)?;
        let permission = permissions
            .get(&identity.key())
            .ok_or(CacheError::NoGrant)?;
        if permission.withdrawn {
            return Err(CacheError::Revoked);
        }
        if !permission.resource.applies(identity) {
            return Err(CacheError::NoGrant);
        }
        Ok(StorageGrant {
            identity: *identity,
            revision: permission.revision,
            content: Some(permission.resource.content),
            basis: AdmissionBasis::IndependentLicense,
        })
    }
    fn check(&self, grant: &StorageGrant) -> Result<()> {
        let current = self.authorize(&grant.identity)?;
        if current.revision == grant.revision && current.content == grant.content {
            Ok(())
        } else {
            Err(CacheError::Revoked)
        }
    }
}

#[cfg(test)]
mod tests;
