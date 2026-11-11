// SPDX-License-Identifier: GPL-3.0-or-later
//! Private HTTP response policy, independent of playback access and copyright.
//!
//! Current account/offer admission is independent from HTTP reuse. A fresh,
//! integrity-checked representation of the same offered object/variant can be
//! reused before resolving another temporary CDN address. Stale/no-cache data
//! still needs representation validation. No URLs or complete headers persist.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(crate) const MAX_METADATA: usize = 8 * 1024;
const MAX_MEDIA: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    Cancelled,
    Network,
    Timeout,
    SourceExpired,
    UnsupportedStatus,
    HeadersInvalid,
    ResponseTooLarge,
    RangeUnsupported,
    NoStore,
    UnsupportedEncoding,
    UnsupportedType,
    InvalidLength,
    PartialResponse,
    VaryUnsupported,
    ValidatorsMissing,
    #[cfg(test)]
    RepresentationChanged,
    #[cfg(test)]
    ResourceChanged,
    ContentChanged,
    LengthChanged,
    ScopedValidatorChanged,
    ValidatorsIncomparable,
    IdentityUnverified,
    EncodingChanged,
    DigestMismatch,
    Integrity,
    ContentRestricted,
    AccessDenied,
    UnsupportedValidator,
    FreshnessMissing,
    FreshnessExpired,
    FreshnessNoCache,
    ClockUncertain,
    ApplicationUnverified,
    ApplicationChanged,
}
impl Reason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::SourceExpired => "source-expired",
            Self::UnsupportedStatus => "http-status-unsupported",
            Self::HeadersInvalid => "http-headers-invalid",
            Self::ResponseTooLarge => "response-too-large",
            Self::RangeUnsupported => "range-unsupported",
            Self::NoStore => "http-no-store",
            Self::UnsupportedEncoding => "content-encoding-unsupported",
            Self::UnsupportedType => "content-type-unsupported",
            Self::InvalidLength => "content-length-invalid",
            Self::PartialResponse => "partial-response",
            Self::VaryUnsupported => "vary-unsupported",
            Self::ValidatorsMissing => "validators-missing",
            #[cfg(test)]
            Self::RepresentationChanged => "representation-changed",
            #[cfg(test)]
            Self::ResourceChanged => "resource-changed",
            Self::ContentChanged => "content-digest-changed",
            Self::LengthChanged => "representation-length-changed",
            Self::ScopedValidatorChanged => "scoped-validator-changed",
            Self::ValidatorsIncomparable => "cdn-validators-incomparable",
            Self::IdentityUnverified => "resource-identity-unverified",
            Self::EncodingChanged => "encoding-changed",
            Self::DigestMismatch => "digest-mismatch",
            Self::Integrity => "metadata-corrupt",
            Self::ContentRestricted => "content-restricted",
            Self::AccessDenied => "access-refused",
            Self::UnsupportedValidator => "validator-unsupported",
            Self::FreshnessMissing => "freshness-unavailable",
            Self::FreshnessExpired => "freshness-expired",
            Self::FreshnessNoCache => "http-revalidation-required",
            Self::ClockUncertain => "freshness-clock-uncertain",
            Self::ApplicationUnverified => "application-identity-unverified",
            Self::ApplicationChanged => "selected-representation-changed",
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Directives {
    pub no_cache: bool,
    pub must_revalidate: bool,
    pub private: bool,
    pub max_age: Option<u64>,
    pub s_maxage: Option<u64>,
    pub age: Option<u64>,
    pub expires_unix: Option<u64>,
    // Both guarded playback and the metadata probe explicitly request identity.
    // Other request-selected dimensions are not represented by this profile.
    #[serde(default)]
    pub vary_accept_encoding: bool,
}
impl fmt::Debug for Directives {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateHttpDirectives([bounded])")
    }
}

#[derive(Clone)]
pub(crate) struct ResponsePolicy {
    pub status: u16,
    pub length: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub modified_unix: Option<u64>,
    pub date_unix: Option<u64>,
    pub repr_sha256: Option<[u8; 32]>,
    pub directives: Directives,
    mp3: bool,
    weak_etag: bool,
    range: Option<(u64, u64, u64)>,
    observed: Option<Observation>,
    cache_control_present: bool,
}
impl fmt::Debug for ResponsePolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateHttpResponsePolicy([redacted])")
    }
}

