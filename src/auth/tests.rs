// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic protocol fixtures only: no provider requests, browser or accounts.

use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Barrier,
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};

use super::*;
use rustix::event::{PollFd, PollFlags, Timespec, poll};

const SYNTHETIC_CODE: &str = "synthetic-private-opaque-code";
const TEST_BOUND: Duration = Duration::from_secs(3);

struct SyntheticFixture {
    attempt: Arc<AuthAttempt>,
    authorization_listener: TcpListener,
}

impl SyntheticFixture {
    fn new(mode: BrowserMode) -> Self {
        let authorization_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        authorization_listener.set_nonblocking(true).unwrap();
        let endpoint = format!(
            "http://{}/authorize",
            authorization_listener.local_addr().unwrap()
        );
        let attempt = AuthAttempt::for_test(&endpoint, mode).unwrap();
        assert_ne!(
            attempt.address,
            authorization_listener.local_addr().unwrap(),
            "the automatic callback has its own loopback listener"
        );
        Self {
            attempt,
            authorization_listener,
        }
    }

    fn assert_no_provider_requests(&self) {
        assert_eq!(
            self.authorization_listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

fn callback(attempt: &AuthAttempt, code: &str) -> String {
    let mut url = Url::parse(attempt.callback_uri()).unwrap();
    url.query_pairs_mut()
        .append_pair("state", attempt.state.as_str())
        .append_pair("code", code);
    url.into()
}

fn callback_query(attempt: &AuthAttempt, query: &str) -> String {
    format!("{}?{query}", attempt.callback_uri())
}

fn rejection(decision: NavigationDecision) -> AuthError {
    match decision {
        NavigationDecision::Reject(error) => error,
        other => panic!("expected a rejected navigation, got {other:?}"),
    }
}

fn spawn_receiver(
    attempt: &Arc<AuthAttempt>,
) -> (
    Receiver<Result<AuthorizationCode, AuthError>>,
    JoinHandle<()>,
) {
    let attempt = Arc::clone(attempt);
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let _ = sender.send(attempt.receive_callback());
    });
    (receiver, worker)
}

fn pending_callback(attempt: &AuthAttempt) -> usize {
    // The descriptor borrow and mutex guard end before a worker can accept or
    // cancellation can close this listener. Poll itself never waits here.
    let listener = attempt.listener.lock().unwrap();
    let mut descriptors = [PollFd::new(listener.as_ref().unwrap(), PollFlags::IN)];
    poll(&mut descriptors, Some(&Timespec::default())).unwrap()
}

fn request(attempt: &AuthAttempt, method: &str, target: &str, headers: &str) -> String {
    format!(
        "{method} {target} HTTP/1.1\r\nHost: {}\r\n{headers}\r\n",
        attempt.address
    )
}

fn callback_target(attempt: &AuthAttempt) -> String {
    let returned = callback(attempt, SYNTHETIC_CODE);
    returned
        .strip_prefix(&format!("http://{}", attempt.address))
        .unwrap()
        .to_owned()
}

fn send_request(attempt: &AuthAttempt, request: &str) -> String {
    let mut stream = TcpStream::connect_timeout(&attempt.address, TEST_BOUND).unwrap();
    stream.set_read_timeout(Some(TEST_BOUND)).unwrap();
    stream.set_write_timeout(Some(TEST_BOUND)).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = Vec::new();
    // Oversized input can leave unread bytes when the server closes the stream.
    // Preserve the bounded response even when the peer consequently sends RST.
    match stream.read_to_end(&mut response) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        Err(error) => panic!("bounded synthetic callback response failed: {error}"),
    }
    String::from_utf8(response).unwrap()
}

fn assert_private_response(response: &str, attempt: &AuthAttempt) {
    let lower = response.to_ascii_lowercase();
    assert!(lower.contains("\r\ncache-control: no-store\r\n"));
    assert!(lower.contains("\r\nreferrer-policy: no-referrer\r\n"));
    assert!(lower.contains("\r\ncontent-security-policy: default-src 'none'"));
    assert!(!lower.contains("\r\nlocation:"));
    for secret in [
        SYNTHETIC_CODE,
        attempt.state.as_str(),
        attempt.verifier(),
        attempt.callback_uri(),
    ] {
        assert!(
            !response.contains(secret),
            "response leaked synthetic material"
        );
    }
}

#[test]
#[cfg(not(feature = "cassette-auth"))]
fn public_provider_stays_blocked_for_begin_and_exchange() {
    let fixture = SyntheticFixture::new(BrowserMode::System);
    let provider = public_provider();
    assert_eq!(
        provider.begin().unwrap_err(),
        AuthError::MusicPermissionUnavailable
    );
    let code = AuthorizationCode(Zeroizing::new(SYNTHETIC_CODE.to_owned()));
    assert_eq!(
        provider.exchange(&fixture.attempt, code).unwrap_err(),
        AuthError::MusicPermissionUnavailable
    );
    fixture.assert_no_provider_requests();
}

