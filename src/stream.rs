// SPDX-License-Identifier: GPL-3.0-or-later
//! Owner-local, opt-in delivery-metadata experiment using Karin's current session.
//!
//! The diagnostic makes one authenticated metadata request and at most one
//! unauthenticated descriptor request; it never requests or stores media.
//! The separate resolver validates an explicit full MP3 offer and constructs a
//! short-lived legacy source for the existing playback boundary. The legacy
//! `direct` hint alone never establishes a playable resource.
//! The endpoint and field names were compared with Cassette v0.2.4, revision
//! 9d34251d01bcd414ffbec2bef7931090d33c9124. No modern extracted signing key
//! or protected-media/decryption mechanism is used.

use std::{
    fmt,
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use reqwest::{
    Url,
    blocking::{Client, Response},
    header::{ACCEPT, AUTHORIZATION, HeaderValue},
    redirect::Policy,
};
use serde::{Deserialize, Deserializer, de::IgnoredAny};
use zeroize::Zeroizing;

use crate::{music::MusicService, session::AccessToken};

const ORIGIN: &str = "https://api.music.yandex.net/";
const MAX_BODY: usize = 128 * 1024;
const MAX_METADATA_BODY: usize = 64 * 1024;
const MAX_OPTIONS: usize = 64;
const MAX_RESOURCE_URL: usize = 8192;

/// Cancellation invalidates a pending result. A bounded in-flight HTTP request
/// can continue until its deadline; neither logout nor GTK waits for it.
#[derive(Clone, Default)]
pub struct DeliveryCancellation(Arc<AtomicBool>);

impl DeliveryCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn cancelled(&self, token: &AccessToken) -> bool {
        self.0.load(Ordering::Acquire) || token.is_expired()
    }
}

impl fmt::Debug for DeliveryCancellation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeliveryCancellation")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryCategory {
    Configuration,
    SessionRequired,
    InvalidTrack,
    Cancelled,
    Network,
    Timeout,
    AccessDenied,
    Forbidden,
    MissingScopes,
    Restricted,
    RateLimited,
    HttpRejected,
    ResponseTooLarge,
    UnsupportedResponse,
    NoDeliveryOptions,
    ProtectionRequired,
    /// A legacy descriptor was offered; it is not itself a playable resource.
    /// The diagnostic remains independent of resolver and media playback.
    LegacyNegotiationRequired,
    MetadataOriginUnverified,
    MetadataPathUnverified,
    LegacySignatureRequired,
    PreviewOnly,
    PreviewUnverified,
    UnsupportedCodec,
    MediaOriginUnverified,
    SourceExpired,
    UnsafeResource,
}

impl DeliveryCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::SessionRequired => "music-session-required",
            Self::InvalidTrack => "invalid-track",
            Self::Cancelled => "cancelled",
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::AccessDenied => "authorization-rejected",
            Self::Forbidden => "forbidden",
            Self::MissingScopes => "missing-scopes",
            Self::Restricted => "restricted",
            Self::RateLimited => "rate-limited",
            Self::HttpRejected => "http-rejected",
            Self::ResponseTooLarge => "response-too-large",
            Self::UnsupportedResponse => "unsupported-response",
            Self::NoDeliveryOptions => "no-delivery-options",
            Self::ProtectionRequired => "protection-required",
            Self::LegacyNegotiationRequired => "legacy-negotiation-required",
            Self::MetadataOriginUnverified => "metadata-origin-unverified",
            Self::MetadataPathUnverified => "metadata-path-unverified",
            Self::PreviewOnly => "preview-only",
            Self::PreviewUnverified => "preview-permission-unverified",
            Self::UnsupportedCodec => "mp3-offer-unavailable",
            Self::MediaOriginUnverified => "media-origin-unverified",
            Self::SourceExpired => "source-expired",
            Self::LegacySignatureRequired => "legacy-client-signature-required",
            Self::UnsafeResource => "unsafe-resource-offered",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeliveryFormats {
    pub mp3: bool,
    pub aac: bool,
    pub flac: bool,
    pub other: bool,
}

