// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright 2023-2024 Vladimir Vaskov (Cassette reference implementation)
// Copyright 2026 Karin contributors (Rust adaptation)
//! Cassette-compatible session grant, adapted from Cassette v0.2.4
//! `src/widgets/begin/begin-view.vala` and `src/client/api/yam-client.vala`.
//! See packaging/third-party-licenses/cassette-auth for provenance and GPL.
//! The published shared registration is not Karin-owned. Source licensing does
//! not establish provider endorsement or public-distribution permission.

use std::{
    io::Read,
    sync::Arc,
    time::{Duration, Instant},
};

use reqwest::{
    blocking::Client,
    header::{COOKIE, HeaderValue},
    redirect::Policy,
};
use serde::Deserialize;
use zeroize::Zeroizing;

use super::{
    AuthAttempt, AuthError, AuthProvider, AuthorizationCode, SessionCookies, TokenErrorCategory,
};
use crate::session::AccessToken;

const TOKEN_ENDPOINT: &str = "https://oauth.yandex.ru/token";
const RESPONSE_LIMIT: u64 = 64 * 1024;

#[derive(Deserialize)]
struct PublishedRegistration {
    client_id: String,
    client_secret: String,
}

/// Published Cassette application configuration is intentionally transparent,
/// not an end-user secret or a promise of service authorization.
pub struct CassetteProvider {
    #[cfg(test)]
    endpoint: Option<String>,
    #[cfg(test)]
    timeout: Duration,
}

impl Default for CassetteProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CassetteProvider {
    pub fn new() -> Self {
        Self {
            #[cfg(test)]
            endpoint: None,
            #[cfg(test)]
            timeout: Duration::from_secs(20),
        }
    }

    fn registration(&self) -> Result<PublishedRegistration, AuthError> {
        #[cfg(test)]
        if self.endpoint.is_some() {
            return Ok(PublishedRegistration {
                client_id: "synthetic-cassette-client".into(),
                client_secret: "synthetic-public-app-configuration".into(),
            });
        }
        serde_json::from_str(include_str!("cassette_client.json"))
            .map_err(|_| AuthError::Configuration)
    }

    fn token_endpoint(&self) -> &str {
        #[cfg(test)]
        if let Some(endpoint) = &self.endpoint {
            return endpoint;
        }
        TOKEN_ENDPOINT
    }

