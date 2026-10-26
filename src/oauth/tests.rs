// SPDX-License-Identifier: GPL-3.0-or-later
//! Loopback fixtures only. Every account/code/token marker below is synthetic.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};

use super::*;

const CLIENT_ID: &str = "fixture-karin-owned-client";
const CODE: &str = "7654321";
const TOKEN: &str = "fixture-private-access-token";
const FULL_TOKEN: &str = r#"{"access_token":"fixture-private-access-token","refresh_token":"fixture-private-refresh-token","token_type":"bearer","expires_in":3600,"scope":"music:api-public music:major-public-api login:info login:avatar"}"#;
const IDENTITY: &str = r#"{"client_id":"fixture-karin-owned-client","id":"1234","login":"fixture-private-login","display_name":"fixture-private-person"}"#;
const ACCOUNT: &str = r#"{"result":{"account":{"uid":1234,"login":"fixture-private-login","serviceAvailable":true}}}"#;
const CATALOG: &str = r#"{"result":{"text":"Beethoven","tracks":{"total":1,"results":[{"title":"fixture-private-title"}]}}}"#;

type Fixture = (u16, &'static str);

// Every endpoint stays on loopback, including paths that an unexpected success
// or future regression might reach. A negative fixture must never fall through
// to Yandex with its synthetic token.
fn isolated_client() -> DiagnosticClient {
    DiagnosticClient {
        client: client_builder().no_proxy().build().unwrap(),
        token_url: "http://127.0.0.1:1/token".to_owned(),
        identity_url: "http://127.0.0.1:1/info?format=json".to_owned(),
        status_url: "http://127.0.0.1:1/account/status".to_owned(),
        catalog_url: "http://127.0.0.1:1/search".to_owned(),
        timeout: Duration::from_secs(2),
    }
}
fn response(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
fn request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
        assert_eq!(stream.read(&mut byte).unwrap(), 1);
        bytes.push(byte[0]);
        assert!(bytes.len() < 32 * 1024);
    }
    let headers = String::from_utf8(bytes.clone()).unwrap();
    let length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    bytes.extend_from_slice(&body);
    String::from_utf8(bytes).unwrap()
}
fn fixture(responses: Vec<Fixture>) -> (DiagnosticClient, JoinHandle<Vec<String>>, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let retained = listener.try_clone().unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let deadline = Instant::now() + Duration::from_secs(3);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(value) => break value,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("expected fixture request: {error}"),
                }
            };
            requests.push(request(&mut stream));
            stream.write_all(response(status, body).as_bytes()).unwrap();
        }
        requests
    });
    let timeout = Duration::from_secs(2);
    (
        DiagnosticClient {
            client: client_builder()
                .no_proxy()
                .timeout(timeout)
                .build()
                .unwrap(),
            token_url: format!("http://{address}/token"),
            identity_url: format!("http://{address}/info?format=json"),
            status_url: format!("http://{address}/account/status"),
            catalog_url: format!(
                "http://{address}/search?text=Beethoven&type=track&page=0&nocorrect=true"
            ),
            timeout,
        },
        handle,
        retained,
    )
}
fn manual() -> AuthorizationResponse {
    AuthorizationResponse::ManualCode(Zeroizing::new(CODE.to_owned()))
}
fn attempt() -> AuthorizationAttempt {
    AuthorizationAttempt::new(CLIENT_ID).unwrap()
}
fn finish(responses: Vec<Fixture>) -> (DiagnosticReport, Vec<String>) {
    let (client, handle, retained) = fixture(responses);
    let report = attempt().complete_with(manual(), &client);
    let requests = handle.join().unwrap();
    retained.set_nonblocking(true).unwrap();
    assert_eq!(
        retained.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "no implicit retry or later probe"
    );
    (report, requests)
}
fn no_sensitive_report(report: &DiagnosticReport) {
    let text = format!("{report:?}\n{report}");
    for value in [
        CODE,
        TOKEN,
        "fixture-private-refresh-token",
        "fixture-private-account",
        "fixture-private-login",
        "fixture-private-person",
        "fixture-private-title",
        CLIENT_ID,
    ] {
        assert!(!text.contains(value), "report leaked fixture marker");
    }
}