impl fmt::Display for DeliveryFormats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut separator = "";
        for (present, name) in [
            (self.mp3, "mp3"),
            (self.aac, "aac"),
            (self.flac, "flac"),
            (self.other, "other"),
        ] {
            if present {
                write!(f, "{separator}{name}")?;
                separator = "+";
            }
        }
        if separator.is_empty() {
            f.write_str("none")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeliveryResource {
    #[default]
    None,
    HttpsDescriptor,
    InsecureDescriptor,
    Unsafe,
}

impl DeliveryResource {
    const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::HttpsDescriptor => "https-descriptor",
            Self::InsecureDescriptor => "insecure-descriptor",
            Self::Unsafe => "unsafe",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryStage {
    Info,
    Descriptor,
    Resolution,
}
impl DeliveryStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "delivery-info",
            Self::Descriptor => "delivery-descriptor",
            Self::Resolution => "stream-resolution",
        }
    }
}

/// Only finite, non-personal values cross the transport/UI boundary. No URL,
/// identifier, request header, body, provider message or credential is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeliveryReport {
    pub stage: DeliveryStage,
    pub category: DeliveryCategory,
    pub http_status: Option<u16>,
    /// Separate HTTP result of the optional descriptor-only request. An API
    /// success must not hide a later denied/unsupported metadata response.
    pub metadata_http_status: Option<u16>,
    pub metadata_attempted: bool,
    pub formats: DeliveryFormats,
    pub resource: DeliveryResource,
    pub protection_required: bool,
    /// A provider hint, explicitly not a verified direct or playable resource.
    pub direct_hint: bool,
    pub preview_hint: bool,
}

impl DeliveryReport {
    fn new(category: DeliveryCategory, http_status: Option<u16>) -> Self {
        Self {
            stage: DeliveryStage::Info,
            category,
            http_status,
            metadata_http_status: None,
            metadata_attempted: false,
            formats: DeliveryFormats::default(),
            resource: DeliveryResource::None,
            protection_required: false,
            direct_hint: false,
            preview_hint: false,
        }
    }
}

impl fmt::Display for DeliveryReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "stage={} category={}",
            self.stage.as_str(),
            self.category.as_str()
        )?;
        if let Some(status) = self.http_status {
            write!(f, " http={status}")?;
        } else {
            f.write_str(" http=none")?;
        }
        if let Some(status) = self.metadata_http_status {
            write!(f, " metadata-http={status}")?;
        } else {
            f.write_str(" metadata-http=none")?;
        }
        write!(
            f,
            " formats={} resource={} protection={} direct-hint={} preview-hint={}",
            self.formats,
            self.resource.as_str(),
            if self.protection_required {
                "required"
            } else {
                "not-established"
            },
            self.direct_hint,
            self.preview_hint,
        )
    }
}

/// One API read and at most one descriptor-only read, with no retries, redirects,
/// media request or user-supplied credentials. Use on a worker, never GTK.
#[derive(Clone)]
pub struct DeliveryProbe {
    client: Client,
    origin: Url,
    #[cfg(test)]
    metadata_fixture_origin: Option<Url>,
}

impl fmt::Debug for DeliveryProbe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeliveryProbe([redacted])")
    }
}

impl DeliveryProbe {
    pub fn new() -> Result<Self, DeliveryReport> {
        let client = Client::builder()
            .user_agent(concat!(
                "Karin/",
                env!("CARGO_PKG_VERSION"),
                " delivery-metadata"
            ))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .map_err(|_| DeliveryReport::new(DeliveryCategory::Configuration, None))?;
        Ok(Self {
            client,
            origin: Url::parse(ORIGIN)
                .map_err(|_| DeliveryReport::new(DeliveryCategory::Configuration, None))?,
            #[cfg(test)]
            metadata_fixture_origin: None,
        })
    }

    pub fn probe(
        &self,
        service: &MusicService,
        token: &AccessToken,
        track_id: &str,
        cancellation: &DeliveryCancellation,
    ) -> DeliveryReport {
        let (body, status) = match self.delivery_info(service, token, track_id, cancellation) {
            Ok(response) => response,
            Err(report) => return report,
        };
        let report = classify(&body, status);
        if report.category != DeliveryCategory::LegacyNegotiationRequired {
            return report;
        }
        match self.metadata_url(&body) {
            Ok(url) => self.probe_descriptor(report, url, token, cancellation),
            Err(category) => DeliveryReport { category, ..report },
        }
    }