#[test]
fn authorization_uses_independent_random_state_and_s256_pkce() {
    let first = SyntheticFixture::new(BrowserMode::Embedded);
    let second = SyntheticFixture::new(BrowserMode::System);
    for fixture in [&first, &second] {
        let attempt = &fixture.attempt;
        assert_eq!(attempt.state.len(), 43);
        assert_eq!(attempt.verifier().len(), 43);
        for value in [attempt.state.as_str(), attempt.verifier()] {
            assert!(
                value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
            );
        }
        assert_ne!(attempt.state.as_str(), attempt.verifier());
        let url = Url::parse(&attempt.authorization_url()).unwrap();
        let pairs: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
        assert_eq!(pairs.len(), 7);
        assert_eq!(pairs.get("response_type").unwrap(), "code");
        assert_eq!(pairs.get("client_id").unwrap(), "synthetic-karin-client");
        assert_eq!(pairs.get("redirect_uri").unwrap(), attempt.callback_uri());
        assert_eq!(pairs.get("state").unwrap(), attempt.state.as_str());
        assert_eq!(pairs.get("code_challenge_method").unwrap(), "S256");
        let challenge =
            URL_SAFE_NO_PAD.encode(digest(&SHA256, attempt.verifier().as_bytes()).as_ref());
        assert_eq!(pairs.get("code_challenge").unwrap(), challenge.as_str());
        assert_eq!(
            pairs.get("scope").unwrap(),
            &crate::oauth::REQUIRED_SCOPES.join(" ")
        );
        assert!(!pairs.contains_key("response_mode"));
        assert!(!pairs.contains_key("access_token"));
        assert!(!pairs.contains_key("client_secret"));
        assert!(!pairs.contains_key("code_verifier"));
        assert!(attempt.time_remaining() <= LIFETIME);
        assert!(attempt.time_remaining() > Duration::from_secs(9 * 60));
        fixture.assert_no_provider_requests();
    }
    assert_ne!(first.attempt.state, second.attempt.state);
    assert_ne!(first.attempt.verifier(), second.attempt.verifier());
    assert_eq!(first.attempt.browser_mode(), BrowserMode::Embedded);
    assert_eq!(second.attempt.browser_mode(), BrowserMode::System);
}

#[test]
fn production_constructor_rejects_unregistered_ports_and_invalid_identity() {
    for port in [0, 80, 443, 1023] {
        assert_eq!(
            AuthAttempt::new("synthetic-client", port, BrowserMode::System).unwrap_err(),
            AuthError::Configuration
        );
    }
    for client in [
        "",
        "synthetic client",
        "synthetic&scope=extra",
        "synthetic/identity",
    ] {
        assert_eq!(
            AuthAttempt::new(client, 49152, BrowserMode::System).unwrap_err(),
            AuthError::Configuration
        );
    }
    assert_eq!(
        AuthAttempt::new(&"x".repeat(129), 49152, BrowserMode::System).unwrap_err(),
        AuthError::Configuration
    );
}

#[test]
fn synthetic_endpoint_escape_hatch_accepts_only_numeric_ipv4_loopback() {
    for endpoint in [
        "https://127.0.0.1:45678/authorize",
        "http://localhost:45678/authorize",
        "http://[::1]:45678/authorize",
        "http://192.0.2.1:45678/authorize",
        "http://127.0.0.1/authorize",
        "http://synthetic@127.0.0.1:45678/authorize",
        "http://synthetic:private@127.0.0.1:45678/authorize",
    ] {
        assert_eq!(
            AuthAttempt::for_test(endpoint, BrowserMode::System).unwrap_err(),
            AuthError::Configuration
        );
    }
}

#[test]
fn callback_requires_exact_raw_origin_port_and_path_before_normalization() {
    let fixture = SyntheticFixture::new(BrowserMode::Embedded);
    let attempt = &fixture.attempt;
    let port = attempt.address.port();
    let query = format!("state={}&code={SYNTHETIC_CODE}", attempt.state.as_str());
    let alternate_port = if port == u16::MAX { port - 1 } else { port + 1 };
    for origin_or_path in [
        format!("https://127.0.0.1:{port}{CALLBACK_PATH}"),
        format!("http://localhost:{port}{CALLBACK_PATH}"),
        format!("http://127.0.0.1:{alternate_port}{CALLBACK_PATH}"),
        format!("http://127.1:{port}{CALLBACK_PATH}"),
        format!("http://2130706433:{port}{CALLBACK_PATH}"),
        format!("http://127.0.0.1.:{port}{CALLBACK_PATH}"),
        format!("http://synthetic@127.0.0.1:{port}{CALLBACK_PATH}"),
        format!("http://127.0.0.1:{port}/oauth/./callback"),
        format!("http://127.0.0.1:{port}/other/../oauth/callback"),
        format!("http://127.0.0.1:{port}/oauth/%63allback"),
        format!("http://127.0.0.1:{port}/oauth/callback/"),
        format!("http://127.0.0.1:{port}/oauth/callback-extra"),
        format!("http://127.0.0.1:{port}//oauth/callback"),
    ] {
        let returned = format!("{origin_or_path}?{query}");
        assert_eq!(
            attempt.validate_callback(&returned).unwrap_err(),
            AuthError::InvalidCallback
        );
        assert!(matches!(
            attempt.navigation(&returned),
            NavigationDecision::Reject(_)
        ));
    }
    let valid = callback(attempt, SYNTHETIC_CODE);
    for returned in [
        format!(" {valid}"),
        format!("{valid} "),
        format!("{valid}#fragment"),
        valid.replace("/oauth/", "\\oauth/"),
        valid.replace("http://", "http:\n//"),
    ] {
        assert_eq!(
            attempt.validate_callback(&returned).unwrap_err(),
            AuthError::InvalidCallback
        );
        assert_eq!(
            rejection(attempt.navigation(&returned)),
            AuthError::UntrustedNavigation
        );
    }
    assert_eq!(
        attempt.validate_callback(&valid).unwrap().expose(),
        SYNTHETIC_CODE
    );
}

