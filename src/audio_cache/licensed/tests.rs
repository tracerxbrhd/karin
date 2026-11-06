// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact original bytes with an explicit creator CC0 dedication exercise the
//! real production policy; no service permission or fake authority is supplied.
use super::*;
use crate::audio_cache::test_support::{SyntheticAudio, account};

struct Fixture {
    _directory: Arc<Sandbox>,
    authority: Arc<LicensedResourceAuthority>,
    cache: AudioCache,
    lease: Lease,
    audio: SyntheticAudio,
}
fn resource(audio: &[u8], scope: &crate::cache::AccountScope, track: u64) -> LicensedResource {
    LicensedResource::cc0(
        scope,
        track.into(),
        digest(&SHA256, audio).as_ref().try_into().unwrap(),
        audio.len() as u64,
    )
    .unwrap()
}
impl Fixture {
    fn new() -> Self {
        let directory = Sandbox::create(&std::env::temp_dir()).unwrap();
        let audio = SyntheticAudio::tone(400).unwrap();
        // Karin's test authors created these bytes and dedicate this test audio
        // to CC0-1.0. The declaration authorizes only the measured payload.
        let authority = Arc::new(
            LicensedResourceAuthority::new(vec![resource(&audio.0, &account(1), 101)]).unwrap(),
        );
        let cache = AudioCache::open(
            directory.path.join("audio"),
            Arc::new(OnlineStreamAuthority::with_storage_permission(
                authority.clone(),
            )),
            CachePolicy::default(),
        )
        .unwrap();
        let lease = cache.bind(Some(account(1))).unwrap().unwrap();
        Self {
            _directory: directory,
            authority,
            cache,
            lease,
            audio,
        }
    }
    fn online(&self) {
        let check = self
            .cache
            .begin_online_check(&self.lease, 101u64.into())
            .unwrap();
        self.cache
            .observe_verified_online(&self.lease, check)
            .unwrap();
    }
    fn put(&self, audio: &[u8]) -> Result<()> {
        self.cache
            .put_reader(self.cache.begin(&self.lease, 101u64)?, audio)
    }
}

#[test]
fn exact_creator_permission_and_online_admission_are_both_required() {
    let f = Fixture::new();
    assert!(matches!(
        f.cache.begin(&f.lease, 101u64),
        Err(CacheError::NoGrant)
    ));
    f.online();
    f.put(&f.audio.0).unwrap();
    let hit = f.cache.get(&f.lease, 101u64).unwrap().unwrap();
    assert_eq!(&*hit.admit().unwrap(), &f.audio.0);
    assert!(matches!(
        f.cache.begin(&f.lease, 102u64),
        Err(CacheError::NoGrant)
    ));
    let other = f.cache.bind(Some(account(2))).unwrap().unwrap();
    let check = f.cache.begin_online_check(&other, 101u64.into()).unwrap();
    assert_eq!(
        f.cache.observe_verified_online(&other, check),
        Err(CacheError::NoGrant)
    );
    assert!(matches!(
        f.cache.get(&other, 101u64),
        Err(CacheError::NoGrant)
    ));
}

#[test]
fn wrong_equal_length_or_incomplete_payload_never_replaces_permitted_record() {
    let f = Fixture::new();
    f.online();
    f.put(&f.audio.0).unwrap();
    let mut other = f.audio.0.clone();
    *other.last_mut().unwrap() ^= 1;
    assert_eq!(f.put(&other), Err(CacheError::NoGrant));
    assert_eq!(
        f.put(&f.audio.0[..f.audio.0.len() - 1]),
        Err(CacheError::NoGrant)
    );
    assert_eq!(
        f.put(&[f.audio.0.as_slice(), b"extra"].concat()),
        Err(CacheError::NoGrant)
    );
    assert_eq!(f.cache.stats().unwrap().entries, 1);
    assert_eq!(
        &*f.cache
            .get(&f.lease, 101u64)
            .unwrap()
            .unwrap()
            .admit()
            .unwrap(),
        &f.audio.0
    );
    assert!(
        !fs::read_dir(f._directory.path.join("audio"))
            .unwrap()
            .filter_map(std::result::Result::ok)
            .any(|entry| entry.file_name().as_encoded_bytes().starts_with(b".part-"))
    );
}