/// Opaque dimensions of a fresh admitted locator. They classify relocation
/// and selected encoding; no component is an immutable byte-version claim.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResourceBinding {
    #[serde(default = "legacy_binding_schema")]
    pub schema: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical: Option<[u8; 32]>,
    pub host: [u8; 32],
    pub path: [u8; 32],
    pub variant: [u8; 32],
}
fn legacy_binding_schema() -> u8 {
    1
}

/// Current authenticated Music offer identity, independent of temporary CDN
/// placement/signatures. This is a selected object/encoding, not an assertion
/// that its bytes are immutable. HTTP freshness is a separate required proof.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ApplicationIdentity {
    pub logical: [u8; 32],
    pub variant: [u8; 32],
    pub schema: u8,
}
impl fmt::Debug for ApplicationIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApplicationAudioIdentity([opaque])")
    }
}

/// RFC9111 corrected initial age at header receipt. Wall time permits restart;
/// the request delay was measured monotonically, rounded upward. No saved
/// response clock in legacy records means no shortcut, rather than a new TTL.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    pub response_unix: u64,
    pub corrected_initial_age: u64,
    pub response_status: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Freshness {
    Explicit,
    Heuristic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FreshReuse {
    pub kind: Freshness,
    pub remaining: Duration,
}
impl fmt::Debug for ResourceBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ResourceBinding([opaque])")
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
    pub uri_hash: [u8; 32],
    pub resource_hash: [u8; 32],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<ResourceBinding>,
    pub sha256: [u8; 32],
    pub bytes: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub modified_unix: Option<u64>,
    pub date_unix: Option<u64>,
    pub directives: Directives,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed: Option<Observation>,
}
impl fmt::Debug for Metadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateHttpMetadata([redacted])")
    }
}

/// Representation equivalence is distinct from a failed operational/policy
/// check. Unproven keeps a verified candidate without authorizing local use or
/// claiming that its audio bytes changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Validation {
    Same(Box<Metadata>),
    Different(Reason),
    Unproven(Reason),
    /// The old bytes are unproved, but the current metadata permits a new
    /// complete fresh response. This never authorizes reading the old file.
    RefreshFresh(Reason),
}

impl ResponsePolicy {
    /// Whether a newly downloaded complete representation can later be checked
    /// without downloading it again. Storage policy and current access remain
    /// independent requirements; no validator is an offline entitlement.
    pub(crate) fn has_validator(&self) -> bool {
        self.etag.is_some() || self.strong_modified() || self.repr_sha256.is_some()
    }

    /// Bind actual header receipt to the monotonic request duration. This must
    /// be called by the HTTP transport, not at the later end of an audio copy.
    pub(crate) fn observed_at(mut self, elapsed: Duration, response_unix: u64) -> Self {
        let delay = elapsed
            .as_secs()
            .checked_add(u64::from(elapsed.subsec_nanos() != 0));
        let apparent_age = match self.date_unix {
            Some(date) => response_unix.checked_sub(date),
            None => Some(0),
        };
        self.observed = delay
            .and_then(|delay| self.directives.age.unwrap_or(0).checked_add(delay))
            .zip(apparent_age)
            .map(|(age, apparent)| Observation {
                response_unix,
                corrected_initial_age: age.max(apparent),
                response_status: self.status,
            });
        self
    }

