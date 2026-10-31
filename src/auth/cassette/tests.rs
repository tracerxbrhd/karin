// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic loopback contracts only; no provider, user accounts or credentials.

use super::*;
use crate::auth::{BrowserMode, NavigationDecision};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
};

const SESSION: &str = "synthetic-private-session";
const TOKEN: &str = "synthetic-private-token";

fn cookies() -> SessionCookies {
    SessionCookies::from_pairs([
        ("Session_id".into(), SESSION.into()),
        ("yandexuid".into(), "synthetic-id".into()),
    ])
    .unwrap()
}

fn returned(attempt: &AuthAttempt) -> String {
    format!(
        "{}#state={}",
        attempt.callback_uri(),
        attempt.state.as_str()
    )
}

fn fixture_attempt(origin: &str) -> Arc<AuthAttempt> {
    AuthAttempt::for_test_session(
        &format!("{origin}/authorize"),
        &format!("{origin}/complete"),
    )
    .unwrap()
}

struct Fixture {
    origin: String,
    request: mpsc::Receiver<String>,
    worker: thread::JoinHandle<()>,
}

impl Fixture {
    fn new(status: u16, body: String, headers: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (sender, request) = mpsc::sync_channel(1);
        let headers = headers.to_owned();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let request = read_request(&mut stream);
            sender.send(request).unwrap();
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
                body.len()
            );
            // An oversized response may be deliberately rejected before its body.
            let _ = stream.write_all(response.as_bytes());
        });
        Self {
            origin,
            request,
            worker,
        }
    }

    fn exchange(self) -> (Result<AccessToken, AuthError>, String) {
        let attempt = fixture_attempt(&self.origin);
        attempt
            .complete_session_return(&returned(&attempt))
            .unwrap();
        let provider = CassetteProvider::for_test(&format!("{}/token", self.origin));
        let result = provider.exchange_session(&attempt, cookies());
        let request = self.request.recv_timeout(Duration::from_secs(3)).unwrap();
        self.worker.join().unwrap();
        (result, request)
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
        assert!(bytes.len() < 32768);
        if bytes.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let headers = String::from_utf8(bytes.clone()).unwrap();
    let length: usize = headers
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| value.trim().parse().unwrap())
        })
        .unwrap();
    let start = bytes.len();
    bytes.resize(start + length, 0);
    stream.read_exact(&mut bytes[start..]).unwrap();
    String::from_utf8(bytes).unwrap()
}

#[test]
fn public_provider_prepares_live_cassette_flow_without_requesting_account() {
    let first = super::super::public_provider().begin().unwrap();
    let second = super::super::public_provider().begin().unwrap();
    assert!(first.is_session_based());
    assert_eq!(first.browser_mode(), BrowserMode::Embedded);
    assert_eq!(first.callback_uri(), "https://music.yandex.ru/");
    assert_eq!(first.session_cookie_uri(), TOKEN_ENDPOINT);
    assert_ne!(first.state, second.state);
    let url = reqwest::Url::parse(&first.authorization_url()).unwrap();
    assert_eq!(
        url.origin().ascii_serialization(),
        "https://oauth.yandex.ru"
    );
    assert_eq!(url.path(), "/authorize");
    let pairs = url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(pairs.get("response_type").unwrap(), "token");
    assert_eq!(pairs.get("state").unwrap(), first.state.as_str());
    assert_eq!(pairs.len(), 3);
    assert!(first.listener.lock().unwrap().is_none());
    assert!(
        first.verifier().is_empty(),
        "implicit/sessionid flow must not claim PKCE"
    );
    assert!(!format!("{first:?}").contains(first.state.as_str()));
}

#[test]
fn session_return_is_nonconsuming_candidate_then_exact_state_checked_single_use() {
    let attempt = fixture_attempt("http://127.0.0.1:12345");
    let returned = returned(&attempt);
    for _ in 0..2 {
        assert!(matches!(
            attempt.navigation(&returned),
            NavigationDecision::SessionReturn
        ));
    }
    assert!(attempt.complete_session_return(&returned).is_ok());
    assert_eq!(
        attempt.complete_session_return(&returned),
        Err(AuthError::InvalidCallback)
    );
}