#[test]
fn pkce_matches_rfc7636_and_authorization_has_only_own_scopes() {
    assert_eq!(
        pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    let first = attempt();
    let second = attempt();
    assert_eq!(first.verifier.len(), 43);
    assert_eq!(first.state.len(), 43);
    assert_ne!(*first.verifier, *second.verifier);
    assert_ne!(*first.state, *second.state);
    assert_ne!(*first.verifier, *first.state);
    let url = first.authorization_url();
    let parsed = reqwest::Url::parse(&url).unwrap();
    assert_eq!(
        parsed.origin().ascii_serialization(),
        "https://oauth.yandex.ru"
    );
    assert_eq!(parsed.path(), "/authorize");
    let query: std::collections::HashMap<_, _> = parsed.query_pairs().collect();
    assert_eq!(query["response_type"], "code");
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["scope"], REQUIRED_SCOPES.join(" "));
    assert_eq!(query["redirect_uri"], VERIFICATION_REDIRECT);
    assert_eq!(query["code_challenge"], pkce_challenge(&first.verifier));
    assert_eq!(query["state"], *first.state);
    assert!(!url.contains("client_secret"));
    assert!(!url.contains(&*first.verifier));
    assert!(!format!("{first:?}").contains(&*first.verifier));
}

#[test]
fn full_diagnostic_is_client_bound_sequential_and_secretless() {
    let (report, requests) = finish(vec![
        (200, FULL_TOKEN),
        (200, IDENTITY),
        (200, ACCOUNT),
        (200, CATALOG),
    ]);
    assert!(report.passed());
    assert_eq!(report.returned_state, ReturnedState::NotObservedManualCode);
    assert_eq!(report.scopes, Some(ScopeEvidence::ReturnedRequiredScopes));
    assert_eq!(report.client_matches, Some(true));
    assert_eq!(report.identity_account_present, Some(true));
    assert_eq!(report.music_account_present, Some(true));
    assert_eq!(report.service_available, Some(true));
    assert!(requests[0].starts_with("POST /token HTTP/1.1"));
    let form: std::collections::HashMap<_, _> = reqwest::Url::parse(&format!(
        "http://fixture/?{}",
        requests[0].split("\r\n\r\n").nth(1).unwrap()
    ))
    .unwrap()
    .query_pairs()
    .map(|(k, v)| (k.into_owned(), v.into_owned()))
    .collect();
    assert_eq!(form["client_id"], CLIENT_ID);
    assert_eq!(form["code"], CODE);
    assert_eq!(form["grant_type"], "authorization_code");
    assert_eq!(form["redirect_uri"], VERIFICATION_REDIRECT);
    assert_eq!(form["code_verifier"].len(), 43);
    assert!(!requests[0].contains("client_secret"));
    assert!(!requests[0].to_ascii_lowercase().contains("authorization:"));
    assert!(requests[1].starts_with("GET /info?format=json HTTP/1.1"));
    assert!(requests[2].starts_with("GET /account/status HTTP/1.1"));
    assert!(
        requests[3]
            .starts_with("GET /search?text=Beethoven&type=track&page=0&nocorrect=true HTTP/1.1")
    );
    for request in &requests[1..] {
        assert!(request.contains(&format!("authorization: OAuth {TOKEN}\r\n")));
        assert!(!request.lines().next().unwrap().contains(TOKEN));
        assert!(!request.to_ascii_lowercase().contains("cookie:"));
    }
    no_sensitive_report(&report);
}

#[test]
fn omitted_scope_uses_documented_provider_semantics_but_subset_stops() {
    let omitted = r#"{"access_token":"fixture-private-access-token","token_type":"bearer","expires_in":3600}"#;
    let (report, _) = finish(vec![
        (200, omitted),
        (200, IDENTITY),
        (200, ACCOUNT),
        (200, CATALOG),
    ]);
    assert!(report.passed());
    assert_eq!(
        report.scopes,
        Some(ScopeEvidence::OmittedMeansRequestedGrant)
    );
    for scope in [
        "login:info login:avatar",
        "music:api-public music:major-public-api",
        "",
    ] {
        let body = format!(
            "{{\"access_token\":\"{TOKEN}\",\"token_type\":\"bearer\",\"expires_in\":3600,\"scope\":\"{scope}\"}}"
        );
        // Static fixture lifetimes are avoided by directly exercising the real
        // parser through an owned local response below.
        let report = one_response(200, body);
        assert_eq!(report.token.error, Some(ErrorCategory::MissingScopes));
        assert!(report.identity.is_none());
    }
}
fn one_response(status: u16, body: String) -> DiagnosticReport {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        request(&mut stream);
        stream
            .write_all(response(status, &body).as_bytes())
            .unwrap();
    });
    let mut client = isolated_client();
    client.client = client_builder().no_proxy().build().unwrap();
    client.token_url = format!("http://{address}/token");
    let report = attempt().complete_with(manual(), &client);
    server.join().unwrap();
    report
}