#[test]
fn withdrawn_or_replaced_rightsholder_declaration_retires_existing_handles() {
    let f = Fixture::new();
    f.online();
    f.put(&f.audio.0).unwrap();
    let hit = f.cache.get(&f.lease, 101u64).unwrap().unwrap();
    f.authority.revoke(&account(1), 101u64.into()).unwrap();
    assert!(matches!(hit.admit(), Err(CacheError::Revoked)));
    assert!(matches!(
        f.cache.begin(&f.lease, 101u64),
        Err(CacheError::Revoked)
    ));
    f.authority
        .replace(resource(&f.audio.0, &account(1), 101))
        .unwrap();
    // Replacing a declaration cannot revive the old online admission.
    assert!(matches!(hit.admit(), Err(CacheError::Revoked)));
    f.online();
    assert_eq!(
        &*f.cache
            .get(&f.lease, 101u64)
            .unwrap()
            .unwrap()
            .admit()
            .unwrap(),
        &f.audio.0
    );
}

#[test]
fn changed_content_claim_cannot_admit_old_valid_integrity_record() {
    let f = Fixture::new();
    f.online();
    f.put(&f.audio.0).unwrap();
    let other = SyntheticAudio::tone(401).unwrap();
    f.authority
        .replace(resource(&other.0, &account(1), 101))
        .unwrap();
    f.online();
    assert!(f.cache.get(&f.lease, 101u64).unwrap().is_none());
    assert_eq!(f.cache.stats().unwrap().entries, 0);
    f.put(&other.0).unwrap();
    assert_eq!(
        &*f.cache
            .get(&f.lease, 101u64)
            .unwrap()
            .unwrap()
            .admit()
            .unwrap(),
        &other.0
    );
}

#[test]
fn registry_is_bounded_explicit_and_does_not_accept_duplicate_declarations() {
    let scope = account(1);
    let declaration = resource(&SyntheticAudio::tone(10).unwrap().0, &scope, 1);
    assert!(matches!(
        LicensedResourceAuthority::new(vec![declaration, declaration]),
        Err(CacheError::InvalidInput)
    ));
    assert!(matches!(
        LicensedResourceAuthority::new(vec![declaration; MAX_ENTRIES + 1]),
        Err(CacheError::Limit)
    ));
    assert!(matches!(
        LicensedResource::cc0(&scope, 1u64.into(), [0; 32], 0),
        Err(CacheError::InvalidInput)
    ));
    assert!(matches!(
        LicensedResource::cc0(&scope, 1u64.into(), [0; 32], MAX_AUDIO as u64 + 1),
        Err(CacheError::InvalidInput)
    ));
    let empty = LicensedResourceAuthority::new(Vec::new()).unwrap();
    let identity = ResourceIdentity {
        account: scope.audio_cache_key(),
        namespace: 1,
        track: 1u64.into(),
        admission: [0; 32],
    };
    assert!(matches!(
        empty.authorize(&identity),
        Err(CacheError::NoGrant)
    ));
    assert_eq!(
        format!("{declaration:?}"),
        "LicensedAudioResource([opaque])"
    );
}

#[test]
fn presented_online_grant_cannot_drop_its_exact_content_claim() {
    let f = Fixture::new();
    f.online();
    let identity = f.cache.identity(&f.lease, 101u64.into()).unwrap();
    let mut granted = f.cache.0.authority.authorize(&identity).unwrap();
    assert!(granted.content.is_some());
    granted.content = None;
    assert_eq!(
        f.cache.0.authority.check(&granted),
        Err(CacheError::Revoked)
    );
}