#[test]
fn session_return_rejects_missing_mismatched_duplicate_and_encoded_duplicate_states() {
    for (suffix, expected) in [
        (
            "#access_token=synthetic-discarded",
            AuthError::SessionStateMissing,
        ),
        ("#state=wrong", AuthError::SessionStateMismatch),
        ("?state=STATE#state=STATE", AuthError::InvalidCallback),
        ("#state=STATE&%73tate=STATE", AuthError::InvalidCallback),
        ("#state=STATE&other=%0a", AuthError::InvalidCallback),
        ("#state=STATE&other=%", AuthError::InvalidCallback),
        (
            "#state=STATE&error=access_denied&access_token=synthetic",
            AuthError::InvalidCallback,
        ),
    ] {
        let attempt = fixture_attempt("http://127.0.0.1:12345");
        let returned = format!(
            "{}{}",
            attempt.callback_uri(),
            suffix.replace("STATE", &attempt.state)
        );
        assert_eq!(attempt.complete_session_return(&returned), Err(expected));
    }
}

#[test]
fn production_session_return_rejects_lookalikes_ports_paths_and_normalization() {
    let attempt = CassetteProvider::new().begin().unwrap();
    for uri in [
        "https://music.yandex.ru.attacker.invalid/",
        "http://music.yandex.ru/",
        "https://music.yandex.ru:444/",
        "https://music.yandex.ru@attacker.invalid/",
        "https://music.yandex.ru/other",
        "https://music.yandex.ru/../",
        "https://music.yandex.ru/%2e/",
        "https://music.yandex.ru\\@attacker.invalid/",
        "https://music.yandex.com/",
    ] {
        assert_eq!(
            attempt.complete_session_return(uri),
            Err(AuthError::InvalidCallback)
        );
    }
    let returned = format!(
        "https://music.yandex.ru/?state={}#access_token=synthetic-discarded&expires_in=3600",
        attempt.state.as_str()
    );
    assert_eq!(attempt.complete_session_return(&returned), Ok(()));
}

#[test]
fn session_denial_expiry_and_cancellation_are_separate_and_terminal() {
    let denied = fixture_attempt("http://127.0.0.1:12345");
    let uri = format!(
        "{}#state={}&error=access_denied",
        denied.callback_uri(),
        denied.state.as_str()
    );
    assert_eq!(denied.complete_session_return(&uri), Err(AuthError::Denied));
    assert_eq!(
        denied.complete_session_return(&uri),
        Err(AuthError::InvalidCallback)
    );
    let cancelled = fixture_attempt("http://127.0.0.1:12345");
    cancelled.cancel();
    assert_eq!(
        cancelled.complete_session_return(&returned(&cancelled)),
        Err(AuthError::Cancelled)
    );
    let mut expired = fixture_attempt("http://127.0.0.1:12345");
    Arc::get_mut(&mut expired).unwrap().created = Instant::now() - super::super::LIFETIME;
    assert_eq!(
        expired.complete_session_return(&returned(&expired)),
        Err(AuthError::Expired)
    );
}

#[test]
fn cookie_material_is_bounded_private_and_rejects_injection_duplicates_or_missing_session() {
    assert_eq!(format!("{:?}", cookies()), "SessionCookies([redacted])");
    for pairs in [
        vec![],
        vec![("Session_id", "")],
        vec![("other", "value")],
        vec![("Session_id", "value\r\nInjected: yes")],
        vec![("Session_id", "value; another=secret")],
        vec![("Session_id", "value"), ("Session_id", "other")],
        vec![("bad name", "value")],
    ] {
        assert!(
            SessionCookies::from_pairs(pairs.into_iter().map(|(k, v)| (k.into(), v.into())))
                .is_err()
        );
    }
    assert!(SessionCookies::from_pairs([("Session_id".into(), "x".repeat(8193))]).is_err());
}

