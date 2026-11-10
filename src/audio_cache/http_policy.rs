// SPDX-License-Identifier: GPL-3.0-or-later
//! Private HTTP response policy, independent of playback access and copyright.
//!
//! All hits require a new online validation, even when a response advertises a
//! freshness lifetime. Only selected bounded validators are persisted; neither
//! URLs nor complete response/request headers belong in a cache record.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use std::fmt;

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
    RepresentationChanged,
    ResourceChanged,
    DigestMismatch,
    Integrity,
    ContentRestricted,
    AccessDenied,
    UnsupportedValidator,
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
            Self::RepresentationChanged => "representation-changed",
            Self::ResourceChanged => "resource-changed",
            Self::DigestMismatch => "digest-mismatch",
            Self::Integrity => "metadata-corrupt",
            Self::ContentRestricted => "content-restricted",
            Self::AccessDenied => "access-refused",
            Self::UnsupportedValidator => "validator-unsupported",
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
}
impl fmt::Debug for ResponsePolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateHttpResponsePolicy([redacted])")
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
    pub uri_hash: [u8; 32],
    pub resource_hash: [u8; 32],
    pub sha256: [u8; 32],
    pub bytes: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub modified_unix: Option<u64>,
    pub date_unix: Option<u64>,
    pub directives: Directives,
}
impl fmt::Debug for Metadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateHttpMetadata([redacted])")
    }
}

impl ResponsePolicy {
    /// Whether a newly downloaded complete representation can later be checked
    /// without downloading it again. Storage policy and current access remain
    /// independent requirements; no validator is an offline entitlement.
    pub(crate) fn has_validator(&self) -> bool {
        self.etag.is_some() || self.strong_modified() || self.repr_sha256.is_some()
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
                "cache-control" => cache_control(value, &mut result.directives)?,
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
        })
    }

    /// Validate a newly admitted response without downloading its full body.
    /// A digest covers the complete selected representation, including on HEAD
    /// and partial responses; Content-Digest is intentionally never used here.
    pub(crate) fn revalidate(
        &self,
        stored: &Metadata,
        uri_hash: [u8; 32],
        resource_hash: [u8; 32],
    ) -> Result<(), Reason> {
        stored.validate()?;
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
            return Err(Reason::RepresentationChanged);
        }
        if let Some(digest) = self.repr_sha256 {
            if digest != stored.sha256 {
                return Err(Reason::RepresentationChanged);
            }
            // A HEAD/range response must establish the full representation's
            // size. 304 can retain its known length only on the same URI.
            if length == Some(stored.bytes) || self.status == 304 && uri_hash == stored.uri_hash {
                return Ok(());
            }
        }
        if uri_hash != stored.uri_hash && resource_hash != stored.resource_hash {
            return Err(Reason::ResourceChanged);
        }
        if self.status != 304 && length.is_none() {
            return Err(Reason::InvalidLength);
        }
        if let (Some(before), Some(now)) = (&stored.etag, &self.etag) {
            return if before == now {
                Ok(())
            } else {
                Err(Reason::RepresentationChanged)
            };
        }
        // RFC9110's conservative strong Last-Modified inference: the origin's
        // Date is at least 60 seconds newer. Require this for both observations,
        // the same resource and the same date, never a bare weak 304 response.
        if stored.strong_modified() && self.strong_modified() {
            return if self.modified_unix == stored.modified_unix {
                Ok(())
            } else {
                Err(Reason::RepresentationChanged)
            };
        }
        Err(Reason::ValidatorsMissing)
    }

    fn strong_modified(&self) -> bool {
        !self.weak_etag && strong_modified(self.modified_unix, self.date_unix)
    }
}

impl Metadata {
    fn strong_modified(&self) -> bool {
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
