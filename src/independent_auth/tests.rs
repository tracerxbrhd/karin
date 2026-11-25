// SPDX-License-Identifier: GPL-3.0-or-later
//! The browser, OAuth, ID and Music endpoints below are loopback fixtures.

use super::*;
use crate::{
    credentials::{BackendKind, CredentialBackend},
    oauth::ErrorCategory,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::Url;
use ring::{
    digest::{SHA256, digest},
    rand::{SecureRandom, SystemRandom},
};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::DirBuilderExt,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const CLIENT: &str = "synthetic-karin-client";
const CODE: &str = "fixture-opaque-code-not-from-provider";
const TOKEN: &str = "fixture-independent-access-token";
const TOKEN_JSON: &str = r#"{"access_token":"fixture-independent-access-token","refresh_token":"fixture-ignored-refresh-token","token_type":"bearer","expires_in":3600}"#;
const ID_JSON: &str =
    r#"{"client_id":"synthetic-karin-client","id":"4123","login":"fixture-private-login"}"#;
const MUSIC_JSON: &str =
    r#"{"result":{"account":{"uid":4123,"displayName":"fixture-private-person"}}}"#;

enum Endpoint {
    Token,
    Identity,
    Music,
}

struct Reply(Endpoint, u16, &'static str);

const EXCHANGE: Reply = Reply(Endpoint::Token, 200, TOKEN_JSON);
const IDENTIFY: Reply = Reply(Endpoint::Identity, 200, ID_JSON);
const MUSIC: Reply = Reply(Endpoint::Music, 200, MUSIC_JSON);

struct ApiFixture {
    origin: String,
    listener: TcpListener,
    server: JoinHandle<Option<String>>,
}

impl ApiFixture {
    fn new(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server_listener = listener.try_clone().unwrap();
        let location = format!("{origin}/must-not-follow");
        let server = thread::spawn(move || {
            let mut challenge = None;
            for Reply(endpoint, status, body) in replies {
                let deadline = Instant::now() + Duration::from_secs(4);
                let mut stream = loop {
                    match server_listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(err)
                            if err.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(err) => panic!("fixture request did not arrive: {err}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let (request, headers, form) = read_request(&stream);
                assert!(!headers.contains_key("cookie"));
                match endpoint {
                    Endpoint::Token => {
                        assert_eq!(request, "POST /token HTTP/1.1");
                        assert!(!headers.contains_key("authorization"));
                        assert_eq!(headers["content-type"], "application/x-www-form-urlencoded");
                        let form_url = Url::parse(&format!("http://127.0.0.1/?{form}")).unwrap();
                        let fields: BTreeMap<_, _> = form_url.query_pairs().collect();
                        assert_eq!(fields.len(), 4);
                        assert_eq!(fields["grant_type"], "authorization_code");
                        assert_eq!(fields["client_id"], CLIENT);
                        assert_eq!(fields["code"], CODE);
                        let verifier = &fields["code_verifier"];
                        assert_eq!(verifier.len(), 43);
                        challenge = Some(
                            URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()).as_ref()),
                        );
                    }
                    Endpoint::Identity | Endpoint::Music => {
                        let path = match endpoint {
                            Endpoint::Identity => "/info?format=json",
                            Endpoint::Music => "/account/status",
                            Endpoint::Token => unreachable!(),
                        };
                        assert_eq!(request, format!("GET {path} HTTP/1.1"));
                        assert_eq!(headers["authorization"], format!("OAuth {TOKEN}"));
                        assert!(form.is_empty());
                    }
                }
                write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nLocation: {location}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            challenge
        });
        Self {
            origin,
            listener,
            server,
        }
    }

    fn auth(&self, storage: &StorageFixture) -> IndependentAuth {
        IndependentAuth {
            oauth: OAuthClient::for_independent_test(&self.origin),
            music: MusicService::for_test(&self.origin),
            store: CredentialStore::with_backend(storage.backend.clone()),
            state_directory: storage.directory.clone(),
        }
    }

    fn finish(self) -> Option<String> {
        let challenge = self.server.join().unwrap();
        assert_eq!(
            self.listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "unexpected additional request"
        );
        challenge
    }
}

fn read_request(stream: &TcpStream) -> (String, BTreeMap<String, String>, String) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let request = line.trim_end().to_owned();
    let mut headers = BTreeMap::new();
    loop {
        line.clear();
        assert_ne!(reader.read_line(&mut line).unwrap(), 0);
        if line == "\r\n" {
            break;
        }
        let (key, value) = line.split_once(':').unwrap();
        assert!(
            headers
                .insert(key.to_ascii_lowercase(), value.trim().to_owned())
                .is_none()
        );
    }
    let length = headers
        .get("content-length")
        .map_or(0, |value| value.parse().unwrap());
    assert!(length <= 8192);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    (request, headers, String::from_utf8(body).unwrap())
}

#[derive(Default)]
struct ProtectedStorage {
    token: Option<AccessToken>,
    saves: usize,
    loads: usize,
    deletes: usize,
    fail_save: bool,
    fail_delete: bool,
}

impl CredentialBackend for Arc<Mutex<ProtectedStorage>> {
    fn probe(&self, _: &Cancellation) -> Result<BackendKind, StorageError> {
        Ok(BackendKind::SecretService)
    }

    fn load(&self, _: &Cancellation) -> Result<Option<AccessToken>, StorageError> {
        let mut storage = self.lock().unwrap();
        storage.loads += 1;
        Ok(storage.token.clone())
    }

    fn save_validated(&self, token: &AccessToken, _: &Cancellation) -> Result<(), StorageError> {
        let mut storage = self.lock().unwrap();
        assert_eq!(token.expose(), TOKEN);
        storage.saves += 1;
        storage.token = token.clone_for_storage();
        if storage.fail_save {
            // A backend can commit a write and fail to confirm it to the client.
            return Err(StorageError::Unconfirmed);
        }
        Ok(())
    }

    fn delete(&self, _: &Cancellation) -> Result<(), StorageError> {
        let mut storage = self.lock().unwrap();
        storage.deletes += 1;
        if storage.fail_delete {
            return Err(StorageError::Unconfirmed);
        }
        storage.token = None;
        Ok(())
    }
}

struct StorageFixture {
    backend: Arc<Mutex<ProtectedStorage>>,
    directory: PathBuf,
}

impl StorageFixture {
    fn new() -> Self {
        let mut random = [0; 16];
        SystemRandom::new().fill(&mut random).unwrap();
        let directory =
            std::env::temp_dir().join(format!("karin-ind-auth-{}", URL_SAFE_NO_PAD.encode(random)));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        Self {
            backend: Arc::default(),
            directory,
        }
    }
}

impl Drop for StorageFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn browser_return(url: &str) -> (TcpStream, String) {
    let url = Url::parse(url).unwrap();
    assert_eq!(url.path(), "/authorize");
    let parameters: BTreeMap<_, _> = url.query_pairs().collect();
    assert_eq!(parameters["response_type"], "code");
    assert_eq!(parameters["client_id"], CLIENT);
    assert_eq!(parameters["code_challenge_method"], "S256");
    assert!(!parameters.contains_key("client_secret"));
    let redirect = Url::parse(&parameters["redirect_uri"]).unwrap();
    assert_eq!(redirect.scheme(), "http");
    assert_eq!(redirect.host_str(), Some("127.0.0.1"));
    assert_eq!(redirect.path(), "/oauth/callback");
    let address = format!("127.0.0.1:{}", redirect.port().unwrap());
    let mut browser = TcpStream::connect(&address).unwrap();
    browser
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        browser,
        "GET /oauth/callback?code={CODE}&state={} HTTP/1.1\r\nHost: {address}\r\n\r\n",
        parameters["state"]
    )
    .unwrap();
    (browser, parameters["code_challenge"].to_string())
}

fn login(
    auth: &mut IndependentAuth,
    api: &ApiFixture,
    remember: bool,
) -> (Result<Outcome, Error>, String) {
    let attempt =
        AuthAttempt::for_test(&format!("{}/authorize", api.origin), BrowserMode::System).unwrap();
    let mut browser = None;
    let outcome =
        auth.login_with_attempt(&attempt, CLIENT, remember, &Cancellation::new(), |url| {
            browser = Some(browser_return(url));
            Ok(())
        });
    let (mut stream, challenge) = browser.expect("the loopback browser was opened");
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 "));
    assert!(response.contains("Cache-Control: no-store\r\n"));
    assert!(!response.contains(CODE));
    (outcome, challenge)
}

#[test]
fn loopback_pkce_validates_before_remembering_then_revalidates_and_forgets() {
    let storage = StorageFixture::new();
    let api = ApiFixture::new(vec![EXCHANGE, IDENTIFY, MUSIC]);
    let (outcome, browser_challenge) = login(&mut api.auth(&storage), &api, true);
    assert_eq!(outcome, Ok(Outcome::Remembered));
    assert_eq!(api.finish().as_deref(), Some(browser_challenge.as_str()));
    assert_eq!(storage.backend.lock().unwrap().saves, 1);

    let api = ApiFixture::new(vec![IDENTIFY, MUSIC]);
    let mut after_restart = api.auth(&storage);
    let cancel = Cancellation::new();
    assert_eq!(
        after_restart.restore(CLIENT, &cancel),
        Ok(Outcome::Restored)
    );
    assert_eq!(
        forget(
            &after_restart.store,
            &after_restart.state_directory,
            &cancel
        ),
        Ok(Outcome::LoggedOut)
    );
    assert_eq!(
        after_restart.restore(CLIENT, &cancel),
        Err(Error::NoSavedSession)
    );
    assert_eq!(api.finish(), None);
    let storage = storage.backend.lock().unwrap();
    assert_eq!(storage.saves, 1);
    assert_eq!(storage.deletes, 1);
    assert!(storage.token.is_none());
}

#[test]
fn access_and_identity_failures_never_save_or_replay_the_exchange() {
    let storage = StorageFixture::new();
    for (replies, expected) in [
        (
            vec![
                EXCHANGE,
                IDENTIFY,
                Reply(
                    Endpoint::Music,
                    403,
                    r#"{"error":"fixture-private-refusal"}"#,
                ),
            ],
            Error::Music(MusicError::Forbidden),
        ),
        (
            vec![
                EXCHANGE,
                IDENTIFY,
                Reply(
                    Endpoint::Music,
                    200,
                    r#"{"result":{"account":{"uid":4124}}}"#,
                ),
            ],
            Error::AccountMismatch,
        ),
        (
            vec![
                EXCHANGE,
                Reply(
                    Endpoint::Identity,
                    200,
                    r#"{"client_id":"other-client","id":"4123"}"#,
                ),
            ],
            Error::OAuth(ProbeResult {
                http_status: Some(200),
                error: Some(ErrorCategory::ClientMismatch),
            }),
        ),
        (
            vec![EXCHANGE, Reply(Endpoint::Identity, 302, "")],
            Error::OAuth(ProbeResult {
                http_status: Some(302),
                error: Some(ErrorCategory::Http),
            }),
        ),
    ] {
        let api = ApiFixture::new(replies);
        let (outcome, _) = login(&mut api.auth(&storage), &api, true);
        assert_eq!(outcome, Err(expected));
        assert!(!format!("{expected:?} {expected}").contains("fixture-private"));
        api.finish();
    }
    assert_eq!(storage.backend.lock().unwrap().saves, 0);
}

#[test]
fn memory_only_login_ignores_protected_storage_and_keeps_existing_token() {
    let storage = StorageFixture::new();
    storage.backend.lock().unwrap().token =
        Some(AccessToken::for_test(TOKEN, Duration::from_secs(60)));
    let api = ApiFixture::new(vec![EXCHANGE, IDENTIFY, MUSIC]);
    assert_eq!(
        login(&mut api.auth(&storage), &api, false).0,
        Ok(Outcome::Validated)
    );
    api.finish();
    let storage = storage.backend.lock().unwrap();
    assert_eq!((storage.saves, storage.loads, storage.deletes), (0, 0, 0));
    assert!(storage.token.is_some());
}

#[test]
fn restore_refusal_preserves_protected_storage_and_a_retry_revalidates() {
    let storage = StorageFixture::new();
    storage.backend.lock().unwrap().token =
        Some(AccessToken::for_test(TOKEN, Duration::from_secs(60)));
    let api = ApiFixture::new(vec![
        IDENTIFY,
        Reply(
            Endpoint::Music,
            403,
            r#"{"error":"fixture-private-refusal"}"#,
        ),
        IDENTIFY,
        MUSIC,
    ]);
    let mut auth = api.auth(&storage);
    let cancel = Cancellation::new();
    assert_eq!(
        auth.restore(CLIENT, &cancel),
        Err(Error::Music(MusicError::Forbidden))
    );
    assert!(storage.backend.lock().unwrap().token.is_some());
    assert_eq!(auth.restore(CLIENT, &cancel), Ok(Outcome::Restored));
    api.finish();
    let storage = storage.backend.lock().unwrap();
    assert_eq!((storage.saves, storage.loads, storage.deletes), (0, 2, 0));
}

#[test]
fn failed_protected_write_and_forget_fence_restart_until_confirmed_cleanup() {
    let storage = StorageFixture::new();
    storage.backend.lock().unwrap().fail_save = true;
    let api = ApiFixture::new(vec![EXCHANGE, IDENTIFY, MUSIC]);
    assert_eq!(
        login(&mut api.auth(&storage), &api, true).0,
        Err(Error::Storage(StorageError::Unconfirmed))
    );
    api.finish();
    let api = ApiFixture::new(vec![]);
    let mut after_restart = api.auth(&storage);
    let cancel = Cancellation::new();
    assert_eq!(
        after_restart.restore(CLIENT, &cancel),
        Err(Error::PendingStorage)
    );
    assert_eq!(storage.backend.lock().unwrap().loads, 0);
    assert!(storage.backend.lock().unwrap().token.is_some());

    storage.backend.lock().unwrap().fail_delete = true;
    assert_eq!(
        forget(
            &after_restart.store,
            &after_restart.state_directory,
            &cancel
        ),
        Err(Error::Storage(StorageError::Unconfirmed))
    );
    assert_eq!(
        api.auth(&storage).restore(CLIENT, &cancel),
        Err(Error::PendingStorage)
    );
    storage.backend.lock().unwrap().fail_delete = false;
    assert_eq!(
        forget(
            &after_restart.store,
            &after_restart.state_directory,
            &cancel
        ),
        Ok(Outcome::LoggedOut)
    );
    assert_eq!(
        after_restart.restore(CLIENT, &cancel),
        Err(Error::NoSavedSession)
    );
    api.finish();
}

#[test]
fn cancellation_closes_loopback_without_exchange_even_with_an_incomplete_callback() {
    let storage = StorageFixture::new();
    let api = ApiFixture::new(vec![]);
    let mut auth = api.auth(&storage);
    let attempt =
        AuthAttempt::for_test(&format!("{}/authorize", api.origin), BrowserMode::System).unwrap();
    let cancel = Cancellation::new();
    cancel.cancel();
    assert_eq!(
        auth.login_with_attempt(&attempt, CLIENT, false, &cancel, |_| panic!(
            "cancelled attempt must not open the browser"
        )),
        Err(Error::Authorization(AuthError::Cancelled))
    );

    let attempt =
        AuthAttempt::for_test(&format!("{}/authorize", api.origin), BrowserMode::System).unwrap();
    let cancel = Cancellation::new();
    let mut sender = None;
    let result = auth.login_with_attempt(&attempt, CLIENT, false, &cancel, |_| {
        let callback = Url::parse(attempt.callback_uri()).unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", callback.port().unwrap())).unwrap();
        stream
            .write_all(b"GET /oauth/callback?unfinished HTTP/1.1\r\n")
            .unwrap();
        let cancel = cancel.clone();
        sender = Some(thread::spawn(move || {
            thread::sleep(Duration::from_millis(70));
            cancel.cancel();
            stream
        }));
        Ok(())
    });
    assert_eq!(result, Err(Error::Authorization(AuthError::Cancelled)));
    drop(sender.unwrap().join().unwrap());
    let callback = Url::parse(attempt.callback_uri()).unwrap();
    assert!(TcpStream::connect(("127.0.0.1", callback.port().unwrap())).is_err());
    api.finish();
    assert_eq!(storage.backend.lock().unwrap().saves, 0);
}