#[test]
fn malformed_duplicate_and_unknown_callback_fields_do_not_consume_attempt() {
    let fixture = SyntheticFixture::new(BrowserMode::Embedded);
    let attempt = &fixture.attempt;
    let state = attempt.state.as_str();
    for query in [
        format!("state={state}&state={state}&code={SYNTHETIC_CODE}"),
        format!("state={state}&%73tate={state}&code={SYNTHETIC_CODE}"),
        format!("state={state}&code={SYNTHETIC_CODE}&code=another"),
        format!("state={state}&code={SYNTHETIC_CODE}&c%6fde=another"),
        format!("state={state}&code={SYNTHETIC_CODE}&unexpected=synthetic"),
        format!("state={state}&code={SYNTHETIC_CODE}&cid=a&cid=b"),
        format!("state={state}&error=access_denied&error=access_denied"),
        format!("state={state}&error="),
        format!("state={state}&error=access_denied&error_description=a&error_description=b"),
        format!("state={state}&error=access_denied&error_uri=a&error_uri=b"),
        format!("state={state}&code={SYNTHETIC_CODE}&error=access_denied"),
        format!("state={state}&code={SYNTHETIC_CODE}&error_description=private"),
        format!("state={state}&code={SYNTHETIC_CODE}&error_uri=https%3A%2F%2Finvalid.test"),
        format!("state={state}&code="),
        format!("state={state}&code"),
        format!("state={state}&code={SYNTHETIC_CODE}&"),
        format!("state={state}&code=%"),
        format!("state={state}&code=%2"),
        format!("state={state}&code=%GG"),
        format!("state={state}&code=%FF"),
        format!("state={state}&code=%00"),
        format!("state={state}&code=%0d%0a"),
        format!("state={state}&co%GGde=synthetic"),
        format!("state={state}%&code={SYNTHETIC_CODE}"),
        format!("state={state}&code={}", "x".repeat(2049)),
    ] {
        assert_eq!(
            attempt
                .validate_callback(&callback_query(attempt, &query))
                .unwrap_err(),
            AuthError::InvalidCallback
        );
        assert!(!attempt.consumed.load(Ordering::Acquire));
    }
    assert_eq!(
        attempt
            .validate_callback(attempt.callback_uri())
            .unwrap_err(),
        AuthError::InvalidCallback
    );
    assert_eq!(
        attempt
            .validate_callback(&format!("{}?", attempt.callback_uri()))
            .unwrap_err(),
        AuthError::InvalidCallback
    );
    assert_eq!(
        attempt
            .validate_callback(&format!(
                "{}?{}",
                attempt.callback_uri(),
                "x".repeat(MAX_RETURN)
            ))
            .unwrap_err(),
        AuthError::InvalidCallback
    );
    assert_eq!(
        attempt
            .validate_callback(&callback(attempt, SYNTHETIC_CODE))
            .unwrap()
            .expose(),
        SYNTHETIC_CODE
    );
}

#[test]
fn authorization_codes_are_opaque_and_form_decoded_exactly_once() {
    for code in [
        "1234567",
        "synthetic/a+b=c%2F&x",
        "synthetic opaque code",
        "синтетический-код",
    ] {
        let fixture = SyntheticFixture::new(BrowserMode::Embedded);
        let attempt = &fixture.attempt;
        let returned = callback(attempt, code);
        assert_eq!(attempt.validate_callback(&returned).unwrap().expose(), code);
    }
    let fixture = SyntheticFixture::new(BrowserMode::Embedded);
    let attempt = &fixture.attempt;
    let returned = callback_query(
        attempt,
        &format!(
            "%73tate={}&c%6fde=synthetic%252Fopaque+code&cid=synthetic",
            attempt.state.as_str()
        ),
    );
    assert_eq!(
        attempt.validate_callback(&returned).unwrap().expose(),
        "synthetic%2Fopaque code"
    );
}