    fn delivery_info(
        &self,
        service: &MusicService,
        token: &AccessToken,
        track_id: &str,
        cancellation: &DeliveryCancellation,
    ) -> Result<(Zeroizing<Vec<u8>>, u16), DeliveryReport> {
        if cancellation.cancelled(token) {
            return Err(DeliveryReport::new(DeliveryCategory::Cancelled, None));
        }
        if service.validate_session(token).is_err() {
            return Err(DeliveryReport::new(DeliveryCategory::SessionRequired, None));
        }
        // A service-returned track identifier may include an album suffix. No
        // free-form path, query, URL or account identifier is accepted here.
        if track_id.is_empty()
            || track_id.len() > 128
            || !track_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b':')
            || track_id.split(':').count() > 2
            || track_id.split(':').any(str::is_empty)
        {
            return Err(DeliveryReport::new(DeliveryCategory::InvalidTrack, None));
        }
        let mut url = self.origin.clone();
        let Ok(mut segments) = url.path_segments_mut() else {
            return Err(DeliveryReport::new(DeliveryCategory::Configuration, None));
        };
        segments.pop_if_empty();
        segments.extend(["tracks", track_id, "download-info"]);
        drop(segments);
        let authorization = Zeroizing::new(format!("OAuth {}", token.expose()));
        let Ok(mut header) = HeaderValue::from_str(&authorization) else {
            return Err(DeliveryReport::new(DeliveryCategory::SessionRequired, None));
        };
        header.set_sensitive(true);
        let response = self
            .client
            .get(url)
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, header)
            .send();
        if cancellation.cancelled(token) {
            return Err(DeliveryReport::new(DeliveryCategory::Cancelled, None));
        }
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                return Err(DeliveryReport::new(
                    if error.is_timeout() {
                        DeliveryCategory::Timeout
                    } else {
                        DeliveryCategory::Network
                    },
                    None,
                ));
            }
        };
        let status = response.status().as_u16();
        let outcome = read_body(response, token, cancellation, MAX_BODY);
        if cancellation.cancelled(token) {
            return Err(DeliveryReport::new(DeliveryCategory::Cancelled, None));
        }
        if status != 200 {
            let category = match status {
                401 => DeliveryCategory::AccessDenied,
                403 => outcome
                    .as_ref()
                    .map(|body| forbidden_category(body))
                    .unwrap_or(DeliveryCategory::Forbidden),
                429 => DeliveryCategory::RateLimited,
                451 => DeliveryCategory::Restricted,
                _ => DeliveryCategory::HttpRejected,
            };
            return Err(DeliveryReport::new(category, Some(status)));
        }
        outcome
            .map(|body| (body, status))
            .map_err(|category| DeliveryReport::new(category, Some(status)))
    }

    fn metadata_url(&self, body: &[u8]) -> Result<Url, DeliveryCategory> {
        let envelope = serde_json::from_slice::<Envelope>(body)
            .map_err(|_| DeliveryCategory::UnsupportedResponse)?;
        if envelope
            .result
            .iter()
            .any(|option| option.transport == DeliveryTransport::Protected)
        {
            return Err(DeliveryCategory::ProtectionRequired);
        }
        if envelope
            .result
            .iter()
            .any(|option| option.transport == DeliveryTransport::Unsupported)
        {
            return Err(DeliveryCategory::UnsupportedResponse);
        }
        let mut failure = DeliveryCategory::UnsupportedResponse;
        for option in envelope.result {
            if option.protected() {
                failure = DeliveryCategory::ProtectionRequired;
                continue;
            }
            if let Some(resource) = option.download_info_url {
                match self.validate_metadata_url(&Zeroizing::new(resource)) {
                    Ok(url) => return Ok(url),
                    Err(category) => failure = category,
                }
            }
        }
        Err(failure)
    }

    fn validate_metadata_url(&self, resource: &str) -> Result<Url, DeliveryCategory> {
        if resource.is_empty()
            || resource.len() > MAX_RESOURCE_URL
            || !resource.is_ascii()
            || resource
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
            || resource.contains('\\')
        {
            return Err(DeliveryCategory::UnsafeResource);
        }
        let authority = resource
            .split_once("://")
            .map(|(_, tail)| tail)
            .and_then(|tail| tail.split(['/', '?', '#']).next())
            .ok_or(DeliveryCategory::UnsafeResource)?;
        // Url normalizes empty userinfo and an empty explicit port away. The
        // raw forms still violate the exact-origin, no-userinfo contract.
        if authority.is_empty() || authority.contains('@') || authority.ends_with(':') {
            return Err(DeliveryCategory::UnsafeResource);
        }
        let url = Url::parse(resource).map_err(|_| DeliveryCategory::UnsafeResource)?;
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err(DeliveryCategory::UnsafeResource);
        }
        let permitted = url.scheme() == "https"
            && url.host_str() == Some("storage.mds.yandex.net")
            && url.port_or_known_default() == Some(443);
        #[cfg(test)]
        let permitted = permitted
            || self
                .metadata_fixture_origin
                .as_ref()
                .is_some_and(|fixture| {
                    url.scheme() == "http"
                        && url.host_str() == Some("127.0.0.1")
                        && url.origin() == fixture.origin()
                });
        if !permitted {
            return Err(DeliveryCategory::MetadataOriginUnverified);
        }
        // Retain /get-download-info/ from the previous policy. The additional
        // /download-info/ is published by lifeart 49cfee7002eb, and the exact
        // /file-download-info/ appears in MarshalX's fixture at 9ca132e5c18a.
        // This does not authorize arbitrary paths, redirects or subdomains.
        let path = url.path();
        let raw_path = resource
            .split_once("://")
            .and_then(|(_, authority_and_path)| authority_and_path.split_once('/'))
            .map(|(_, path)| path.split('?').next().unwrap_or_default())
            .unwrap_or_default();
        let suffix = path
            .strip_prefix("/get-download-info/")
            .or_else(|| path.strip_prefix("/download-info/"))
            .or_else(|| path.strip_prefix("/file-download-info/"));
        if suffix.is_none_or(str::is_empty)
            || path.contains(['%', '\\'])
            || path.split('/').any(|part| matches!(part, "." | ".."))
            || raw_path.contains('%')
            || raw_path.split('/').any(|part| matches!(part, "." | ".."))
        {
            return Err(DeliveryCategory::MetadataPathUnverified);
        }
        Ok(url)
    }

    fn probe_descriptor(
        &self,
        report: DeliveryReport,
        url: Url,
        token: &AccessToken,
        cancellation: &DeliveryCancellation,
    ) -> DeliveryReport {
        let (body, mut report) = match self.descriptor_body(report, url, token, cancellation) {
            Ok(response) => response,
            Err(report) => return report,
        };
        report.category = match xml::classify(&body) {
            xml::MetadataCategory::LegacySignatureRequired => {
                DeliveryCategory::LegacySignatureRequired
            }
            xml::MetadataCategory::ProtectionRequired => {
                report.protection_required = true;
                DeliveryCategory::ProtectionRequired
            }
            xml::MetadataCategory::UnsupportedResponse => DeliveryCategory::UnsupportedResponse,
            xml::MetadataCategory::PreviewOnly => DeliveryCategory::PreviewOnly,
        };
        report
    }

    fn descriptor_body(
        &self,
        mut report: DeliveryReport,
        url: Url,
        token: &AccessToken,
        cancellation: &DeliveryCancellation,
    ) -> Result<(Zeroizing<Vec<u8>>, DeliveryReport), DeliveryReport> {
        if cancellation.cancelled(token) {
            return Err(DeliveryReport::new(DeliveryCategory::Cancelled, None));
        }
        report.metadata_attempted = true;
        report.stage = DeliveryStage::Descriptor;
        // Authentication is attached per request, never to Client defaults.
        // This request intentionally has NO OAuth, Cookie, Origin or Referer.
        let response = self
            .client
            .get(url)
            .header(ACCEPT, "application/xml")
            .send();
        if cancellation.cancelled(token) {
            return Err(DeliveryReport::new(DeliveryCategory::Cancelled, None));
        }
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                report.category = if error.is_timeout() {
                    DeliveryCategory::Timeout
                } else {
                    DeliveryCategory::Network
                };
                return Err(report);
            }
        };
        let status = response.status().as_u16();
        report.metadata_http_status = Some(status);
        if status != 200 {
            report.category = match status {
                401 => DeliveryCategory::AccessDenied,
                403 => DeliveryCategory::Forbidden,
                410 => DeliveryCategory::SourceExpired,
                429 => DeliveryCategory::RateLimited,
                451 => DeliveryCategory::Restricted,
                _ => DeliveryCategory::HttpRejected,
            };
            return Err(report);
        }
        let outcome = read_body(response, token, cancellation, MAX_METADATA_BODY);
        if cancellation.cancelled(token) {
            return Err(DeliveryReport::new(DeliveryCategory::Cancelled, None));
        }
        match outcome {
            Ok(body) => Ok((body, report)),
            Err(category) => {
                report.category = category;
                Err(report)
            }
        }
    }
}

