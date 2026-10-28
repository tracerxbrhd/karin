// SPDX-License-Identifier: GPL-3.0-or-later
//! Loopback fixtures only. Every account/code/token marker below is synthetic.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};

use super::*;

const CLIENT_ID: &str = "fixture-karin-owned-client";
const CODE: &str = "fixtureOpaqueCode2026LongerThanSevenDigits";
const TOKEN: &str = "fixture-private-access-token";
const FULL_TOKEN: &str = r#"{"access_token":"fixture-private-access-token","refresh_token":"fixture-private-refresh-token","token_type":"bearer","expires_in":3600,"scope":"music:api-public music:major-public-api login:info login:avatar"}"#;
const IDENTITY: &str = r#"{"client_id":"fixture-karin-owned-client","id":"1234","login":"fixture-private-login","display_name":"fixture-private-person"}"#;
const ACCOUNT: &str = r#"{"result":{"account":{"uid":1234,"login":"fixture-private-login","serviceAvailable":true}}}"#;
const ABOUT: &str = r#"{"result":{"uid":1234,"login":"fixture-private-login","name":"fixture-private-person","nested":{"access_token":"fixture-private-access-token"}}}"#;
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
        about_url: "http://127.0.0.1:1/account/about".to_owned(),
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
            about_url: format!("http://{address}/account/about"),
            catalog_url: format!(
                "http://{address}/search?text=Beethoven&type=track&page=0&nocorrect=true"
            ),
            timeout,
        },
        handle,
        retained,
    )
}
fn returned(current: &AuthorizationAttempt) -> AuthorizationResponse {
    returned_code(current, CODE)
}
fn returned_code(current: &AuthorizationAttempt, code: &str) -> AuthorizationResponse {
    let mut query = Zeroizing::new(String::with_capacity(MAX_INPUT));
    query.push_str(VERIFICATION_REDIRECT);
    query.push('?');
    append_form(
        &mut query,
        &[
            ("code", code),
            ("state", &current.state),
            ("cid", "fixture-uninterpreted-provider-value"),
        ],
    );
    AuthorizationResponse::ReturnedRedirect(query)
}
fn parse_code(
    current: &AuthorizationAttempt,
    response: AuthorizationResponse,
) -> Result<Zeroizing<String>, ErrorCategory> {
    match current.validated_authorization(response)? {
        ValidatedAuthorization::Code(code) => Ok(code),
        ValidatedAuthorization::ProviderError(error) => Err(error),
    }
}
fn attempt() -> AuthorizationAttempt {
    AuthorizationAttempt::new(CLIENT_ID).unwrap()
}
fn finish(responses: Vec<Fixture>) -> (DiagnosticReport, Vec<String>) {
    let (client, handle, retained) = fixture(responses);
    let current = attempt();
    let response = returned(&current);
    let report = current.complete_with(response, &client);
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
        (200, ABOUT),
        (200, CATALOG),
    ]);
    assert!(report.passed());
    assert_eq!(report.returned_state, ReturnedState::ValidatedRedirect);
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
    assert!(requests[3].starts_with("GET /account/about HTTP/1.1"));
    assert_eq!(requests.len(), 5);
    assert_eq!(report.music_about, Some(ProbeResult::success(200)));
    assert!(
        requests[4]
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
        (200, ABOUT),
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
        if let Err(error) = stream.write_all(response(status, &body).as_bytes()) {
            // Oversized responses are rejected from their headers. A client
            // that correctly closes early need not accept the fixture's body.
            // The caller still asserts the exact diagnostic error category.
            assert!(matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ));
        }
    });
    let mut client = isolated_client();
    client.client = client_builder().no_proxy().build().unwrap();
    client.token_url = format!("http://{address}/token");
    let current = attempt();
    let response = returned(&current);
    let report = current.complete_with(response, &client);
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
        assert!(report.music_about.is_none());
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
        (200, ABOUT),
    ]);
    assert_eq!(report.music_account_present, Some(false));
    assert_eq!(report.service_available, Some(false));
    assert_eq!(
        report.music_account.unwrap().error,
        Some(ErrorCategory::AccountAbsent)
    );
    assert_eq!(requests.len(), 4);
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
    let response = returned(&tampered);
    let report = tampered.complete_with(response, &client);
    server.join().unwrap();
    assert_eq!(
        report.token.error,
        Some(ErrorCategory::CodeInvalidOrExpired)
    );
    assert!(report.identity.is_none());
}