#[test]
fn callback_state_is_required_and_errors_are_validated_before_becoming_terminal() {
    let fixture = SyntheticFixture::new(BrowserMode::Embedded);
    let attempt = &fixture.attempt;
    let replacement = if attempt.state.starts_with('A') {
        "B"
    } else {
        "A"
    };
    let wrong_same_length = format!("{replacement}{}", &attempt.state[1..]);
    for query in [
        format!("code={SYNTHETIC_CODE}"),
        "error=access_denied".to_owned(),
        format!("state=synthetic-wrong-state&code={SYNTHETIC_CODE}"),
        "state=synthetic-wrong-state&error=access_denied".to_owned(),
        format!("state={wrong_same_length}&code={SYNTHETIC_CODE}"),
        format!("state={}%20&error=access_denied", attempt.state.as_str()),
    ] {
        assert_eq!(
            attempt
                .validate_callback(&callback_query(attempt, &query))
                .unwrap_err(),
            AuthError::StateMismatch
        );
        assert!(!attempt.consumed.load(Ordering::Acquire));
    }
    for (provider_error, expected) in [
        ("access_denied", AuthError::Denied),
        ("synthetic-provider-private-error", AuthError::Rejected),
    ] {
        let fixture = SyntheticFixture::new(BrowserMode::Embedded);
        let attempt = &fixture.attempt;
        let returned = callback_query(
            attempt,
            &format!(
                "state={}&error={provider_error}&error_description=synthetic-private-message&error_uri=https%3A%2F%2Finvalid.test",
                attempt.state.as_str()
            ),
        );
        assert_eq!(attempt.validate_callback(&returned).unwrap_err(), expected);
        assert_eq!(
            attempt.validate_callback(&returned).unwrap_err(),
            AuthError::InvalidCallback
        );
        let displayed = format!("{expected:?} {expected}");
        assert!(!displayed.contains("synthetic-private-message"));
        assert!(!displayed.contains("synthetic-provider-private-error"));
    }
}

#[test]
fn callbacks_are_single_use_across_manual_and_browser_navigation() {
    let fixture = SyntheticFixture::new(BrowserMode::Embedded);
    let attempt = &fixture.attempt;
    let returned = callback(attempt, SYNTHETIC_CODE);
    assert!(matches!(
        attempt.navigation(&returned),
        NavigationDecision::CodeReturn
    ));
    assert!(!attempt.consumed.load(Ordering::Acquire));
    assert_eq!(
        attempt.complete_code_return(&returned).unwrap().expose(),
        SYNTHETIC_CODE
    );
    assert_eq!(
        attempt.validate_callback(&returned).unwrap_err(),
        AuthError::InvalidCallback
    );
    assert_eq!(
        rejection(attempt.navigation(&returned)),
        AuthError::InvalidCallback
    );
    assert_eq!(
        rejection(attempt.navigation("https://oauth.yandex.ru/authorize")),
        AuthError::InvalidCallback
    );
}

#[test]
fn concurrent_callback_replay_yields_exactly_one_authorization_code() {
    let fixture = SyntheticFixture::new(BrowserMode::Embedded);
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let attempt = Arc::clone(&fixture.attempt);
        let barrier = Arc::clone(&barrier);
        let returned = callback(&attempt, SYNTHETIC_CODE);
        workers.push(thread::spawn(move || {
            barrier.wait();
            attempt.validate_callback(&returned).map(|code| {
                assert_eq!(code.expose(), SYNTHETIC_CODE);
            })
        }));
    }
    barrier.wait();
    let outcomes: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .into_iter()
            .filter_map(Result::err)
            .collect::<Vec<_>>(),
        vec![AuthError::InvalidCallback]
    );
}

#[test]
fn expired_and_cancelled_attempts_cannot_navigate_or_receive() {
    let mut fixture = SyntheticFixture::new(BrowserMode::System);
    Arc::get_mut(&mut fixture.attempt).unwrap().created =
        Instant::now() - LIFETIME - Duration::from_secs(1);
    let attempt = &fixture.attempt;
    assert!(attempt.expired());
    assert_eq!(attempt.time_remaining(), Duration::ZERO);
    assert_eq!(
        attempt
            .validate_callback(&callback(attempt, SYNTHETIC_CODE))
            .unwrap_err(),
        AuthError::Expired
    );
    assert_eq!(
        rejection(attempt.navigation("https://oauth.yandex.ru/authorize")),
        AuthError::Expired
    );
    assert_eq!(attempt.receive_callback().unwrap_err(), AuthError::Expired);

    let fixture = SyntheticFixture::new(BrowserMode::System);
    let attempt = &fixture.attempt;
    attempt.cancel();
    attempt.cancel();
    assert!(attempt.is_cancelled());
    assert_eq!(
        attempt
            .validate_callback(&callback(attempt, SYNTHETIC_CODE))
            .unwrap_err(),
        AuthError::Cancelled
    );
    assert_eq!(
        rejection(attempt.navigation("https://oauth.yandex.ru/authorize")),
        AuthError::Cancelled
    );
    assert_eq!(
        attempt.receive_callback().unwrap_err(),
        AuthError::Cancelled
    );
}

