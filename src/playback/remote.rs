// SPDX-License-Identifier: GPL-3.0-or-later
//! Fail-closed native transport policy for an already admitted remote source.

use super::*;

pub(super) fn guard_pipeline(
    pipeline: &gst::Element,
    admission: RemotePlaybackSource,
) -> Result<Arc<AtomicBool>, PlaybackError> {
    let refused = Arc::new(AtomicBool::new(false));
    let admission = Arc::new(admission);
    if gst::glib::subclass::SignalId::lookup("source-setup", pipeline.type_()).is_none() {
        return Err(PlaybackError::SourcePolicy);
    }
    let source_refused = refused.clone();
    let source_admission = admission.clone();
    pipeline.connect("source-setup", false, move |values| {
        if let Some(source) = values
            .get(1)
            .and_then(|value| value.get::<gst::Element>().ok())
        {
            if !configure_source(&source, &source_admission) {
                reject(&source, &source_refused);
            }
        } else {
            source_refused.store(true, Ordering::Release);
        }
        None
    });
    let bin = pipeline
        .dynamic_cast_ref::<gst::Bin>()
        .ok_or(PlaybackError::SourcePolicy)?;
    let child_refused = refused.clone();
    bin.connect_deep_element_added(move |_, _, child| {
        let Some(factory) = child.factory() else {
            return;
        };
        let name = factory.name();
        if name == "queue2" {
            // Never turn remote streaming into a download cache. The playbin
            // flags exclude DOWNLOAD; also clear queue2's tempfile template
            // and retain only a small bounded in-memory buffer.
            for (property, expected) in [
                ("temp-template", String::static_type()),
                ("max-size-bytes", u32::static_type()),
                ("max-size-buffers", u32::static_type()),
                ("max-size-time", u64::static_type()),
            ] {
                if child
                    .find_property(property)
                    .is_none_or(|property| property.value_type() != expected)
                {
                    reject(child, &child_refused);
                    return;
                }
            }
            child.set_property("temp-template", Option::<&str>::None);
            child.set_property("max-size-bytes", 2_u32 * 1024 * 1024);
            child.set_property("max-size-buffers", 64_u32);
            child.set_property("max-size-time", 2_000_000_000_u64);
        }
        // A resource offered as MP3 may not turn into a manifest or create a
        // second protocol handler that escapes the admitted origin/URI.
        if factory
            .metadata("klass")
            .is_some_and(|class| class.contains("Adaptive"))
            || [
                "hlsdemux",
                "dashdemux",
                "mssdemux",
                "playlist",
                "downloadbuffer",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix))
            || child
                .dynamic_cast_ref::<gst::URIHandler>()
                .is_some_and(|handler| {
                    handler.uri_type() == gst::URIType::Src && name != "souphttpsrc"
                })
        {
            reject(child, &child_refused);
            return;
        }
        if matches!(
            name.as_str(),
            "uridecodebin" | "decodebin" | "uridecodebin3" | "decodebin3"
        ) {
            if gst::glib::subclass::SignalId::lookup("autoplug-continue", child.type_()).is_none() {
                reject(child, &child_refused);
                return;
            }
            let caps_refused = child_refused.clone();
            child.connect("autoplug-continue", false, move |values| {
                let allowed = values
                    .get(2)
                    .and_then(|value| value.get::<gst::Caps>().ok())
                    .is_some_and(|caps| permitted_caps(&caps));
                if !allowed {
                    caps_refused.store(true, Ordering::Release);
                }
                Some(allowed.to_value())
            });
        }
        if name == "souphttpsrc" {
            // Notify also covers a later attempt to replace this source's
            // location. The source-setup hook below still checks its initial
            // final URI before the element can leave NULL.
            let location_refused = child_refused.clone();
            let location_admission = admission.clone();
            child.connect_notify(Some("location"), move |source, _| {
                if !configure_source(source, &location_admission) {
                    reject(source, &location_refused);
                }
            });
        }
    });
    Ok(refused)
}