#[test]
fn full_redirect_requires_matching_state_and_reports_validated_denial() {
    let current = attempt();
    for state in ["", "wrong-state"] {
        let response = AuthorizationResponse::ReturnedRedirect(Zeroizing::new(format!(
            "{VERIFICATION_REDIRECT}?code={CODE}&state={state}"
        )));
        assert_eq!(
            parse_code(&current, response).unwrap_err(),
            ErrorCategory::StateMismatch
        );
    }
    assert_eq!(&*parse_code(&current, returned(&current)).unwrap(), CODE);
    let denied = AuthorizationResponse::ReturnedRedirect(Zeroizing::new(format!(
        "{VERIFICATION_REDIRECT}?error=access_denied&error_description=fixture-private-person&error_uri=https%3A%2F%2Fexample.invalid%2Fnever-follow&state={}",
        *current.state
    )));
    let report = current.complete_with(denied, &isolated_client());
    assert_eq!(report.returned_state, ReturnedState::ValidatedRedirect);
    assert_eq!(report.token.error, Some(ErrorCategory::AccessDenied));
    assert!(report.token.http_status.is_none());
    assert!(report.identity.is_none());
    no_sensitive_report(&report);
}

#[test]
fn missing_duplicate_or_unwanted_parameters_stop_before_transport() {
    for query in [
        format!("code={CODE}"),
        "state={state}".to_owned(),
        format!("code={CODE}&state={{state}}&code={CODE}"),
        format!("code={CODE}&state={{state}}&state={{state}}"),
        format!("code={CODE}&state={{state}}&%73tate={{state}}"),
        format!("code={CODE}&state={{state}}&cid=one&cid=two"),
        "error=access_denied&state={state}&error=access_denied".to_owned(),
        format!("code={CODE}&state={{state}}&error=access_denied"),
        format!("code={CODE}&state={{state}}&error_description=unexpected"),
        format!("code={CODE}&state={{state}}&error_uri=https%3A%2F%2Fexample.invalid"),
        format!("code={CODE}&state={{state}}&access_token=fixture-private-access-token"),
        format!("code={CODE}&state={{state}}&refresh_token=fixture-private-refresh-token"),
        format!("code={CODE}&state={{state}}&token_type=bearer"),
        format!("code={CODE}&state={{state}}&client_secret=fixture-private-secret"),
        format!("code={CODE}&state={{state}}&unexpected=value"),
        format!("code={CODE}&state={{state}}&"),
    ] {
        let current = attempt();
        let query = query.replace("{state}", &current.state);
        let response = AuthorizationResponse::ReturnedRedirect(Zeroizing::new(format!(
            "{VERIFICATION_REDIRECT}?{query}"
        )));
        let report = current.complete_with(response, &isolated_client());
        assert_eq!(report.returned_state, ReturnedState::RejectedRedirect);
        assert!(report.token.error.is_some());
        assert!(report.token.http_status.is_none());
        assert!(report.identity.is_none());
        no_sensitive_report(&report);
    }
}

#[test]
fn strict_origin_path_and_query_decoding_reject_normalization_tricks() {
    let current = attempt();
    for base in [
        "http://oauth.yandex.ru/verification_code",
        "https://other.invalid/verification_code",
        "https://oauth.yandex.ru.evil.invalid/verification_code",
        "https://oauth.yandex.ru@other.invalid/verification_code",
        "https://user@oauth.yandex.ru/verification_code",
        "https://oauth.yandex.ru:444/verification_code",
        "https://oauth.yandex.ru/verification_code/extra",
        "https://oauth.yandex.ru/other/../verification_code",
        "https://oauth.yandex.ru/%76erification_code",
        "https://oauth.yandex.ru/verification_code/..",
        "https://oauth.yandex.ru\\verification_code",
        " https://oauth.yandex.ru/verification_code",
        "https://oauth.yandex.ru/verification_code\n",
    ] {
        let response = AuthorizationResponse::ReturnedRedirect(Zeroizing::new(format!(
            "{base}?code={CODE}&state={}",
            *current.state
        )));
        assert_eq!(
            parse_code(&current, response).unwrap_err(),
            ErrorCategory::InvalidReturn
        );
    }
    for code in [
        "%", "%0", "%GG", "%FF", "%C0%AF", "%00", "%0A", "%7F", "%C2%85",
    ] {
        let response = AuthorizationResponse::ReturnedRedirect(Zeroizing::new(format!(
            "{VERIFICATION_REDIRECT}?code={code}&state={}",
            *current.state
        )));
        assert_eq!(
            parse_code(&current, response).unwrap_err(),
            ErrorCategory::InvalidReturn
        );
    }
    for suffix in [
        "#fragment",
        "#access_token=fixture-private-access-token",
        "\t",
    ] {
        let response = AuthorizationResponse::ReturnedRedirect(Zeroizing::new(format!(
            "{VERIFICATION_REDIRECT}?code={CODE}&state={}{suffix}",
            *current.state
        )));
        assert_eq!(
            parse_code(&current, response).unwrap_err(),
            ErrorCategory::InvalidReturn
        );
    }
}

