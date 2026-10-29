// SPDX-License-Identifier: GPL-3.0-or-later
//! Finite classification of an untrusted legacy metadata document. No URI is
//! constructed, no checksum/key is used, and no provider text leaves this module.

use roxmltree::{Document, Node, ParsingOptions};

const MAX_XML_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MetadataCategory {
    LegacySignatureRequired,
    UnsupportedResponse,
    ProtectionRequired,
}

pub(super) fn classify(body: &[u8]) -> MetadataCategory {
    use MetadataCategory::{LegacySignatureRequired, ProtectionRequired, UnsupportedResponse};

    if body.is_empty() || body.len() > MAX_XML_BYTES {
        return UnsupportedResponse;
    }
    let Ok(text) = std::str::from_utf8(body) else {
        return UnsupportedResponse;
    };
    // roxmltree intentionally accepts an *empty* DTD with allow_dtd=false.
    // This diagnostic does not need any DTD, even an empty one.
    if text.contains("<!DOCTYPE") {
        return UnsupportedResponse;
    }
    let Ok(document) = Document::parse_with_options(
        text,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: 64,
            entity_resolver: None,
        },
    ) else {
        return UnsupportedResponse;
    };
    let root = document.root_element();
    if root.tag_name().name() != "download-info" || root.tag_name().namespace().is_some() {
        return UnsupportedResponse;
    }
    if root.descendants().any(protection_hint) {
        return ProtectionRequired;
    }
    if root.attributes().len() != 0
        || root.children().any(|node| {
            node.is_element()
                && (node.tag_name().namespace().is_some()
                    || node.attributes().len() != 0
                    || node.children().any(|child| child.is_element()))
        })
    {
        return UnsupportedResponse;
    }

    let host = unique_text(root, "host", 253);
    let path = unique_text(root, "path", 8192);
    let timestamp = unique_text(root, "ts", 20);
    let server_input = unique_text(root, "s", 512);
    if !host.is_some_and(valid_host)
        || !path.is_some_and(|value| {
            value.starts_with('/') && !value.starts_with("//") && !value.contains(['\\', '?', '#'])
        })
        || !timestamp.is_some_and(|value| value.bytes().all(|byte| byte.is_ascii_digit()))
        || server_input.is_none()
    {
        return UnsupportedResponse;
    }
    // Recognize the documented legacy XML shape, not a playable source. The
    // published clients perform another client-side construction step here;
    // this diagnostic intentionally stops before that step.
    LegacySignatureRequired
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
            "licenseUrl",
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
            "<protection><kind>synthetic</kind></protection>",
            "<extra encryption=\"synthetic-encryption\"/>",
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