fn read_body(
    mut response: Response,
    token: &AccessToken,
    cancellation: &DeliveryCancellation,
    max_body: usize,
) -> Result<Zeroizing<Vec<u8>>, DeliveryCategory> {
    if response
        .content_length()
        .is_some_and(|length| length > max_body as u64)
    {
        return Err(DeliveryCategory::ResponseTooLarge);
    }
    let mut body = Zeroizing::new(Vec::new());
    let mut buffer = Zeroizing::new([0; 8192]);
    loop {
        if cancellation.cancelled(token) {
            return Err(DeliveryCategory::Cancelled);
        }
        let count = response.read(buffer.as_mut()).map_err(|error| {
            if error.kind() == std::io::ErrorKind::TimedOut
                || error
                    .get_ref()
                    .and_then(|source| source.downcast_ref::<reqwest::Error>())
                    .is_some_and(reqwest::Error::is_timeout)
            {
                DeliveryCategory::Timeout
            } else {
                DeliveryCategory::Network
            }
        })?;
        if count == 0 {
            return Ok(body);
        }
        if body.len() + count > max_body {
            return Err(DeliveryCategory::ResponseTooLarge);
        }
        body.extend_from_slice(&buffer[..count]);
    }
}

#[derive(Deserialize)]
struct Envelope {
    result: Vec<DeliveryOption>,
    #[serde(default, rename = "error", deserialize_with = "reject_delivery_error")]
    _error: (),
}

