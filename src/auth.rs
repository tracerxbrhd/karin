// SPDX-License-Identifier: GPL-3.0-or-later
//! Browser/session boundary. Public sign-in stays blocked until Karin has a
//! permitted Music client. A browser login or OAuth grant is never Music proof.

use std::{
    fmt,
    io::{Read, Write},
    net::{SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::Url;
use ring::{
    digest::{SHA256, digest},
    rand::{SecureRandom, SystemRandom},
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::session::AccessToken;

const CALLBACK_PATH: &str = "/oauth/callback";
const MAX_RETURN: usize = 4096;
const MAX_REQUEST: usize = 8192;
const LIFETIME: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthError {
    MusicPermissionUnavailable,
    Configuration,
    Cancelled,
    Expired,
    StateMismatch,
    InvalidCallback,
    Denied,
    Rejected,
    Network,
    Timeout,
    ResponseTooLarge,
    InvalidResponse,
    UntrustedNavigation,
    BrowserLoad,
    BrowserTls,
    BrowserProcess,
}

impl AuthError {
    pub const fn user_message(self) -> &'static str {
        match self {
            Self::MusicPermissionUnavailable => {
                "Вход в Яндекс Музыку пока недоступен: разрешённый Music-доступ для приложения Karin не подтверждён. Это не ошибка вашего пароля."
            }
            Self::Configuration => "Не удалось подготовить безопасный вход. Попробуйте ещё раз.",
            Self::Cancelled => "Вход отменён.",
            Self::Expired => "Время ожидания входа истекло. Начните заново.",
            Self::StateMismatch | Self::InvalidCallback => {
                "Не удалось проверить ответ авторизации. Начните вход заново."
            }
            Self::Denied => "Разрешение на вход не предоставлено.",
            Self::Rejected => "Сервис отклонил запрос авторизации.",
            Self::Network => "Не удалось связаться с сервисом авторизации.",
            Self::Timeout => "Сервис авторизации не ответил вовремя.",
            Self::ResponseTooLarge | Self::InvalidResponse => {
                "Сервис вернул неподдерживаемый ответ."
            }
            Self::UntrustedNavigation => {
                "Переход за пределы разрешённой страницы входа заблокирован."
            }
            Self::BrowserLoad => {
                "Не удалось загрузить страницу входа. Проверьте соединение и повторите попытку."
            }
            Self::BrowserTls => "Не удалось проверить защищённое соединение со страницей входа.",
            Self::BrowserProcess => "Окно входа завершило работу. Попробуйте ещё раз.",
        }
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.user_message())
    }
}
impl std::error::Error for AuthError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserMode {
    /// RFC 8252 default; needs an actually registered, supported fixed callback.
    System,
    /// Only for a provider that separately permits this embedded integration.
    Embedded,
}

/// Implementations must establish the right to use their client identity. The
/// public factory deliberately supplies no provider for the known 403 client.
pub trait AuthProvider: Send + Sync {
    fn begin(&self) -> Result<Arc<AuthAttempt>, AuthError>;
    fn exchange(
        &self,
        attempt: &AuthAttempt,
        code: AuthorizationCode,
    ) -> Result<AccessToken, AuthError>;
}

struct BlockedProvider;
impl AuthProvider for BlockedProvider {
    fn begin(&self) -> Result<Arc<AuthAttempt>, AuthError> {
        Err(AuthError::MusicPermissionUnavailable)
    }
    fn exchange(&self, _: &AuthAttempt, _: AuthorizationCode) -> Result<AccessToken, AuthError> {
        Err(AuthError::MusicPermissionUnavailable)
    }
}

pub fn public_provider() -> Arc<dyn AuthProvider> {
    Arc::new(BlockedProvider)
}

/// Single-use callback material. Never render or log this value.
pub struct AuthorizationCode(Zeroizing<String>);
impl AuthorizationCode {
    /// Only the provider's code exchange may consume this value; never log it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for AuthorizationCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthorizationCode([redacted])")
    }
}

pub enum NavigationDecision {
    AllowTrusted,
    Callback(AuthorizationCode),
    Reject(AuthError),
}
impl fmt::Debug for NavigationDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllowTrusted => f.write_str("AllowTrusted"),
            Self::Callback(_) => f.write_str("Callback([redacted])"),
            Self::Reject(error) => f.debug_tuple("Reject").field(error).finish(),
        }
    }
}