#[test]
fn expired_cancelled_and_invalid_input_stop_before_transport() {
    let mut expired = attempt();
    expired.created = Instant::now() - CODE_LIFETIME;
    let response = returned(&expired);
    let report = expired.complete_with(response, &isolated_client());
    assert_eq!(report.token.error, Some(ErrorCategory::AttemptExpired));
    assert!(report.token.http_status.is_none());
    let report = attempt().complete_with(AuthorizationResponse::Cancelled, &isolated_client());
    assert_eq!(report.token.error, Some(ErrorCategory::Cancelled));
    assert_eq!(report.returned_state, ReturnedState::NotObserved);
    for code in [String::new(), "x".repeat(MAX_CODE_BYTES + 1)] {
        let current = attempt();
        assert_eq!(
            parse_code(&current, returned_code(&current, &code)).unwrap_err(),
            ErrorCategory::InvalidInput
        );
    }
    let current = attempt();
    let response =
        AuthorizationResponse::ReturnedRedirect(Zeroizing::new("x".repeat(MAX_INPUT + 1)));
    assert_eq!(
        parse_code(&current, response).unwrap_err(),
        ErrorCategory::InvalidReturn
    );
    for id in ["", "a b", "https://fixture.invalid"] {
        assert_eq!(
            AuthorizationAttempt::new(id).unwrap_err(),
            ErrorCategory::InvalidInput
        );
    }
}

#[test]
fn opaque_codes_and_reserved_characters_survive_the_secretless_form() {
    for code in [
        CODE,
        "short",
        "0123456",
        "opaque+/%&=?: spaced \u{00FC}@._~",
    ] {
        let current = attempt();
        let response = returned_code(&current, code);
        let (client, handle, retained) = fixture(vec![
            (200, FULL_TOKEN),
            (200, IDENTITY),
            (200, ACCOUNT),
            (200, ABOUT),
            (200, CATALOG),
        ]);
        let report = current.complete_with(response, &client);
        assert!(report.passed());
        assert!(
            report
                .to_string()
                .contains("Returned state: ValidatedRedirect")
        );
        let requests = handle.join().unwrap();
        let form = requests[0].split("\r\n\r\n").nth(1).unwrap();
        let parsed = reqwest::Url::parse(&format!("http://fixture/?{form}")).unwrap();
        let code_sent = parsed
            .query_pairs()
            .find(|(key, _)| key == "code")
            .unwrap()
            .1
            .into_owned();
        assert_eq!(code_sent, code);
        assert!(!requests[0].contains("client_secret"));
        assert!(!report.to_string().contains(code));
        no_sensitive_report(&report);
        retained.set_nonblocking(true).unwrap();
        assert_eq!(
            retained.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
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
            (200, ABOUT),
        ]);
        assert_eq!(
            report.music_account.unwrap(),
            ProbeResult::failure(Some(status), error)
        );
        assert_eq!(requests.len(), 4);
        assert!(report.catalog.is_none());
        no_sensitive_report(&report);
    }
}

