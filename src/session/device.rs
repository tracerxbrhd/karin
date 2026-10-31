// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit developer-only OAuth Device Flow. No embedded client credentials.

use std::{
    fmt,
    io::{Cursor, Read},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use reqwest::{
    Url,
    blocking::{Body, Client, Response},
    header::{ACCEPT, CONTENT_TYPE},
    redirect::Policy,
};
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;
use zeroize::Zeroizing;

use super::AccessToken;

const MAX_BODY: usize = 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const BEGIN_URL: &str = "https://oauth.yandex.ru/device/code";
const TOKEN_URL: &str = "https://oauth.yandex.ru/token";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    #[error("Development OAuth configuration is invalid or unavailable.")]
    Configuration,
    #[error("Cryptographic randomness is unavailable.")]
    Randomness,
    #[error("The authorization request could not reach the service.")]
    Network,
    #[error("The authorization request timed out.")]
    Timeout,
    #[error("The authorization response is unsupported.")]
    InvalidResponse,
    #[error("The authorization response exceeds the size limit.")]
    ResponseTooLarge,
    #[error("The OAuth application was rejected.")]
    ClientRejected,
    #[error("The authorization request was rejected.")]
    Rejected,
    #[error("Authorization was cancelled locally.")]
    Cancelled,
}

#[derive(Clone)]
pub struct DeviceAuthClient {
    http: Client,
    client_id: Zeroizing<String>,
    client_secret: Zeroizing<String>,
    device_id: Zeroizing<String>,
    begin_url: String,
    token_url: String,
    timeout: Duration,
}

impl fmt::Debug for DeviceAuthClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceAuthClient([redacted])")
    }
}

/// One native code screen. Codes/URL are intentionally accessible to that screen
/// only; Debug and errors cannot render them. Clones share cancellation/poll state.
#[derive(Clone)]
pub struct DeviceChallenge {
    device_code: Zeroizing<String>,
    user_code: Zeroizing<String>,
    verification_url: String,
    expires_at: Instant,
    state: Arc<Mutex<PollState>>,
    cancelled: Arc<AtomicBool>,
}

struct PollState {
    interval: Duration,
    next_poll: Instant,
    in_flight: bool,
    finished: bool,
}

impl fmt::Debug for DeviceChallenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceChallenge([redacted])")
    }
}

impl DeviceChallenge {
    pub fn user_code(&self) -> &str {
        &self.user_code
    }
    pub fn verification_url(&self) -> &str {
        &self.verification_url
    }
    pub fn expires_in(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }
    pub fn interval(&self) -> Duration {
        self.state
            .lock()
            .map_or(Duration::from_secs(5), |state| state.interval)
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Debug)]
pub enum DevicePoll {
    Pending,
    SlowDown,
    Authorized(AccessToken),
    Denied,
    Expired,
}

impl DeviceAuthClient {
    /// No default or borrowed OAuth identity exists. The consenting developer
    /// supplies application configuration through the local developer dialog.
    /// This constructor performs no network request and never writes credentials.
    pub fn new(
        client_id: Zeroizing<String>,
        client_secret: Zeroizing<String>,
    ) -> Result<Self, SessionError> {
        if !bounded_graphic(&client_id, 128) || !bounded_graphic(&client_secret, 4096) {
            return Err(SessionError::Configuration);
        }
        let mut random = Zeroizing::new([0_u8; 16]);
        SystemRandom::new()
            .fill(random.as_mut())
            .map_err(|_| SessionError::Randomness)?;
        // Match the documented client's UUID device identifier shape without a
        // new runtime dependency or a stable machine identifier.
        random[6] = (random[6] & 0x0f) | 0x40;
        random[8] = (random[8] & 0x3f) | 0x80;
        let mut device_id = Zeroizing::new(String::with_capacity(36));
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for (index, byte) in random.iter().copied().enumerate() {
            if matches!(index, 4 | 6 | 8 | 10) {
                device_id.push('-');
            }
            device_id.push(char::from(HEX[(byte >> 4) as usize]));
            device_id.push(char::from(HEX[(byte & 15) as usize]));
        }
        let http = Client::builder()
            .user_agent(concat!(
                "Karin/",
                env!("CARGO_PKG_VERSION"),
                " development-device-auth"
            ))
            .connect_timeout(Duration::from_secs(5))
            .timeout(REQUEST_TIMEOUT)
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .map_err(|_| SessionError::Configuration)?;
        Ok(Self {
            http,
            client_id,
            client_secret,
            device_id,
            begin_url: BEGIN_URL.into(),
            token_url: TOKEN_URL.into(),
            timeout: REQUEST_TIMEOUT,
        })
    }