    pub(crate) fn observed_now(self, elapsed: Duration) -> Self {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(now) => self.observed_at(elapsed, now.as_secs()),
            Err(_) => self,
        }
    }

    /// Fresh responses need no validator until their reuse lease expires.
    /// Revalidation-capable responses may still be stored without freshness.
    pub(crate) fn reusable(&self) -> bool {
        self.has_validator() || self.fresh_reusable()
    }

    pub(crate) fn fresh_reusable(&self) -> bool {
        matches!(self.status, 200 | 206)
            && self.mp3
            && self.length.is_some_and(|bytes| bytes > 0)
            && !self.directives.no_cache
            && self.observed.is_some_and(|observed| {
                freshness_lifetime(
                    &self.directives,
                    self.date_unix,
                    self.modified_unix,
                    self.strong_modified(),
                    observed,
                )
                .is_some_and(|(seconds, _)| seconds > observed.corrected_initial_age)
            })
    }

    fn unproven(&self, reason: Reason) -> Validation {
        if self.fresh_reusable() {
            Validation::RefreshFresh(reason)
        } else {
            Validation::Unproven(reason)
        }
    }

    pub(crate) fn parse(
        status: u16,
        headers: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, Reason> {
        if matches!(status, 401 | 403) {
            return Err(Reason::AccessDenied);
        }
        if status == 451 {
            return Err(Reason::ContentRestricted);
        }
        if !matches!(status, 200 | 206 | 304) {
            return Err(Reason::UnsupportedStatus);
        }
        let mut result = Self {
            status,
            length: None,
            etag: None,
            last_modified: None,
            modified_unix: None,
            date_unix: None,
            repr_sha256: None,
            directives: Directives::default(),
            mp3: false,
            weak_etag: false,
            range: None,
            observed: None,
            cache_control_present: false,
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut total = 0usize;
        let mut count = 0usize;
        for (name, value) in headers {
            count += 1;
            total = total
                .checked_add(name.len())
                .and_then(|n| n.checked_add(value.len()))
                .ok_or(Reason::ResponseTooLarge)?;
            if count > 64 || total > MAX_METADATA {
                return Err(Reason::ResponseTooLarge);
            }
            if name.is_empty()
                || name.len() > 128
                || !name.bytes().all(token_byte)
                || value.len() > 4096
                || value.bytes().any(|b| b < 0x20 && b != b'\t' || b == 0x7f)
            {
                return Err(Reason::HeadersInvalid);
            }
            let name = name.to_ascii_lowercase();
            let value = value.trim();
            // Repeated selected fields are deliberately rejected rather than
            // choosing whichever spelling or conflicting value arrived first.
            if matches!(
                name.as_str(),
                "content-type"
                    | "content-length"
                    | "content-range"
                    | "content-encoding"
                    | "etag"
                    | "last-modified"
                    | "date"
                    | "expires"
                    | "age"
                    | "repr-digest"
            ) && !seen.insert(name.clone())
            {
                return Err(Reason::HeadersInvalid);
            }
            match name.as_str() {
                "content-type" => {
                    result.mp3 = value
                        .split(';')
                        .next()
                        .is_some_and(|v| v.trim().eq_ignore_ascii_case("audio/mpeg"));
                    if !result.mp3 {
                        return Err(Reason::UnsupportedType);
                    }
                }
                "content-length" => result.length = Some(decimal(value)?),
                "content-range" => result.range = Some(content_range(value)?),
                "content-encoding" if !value.eq_ignore_ascii_case("identity") => {
                    return Err(Reason::UnsupportedEncoding);
                }
                "etag" => {
                    if let Some(weak) = value.strip_prefix("W/") {
                        valid_etag(weak)?;
                        result.weak_etag = true;
                    } else {
                        valid_etag(value)?;
                        result.etag = Some(value.to_owned());
                    }
                }
                "last-modified" => {
                    result.modified_unix = Some(http_date(value)?);
                    result.last_modified = Some(value.to_owned());
                }
                "date" => result.date_unix = Some(http_date(value)?),
                "expires" => {
                    // RFC9111 treats invalid Expires (including 0 and -1) as
                    // already stale, rather than a prohibition on storage.
                    result.directives.expires_unix = Some(http_date(value).unwrap_or(0));
                }
                "age" => result.directives.age = Some(decimal(value)?),
                "cache-control" => {
                    result.cache_control_present = true;
                    cache_control(value, &mut result.directives)?;
                }
                "pragma" => {
                    for item in directives(value)? {
                        if item.trim().eq_ignore_ascii_case("no-cache") {
                            result.directives.no_cache = true;
                        }
                    }
                }
                "vary" if !value.is_empty() => {
                    for field in value.split(',').map(str::trim) {
                        if !field.eq_ignore_ascii_case("accept-encoding") {
                            return Err(Reason::VaryUnsupported);
                        }
                        result.directives.vary_accept_encoding = true;
                    }
                }
                "repr-digest" => result.repr_sha256 = repr_digest(value)?,
                _ => {}
            }
        }
        if result.length.is_some_and(|n| n > MAX_MEDIA) {
            return Err(Reason::InvalidLength);
        }
        Ok(result)
    }

    pub(crate) fn complete_mp3(&self) -> Result<u64, Reason> {
        if !self.mp3 {
            return Err(Reason::UnsupportedType);
        }
        let length = self
            .length
            .filter(|n| *n > 0)
            .ok_or(Reason::InvalidLength)?;
        match (self.status, self.range) {
            (200, None) => Ok(length),
            (206, Some((0, end, total)))
                if end.checked_add(1) == Some(total) && total == length =>
            {
                Ok(length)
            }
            _ => Err(Reason::PartialResponse),
        }
    }

    pub(crate) fn record(
        &self,
        uri_hash: [u8; 32],
        resource_hash: [u8; 32],
        sha256: [u8; 32],
        bytes: u64,
    ) -> Result<Metadata, Reason> {
        if self.complete_mp3()? != bytes {
            return Err(Reason::InvalidLength);
        }
        if self.repr_sha256.is_some_and(|v| v != sha256) {
            return Err(Reason::DigestMismatch);
        }
        Ok(Metadata {
            uri_hash,
            resource_hash,
            binding: None,
            sha256,
            bytes,
            etag: self.etag.clone(),
            last_modified: self
                .strong_modified()
                .then(|| self.last_modified.clone())
                .flatten(),
            modified_unix: self
                .strong_modified()
                .then_some(self.modified_unix)
                .flatten(),
            date_unix: self.date_unix,
            directives: self.directives.clone(),
            observed: self.observed,
        })
    }

    /// Production records retain only opaque locator dimensions. This is not a
    /// second track database or a globally scoped ETag/content identifier.
    pub(crate) fn record_source(
        &self,
        remote: &crate::playback::RemotePlaybackSource,
        sha256: [u8; 32],
        bytes: u64,
    ) -> Result<Metadata, Reason> {
        let mut metadata = self.record(
            remote.http_cache_uri_hash(),
            remote.http_cache_resource_hash(),
            sha256,
            bytes,
        )?;
        metadata.binding = remote.http_cache_binding();
        Ok(metadata)
    }

    pub(crate) fn revalidate_source(
        &self,
        stored: &Metadata,
        remote: &crate::playback::RemotePlaybackSource,
    ) -> Result<Validation, Reason> {
        stored.validate()?;
        if stored
            .binding
            .zip(remote.http_cache_binding())
            .is_some_and(|(before, now)| {
                before.schema == now.schema && before.variant != now.variant
            })
        {
            return Ok(Validation::Different(Reason::EncodingChanged));
        }
        self.revalidate_scoped(
            stored,
            remote.http_cache_uri_hash(),
            remote.http_cache_resource_hash(),
            remote.http_cache_binding(),
            stored.validator_scope(remote),
        )
    }

    /// Numeric-hash test compatibility for the original policy fixtures. The
    /// production path always carries the sealed descriptor dimensions above.
    #[cfg(test)]
    pub(crate) fn revalidate(
        &self,
        stored: &Metadata,
        uri_hash: [u8; 32],
        resource_hash: [u8; 32],
    ) -> Result<(), Reason> {
        match self.revalidate_scoped(
            stored,
            uri_hash,
            resource_hash,
            None,
            uri_hash == stored.uri_hash && resource_hash == stored.resource_hash,
        )? {
            Validation::Same(_) => Ok(()),
            Validation::Different(
                Reason::ContentChanged | Reason::LengthChanged | Reason::ScopedValidatorChanged,
            ) => Err(Reason::RepresentationChanged),
            Validation::Unproven(Reason::IdentityUnverified | Reason::ValidatorsIncomparable)
            | Validation::RefreshFresh(
                Reason::IdentityUnverified | Reason::ValidatorsIncomparable,
            ) => Err(Reason::ResourceChanged),
            Validation::Different(reason)
            | Validation::Unproven(reason)
            | Validation::RefreshFresh(reason) => Err(reason),
        }
    }

    fn revalidate_scoped(
        &self,
        stored: &Metadata,
        uri_hash: [u8; 32],
        resource_hash: [u8; 32],
        binding: Option<ResourceBinding>,
        comparable: bool,
    ) -> Result<Validation, Reason> {
        stored.validate()?;
        // A 304 only describes a conditional request for a selectable stored
        // response. It cannot bootstrap a new URI/replica from a coincidental
        // tag or even an unsolicited digest header.
        if self.status == 304 {
            if !comparable {
                return Ok(Validation::Unproven(Reason::UnsupportedValidator));
            }
            let matched = if let Some(before) = &stored.etag {
                match &self.etag {
                    Some(now) if before == now => true,
                    Some(_) => return Ok(Validation::Unproven(Reason::UnsupportedValidator)),
                    None => return Ok(Validation::Unproven(Reason::ValidatorsMissing)),
                }
            } else if stored.strong_modified() {
                self.strong_modified() && self.modified_unix == stored.modified_unix
            } else {
                return Ok(Validation::Unproven(Reason::UnsupportedValidator));
            };
            if !matched {
                return Ok(Validation::Unproven(Reason::UnsupportedValidator));
            }
        }
        let length = match (self.status, self.range) {
            (200, None) if self.mp3 => self.length,
            (206, Some((0, 0, total))) if self.mp3 && self.length == Some(1) => Some(total),
            (206, Some((0, end, total)))
                if self.mp3 && end.checked_add(1) == Some(total) && self.length == Some(total) =>
            {
                Some(total)
            }
            (304, None) => self.length,
            _ => return Err(Reason::PartialResponse),
        };
        if length.is_some_and(|n| n != stored.bytes) {
            return Ok(if self.status == 304 {
                Validation::Unproven(Reason::UnsupportedValidator)
            } else {
                Validation::Different(Reason::LengthChanged)
            });
        }
        if let Some(digest) = self.repr_sha256 {
            if digest != stored.sha256 {
                return Ok(if self.status == 304 {
                    Validation::Unproven(Reason::UnsupportedValidator)
                } else {
                    Validation::Different(Reason::ContentChanged)
                });
            }
            // A HEAD/range response establishes the whole selected length.
            // Only a scoped conditional 304 on the exact URI can retain it.
            if length == Some(stored.bytes) || self.status == 304 && uri_hash == stored.uri_hash {
                return Ok(Validation::Same(Box::new(self.refreshed(
                    stored,
                    uri_hash,
                    resource_hash,
                    binding,
                )?)));
            }
        }
        if !comparable {
            let old_opaque = stored.etag.is_some() || stored.last_modified.is_some();
            let new_opaque = self.etag.is_some() || self.last_modified.is_some();
            return Ok(self.unproven(if old_opaque && new_opaque {
                Reason::ValidatorsIncomparable
            } else {
                Reason::IdentityUnverified
            }));
        }
        if self.status != 304 && length.is_none() {
            return Err(Reason::InvalidLength);
        }
        let same = if let (Some(before), Some(now)) = (&stored.etag, &self.etag) {
            before == now
        } else if stored.strong_modified() && self.strong_modified() {
            // Both observations must independently meet RFC9110's conservative
            // strong Last-Modified inference and share the proven resource.
            self.modified_unix == stored.modified_unix
        } else {
            return Ok(self.unproven(Reason::ValidatorsMissing));
        };
        if !same {
            return Ok(if self.status == 304 {
                // A contradictory Not Modified response is not proof of either
                // version. Do not replace data based on an invalid response.
                Validation::Unproven(Reason::UnsupportedValidator)
            } else {
                Validation::Different(Reason::ScopedValidatorChanged)
            });
        }
        Ok(Validation::Same(Box::new(self.refreshed(
            stored,
            uri_hash,
            resource_hash,
            binding,
        )?)))
    }

    /// Rebind only after positive byte/version proof. A digest-bridged new
    /// replica receives its own validators; old opaque tags never migrate into
    /// that scope. A valid same-scope304 may retain omitted stored metadata.
    fn refreshed(
        &self,
        stored: &Metadata,
        uri_hash: [u8; 32],
        resource_hash: [u8; 32],
        binding: Option<ResourceBinding>,
    ) -> Result<Metadata, Reason> {
        let mut current = Metadata {
            uri_hash,
            resource_hash,
            binding,
            sha256: stored.sha256,
            bytes: stored.bytes,
            etag: self.etag.clone(),
            last_modified: self
                .strong_modified()
                .then(|| self.last_modified.clone())
                .flatten(),
            modified_unix: self
                .strong_modified()
                .then_some(self.modified_unix)
                .flatten(),
            date_unix: self.date_unix,
            directives: self.directives.clone(),
            observed: self.observed,
        };
        if self.status == 304 {
            if let Some(observed) = &mut current.observed {
                // A validated 304 refreshes an existing representation, rather
                // than changing its original cacheable response status.
                observed.response_status =
                    stored.observed.map_or(0, |before| before.response_status);
            }
            current.etag = current.etag.or_else(|| stored.etag.clone());
            if current.last_modified.is_none() {
                current.last_modified = stored.last_modified.clone();
                current.modified_unix = stored.modified_unix;
            }
            current.date_unix = current.date_unix.or(stored.date_unix);
            if let Some(mut observed) = current.observed
                && let Some(date) = current.date_unix
            {
                current.observed = observed.response_unix.checked_sub(date).map(|apparent| {
                    observed.corrected_initial_age = observed.corrected_initial_age.max(apparent);
                    observed
                });
            }
            // Preserve stricter absent metadata on a 304; never turn an old
            // revalidation requirement into a fresh/offline permission.
            current.directives.no_cache |= stored.directives.no_cache;
            current.directives.must_revalidate |= stored.directives.must_revalidate;
            current.directives.private |= stored.directives.private;
            current.directives.vary_accept_encoding |= stored.directives.vary_accept_encoding;
            // A present304 Cache-Control replaces the stored header as a
            // whole (RFC9111 §3.2). Its omission of max-age must not silently
            // revive an old freshness lifetime over an explicit stale Expires.
            if !self.cache_control_present {
                current.directives.max_age =
                    current.directives.max_age.or(stored.directives.max_age);
                current.directives.s_maxage =
                    current.directives.s_maxage.or(stored.directives.s_maxage);
            }
            current.directives.expires_unix = current
                .directives
                .expires_unix
                .or(stored.directives.expires_unix);
        }
        current.validate()?;
        Ok(current)
    }

    fn strong_modified(&self) -> bool {
        !self.weak_etag && strong_modified(self.modified_unix, self.date_unix)
    }
}

impl Metadata {
    pub(crate) fn needs_initial_refresh(&self) -> bool {
        self.observed
            .is_none_or(|observed| observed.response_status == 0)
            || self.binding.is_none_or(|binding| binding.schema != 2)
    }

    /// A changed current offered object/encoding is a new application
    /// selection. Legacy schemas are unknown, not proof of a version change.
    pub(crate) fn application_match(&self, current: ApplicationIdentity) -> Result<(), Reason> {
        let before = self.binding.ok_or(Reason::ApplicationUnverified)?;
        let logical = before.logical.ok_or(Reason::ApplicationUnverified)?;
        if before.schema != 2 || current.schema != 2 {
            return Err(Reason::ApplicationUnverified);
        }
        if logical != current.logical || before.variant != current.variant {
            return Err(Reason::ApplicationChanged);
        }
        Ok(())
    }

    /// RFC9111 freshness authorizes bounded reuse of the same selected
    /// representation, not permanent/offline rights or immutable object names.
    pub(crate) fn fresh_for(
        &self,
        current: ApplicationIdentity,
        now_unix: u64,
    ) -> Result<FreshReuse, Reason> {
        self.validate()?;
        self.application_match(current)?;
        if self.directives.no_cache {
            return Err(Reason::FreshnessNoCache);
        }
        let observed = self.observed.ok_or(Reason::FreshnessMissing)?;
        let resident = now_unix
            .checked_sub(observed.response_unix)
            .ok_or(Reason::ClockUncertain)?;
        let age = observed
            .corrected_initial_age
            .checked_add(resident)
            .ok_or(Reason::ClockUncertain)?;
        let (lifetime, kind) = self.freshness_lifetime().ok_or(Reason::FreshnessMissing)?;
        if age >= lifetime {
            return Err(Reason::FreshnessExpired);
        }
        Ok(FreshReuse {
            kind,
            remaining: Duration::from_secs(lifetime - age),
        })
    }

    fn freshness_lifetime(&self) -> Option<(u64, Freshness)> {
        freshness_lifetime(
            &self.directives,
            self.date_unix,
            self.modified_unix,
            self.strong_modified(),
            self.observed?,
        )
    }

    /// A logical object or physical locator does not establish one validator
    /// namespace across different request URIs. The deployed legacy signing
    /// timestamp is opaque; only an exact provider request may carry the old
    /// conditional. Resource/variant checks additionally protect legacy records.
    pub(crate) fn validator_scope(&self, remote: &crate::playback::RemotePlaybackSource) -> bool {
        if let Some((before, now)) = self.binding.zip(remote.http_cache_binding())
            && (before.schema == now.schema && before.variant != now.variant
                || before
                    .logical
                    .zip(now.logical)
                    .is_some_and(|(before, now)| before != now))
        {
            return false;
        }
        self.uri_hash == remote.http_cache_uri_hash()
            && self.resource_hash == remote.http_cache_resource_hash()
    }

    pub(crate) fn strong_modified(&self) -> bool {
        strong_modified(self.modified_unix, self.date_unix)
    }

    fn validate(&self) -> Result<(), Reason> {
        if self.bytes == 0 || self.bytes > MAX_MEDIA {
            return Err(Reason::Integrity);
        }
        if let Some(etag) = &self.etag {
            valid_etag(etag).map_err(|_| Reason::Integrity)?;
        }
        match (&self.last_modified, self.modified_unix) {
            (Some(date), Some(unix)) if http_date(date).ok() == Some(unix) => {}
            (None, None) => {}
            _ => return Err(Reason::Integrity),
        }
        Ok(())
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, Reason> {
        self.validate()?;
        let body = serde_json::to_vec(self).map_err(|_| Reason::Integrity)?;
        let mut result = b"KARHTTP1".to_vec();
        result.extend_from_slice(digest(&SHA256, &body).as_ref());
        result.extend_from_slice(&body);
        if result.len() > MAX_METADATA {
            return Err(Reason::ResponseTooLarge);
        }
        Ok(result)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, Reason> {
        if bytes.len() <= 40 || bytes.len() > MAX_METADATA || &bytes[..8] != b"KARHTTP1" {
            return Err(Reason::Integrity);
        }
        let body = &bytes[40..];
        if digest(&SHA256, body).as_ref() != &bytes[8..40] {
            return Err(Reason::Integrity);
        }
        let result: Self = serde_json::from_slice(body).map_err(|_| Reason::Integrity)?;
        result.validate()?;
        Ok(result)
    }
}

fn freshness_lifetime(
    directives: &Directives,
    date: Option<u64>,
    modified: Option<u64>,
    strong_modified: bool,
    observed: Observation,
) -> Option<(u64, Freshness)> {
    // s-maxage governs shared caches only. Karin is account-private.
    if let Some(seconds) = directives.max_age {
        return Some((seconds, Freshness::Explicit));
    }
    if let Some(expires) = directives.expires_unix {
        return Some((
            expires.saturating_sub(date.unwrap_or(observed.response_unix)),
            Freshness::Explicit,
        ));
    }
    // RFC9111 §4.2.2 permits a conservative heuristic when no explicit
    // expiration exists. Use only valid origin Date/strong Last-Modified on an
    // ordinary complete200, <=10% of age, capped at one hour. Never manufacture
    // a track-based TTL or override explicit stale metadata.
    if observed.response_status == 200 && strong_modified {
        let age = date?.checked_sub(modified?)?;
        return Some(((age / 10).min(3600), Freshness::Heuristic));
    }
    None
}

fn strong_modified(modified: Option<u64>, date: Option<u64>) -> bool {
    modified
        .zip(date)
        .is_some_and(|(modified, date)| date.checked_sub(modified).is_some_and(|age| age >= 60))
}

fn valid_etag(value: &str) -> Result<(), Reason> {
    if value.len() < 3
        || value.len() > 258
        || !value.starts_with('"')
        || !value.ends_with('"')
        || !value[1..value.len() - 1].bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'+' | b'/' | b'=')
        })
    {
        return Err(Reason::UnsupportedValidator);
    }
    Ok(())
}