/// A bounded browser attempt. No production provider constructs one today.
/// Loopback ports MUST match a provider-approved registration; random-port
/// acceptance is not inferred from RFC 8252 or from synthetic tests.
pub struct AuthAttempt {
    authorization: Zeroizing<String>,
    callback: String,
    state: Zeroizing<String>,
    verifier: Zeroizing<String>,
    listener: Mutex<Option<TcpListener>>,
    address: SocketAddr,
    mode: BrowserMode,
    created: Instant,
    cancelled: AtomicBool,
    consumed: AtomicBool,
    #[cfg(test)]
    fixture_origin: Option<String>,
}

impl fmt::Debug for AuthAttempt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthAttempt([redacted])")
    }
}

impl AuthAttempt {
    /// Preparing a PKCE request establishes neither provider permission nor
    /// Music access. This is not connected to the public sign-in factory.
    pub fn new(
        client_id: &str,
        registered_port: u16,
        mode: BrowserMode,
    ) -> Result<Arc<Self>, AuthError> {
        if registered_port < 1024 {
            return Err(AuthError::Configuration);
        }
        Self::prepare(
            client_id,
            registered_port,
            mode,
            "https://oauth.yandex.ru/authorize",
        )
    }

    fn prepare(
        client_id: &str,
        port: u16,
        mode: BrowserMode,
        endpoint: &str,
    ) -> Result<Arc<Self>, AuthError> {
        if client_id.is_empty()
            || client_id.len() > 128
            || !client_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err(AuthError::Configuration);
        }
        let listener = TcpListener::bind(SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, port))
            .map_err(|_| AuthError::Configuration)?;
        listener
            .set_nonblocking(true)
            .map_err(|_| AuthError::Configuration)?;
        let address = listener
            .local_addr()
            .map_err(|_| AuthError::Configuration)?;
        let callback = format!("http://{address}{CALLBACK_PATH}");
        let random = || {
            let mut bytes = Zeroizing::new([0u8; 32]);
            SystemRandom::new()
                .fill(bytes.as_mut())
                .map_err(|_| AuthError::Configuration)?;
            Ok::<_, AuthError>(Zeroizing::new(URL_SAFE_NO_PAD.encode(bytes.as_ref())))
        };
        let state = random()?;
        let verifier = random()?;
        let challenge = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()).as_ref());
        let mut authorization = Url::parse(endpoint).map_err(|_| AuthError::Configuration)?;
        authorization.query_pairs_mut().extend_pairs([
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", &callback),
            ("scope", &crate::oauth::REQUIRED_SCOPES.join(" ")),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", &state),
        ]);
        Ok(Arc::new(Self {
            authorization: Zeroizing::new(authorization.into()),
            callback,
            state,
            verifier,
            listener: Mutex::new(Some(listener)),
            address,
            mode,
            created: Instant::now(),
            cancelled: AtomicBool::new(false),
            consumed: AtomicBool::new(false),
            #[cfg(test)]
            fixture_origin: None,
        }))
    }

    #[cfg(test)]
    pub(crate) fn for_test(endpoint: &str, mode: BrowserMode) -> Result<Arc<Self>, AuthError> {
        let origin = Url::parse(endpoint).map_err(|_| AuthError::Configuration)?;
        if origin.scheme() != "http"
            || origin.host_str() != Some("127.0.0.1")
            || origin.port().is_none()
            || origin.username() != ""
            || origin.password().is_some()
        {
            return Err(AuthError::Configuration);
        }
        let mut attempt = Self::prepare("synthetic-karin-client", 0, mode, endpoint)?;
        Arc::get_mut(&mut attempt)
            .ok_or(AuthError::Configuration)?
            .fixture_origin = Some(origin.origin().ascii_serialization());
        Ok(attempt)
    }

    pub fn browser_mode(&self) -> BrowserMode {
        self.mode
    }
    pub fn authorization_url(&self) -> Zeroizing<String> {
        self.authorization.clone()
    }
    pub fn callback_uri(&self) -> &str {
        &self.callback
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.close_listener();
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn time_remaining(&self) -> Duration {
        LIFETIME.saturating_sub(self.created.elapsed())
    }
    pub fn expired(&self) -> bool {
        self.time_remaining().is_zero()
    }
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    fn close_listener(&self) {
        // A cancelled/consumed attempt may still be owned by a worker or UI
        // callback. Release its registered port independently of those owners.
        self.listener
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    fn check_live(&self) -> Result<(), AuthError> {
        if self.is_cancelled() {
            Err(AuthError::Cancelled)
        } else if self.created.elapsed() >= LIFETIME {
            self.close_listener();
            Err(AuthError::Expired)
        } else if self.consumed.load(Ordering::Acquire) {
            Err(AuthError::InvalidCallback)
        } else {
            Ok(())
        }
    }

    /// Inspect the original bytes before URL normalization. No substring-origin
    /// matches, alternate ports, credentials, path aliases or duplicate state.
    pub fn navigation(&self, raw: &str) -> NavigationDecision {
        if let Err(error) = self.check_live() {
            return NavigationDecision::Reject(error);
        }
        if !valid_url_bytes(raw) {
            return NavigationDecision::Reject(AuthError::UntrustedNavigation);
        }
        let Ok(url) = Url::parse(raw) else {
            return NavigationDecision::Reject(AuthError::UntrustedNavigation);
        };
        let callback_origin = self
            .callback
            .rsplit_once(CALLBACK_PATH)
            .map(|(origin, _)| origin)
            .unwrap_or_default();
        if url.origin().ascii_serialization() == callback_origin {
            return match self.validate_callback(raw) {
                Ok(code) => NavigationDecision::Callback(code),
                Err(error) => NavigationDecision::Reject(error),
            };
        }
        if self.permitted_origin(&url).is_some() {
            NavigationDecision::AllowTrusted
        } else {
            NavigationDecision::Reject(AuthError::UntrustedNavigation)
        }
    }

    /// Sanitized active-document origin, without parsing or consuming a callback.
    pub fn display_origin(&self, raw: &str) -> Option<String> {
        if !valid_url_bytes(raw)
            || self.is_cancelled()
            || self.expired()
            || self.consumed.load(Ordering::Acquire)
        {
            return None;
        }
        self.permitted_origin(&Url::parse(raw).ok()?)
    }

    fn permitted_origin(&self, url: &Url) -> Option<String> {
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return None;
        }
        let origin = url.origin().ascii_serialization();
        #[cfg(test)]
        if self.fixture_origin.as_deref() == Some(origin.as_str()) {
            return Some(origin);
        }
        if url.scheme() == "https"
            && url.port().is_none_or(|p| p == 443)
            && matches!(
                url.host_str(),
                Some("oauth.yandex.ru" | "passport.yandex.ru" | "id.yandex.ru")
            )
        {
            Some(origin)
        } else {
            None
        }
    }

    fn validate_callback(&self, raw: &str) -> Result<AuthorizationCode, AuthError> {
        self.check_live()?;
        if !valid_url_bytes(raw) {
            return Err(AuthError::InvalidCallback);
        }
        let query = raw
            .strip_prefix(&self.callback)
            .and_then(|tail| tail.strip_prefix('?'))
            .filter(|q| !q.is_empty())
            .ok_or(AuthError::InvalidCallback)?;
        let mut state = None;
        let mut code = None;
        let mut error = None;
        let mut optional = [false; 3];
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').ok_or(AuthError::InvalidCallback)?;
            let key = decode(key)?;
            let value = decode(value)?;
            match key.as_str() {
                "state" if state.is_none() => state = Some(value),
                "code" if code.is_none() => code = Some(value),
                "error" if error.is_none() => error = Some(value),
                "cid" | "error_description" | "error_uri" => {
                    let index = match key.as_str() {
                        "cid" => 0,
                        "error_description" => 1,
                        _ => 2,
                    };
                    if optional[index] {
                        return Err(AuthError::InvalidCallback);
                    }
                    optional[index] = true;
                }
                _ => return Err(AuthError::InvalidCallback),
            }
        }
        let state = state.ok_or(AuthError::StateMismatch)?;
        if state.len() != self.state.len()
            || !bool::from(state.as_bytes().ct_eq(self.state.as_bytes()))
        {
            return Err(AuthError::StateMismatch);
        }
        if let Some(error) = error {
            if code.is_some() || error.is_empty() {
                return Err(AuthError::InvalidCallback);
            }
            self.consumed
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| AuthError::InvalidCallback)?;
            self.close_listener();
            return Err(if error.as_str() == "access_denied" {
                AuthError::Denied
            } else {
                AuthError::Rejected
            });
        }
        if optional[1] || optional[2] {
            return Err(AuthError::InvalidCallback);
        }
        let code = code
            .filter(|s| !s.is_empty() && s.len() <= 2048)
            .ok_or(AuthError::InvalidCallback)?;
        if self
            .consumed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(AuthError::InvalidCallback);
        }
        self.close_listener();
        if self.is_cancelled() {
            return Err(AuthError::Cancelled);
        }
        Ok(AuthorizationCode(code))
    }

    /// Automatic browser return. The listener is numeric IPv4 loopback only,
    /// bounded, cancellable and sends no credential-bearing redirect or body.
    pub fn receive_callback(&self) -> Result<AuthorizationCode, AuthError> {
        loop {
            self.check_live()?;
            let accepted = {
                // Only nonblocking accept runs under the lock. Never retain a
                // listener/FD clone or hold this guard while reading or waiting.
                let listener = self.listener.lock().map_err(|_| AuthError::Configuration)?;
                let Some(listener) = listener.as_ref() else {
                    return Err(if self.is_cancelled() {
                        AuthError::Cancelled
                    } else {
                        AuthError::InvalidCallback
                    });
                };
                listener.accept()
            };
            match accepted {
                Ok((mut stream, peer)) => {
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                    let result = self.read_return(&mut stream);
                    let status = if result.is_ok() {
                        "200 OK"
                    } else {
                        "400 Bad Request"
                    };
                    let body = "Return to Karin. You may close this browser tab.";
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                    match result {
                        Ok(code) => return Ok(code),
                        Err(AuthError::InvalidResponse) => continue,
                        Err(error) => {
                            self.close_listener();
                            return Err(error);
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(25))
                }
                Err(_) => return Err(AuthError::Network),
            }
        }
    }

    fn read_return(&self, stream: &mut TcpStream) -> Result<AuthorizationCode, AuthError> {
        stream
            .set_nonblocking(true)
            .map_err(|_| AuthError::Network)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut bytes = Zeroizing::new(Vec::new());
        while !bytes.ends_with(b"\r\n\r\n") {
            self.check_live()?;
            if Instant::now() >= deadline {
                return Err(AuthError::InvalidResponse);
            }
            let mut byte = [0];
            match stream.read(&mut byte) {
                Ok(1) => {
                    bytes.push(byte[0]);
                    if bytes.len() >= MAX_REQUEST {
                        return Err(AuthError::InvalidResponse);
                    }
                }
                Ok(_) => return Err(AuthError::InvalidResponse),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(_) => return Err(AuthError::InvalidResponse),
            }
        }
        let headers = std::str::from_utf8(&bytes).map_err(|_| AuthError::InvalidResponse)?;
        let mut lines = headers.split("\r\n");
        let mut request = lines.next().ok_or(AuthError::InvalidResponse)?.split(' ');
        let (Some("GET"), Some(target), Some("HTTP/1.1"), None) = (
            request.next(),
            request.next(),
            request.next(),
            request.next(),
        ) else {
            return Err(AuthError::InvalidResponse);
        };
        let address = self.address.to_string();
        let mut host = None;
        for line in lines.take_while(|line| !line.is_empty()) {
            let (key, value) = line.split_once(':').ok_or(AuthError::InvalidResponse)?;
            if key.eq_ignore_ascii_case("host") {
                if host.is_some() {
                    return Err(AuthError::InvalidResponse);
                }
                host = Some(value.trim());
            }
            if key.eq_ignore_ascii_case("content-length") && value.trim() != "0"
                || key.eq_ignore_ascii_case("transfer-encoding")
            {
                return Err(AuthError::InvalidResponse);
            }
        }
        if host != Some(address.as_str()) || !target.starts_with(&format!("{CALLBACK_PATH}?")) {
            return Err(AuthError::InvalidResponse);
        }
        let returned = Zeroizing::new(format!("http://{address}{target}"));
        self.validate_callback(&returned)
    }
}

fn valid_url_bytes(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_RETURN
        && !value.contains(['\\', '#'])
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn decode(value: &str) -> Result<Zeroizing<String>, AuthError> {
    let mut decoded = Zeroizing::new(Vec::with_capacity(value.len()));
    let mut bytes = value.bytes();
    let hex = |b| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    };
    while let Some(byte) = bytes.next() {
        decoded.push(match byte {
            b'+' => b' ',
            b'%' => {
                let a = bytes
                    .next()
                    .and_then(hex)
                    .ok_or(AuthError::InvalidCallback)?;
                let b = bytes
                    .next()
                    .and_then(hex)
                    .ok_or(AuthError::InvalidCallback)?;
                a * 16 + b
            }
            b => b,
        });
    }
    let value = std::str::from_utf8(&decoded).map_err(|_| AuthError::InvalidCallback)?;
    if value.chars().any(char::is_control) {
        return Err(AuthError::InvalidCallback);
    }
    Ok(Zeroizing::new(value.to_owned()))
}

#[cfg(test)]
mod tests;