#[test]
fn music_403_known_identifiers_are_closed_and_account_facts_stay_unknown() {
    for body in [
        r#"{"error":"missing-required-scopes"}"#,
        r#"{"error":{"name":"missing-required-scopes"}}"#,
        r#"{"error":{"name":"missing\u002drequired-scopes"}}"#,
        r#"{"error":{"name":"missing-required-scopes","message":"fixture-private-person fixture-private-access-token","description":"fixture-private-login","code":"fixture-private-account"},"result":{"account":{"uid":1234,"serviceAvailable":true,"login":"fixture-private-login"}},"invocationInfo":{"requestId":"fixture-private-request-id"},"headers":{"Authorization":"fixture-private-access-token"},"url":"https://fixture-private.invalid/secret"}"#,
    ] {
        let (report, requests) = finish(vec![
            (200, FULL_TOKEN),
            (200, IDENTITY),
            (403, body),
            (200, ABOUT),
        ]);
        assert_eq!(
            report.music_account,
            Some(ProbeResult::failure(
                Some(403),
                ErrorCategory::MusicMissingRequiredScopes
            ))
        );
        assert_eq!(report.music_account_present, None);
        assert_eq!(report.service_available, None);
        assert!(report.catalog.is_none());
        assert!(!report.passed());
        assert_eq!(requests.len(), 4);
        let visible = format!("{report:?}\n{report}");
        assert!(
            visible.contains("Music API reports missing required scopes (missing-required-scopes)")
        );
        assert!(visible.contains("Music account present: unknown; service available: unknown"));
        for private in [
            "fixture-private-request-id",
            "https://fixture-private.invalid/secret",
        ] {
            assert!(!visible.contains(private));
        }
        no_sensitive_report(&report);
    }
}

#[test]
fn music_403_unknown_malformed_and_ambiguous_envelopes_remain_forbidden() {
    for body in [
        "not JSON: fixture-private-person missing-required-scopes",
        "<html>missing-required-scopes fixture-private-person</html>",
        r#"{}"#,
        r#"{"error":"fixture-private-person missing-required-scopes"}"#,
        r#"{"error":"MISSING-REQUIRED-SCOPES"}"#,
        r#"{"error":" missing-required-scopes"}"#,
        r#"{"error":"API access restricted"}"#,
        r#"{"error":"api-access-restricted"}"#,
        r#"{"error":{"name":"Forbidden","message":"missing-required-scopes","description":"API access restricted"}}"#,
        r#"{"error_description":"missing-required-scopes"}"#,
        r#"{"error":{"message":"missing-required-scopes"}}"#,
        r#"{"error":{"code":"missing-required-scopes"}}"#,
        r#"{"code":"missing-required-scopes"}"#,
        r#"{"name":"missing-required-scopes"}"#,
        r#"{"result":{"error":{"name":"missing-required-scopes"}}}"#,
        r#"{"error":null}"#,
        r#"{"error":403}"#,
        r#"{"error":["missing-required-scopes"]}"#,
        r#"{"error":{"name":null}}"#,
        r#"{"error":{"name":{"missing-required-scopes":null}}}"#,
        r#"{"error":{"name":["missing-required-scopes"]}}"#,
        r#"{"error":{"name":"missing-required-scopes","name":"Forbidden"}}"#,
        r#"{"error":{"name":"Forbidden","name":"missing-required-scopes"}}"#,
        r#"{"error":{"name":"missing-required-scopes","na\u006de":"Forbidden"}}"#,
        r#"{"error":"missing-required-scopes","error":"Forbidden"}"#,
        r#"{"error":"Forbidden","\u0065rror":"missing-required-scopes"}"#,
        r#"{"error":"missing-required-scopes"} trailing-data"#,
    ] {
        let (report, requests) = finish(vec![
            (200, FULL_TOKEN),
            (200, IDENTITY),
            (403, body),
            (200, ABOUT),
        ]);
        assert_eq!(
            report.music_account,
            Some(ProbeResult::failure(Some(403), ErrorCategory::Forbidden))
        );
        assert_eq!(report.music_account_present, None);
        assert_eq!(report.service_available, None);
        assert!(report.catalog.is_none());
        assert_eq!(requests.len(), 4);
        assert!(
            report
                .to_string()
                .contains("Music account present: unknown; service available: unknown")
        );
        no_sensitive_report(&report);
    }
}

#[test]
fn music_error_codes_do_not_change_other_status_or_identity_oauth_semantics() {
    let body = r#"{"error":"missing-required-scopes"}"#;
    let (report, requests) = finish(vec![(200, FULL_TOKEN), (403, body)]);
    assert_eq!(
        report.identity,
        Some(ProbeResult::failure(Some(403), ErrorCategory::Forbidden))
    );
    assert_eq!(requests.len(), 2);
    assert!(report.music_account.is_none());
    assert!(report.music_about.is_none());
    let (report, requests) = finish(vec![(403, body)]);
    assert_eq!(
        report.token,
        ProbeResult::failure(Some(403), ErrorCategory::OAuthRequestRejected)
    );
    assert_eq!(requests.len(), 1);
    for (status, category) in [
        (401, ErrorCategory::AccessDenied),
        (451, ErrorCategory::Http),
        (500, ErrorCategory::Http),
    ] {
        let (report, requests) = finish(vec![
            (200, FULL_TOKEN),
            (200, IDENTITY),
            (status, body),
            (200, ABOUT),
        ]);
        assert_eq!(
            report.music_account,
            Some(ProbeResult::failure(Some(status), category))
        );
        assert_eq!(requests.len(), 4);
        assert!(report.catalog.is_none());
    }
}