#[test]
fn identity_client_mismatch_or_absence_prevents_music_requests() {
    for body in [
        r#"{"client_id":"fixture-other-client","id":"1234"}"#,
        r#"{"client_id":"fixture-karin-owned-client","id":""}"#,
        r#"{"client_id":"fixture-karin-owned-client","id":null}"#,
        r#"{"oauth_client_id":"fixture-karin-owned-client","id":"1234"}"#,
    ] {
        let (report, requests) = finish(vec![(200, FULL_TOKEN), (200, body)]);
        assert!(!report.passed());
        assert!(report.music_account.is_none());
        assert!(report.catalog.is_none());
        assert_eq!(requests.len(), 2);
        no_sensitive_report(&report);
    }
}

#[test]
fn anonymous_music_account_never_becomes_signed_in_and_stops_catalog() {
    let (report, requests) = finish(vec![
        (200, FULL_TOKEN),
        (200, IDENTITY),
        (200, r#"{"result":{"account":{"serviceAvailable":false}}}"#),
    ]);
    assert_eq!(report.music_account_present, Some(false));
    assert_eq!(report.service_available, Some(false));
    assert_eq!(
        report.music_account.unwrap().error,
        Some(ErrorCategory::AccountAbsent)
    );
    assert_eq!(requests.len(), 3);
    assert!(report.catalog.is_none());
}

#[test]
fn oauth_expiry_denial_bad_pkce_and_scope_errors_are_redacted() {
    for (error, category) in [
        ("invalid_grant", ErrorCategory::CodeInvalidOrExpired),
        ("access_denied", ErrorCategory::AccessDenied),
        ("invalid_request", ErrorCategory::OAuthRequestRejected),
        ("invalid_scope", ErrorCategory::MissingScopes),
        ("invalid_client", ErrorCategory::ClientRejected),
    ] {
        let report = one_response(
            400,
            format!(
                "{{\"error\":\"{error}\",\"error_description\":\"{CODE} {TOKEN} fixture-private-person\"}}"
            ),
        );
        assert_eq!(report.token.error, Some(category));
        assert_eq!(report.token.http_status, Some(400));
        assert!(report.identity.is_none());
        no_sensitive_report(&report);
    }
}

#[test]
fn wrong_verifier_is_rejected_by_fixture_without_fallback() {
    let original = attempt();
    let challenge = pkce_challenge(&original.verifier);
    let mut tampered = original;
    tampered.verifier = Zeroizing::new("wrong-verifier-for-fixture".to_owned());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let req = request(&mut stream);
        let form = req.split("\r\n\r\n").nth(1).unwrap();
        let parsed = reqwest::Url::parse(&format!("http://fixture/?{form}")).unwrap();
        let verifier = parsed
            .query_pairs()
            .find(|(key, _)| key == "code_verifier")
            .unwrap()
            .1
            .into_owned();
        assert_ne!(pkce_challenge(&verifier), challenge);
        stream
            .write_all(
                response(
                    400,
                    r#"{"error":"invalid_grant","error_description":"fixture-invalid-pkce"}"#,
                )
                .as_bytes(),
            )
            .unwrap();
    });
    let mut client = isolated_client();
    client.client = client_builder().no_proxy().build().unwrap();
    client.token_url = format!("http://{address}/token");
    let report = tampered.complete_with(manual(), &client);
    server.join().unwrap();
    assert_eq!(
        report.token.error,
        Some(ErrorCategory::CodeInvalidOrExpired)
    );
    assert!(report.identity.is_none());
}