fn decimal(value: &str) -> Result<u64, Reason> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Reason::HeadersInvalid);
    }
    value.parse().map_err(|_| Reason::HeadersInvalid)
}

fn content_range(value: &str) -> Result<(u64, u64, u64), Reason> {
    if value.len() > 96 {
        return Err(Reason::HeadersInvalid);
    }
    let value = value
        .strip_prefix("bytes ")
        .ok_or(Reason::RangeUnsupported)?;
    let (extent, total) = value.split_once('/').ok_or(Reason::RangeUnsupported)?;
    let (start, end) = extent.split_once('-').ok_or(Reason::RangeUnsupported)?;
    let (start, end, total) = (decimal(start)?, decimal(end)?, decimal(total)?);
    if total == 0 || total > MAX_MEDIA || start > end || end >= total {
        return Err(Reason::RangeUnsupported);
    }
    Ok((start, end, total))
}

fn token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

fn directives(value: &str) -> Result<Vec<&str>, Reason> {
    let mut quoted = false;
    let mut start = 0;
    let mut result = Vec::new();
    for (i, b) in value.bytes().enumerate() {
        match b {
            b'\\' => return Err(Reason::HeadersInvalid),
            b'"' => quoted = !quoted,
            b',' if !quoted => {
                result.push(value[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    if quoted {
        return Err(Reason::HeadersInvalid);
    }
    result.push(value[start..].trim());
    if result.iter().any(|v| v.is_empty()) {
        return Err(Reason::HeadersInvalid);
    }
    Ok(result)
}

fn cache_control(value: &str, policy: &mut Directives) -> Result<(), Reason> {
    for item in directives(value)? {
        let (name, value) = item
            .split_once('=')
            .map_or((item.trim(), None), |(name, value)| {
                (name.trim(), Some(value.trim()))
            });
        if name.is_empty() || !name.bytes().all(token_byte) {
            return Err(Reason::HeadersInvalid);
        }
        match name.to_ascii_lowercase().as_str() {
            "no-store" => return Err(Reason::NoStore),
            "no-cache" => policy.no_cache = true,
            "must-revalidate" | "proxy-revalidate" => policy.must_revalidate = true,
            "private" => policy.private = true,
            "max-age" | "s-maxage" => {
                let value = value.ok_or(Reason::HeadersInvalid)?;
                let value = value
                    .strip_prefix('"')
                    .and_then(|v| v.strip_suffix('"'))
                    .unwrap_or(value);
                let number = decimal(value)?;
                let target = if name.eq_ignore_ascii_case("max-age") {
                    &mut policy.max_age
                } else {
                    &mut policy.s_maxage
                };
                if target.replace(number).is_some() {
                    return Err(Reason::HeadersInvalid);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn repr_digest(value: &str) -> Result<Option<[u8; 32]>, Reason> {
    let mut result = None;
    for item in directives(value)? {
        let (algorithm, encoded) = item.split_once('=').ok_or(Reason::HeadersInvalid)?;
        if algorithm.trim() != "sha-256" {
            continue;
        }
        if result.is_some() {
            return Err(Reason::HeadersInvalid);
        }
        let encoded = encoded.trim();
        let encoded = encoded
            .strip_prefix(':')
            .and_then(|v| v.strip_suffix(':'))
            .ok_or(Reason::HeadersInvalid)?;
        if encoded.len() != 44 {
            return Err(Reason::HeadersInvalid);
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| Reason::HeadersInvalid)?;
        result = Some(bytes.try_into().map_err(|_| Reason::HeadersInvalid)?);
    }
    Ok(result)
}

// Strict IMF-fixdate parsing avoids another dependency and rejects ambiguous
// obsolete dates. Incorrect/future validators fail closed instead of guessing.
fn http_date(value: &str) -> Result<u64, Reason> {
    let b = value.as_bytes();
    if !value.is_ascii()
        || b.len() != 29
        || &b[3..5] != b", "
        || b[7] != b' '
        || b[11] != b' '
        || b[16] != b' '
        || b[19] != b':'
        || b[22] != b':'
        || &b[25..] != b" GMT"
        || !["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].contains(&&value[..3])
    {
        return Err(Reason::HeadersInvalid);
    }
    let number = |a, z| decimal(&value[a..z]).map(|n| n as i64);
    let day = number(5, 7)?;
    let year = number(12, 16)?;
    let hour = number(17, 19)?;
    let minute = number(20, 22)?;
    let second = number(23, 25)?;
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|month| *month == &value[8..11])
    .map(|n| n as i64 + 1)
    .ok_or(Reason::HeadersInvalid)?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1970..=9999).contains(&year)
        || !(1..=days).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(Reason::HeadersInvalid);
    }
    let y = year - i64::from(month <= 2);
    let era = y / 400;
    let yoe = y - era * 400;
    let m = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    if days < 0 {
        return Err(Reason::HeadersInvalid);
    }
    if ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][days as usize % 7] != &value[..3] {
        return Err(Reason::HeadersInvalid);
    }
    Ok((days * 86400 + hour * 3600 + minute * 60 + second) as u64)
}

#[cfg(test)]
mod tests;
