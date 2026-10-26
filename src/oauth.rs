// SPDX-License-Identifier: GPL-3.0-or-later
//! Owner-only OAuth experiment. Nothing here establishes production Music login.
//!
//! The verification-code page gives a manually entered code, not an observable
//! callback. The report explicitly keeps that state's round trip unverified.
//! Credentials never occur in public reports or errors. Owned sensitive buffers
//! are zeroized; HTTP/TLS libraries and the OS can still hold transient copies.

use std::fmt;
use std::io::{Cursor, Read};
use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::blocking::{Body, Client, ClientBuilder, Response};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use reqwest::redirect::Policy;
use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub const VERIFICATION_REDIRECT: &str = "https://oauth.yandex.ru/verification_code";
pub const REQUIRED_SCOPES: [&str; 4] = [
    "music:api-public",
    "music:major-public-api",
    "login:info",
    "login:avatar",
];
const AUTHORIZE_URL: &str = "https://oauth.yandex.ru/authorize";
const TOKEN_URL: &str = "https://oauth.yandex.ru/token";
const IDENTITY_URL: &str = "https://login.yandex.ru/info?format=json";
const STATUS_URL: &str = "https://api.music.yandex.net/account/status";
// One fixed, non-personal, read-only query. Never follows returned content URLs.
const CATALOG_URL: &str =
    "https://api.music.yandex.net/search?text=Beethoven&type=track&page=0&nocorrect=true";
const MAX_BODY: usize = 1024 * 1024;
const MAX_INPUT: usize = 4096;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const CODE_LIFETIME: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ErrorCategory {
    #[error("invalid local input")]
    InvalidInput,
    #[error("cryptographic randomness unavailable")]
    Randomness,
    #[error("HTTP configuration failed")]
    Configuration,
    #[error("authorization cancelled locally")]
    Cancelled,
    #[error("authorization denied")]
    AccessDenied,
    #[error("authorization attempt expired locally")]
    AttemptExpired,
    #[error("returned state does not match")]
    StateMismatch,
    #[error("invalid authorization return")]
    InvalidReturn,
    #[error("authorization code invalid or expired")]
    CodeInvalidOrExpired,
    #[error("required scopes missing or changed")]
    MissingScopes,
    #[error("PKCE or OAuth request rejected")]
    OAuthRequestRejected,
    #[error("OAuth client rejected")]
    ClientRejected,
    #[error("network failure")]
    Network,
    #[error("request timed out")]
    Timeout,
    #[error("HTTP request rejected")]
    Http,
    #[error("access forbidden by service")]
    Forbidden,
    #[error("unsupported response")]
    InvalidResponse,
    #[error("response too large")]
    ResponseTooLarge,
    #[error("token belongs to a different client")]
    ClientMismatch,
    #[error("authenticated account absent")]
    AccountAbsent,
    #[error("access token expired")]
    TokenExpired,
}