#[test]
fn state_is_validated_only_for_observed_return_and_denial_is_distinct() {
    let current = attempt();
    for state in ["", "wrong-state"] {
        let url = Zeroizing::new(format!("{VERIFICATION_REDIRECT}?code={CODE}&state={state}"));
        assert_eq!(
            current
                .authorization_code(AuthorizationResponse::ReturnedRedirect(url))
                .unwrap_err(),
            ErrorCategory::StateMismatch
        );
    }
    for url in [
        format!("{VERIFICATION_REDIRECT}?code={CODE}"),
        format!(
            "{VERIFICATION_REDIRECT}?code={CODE}&state={}&state={}",
            *current.state, *current.state
        ),
    ] {
        assert!(
            current
                .authorization_code(AuthorizationResponse::ReturnedRedirect(Zeroizing::new(url)))
                .is_err()
        );
    }
    let denied = Zeroizing::new(format!(
        "{VERIFICATION_REDIRECT}?error=access_denied&state={}",
        *current.state
    ));
    assert_eq!(
        current
            .authorization_code(AuthorizationResponse::ReturnedRedirect(denied))
            .unwrap_err(),
        ErrorCategory::AccessDenied
    );
    let valid = Zeroizing::new(format!(
        "{VERIFICATION_REDIRECT}?code={CODE}&state={}",
        *current.state
    ));
    assert_eq!(
        &*current
            .authorization_code(AuthorizationResponse::ReturnedRedirect(valid))
            .unwrap(),
        CODE
    );
    let evil = Zeroizing::new(format!(
        "https://other.invalid/verification_code?code={CODE}&state={}",
        *current.state
    ));
    assert_eq!(
        current
            .authorization_code(AuthorizationResponse::ReturnedRedirect(evil))
            .unwrap_err(),
        ErrorCategory::InvalidReturn
    );
    // Manual code is session-bound by verifier, with no claimed state roundtrip.
    assert_eq!(&*current.authorization_code(manual()).unwrap(), CODE);
}

#[test]
fn expired_cancelled_and_invalid_input_stop_before_transport() {
    let mut expired = attempt();
    expired.created = Instant::now() - CODE_LIFETIME;
    assert_eq!(
        expired.authorization_code(manual()).unwrap_err(),
        ErrorCategory::AttemptExpired
    );
    assert_eq!(
        attempt()
            .authorization_code(AuthorizationResponse::Cancelled)
            .unwrap_err(),
        ErrorCategory::Cancelled
    );
    assert_eq!(
        attempt()
            .authorization_code(AuthorizationResponse::ManualCode(Zeroizing::new(
                String::new()
            )))
            .unwrap_err(),
        ErrorCategory::Cancelled
    );
    for code in [
        "https://fixture.invalid/#access_token=fixture-secret".to_owned(),
        "two values".to_owned(),
        "x".repeat(MAX_INPUT + 1),
    ] {
        assert_eq!(
            attempt()
                .authorization_code(AuthorizationResponse::ManualCode(Zeroizing::new(code)))
                .unwrap_err(),
            ErrorCategory::InvalidInput
        );
    }
    for id in ["", "a b", "https://fixture.invalid"] {
        assert_eq!(
            AuthorizationAttempt::new(id).unwrap_err(),
            ErrorCategory::InvalidInput
        );
    }
}

#[test]
fn service_http_categories_never_follow_redirects_retry_or_claim_scope_cause() {
    for (status, error) in [
        (401, ErrorCategory::AccessDenied),
        (403, ErrorCategory::Forbidden),
        (451, ErrorCategory::Http),
        (302, ErrorCategory::Http),
        (500, ErrorCategory::Http),
    ] {
        let (report, requests) = finish(vec![
            (200, FULL_TOKEN),
            (200, IDENTITY),
            (status, "fixture-private-person"),
        ]);
        assert_eq!(
            report.music_account.unwrap(),
            ProbeResult::failure(Some(status), error)
        );
        assert_eq!(requests.len(), 3);
        assert!(report.catalog.is_none());
        no_sensitive_report(&report);
    }
}