#[test]
fn catalog_403_classification_preserves_preceding_known_account_facts() {
    let (report, requests) = finish(vec![
        (200, FULL_TOKEN),
        (200, IDENTITY),
        (200, ACCOUNT),
        (200, ABOUT),
        (
            403,
            r#"{"error":{"name":"missing-required-scopes","message":"fixture-private-person"}}"#,
        ),
    ]);
    assert_eq!(
        report.catalog,
        Some(ProbeResult::failure(
            Some(403),
            ErrorCategory::MusicMissingRequiredScopes
        ))
    );
    assert_eq!(report.music_account, Some(ProbeResult::success(200)));
    assert_eq!(report.music_account_present, Some(true));
    assert_eq!(report.service_available, Some(true));
    assert!(
        report
            .to_string()
            .contains("Music account present: yes; service available: yes")
    );
    assert_eq!(requests.len(), 5);
    assert!(!report.passed());
    no_sensitive_report(&report);
}

fn music_403_wire(response: String, pause_before_body: Duration) -> ProbeResult {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let retained = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        request(&mut stream);
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        write!(stream, "{headers}\r\n\r\n").unwrap();
        thread::sleep(pause_before_body);
        if let Err(error) = stream.write_all(body.as_bytes()) {
            assert!(matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ));
        }
    });
    let mut client = isolated_client();
    client.timeout = Duration::from_millis(500);
    let token = Token {
        access: Zeroizing::new(TOKEN.to_owned()),
        expires_at: Instant::now() + Duration::from_secs(30),
        scopes: ScopeEvidence::ReturnedRequiredScopes,
        http_status: 200,
    };
    let result = client
        .authorized_get(
            &format!("http://{address}/account/status"),
            &token,
            RemoteService::Music,
        )
        .err()
        .unwrap();
    server.join().unwrap();
    retained.set_nonblocking(true).unwrap();
    assert_eq!(
        retained.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    result
}

#[test]
fn music_403_body_limits_read_failure_and_timeout_keep_the_known_refusal() {
    let known = r#"{"error":"missing-required-scopes"}"#;
    for (wire, delay) in [
        (format!("HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", MAX_BODY + 1), Duration::ZERO),
        (format!("HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n{}", "x".repeat(MAX_BODY + 1)), Duration::ZERO),
        ("HTTP/1.1 403 Forbidden\r\nContent-Length: 1024\r\nConnection: close\r\n\r\nfixture-private-person".to_owned(), Duration::ZERO),
        (response(403, known), Duration::from_secs(1)),
    ] {
        assert_eq!(music_403_wire(wire, delay), ProbeResult::failure(Some(403), ErrorCategory::Forbidden));
    }
}