fn reject_delivery_error<'de, D: Deserializer<'de>>(_: D) -> Result<(), D::Error> {
    // Even a null error alongside a result is an ambiguous response. Do not
    // copy provider messages or allow a usable-looking result to hide it.
    Err(serde::de::Error::custom("error response"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeliveryOption {
    codec: Codec,
    download_info_url: Option<String>,
    #[serde(default)]
    direct: bool,
    preview: Option<bool>,
    bitrate_in_kbps: Option<u32>,
    #[serde(default)]
    transport: DeliveryTransport,
    // These values are skipped, not copied, displayed or used for decryption.
    key: Option<IgnoredAny>,
    #[serde(alias = "decryption_key")]
    decryption_key: Option<IgnoredAny>,
    #[serde(default)]
    drm: ProtectionMarker,
    #[serde(default)]
    encryption: ProtectionMarker,
    #[serde(default)]
    encrypted: ProtectionMarker,
    #[serde(default)]
    protected: ProtectionMarker,
    #[serde(default)]
    protection: ProtectionMarker,
    #[serde(default, alias = "license_url")]
    license_url: ProtectionMarker,
}

impl DeliveryOption {
    fn protected(&self) -> bool {
        self.key.is_some()
            || self.decryption_key.is_some()
            || self.drm.0
            || self.encryption.0
            || self.encrypted.0
            || self.protected.0
            || self.protection.0
            || self.license_url.0
            || self.transport == DeliveryTransport::Protected
    }
}

/// Legacy offers can omit transport; null is likewise an absent optional field.
/// The exact raw label is independently used by pinned yamu's transport model.
/// It does not override any protection declaration or change resource lookup.
/// encraw denotes protection; other values remain unsupported. Provider text
/// is classified without copying or exposing it.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum DeliveryTransport {
    #[default]
    Absent,
    Raw,
    Protected,
    Unsupported,
}

fn declared_transport(value: &str) -> DeliveryTransport {
    if value == "raw" {
        DeliveryTransport::Raw
    } else if value.len() <= 32 && value.trim().eq_ignore_ascii_case("encraw") {
        DeliveryTransport::Protected
    } else {
        DeliveryTransport::Unsupported
    }
}

impl<'de> Deserialize<'de> for DeliveryTransport {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = DeliveryTransport;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("transport declaration")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(declared_transport(value))
            }

            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(DeliveryTransport::Absent)
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

/// Skip protection metadata instead of copying keys or arbitrary provider text.
/// Explicit false/null/empty values are not a protection requirement.
#[derive(Default)]
struct ProtectionMarker(bool);

impl<'de> Deserialize<'de> for ProtectionMarker {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ProtectionMarker;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("protection metadata")
            }

            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(ProtectionMarker(value))
            }

            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(ProtectionMarker(false))
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(ProtectionMarker(!value.is_empty()))
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut present = false;
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {
                    present = true;
                }
                Ok(ProtectionMarker(present))
            }

            fn visit_seq<S: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: S,
            ) -> Result<Self::Value, S::Error> {
                let mut present = false;
                while sequence.next_element::<IgnoredAny>()?.is_some() {
                    present = true;
                }
                Ok(ProtectionMarker(present))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Codec {
    Mp3,
    Aac,
    Flac,
    #[serde(other)]
    Other,
}

fn classify(body: &[u8], status: u16) -> DeliveryReport {
    let mut report = DeliveryReport::new(DeliveryCategory::UnsupportedResponse, Some(status));
    let Ok(envelope) = serde_json::from_slice::<Envelope>(body) else {
        return report;
    };
    if envelope.result.len() > MAX_OPTIONS {
        return report;
    }
    if envelope.result.is_empty() {
        report.category = DeliveryCategory::NoDeliveryOptions;
        return report;
    }
    let mut descriptor = false;
    let mut protected = false;
    let mut protected_transport = false;
    let mut unsupported_transport = false;
    for option in envelope.result {
        match option.codec {
            Codec::Mp3 => report.formats.mp3 = true,
            Codec::Aac => report.formats.aac = true,
            Codec::Flac => report.formats.flac = true,
            Codec::Other => report.formats.other = true,
        }
        report.direct_hint |= option.direct;
        report.preview_hint |= option.preview.unwrap_or(false);
        protected_transport |= option.transport == DeliveryTransport::Protected;
        unsupported_transport |= option.transport == DeliveryTransport::Unsupported;
        if option.protected() {
            protected = true;
            report.protection_required = true;
            continue;
        }
        let Some(resource) = option.download_info_url.map(Zeroizing::new) else {
            continue;
        };
        if resource.len() > MAX_RESOURCE_URL {
            report.resource = DeliveryResource::Unsafe;
            continue;
        }
        let Ok(url) = Url::parse(&resource) else {
            report.resource = DeliveryResource::Unsafe;
            continue;
        };
        if url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || !matches!(url.scheme(), "https" | "http")
        {
            report.resource = DeliveryResource::Unsafe;
            continue;
        }
        descriptor = true;
        if url.scheme() == "http" {
            report.resource = DeliveryResource::InsecureDescriptor;
        } else if matches!(report.resource, DeliveryResource::None) {
            report.resource = DeliveryResource::HttpsDescriptor;
        }
    }
    report.category = if protected_transport {
        DeliveryCategory::ProtectionRequired
    } else if unsupported_transport {
        DeliveryCategory::UnsupportedResponse
    } else if descriptor {
        DeliveryCategory::LegacyNegotiationRequired
    } else if protected {
        DeliveryCategory::ProtectionRequired
    } else if report.resource == DeliveryResource::Unsafe {
        DeliveryCategory::UnsafeResource
    } else {
        DeliveryCategory::UnsupportedResponse
    };
    report
}

fn forbidden_category(body: &[u8]) -> DeliveryCategory {
    // Reuse the exact reviewed whitelist; no free-form message/body inference.
    match crate::music::forbidden_error(body) {
        crate::music::MusicError::MissingScopes => DeliveryCategory::MissingScopes,
        _ => DeliveryCategory::Forbidden,
    }
}

#[cfg(test)]
mod tests;

mod xml;

mod resolver;
pub use resolver::StreamResolver;