#[test]
fn malformed_oversized_and_error_envelopes_never_pass() {
    for body in [
        "not json".to_owned(),
        "{}".to_owned(),
        "x".repeat(MAX_BODY + 1),
        r#"{"access_token":"fixture-private-access-token","token_type":"bearer","expires_in":0}"#
            .to_owned(),
    ] {
        let report = one_response(200, body);
        assert!(report.token.error.is_some());
        assert!(report.identity.is_none());
        no_sensitive_report(&report);
    }
    for body in [
        r#"{"error":{"name":"fixture-error"}}"#,
        r#"{"result":null}"#,
        r#"{}"#,
    ] {
        let (report, _) = finish(vec![
            (200, FULL_TOKEN),
            (200, IDENTITY),
            (200, ACCOUNT),
            (200, body),
        ]);
        assert_eq!(
            report.catalog.unwrap().error,
            Some(ErrorCategory::InvalidResponse)
        );
        assert!(!report.passed());
    }
}

#[test]
fn disconnect_timeout_and_sensitive_headers_remain_safe() {
    for timeout_case in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            request(&mut stream);
            if timeout_case {
                thread::sleep(Duration::from_secs(1));
            }
        });
        let mut client = isolated_client();
        client.client = client_builder().no_proxy().build().unwrap();
        client.timeout = if timeout_case {
            Duration::from_millis(500)
        } else {
            Duration::from_secs(2)
        };
        client.token_url = format!("http://{address}/token");
        let report = attempt().complete_with(manual(), &client);
        server.join().unwrap();
        assert_eq!(
            report.token.error,
            Some(if timeout_case {
                ErrorCategory::Timeout
            } else {
                ErrorCategory::Network
            })
        );
        no_sensitive_report(&report);
    }
}

#[test]
fn null_scope_is_schema_drift_not_documented_omission() {
    let report = one_response(200, r#"{"access_token":"fixture-private-access-token","token_type":"bearer","expires_in":3600,"scope":null}"#.to_owned());
    assert_eq!(report.token.error, Some(ErrorCategory::InvalidResponse));
    assert!(report.identity.is_none());
}

#[test]
fn actual_location_is_not_followed_and_token_is_never_forwarded() {
    let alternate = TcpListener::bind("127.0.0.1:0").unwrap();
    let alternate_address = alternate.local_addr().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let retained = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        request(&mut stream);
        stream
            .write_all(response(200, FULL_TOKEN).as_bytes())
            .unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let sent = request(&mut stream);
        assert!(sent.contains(TOKEN));
        write!(stream, "HTTP/1.1 302 Found\r\nLocation: http://{alternate_address}/capture\r\nSet-Cookie: fixture-private-cookie=synthetic\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let mut client = isolated_client();
    client.client = client_builder().no_proxy().build().unwrap();
    client.token_url = format!("http://{address}/token");
    client.identity_url = format!("http://{address}/info");
    let report = attempt().complete_with(manual(), &client);
    server.join().unwrap();
    assert_eq!(
        report.identity.unwrap(),
        ProbeResult::failure(Some(302), ErrorCategory::Http)
    );
    assert!(report.music_account.is_none());
    for listener in [retained, alternate] {
        listener.set_nonblocking(true).unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    no_sensitive_report(&report);
}

#[test]
fn expired_memory_token_cannot_start_an_http_request() {
    let client = isolated_client();
    let token = Token {
        access: Zeroizing::new(TOKEN.to_owned()),
        expires_at: Instant::now() - Duration::from_secs(1),
        scopes: ScopeEvidence::ReturnedRequiredScopes,
        http_status: 200,
    };
    assert_eq!(
        client
            .authorized_get("http://127.0.0.1:1/not-contacted", &token)
            .err(),
        Some(ProbeResult::failure(None, ErrorCategory::TokenExpired))
    );
}

#[test]
fn total_deadline_bounds_a_continuously_streaming_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            FULL_TOKEN.len()
        )
        .unwrap();
        for byte in FULL_TOKEN.bytes() {
            if stream.write_all(&[byte]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(15));
        }
    });
    let mut client = isolated_client();
    client.client = client_builder().no_proxy().build().unwrap();
    client.timeout = Duration::from_millis(500);
    client.token_url = format!("http://{address}/token");
    let report = attempt().complete_with(manual(), &client);
    server.join().unwrap();
    assert_eq!(
        report.token,
        ProbeResult::failure(Some(200), ErrorCategory::Timeout)
    );
    assert!(report.identity.is_none());
    no_sensitive_report(&report);
}