    fn request_timeout(&self) -> Duration {
        #[cfg(test)]
        {
            self.timeout
        }
        #[cfg(not(test))]
        {
            Duration::from_secs(20)
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(endpoint: &str) -> Self {
        let url = reqwest::Url::parse(endpoint).expect("numeric loopback fixture");
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert!(url.port().is_some());
        Self {
            endpoint: Some(endpoint.into()),
            timeout: Duration::from_secs(3),
        }
    }
}

impl AuthProvider for CassetteProvider {
    fn begin(&self) -> Result<Arc<AuthAttempt>, AuthError> {
        let registration = self.registration()?;
        AuthAttempt::session(&registration.client_id)
    }

    fn exchange(&self, _: &AuthAttempt, _: AuthorizationCode) -> Result<AccessToken, AuthError> {
        // The original diagnostic's authorization-code/PKCE flow is untouched.
        Err(AuthError::Configuration)
    }

    fn exchange_session(
        &self,
        attempt: &AuthAttempt,
        cookies: SessionCookies,
    ) -> Result<AccessToken, AuthError> {
        attempt.claim_session_exchange()?;
        // Do not reuse this cookie header for Music, follow redirects, import a
        // browser profile or attach an official-client identity header.
        if attempt.session_cookie_uri() != self.token_endpoint() {
            return Err(AuthError::Configuration);
        }
        let registration = self.registration()?;
        let form = Zeroizing::new(
            reqwest::Url::parse_with_params(
                "https://localhost/",
                [
                    ("grant_type", "sessionid"),
                    ("client_id", registration.client_id.as_str()),
                    ("client_secret", registration.client_secret.as_str()),
                    ("host", "oauth.yandex.ru"),
                ],
            )
            .map_err(|_| AuthError::Configuration)?
            .query()
            .ok_or(AuthError::Configuration)?
            .to_owned(),
        );
        let mut cookie_header =
            HeaderValue::from_str(&cookies.0).map_err(|_| AuthError::BrowserSessionRead)?;
        cookie_header.set_sensitive(true);
        let client = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(self.request_timeout())
            .user_agent("Karin/0.1 (native Linux music client)")
            .build()
            .map_err(|_| AuthError::Configuration)?;
        attempt.check_session_exchange()?;
        let mut response = client
            .post(self.token_endpoint())
            .header(COOKIE, cookie_header)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(form.as_bytes().to_vec())
            .send()
            .map_err(network_error)?;
        attempt.check_session_exchange()?;
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|length| length > RESPONSE_LIMIT)
        {
            return Err(AuthError::ResponseTooLarge);
        }
        let mut body = Zeroizing::new(Vec::new());
        response
            .by_ref()
            .take(RESPONSE_LIMIT + 1)
            .read_to_end(&mut body)
            .map_err(body_error)?;
        attempt.check_session_exchange()?;
        if body.len() as u64 > RESPONSE_LIMIT {
            return Err(AuthError::ResponseTooLarge);
        }
        if status != 200 {
            #[derive(Deserialize)]
            struct ErrorBody {
                error: Option<String>,
            }
            let error = serde_json::from_slice::<ErrorBody>(&body)
                .ok()
                .and_then(|body| body.error)
                .map(Zeroizing::new);
            let category = match error.as_deref().map(|value| value.as_str()) {
                Some("invalid_client") => TokenErrorCategory::InvalidClient,
                Some("invalid_grant") | Some("invalid_token") => TokenErrorCategory::InvalidGrant,
                Some("access_denied") => TokenErrorCategory::AccessDenied,
                Some("invalid_request") => TokenErrorCategory::InvalidRequest,
                Some("unsupported_grant_type") => TokenErrorCategory::UnsupportedGrant,
                Some("rate_limit_exceeded") | Some("too_many_requests") => {
                    TokenErrorCategory::RateLimited
                }
                _ if status == 429 => TokenErrorCategory::RateLimited,
                _ => TokenErrorCategory::Unknown,
            };
            return Err(AuthError::TokenExchange { status, category });
        }
        #[derive(Deserialize)]
        struct TokenBody {
            access_token: Option<String>,
            expires_in: Option<u64>,
            token_type: Option<String>,
        }
        let parsed: TokenBody =
            serde_json::from_slice(&body).map_err(|_| AuthError::TokenMalformed)?;
        let token = Zeroizing::new(parsed.access_token.ok_or(AuthError::TokenMalformed)?);
        if token.is_empty()
            || token.len() > 8192
            || !token.bytes().all(|b| (0x21..=0x7e).contains(&b))
            || parsed.token_type.is_some_and(|kind| {
                !kind.eq_ignore_ascii_case("bearer") && !kind.eq_ignore_ascii_case("oauth")
            })
        {
            return Err(AuthError::TokenMalformed);
        }
        if parsed.expires_in == Some(0) {
            return Err(AuthError::TokenExpired);
        }
        let expires_at = parsed
            .expires_in
            .map(|seconds| {
                Instant::now()
                    .checked_add(Duration::from_secs(seconds))
                    .ok_or(AuthError::TokenMalformed)
            })
            .transpose()?;
        attempt.check_session_exchange()?;
        Ok(AccessToken::new(token, expires_at))
    }
}

fn network_error(error: reqwest::Error) -> AuthError {
    if error.is_timeout() {
        AuthError::TokenTimeout
    } else {
        AuthError::TokenNetwork
    }
}

fn body_error(error: std::io::Error) -> AuthError {
    if error.kind() == std::io::ErrorKind::TimedOut
        || error
            .get_ref()
            .and_then(|cause| cause.downcast_ref::<reqwest::Error>())
            .is_some_and(reqwest::Error::is_timeout)
    {
        AuthError::TokenTimeout
    } else {
        AuthError::TokenNetwork
    }
}

#[cfg(test)]
mod tests;