fn reject(source: &gst::Element, refused: &AtomicBool) {
    refused.store(true, Ordering::Release);
    // The native parent may continue its asynchronous state change after a
    // signal returns. Lock an untrusted child in NULL rather than merely
    // waiting for the worker's next poll to notice the refusal.
    source.set_locked_state(true);
    if source.current_state() == gst::State::Null {
        let _ = source.set_state(gst::State::Null);
    } else {
        // A notification may run on a streaming thread. Never try to join that
        // same task there; the worker also observes refusal on its next poll.
        source.call_async(|source| {
            let _ = source.set_state(gst::State::Null);
        });
    }
}

pub(super) fn error_http_status(error: &gst::message::Error) -> Option<u16> {
    let source = error.src()?.dynamic_cast_ref::<gst::Element>()?;
    if !source
        .factory()
        .is_some_and(|factory| factory.name() == "souphttpsrc")
    {
        return None;
    }
    http_error_status(error.details())
}

fn http_error_status(details: Option<&gst::StructureRef>) -> Option<u16> {
    // souphttpsrc publishes this numeric field; all other details (including
    // its redirect URI) remain unread and are dropped with the native message.
    details
        .and_then(|details| details.get::<u32>("http-status-code").ok())
        .filter(|status| (100..=599).contains(status))
        .and_then(|status| u16::try_from(status).ok())
}

fn permitted_caps(caps: &gst::CapsRef) -> bool {
    !caps.is_empty()
        && !caps.is_any()
        && caps.iter().all(|structure| {
            matches!(
                structure.name().as_str(),
                "audio/mpeg" | "audio/x-raw" | "application/x-id3" | "application/x-apetag"
            ) && (!structure.has_field("mpegversion")
                || structure
                    .get::<i32>("mpegversion")
                    .is_ok_and(|version| version == 1))
                && (!structure.has_field("layer")
                    || structure.get::<i32>("layer").is_ok_and(|layer| layer == 3))
        })
}