/// Safe stage outcome: no URLs, headers, scope strings supplied by a server or
/// personal fields can be retained in this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeResult {
    pub http_status: Option<u16>,
    pub error: Option<ErrorCategory>,
}
impl ProbeResult {
    fn success(status: u16) -> Self {
        Self {
            http_status: Some(status),
            error: None,
        }
    }
    fn failure(status: Option<u16>, error: ErrorCategory) -> Self {
        Self {
            http_status: status,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnedState {
    /// Manual entry does not observe the browser callback. PKCE binds the code;
    /// sending a generated state does not prove its return was checked.
    NotObservedManualCode,
    ValidatedRedirect,
    RejectedRedirect,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeEvidence {
    /// Official Yandex docs say scope is omitted when all requested rights were
    /// granted. This is provider response semantics, not an endpoint entitlement.
    OmittedMeansRequestedGrant,
    ReturnedRequiredScopes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticReport {
    pub returned_state: ReturnedState,
    pub token: ProbeResult,
    pub scopes: Option<ScopeEvidence>,
    pub identity: Option<ProbeResult>,
    pub client_matches: Option<bool>,
    pub identity_account_present: Option<bool>,
    pub music_account: Option<ProbeResult>,
    pub music_account_present: Option<bool>,
    pub service_available: Option<bool>,
    pub catalog: Option<ProbeResult>,
}
impl DiagnosticReport {
    fn new(returned_state: ReturnedState, token: ProbeResult) -> Self {
        Self {
            returned_state,
            token,
            scopes: None,
            identity: None,
            client_matches: None,
            identity_account_present: None,
            music_account: None,
            music_account_present: None,
            service_available: None,
            catalog: None,
        }
    }
}

impl DiagnosticReport {
    /// This is only the scoped read-only diagnostic outcome, never production
    /// authorization, a playback entitlement or stream-delivery approval.
    /// Service availability remains an independent reported flag.
    pub fn passed(&self) -> bool {
        self.token.error.is_none()
            && [self.identity, self.music_account, self.catalog]
                .into_iter()
                .all(|stage| stage.is_some_and(|value| value.error.is_none()))
            && self.client_matches == Some(true)
            && self.music_account_present == Some(true)
    }
}
impl fmt::Display for DiagnosticReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn stage(
            f: &mut fmt::Formatter<'_>,
            label: &str,
            value: Option<ProbeResult>,
        ) -> fmt::Result {
            match value {
                None => writeln!(f, "{label}: not run"),
                Some(value) => {
                    write!(f, "{label}: HTTP ")?;
                    match value.http_status {
                        Some(status) => write!(f, "{status}")?,
                        None => write!(f, "not observed")?,
                    }
                    match value.error {
                        Some(error) => writeln!(f, "; {error}"),
                        None => writeln!(f, "; accepted diagnostic response"),
                    }
                }
            }
        }
        writeln!(f, "Returned state: {:?}", self.returned_state)?;
        stage(f, "Code exchange", Some(self.token))?;
        writeln!(f, "Scope evidence: {:?}", self.scopes)?;
        stage(f, "Yandex ID", self.identity)?;
        writeln!(
            f,
            "Karin client matches: {:?}; ID account present: {:?}",
            self.client_matches, self.identity_account_present
        )?;
        stage(f, "Music account", self.music_account)?;
        writeln!(
            f,
            "Music account present: {:?}; service available: {:?}",
            self.music_account_present, self.service_available
        )?;
        stage(f, "Read-only catalog", self.catalog)
    }
}

/// Sensitive input, intentionally redacted even under Debug.
pub enum AuthorizationResponse {
    /// The only input offered by the developer CLI. A blank input cancels.
    ManualCode(Zeroizing<String>),
    /// Protocol-level support for a complete documented debug return. The CLI
    /// does not solicit a URL or claim the verification page returns it.
    ReturnedRedirect(Zeroizing<String>),
    Cancelled,
}
impl fmt::Debug for AuthorizationResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthorizationResponse([redacted])")
    }
}

/// A single-use attempt. Neither its verifier nor its random state is exposed
/// separately; the authorization URL is handed directly to the system browser.
pub struct AuthorizationAttempt {
    client_id: Zeroizing<String>,
    verifier: Zeroizing<String>,
    state: Zeroizing<String>,
    created: Instant,
}
impl fmt::Debug for AuthorizationAttempt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthorizationAttempt([redacted])")
    }
}
impl AuthorizationAttempt {
    pub fn new(client_id: &str) -> Result<Self, ErrorCategory> {
        if client_id.is_empty()
            || client_id.len() > 128
            || !client_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err(ErrorCategory::InvalidInput);
        }
        let rng = SystemRandom::new();
        Ok(Self {
            client_id: Zeroizing::new(client_id.to_owned()),
            verifier: random_secret(&rng)?,
            state: random_secret(&rng)?,
            created: Instant::now(),
        })
    }

    /// Do not print this URL: it contains session binding material.
    pub fn authorization_url(&self) -> Zeroizing<String> {
        let challenge = pkce_challenge(&self.verifier);
        let mut result = Zeroizing::new(String::from(AUTHORIZE_URL));
        result.push('?');
        append_form(
            &mut result,
            &[
                ("response_type", "code"),
                ("client_id", &self.client_id),
                ("redirect_uri", VERIFICATION_REDIRECT),
                ("scope", &REQUIRED_SCOPES.join(" ")),
                ("code_challenge", &challenge),
                ("code_challenge_method", "S256"),
                ("state", &self.state),
            ],
        );
        result
    }

    pub fn complete(self, response: AuthorizationResponse) -> DiagnosticReport {
        let client = match DiagnosticClient::new() {
            Ok(client) => client,
            Err(error) => {
                return DiagnosticReport::new(
                    ReturnedState::NotObservedManualCode,
                    ProbeResult::failure(None, error),
                );
            }
        };
        self.complete_with(response, &client)
    }

    fn complete_with(
        self,
        response: AuthorizationResponse,
        client: &DiagnosticClient,
    ) -> DiagnosticReport {
        let observed = match &response {
            AuthorizationResponse::ReturnedRedirect(_) => ReturnedState::RejectedRedirect,
            _ => ReturnedState::NotObservedManualCode,
        };
        let code = match self.authorization_code(response) {
            Ok(code) => code,
            Err(error) => {
                return DiagnosticReport::new(observed, ProbeResult::failure(None, error));
            }
        };
        let observed = if observed == ReturnedState::RejectedRedirect {
            ReturnedState::ValidatedRedirect
        } else {
            observed
        };
        let token = match client.exchange(&self, &code) {
            Ok(token) => token,
            Err(result) => return DiagnosticReport::new(observed, result),
        };
        let mut report = DiagnosticReport::new(observed, ProbeResult::success(token.http_status));
        report.scopes = Some(token.scopes);
        client.probe(&self.client_id, token, &mut report);
        report
    }

    fn authorization_code(
        &self,
        response: AuthorizationResponse,
    ) -> Result<Zeroizing<String>, ErrorCategory> {
        if self.created.elapsed() >= CODE_LIFETIME {
            return Err(ErrorCategory::AttemptExpired);
        }
        let code = match response {
            AuthorizationResponse::Cancelled => return Err(ErrorCategory::Cancelled),
            AuthorizationResponse::ManualCode(code) => code,
            AuthorizationResponse::ReturnedRedirect(url) => {
                if url.len() > MAX_INPUT {
                    return Err(ErrorCategory::InvalidReturn);
                }
                let url = reqwest::Url::parse(&url).map_err(|_| ErrorCategory::InvalidReturn)?;
                if url.scheme() != "https"
                    || url.host_str() != Some("oauth.yandex.ru")
                    || url.port().is_some()
                    || url.path() != "/verification_code"
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.fragment().is_some()
                {
                    return Err(ErrorCategory::InvalidReturn);
                }
                let mut state = None;
                let mut code = None;
                let mut error = None;
                for (key, value) in url.query_pairs() {
                    match key.as_ref() {
                        "state" if state.is_none() => {
                            state = Some(Zeroizing::new(value.into_owned()))
                        }
                        "code" if code.is_none() => code = Some(Zeroizing::new(value.into_owned())),
                        "error" if error.is_none() => {
                            error = Some(Zeroizing::new(value.into_owned()))
                        }
                        "state" | "code" | "error" => return Err(ErrorCategory::InvalidReturn),
                        // Provider descriptions are neither retained nor displayed.
                        _ => {}
                    }
                }
                let state = state.ok_or(ErrorCategory::StateMismatch)?;
                if !states_equal(&self.state, &state) {
                    return Err(ErrorCategory::StateMismatch);
                }
                if let Some(error) = error {
                    if code.is_some() {
                        return Err(ErrorCategory::InvalidReturn);
                    }
                    return Err(oauth_error(&error));
                }
                code.ok_or(ErrorCategory::InvalidReturn)?
            }
        };
        if code.is_empty() {
            return Err(ErrorCategory::Cancelled);
        }
        // Yandex documents a seven-digit verification code (bad_verification_code).
        // Fail closed if that format changes; never accept a pasted OAuth token.
        if code.len() != 7 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ErrorCategory::InvalidInput);
        }
        Ok(code)
    }
}