#[test]
fn music_status_and_about_are_independent_and_only_status_gates_catalog() {
    let known =
        r#"{"error":{"name":"missing-required-scopes","message":"fixture-private-person"}}"#;
    for (status_response, about_response, status_error, about_error, catalog_runs) in [
        (
            (403, known),
            (200, ABOUT),
            Some(ErrorCategory::MusicMissingRequiredScopes),
            None,
            false,
        ),
        (
            (403, "fixture-private-person"),
            (403, known),
            Some(ErrorCategory::Forbidden),
            Some(ErrorCategory::MusicMissingRequiredScopes),
            false,
        ),
        (
            (200, ACCOUNT),
            (403, known),
            None,
            Some(ErrorCategory::MusicMissingRequiredScopes),
            true,
        ),
        (
            (200, ACCOUNT),
            (401, known),
            None,
            Some(ErrorCategory::AccessDenied),
            true,
        ),
        (
            (200, "not JSON fixture-private-person"),
            (200, ABOUT),
            Some(ErrorCategory::InvalidResponse),
            None,
            false,
        ),
        (
            (
                200,
                r#"{"result":{"account":{"uid":null,"serviceAvailable":false}}}"#,
            ),
            (200, ABOUT),
            Some(ErrorCategory::AccountAbsent),
            None,
            false,
        ),
    ] {
        let mut responses = vec![
            (200, FULL_TOKEN),
            (200, IDENTITY),
            status_response,
            about_response,
        ];
        if catalog_runs {
            responses.push((200, CATALOG));
        }
        let (report, requests) = finish(responses);
        assert_eq!(
            report.music_account,
            Some(ProbeResult {
                http_status: Some(status_response.0),
                error: status_error
            })
        );
        assert_eq!(
            report.music_about,
            Some(ProbeResult {
                http_status: Some(about_response.0),
                error: about_error
            })
        );
        assert_eq!(requests.len(), if catalog_runs { 5 } else { 4 });
        assert!(requests[2].starts_with("GET /account/status HTTP/1.1"));
        assert!(requests[3].starts_with("GET /account/about HTTP/1.1"));
        for request in &requests[1..] {
            assert!(request.contains(&format!("authorization: OAuth {TOKEN}\r\n")));
            assert!(!request.contains("x-yandex-music-client:"));
            assert!(!request.contains("client_secret"));
        }
        assert_eq!(report.catalog.is_some(), catalog_runs);
        assert!(!report.passed());
        if catalog_runs {
            assert_eq!(report.music_account_present, Some(true));
            assert_eq!(report.service_available, Some(true));
            assert_eq!(report.catalog, Some(ProbeResult::success(200)));
        } else if status_response.0 == 403 {
            assert_eq!(report.music_account_present, None);
            assert_eq!(report.service_available, None);
        }
        let visible = report.to_string();
        assert!(visible.contains(&format!(
            "Music /account/status: HTTP {}",
            status_response.0
        )));
        assert!(visible.contains(&format!("Music /account/about: HTTP {}", about_response.0)));
        no_sensitive_report(&report);
    }
}