fn configure_source(source: &gst::Element, admission: &RemotePlaybackSource) -> bool {
    if admission.is_expired()
        || !source
            .factory()
            .is_some_and(|factory| factory.name() == "souphttpsrc")
        || source
            .dynamic_cast_ref::<gst::URIHandler>()
            .and_then(|handler| handler.uri())
            .is_none_or(|uri| uri.as_str() != admission.uri.as_str())
    {
        return false;
    }
    // Check the runtime property types before setting them: native warnings or
    // a Rust property panic must not format a URI/header value into stderr.
    for (name, expected) in [
        ("automatic-redirect", bool::static_type()),
        ("ssl-strict", bool::static_type()),
        ("cookies", gst::glib::StrV::static_type()),
        ("extra-headers", gst::Structure::static_type()),
        ("user-id", String::static_type()),
        ("user-pw", String::static_type()),
        ("user-agent", String::static_type()),
        ("retries", i32::static_type()),
        ("timeout", u32::static_type()),
    ] {
        if source
            .find_property(name)
            .is_none_or(|property| property.value_type() != expected)
        {
            return false;
        }
    }
    source.set_property("automatic-redirect", false);
    source.set_property("ssl-strict", true);
    // A non-NULL empty GStrv disables SoupCookieJar for every request without
    // adding a Cookie header (souphttpsrc's public cookies-property contract).
    // Passing NULL instead would leave the session's automatic jar enabled.
    source.set_property("cookies", gst::glib::StrV::new().to_value());
    source.set_property("extra-headers", gst::Structure::new_empty("headers"));
    source.set_property("user-id", Option::<&str>::None);
    source.set_property("user-pw", Option::<&str>::None);
    source.set_property(
        "user-agent",
        concat!("Karin/", env!("CARGO_PKG_VERSION"), " playback"),
    );
    source.set_property("retries", 0_i32);
    source.set_property("timeout", 4_u32);
    if source
        .find_property("keep-alive")
        .is_some_and(|property| property.value_type() == bool::static_type())
    {
        source.set_property("keep-alive", false);
    }
    if source
        .find_property("iradio-mode")
        .is_some_and(|property| property.value_type() == bool::static_type())
    {
        source.set_property("iradio-mode", false);
    }
    if source
        .find_property("compress")
        .is_some_and(|property| property.value_type() == bool::static_type())
    {
        source.set_property("compress", false);
    }
    #[cfg(test)]
    {
        let loopback = Url::parse(admission.uri.as_str()).ok().is_some_and(|url| {
            matches!(url.host_str(), Some("127.0.0.1" | "[::1]")) && url.port().is_some()
        });
        if loopback {
            // souphttpsrc copies http_proxy into an explicit GSimpleProxyResolver
            // and may therefore ignore NO_PROXY. Only synthetic loopback
            // sources get an explicit direct resolver; production environment
            // proxy settings are neither read nor changed here.
            if source
                .find_property("proxy")
                .is_none_or(|property| property.value_type() != String::static_type())
            {
                return false;
            }
            source.set_property("proxy", "direct://");
        }
        if let Some(ca_file) = &admission.ca_file {
            // ssl-ca-file is a no-op with libsoup3. A fixture-local TLS database
            // trusts only its generated test CA; verification remains enabled.
            if source.find_property("tls-database").is_none_or(|property| {
                property.value_type() != gtk::gio::TlsDatabase::static_type()
            }) {
                return false;
            }
            let Some(path) = ca_file.to_str() else {
                return false;
            };
            let Ok(database) = gtk::gio::TlsFileDatabase::new(path) else {
                return false;
            };
            source.set_property("tls-database", &database);
        }
    }
    !source.property::<bool>("automatic-redirect")
        && source.property::<bool>("ssl-strict")
        && source.property::<gst::glib::StrV>("cookies").is_empty()
        && source
            .property::<Option<gst::Structure>>("extra-headers")
            .is_none_or(|headers| headers.n_fields() == 0)
        && source.property::<Option<String>>("user-id").is_none()
        && source.property::<Option<String>>("user-pw").is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_audio_codec_caps_can_autoplug_not_a_manifest_or_uri_list() {
        gst::init().unwrap();
        for caps in [
            "audio/mpeg,mpegversion=(int)1,layer=(int)3",
            "application/x-id3",
            "audio/x-raw,format=(string)S16LE",
        ] {
            assert!(permitted_caps(&caps.parse::<gst::Caps>().unwrap()));
        }
        for caps in [
            "application/x-hls",
            "application/dash+xml",
            "text/uri-list",
            "audio/mpeg,mpegversion=(int)4",
            "audio/mpeg,layer=(int)2",
            "video/x-raw",
            "ANY",
            "EMPTY",
        ] {
            assert!(!permitted_caps(&caps.parse::<gst::Caps>().unwrap()));
        }
    }

    #[test]
    fn only_documented_numeric_http_details_cross_the_error_boundary() {
        gst::init().unwrap();
        let details = gst::Structure::builder("details")
            .field("http-status-code", 403_u32)
            .field("http-redirect-uri", "synthetic-private-location")
            .field("private-message", "synthetic-provider-text")
            .build();
        assert_eq!(http_error_status(Some(&details)), Some(403));
        assert_eq!(http_error_status(None), None);
        for status in [0_u32, 99, 600, u32::MAX] {
            let details = gst::Structure::builder("details")
                .field("http-status-code", status)
                .build();
            assert_eq!(http_error_status(Some(&details)), None);
        }
        let text = gst::Structure::builder("details")
            .field("http-status-code", "403")
            .build();
        assert_eq!(http_error_status(Some(&text)), None);
    }
}
