// SPDX-License-Identifier: GPL-3.0-or-later
//! Anonymous Yandex Music status probe. The API is unofficial and unstable.
//!
//! No OAuth credentials, cookies or account passwords are accepted or stored.
//! HTTP diagnostics are converted into domain models at this boundary.

use std::io::Read;
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use reqwest::redirect::Policy;
use serde::Deserialize;

use crate::application::MusicApi;
use crate::domain::{AccountAccess, ConnectionReport, ServiceError};

const STATUS_URL: &str = "https://api.music.yandex.net/account/status";
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Clone)]
pub struct YandexApi {
    client: Client,
    status_url: String,
    request_timeout: Duration,
}

impl YandexApi {
    pub fn new() -> Result<Self, ServiceError> {
        let client = Client::builder()
            .user_agent(concat!("Karin/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(5))
            .timeout(REQUEST_TIMEOUT)
            .redirect(Policy::none())
            .build()
            .map_err(|_| ServiceError::Configuration)?;
        Ok(Self {
            client,
            status_url: STATUS_URL.to_owned(),
            request_timeout: REQUEST_TIMEOUT,
        })
    }
}

impl MusicApi for YandexApi {
    fn check_connection(&self) -> Result<ConnectionReport, ServiceError> {
        let response = self
            .client
            .get(&self.status_url)
            // A request-level timeout reaches reqwest's async body timer. The
            // blocking client's timeout alone resets for individual reads.
            .timeout(self.request_timeout)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .map_err(classify_request_error)?;
        parse_status_response(response)
    }
}

fn classify_request_error(error: reqwest::Error) -> ServiceError {
    if error.is_timeout() {
        ServiceError::Timeout
    } else {
        // reqwest errors may contain URLs; do not pass them to UI or logs.
        ServiceError::Network
    }
}

fn classify_body_error(error: std::io::Error) -> ServiceError {
    let request_timed_out = error
        .get_ref()
        .and_then(|source| source.downcast_ref::<reqwest::Error>())
        .is_some_and(reqwest::Error::is_timeout);
    if error.kind() == std::io::ErrorKind::TimedOut || request_timed_out {
        ServiceError::Timeout
    } else {
        ServiceError::Network
    }
}

#[derive(Deserialize)]
struct StatusEnvelope {
    result: StatusResult,
}

#[derive(Deserialize)]
struct StatusResult {
    account: AnonymousAccount,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnonymousAccount {
    service_available: Option<bool>,
}

fn parse_status_response(mut response: Response) -> Result<ConnectionReport, ServiceError> {
    let http_status = response.status().as_u16();
    let access = match http_status {
        401 => AccountAccess::AuthorizationRequired,
        403 => AccountAccess::Forbidden,
        200 => AccountAccess::Anonymous,
        status => return Err(ServiceError::Http(status)),
    };
    if access != AccountAccess::Anonymous {
        return Ok(ConnectionReport {
            access,
            http_status,
            service_available: None,
        });
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES)
    {
        return Err(ServiceError::ResponseTooLarge);
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(classify_body_error)?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(ServiceError::ResponseTooLarge);
    }
    let envelope: StatusEnvelope =
        serde_json::from_slice(&bytes).map_err(|_| ServiceError::InvalidResponse)?;
    Ok(ConnectionReport {
        access,
        http_status,
        service_available: envelope.result.account.service_available,
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};

    use super::*;

    /// A local wire-level server; tests never contact Yandex or use real tokens.
    fn local_api(
        response: String,
        delay: Duration,
        timeout: Duration,
    ) -> (YandexApi, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0_u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                assert_eq!(stream.read(&mut byte).unwrap(), 1);
                request.extend_from_slice(&byte);
            }
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with("GET /account/status HTTP/1.1\r\n"));
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            assert!(!request.to_ascii_lowercase().contains("cookie:"));
            thread::sleep(delay);
            // The timeout test intentionally closes its client before this write.
            let _ = stream.write_all(response.as_bytes());
        });
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .timeout(timeout)
            .build()
            .unwrap();
        (
            YandexApi {
                client,
                status_url: format!("http://{address}/account/status"),
                request_timeout: timeout,
            },
            server,
        )
    }

