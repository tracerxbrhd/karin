// SPDX-License-Identifier: GPL-3.0-or-later
//! Anonymous release metadata and the expected signed Flatpak deployment.

use std::{
    cmp::Ordering,
    io::Read,
    sync::atomic::{AtomicBool, Ordering as AtomicOrdering},
    time::Duration,
};

use reqwest::{StatusCode, blocking::Client, header, redirect};
use semver::Version;
use serde::{Deserialize, de::DeserializeOwned};

use crate::config;

const API: &str = "https://api.github.com/repos/tracerxbrhd/karin";
const RELEASE_PAGE: &str = "https://github.com/tracerxbrhd/karin/releases/tag/";
const UPDATE_ASSET: &str = "KARIN-FLATPAK-UPDATE.json";
const MAX_RELEASE_BYTES: usize = 512 * 1024;
const MAX_DEPLOYMENT_BYTES: usize = 4096;
pub(crate) const KEY_PIN: &str = include_str!("../../packaging/release/flatpak-key-fingerprint");

pub(crate) fn channel_configured() -> bool {
    valid_key_pin(KEY_PIN.trim())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Deployment {
    pub version: Version,
    pub source_commit: String,
    pub ostree_commit: String,
    pub app_ref: String,
    pub key_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DeploymentStatus {
    Native,
    ChannelNotConfigured,
    NotPublished,
    Unverified(CheckError),
    Verified(Deployment),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Release {
    pub version: Version,
    pub notes: String,
    pub page: String,
    pub deployment: DeploymentStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CheckResult {
    UpToDate,
    Available(Box<Release>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckError {
    Cancelled,
    Network,
    Timeout,
    RateLimited,
    Http(u16),
    InvalidData,
    Internal,
}

#[derive(Deserialize)]
struct Latest {
    tag_name: String,
    published_at: Option<String>,
    draft: bool,
    prerelease: bool,
    body: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    id: u64,
    size: u64,
    state: String,
}

#[derive(Deserialize)]
struct GitObject {
    #[serde(rename = "type")]
    kind: String,
    sha: String,
}

#[derive(Deserialize)]
struct GitRef {
    object: GitObject,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishedDeployment {
    schema: u8,
    app_id: String,
    #[serde(rename = "ref")]
    app_ref: String,
    version: String,
    source_commit: String,
    ostree_commit: String,
    key_fingerprint: String,
}

/// Blocking operation with finite request/response bounds. The GTK coordinator
/// cancels admission immediately on close. An in-flight socket may live until
/// its request deadline, so a second check is not started until this returns.
pub(crate) fn check(cancelled: &AtomicBool, flatpak: bool) -> Result<CheckResult, CheckError> {
    let current = Version::parse(config::VERSION).map_err(|_| CheckError::Internal)?;
    let client = Client::builder()
        .user_agent(concat!("Karin/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .redirect(redirect::Policy::none())
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|_| CheckError::Internal)?;
    let latest: Latest = fetch_json(&client, &format!("{API}/releases/latest"), cancelled)?;
    let Some(version) = newer_stable(&latest, &current)? else {
        return Ok(CheckResult::UpToDate);
    };
    let deployment = if flatpak {
        get_deployment(&client, &latest, &version, KEY_PIN.trim(), cancelled)
    } else {
        DeploymentStatus::Native
    };
    check_cancelled(cancelled)?;
    Ok(CheckResult::Available(Box::new(Release {
        version,
        notes: plain_notes(latest.body.as_deref().unwrap_or_default()),
        page: format!("{RELEASE_PAGE}{}", latest.tag_name),
        deployment,
    })))
}

fn newer_stable(latest: &Latest, current: &Version) -> Result<Option<Version>, CheckError> {
    if latest.draft || latest.prerelease || latest.published_at.is_none() {
        return Ok(None);
    }
    // Strict SemVer keeps the tag suitable for both comparison and use as one
    // path segment. Limit its size before constructing any network request.
    if latest.tag_name.len() > 72 {
        return Err(CheckError::InvalidData);
    }
    let raw = latest
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&latest.tag_name);
    let version = Version::parse(raw).map_err(|_| CheckError::InvalidData)?;
    if version.to_string() != raw {
        return Err(CheckError::InvalidData);
    }
    Ok(
        (version.pre.is_empty() && version.cmp_precedence(current) == Ordering::Greater)
            .then_some(version),
    )
}

fn plain_notes(notes: &str) -> String {
    let filtered = notes.chars().filter(|ch| {
        (matches!(*ch, '\n' | '\t') || !ch.is_control())
            && !matches!(*ch, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    });
    let mut filtered = filtered.peekable();
    let mut text: String = filtered.by_ref().take(1600).collect();
    if filtered.peek().is_some() {
        text.push('…');
    }
    text.trim().to_owned()
}

fn get_deployment(
    client: &Client,
    latest: &Latest,
    version: &Version,
    key_pin: &str,
    cancelled: &AtomicBool,
) -> DeploymentStatus {
    if key_pin.is_empty() {
        return DeploymentStatus::ChannelNotConfigured;
    }
    if !valid_key_pin(key_pin) {
        return DeploymentStatus::Unverified(CheckError::Internal);
    }
    let mut matches = latest
        .assets
        .iter()
        .filter(|asset| asset.name == UPDATE_ASSET);
    match (matches.next(), matches.next()) {
        (None, _) => DeploymentStatus::NotPublished,
        (Some(asset), None)
            if asset.state == "uploaded"
                && asset.id > 0
                && (1..=MAX_DEPLOYMENT_BYTES as u64).contains(&asset.size) =>
        {
            match fetch_deployment(client, &latest.tag_name, asset, version, key_pin, cancelled) {
                Ok(deployment) => DeploymentStatus::Verified(deployment),
                Err(error) => DeploymentStatus::Unverified(error),
            }
        }
        _ => DeploymentStatus::Unverified(CheckError::InvalidData),
    }
}

fn fetch_deployment(
    client: &Client,
    tag: &str,
    asset: &Asset,
    version: &Version,
    key_pin: &str,
    cancelled: &AtomicBool,
) -> Result<Deployment, CheckError> {
    let bytes = fetch_asset(client, asset, cancelled)?;
    let published: PublishedDeployment =
        serde_json::from_slice(&bytes).map_err(|_| CheckError::InvalidData)?;
    let commit = fetch_tag_commit(client, tag, cancelled)?;
    verify_deployment(published, version, &commit, key_pin)
}

fn valid_key_pin(pin: &str) -> bool {
    pin.len() == 40
        && pin
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(&c))
}

fn valid_sha(sha: &str, size: usize) -> bool {
    sha.len() == size
        && sha
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn verify_deployment(
    published: PublishedDeployment,
    version: &Version,
    source_commit: &str,
    key_pin: &str,
) -> Result<Deployment, CheckError> {
    if published.schema != 1
        || published.app_id != config::APP_ID
        || published.app_ref != format!("app/{}/{}/master", config::APP_ID, std::env::consts::ARCH)
        || published.version != version.to_string()
        || !valid_sha(&published.source_commit, 40)
        || published.source_commit != source_commit
        || !valid_sha(&published.ostree_commit, 64)
        || !valid_key_pin(key_pin)
        || published.key_fingerprint != key_pin
    {
        return Err(CheckError::InvalidData);
    }
    Ok(Deployment {
        version: version.clone(),
        source_commit: published.source_commit,
        ostree_commit: published.ostree_commit,
        app_ref: published.app_ref,
        key_fingerprint: published.key_fingerprint,
    })
}

fn fetch_tag_commit(
    client: &Client,
    tag: &str,
    cancelled: &AtomicBool,
) -> Result<String, CheckError> {
    let raw: GitRef = fetch_json(client, &format!("{API}/git/ref/tags/{tag}"), cancelled)?;
    let mut object = raw.object;
    // Normal releases use a lightweight or a single annotated tag. Do not
    // recursively follow references supplied by a remote response.
    if object.kind == "tag" && valid_sha(&object.sha, 40) {
        object =
            fetch_json::<GitRef>(client, &format!("{API}/git/tags/{}", object.sha), cancelled)?
                .object;
    }
    if object.kind != "commit" || !valid_sha(&object.sha, 40) {
        return Err(CheckError::InvalidData);
    }
    Ok(object.sha)
}

fn fetch_json<T: DeserializeOwned>(
    client: &Client,
    url: &str,
    cancelled: &AtomicBool,
) -> Result<T, CheckError> {
    let response = request(client, url, "application/vnd.github+json", cancelled)?;
    let bytes = read_response(response, MAX_RELEASE_BYTES, cancelled)?;
    serde_json::from_slice(&bytes).map_err(|_| CheckError::InvalidData)
}

fn fetch_asset(
    client: &Client,
    asset: &Asset,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, CheckError> {
    let response = request(
        client,
        &format!("{API}/releases/assets/{}", asset.id),
        "application/octet-stream",
        cancelled,
    )?;
    let response = if response.status().is_redirection() {
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|location| location.to_str().ok())
            .and_then(|location| reqwest::Url::parse(location).ok())
            .filter(allowed_asset_redirect)
            .ok_or(CheckError::InvalidData)?;
        request(
            client,
            location.as_str(),
            "application/octet-stream",
            cancelled,
        )?
    } else {
        response
    };
    let bytes = read_response(response, MAX_DEPLOYMENT_BYTES, cancelled)?;
    if bytes.len() as u64 != asset.size {
        return Err(CheckError::InvalidData);
    }
    Ok(bytes)
}

fn allowed_asset_redirect(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && matches!(
            url.host_str(),
            Some("release-assets.githubusercontent.com" | "objects.githubusercontent.com")
        )
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
}

fn request(
    client: &Client,
    url: &str,
    accept: &str,
    cancelled: &AtomicBool,
) -> Result<reqwest::blocking::Response, CheckError> {
    check_cancelled(cancelled)?;
    let response = client
        .get(url)
        .header(header::ACCEPT, accept)
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .map_err(|error| {
            if error.is_timeout() {
                CheckError::Timeout
            } else {
                CheckError::Network
            }
        });
    check_cancelled(cancelled)?;
    response
}

fn read_response(
    response: reqwest::blocking::Response,
    max_len: usize,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, CheckError> {
    let status = response.status();
    let headers = response.headers();
    if status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN
            && (headers
                .get("X-RateLimit-Remaining")
                .is_some_and(|left| left == "0")
                || headers.contains_key(header::RETRY_AFTER)))
    {
        return Err(CheckError::RateLimited);
    }
    if status != StatusCode::OK {
        return Err(CheckError::Http(status.as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|len| len > max_len as u64)
    {
        return Err(CheckError::InvalidData);
    }
    let mut bytes = Vec::new();
    response
        .take(max_len as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::TimedOut {
                CheckError::Timeout
            } else {
                CheckError::Network
            }
        })?;
    check_cancelled(cancelled)?;
    if bytes.len() > max_len {
        return Err(CheckError::InvalidData);
    }
    Ok(bytes)
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), CheckError> {
    if cancelled.load(AtomicOrdering::Relaxed) {
        Err(CheckError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Write, net::TcpListener, thread};

    use super::*;

    fn latest(tag: &str) -> Latest {
        Latest {
            tag_name: tag.into(),
            published_at: Some("2026-11-24T10:00:00Z".into()),
            draft: false,
            prerelease: false,
            body: None,
            assets: Vec::new(),
        }
    }

    #[test]
    fn stable_version_precedence_is_numeric_and_never_downgrades() {
        let current = Version::new(0, 8, 9);
        assert_eq!(
            newer_stable(&latest("v0.8.10"), &current),
            Ok(Some(Version::new(0, 8, 10)))
        );
        assert_eq!(
            newer_stable(&latest("0.8.10"), &current),
            Ok(Some(Version::new(0, 8, 10)))
        );
        for tag in ["v0.8.9", "v0.8.4", "v0.8.10-rc.2", "v0.8.9+metadata"] {
            assert_eq!(newer_stable(&latest(tag), &current), Ok(None));
        }
        for tag in ["vv0.8.10", "v0.8.10/other", "v0.8.010", "v0.8"] {
            assert_eq!(
                newer_stable(&latest(tag), &current),
                Err(CheckError::InvalidData)
            );
        }
    }

    #[test]
    fn unpublished_or_prerelease_records_are_ignored_even_with_a_stable_tag() {
        let current = Version::new(0, 8, 1);
        let mut draft = latest("v0.8.2");
        draft.draft = true;
        let mut pre = latest("v0.8.3");
        pre.prerelease = true;
        let mut unpublished = latest("v0.8.4");
        unpublished.published_at = None;
        for release in [&draft, &pre, &unpublished] {
            assert_eq!(newer_stable(release, &current), Ok(None));
        }
    }

    fn deployment() -> PublishedDeployment {
        PublishedDeployment {
            schema: 1,
            app_id: config::APP_ID.into(),
            app_ref: format!("app/{}/{}/master", config::APP_ID, std::env::consts::ARCH),
            version: "0.8.2".into(),
            source_commit: "a".repeat(40),
            ostree_commit: "b".repeat(64),
            key_fingerprint: "C".repeat(40),
        }
    }

    #[test]
    fn deployment_must_match_release_tag_verified_commit_ref_and_pinned_key() {
        let version = Version::new(0, 8, 2);
        let commit = "a".repeat(40);
        let pin = "C".repeat(40);
        assert!(verify_deployment(deployment(), &version, &commit, &pin).is_ok());
        assert_eq!(
            verify_deployment(deployment(), &Version::new(0, 8, 3), &commit, &pin),
            Err(CheckError::InvalidData)
        );
        assert_eq!(
            verify_deployment(deployment(), &version, &"d".repeat(40), &pin),
            Err(CheckError::InvalidData)
        );
        assert_eq!(
            verify_deployment(deployment(), &version, &commit, &"A".repeat(40)),
            Err(CheckError::InvalidData)
        );
        assert_eq!(
            verify_deployment(deployment(), &version, &commit, ""),
            Err(CheckError::InvalidData)
        );
        let mut wrong_ref = deployment();
        wrong_ref.app_ref = format!("app/{}/x86_64/beta", config::APP_ID);
        assert_eq!(
            verify_deployment(wrong_ref, &version, &commit, &pin),
            Err(CheckError::InvalidData)
        );
        let mut wrong_ostree = deployment();
        wrong_ostree.ostree_commit = "B".repeat(64);
        assert_eq!(
            verify_deployment(wrong_ostree, &version, &commit, &pin),
            Err(CheckError::InvalidData)
        );
    }

    #[test]
    fn release_notes_remain_literal_bounded_text_without_controls() {
        assert_eq!(
            plain_notes("  <b>буквально</b>\n\u{202e}abc\u{0000}  "),
            "<b>буквально</b>\nabc"
        );
        let text = plain_notes(&"Я".repeat(1601));
        assert_eq!(text.chars().count(), 1601);
        assert!(text.ends_with('…'));
    }

    #[test]
    fn asset_redirect_is_limited_to_github_https_hosts() {
        for url in [
            "http://release-assets.githubusercontent.com/file",
            "https://release-assets.githubusercontent.com.evil.net/file",
            "https://x@release-assets.githubusercontent.com/file",
            "https://release-assets.githubusercontent.com:444/file",
            "https://api.github.com/file",
        ] {
            assert!(!allowed_asset_redirect(&reqwest::Url::parse(url).unwrap()));
        }
        assert!(allowed_asset_redirect(
            &reqwest::Url::parse("https://release-assets.githubusercontent.com/file?expiry=1")
                .unwrap()
        ));
    }

    fn http_fixture(
        response: impl Into<Vec<u8>>,
        delay: Duration,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/latest", listener.local_addr().unwrap());
        let response = response.into();
        let join = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") && request.len() < 4096 {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            assert!(request.starts_with("get /latest http/1.1\r\n"));
            assert!(!request.contains("\r\nauthorization:"));
            assert!(!request.contains("\r\ncookie:"));
            thread::sleep(delay);
            let _ = stream.write_all(&response);
        });
        (endpoint, join)
    }

    #[test]
    fn http_errors_corrupt_json_and_oversize_responses_are_typed() {
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let cancelled = AtomicBool::new(false);
        for (headers, body, expected) in [
            (
                "403 Forbidden\r\nX-RateLimit-Remaining: 0\r\n",
                "",
                CheckError::RateLimited,
            ),
            ("403 Forbidden\r\n", "", CheckError::Http(403)),
            ("404 Not Found\r\n", "", CheckError::Http(404)),
            ("429 Too Many Requests\r\n", "", CheckError::RateLimited),
            ("200 OK\r\n", "not json", CheckError::InvalidData),
        ] {
            let (url, server) = http_fixture(
                format!(
                    "HTTP/1.1 {headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                ),
                Duration::ZERO,
            );
            assert_eq!(
                fetch_json::<serde_json::Value>(&client, &url, &cancelled),
                Err(expected)
            );
            server.join().unwrap();
        }
        let (url, server) = http_fixture(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\n12345\r\n0\r\n\r\n",
            Duration::ZERO,
        );
        let response = request(&client, &url, "application/json", &cancelled).unwrap();
        assert_eq!(
            read_response(response, 4, &cancelled),
            Err(CheckError::InvalidData)
        );
        server.join().unwrap();
    }

    #[test]
    fn timeout_and_cancelled_admission_do_not_produce_an_offer() {
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(50))
            .build()
            .unwrap();
        let (url, server) = http_fixture(
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}",
            Duration::from_millis(250),
        );
        assert_eq!(
            fetch_json::<serde_json::Value>(&client, &url, &AtomicBool::new(false)),
            Err(CheckError::Timeout)
        );
        server.join().unwrap();
        assert_eq!(
            fetch_json::<serde_json::Value>(&client, &url, &AtomicBool::new(true)),
            Err(CheckError::Cancelled)
        );
    }
}
