// SPDX-License-Identifier: GPL-3.0-or-later
//! Fresh, account-bound legacy MP3 resolution. This module never fetches media.

use std::{fmt, time::Instant};

use gtk::glib::{Checksum, ChecksumType};

use super::*;
use crate::playback::{RemotePlaybackSource, permitted_media_host};

// The historical public legacy protocol uses this fixed checksum prefix, not
// the extracted HMAC/decryption material of modern official-client flows.
// Compared independently against Cassette v0.2.4 9d34251d and the published
// 2026-07-16 lifeart investigation at 49cfee7002ebf816f4144860e67d6fa3be5f5708.
const LEGACY_CHECKSUM_PREFIX: &[u8] = b"XGRlBW9FXlekgbPrRHuSiA";
// Local admission freshness, deliberately conservative. It does not interpret
// XML `ts` or claim an authoritative server expiry / renew playback rights.
const SOURCE_FRESHNESS: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct StreamResolver {
    transport: DeliveryProbe,
    #[cfg(test)]
    media_fixture_origin: Option<Url>,
}

impl fmt::Debug for StreamResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StreamResolver([redacted])")
    }
}

impl StreamResolver {
    pub fn new() -> Result<Self, DeliveryReport> {
        Ok(Self {
            transport: DeliveryProbe::new()?,
            #[cfg(test)]
            media_fixture_origin: None,
        })
    }

    /// Resolve only after an explicit user playback action. Existing Music
    /// validation is token-bound; no credential or caller-supplied URL enters
    /// the descriptor or media requests. Run on a bounded worker, never GTK.
    pub fn resolve(
        &self,
        service: &MusicService,
        token: &AccessToken,
        track_id: &str,
        cancellation: &DeliveryCancellation,
    ) -> Result<RemotePlaybackSource, DeliveryReport> {
        // Start before requesting offers: a slow descriptor cannot reset the
        // lifetime documented by legacy clients as about one minute per offer.
        let expires_at = Instant::now() + SOURCE_FRESHNESS;
        self.resolve_until(service, token, track_id, cancellation, expires_at)
    }

    fn resolve_until(
        &self,
        service: &MusicService,
        token: &AccessToken,
        track_id: &str,
        cancellation: &DeliveryCancellation,
        expires_at: Instant,
    ) -> Result<RemotePlaybackSource, DeliveryReport> {
        if Instant::now() >= expires_at {
            return Err(DeliveryReport {
                stage: DeliveryStage::Resolution,
                ..DeliveryReport::new(DeliveryCategory::SourceExpired, None)
            });
        }
        let (body, status) =
            self.transport
                .delivery_info(service, token, track_id, cancellation)?;
        let mut report = classify(&body, status);
        let envelope = serde_json::from_slice::<Envelope>(&body).map_err(|_| report)?;
        if envelope.result.len() > MAX_OPTIONS {
            report.category = DeliveryCategory::ResponseTooLarge;
            return Err(report);
        }
        if envelope.result.is_empty() {
            report.category = DeliveryCategory::NoDeliveryOptions;
            return Err(report);
        }
        let url = self
            .select_mp3(envelope.result)
            .map_err(|category| DeliveryReport { category, ..report })?;
        if Instant::now() >= expires_at {
            report.stage = DeliveryStage::Resolution;
            report.category = DeliveryCategory::SourceExpired;
            return Err(report);
        }
        let (descriptor, mut report) =
            self.transport
                .descriptor_body(report, url, token, cancellation)?;
        report.stage = DeliveryStage::Resolution;
        let parts = xml::parse(&descriptor).map_err(|category| {
            report.category = match category {
                xml::MetadataCategory::ProtectionRequired => {
                    report.protection_required = true;
                    DeliveryCategory::ProtectionRequired
                }
                xml::MetadataCategory::PreviewOnly => DeliveryCategory::PreviewOnly,
                xml::MetadataCategory::UnsupportedResponse => DeliveryCategory::UnsupportedResponse,
                xml::MetadataCategory::LegacySignatureRequired => {
                    DeliveryCategory::UnsupportedResponse
                }
            };
            report
        })?;
        if cancellation.cancelled(token) {
            report.category = DeliveryCategory::Cancelled;
            return Err(report);
        }
        if service.validate_session(token).is_err() {
            report.category = DeliveryCategory::SessionRequired;
            return Err(report);
        }
        if Instant::now() >= expires_at {
            report.category = DeliveryCategory::SourceExpired;
            return Err(report);
        }
        let uri = legacy_uri(&parts).map_err(|category| DeliveryReport { category, ..report })?;
        #[cfg(test)]
        if let Some(origin) = &self.media_fixture_origin {
            let parsed = Url::parse(&uri).map_err(|_| DeliveryReport {
                category: DeliveryCategory::UnsafeResource,
                ..report
            })?;
            let resource = Zeroizing::new(format!(
                "{}{}",
                origin.as_str().trim_end_matches('/'),
                parsed.path()
            ));
            return RemotePlaybackSource::from_test_resolver(&resource, expires_at).map_err(|_| {
                DeliveryReport {
                    category: DeliveryCategory::SourceExpired,
                    ..report
                }
            });
        }
        RemotePlaybackSource::from_resolver(&uri, expires_at).map_err(|_| DeliveryReport {
            category: if Instant::now() >= expires_at {
                DeliveryCategory::SourceExpired
            } else {
                DeliveryCategory::UnsafeResource
            },
            ..report
        })
    }

