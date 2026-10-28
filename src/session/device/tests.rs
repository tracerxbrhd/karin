// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic loopback protocol tests. No provider, browser or real credentials.

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread::{self, JoinHandle},
};

use super::*;

const CLIENT_ID: &str = "synthetic-private-client";
const CLIENT_SECRET: &str = "synthetic-private-client-secret&+";
const DEVICE_CODE: &str = "synthetic-private-device-code";
const USER_CODE: &str = "SYNTHETIC-CODE";
const TOKEN: &str = "synthetic-private-access-token";
const CHALLENGE: &str = r#"{"device_code":"synthetic-private-device-code","user_code":"SYNTHETIC-CODE","verification_url":"https://oauth.yandex.ru/device","expires_in":600,"interval":1}"#;
const GRANT: &str = r#"{"access_token":"synthetic-private-access-token","refresh_token":"synthetic-private-refresh-token","token_type":"bearer","expires_in":3600,"private":"synthetic-private-profile"}"#;

fn response(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn accept(listener: &TcpListener) -> TcpStream {
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
            Err(error) => panic!("expected loopback request: {error}"),
        }
    }
}

fn request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 1);
        bytes.push(byte[0]);
        assert!(bytes.len() < 16 * 1024);
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
    assert!(length < 16 * 1024);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    bytes.extend(body);
    String::from_utf8(bytes).unwrap()
}

fn fixture(
    responses: Vec<(u16, String)>,
) -> (DeviceAuthClient, JoinHandle<Vec<String>>, TcpListener) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let retained = listener.try_clone().unwrap();
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let mut stream = accept(&listener);
            requests.push(request(&mut stream));
            if let Err(error) = stream.write_all(response(status, &body).as_bytes()) {
                assert!(matches!(
                    error.kind(),
                    std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                ));
            }
        }
        requests
    });
    let mut client = DeviceAuthClient::new(
        Zeroizing::new(CLIENT_ID.into()),
        Zeroizing::new(CLIENT_SECRET.into()),
    )
    .unwrap();
    client.http = Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .unwrap();
    client.begin_url = format!("http://{address}/device/code");
    client.token_url = format!("http://{address}/token");
    client.timeout = Duration::from_millis(500);
    (client, server, retained)
}

fn arm_poll(challenge: &DeviceChallenge) {
    challenge.state.lock().unwrap().next_poll = Instant::now();
}

fn finish(server: JoinHandle<Vec<String>>, retained: TcpListener) -> Vec<String> {
    let requests = server.join().unwrap();
    retained.set_nonblocking(true).unwrap();
    assert_eq!(
        retained.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "no automatic retries or unexpected endpoint requests"
    );
    requests
}

fn private_markers_absent(value: &impl fmt::Debug) {
    let visible = format!("{value:?}");
    for marker in [
        CLIENT_ID,
        CLIENT_SECRET,
        DEVICE_CODE,
        USER_CODE,
        TOKEN,
        "synthetic-private-refresh-token",
        "synthetic-private-profile",
    ] {
        assert!(!visible.contains(marker), "private marker in Debug");
    }
}