fn random_secret(rng: &impl SecureRandom) -> Result<Zeroizing<String>, ErrorCategory> {
    let mut bytes = Zeroizing::new([0_u8; 32]);
    rng.fill(bytes.as_mut())
        .map_err(|_| ErrorCategory::Randomness)?;
    Ok(Zeroizing::new(URL_SAFE_NO_PAD.encode(bytes.as_ref())))
}
fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()).as_ref())
}
fn states_equal(expected: &str, actual: &str) -> bool {
    bool::from(expected.as_bytes().ct_eq(actual.as_bytes()))
}

fn append_form(target: &mut String, values: &[(&str, &str)]) {
    for (index, (key, value)) in values.iter().enumerate() {
        if index > 0 {
            target.push('&');
        }
        encode_form(target, key);
        target.push('=');
        encode_form(target, value);
    }
}
fn encode_form(target: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"*-._".contains(&byte) {
            target.push(char::from(byte));
        } else if byte == b' ' {
            target.push('+');
        } else {
            target.push('%');
            target.push(char::from(HEX[(byte >> 4) as usize]));
            target.push(char::from(HEX[(byte & 15) as usize]));
        }
    }
}

struct DiagnosticClient {
    client: Client,
    token_url: String,
    identity_url: String,
    status_url: String,
    catalog_url: String,
    timeout: Duration,
}
fn client_builder() -> ClientBuilder {
    Client::builder()
        .user_agent(concat!(
            "Karin/",
            env!("CARGO_PKG_VERSION"),
            " oauth-diagnostic"
        ))
        .connect_timeout(Duration::from_secs(5))
        .timeout(REQUEST_TIMEOUT)
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
}
impl DiagnosticClient {
    fn new() -> Result<Self, ErrorCategory> {
        Ok(Self {
            client: client_builder()
                .build()
                .map_err(|_| ErrorCategory::Configuration)?,
            token_url: TOKEN_URL.into(),
            identity_url: IDENTITY_URL.into(),
            status_url: STATUS_URL.into(),
            catalog_url: CATALOG_URL.into(),
            timeout: REQUEST_TIMEOUT,
        })
    }
    fn exchange(&self, attempt: &AuthorizationAttempt, code: &str) -> Result<Token, ProbeResult> {
        // Bounds on the Client ID, seven-digit code and verifier fit within
        // this buffer, including form escaping, without reallocating secrets.
        let mut form = Zeroizing::new(String::with_capacity(1024));
        append_form(
            &mut form,
            &[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", &attempt.client_id),
                ("redirect_uri", VERIFICATION_REDIRECT),
                ("code_verifier", &attempt.verifier),
            ],
        );
        let bytes = Zeroizing::new(std::mem::take(&mut *form).into_bytes());
        let len = bytes.len() as u64;
        let response = self
            .client
            .post(&self.token_url)
            .timeout(self.timeout)
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::sized(Cursor::new(bytes), len))
            .send()
            .map_err(network_failure)?;
        let status = response.status().as_u16();
        let body =
            read_body(response).map_err(|error| ProbeResult::failure(Some(status), error))?;
        if status != 200 {
            let category = serde_json::from_slice::<OAuthError<'_>>(&body)
                .map(|value| oauth_error(value.error))
                .unwrap_or(ErrorCategory::Http);
            return Err(ProbeResult::failure(Some(status), category));
        }
        let value: TokenResponse<'_> = serde_json::from_slice(&body)
            .map_err(|_| ProbeResult::failure(Some(status), ErrorCategory::InvalidResponse))?;
        if !value.token_type.eq_ignore_ascii_case("bearer")
            || value.access_token.is_empty()
            || value.access_token.len() > MAX_INPUT
            || !value
                .access_token
                .bytes()
                .all(|byte| byte.is_ascii_graphic())
            || value.expires_in == 0
        {
            return Err(ProbeResult::failure(
                Some(status),
                ErrorCategory::InvalidResponse,
            ));
        }
        let scopes = match value.scope {
            None => ScopeEvidence::OmittedMeansRequestedGrant,
            Some(scope)
                if REQUIRED_SCOPES
                    .iter()
                    .all(|required| scope.split_ascii_whitespace().any(|got| got == *required)) =>
            {
                ScopeEvidence::ReturnedRequiredScopes
            }
            Some(_) => {
                return Err(ProbeResult::failure(
                    Some(status),
                    ErrorCategory::MissingScopes,
                ));
            }
        };
        Ok(Token {
            access: Zeroizing::new(value.access_token.to_owned()),
            expires_at: Instant::now()
                .checked_add(Duration::from_secs(value.expires_in))
                .ok_or(ProbeResult::failure(
                    Some(status),
                    ErrorCategory::InvalidResponse,
                ))?,
            scopes,
            http_status: status,
        })
    }

    fn authorized_get(
        &self,
        url: &str,
        token: &Token,
    ) -> Result<(u16, Zeroizing<Vec<u8>>), ProbeResult> {
        if Instant::now() >= token.expires_at {
            return Err(ProbeResult::failure(None, ErrorCategory::TokenExpired));
        }
        let header = Zeroizing::new(format!("OAuth {}", *token.access));
        let mut header = HeaderValue::from_str(&header)
            .map_err(|_| ProbeResult::failure(None, ErrorCategory::InvalidResponse))?;
        header.set_sensitive(true);
        let response = self
            .client
            .get(url)
            .timeout(self.timeout)
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, header)
            .send()
            .map_err(network_failure)?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(ProbeResult::failure(
                Some(status),
                match status {
                    401 => ErrorCategory::AccessDenied,
                    403 => ErrorCategory::Forbidden,
                    _ => ErrorCategory::Http,
                },
            ));
        }
        read_body(response)
            .map(|body| (status, body))
            .map_err(|error| ProbeResult::failure(Some(status), error))
    }
    fn probe(&self, client_id: &str, token: Token, report: &mut DiagnosticReport) {
        let (status, body) = match self.authorized_get(&self.identity_url, &token) {
            Ok(value) => value,
            Err(result) => {
                report.identity = Some(result);
                return;
            }
        };
        let identity: IdentityResponse<'_> = match serde_json::from_slice(&body) {
            Ok(value) => value,
            Err(_) => {
                report.identity = Some(ProbeResult::failure(
                    Some(status),
                    ErrorCategory::InvalidResponse,
                ));
                return;
            }
        };
        let matches = identity.client_id == client_id;
        report.client_matches = Some(matches);
        report.identity_account_present = Some(identity.id.0);
        report.identity = Some(if !matches {
            ProbeResult::failure(Some(status), ErrorCategory::ClientMismatch)
        } else if !identity.id.0 {
            ProbeResult::failure(Some(status), ErrorCategory::AccountAbsent)
        } else {
            ProbeResult::success(status)
        });
        if !matches || !identity.id.0 {
            return;
        }
        // Drop raw identity JSON before further probes; no avatar/profile query.
        drop(body);
        let (status, body) = match self.authorized_get(&self.status_url, &token) {
            Ok(value) => value,
            Err(result) => {
                report.music_account = Some(result);
                return;
            }
        };
        let envelope: AccountEnvelope = match serde_json::from_slice(&body) {
            Ok(value) => value,
            Err(_) => {
                report.music_account = Some(ProbeResult::failure(
                    Some(status),
                    ErrorCategory::InvalidResponse,
                ));
                return;
            }
        };
        let present = envelope.result.account.uid.0;
        report.music_account_present = Some(present);
        report.service_available = envelope.result.account.service_available;
        report.music_account = Some(if present {
            ProbeResult::success(status)
        } else {
            ProbeResult::failure(Some(status), ErrorCategory::AccountAbsent)
        });
        drop(body);
        if !present {
            return;
        }
        let (status, body) = match self.authorized_get(&self.catalog_url, &token) {
            Ok(value) => value,
            Err(result) => {
                report.catalog = Some(result);
                return;
            }
        };
        // Require a track-search result collection while discarding its contents.
        // HTTP200 and arbitrary JSON objects never establish catalog success.
        report.catalog = Some(match serde_json::from_slice::<CatalogEnvelope>(&body) {
            Ok(_) => ProbeResult::success(status),
            _ => ProbeResult::failure(Some(status), ErrorCategory::InvalidResponse),
        });
    }
}

