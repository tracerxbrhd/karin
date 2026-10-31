// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded parsing of untrusted legacy metadata. Only the resolver may consume
//! these private components; the diagnostic exposes a finite classification.

use roxmltree::{Document, Node, ParsingOptions};
use zeroize::Zeroizing;

use super::{DeliveryTransport, declared_transport};

const MAX_XML_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MetadataCategory {
    LegacySignatureRequired,
    UnsupportedResponse,
    ProtectionRequired,
    PreviewOnly,
}

pub(super) struct LegacyParts {
    pub(super) host: Zeroizing<String>,
    pub(super) path: Zeroizing<String>,
    pub(super) timestamp: Zeroizing<String>,
    pub(super) server_input: Zeroizing<String>,
}

impl std::fmt::Debug for LegacyParts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LegacyParts([redacted])")
    }
}

pub(super) fn classify(body: &[u8]) -> MetadataCategory {
    match parse(body) {
        Ok(_) => MetadataCategory::LegacySignatureRequired,
        Err(category) => category,
    }
}

pub(super) fn parse(body: &[u8]) -> Result<LegacyParts, MetadataCategory> {
    use MetadataCategory::{ProtectionRequired, UnsupportedResponse};

    if body.is_empty() || body.len() > MAX_XML_BYTES {
        return Err(UnsupportedResponse);
    }
    let Ok(text) = std::str::from_utf8(body) else {
        return Err(UnsupportedResponse);
    };
    // roxmltree intentionally accepts an *empty* DTD with allow_dtd=false.
    // This diagnostic does not need any DTD, even an empty one.
    if text.contains("<!DOCTYPE") {
        return Err(UnsupportedResponse);
    }
    let Ok(document) = Document::parse_with_options(
        text,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: 64,
            entity_resolver: None,
        },
    ) else {
        return Err(UnsupportedResponse);
    };
    let root = document.root_element();
    if root.tag_name().name() != "download-info" || root.tag_name().namespace().is_some() {
        return Err(UnsupportedResponse);
    }
    if root.descendants().any(protection_hint) {
        return Err(ProtectionRequired);
    }
    if let Some(category) = transport_category(root) {
        return Err(category);
    }
    if root
        .children()
        .filter(|node| node.is_element() && node.tag_name().name().eq_ignore_ascii_case("preview"))
        .any(|node| {
            node.text().is_some_and(|value| {
                !["false", "0", "none", "null", ""]
                    .iter()
                    .any(|negative| value.trim().eq_ignore_ascii_case(negative))
            })
        })
    {
        return Err(MetadataCategory::PreviewOnly);
    }
    if root.attributes().len() != 0
        || root.children().any(|node| {
            node.is_element()
                && (node.tag_name().namespace().is_some()
                    || node.attributes().len() != 0
                    || node.children().any(|child| child.is_element()))
        })
    {
        return Err(UnsupportedResponse);
    }

    let host = unique_text(root, "host", 253);
    let path = unique_text(root, "path", 8192);
    let timestamp = unique_text(root, "ts", 32);
    let server_input = unique_text(root, "s", 512);
    if !host.is_some_and(valid_host)
        || !path.is_some_and(|value| {
            value.starts_with('/') && !value.starts_with("//") && !value.contains(['\\', '?', '#'])
        })
        || !timestamp.is_some_and(|value| value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        || server_input.is_none()
    {
        return Err(UnsupportedResponse);
    }
    // `ts` is an opaque legacy path component, not a Unix grant/expiry time.
    // Published examples contain hexadecimal digits; freshness is measured by
    // the caller from the new offer request rather than inferred from `ts`.
    Ok(LegacyParts {
        host: Zeroizing::new(host.unwrap().to_owned()),
        path: Zeroizing::new(path.unwrap().to_owned()),
        timestamp: Zeroizing::new(timestamp.unwrap().to_owned()),
        server_input: Zeroizing::new(server_input.unwrap().to_owned()),
    })
}