#[test]
fn native_challenge_and_secret_form_exchange_remain_memory_only_and_redacted() {
    let (client, server, retained) = fixture(vec![(200, CHALLENGE.into()), (200, GRANT.into())]);
    let challenge = client.begin().unwrap();
    assert_eq!(challenge.user_code(), USER_CODE);
    assert_eq!(
        challenge.verification_url(),
        "https://oauth.yandex.ru/device"
    );
    assert!(!challenge.expires_in().is_zero());
    assert_eq!(challenge.interval(), Duration::from_secs(1));
    // An early caller cannot send a poll before the provider interval.
    assert!(matches!(
        client.poll(&challenge).unwrap(),
        DevicePoll::Pending
    ));
    arm_poll(&challenge);
    let result = client.poll(&challenge).unwrap();
    let DevicePoll::Authorized(token) = &result else {
        panic!("expected synthetic grant");
    };
    assert_eq!(token.expose(), TOKEN);
    assert!(!token.is_expired());
    for value in [
        format!("{client:?}"),
        format!("{challenge:?}"),
        format!("{result:?}"),
    ] {
        assert!(!value.contains("synthetic-private"));
        assert!(!value.contains(USER_CODE));
    }
    private_markers_absent(&client);
    private_markers_absent(&challenge);
    private_markers_absent(&result);
    assert_eq!(client.poll(&challenge).unwrap_err(), SessionError::Rejected);
    let requests = finish(server, retained);
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("POST /device/code HTTP/1.1"));
    assert!(requests[0].contains("device_name=Karin+development+experiment"));
    assert!(requests[0].contains("device_id="));
    assert!(!requests[0].contains("client_secret="));
    assert!(requests[1].starts_with("POST /token HTTP/1.1"));
    assert!(requests[1].contains("grant_type=device_code"));
    assert!(requests[1].contains(&format!("code={DEVICE_CODE}")));
    assert!(requests[1].contains("client_secret=synthetic-private-client-secret%26%2B"));
    assert!(
        requests
            .iter()
            .all(|value| !value.to_ascii_lowercase().contains("authorization:"))
    );
    assert!(requests.iter().all(|value| {
        !value
            .to_ascii_lowercase()
            .contains("x-yandex-music-client:")
    }));
}

#[test]
fn pending_slow_down_cancellation_and_shared_interval_are_enforced() {
    let (client, server, retained) = fixture(vec![
        (200, CHALLENGE.into()),
        (
            400,
            r#"{"error":"authorization_pending","error_description":"synthetic-private-profile"}"#
                .into(),
        ),
        (
            400,
            r#"{"error":"slow_down","error_description":"synthetic-private-profile"}"#.into(),
        ),
    ]);
    let challenge = client.begin().unwrap();
    arm_poll(&challenge);
    assert!(matches!(
        client.poll(&challenge).unwrap(),
        DevicePoll::Pending
    ));
    arm_poll(&challenge);
    assert!(matches!(
        client.poll(&challenge).unwrap(),
        DevicePoll::SlowDown
    ));
    assert_eq!(challenge.interval(), Duration::from_secs(6));
    assert!(matches!(
        client.poll(&challenge.clone()).unwrap(),
        DevicePoll::Pending
    ));
    challenge.clone().cancel();
    assert_eq!(
        client.poll(&challenge).unwrap_err(),
        SessionError::Cancelled
    );
    assert_eq!(finish(server, retained).len(), 3);
}

