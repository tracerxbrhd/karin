// SPDX-License-Identifier: GPL-3.0-or-later
//! Anonymous release metadata and the expected signed Flatpak deployment.

use std::{
    cmp::Ordering,
    io::Read,
    sync::atomic::{AtomicBool, Ordering as AtomicOrdering},
    time::Duration,
};

use reqwest::{
    StatusCode,
    blocking::{Client, RequestBuilder},
    header, redirect,
};
use ring::digest::{SHA256, digest};
use semver::Version;
use serde::{Deserialize, de::DeserializeOwned};

use crate::config;

struct Endpoints<'a> {
    github: &'a str,
    flatpak_record: &'a str,
}

const ENDPOINTS: Endpoints<'static> = Endpoints {
    github: "https://api.github.com/repos/tracerxbrhd/karin",
    flatpak_record: "https://tracerxbrhd.github.io/karin/flatpak/KARIN-FLATPAK-UPDATE.json",
};
const RELEASE_PAGE: &str = "https://github.com/tracerxbrhd/karin/releases/tag/";
const RECORD_DIGEST_LABEL: &str = "karin-flatpak-update-sha256";
const RECORD_DIGEST_PREFIX: &str = "<!-- karin-flatpak-update-sha256: ";
const RECORD_DIGEST_SUFFIX: &str = " -->";
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
    check_release(&client, &ENDPOINTS, &current, flatpak, cancelled)
}