#[test]
fn only_exact_yandex_https_origins_and_explicit_fixture_origin_are_trusted() {
    let fixture = SyntheticFixture::new(BrowserMode::Embedded);
    let attempt = &fixture.attempt;
    for url in [
        "https://oauth.yandex.ru/authorize?synthetic=value",
        "https://passport.yandex.ru/auth",
        "https://id.yandex.ru/",
        "https://smartcaptcha.yandexcloud.net/checkbox?synthetic=value",
        "https://captcha-api.yandex.ru/checkbox?synthetic=value",
        "https://oauth.yandex.ru:443/authorize",
        "https://oauth.yandex.ru/authorize#fragment",
        "https://passport.yandex.ru/auth#phone",
    ] {
        assert!(matches!(
            attempt.navigation(url),
            NavigationDecision::AllowTrusted
        ));
    }
    assert!(matches!(
        attempt.navigation(&attempt.authorization_url()),
        NavigationDecision::AllowTrusted
    ));
    for url in [
        "http://oauth.yandex.ru/authorize",
        "https://oauth.yandex.ru:444/authorize",
        "http://smartcaptcha.yandexcloud.net/checkbox",
        "https://smartcaptcha.yandexcloud.net:444/checkbox",
        "https://smartcaptcha.yandexcloud.net.invalid.test/checkbox",
        "https://other.yandexcloud.net/checkbox",
        "https://synthetic@smartcaptcha.yandexcloud.net/checkbox",
        "https://captcha.yandex.net/",
        "http://captcha-api.yandex.ru/checkbox",
        "https://captcha-api.yandex.ru:444/checkbox",
        "https://captcha-api.yandex.ru.invalid.test/checkbox",
        "https://sub.captcha-api.yandex.ru/checkbox",
        "https://oauth.yandex.ru.invalid.test/authorize",
        "https://invalid.test/oauth.yandex.ru",
        "https://sub.oauth.yandex.ru/authorize",
        "https://yandex.ru/authorize",
        "https://oauth.yandex.com/authorize",
        "https://synthetic@oauth.yandex.ru/authorize",
        "https://synthetic:private@oauth.yandex.ru/authorize",
        "https://oauth.yandex.ru@invalid.test/authorize",
        "https://oauth.yandex.ru\\@invalid.test/authorize",
        " https://oauth.yandex.ru/authorize",
        "https://oauth.yandex.ru/authorize\n",
        "javascript:alert('synthetic')",
        "data:text/plain,synthetic",
        "file:///synthetic",
        "about:blank",
        "http://127.0.0.1:1/authorize",
        "http://localhost:1/authorize",
    ] {
        assert_eq!(
            rejection(attempt.navigation(url)),
            AuthError::UntrustedNavigation
        );
    }
    let local = Url::parse(&attempt.authorization_url()).unwrap();
    let fixture_with_fragment = format!("{}#synthetic", local.as_str());
    assert!(matches!(
        attempt.navigation(&fixture_with_fragment),
        NavigationDecision::AllowTrusted
    ));
    fixture.assert_no_provider_requests();
}

#[test]
fn regression_trusted_page_fragment_must_not_abort_login() {
    let fixture = SyntheticFixture::new(BrowserMode::Embedded);
    let attempt = &fixture.attempt;
    let local_fragment = format!(
        "{}#synthetic-phone-step",
        attempt.authorization_url().as_str()
    );
    assert!(matches!(
        attempt.navigation(&local_fragment),
        NavigationDecision::AllowTrusted
    ));
    assert!(!attempt.consumed.load(Ordering::Acquire));
    // These failed in 77a1a27 solely because a fragment was present. They model
    // sign-in page state changes, not real phone/QR behavior at Yandex.
    for uri in [
        "https://passport.yandex.ru/auth#phone",
        "https://passport.yandex.ru/auth#phone=synthetic-five-characters",
        "https://passport.yandex.ru/auth#qr-confirmation",
        "https://passport.yandex.ru/auth#state=synthetic-spa-step",
        "https://id.yandex.ru/#route?screen=synthetic-step",
        "https://smartcaptcha.yandexcloud.net/checkbox#challenge",
    ] {
        assert!(matches!(
            attempt.navigation(uri),
            NavigationDecision::AllowTrusted
        ));
        assert_eq!(attempt.navigation_rejection_reason(uri), None);
        assert!(!attempt.consumed.load(Ordering::Acquire));
    }
    assert_eq!(
        attempt.display_origin(
            "https://passport.yandex.ru/auth?phone=synthetic-private#phone=synthetic-private"
        ),
        Some("https://passport.yandex.ru".into())
    );
    // Page state has its own bounded URL budget; callback's 4 KiB ceiling must
    // not reject a trusted document's longer nested retpath/challenge query.
    let long_page = format!(
        "https://passport.yandex.ru/auth?retpath={}#phone",
        "x".repeat(MAX_RETURN + 1)
    );
    assert!(matches!(
        attempt.navigation(&long_page),
        NavigationDecision::AllowTrusted
    ));
    assert!(!attempt.consumed.load(Ordering::Acquire));
    let oversized_page = format!(
        "https://passport.yandex.ru/?challenge={}",
        "x".repeat(MAX_PAGE_URL)
    );
    assert_eq!(
        rejection(attempt.navigation(&oversized_page)),
        AuthError::UntrustedNavigation
    );
    let returned = callback(attempt, SYNTHETIC_CODE);
    for _ in 0..2 {
        assert!(matches!(
            attempt.navigation(&returned),
            NavigationDecision::CodeReturn
        ));
        assert!(!attempt.consumed.load(Ordering::Acquire));
    }
    assert_eq!(
        attempt.complete_code_return(&returned).unwrap().expose(),
        SYNTHETIC_CODE
    );
    assert_eq!(
        attempt.complete_code_return(&returned).unwrap_err(),
        AuthError::InvalidCallback
    );
    fixture.assert_no_provider_requests();
}

