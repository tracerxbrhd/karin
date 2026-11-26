// SPDX-License-Identifier: GPL-3.0-or-later
//! Plain, bounded excerpts of GitHub's common release-note constructs.

/// Keep the visible copy and list structure. Never pass remote markup or link
/// destinations to GTK, even when a note was cut off at the response limit.
pub(super) fn excerpt(notes: &str) -> String {
    let mut text = remove_comments(notes);
    text.retain(|ch| {
        (matches!(ch, '\n' | '\t') || !ch.is_control())
            && !matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    });
    let mut output = Vec::new();
    let mut blank = false;
    for raw in text.lines() {
        let line = raw.trim();
        let line = line.trim_start_matches('>').trim_start();
        if line.starts_with("```")
            || line.starts_with("~~~")
            || line.len() >= 3
                && b"-=*_"
                    .iter()
                    .any(|rule| line.bytes().all(|ch| ch == *rule))
            || is_reference(line)
        {
            continue;
        }
        if line.is_empty() {
            blank = true;
            continue;
        }
        let rendered = if let Some(item) = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .or_else(|| line.strip_prefix("+ "))
        {
            format!("• {}", inline(item))
        } else {
            let title = line.trim_start_matches('#');
            let title = if title.starts_with(' ') {
                title.trim().trim_end_matches('#').trim_end()
            } else {
                line
            };
            inline(title)
        };
        let rendered = rendered.trim();
        if rendered.is_empty() || rendered == "•" || rendered.starts_with("Full Changelog:") {
            continue;
        }
        if blank && !output.is_empty() {
            output.push(String::new());
        }
        blank = false;
        output.push(rendered.to_owned());
    }
    output.join("\n")
}

fn remove_comments(mut notes: &str) -> String {
    let mut text = String::new();
    while let Some((visible, tail)) = notes.split_once("<!--") {
        text.push_str(visible);
        let Some((_, remainder)) = tail.split_once("-->") else {
            return text;
        };
        notes = remainder;
    }
    text.push_str(notes);
    text
}

fn is_reference(line: &str) -> bool {
    line.strip_prefix('[')
        .and_then(|line| line.split_once("]:"))
        .is_some_and(|(_, value)| !value.trim().is_empty())
}

fn inline(mut rest: &str) -> String {
    let mut text = String::new();
    while let Some(ch) = rest.chars().next() {
        let image = rest.starts_with("![");
        if (image || ch == '[')
            && let Some((label, after)) = rest[if image { 2 } else { 1 }..].split_once(']')
        {
            let destination = after.strip_prefix('(').map(after_destination).or_else(|| {
                after
                    .strip_prefix('[')
                    .map(|tail| tail.split_once(']').map_or("", |(_, tail)| tail))
            });
            if let Some(tail) = destination {
                if !image {
                    text.push_str(&label.replace(['*', '`'], ""));
                }
                rest = tail;
                continue;
            }
        }
        if ch == '<'
            && let Some((_, tail)) = rest.split_once('>')
        {
            rest = tail;
            continue;
        }
        if rest.starts_with("__") || rest.starts_with("~~") {
            rest = &rest[2..];
            continue;
        }
        if ch == '\\'
            && let Some(escaped) = rest[1..].chars().next().filter(char::is_ascii_punctuation)
        {
            text.push(escaped);
            rest = &rest[1 + escaped.len_utf8()..];
            continue;
        }
        let tail = &rest[ch.len_utf8()..];
        let emphasis = ch == '_'
            && !(text.chars().next_back().is_some_and(char::is_alphanumeric)
                && tail.chars().next().is_some_and(char::is_alphanumeric));
        if !emphasis && !matches!(ch, '*' | '`') {
            text.push(ch);
        }
        rest = tail;
    }
    text
}

fn after_destination(destination: &str) -> &str {
    let mut depth = 0;
    let mut escaped = false;
    for (index, ch) in destination.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '(' => depth += 1,
            ')' if depth == 0 => return &destination[index + 1..],
            ')' => depth -= 1,
            _ => {}
        }
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_keep_headings_lists_and_visible_link_text() {
        assert_eq!(
            excerpt(
                "## Новое\n\n- **Поиск** и _ползунок_, `run-karin`. [Инструкция](https://example.test/one(two))\n- Быстрее.\n\n<!-- karin-flatpak-update-sha256: digest -->\n[ref]: https://example.test/path\n\n<!-- internal\nmultiline -->\n![Badge](https://example.test/icon)\n"
            ),
            "Новое\n\n• Поиск и ползунок, run-karin. Инструкция\n• Быстрее.",
        );
    }

    #[test]
    fn notes_drop_cut_off_metadata_destinations_and_control_characters() {
        assert_eq!(
            excerpt(
                "Сборка <b>готова</b>\u{202e}. [Подробнее](https://example.test/long\n<!-- digest cropped"
            ),
            "Сборка готова. Подробнее",
        );
    }
}