#[test]
fn session_token_exchange_matches_cassette_contract_and_yields_memory_token() {
    let (result, request) = Fixture::new(200, format!(r#"{{"access_token":"{TOKEN}","expires_in":3600,"token_type":"bearer","refresh_token":"synthetic-discarded"}}"#), "").exchange();
    let token = result.unwrap();
    assert_eq!(token.expose(), TOKEN);
    assert!(!token.is_expired());
    assert_eq!(format!("{token:?}"), "AccessToken([redacted])");
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("POST /token HTTP/1.1\r\n"));
    assert!(headers.to_ascii_lowercase().contains(
        &format!("cookie: Session_id={SESSION}; yandexuid=synthetic-id").to_ascii_lowercase()
    ));
    assert!(
        !headers
            .to_ascii_lowercase()
            .contains("x-yandex-music-client")
    );
    let pairs = reqwest::Url::parse(&format!("http://localhost/?{body}"))
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(pairs.len(), 4);
    assert_eq!(pairs.get("grant_type").unwrap(), "sessionid");
    assert_eq!(pairs.get("host").unwrap(), "oauth.yandex.ru");
    assert_eq!(pairs.get("client_id").unwrap(), "synthetic-cassette-client");
    assert_eq!(
        pairs.get("client_secret").unwrap(),
        "synthetic-public-app-configuration"
    );
}

#[test]
fn token_error_categories_never_echo_provider_messages_or_personal_fields() {
    for (error, category) in [
        ("invalid_client", TokenErrorCategory::InvalidClient),
        ("invalid_grant", TokenErrorCategory::InvalidGrant),
        ("access_denied", TokenErrorCategory::AccessDenied),
        (
            "unsupported_grant_type",
            TokenErrorCategory::UnsupportedGrant,
        ),
        ("synthetic-private-token", TokenErrorCategory::Unknown),
    ] {
        let (result, _) = Fixture::new(
            403,
            format!(r#"{{"error":"{error}","error_description":"{TOKEN}","account":"{SESSION}"}}"#),
            "",
        )
        .exchange();
        let actual = result.unwrap_err();
        assert_eq!(
            actual,
            AuthError::TokenExchange {
                status: 403,
                category
            }
        );
        let safe = format!("{actual} {actual:?}");
        assert!(!safe.contains(TOKEN));
        assert!(!safe.contains(SESSION));
        assert!(safe.contains("403"));
    }
}

#[test]
fn redirects_are_never_followed_and_cookies_never_sent_to_redirect_target() {
    let redirected = TcpListener::bind("127.0.0.1:0").unwrap();
    redirected.set_nonblocking(true).unwrap();
    let headers = format!(
        "Location: http://{}/stolen\r\n",
        redirected.local_addr().unwrap()
    );
    let (result, _) = Fixture::new(302, "{}".into(), &headers).exchange();
    assert_eq!(
        result.unwrap_err(),
        AuthError::TokenExchange {
            status: 302,
            category: TokenErrorCategory::Unknown
        }
    );
    assert_eq!(
        redirected.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn malformed_expired_and_oversized_token_responses_are_rejected() {
    for (body, error) in [
        ("not-json".into(), AuthError::TokenMalformed),
        ("{}".into(), AuthError::TokenMalformed),
        (
            r#"{"access_token":"","expires_in":3600}"#.into(),
            AuthError::TokenMalformed,
        ),
        (
            r#"{"access_token":"synthetic","expires_in":0}"#.into(),
            AuthError::TokenExpired,
        ),
        (
            r#"{"access_token":"synthetic","expires_in":-1}"#.into(),
            AuthError::TokenMalformed,
        ),
        (
            r#"{"access_token":"synthetic","token_type":"unexpected"}"#.into(),
            AuthError::TokenMalformed,
        ),
        (
            "x".repeat(RESPONSE_LIMIT as usize + 1),
            AuthError::ResponseTooLarge,
        ),
    ] {
        let (result, _) = Fixture::new(200, body, "").exchange();
        assert_eq!(result.unwrap_err(), error);
    }
}

#[test]
fn uncompleted_cancelled_expired_and_replayed_exchange_do_not_send_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let provider = CassetteProvider::for_test(&format!("{origin}/token"));
    let attempt = fixture_attempt(&origin);
    assert_eq!(
        provider.exchange_session(&attempt, cookies()).unwrap_err(),
        AuthError::InvalidCallback
    );
    attempt
        .complete_session_return(&returned(&attempt))
        .unwrap();
    attempt.cancel();
    assert_eq!(
        provider.exchange_session(&attempt, cookies()).unwrap_err(),
        AuthError::Cancelled
    );
    let denied = fixture_attempt(&origin);
    let denial = format!(
        "{}#state={}&error=access_denied",
        denied.callback_uri(),
        denied.state.as_str()
    );
    assert_eq!(
        denied.complete_session_return(&denial),
        Err(AuthError::Denied)
    );
    assert_eq!(
        provider.exchange_session(&denied, cookies()).unwrap_err(),
        AuthError::InvalidCallback
    );
    let mut expired = fixture_attempt(&origin);
    expired
        .complete_session_return(&returned(&expired))
        .unwrap();
    Arc::get_mut(&mut expired).unwrap().created = Instant::now() - super::super::LIFETIME;
    assert_eq!(
        provider.exchange_session(&expired, cookies()).unwrap_err(),
        AuthError::Expired
    );
    let replay = fixture_attempt(&origin);
    replay.complete_session_return(&returned(&replay)).unwrap();
    replay.claim_session_exchange().unwrap();
    assert_eq!(
        provider.exchange_session(&replay, cookies()).unwrap_err(),
        AuthError::InvalidCallback
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn unavailable_token_endpoint_reports_exchange_network_stage_without_url_or_cookies() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let attempt = fixture_attempt(&origin);
    attempt
        .complete_session_return(&returned(&attempt))
        .unwrap();
    let provider = CassetteProvider::for_test(&format!("{origin}/token"));
    let error = provider.exchange_session(&attempt, cookies()).unwrap_err();
    assert_eq!(error, AuthError::TokenNetwork);
    assert!(!format!("{error} {error:?}").contains(&origin));
}

#[test]
fn cancellation_during_exchange_discards_late_token() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let attempt = fixture_attempt(&origin);
    attempt
        .complete_session_return(&returned(&attempt))
        .unwrap();
    let provider = CassetteProvider::for_test(&format!("{origin}/token"));
    let (received_tx, received_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request(&mut stream);
        received_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let body = format!(r#"{{"access_token":"{TOKEN}"}}"#);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let worker_attempt = attempt.clone();
    let worker = thread::spawn(move || provider.exchange_session(&worker_attempt, cookies()));
    received_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    attempt.cancel();
    release_tx.send(()).unwrap();
    assert_eq!(worker.join().unwrap().unwrap_err(), AuthError::Cancelled);
    server.join().unwrap();
}

#[test]
fn slow_token_headers_and_body_report_timeout_at_exchange_stage() {
    for send_headers in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let attempt = fixture_attempt(&origin);
        attempt
            .complete_session_return(&returned(&attempt))
            .unwrap();
        let mut provider = CassetteProvider::for_test(&format!("{origin}/token"));
        provider.timeout = Duration::from_millis(200);
        let (request_tx, request_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            if send_headers {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
                stream.flush().unwrap();
            }
            request_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        });
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            result_tx
                .send(provider.exchange_session(&attempt, cookies()))
                .unwrap();
        });
        request_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let result = result_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        release_tx.send(()).unwrap();
        server.join().unwrap();
        worker.join().unwrap();
        assert_eq!(result.unwrap_err(), AuthError::TokenTimeout);
    }
}