#[test]
fn oauth_result_fragments_never_become_ordinary_navigation_or_consume_an_attempt() {
    let fixture = SyntheticFixture::new(BrowserMode::Embedded);
    let attempt = &fixture.attempt;
    for fragment in [
        "access_token=synthetic-private-token",
        "%61ccess_token=synthetic-private-token",
        "ACCESS_TOKEN=synthetic-private-token",
        "route?code=synthetic-private-code&state=synthetic-private-state",
        "error=access_denied",
        "id_token=synthetic-private-token",
        "refresh_token=synthetic-private-token",
        "token_type=bearer",
        "expires_in=3600",
    ] {
        let uri = format!("https://passport.yandex.ru/auth#{fragment}");
        assert_eq!(
            rejection(attempt.navigation(&uri)),
            AuthError::UntrustedNavigation
        );
        assert_eq!(
            attempt.navigation_rejection_reason(&uri),
            Some(RejectionReason::OAuthResultOutsideCallback)
        );
        assert_eq!(attempt.display_origin(&uri), None);
        assert!(!attempt.consumed.load(Ordering::Acquire));
    }
    assert_eq!(
        attempt
            .complete_code_return(&callback(attempt, SYNTHETIC_CODE))
            .unwrap()
            .expose(),
        SYNTHETIC_CODE
    );
}

#[test]
fn rejection_reasons_are_fixed_and_do_not_change_navigation_policy() {
    let fixture = SyntheticFixture::new(BrowserMode::Embedded);
    let attempt = &fixture.attempt;
    for (uri, reason) in [
        (
            " https://passport.yandex.ru/",
            RejectionReason::InvalidAddress,
        ),
        (
            "https://user:private@passport.yandex.ru/",
            RejectionReason::CredentialsInAddress,
        ),
        (
            "https://untrusted.invalid/",
            RejectionReason::UntrustedOrigin,
        ),
        (
            "http://passport.yandex.ru/",
            RejectionReason::UnsupportedScheme,
        ),
        ("about:blank", RejectionReason::ServiceDocument),
        ("javascript:synthetic", RejectionReason::UnsupportedScheme),
    ] {
        assert!(matches!(
            attempt.navigation(uri),
            NavigationDecision::Reject(_)
        ));
        assert_eq!(attempt.navigation_rejection_reason(uri), Some(reason));
    }
    let malformed_return = format!(
        "{}#ordinary-fragment-is-invalid-on-code-return",
        callback(attempt, SYNTHETIC_CODE)
    );
    assert_eq!(
        attempt.navigation_rejection_reason(&malformed_return),
        Some(RejectionReason::InvalidCallback)
    );
    attempt.cancel();
    assert_eq!(
        attempt.navigation_rejection_reason("https://passport.yandex.ru/"),
        Some(RejectionReason::AttemptInactive)
    );
}

#[test]
fn sensitive_auth_material_is_redacted_from_debug_and_user_errors() {
    let fixture = SyntheticFixture::new(BrowserMode::Embedded);
    let attempt = &fixture.attempt;
    let code = AuthorizationCode(Zeroizing::new(SYNTHETIC_CODE.to_owned()));
    let code_debug = format!("{code:?}");
    let decision = NavigationDecision::CodeReturn;
    drop(code);
    let rendered = format!("{attempt:?} {decision:?} {code_debug}");
    assert!(rendered.contains("redacted"));
    for value in [
        attempt.state.as_str(),
        attempt.verifier(),
        SYNTHETIC_CODE,
        attempt.authorization.as_str(),
        attempt.callback_uri(),
    ] {
        assert!(!rendered.contains(value));
    }
    for error in [
        AuthError::MusicPermissionUnavailable,
        AuthError::Configuration,
        AuthError::Cancelled,
        AuthError::Expired,
        AuthError::StateMismatch,
        AuthError::InvalidCallback,
        AuthError::Denied,
        AuthError::Rejected,
        AuthError::Network,
        AuthError::Timeout,
        AuthError::ResponseTooLarge,
        AuthError::InvalidResponse,
        AuthError::UntrustedNavigation,
        AuthError::BrowserLoad,
        AuthError::BrowserTls,
        AuthError::BrowserProcess,
    ] {
        let text = format!("{error:?} {error} {}", error.user_message());
        assert!(!error.user_message().is_empty());
        for value in [attempt.state.as_str(), attempt.verifier(), SYNTHETIC_CODE] {
            assert!(!text.contains(value));
        }
    }
}