fn check_release(
    client: &Client,
    endpoints: &Endpoints<'_>,
    current: &Version,
    flatpak: bool,
    cancelled: &AtomicBool,
) -> Result<CheckResult, CheckError> {
    let latest: Latest = github_json(
        client,
        &format!("{}/releases/latest", endpoints.github),
        cancelled,
    )?;
    let Some(version) = newer_stable(&latest, current)? else {
        check_cancelled(cancelled)?;
        return Ok(CheckResult::UpToDate);
    };
    let notes = latest.body.as_deref().unwrap_or_default();
    let deployment = if flatpak {
        get_deployment(
            client,
            endpoints,
            &latest.tag_name,
            &version,
            notes,
            cancelled,
        )
    } else {
        DeploymentStatus::Native
    };
    check_cancelled(cancelled)?;
    Ok(CheckResult::Available(Box::new(Release {
        version,
        notes: plain_notes(notes),
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
    let content = notes
        .split_inclusive('\n')
        .filter(|line| !has_record_digest_label(line))
        .flat_map(str::chars);
    let filtered = content.filter(|ch| {
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

fn has_record_digest_label(line: &str) -> bool {
    line.to_ascii_lowercase().contains(RECORD_DIGEST_LABEL)
}

fn record_digest(notes: &str) -> Result<&str, CheckError> {
    let mut lines = notes.lines().filter(|line| has_record_digest_label(line));
    let line = lines.next().ok_or(CheckError::InvalidData)?;
    if lines.next().is_some() {
        return Err(CheckError::InvalidData);
    }
    line.strip_prefix(RECORD_DIGEST_PREFIX)
        .and_then(|value| value.strip_suffix(RECORD_DIGEST_SUFFIX))
        .filter(|value| valid_sha(value, 64))
        .ok_or(CheckError::InvalidData)
}

fn record_sha256(bytes: &[u8]) -> String {
    digest(&SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn get_deployment(
    client: &Client,
    endpoints: &Endpoints<'_>,
    tag: &str,
    version: &Version,
    notes: &str,
    cancelled: &AtomicBool,
) -> DeploymentStatus {
    let key_pin = KEY_PIN.trim();
    if key_pin.is_empty() {
        return DeploymentStatus::ChannelNotConfigured;
    }
    if !valid_key_pin(key_pin) {
        return DeploymentStatus::Unverified(CheckError::Internal);
    }
    let expected_digest = match record_digest(notes) {
        Ok(value) => value,
        Err(error) => return DeploymentStatus::Unverified(error),
    };
    match fetch_deployment(
        client,
        endpoints,
        tag,
        version,
        expected_digest,
        key_pin,
        cancelled,
    ) {
        Ok(None) => DeploymentStatus::NotPublished,
        Ok(Some(deployment)) => DeploymentStatus::Verified(deployment),
        Err(error) => DeploymentStatus::Unverified(error),
    }
}

fn fetch_deployment(
    client: &Client,
    endpoints: &Endpoints<'_>,
    tag: &str,
    version: &Version,
    expected_digest: &str,
    key_pin: &str,
    cancelled: &AtomicBool,
) -> Result<Option<Deployment>, CheckError> {
    let response = request(
        client
            .get(endpoints.flatpak_record)
            .header(header::ACCEPT, "application/json"),
        cancelled,
    )?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let bytes = read_response(response, MAX_DEPLOYMENT_BYTES, cancelled)?;
    // The two services publish separately. Gate on the exact record bound
    // to the stable release before considering any data from Pages.
    if record_sha256(&bytes) != expected_digest {
        return Ok(None);
    }
    let published: PublishedDeployment =
        serde_json::from_slice(&bytes).map_err(|_| CheckError::InvalidData)?;
    let deployment = validate_deployment(published, key_pin)?;
    if deployment.version != *version {
        return Err(CheckError::InvalidData);
    }
    let commit = fetch_tag_commit(client, endpoints.github, tag, cancelled)?;
    if deployment.source_commit != commit {
        return Err(CheckError::InvalidData);
    }
    // This record identifies the expected deployment. Flatpak is still
    // responsible for signature verification and reporting the exact commit.
    Ok(Some(deployment))
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

fn validate_deployment(
    published: PublishedDeployment,
    key_pin: &str,
) -> Result<Deployment, CheckError> {
    if published.version.len() > 72 {
        return Err(CheckError::InvalidData);
    }
    let version = Version::parse(&published.version).map_err(|_| CheckError::InvalidData)?;
    if published.schema != 1
        || published.app_id != config::APP_ID
        || published.app_ref != format!("app/{}/{}/master", config::APP_ID, std::env::consts::ARCH)
        || !version.pre.is_empty()
        || version.to_string() != published.version
        || !valid_sha(&published.source_commit, 40)
        || !valid_sha(&published.ostree_commit, 64)
        || !valid_key_pin(key_pin)
        || published.key_fingerprint != key_pin
    {
        return Err(CheckError::InvalidData);
    }
    Ok(Deployment {
        version,
        source_commit: published.source_commit,
        ostree_commit: published.ostree_commit,
        app_ref: published.app_ref,
        key_fingerprint: published.key_fingerprint,
    })
}

fn fetch_tag_commit(
    client: &Client,
    github: &str,
    tag: &str,
    cancelled: &AtomicBool,
) -> Result<String, CheckError> {
    let raw: GitRef = github_json(client, &format!("{github}/git/ref/tags/{tag}"), cancelled)?;
    let mut object = raw.object;
    // Normal releases use a lightweight or a single annotated tag. Do not
    // recursively follow references supplied by a remote response.
    if object.kind == "tag" && valid_sha(&object.sha, 40) {
        object = github_json::<GitRef>(
            client,
            &format!("{github}/git/tags/{}", object.sha),
            cancelled,
        )?
        .object;
    }
    if object.kind != "commit" || !valid_sha(&object.sha, 40) {
        return Err(CheckError::InvalidData);
    }
    Ok(object.sha)
}

fn github_json<T: DeserializeOwned>(
    client: &Client,
    url: &str,
    cancelled: &AtomicBool,
) -> Result<T, CheckError> {
    let response = request(
        client
            .get(url)
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28"),
        cancelled,
    )?;
    let bytes = read_response(response, MAX_RELEASE_BYTES, cancelled)?;
    serde_json::from_slice(&bytes).map_err(|_| CheckError::InvalidData)
}

fn request(
    request: RequestBuilder,
    cancelled: &AtomicBool,
) -> Result<reqwest::blocking::Response, CheckError> {
    check_cancelled(cancelled)?;
    let response = request.send().map_err(|error| {
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

    use serde_json::json;

    use super::*;

    fn latest(tag: &str) -> Latest {
        Latest {
            tag_name: tag.into(),
            published_at: Some("2026-11-24T10:00:00Z".into()),
            draft: false,
            prerelease: false,
            body: None,
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
    fn deployment_record_requires_expected_ref_and_pinned_key() {
        let pin = "C".repeat(40);
        assert!(validate_deployment(deployment(), &pin).is_ok());
        assert_eq!(
            validate_deployment(deployment(), &"A".repeat(40)),
            Err(CheckError::InvalidData)
        );
        assert_eq!(
            validate_deployment(deployment(), ""),
            Err(CheckError::InvalidData)
        );
        let mut wrong_ref = deployment();
        wrong_ref.app_ref = format!("app/{}/x86_64/beta", config::APP_ID);
        assert_eq!(
            validate_deployment(wrong_ref, &pin),
            Err(CheckError::InvalidData)
        );
        let mut wrong_ostree = deployment();
        wrong_ostree.ostree_commit = "B".repeat(64);
        assert_eq!(
            validate_deployment(wrong_ostree, &pin),
            Err(CheckError::InvalidData)
        );
        let mut prerelease = deployment();
        prerelease.version = "0.8.3-rc.1".into();
        assert_eq!(
            validate_deployment(prerelease, &pin),
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
    fn release_record_binding_is_exact_and_hidden_from_visible_notes() {
        let digest = "a".repeat(64);
        let marker = format!("{RECORD_DIGEST_PREFIX}{digest}{RECORD_DIGEST_SUFFIX}");
        let notes = format!("Заметки о выпуске\r\n{marker}\r\nПродолжение");
        assert_eq!(record_digest(&notes), Ok(digest.as_str()));
        assert_eq!(plain_notes(&notes), "Заметки о выпуске\nПродолжение");
        for bad_notes in [
            String::new(),
            format!("{marker}\n{marker}"),
            format!("{marker}\n{}", marker.to_ascii_uppercase()),
            format!(" {marker}"),
            format!("{marker} "),
            format!(
                "{RECORD_DIGEST_PREFIX}{}{RECORD_DIGEST_SUFFIX}",
                "A".repeat(64)
            ),
            format!("{RECORD_DIGEST_PREFIX}abc{RECORD_DIGEST_SUFFIX}"),
        ] {
            assert_eq!(record_digest(&bad_notes), Err(CheckError::InvalidData));
        }
    }

    fn http_fixture(
        response: impl Into<Vec<u8>>,
        delay: Duration,
    ) -> (String, thread::JoinHandle<()>) {
        let (base, server) = http_exchanges([("/latest", response.into(), delay)]);
        (format!("{base}/latest"), server)
    }

    fn http_exchanges<const N: usize>(
        exchanges: [(&'static str, Vec<u8>, Duration); N],
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let join = thread::spawn(move || {
            for (path, response, delay) in exchanges {
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
                let request = String::from_utf8(request).unwrap();
                assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
                let headers = request.to_ascii_lowercase();
                assert!(!headers.contains("\r\nauthorization:"));
                assert!(!headers.contains("\r\ncookie:"));
                if path.starts_with("/flatpak/") {
                    assert!(headers.contains("\r\naccept: application/json\r\n"));
                    assert!(!headers.contains("\r\nx-github-api-version:"));
                }
                thread::sleep(delay);
                let _ = stream.write_all(&response);
            }
        });
        (endpoint, join)
    }

    const LATEST_PATH: &str = "/github/releases/latest";
    const FLATPAK_PATH: &str = "/flatpak/KARIN-FLATPAK-UPDATE.json";
    const TAG_PATH: &str = "/github/git/ref/tags/v0.8.3";

    fn response(status: &str, body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    fn latest_response(notes: &str) -> Vec<u8> {
        latest_response_for("0.8.3", notes)
    }

    fn latest_response_for(version: &str, notes: &str) -> Vec<u8> {
        response(
            "200 OK",
            &json!({
                "tag_name": format!("v{version}"),
                "published_at": "2026-11-26T10:00:00Z",
                "draft": false,
                "prerelease": false,
                "body": notes,
                "assets": [
                    {"name": format!("karin-{version}-x86_64.flatpak")},
                    {"name": format!("karin-{version}-linux-x86_64.tar.gz")},
                ],
            })
            .to_string(),
        )
    }

    fn pages_json(version: &str, source_commit: &str) -> String {
        json!({
            "schema": 1,
            "app_id": config::APP_ID,
            "ref": format!("app/{}/{}/master", config::APP_ID, std::env::consts::ARCH),
            "version": version,
            "source_commit": source_commit,
            "ostree_commit": "b".repeat(64),
            "key_fingerprint": KEY_PIN.trim(),
        })
        .to_string()
    }

    fn notes_for_record(body: &str) -> String {
        let sha = record_sha256(body.as_bytes());
        format!("Новый выпуск\n\n{RECORD_DIGEST_PREFIX}{sha}{RECORD_DIGEST_SUFFIX}")
    }

    fn check_fixture<const N: usize>(
        flatpak: bool,
        exchanges: [(&'static str, Vec<u8>, Duration); N],
    ) -> Release {
        check_fixture_after(Version::new(0, 8, 2), flatpak, exchanges)
    }

    fn check_fixture_after<const N: usize>(
        current: Version,
        flatpak: bool,
        exchanges: [(&'static str, Vec<u8>, Duration); N],
    ) -> Release {
        let (base, server) = http_exchanges(exchanges);
        let client = Client::builder()
            .no_proxy()
            .redirect(redirect::Policy::none())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let result = check_release(
            &client,
            &Endpoints {
                github: &format!("{base}/github"),
                flatpak_record: &format!("{base}{FLATPAK_PATH}"),
            },
            &current,
            flatpak,
            &AtomicBool::new(false),
        )
        .unwrap();
        let CheckResult::Available(release) = result else {
            panic!("fixture should report a newer stable release");
        };
        server.join().unwrap();
        *release
    }

    #[test]
    fn flatpak_uses_pages_for_a_release_with_only_installers() {
        for (current, next, tag_path) in [
            (Version::new(0, 8, 2), Version::new(0, 8, 3), TAG_PATH),
            (
                Version::new(0, 8, 3),
                Version::new(0, 8, 4),
                "/github/git/ref/tags/v0.8.4",
            ),
        ] {
            let source = "a".repeat(40);
            let pages = pages_json(&next.to_string(), &source);
            let tag_response = response(
                "200 OK",
                &json!({"object": {"type": "commit", "sha": source}}).to_string(),
            );
            let release = check_fixture_after(
                current,
                true,
                [
                    (
                        LATEST_PATH,
                        latest_response_for(&next.to_string(), &notes_for_record(&pages)),
                        Duration::ZERO,
                    ),
                    (FLATPAK_PATH, response("200 OK", &pages), Duration::ZERO),
                    (tag_path, tag_response, Duration::ZERO),
                ],
            );
            assert_eq!(release.version, next);
            assert_eq!(release.notes, "Новый выпуск");
            assert_eq!(release.page, format!("{RELEASE_PAGE}v{next}"));
            assert_eq!(
                release.deployment,
                DeploymentStatus::Verified(Deployment {
                    version: next,
                    source_commit: source,
                    ostree_commit: "b".repeat(64),
                    app_ref: format!("app/{}/{}/master", config::APP_ID, std::env::consts::ARCH),
                    key_fingerprint: KEY_PIN.trim().to_owned(),
                })
            );
        }
    }

    #[test]
    fn native_does_not_fetch_the_flatpak_record() {
        let release = check_fixture(
            false,
            [(LATEST_PATH, latest_response("Новый выпуск"), Duration::ZERO)],
        );
        assert_eq!(release.deployment, DeploymentStatus::Native);
        assert_eq!(release.notes, "Новый выпуск");
        assert_eq!(release.page, format!("{RELEASE_PAGE}v0.8.3"));
    }

    #[test]
    fn missing_or_different_pages_snapshot_keeps_the_deployment_pending() {
        let source = "a".repeat(40);
        let notes = notes_for_record(&pages_json("0.8.3", &source));
        let forged = pages_json("0.8.3", &source).replace(&"b".repeat(64), &"c".repeat(64));
        for channel_response in [
            response("404 Not Found", ""),
            response("200 OK", &pages_json("0.8.2", &source)),
            response("200 OK", &pages_json("0.8.4", &source)),
            response("200 OK", &forged),
        ] {
            let release = check_fixture(
                true,
                [
                    (LATEST_PATH, latest_response(&notes), Duration::ZERO),
                    (FLATPAK_PATH, channel_response, Duration::ZERO),
                ],
            );
            assert_eq!(release.deployment, DeploymentStatus::NotPublished);
        }
    }

    #[test]
    fn flatpak_record_must_match_stable_tag_source_commit() {
        let pages = pages_json("0.8.3", &"a".repeat(40));
        let release = check_fixture(
            true,
            [
                (
                    LATEST_PATH,
                    latest_response(&notes_for_record(&pages)),
                    Duration::ZERO,
                ),
                (FLATPAK_PATH, response("200 OK", &pages), Duration::ZERO),
                (
                    TAG_PATH,
                    response(
                        "200 OK",
                        &json!({"object": {"type": "commit", "sha": "c".repeat(40)}}).to_string(),
                    ),
                    Duration::ZERO,
                ),
            ],
        );
        assert_eq!(
            release.deployment,
            DeploymentStatus::Unverified(CheckError::InvalidData)
        );
    }

    #[test]
    fn invalid_oversized_redirected_or_unavailable_pages_never_produce_a_deployment() {
        let wrong_version = pages_json("0.8.4", &"a".repeat(40));
        for (channel_response, expected) in [
            (response("200 OK", "not json"), CheckError::InvalidData),
            (
                response("200 OK", &"x".repeat(MAX_DEPLOYMENT_BYTES + 1)),
                CheckError::InvalidData,
            ),
            (
                b"HTTP/1.1 302 Found\r\nLocation: https://example.invalid/untrusted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                CheckError::Http(302),
            ),
            (
                response("503 Service Unavailable", ""),
                CheckError::Http(503),
            ),
        ] {
            let release = check_fixture(
                true,
                [
                    (LATEST_PATH, latest_response(&notes_for_record("not json")), Duration::ZERO),
                    (FLATPAK_PATH, channel_response, Duration::ZERO),
                ],
            );
            assert_eq!(release.deployment, DeploymentStatus::Unverified(expected));
        }
        let release = check_fixture(
            true,
            [
                (
                    LATEST_PATH,
                    latest_response(&notes_for_record(&wrong_version)),
                    Duration::ZERO,
                ),
                (
                    FLATPAK_PATH,
                    response("200 OK", &wrong_version),
                    Duration::ZERO,
                ),
            ],
        );
        assert_eq!(
            release.deployment,
            DeploymentStatus::Unverified(CheckError::InvalidData)
        );
    }

    #[test]
    fn flatpak_refuses_check_without_release_record_binding() {
        let release = check_fixture(
            true,
            [(LATEST_PATH, latest_response("Новый выпуск"), Duration::ZERO)],
        );
        assert_eq!(
            release.deployment,
            DeploymentStatus::Unverified(CheckError::InvalidData)
        );
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
                github_json::<serde_json::Value>(&client, &url, &cancelled),
                Err(expected)
            );
            server.join().unwrap();
        }
        let (url, server) = http_fixture(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\n12345\r\n0\r\n\r\n",
            Duration::ZERO,
        );
        let response = request(client.get(&url), &cancelled).unwrap();
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
            github_json::<serde_json::Value>(&client, &url, &AtomicBool::new(false)),
            Err(CheckError::Timeout)
        );
        server.join().unwrap();
        assert_eq!(
            github_json::<serde_json::Value>(&client, &url, &AtomicBool::new(true)),
            Err(CheckError::Cancelled)
        );
    }
}