#[test]
fn denied_expired_and_local_expiry_do_not_poll_again() {
    for (body, denied) in [
        (
            r#"{"error":"access_denied","message":"synthetic-private-profile"}"#,
            true,
        ),
        (r#"{"error":"expired_token"}"#, false),
        (r#"{"error":"code_expired"}"#, false),
    ] {
        let (client, server, retained) = fixture(vec![(200, CHALLENGE.into()), (400, body.into())]);
        let challenge = client.begin().unwrap();
        arm_poll(&challenge);
        let result = client.poll(&challenge).unwrap();
        assert_eq!(matches!(result, DevicePoll::Denied), denied);
        assert_eq!(matches!(result, DevicePoll::Expired), !denied);
        assert_eq!(client.poll(&challenge).unwrap_err(), SessionError::Rejected);
        private_markers_absent(&result);
        assert_eq!(finish(server, retained).len(), 2);
    }
    let (client, server, retained) = fixture(vec![(200, CHALLENGE.into())]);
    let mut challenge = client.begin().unwrap();
    challenge.expires_at = Instant::now();
    assert!(matches!(
        client.poll(&challenge).unwrap(),
        DevicePoll::Expired
    ));
    assert_eq!(finish(server, retained).len(), 1);
}

#[test]
fn provider_messages_and_unknown_errors_are_never_public_categories() {
    for (status, body, expected) in [
        (
            400,
            r#"{"error":"invalid_client","error_description":"synthetic-private-access-token"}"#,
            SessionError::ClientRejected,
        ),
        (
            401,
            r#"{"error":"unauthorized_client"}"#,
            SessionError::ClientRejected,
        ),
        (
            403,
            r#"{"error":"synthetic-private-profile"}"#,
            SessionError::Rejected,
        ),
        (500, "synthetic-private-profile", SessionError::Rejected),
        (
            500,
            r#"{"error":"authorization_pending"}"#,
            SessionError::Rejected,
        ),
        (
            200,
            "synthetic-private-profile",
            SessionError::InvalidResponse,
        ),
        (
            200,
            r#"{"access_token":"synthetic-private-access-token","token_type":"MAC"}"#,
            SessionError::InvalidResponse,
        ),
        (
            200,
            r#"{"access_token":"synthetic-private-access-token","expires_in":0}"#,
            SessionError::InvalidResponse,
        ),
        (
            200,
            r#"{"access_token":"synthetic-private-access-token","expires_in":null}"#,
            SessionError::InvalidResponse,
        ),
        (
            200,
            r#"{"access_token":"synthetic-private-access-token","token_type":null}"#,
            SessionError::InvalidResponse,
        ),
        (
            200,
            r#"{"access_token":"synthetic-private-access-token","error":null}"#,
            SessionError::InvalidResponse,
        ),
        (
            200,
            r#"{"access_token":"has space"}"#,
            SessionError::InvalidResponse,
        ),
        (
            200,
            r#"{"access_token":"has\ncontrol"}"#,
            SessionError::InvalidResponse,
        ),
        (
            200,
            r#"{"access_token":"synthetic-private-access-token","access_token":"duplicate"}"#,
            SessionError::InvalidResponse,
        ),
        (
            200,
            r#"{"access_token":"synthetic-private-access-token","expires_in":18446744073709551615}"#,
            SessionError::InvalidResponse,
        ),
    ] {
        let (client, server, retained) =
            fixture(vec![(200, CHALLENGE.into()), (status, body.into())]);
        let challenge = client.begin().unwrap();
        arm_poll(&challenge);
        let error = client.poll(&challenge).unwrap_err();
        assert_eq!(error, expected);
        assert!(!error.to_string().contains("synthetic-private"));
        private_markers_absent(&error);
        finish(server, retained);
    }
    // The inspected provider implementation permits both optional fields to be
    // absent. No fabricated server expiry or token refresh is inferred.
    let (client, server, retained) = fixture(vec![
        (200, CHALLENGE.into()),
        (
            200,
            r#"{"access_token":"synthetic-private-access-token"}"#.into(),
        ),
    ]);
    let challenge = client.begin().unwrap();
    arm_poll(&challenge);
    let DevicePoll::Authorized(token) = client.poll(&challenge).unwrap() else {
        panic!("expected synthetic grant");
    };
    assert_eq!(token.expires_at, None);
    finish(server, retained);
}

#[test]
fn challenge_validation_rejects_redirects_untrusted_urls_and_malformed_codes() {
    for url in ["https://oauth.yandex.ru/device", "https://ya.ru/device"] {
        let body = CHALLENGE.replace("https://oauth.yandex.ru/device", url);
        let (client, server, retained) = fixture(vec![(200, body)]);
        let challenge = client.begin().unwrap();
        assert_eq!(challenge.verification_url(), url);
        finish(server, retained);
    }
    for url in [
        "http://oauth.yandex.ru/device",
        "https://oauth.yandex.ru.evil.invalid/device",
        "https://evil.invalid/device",
        "https://oauth.yandex.ru:8443/device",
        "https://user:pass@oauth.yandex.ru/device",
        "https://oauth.yandex.ru/device?code=private",
        "https://oauth.yandex.ru/device#private",
        "https://oauth.yandex.ru/token",
        "javascript:alert(1)",
        "https://127.0.0.1/device",
        "https://ya.ru.evil.invalid/device",
        "https://ya.ru/device?code=private",
        "https://oauth.yandex.ru/dev\nice",
    ] {
        assert!(!valid_verification_url(url));
        let body = CHALLENGE.replace("https://oauth.yandex.ru/device", url);
        let (client, server, retained) = fixture(vec![(200, body)]);
        assert_eq!(client.begin().unwrap_err(), SessionError::InvalidResponse);
        finish(server, retained);
    }
    for body in [
        CHALLENGE.replace("\"expires_in\":600", "\"expires_in\":0"),
        CHALLENGE.replace("\"interval\":1", "\"interval\":0"),
        CHALLENGE.replace(USER_CODE, "control\\nvalue"),
        CHALLENGE.replace(DEVICE_CODE, ""),
        CHALLENGE.replace("\"interval\":1", "\"interval\":1,\"error\":null"),
        "{}".into(),
        r#"{"error":"authorization_pending"}"#.into(),
    ] {
        let (client, server, retained) = fixture(vec![(200, body)]);
        assert_eq!(client.begin().unwrap_err(), SessionError::InvalidResponse);
        finish(server, retained);
    }
    let (client, server, retained) = fixture(vec![(302, CHALLENGE.into())]);
    assert_eq!(client.begin().unwrap_err(), SessionError::Rejected);
    assert_eq!(finish(server, retained).len(), 1);
}

#[test]
fn response_bound_and_transport_failures_are_fixed_errors() {
    let (client, server, retained) = fixture(vec![(200, "x".repeat(MAX_BODY + 1))]);
    assert_eq!(client.begin().unwrap_err(), SessionError::ResponseTooLarge);
    finish(server, retained);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let mut client = DeviceAuthClient::new(
        Zeroizing::new(CLIENT_ID.into()),
        Zeroizing::new(CLIENT_SECRET.into()),
    )
    .unwrap();
    client.http = Client::builder().no_proxy().build().unwrap();
    client.begin_url = format!("http://{address}/device/code");
    client.timeout = Duration::from_millis(150);
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        request(&mut stream);
        thread::sleep(Duration::from_millis(400));
    });
    assert_eq!(client.begin().unwrap_err(), SessionError::Timeout);
    server.join().unwrap();
    client.begin_url = "http://127.0.0.1:1/device/code".into();
    assert_eq!(client.begin().unwrap_err(), SessionError::Network);
}