#[test]
fn displayed_origin_omits_private_url_parts_and_cannot_consume_a_callback() {
    let fixture = SyntheticFixture::new(BrowserMode::Embedded);
    let attempt = &fixture.attempt;
    assert_eq!(
        attempt.display_origin("https://passport.yandex.ru/private/path?code=synthetic-private-code&state=synthetic-private-state"),
        Some("https://passport.yandex.ru".to_owned())
    );
    assert_eq!(
        attempt.display_origin("https://passport.yandex.ru.invalid/path"),
        None
    );
    assert_eq!(
        attempt.display_origin("https://user:pass@passport.yandex.ru/path"),
        None
    );
    let returned = callback(attempt, SYNTHETIC_CODE);
    assert_eq!(attempt.display_origin(&returned), None);
    assert!(!attempt.consumed.load(Ordering::Acquire));
    assert_eq!(
        attempt.validate_callback(&returned).unwrap().expose(),
        SYNTHETIC_CODE
    );
    assert_eq!(attempt.display_origin("https://passport.yandex.ru/"), None);
    fixture.assert_no_provider_requests();
}

#[test]
fn automatic_loopback_rejects_unrelated_requests_then_accepts_valid_return() {
    let fixture = SyntheticFixture::new(BrowserMode::System);
    let attempt = &fixture.attempt;
    let (receiver, worker) = spawn_receiver(attempt);
    let target = callback_target(attempt);
    let address = attempt.address;
    let invalid_requests = [
        format!("GET {target} HTTP/1.1\r\nHost: invalid.test\r\n\r\n"),
        format!(
            "GET {target} HTTP/1.1\r\nHost: localhost:{}\r\n\r\n",
            address.port()
        ),
        format!("GET {target} HTTP/1.1\r\n\r\n"),
        request(attempt, "GET", "/unrelated?synthetic=value", ""),
        request(attempt, "GET", "/oauth/./callback?synthetic=value", ""),
        request(attempt, "POST", &target, ""),
        request(attempt, "GET", &target, &format!("Host: {address}\r\n")),
        request(attempt, "GET", &target, "Content-Length: 1\r\n"),
        request(attempt, "GET", &target, "Transfer-Encoding: chunked\r\n"),
        request(attempt, "GET", &target, "Malformed-header\r\n"),
        request(
            attempt,
            "GET",
            &target,
            &format!("X-Synthetic: {}\r\n", "x".repeat(MAX_REQUEST)),
        ),
    ];
    for request in invalid_requests {
        let response = send_request(attempt, &request);
        if request.len() >= MAX_REQUEST {
            // Rejecting oversized input leaves unread bytes: TCP can reset
            // before the complete fixed response reaches the client.
            assert!(response.is_empty() || response.starts_with("HTTP/1.1 400"));
            for secret in [SYNTHETIC_CODE, attempt.state.as_str(), attempt.verifier()] {
                assert!(!response.contains(secret));
            }
        } else {
            assert!(response.starts_with("HTTP/1.1 400 Bad Request\r\n"));
            assert_private_response(&response, attempt);
        }
        assert!(!attempt.consumed.load(Ordering::Acquire));
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }
    let response = send_request(
        attempt,
        &request(attempt, "GET", &target, "Content-Length: 0\r\n"),
    );
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert_private_response(&response, attempt);
    assert_eq!(
        receiver.recv_timeout(TEST_BOUND).unwrap().unwrap().expose(),
        SYNTHETIC_CODE
    );
    worker.join().unwrap();
    assert_eq!(
        attempt.receive_callback().unwrap_err(),
        AuthError::InvalidCallback
    );
    fixture.assert_no_provider_requests();
}

#[test]
fn automatic_loopback_does_not_echo_invalid_state_or_provider_error() {
    for (query, expected) in [
        (
            "state=synthetic-wrong-state&code=synthetic-private-opaque-code",
            AuthError::StateMismatch,
        ),
        (
            "state={state}&error=access_denied&error_description=synthetic-private-provider-message",
            AuthError::Denied,
        ),
    ] {
        let fixture = SyntheticFixture::new(BrowserMode::System);
        let attempt = &fixture.attempt;
        let (receiver, worker) = spawn_receiver(attempt);
        let query = query.replace("{state}", attempt.state.as_str());
        let target = format!("{CALLBACK_PATH}?{query}");
        let response = send_request(attempt, &request(attempt, "GET", &target, ""));
        assert!(response.starts_with("HTTP/1.1 400 Bad Request\r\n"));
        assert_private_response(&response, attempt);
        assert!(!response.contains("synthetic-wrong-state"));
        assert!(!response.contains("synthetic-private-provider-message"));
        assert_eq!(
            receiver.recv_timeout(TEST_BOUND).unwrap().unwrap_err(),
            expected
        );
        worker.join().unwrap();
    }
}

