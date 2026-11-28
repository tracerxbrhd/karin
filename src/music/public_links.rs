// SPDX-License-Identifier: GPL-3.0-or-later
//! Public Yandex Music pages derived only from catalog identifiers.

const ORIGIN: &str = "https://music.yandex.ru";

fn public_id(id: &str) -> Option<u64> {
    if id.len() > 20 || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    id.parse::<u64>().ok().filter(|id| *id != 0)
}

/// The first-party player routes both `/track/{id}` and an album's track page.
/// Use the album URL when its independently validated identifier is available.
pub fn track(track_id: &str, album_id: Option<&str>) -> Option<String> {
    let track_id = public_id(track_id)?;
    Some(match album_id.and_then(public_id) {
        Some(album_id) => format!("{ORIGIN}/album/{album_id}/track/{track_id}"),
        None => format!("{ORIGIN}/track/{track_id}"),
    })
}

/// Build a link for external presence only when catalog album metadata is
/// present. A collection selector may qualify the track with that same album.
pub(crate) fn catalog_track(track_id: &str, album_id: &str) -> Option<String> {
    let album_id = public_id(album_id)?;
    let track_id = match track_id.split_once(':') {
        Some((track, album)) if public_id(album) == Some(album_id) => track,
        Some(_) => return None,
        None => track_id,
    };
    Some(format!(
        "{ORIGIN}/album/{album_id}/track/{}",
        public_id(track_id)?
    ))
}

pub fn album(album_id: &str) -> Option<String> {
    Some(format!("{ORIGIN}/album/{}", public_id(album_id)?))
}

pub fn artist(artist_id: &str) -> Option<String> {
    Some(format!("{ORIGIN}/artist/{}", public_id(artist_id)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_public_pages_never_admit_a_supplied_url_or_account_path() {
        assert_eq!(
            track("2816", Some("1039")).as_deref(),
            Some("https://music.yandex.ru/album/1039/track/2816")
        );
        assert_eq!(
            track("2816", None).as_deref(),
            Some("https://music.yandex.ru/track/2816")
        );
        assert_eq!(
            album("1039").as_deref(),
            Some("https://music.yandex.ru/album/1039")
        );
        assert_eq!(
            artist("289674").as_deref(),
            Some("https://music.yandex.ru/artist/289674")
        );
        for value in [
            "",
            "0",
            "1:2",
            "+1",
            " 1",
            "１",
            "18446744073709551616",
            "../users/1",
            "1?oauth=private",
            "https://storage.example/audio",
        ] {
            assert!(track(value, Some("1039")).is_none());
            assert!(album(value).is_none());
            assert!(artist(value).is_none());
        }
        assert_eq!(track("2816", Some("7?token=private")), track("2816", None));
    }
}