struct Token {
    access: Zeroizing<String>,
    expires_at: Instant,
    scopes: ScopeEvidence,
    http_status: u16,
}
#[derive(Deserialize)]
struct TokenResponse<'a> {
    #[serde(borrow)]
    access_token: &'a str,
    token_type: &'a str,
    expires_in: u64,
    #[serde(default, deserialize_with = "present_scope")]
    scope: Option<&'a str>,
}
fn present_scope<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<&'de str>, D::Error> {
    // Only an absent property invokes default. An explicit null is schema drift,
    // not the documented omitted full-grant response.
    <&str>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
struct OAuthError<'a> {
    #[serde(borrow)]
    error: &'a str,
}
#[derive(Deserialize)]
struct IdentityResponse<'a> {
    #[serde(borrow)]
    client_id: &'a str,
    #[serde(default)]
    id: PresentIdentifier,
}
#[derive(Deserialize)]
struct AccountEnvelope {
    result: AccountResult,
}
#[derive(Deserialize)]
struct AccountResult {
    account: MusicAccount,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MusicAccount {
    #[serde(default)]
    uid: PresentIdentifier,
    service_available: Option<bool>,
}
#[derive(Default)]
struct PresentIdentifier(bool);
impl<'de> Deserialize<'de> for PresentIdentifier {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IdentifierVisitor;
        impl serde::de::Visitor<'_> for IdentifierVisitor {
            type Value = PresentIdentifier;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("optional account identifier")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(PresentIdentifier(
                    value.bytes().all(|byte| byte.is_ascii_digit())
                        && value.parse::<u64>().is_ok_and(|id| id > 0),
                ))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(PresentIdentifier(value > 0))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(PresentIdentifier(value > 0))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(PresentIdentifier(false))
            }
        }
        deserializer.deserialize_any(IdentifierVisitor)
    }
}
#[derive(Deserialize)]
struct CatalogEnvelope {
    #[serde(rename = "result")]
    _result: CatalogSearch,
}
#[derive(Deserialize)]
struct CatalogSearch {
    #[serde(rename = "tracks")]
    _tracks: TrackResults,
}
#[derive(Deserialize)]
struct TrackResults {
    #[serde(rename = "total")]
    _total: u64,
    #[serde(rename = "results")]
    _results: IgnoredArray,
}
struct IgnoredArray;
impl<'de> Deserialize<'de> for IgnoredArray {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ArrayVisitor;
        impl<'de> serde::de::Visitor<'de> for ArrayVisitor {
            type Value = IgnoredArray;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("catalog results array")
            }
            fn visit_seq<S: serde::de::SeqAccess<'de>>(
                self,
                mut seq: S,
            ) -> Result<Self::Value, S::Error> {
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(IgnoredArray)
            }
        }
        deserializer.deserialize_seq(ArrayVisitor)
    }
}