    fn response(status: u16, body: &str) -> String {
        format!(
            "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn request(status: u16, body: &str) -> Result<ConnectionReport, ServiceError> {
        let (api, server) = local_api(
            response(status, body),
            Duration::ZERO,
            Duration::from_secs(2),
        );
        let result = api.check_connection();
        server.join().unwrap();
        result
    }

    #[test]
    fn anonymous_response_keeps_service_unavailable_flag() {
        let report = request(
            200,
            r#"{"result":{"account":{"now":"2026-10-24T00:00:00+00:00","region":10000,"serviceAvailable":false},"permissions":{"values":[]}}}"#,
        )
        .unwrap();
        assert_eq!(report.access, AccountAccess::Anonymous);
        assert_eq!(report.service_available, Some(false));
        assert_eq!(report.http_status, 200);
    }

    #[test]
    fn authorization_and_forbidden_responses_remain_distinct() {
        assert_eq!(
            request(401, "not JSON").unwrap().access,
            AccountAccess::AuthorizationRequired
        );
        assert_eq!(
            request(403, "not JSON").unwrap().access,
            AccountAccess::Forbidden
        );
    }

    #[test]
    fn legal_restrictions_and_server_errors_are_not_successes() {
        assert_eq!(request(451, "restricted"), Err(ServiceError::Http(451)));
        assert_eq!(request(503, "unavailable"), Err(ServiceError::Http(503)));
    }

    #[test]
    fn malformed_or_changed_success_schema_is_rejected() {
        for body in ["not JSON", "{}", r#"{"result":{"account":null}}"#] {
            assert_eq!(request(200, body), Err(ServiceError::InvalidResponse));
        }
    }

    #[test]
    fn oversized_response_is_rejected() {
        let body = "x".repeat(MAX_RESPONSE_BYTES as usize + 1);
        assert_eq!(request(200, &body), Err(ServiceError::ResponseTooLarge));
    }

    #[test]
    fn streamed_response_is_bounded_even_without_content_length() {
        let (api, server) = local_api(
            format!(
                "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}",
                "x".repeat(MAX_RESPONSE_BYTES as usize + 1)
            ),
            Duration::ZERO,
            Duration::from_secs(2),
        );
        assert_eq!(api.check_connection(), Err(ServiceError::ResponseTooLarge));
        server.join().unwrap();
    }

    #[test]
    fn disconnected_service_is_a_sanitized_network_error() {
        let (api, server) = local_api(String::new(), Duration::ZERO, Duration::from_secs(2));
        assert_eq!(api.check_connection(), Err(ServiceError::Network));
        server.join().unwrap();
    }

    #[test]
    fn redirects_are_not_followed() {
        let (api, server) = local_api(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/private\r\nContent-Length: 0\r\n\r\n".to_owned(),
            Duration::ZERO,
            Duration::from_secs(2),
        );
        assert_eq!(api.check_connection(), Err(ServiceError::Http(302)));
        server.join().unwrap();
    }

    #[test]
    fn slow_service_maps_to_timeout() {
        let (api, server) = local_api(
            response(200, "{}"),
            Duration::from_millis(500),
            Duration::from_millis(200),
        );
        assert_eq!(api.check_connection(), Err(ServiceError::Timeout));
        server.join().unwrap();
    }

    #[test]
    fn total_deadline_also_stops_a_slowly_streamed_body() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0_u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                assert_eq!(stream.read(&mut byte).unwrap(), 1);
                request.extend_from_slice(&byte);
            }
            let body = r#"{"result":{"account":{"serviceAvailable":false}}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            // Every chunk arrives well inside the per-read timeout. Only the
            // explicit request deadline can interrupt this slow-drip response.
            for byte in body.bytes() {
                if stream.write_all(&[byte]).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(40));
            }
        });
        let api = YandexApi {
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
            status_url: format!("http://{address}/account/status"),
            request_timeout: Duration::from_millis(200),
        };
        assert_eq!(api.check_connection(), Err(ServiceError::Timeout));
        server.join().unwrap();
    }
}
