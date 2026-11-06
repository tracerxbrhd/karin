// SPDX-License-Identifier: GPL-3.0-or-later
//! Fake rights and original generated media; never compiled into the application.
use super::*;

/// The only input constructor synthesizes original audio; it cannot fetch or
/// import a service file. This small PCM WAV is independently decodable by Gst.
pub(super) struct SyntheticAudio(pub(super) Vec<u8>);
impl SyntheticAudio {
    pub(super) fn tone(samples: u32) -> Result<Self> {
        let data_len = samples.checked_mul(2).ok_or(CacheError::Limit)?;
        if data_len == 0 || data_len as usize + 44 > MAX_AUDIO {
            return Err(CacheError::Limit);
        }
        let mut data = Vec::with_capacity(data_len as usize + 44);
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&(data_len + 36).to_le_bytes());
        data.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        data.extend_from_slice(&8_000u32.to_le_bytes());
        data.extend_from_slice(&16_000u32.to_le_bytes());
        data.extend_from_slice(b"\x02\0\x10\0data");
        data.extend_from_slice(&data_len.to_le_bytes());
        for sample in 0..samples {
            let amplitude: i16 = if sample % 20 < 10 { 512 } else { -512 };
            data.extend_from_slice(&amplitude.to_le_bytes());
        }
        Ok(Self(data))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Identity {
    pub(super) account: u64,
    pub(super) device: u64,
    pub(super) track: u64,
}
impl Identity {
    pub(super) fn real(self) -> ResourceIdentity {
        ResourceIdentity {
            account: account(self.account).audio_cache_key(),
            namespace: self.device,
            track: self.track.into(),
            admission: [0; 32],
        }
    }
    pub(super) fn key(self) -> Key {
        self.real().key()
    }
}
pub(super) fn account(value: u64) -> crate::cache::AccountScope {
    crate::cache::AccountScope::from_account_id(&value.to_string()).unwrap()
}
impl From<u64> for ResourceKey {
    fn from(value: u64) -> Self {
        Self::track(&value.to_string()).unwrap()
    }
}
#[derive(Clone, Copy)]
pub(super) struct SyntheticRights {
    pub(super) revision: u64,
    pub(super) expires_at: u64,
    pub(super) offline_allowed: bool,
    pub(super) online_check_required: bool,
    pub(super) revoked: bool,
}
#[derive(Default)]
pub(crate) struct SyntheticAuthority {
    pub(super) rights: Mutex<BTreeMap<Key, SyntheticRights>>,
    pub(super) now: AtomicU64,
    pub(super) online: AtomicBool,
}
impl SyntheticAuthority {
    pub(super) fn grant(&self, identity: Identity, expires_at: u64, offline_allowed: bool) {
        self.grant_identity(identity.real(), expires_at, offline_allowed)
    }
    fn grant_identity(&self, identity: ResourceIdentity, expires_at: u64, offline_allowed: bool) {
        let mut rights = self.rights.lock().unwrap();
        let revision = rights.get(&identity.key()).map_or(1, |r| r.revision + 1);
        rights.insert(
            identity.key(),
            SyntheticRights {
                revision,
                expires_at,
                offline_allowed,
                online_check_required: false,
                revoked: false,
            },
        );
    }
    pub(super) fn revoke(&self, identity: Identity) {
        if let Some(rights) = self.rights.lock().unwrap().get_mut(&identity.key()) {
            rights.revoked = true;
            rights.revision += 1;
        }
    }
    pub(crate) fn allow(&self, scope: &crate::cache::AccountScope, track: &ResourceKey) {
        self.online.store(true, Ordering::Release);
        self.grant_identity(
            ResourceIdentity {
                account: scope.audio_cache_key(),
                namespace: 1,
                track: *track,
                admission: [0; 32],
            },
            100,
            false,
        );
    }
    pub(crate) fn deny(&self, scope: &crate::cache::AccountScope, track: &ResourceKey) {
        let identity = ResourceIdentity {
            account: scope.audio_cache_key(),
            namespace: 1,
            track: *track,
            admission: [0; 32],
        };
        if let Some(rights) = self.rights.lock().unwrap().get_mut(&identity.key()) {
            rights.revoked = true;
            rights.revision += 1;
        }
    }
}
impl StorageAuthority for SyntheticAuthority {
    fn authorize(&self, identity: &ResourceIdentity) -> Result<StorageGrant> {
        let all = self.rights.lock().map_err(|_| CacheError::Unavailable)?;
        let rights = all.get(&identity.key()).ok_or(CacheError::NoGrant)?;
        if rights.revoked {
            return Err(CacheError::Revoked);
        }
        if self.now.load(Ordering::Acquire) >= rights.expires_at {
            return Err(CacheError::Expired);
        }
        if (rights.online_check_required || !rights.offline_allowed)
            && !self.online.load(Ordering::Acquire)
        {
            return Err(CacheError::NeedsOnline);
        }
        Ok(StorageGrant {
            identity: *identity,
            revision: rights.revision,
        })
    }
    fn check(&self, permit: &StorageGrant) -> Result<()> {
        let current = self.authorize(&permit.identity)?;
        if current.revision != permit.revision {
            return Err(CacheError::Cancelled);
        }
        Ok(())
    }
}
pub(super) fn test_open(
    sandbox: Arc<Sandbox>,
    authority: Arc<SyntheticAuthority>,
    device: u64,
    budget: u64,
) -> Result<AudioCache> {
    AudioCache::open_namespace(sandbox, authority, device, budget)
}