    fn select_mp3(&self, options: Vec<DeliveryOption>) -> Result<Url, DeliveryCategory> {
        // A declared transport changes the protocol contract. Do not silently
        // ignore protected/unknown declarations by selecting another entry.
        if options
            .iter()
            .any(|option| option.transport == DeliveryTransport::Protected)
        {
            return Err(DeliveryCategory::ProtectionRequired);
        }
        if options
            .iter()
            .any(|option| option.transport == DeliveryTransport::Unsupported)
        {
            return Err(DeliveryCategory::UnsupportedResponse);
        }
        let mp3 = options
            .iter()
            .filter(|option| option.codec == Codec::Mp3)
            .collect::<Vec<_>>();
        if mp3.is_empty() {
            return Err(DeliveryCategory::UnsupportedCodec);
        }
        // Conflicting protection declarations are never resolved by selecting
        // the less restrictive MP3 entry or interpreting an opaque key.
        if mp3.iter().any(|option| option.protected()) {
            return Err(DeliveryCategory::ProtectionRequired);
        }
        for (index, option) in mp3.iter().enumerate() {
            if option.download_info_url.is_some()
                && mp3.iter().skip(index + 1).any(|other| {
                    option.download_info_url == other.download_info_url
                        && option.preview != other.preview
                })
            {
                return Err(DeliveryCategory::UnsupportedResponse);
            }
        }
        let mut selected = None;
        let mut failure = if mp3.iter().any(|option| option.preview.is_none()) {
            DeliveryCategory::PreviewUnverified
        } else {
            DeliveryCategory::PreviewOnly
        };
        for option in options {
            if option.codec != Codec::Mp3 || option.preview != Some(false) {
                continue;
            }
            let Some(resource) = option.download_info_url else {
                failure = DeliveryCategory::UnsupportedResponse;
                continue;
            };
            let url = match self
                .transport
                .validate_metadata_url(&Zeroizing::new(resource))
            {
                Ok(url) => url,
                Err(category) => {
                    failure = category;
                    continue;
                }
            };
            let bitrate = option.bitrate_in_kbps.unwrap_or(0);
            if selected
                .as_ref()
                .is_none_or(|(selected_bitrate, _)| bitrate > *selected_bitrate)
            {
                selected = Some((bitrate, url));
            }
        }
        selected.map(|(_, url)| url).ok_or(failure)
    }

    #[cfg(test)]
    pub(crate) fn for_test(api_origin: &str, metadata_origin: &str, media_origin: &str) -> Self {
        let fixture = |value: &str| {
            let url = Url::parse(value).expect("synthetic loopback origin");
            assert_eq!(url.scheme(), "http");
            assert_eq!(url.host_str(), Some("127.0.0.1"));
            assert!(url.port().is_some_and(|port| port != 0));
            assert!(url.username().is_empty() && url.password().is_none());
            assert!(url.query().is_none() && url.fragment().is_none());
            assert_eq!(url.path(), "/");
            url
        };
        Self {
            transport: DeliveryProbe {
                client: Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(2))
                    .redirect(Policy::none())
                    .retry(reqwest::retry::never())
                    .build()
                    .expect("synthetic descriptor client"),
                origin: fixture(api_origin),
                metadata_fixture_origin: Some(fixture(metadata_origin)),
            },
            media_fixture_origin: Some(fixture(media_origin)),
        }
    }
}

fn legacy_uri(parts: &xml::LegacyParts) -> Result<Zeroizing<String>, DeliveryCategory> {
    let host = Zeroizing::new(parts.host.to_ascii_lowercase());
    if !permitted_media_host(&host) {
        return Err(DeliveryCategory::MediaOriginUnverified);
    }
    // Preserve the exact opaque path for the checksum while rejecting URI
    // delimiters, encoded traversal and control characters before construction.
    if !parts.path.is_ascii()
        || !parts.path.starts_with('/')
        || parts.path.starts_with("//")
        || parts
            .path
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || b"/._-+=~".contains(&byte)))
        || parts.path.split('/').any(|part| matches!(part, "." | ".."))
        || !parts.server_input.is_ascii()
        || parts
            .server_input
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return Err(DeliveryCategory::UnsafeResource);
    }
    let mut checksum = Checksum::new(ChecksumType::Md5).ok_or(DeliveryCategory::Configuration)?;
    checksum.update(LEGACY_CHECKSUM_PREFIX);
    checksum.update(&parts.path.as_bytes()[1..]);
    checksum.update(parts.server_input.as_bytes());
    let checksum = Zeroizing::new(checksum.string().ok_or(DeliveryCategory::Configuration)?);
    let uri = Zeroizing::new(format!(
        "https://{}/get-mp3/{}/{}{}",
        host.as_str(),
        checksum.as_str(),
        parts.timestamp.as_str(),
        parts.path.as_str()
    ));
    if uri.len() > MAX_RESOURCE_URL {
        return Err(DeliveryCategory::UnsafeResource);
    }
    Ok(uri)
}

#[cfg(test)]
mod tests;