fn unique_text<'a>(root: Node<'a, 'a>, name: &str, maximum: usize) -> Option<&'a str> {
    let mut matching = root
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == name);
    let node = matching.next()?;
    if matching.next().is_some() {
        return None;
    }
    let value = node.text()?.trim();
    (!value.is_empty()
        && value.len() <= maximum
        && !value.chars().any(char::is_control)
        && node.children().filter(|child| child.is_text()).count() == 1)
        .then_some(value)
}

fn valid_host(host: &str) -> bool {
    host.is_ascii()
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn protection_hint(node: Node<'_, '_>) -> bool {
    if !node.is_element() {
        return false;
    }
    let named = |name: &str| {
        [
            "key",
            "drm",
            "encrypted",
            "encryption",
            "decryptionKey",
            "decryption_key",
            "licenseUrl",
            "license_url",
            "protected",
            "protection",
        ]
        .iter()
        .any(|known| name.eq_ignore_ascii_case(known))
    };
    let positive = |value: &str| {
        let value = value.trim();
        !value.is_empty()
            && !["false", "0", "none", "null"]
                .iter()
                .any(|negative| value.eq_ignore_ascii_case(negative))
    };
    (named(node.tag_name().name())
        && (node.text().is_some_and(positive) || node.children().any(|child| child.is_element())))
        || node
            .attributes()
            .any(|attribute| named(attribute.name()) && positive(attribute.value()))
}

fn transport_category(root: Node<'_, '_>) -> Option<MetadataCategory> {
    let mut unsupported = false;
    for node in root.descendants().filter(Node::is_element) {
        if node.tag_name().name().eq_ignore_ascii_case("transport") {
            match node
                .text()
                .map(declared_transport)
                .unwrap_or(DeliveryTransport::Unsupported)
            {
                DeliveryTransport::Protected => return Some(MetadataCategory::ProtectionRequired),
                DeliveryTransport::Unsupported => unsupported = true,
                DeliveryTransport::Absent | DeliveryTransport::Raw => {}
            }
        }
        for attribute in node
            .attributes()
            .filter(|attribute| attribute.name().eq_ignore_ascii_case("transport"))
        {
            match declared_transport(attribute.value()) {
                DeliveryTransport::Protected => return Some(MetadataCategory::ProtectionRequired),
                DeliveryTransport::Unsupported => unsupported = true,
                DeliveryTransport::Absent | DeliveryTransport::Raw => {}
            }
        }
    }
    unsupported.then_some(MetadataCategory::UnsupportedResponse)
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = "<download-info><host>media.example.test</host><path>/synthetic-resource</path><ts>1700000000</ts><region>test</region><s>synthetic-input</s></download-info>";

    #[test]
    fn legacy_components_are_not_a_direct_playback_resource() {
        assert_eq!(
            classify(XML.as_bytes()),
            MetadataCategory::LegacySignatureRequired
        );
        let reordered = "<?xml version=\"1.0\"?><download-info><s>synthetic-input</s><ts>1700000000</ts><path>/fixture</path><host>media.example.test</host></download-info>";
        assert_eq!(
            classify(reordered.as_bytes()),
            MetadataCategory::LegacySignatureRequired
        );
    }

    #[test]
    fn malformed_missing_duplicate_and_ambiguous_fields_are_unsupported() {
        for input in [
            String::new(),
            "not xml".into(),
            "<download-info><host>media.example.test</host></download-info>".into(),
            XML.replace("<host>", "<host><nested/>"),
            XML.replace("<s>synthetic-input</s>", "<s/><s>synthetic-input</s>"),
            XML.replace("<ts>1700000000</ts>", "<ts>timestamp</ts>"),
            XML.replace("/synthetic-resource", "//other.example.test/path"),
            XML.replace("media.example.test", "user@media.example.test"),
            XML.replace("<download-info>", "<download-info xmlns=\"urn:unknown\">"),
            XML.replace("<host>", "<host extra=\"synthetic\">"),
        ] {
            assert_eq!(
                classify(input.as_bytes()),
                MetadataCategory::UnsupportedResponse
            );
        }
        assert_eq!(
            classify(&[0xff, 0xfe]),
            MetadataCategory::UnsupportedResponse
        );
    }

    #[test]
    fn dtd_entities_and_resource_exhaustion_are_rejected() {
        for declaration in [
            "<!DOCTYPE download-info>",
            "<!DOCTYPE download-info [<!ENTITY x SYSTEM \"file:///synthetic-private\">]>",
            "<!DOCTYPE download-info [<!ENTITY x \"synthetic-sensitive\">]>",
        ] {
            assert_eq!(
                classify(format!("{declaration}{XML}").as_bytes()),
                MetadataCategory::UnsupportedResponse
            );
        }
        assert_eq!(
            classify(&vec![b'x'; MAX_XML_BYTES + 1]),
            MetadataCategory::UnsupportedResponse
        );
        let nodes = format!("<download-info>{}</download-info>", "<extra/>".repeat(65));
        assert_eq!(
            classify(nodes.as_bytes()),
            MetadataCategory::UnsupportedResponse
        );
    }

    #[test]
    fn explicit_protection_is_classified_without_using_its_material() {
        for field in [
            "<key>synthetic-key-never-used</key>",
            "<drm>true</drm>",
            "<encrypted>1</encrypted>",
            "<protected>true</protected>",
            "<decryptionKey>synthetic-key-never-used</decryptionKey>",
            "<decryption_key>synthetic-key-never-used</decryption_key>",
            "<licenseUrl>synthetic-license-never-used</licenseUrl>",
            "<license_url>synthetic-license-never-used</license_url>",
            "<protection><kind>synthetic</kind></protection>",
            "<extra encryption=\"synthetic-encryption\"/>",
            "<extra protected=\"true\"/>",
            "<extra decryption_key=\"synthetic-key-never-used\"/>",
            "<extra license_url=\"synthetic-license-never-used\"/>",
        ] {
            let input = XML.replace("</download-info>", &format!("{field}</download-info>"));
            assert_eq!(
                classify(input.as_bytes()),
                MetadataCategory::ProtectionRequired
            );
        }
        let negative = XML.replace(
            "</download-info>",
            "<encrypted>false</encrypted><key/></download-info>",
        );
        assert_eq!(
            classify(negative.as_bytes()),
            MetadataCategory::LegacySignatureRequired
        );
    }

    #[test]
    fn declared_transport_cannot_promote_protected_or_unknown_delivery_to_legacy() {
        for (field, expected) in [
            (
                "<transport>encraw</transport>",
                MetadataCategory::ProtectionRequired,
            ),
            (
                "<extra transport=\"encraw\"/>",
                MetadataCategory::ProtectionRequired,
            ),
            (
                "<transport>raw</transport>",
                MetadataCategory::LegacySignatureRequired,
            ),
            (
                "<transport>plain</transport>",
                MetadataCategory::UnsupportedResponse,
            ),
            ("<transport/>", MetadataCategory::UnsupportedResponse),
            (
                "<transport>synthetic-private-transport</transport>",
                MetadataCategory::UnsupportedResponse,
            ),
            (
                "<extra transport=\"synthetic-private-transport\"/>",
                MetadataCategory::UnsupportedResponse,
            ),
        ] {
            let body = XML.replace("</download-info>", &format!("{field}</download-info>"));
            assert_eq!(classify(body.as_bytes()), expected);
            assert!(!format!("{expected:?}").contains("synthetic-private-transport"));
        }
        for protection in [
            "<key>synthetic-private-key</key>",
            "<protected>true</protected>",
            "<transport>encraw</transport>",
        ] {
            let body = XML.replace(
                "</download-info>",
                &format!("<transport>raw</transport>{protection}</download-info>"),
            );
            assert_eq!(
                classify(body.as_bytes()),
                MetadataCategory::ProtectionRequired
            );
        }
    }

    #[test]
    fn diagnostic_contains_no_descriptor_or_private_values() {
        let synthetic = XML
            .replace("synthetic-resource", "private-track-marker")
            .replace("synthetic-input", "private-server-material");
        let output = format!("{:?}", classify(synthetic.as_bytes()));
        for marker in [
            "private-track-marker",
            "private-server-material",
            "media.example.test",
            "1700000000",
        ] {
            assert!(!output.contains(marker));
        }
        assert_eq!(output, "LegacySignatureRequired");
    }
}
