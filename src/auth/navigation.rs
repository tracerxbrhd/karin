// SPDX-License-Identifier: GPL-3.0-or-later
//! Small privacy boundary for a single rejected browser event. This value must
//! never retain an address or provider-supplied string, even in Debug output.

use std::fmt;

use reqwest::Url;

use super::{sensitive_oauth_fragment, valid_page_url_bytes};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationEvent {
    PolicyNavigation,
    PolicyNewWindow,
    PolicyResponse,
    MainUri,
    LoadFailed,
    Tls,
    WebProcess,
}

/// Supplied from confirmed WebKit context, never inferred from frame_name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameContext {
    Main,
    Subframe,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectionReason {
    InvalidAddress,
    CredentialsInAddress,
    UnsupportedScheme,
    UntrustedOrigin,
    OAuthResultOutsideCallback,
    InvalidCallback,
    UnconfirmedMainDocument,
    ServiceDocument,
    AttemptInactive,
    LoadFailure,
    CancelledLoad,
    TlsFailure,
    ProcessTerminated,
    MissingAddress,
    Download,
    Popup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SafeAddress {
    Missing,
    Invalid,
    Sensitive,
    Unrecognized,
    ServiceDocument,
    KnownOrigin {
        scheme: &'static str,
        host: &'static str,
        port: u16,
        fragment: bool,
    },
}

/// Contains only finite categories and a canonical public origin. Unknown hosts
/// can contain account/QR identifiers and are deliberately not echoed. Paths,
/// query/fragment contents and userinfo are never copied into this value.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SafeNavigationDiagnostic {
    event: NavigationEvent,
    reason: RejectionReason,
    frame: FrameContext,
    address: SafeAddress,
}

impl SafeNavigationDiagnostic {
    pub fn new(
        event: NavigationEvent,
        reason: RejectionReason,
        frame: FrameContext,
        raw: Option<&str>,
    ) -> Self {
        Self {
            event,
            reason,
            frame,
            address: sanitize(raw),
        }
    }

    pub fn event(self) -> NavigationEvent {
        self.event
    }
    pub fn reason(self) -> RejectionReason {
        self.reason
    }
    pub fn frame(self) -> FrameContext {
        self.frame
    }
}

fn sanitize(raw: Option<&str>) -> SafeAddress {
    let Some(raw) = raw else {
        return SafeAddress::Missing;
    };
    if !valid_page_url_bytes(raw) {
        return SafeAddress::Invalid;
    }
    if matches!(raw, "about:blank" | "about:srcdoc") {
        return SafeAddress::ServiceDocument;
    }
    let Ok(url) = Url::parse(raw) else {
        return SafeAddress::Invalid;
    };
    if !url.username().is_empty() || url.password().is_some() || sensitive_oauth_fragment(&url) {
        return SafeAddress::Sensitive;
    }
    // An OAuth result can also be delivered in a query. Even its safe origin is
    // omitted for this category; parameter values are not decoded or retained.
    if url.query().is_some_and(|query| {
        query.split('&').any(|pair| {
            let key = pair.split_once('=').map_or(pair, |(key, _)| key);
            super::decode(key).is_ok_and(|key| {
                matches!(
                    key.to_ascii_lowercase().as_str(),
                    "access_token" | "refresh_token" | "id_token" | "code" | "state" | "error"
                )
            })
        })
    }) {
        return SafeAddress::Sensitive;
    }
    let scheme = match url.scheme() {
        "https" => "https",
        "http" => "http",
        _ => return SafeAddress::Unrecognized,
    };
    let host = match url.host_str() {
        Some("oauth.yandex.ru") => "oauth.yandex.ru",
        Some("passport.yandex.ru") => "passport.yandex.ru",
        Some("id.yandex.ru") => "id.yandex.ru",
        Some("music.yandex.ru") => "music.yandex.ru",
        Some("smartcaptcha.yandexcloud.net") => "smartcaptcha.yandexcloud.net",
        Some("captcha-api.yandex.ru") => "captcha-api.yandex.ru",
        #[cfg(test)]
        Some("127.0.0.1") => "127.0.0.1",
        _ => return SafeAddress::Unrecognized,
    };
    let Some(port) = url.port_or_known_default() else {
        return SafeAddress::Invalid;
    };
    SafeAddress::KnownOrigin {
        scheme,
        host,
        port,
        fragment: url.fragment().is_some(),
    }
}