#[test]
fn cancelling_retained_attempt_releases_its_registered_port_for_retry() {
    for mode in [BrowserMode::Embedded, BrowserMode::System] {
        let fixture = SyntheticFixture::new(mode);
        let attempt = Arc::clone(&fixture.attempt);
        attempt.cancel();
        let retry = AuthAttempt::new("synthetic-karin-client", attempt.address.port(), mode)
            .expect("cancel must release the fixed port while old owners remain");
        assert_eq!(retry.address, attempt.address);
        assert!(attempt.listener.lock().unwrap().is_none());
        assert!(attempt.is_cancelled());
        fixture.assert_no_provider_requests();
    }
}

#[test]
fn consumed_callbacks_release_registered_port_with_attempt_still_owned() {
    for denied in [false, true] {
        let fixture = SyntheticFixture::new(BrowserMode::Embedded);
        let attempt = &fixture.attempt;
        if denied {
            let returned = callback_query(
                attempt,
                &format!("state={}&error=access_denied", attempt.state.as_str()),
            );
            assert_eq!(
                attempt.validate_callback(&returned).unwrap_err(),
                AuthError::Denied
            );
        } else {
            assert_eq!(
                attempt
                    .validate_callback(&callback(attempt, SYNTHETIC_CODE))
                    .unwrap()
                    .expose(),
                SYNTHETIC_CODE
            );
        }
        let retry = AuthAttempt::new(
            "synthetic-karin-client",
            attempt.address.port(),
            BrowserMode::Embedded,
        )
        .expect("terminal callback must release the fixed port before attempt drop");
        assert_eq!(retry.address, attempt.address);
        assert!(attempt.listener.lock().unwrap().is_none());
        fixture.assert_no_provider_requests();
    }
}

#[test]
fn automatic_receive_cancels_and_releases_port_before_worker_or_attempt_drop() {
    let fixture = SyntheticFixture::new(BrowserMode::System);
    let address: SocketAddr = fixture.attempt.address;
    let (receiver, worker) = spawn_receiver(&fixture.attempt);
    assert!(Arc::strong_count(&fixture.attempt) >= 2);
    fixture.attempt.cancel();
    let retry = AuthAttempt::new(
        "synthetic-karin-client",
        address.port(),
        BrowserMode::System,
    )
    .expect("cancel must release the fixed port before the receiving worker exits");
    assert_eq!(retry.address, address);
    assert_eq!(
        receiver.recv_timeout(TEST_BOUND).unwrap().unwrap_err(),
        AuthError::Cancelled
    );
    worker.join().unwrap();
    assert!(fixture.attempt.listener.lock().unwrap().is_none());
    drop(retry);
    drop(fixture);
    assert_eq!(
        TcpStream::connect_timeout(&address, TEST_BOUND)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::ConnectionRefused
    );
}

#[test]
fn automatic_receive_cancels_while_reading_partial_request() {
    let fixture = SyntheticFixture::new(BrowserMode::System);
    let attempt = &fixture.attempt;
    let address = attempt.address;
    let mut stream = TcpStream::connect_timeout(&address, TEST_BOUND).unwrap();
    stream.set_write_timeout(Some(TEST_BOUND)).unwrap();
    stream.set_read_timeout(Some(TEST_BOUND)).unwrap();
    stream
        .write_all(b"GET /oauth/callback?state=synthetic-partial")
        .unwrap();
    // Wait until receive_callback has accepted this particular connection, so
    // cancellation exercises its partial-header loop rather than idle accept.
    assert_eq!(pending_callback(attempt), 1);
    let (receiver, worker) = spawn_receiver(attempt);
    let deadline = Instant::now() + TEST_BOUND;
    while pending_callback(attempt) != 0 {
        assert!(
            Instant::now() < deadline,
            "receiver did not accept partial request"
        );
        thread::sleep(Duration::from_millis(5));
    }
    attempt.cancel();
    let retry = AuthAttempt::new(
        "synthetic-karin-client",
        address.port(),
        BrowserMode::System,
    )
    .expect("a partial callback reader must not retain the listening port");
    assert_eq!(
        receiver.recv_timeout(TEST_BOUND).unwrap().unwrap_err(),
        AuthError::Cancelled
    );
    worker.join().unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert_private_response(&response, attempt);
    assert!(!response.contains("synthetic-partial"));
    drop(stream);
    drop(retry);
    drop(fixture);
    assert_eq!(
        TcpStream::connect_timeout(&address, TEST_BOUND)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::ConnectionRefused
    );
}