#[test]
fn invalid_application_configuration_never_constructs_a_network_client() {
    for (id, secret) in [
        ("", "valid"),
        ("client", ""),
        ("client\n", "valid"),
        ("client", "secret value"),
    ] {
        assert_eq!(
            DeviceAuthClient::new(Zeroizing::new(id.into()), Zeroizing::new(secret.into()))
                .unwrap_err(),
            SessionError::Configuration
        );
    }
}

#[test]
fn cancellation_discards_an_in_flight_grant_without_retaining_a_session() {
    use std::sync::mpsc;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (seen_sender, seen_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut begin = accept(&listener);
        request(&mut begin);
        begin
            .write_all(response(200, CHALLENGE).as_bytes())
            .unwrap();
        let mut poll = accept(&listener);
        request(&mut poll);
        seen_sender.send(()).unwrap();
        release_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        poll.write_all(response(200, GRANT).as_bytes()).unwrap();
    });
    let mut client = DeviceAuthClient::new(
        Zeroizing::new(CLIENT_ID.into()),
        Zeroizing::new(CLIENT_SECRET.into()),
    )
    .unwrap();
    client.http = Client::builder().no_proxy().build().unwrap();
    client.begin_url = format!("http://{address}/device/code");
    client.token_url = format!("http://{address}/token");
    client.timeout = Duration::from_secs(2);
    let challenge = client.begin().unwrap();
    arm_poll(&challenge);
    let worker_challenge = challenge.clone();
    let worker = thread::spawn(move || client.poll(&worker_challenge));
    seen_receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    challenge.cancel();
    release_sender.send(()).unwrap();
    assert_eq!(worker.join().unwrap().unwrap_err(), SessionError::Cancelled);
    server.join().unwrap();
}