#[test]
fn about_checks_only_an_object_envelope_and_never_infers_status_facts() {
    for (body, accepted) in [
        (ABOUT, true),
        (
            r#"{"result":{},"invocationInfo":{"requestId":"fixture-private-request-id"}}"#,
            true,
        ),
        (r#"{"result":{"serviceAvailable":true,"uid":1234}}"#, true),
        ("not JSON fixture-private-person", false),
        ("<html>fixture-private-person</html>", false),
        (r#"{}"#, false),
        (r#"{"result":null}"#, false),
        (r#"{"result":[]}"#, false),
        (r#"{"result":"fixture-private-person"}"#, false),
        (r#"{"result":1234}"#, false),
        (r#"{"result":true}"#, false),
        (r#"{"error":"missing-required-scopes"}"#, false),
        (r#"{"result":{},"error":"missing-required-scopes"}"#, false),
        (r#"{"result":{},"error":null}"#, false),
        (
            r#"{"result":{},"\u0065rror":{"message":"fixture-private-person"}}"#,
            false,
        ),
        (r#"{"result":{},"result":{"uid":1234}}"#, false),
        (r#"{"result":{},"res\u0075lt":{}}"#, false),
        (r#"{"result":{}} trailing data"#, false),
    ] {
        let (report, requests) = finish(vec![
            (200, FULL_TOKEN),
            (200, IDENTITY),
            (403, "{}"),
            (200, body),
        ]);
        assert_eq!(
            report.music_about,
            Some(if accepted {
                ProbeResult::success(200)
            } else {
                ProbeResult::failure(Some(200), ErrorCategory::InvalidResponse)
            })
        );
        assert_eq!(
            report.music_account,
            Some(ProbeResult::failure(Some(403), ErrorCategory::Forbidden))
        );
        assert_eq!(report.music_account_present, None);
        assert_eq!(report.service_available, None);
        assert!(report.catalog.is_none());
        assert!(!report.passed());
        assert_eq!(requests.len(), 4);
        no_sensitive_report(&report);
        assert!(!format!("{report:?}\n{report}").contains("fixture-private-request-id"));
    }
}

#[test]
fn about_reuses_the_closed_music_error_classifier_and_status_categories() {
    for (status, body, category) in [
        (
            403,
            r#"{"error":"missing-required-scopes"}"#,
            ErrorCategory::MusicMissingRequiredScopes,
        ),
        (
            403,
            r#"{"error":{"name":"missing-required-scopes","message":"fixture-private-person","requestId":"fixture-private-request-id"}}"#,
            ErrorCategory::MusicMissingRequiredScopes,
        ),
        (
            403,
            r#"{"error":{"message":"missing-required-scopes"}}"#,
            ErrorCategory::Forbidden,
        ),
        (
            403,
            r#"{"error":"API access restricted"}"#,
            ErrorCategory::Forbidden,
        ),
        (
            403,
            r#"{"error":"missing-required-scopes","error":"fixture-private-person"}"#,
            ErrorCategory::Forbidden,
        ),
        (
            403,
            "<html>fixture-private-person</html>",
            ErrorCategory::Forbidden,
        ),
        (
            401,
            r#"{"error":"missing-required-scopes"}"#,
            ErrorCategory::AccessDenied,
        ),
        (
            302,
            r#"{"error":"missing-required-scopes"}"#,
            ErrorCategory::Http,
        ),
        (
            451,
            r#"{"error":"missing-required-scopes"}"#,
            ErrorCategory::Http,
        ),
        (
            500,
            r#"{"error":"missing-required-scopes"}"#,
            ErrorCategory::Http,
        ),
    ] {
        let (report, requests) = finish(vec![
            (200, FULL_TOKEN),
            (200, IDENTITY),
            (403, "{}"),
            (status, body),
        ]);
        assert_eq!(
            report.music_about,
            Some(ProbeResult::failure(Some(status), category))
        );
        assert_eq!(requests.len(), 4);
        assert_eq!(report.music_account_present, None);
        assert_eq!(report.service_available, None);
        assert!(report.catalog.is_none());
        assert!(!report.passed());
        no_sensitive_report(&report);
        assert!(!format!("{report:?}\n{report}").contains("fixture-private-request-id"));
    }
}

fn accept_before_deadline(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("expected bounded fixture request: {error}"),
        }
    }
}

#[test]
fn failed_status_transport_cannot_suppress_the_independent_about_request() {
    for timeout_case in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let retained = listener.try_clone().unwrap();
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for body in [FULL_TOKEN, IDENTITY] {
                let mut stream = accept_before_deadline(&listener);
                requests.push(request(&mut stream));
                stream.write_all(response(200, body).as_bytes()).unwrap();
            }
            let mut status_stream = accept_before_deadline(&listener);
            requests.push(request(&mut status_stream));
            let pending = if timeout_case {
                Some(thread::spawn(move || {
                    thread::sleep(Duration::from_millis(600));
                    drop(status_stream);
                }))
            } else {
                drop(status_stream);
                None
            };
            let mut stream = accept_before_deadline(&listener);
            requests.push(request(&mut stream));
            stream.write_all(response(200, ABOUT).as_bytes()).unwrap();
            if let Some(pending) = pending {
                pending.join().unwrap();
            }
            requests
        });
        let mut client = isolated_client();
        client.timeout = Duration::from_millis(250);
        client.token_url = format!("http://{address}/token");
        client.identity_url = format!("http://{address}/info");
        client.status_url = format!("http://{address}/account/status");
        client.about_url = format!("http://{address}/account/about");
        client.catalog_url = format!("http://{address}/search");
        let current = attempt();
        let authorization = returned(&current);
        let report = current.complete_with(authorization, &client);
        let requests = server.join().unwrap();
        assert_eq!(
            report.music_account,
            Some(ProbeResult::failure(
                None,
                if timeout_case {
                    ErrorCategory::Timeout
                } else {
                    ErrorCategory::Network
                }
            ))
        );
        assert_eq!(report.music_about, Some(ProbeResult::success(200)));
        assert_eq!(report.music_account_present, None);
        assert_eq!(report.service_available, None);
        assert!(report.catalog.is_none());
        assert_eq!(requests.len(), 4);
        assert!(requests[3].starts_with("GET /account/about HTTP/1.1"));
        retained.set_nonblocking(true).unwrap();
        assert_eq!(
            retained.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        no_sensitive_report(&report);
    }
}

fn about_wire(wire: String, pause_before_body: Duration) -> DiagnosticReport {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let retained = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        for (expected, status, body) in [
            ("POST /token ", 200, FULL_TOKEN),
            ("GET /info ", 200, IDENTITY),
            ("GET /account/status ", 403, "{}"),
        ] {
            let mut stream = accept_before_deadline(&listener);
            assert!(request(&mut stream).starts_with(expected));
            stream.write_all(response(status, body).as_bytes()).unwrap();
        }
        let mut stream = accept_before_deadline(&listener);
        assert!(request(&mut stream).starts_with("GET /account/about HTTP/1.1"));
        let (headers, body) = wire.split_once("\r\n\r\n").unwrap();
        write!(stream, "{headers}\r\n\r\n").unwrap();
        thread::sleep(pause_before_body);
        if let Err(error) = stream.write_all(body.as_bytes()) {
            assert!(matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ));
        }
    });
    let mut client = isolated_client();
    client.timeout = Duration::from_millis(500);
    client.token_url = format!("http://{address}/token");
    client.identity_url = format!("http://{address}/info");
    client.status_url = format!("http://{address}/account/status");
    client.about_url = format!("http://{address}/account/about");
    client.catalog_url = format!("http://{address}/search");
    let current = attempt();
    let authorization = returned(&current);
    let report = current.complete_with(authorization, &client);
    server.join().unwrap();
    retained.set_nonblocking(true).unwrap();
    assert_eq!(
        retained.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        report.music_account,
        Some(ProbeResult::failure(Some(403), ErrorCategory::Forbidden))
    );
    assert_eq!(report.music_account_present, None);
    assert_eq!(report.service_available, None);
    assert!(report.catalog.is_none());
    no_sensitive_report(&report);
    report
}

#[test]
fn about_body_limits_read_failures_and_deadlines_remain_bounded() {
    for (wire, delay, expected) in [
        (format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", MAX_BODY + 1), Duration::ZERO, ProbeResult::failure(Some(200), ErrorCategory::ResponseTooLarge)),
        (format!("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}", "x".repeat(MAX_BODY + 1)), Duration::ZERO, ProbeResult::failure(Some(200), ErrorCategory::ResponseTooLarge)),
        ("HTTP/1.1 200 OK\r\nContent-Length: 1024\r\nConnection: close\r\n\r\nfixture-private-person".to_owned(), Duration::ZERO, ProbeResult::failure(Some(200), ErrorCategory::Network)),
        (response(200, ABOUT), Duration::from_secs(1), ProbeResult::failure(Some(200), ErrorCategory::Timeout)),
        (format!("HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", MAX_BODY + 1), Duration::ZERO, ProbeResult::failure(Some(403), ErrorCategory::Forbidden)),
        ("HTTP/1.1 403 Forbidden\r\nContent-Length: 1024\r\nConnection: close\r\n\r\nfixture-private-person".to_owned(), Duration::ZERO, ProbeResult::failure(Some(403), ErrorCategory::Forbidden)),
        (response(403, r#"{"error":"missing-required-scopes"}"#), Duration::from_secs(1), ProbeResult::failure(Some(403), ErrorCategory::Forbidden)),
    ] {
        assert_eq!(about_wire(wire, delay).music_about, Some(expected));
    }
}

#[test]
fn malformed_oversized_and_error_envelopes_never_pass() {
    for (body, expected) in [
        ("not json".to_owned(), ErrorCategory::InvalidResponse),
        ("{}".to_owned(), ErrorCategory::InvalidResponse),
        ("x".repeat(MAX_BODY + 1), ErrorCategory::ResponseTooLarge),
        (
            r#"{"access_token":"fixture-private-access-token","token_type":"bearer","expires_in":0}"#
                .to_owned(),
            ErrorCategory::InvalidResponse,
        ),
    ] {
        let report = one_response(200, body);
        assert_eq!(report.token.error, Some(expected));
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
            (200, ABOUT),
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
        let current = attempt();
        let response = returned(&current);
        let report = current.complete_with(response, &client);
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
    let current = attempt();
    let response = returned(&current);
    let report = current.complete_with(response, &client);
    server.join().unwrap();
    assert_eq!(
        report.identity.unwrap(),
        ProbeResult::failure(Some(302), ErrorCategory::Http)
    );
    assert!(report.music_account.is_none());
    assert!(report.music_about.is_none());
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
            .authorized_get(
                "http://127.0.0.1:1/not-contacted",
                &token,
                RemoteService::Identity
            )
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
    let current = attempt();
    let response = returned(&current);
    let report = current.complete_with(response, &client);
    server.join().unwrap();
    assert_eq!(
        report.token,
        ProbeResult::failure(Some(200), ErrorCategory::Timeout)
    );
    assert!(report.identity.is_none());
    no_sensitive_report(&report);
}