fn oauth_error(error: &str) -> ErrorCategory {
    match error {
        "access_denied" => ErrorCategory::AccessDenied,
        "invalid_grant" | "bad_verification_code" => ErrorCategory::CodeInvalidOrExpired,
        "invalid_scope" | "insufficient_scope" => ErrorCategory::MissingScopes,
        "invalid_client" | "unauthorized_client" => ErrorCategory::ClientRejected,
        _ => ErrorCategory::OAuthRequestRejected,
    }
}
fn network_failure(error: reqwest::Error) -> ProbeResult {
    ProbeResult::failure(
        None,
        if error.is_timeout() {
            ErrorCategory::Timeout
        } else {
            ErrorCategory::Network
        },
    )
}
fn read_body(mut response: Response) -> Result<Zeroizing<Vec<u8>>, ErrorCategory> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BODY as u64)
    {
        return Err(ErrorCategory::ResponseTooLarge);
    }
    // Preallocate once so sensitive bytes aren't copied by Vec reallocation.
    let mut body = Zeroizing::new(Vec::with_capacity(MAX_BODY + 1));
    response
        .by_ref()
        .take((MAX_BODY + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|error| {
            let timed_out = error.kind() == std::io::ErrorKind::TimedOut
                || error
                    .get_ref()
                    .and_then(|value| value.downcast_ref::<reqwest::Error>())
                    .is_some_and(reqwest::Error::is_timeout);
            if timed_out {
                ErrorCategory::Timeout
            } else {
                ErrorCategory::Network
            }
        })?;
    if body.len() > MAX_BODY {
        return Err(ErrorCategory::ResponseTooLarge);
    }
    Ok(body)
}

#[cfg(test)]
mod tests;