impl fmt::Display for SafeNavigationDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "event={:?} reason={:?} frame={:?}",
            self.event, self.reason, self.frame
        )?;
        match self.address {
            SafeAddress::ServiceDocument => write!(
                f,
                " address=ServiceDocument scheme=about host=none port=none fragment=false"
            ),
            SafeAddress::KnownOrigin {
                scheme,
                host,
                port,
                fragment,
            } => write!(
                f,
                " address=KnownOrigin scheme={scheme} host={host} port={port} fragment={fragment}"
            ),
            category => write!(f, " address={category:?}"),
        }
    }
}

impl fmt::Debug for SafeNavigationDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(raw: Option<&str>) -> String {
        let diagnostic = SafeNavigationDiagnostic::new(
            NavigationEvent::PolicyNavigation,
            RejectionReason::UntrustedOrigin,
            FrameContext::Unknown,
            raw,
        );
        assert_eq!(diagnostic.event(), NavigationEvent::PolicyNavigation);
        assert_eq!(diagnostic.reason(), RejectionReason::UntrustedOrigin);
        assert_eq!(diagnostic.frame(), FrameContext::Unknown);
        format!("{diagnostic} {diagnostic:?}")
    }

    #[test]
    fn diagnostics_retain_only_canonical_known_origin_and_presence_of_plain_fragment() {
        let shown = report(Some(
            "https://PASSPORT.YANDEX.RU:444/synthetic-private-path?phone=synthetic-private-phone#synthetic-private-step",
        ));
        for known in [
            "scheme=https",
            "host=passport.yandex.ru",
            "port=444",
            "fragment=true",
            "frame=Unknown",
        ] {
            assert!(shown.contains(known));
        }
        for secret in [
            "synthetic-private-path",
            "synthetic-private-phone",
            "synthetic-private-step",
            "phone=",
        ] {
            assert!(!shown.contains(secret));
        }
        let main = SafeNavigationDiagnostic::new(
            NavigationEvent::MainUri,
            RejectionReason::InvalidCallback,
            FrameContext::Main,
            Some("https://oauth.yandex.ru/"),
        );
        assert!(format!("{main}").contains("frame=Main"));
        assert!(format!("{main}").contains("port=443 fragment=false"));
    }

    #[test]
    fn malformed_sensitive_unknown_and_embedded_addresses_are_fixed_categories_only() {
        for raw in [
            "https://synthetic-private-phone@passport.yandex.ru/private",
            "https://synthetic-user:synthetic-secret@oauth.yandex.ru/",
            "https://synthetic-private-qr.invalid/private?private=synthetic-private-value",
            "https://synthetic-private-qr.yandex.ru/",
            "https://синтетический-секрет.yandex.ru/private",
            "https://passport.yandex.ru/#access_token=synthetic-private-token&state=synthetic-private-state",
            "https://passport.yandex.ru/?code=synthetic-private-code&state=synthetic-private-state",
            "https://passport.yandex.ru/#%61ccess_token=synthetic-private-token",
            "https://passport.yandex.ru/#state=synthetic-private-state",
            "https://passport.yandex.ru\\@synthetic-private-qr.invalid/",
            "javascript:synthetic-private-input",
            "data:text/html,synthetic-private-phone",
            "https://passport.yandex.ru:synthetic-private-port/",
            " https://passport.yandex.ru/?synthetic-private-value",
        ] {
            let shown = report(Some(raw));
            assert!(!shown.contains("synthetic"));
            for private_component in [
                "scheme=",
                "host=",
                "port=",
                "fragment=",
                "access_token",
                "state=",
                "code=",
            ] {
                assert!(!shown.contains(private_component));
            }
        }
        assert!(report(None).contains("address=Missing"));
        assert!(report(Some("about:blank")).contains("address=ServiceDocument"));
        assert!(
            report(Some("about:blank")).contains("scheme=about host=none port=none fragment=false")
        );
        assert!(!report(Some("about:blank")).contains("about:blank"));
        let huge = format!(
            "https://passport.yandex.ru/?phone={}",
            "synthetic-private-".repeat(8192)
        );
        assert!(report(Some(&huge)).contains("address=Invalid"));
        assert!(!report(Some(&huge)).contains("synthetic-private"));
    }
}