    /// A bounded blocking request; call only on an application worker.
    pub fn begin(&self) -> Result<DeviceChallenge, SessionError> {
        let (status, body) = self.post(
            &self.begin_url,
            &[
                ("client_id", &self.client_id),
                ("device_id", &self.device_id),
                ("device_name", "Karin development experiment"),
            ],
        )?;
        if status != 200 {
            return Err(provider_error(&body));
        }
        let value: ChallengeResponse<'_> =
            serde_json::from_slice(&body).map_err(|_| SessionError::InvalidResponse)?;
        if !bounded_graphic(value.device_code, 4096)
            || !bounded_graphic(value.user_code, 128)
            || value.expires_in == 0
            || value.expires_in > 3600
            || value.interval == 0
            || value.interval > 300
            || !valid_verification_url(value.verification_url)
        {
            return Err(SessionError::InvalidResponse);
        }
        let now = Instant::now();
        let interval = Duration::from_secs(value.interval);
        Ok(DeviceChallenge {
            device_code: Zeroizing::new(value.device_code.to_owned()),
            user_code: Zeroizing::new(value.user_code.to_owned()),
            verification_url: value.verification_url.to_owned(),
            expires_at: now + Duration::from_secs(value.expires_in),
            state: Arc::new(Mutex::new(PollState {
                interval,
                next_poll: now + interval,
                in_flight: false,
                finished: false,
            })),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Poll once, without sleeping. Enforce the provider interval across clones;
    /// the native UI owns scheduling, cancellation and its visible countdown.
    pub fn poll(&self, challenge: &DeviceChallenge) -> Result<DevicePoll, SessionError> {
        if challenge.cancelled.load(Ordering::Acquire) {
            return Err(SessionError::Cancelled);
        }
        if challenge.expires_in().is_zero() {
            return Ok(DevicePoll::Expired);
        }
        {
            let mut state = challenge.state.lock().map_err(|_| SessionError::Rejected)?;
            if state.finished {
                return Err(SessionError::Rejected);
            }
            if state.in_flight || Instant::now() < state.next_poll {
                return Ok(DevicePoll::Pending);
            }
            state.in_flight = true;
        }
        let result = self.poll_once(challenge);
        let mut state = challenge.state.lock().map_err(|_| SessionError::Rejected)?;
        state.in_flight = false;
        if matches!(result, Ok(DevicePoll::SlowDown)) {
            state.interval = state.interval.saturating_add(Duration::from_secs(5));
        }
        // Start-to-start polling could immediately repeat after a slow HTTP
        // response. Wait the full interval from completion instead.
        state.next_poll = Instant::now() + state.interval;
        if matches!(
            result,
            Ok(DevicePoll::Authorized(_) | DevicePoll::Denied | DevicePoll::Expired)
        ) {
            state.finished = true;
        }
        result
    }

    fn poll_once(&self, challenge: &DeviceChallenge) -> Result<DevicePoll, SessionError> {
        let (status, body) = self.post(
            &self.token_url,
            &[
                ("grant_type", "device_code"),
                ("code", &challenge.device_code),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
            ],
        )?;
        if challenge.cancelled.load(Ordering::Acquire) {
            return Err(SessionError::Cancelled);
        }
        if challenge.expires_in().is_zero() {
            return Ok(DevicePoll::Expired);
        }
        if let Ok(error) = serde_json::from_slice::<OAuthError<'_>>(&body) {
            if status != 400 {
                return Err(provider_error(&body));
            }
            return match error.error {
                "authorization_pending" => Ok(DevicePoll::Pending),
                "slow_down" => Ok(DevicePoll::SlowDown),
                "access_denied" => Ok(DevicePoll::Denied),
                "expired_token" | "code_expired" => Ok(DevicePoll::Expired),
                _ => Err(provider_error(&body)),
            };
        }
        if status != 200 {
            return Err(SessionError::Rejected);
        }
        let value: TokenResponse<'_> =
            serde_json::from_slice(&body).map_err(|_| SessionError::InvalidResponse)?;
        if !bounded_graphic(value.access_token, 4096)
            || value
                .token_type
                .is_some_and(|kind| !kind.eq_ignore_ascii_case("bearer"))
            || value.expires_in == Some(0)
        {
            return Err(SessionError::InvalidResponse);
        }
        let expires_at = match value.expires_in {
            Some(seconds) => Some(
                Instant::now()
                    .checked_add(Duration::from_secs(seconds))
                    .ok_or(SessionError::InvalidResponse)?,
            ),
            None => None,
        };
        Ok(DevicePoll::Authorized(AccessToken::new(
            Zeroizing::new(value.access_token.to_owned()),
            expires_at,
        )))
    }

    fn post(
        &self,
        url: &str,
        fields: &[(&str, &str)],
    ) -> Result<(u16, Zeroizing<Vec<u8>>), SessionError> {
        // Form serialization uses a zeroizing allocation rather than reqwest's
        // owned string form; no credential is ever placed in a request URL.
        let capacity = fields
            .iter()
            .map(|(key, value)| (key.len() + value.len()) * 3 + 2)
            .sum();
        let mut form = Zeroizing::new(String::with_capacity(capacity));
        for (index, (key, value)) in fields.iter().enumerate() {
            if index > 0 {
                form.push('&');
            }
            encode_form(&mut form, key);
            form.push('=');
            encode_form(&mut form, value);
        }
        let bytes = Zeroizing::new(std::mem::take(&mut *form).into_bytes());
        let len = bytes.len() as u64;
        let response = self
            .http
            .post(url)
            .timeout(self.timeout)
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::sized(Cursor::new(bytes), len))
            .send()
            .map_err(network_error)?;
        let status = response.status().as_u16();
        read_body(response).map(|body| (status, body))
    }
}

fn valid_verification_url(value: &str) -> bool {
    if !bounded_graphic(value, 512) {
        return false;
    }
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && matches!(url.host_str(), Some("oauth.yandex.ru" | "ya.ru"))
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(url.path(), "/device" | "/device/")
        && url.query().is_none()
        && url.fragment().is_none()
}

fn bounded_graphic(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.bytes().all(|byte| byte.is_ascii_graphic())
}

fn network_error(error: reqwest::Error) -> SessionError {
    if error.is_timeout() {
        SessionError::Timeout
    } else {
        SessionError::Network
    }
}
fn read_body(mut response: Response) -> Result<Zeroizing<Vec<u8>>, SessionError> {
    if response
        .content_length()
        .is_some_and(|len| len > MAX_BODY as u64)
    {
        return Err(SessionError::ResponseTooLarge);
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(MAX_BODY + 1));
    let mut chunk = Zeroizing::new([0_u8; 8192]);
    loop {
        let count = response.read(chunk.as_mut()).map_err(|error| {
            let timed_out = error.kind() == std::io::ErrorKind::TimedOut
                || error.get_ref().is_some_and(|inner| {
                    inner
                        .downcast_ref::<reqwest::Error>()
                        .is_some_and(reqwest::Error::is_timeout)
                });
            if timed_out {
                SessionError::Timeout
            } else {
                SessionError::Network
            }
        })?;
        if count == 0 {
            return Ok(bytes);
        }
        if bytes.len() + count > MAX_BODY {
            return Err(SessionError::ResponseTooLarge);
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}
fn provider_error(body: &[u8]) -> SessionError {
    match serde_json::from_slice::<OAuthError<'_>>(body).map(|value| value.error) {
        Ok("invalid_client" | "unauthorized_client") => SessionError::ClientRejected,
        _ => SessionError::Rejected,
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

#[derive(Deserialize)]
struct ChallengeResponse<'a> {
    #[serde(borrow)]
    device_code: &'a str,
    user_code: &'a str,
    verification_url: &'a str,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
    #[serde(default, rename = "error", deserialize_with = "reject_error")]
    _error: (),
}
fn default_interval() -> u64 {
    5
}
#[derive(Deserialize)]
struct OAuthError<'a> {
    #[serde(borrow)]
    error: &'a str,
}
#[derive(Deserialize)]
struct TokenResponse<'a> {
    #[serde(borrow)]
    access_token: &'a str,
    #[serde(default, deserialize_with = "present_token_type")]
    token_type: Option<&'a str>,
    #[serde(default, deserialize_with = "present_expiry")]
    expires_in: Option<u64>,
    #[serde(default, rename = "error", deserialize_with = "reject_error")]
    _error: (),
}

fn present_token_type<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<&'de str>, D::Error> {
    <&str>::deserialize(deserializer).map(Some)
}
fn present_expiry<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    u64::deserialize(deserializer).map(Some)
}
fn reject_error<'de, D: serde::Deserializer<'de>>(_: D) -> Result<(), D::Error> {
    Err(serde::de::Error::custom("error envelope is not a grant"))
}

#[cfg(test)]
mod tests;
