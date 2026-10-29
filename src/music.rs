// SPDX-License-Identifier: GPL-3.0-or-later
//! Identity-neutral Music service. Invoke on a bounded worker, not GTK.
//!
//! Endpoint/envelope knowledge was compared with yamu 0.1.2 at
//! 9803f319b9ea6f81befbc3117e4592132ed07d0e. No upstream implementation,
//! credentials, Android identity, delivery signatures or media code is copied.

use std::{
    fmt,
    io::{Cursor, Read},
    sync::{Arc, Mutex},
    time::Duration,
};

use reqwest::{
    Url,
    blocking::{Body, Client, Response},
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue},
    redirect::Policy,
};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, de::DeserializeOwned};
use zeroize::Zeroizing;

use crate::session::AccessToken;

pub mod interactions;

const ORIGIN: &str = "https://api.music.yandex.net/";
const MAX_BODY: usize = 1024 * 1024;
const PAGE_SIZE: usize = 50;
const MAX_COLLECTION: usize = 20_000;
const MAX_SEARCH_PAGE: usize = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MusicError {
    #[error("Music service could not be initialized")]
    Configuration,
    #[error("A validated Music session is required")]
    Unauthenticated,
    #[error("Music authorization was rejected")]
    AccessDenied,
    #[error("Music API access is forbidden")]
    Forbidden,
    #[error("Music API reports missing required scopes")]
    MissingScopes,
    #[error("Music API access is restricted")]
    Restricted,
    #[error("Music API rate limit reached; retry explicitly later")]
    RateLimited,
    #[error("Music service could not be reached")]
    Network,
    #[error("Music request timed out")]
    Timeout,
    #[error("Music response exceeds the supported size")]
    ResponseTooLarge,
    #[error("Music returned an unsupported response")]
    InvalidResponse,
    #[error("Invalid Music request")]
    InvalidInput,
    #[error("Music request rejected (HTTP {0})")]
    Http(u16),
}
impl MusicError {
    /// Fixed diagnostic vocabulary. Never include the endpoint, query, service
    /// message or personal response material in an operation report.
    pub fn category(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Unauthenticated => "session-required",
            Self::AccessDenied => "authorization-rejected",
            Self::Forbidden => "forbidden",
            Self::MissingScopes => "missing-scopes",
            Self::Restricted => "restricted",
            Self::RateLimited => "rate-limited",
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::ResponseTooLarge => "response-size",
            Self::InvalidResponse => "response-contract",
            Self::InvalidInput => "invalid-input",
            Self::Http(_) => "http-rejected",
        }
    }
    pub fn http_status(self) -> Option<u16> {
        match self {
            Self::AccessDenied => Some(401),
            Self::Forbidden | Self::MissingScopes => Some(403),
            Self::Restricted => Some(451),
            Self::RateLimited => Some(429),
            Self::Http(status) => Some(status),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Account {
    pub display_name: String,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album_id: Option<String>,
    pub album_title: Option<String>,
    pub duration_ms: Option<u64>,
    pub cover: Option<crate::artwork::CoverArt>,
    /// Only the provider's explicit boolean supplies this content marker.
    pub explicit: bool,
    /// Metadata availability is not stream permission or a playable resource.
    pub available: bool,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub cover: Option<crate::artwork::CoverArt>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub tracks: Vec<Track>,
    pub cover: Option<crate::artwork::CoverArt>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct PlaylistId {
    pub owner: String,
    pub kind: String,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Playlist {
    pub id: PlaylistId,
    pub title: String,
    pub track_count: u64,
    pub cover: Option<crate::artwork::CoverArt>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchKind {
    Tracks,
    Albums,
    Artists,
}
#[derive(Clone, PartialEq, Eq)]
pub struct SearchResults {
    pub tracks: Vec<Track>,
    pub albums: Vec<Album>,
    pub artists: Vec<Artist>,
    pub has_more: bool,
}

macro_rules! redacted_debug {
    ($($name:ident),+ $(,)?) => {$(
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(concat!(stringify!($name), "([redacted])")) }
        }
    )+};
}
redacted_debug!(
    Account,
    Track,
    Artist,
    Album,
    PlaylistId,
    Playlist,
    SearchResults
);
impl<T> fmt::Debug for Page<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Page")
            .field("count", &self.items.len())
            .field("has_more", &self.has_more)
            .finish()
    }
}

#[derive(Default)]
struct AccountCache {
    epoch: u64,
    validated: Option<([u8; 32], String)>,
}
impl AccountCache {
    fn invalidate(&mut self) -> u64 {
        self.epoch = self.epoch.wrapping_add(1);
        self.validated = None;
        self.epoch
    }
}
#[derive(Clone)]
pub struct MusicService {
    client: Client,
    origin: Url,
    account: Arc<Mutex<AccountCache>>,
}
impl fmt::Debug for MusicService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MusicService([redacted])")
    }
}
impl MusicService {
    #[cfg(test)]
    pub(crate) fn for_test(base_url: &str) -> Self {
        let origin = Url::parse(base_url).expect("synthetic fixture URL");
        assert_eq!(origin.scheme(), "http");
        assert!(matches!(origin.host_str(), Some("127.0.0.1" | "[::1]")));
        assert!(
            origin.query().is_none()
                && origin.fragment().is_none()
                && origin.username().is_empty()
                && origin.password().is_none()
        );
        Self {
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .redirect(Policy::none())
                .retry(reqwest::retry::never())
                .build()
                .expect("fixture client"),
            origin,
            account: Arc::new(Mutex::new(AccountCache::default())),
        }
    }
    pub fn new() -> Result<Self, MusicError> {
        let client = Client::builder()
            .user_agent(concat!("Karin/", env!("CARGO_PKG_VERSION"), " music"))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .map_err(|_| MusicError::Configuration)?;
        Ok(Self {
            client,
            origin: Url::parse(ORIGIN).map_err(|_| MusicError::Configuration)?,
            account: Arc::new(Mutex::new(AccountCache::default())),
        })
    }
    pub fn clear_session(&self) {
        if let Ok(mut account) = self.account.lock() {
            account.invalidate();
        }
    }
    fn token_key(token: &AccessToken) -> [u8; 32] {
        digest(&SHA256, token.expose().as_bytes())
            .as_ref()
            .try_into()
            .expect("SHA256 length")
    }
    fn account_id(&self, token: &AccessToken) -> Result<String, MusicError> {
        if token.is_expired() {
            return Err(MusicError::Unauthenticated);
        }
        self.account
            .lock()
            .map_err(|_| MusicError::Configuration)?
            .validated
            .as_ref()
            .filter(|(key, _)| *key == Self::token_key(token))
            .map(|(_, uid)| uid.clone())
            .ok_or(MusicError::Unauthenticated)
    }
    /// Check the existing token-bound Music validation without changing it.
    /// A successful Yandex ID grant alone never opens authenticated operations.
    pub(crate) fn validate_session(&self, token: &AccessToken) -> Result<(), MusicError> {
        self.account_id(token).map(|_| ())
    }
    pub fn account(&self, token: &AccessToken) -> Result<Account, MusicError> {
        // A failed revalidation must not leave a previously accepted account
        // available to later personal requests with this token.
        // The same lock both invalidates old facts and identifies this attempt.
        // Logout or a newer validation invalidates its future response as well.
        let epoch = {
            let mut cache = self.account.lock().map_err(|_| MusicError::Configuration)?;
            // A queued worker may begin after logout and after a new session
            // has validated. Its shared invalidated token must not clear that
            // newer account merely by starting this method.
            if token.is_expired() {
                return Err(MusicError::Unauthenticated);
            }
            cache.invalidate()
        };
        let outcome = (|| {
            let status: AccountWire = self.get(token, &["account", "status"], &[])?;
            let uid = status.account.uid.0;
            if !uid.bytes().all(|byte| byte.is_ascii_digit())
                || !uid.parse::<u64>().is_ok_and(|uid| uid > 0)
            {
                return Err(MusicError::Unauthenticated);
            }
            let name = text(
                status
                    .account
                    .display_name
                    .or(status.account.full_name)
                    .or(status.account.login)
                    .unwrap_or_else(|| "Аккаунт Яндекс Музыки".into()),
            )?;
            Ok((uid, name))
        })();
        let mut cache = self.account.lock().map_err(|_| MusicError::Configuration)?;
        if cache.epoch != epoch || token.is_expired() {
            return Err(MusicError::Unauthenticated);
        }
        let (uid, name) = outcome?;
        cache.validated = Some((Self::token_key(token), uid));
        Ok(Account { display_name: name })
    }
    pub fn liked_tracks(&self, token: &AccessToken, page: u32) -> Result<Page<Track>, MusicError> {
        let uid = self.account_id(token)?;
        let wire: LibraryWire = self.get(
            token,
            &["users", &uid, "likes", "tracks"],
            &[("if-modified-since-revision", "0")],
        )?;
        let library = wire.library.ok_or(MusicError::InvalidResponse)?;
        self.expand_page(token, library.tracks, page)
    }
    pub fn playlists(&self, token: &AccessToken, page: u32) -> Result<Page<Playlist>, MusicError> {
        let uid = self.account_id(token)?;
        let wire: Vec<PlaylistWire> =
            self.get(token, &["users", &uid, "playlists", "list"], &[])?;
        let (wire, has_more) = local_page(wire, page)?;
        let items = wire
            .into_iter()
            .map(playlist_from_wire)
            .collect::<Result<_, MusicError>>()?;
        Ok(Page { items, has_more })
    }
    pub fn playlist_tracks(
        &self,
        token: &AccessToken,
        id: &PlaylistId,
        page: u32,
    ) -> Result<Page<Track>, MusicError> {
        validate_id(&id.owner)?;
        validate_id(&id.kind)?;
        let wire: PlaylistWire = self.get(
            token,
            &["users", &id.owner, "playlists", &id.kind],
            &[("richTracks", "true")],
        )?;
        let references = match wire.tracks {
            Some(tracks) => tracks,
            None if wire.track_count == Some(0) => Vec::new(),
            None => return Err(MusicError::InvalidResponse),
        };
        self.expand_page(token, references, page)
    }
    fn expand_page(
        &self,
        token: &AccessToken,
        references: Vec<TrackReference>,
        page: u32,
    ) -> Result<Page<Track>, MusicError> {
        let (references, has_more) = local_page(references, page)?;
        // richTracks embeds the actual track in a compact entry. Preserve it,
        // and expand only missing entries in one bounded batch. Duplicates and
        // playlist order belong to the collection, not to the batch response.
        let mut ordered = Vec::with_capacity(references.len());
        let mut missing = Vec::new();
        for entry in references {
            let id = match entry.album_id {
                Some(album) => format!("{}:{}", entry.id.0, album.0),
                None => entry.id.0.clone(),
            };
            let embedded = entry.track.map(track_from_wire).transpose()?;
            if let Some(track) = &embedded {
                if track.id != id && track.id != entry.id.0 {
                    return Err(MusicError::InvalidResponse);
                }
            } else {
                missing.push(id);
            }
            ordered.push(embedded);
        }
        let expanded = self.tracks(token, &missing)?;
        let mut expanded = expanded.into_iter();
        let items = ordered
            .into_iter()
            .map(|track| {
                track
                    .or_else(|| expanded.next())
                    .ok_or(MusicError::InvalidResponse)
            })
            .collect::<Result<_, _>>()?;
        Ok(Page { items, has_more })
    }
    pub fn search(
        &self,
        token: &AccessToken,
        query: &str,
        kind: SearchKind,
        page: u32,
    ) -> Result<SearchResults, MusicError> {
        if query.trim().is_empty()
            || query.len() > 512
            || query.chars().any(char::is_control)
            || page > 10_000
        {
            return Err(MusicError::InvalidInput);
        }
        let category = match kind {
            SearchKind::Tracks => "track",
            SearchKind::Albums => "album",
            SearchKind::Artists => "artist",
        };
        let page_text = page.to_string();
        let wire: SearchWire = self.get(
            token,
            &["search"],
            &[
                ("text", query),
                ("type", category),
                ("page", &page_text),
                ("nocorrect", "true"),
                ("playlist-in-best", "false"),
            ],
        )?;
        // Upstream models make the echo and page counters optional. Require a
        // recognized envelope, without rejecting a real section merely because
        // the service omitted its redundant query echo.
        let has_echo = wire.text.is_some();
        if let Some(echo) = wire.text {
            text(echo)?;
        }
        let has_section = match kind {
            SearchKind::Tracks => wire.tracks.is_some(),
            SearchKind::Albums => wire.albums.is_some(),
            SearchKind::Artists => wire.artists.is_some(),
        };
        if !has_echo && !has_section {
            return Err(MusicError::InvalidResponse);
        }
        let mut results = SearchResults {
            tracks: Vec::new(),
            albums: Vec::new(),
            artists: Vec::new(),
            has_more: false,
        };
        match kind {
            SearchKind::Tracks => {
                if let Some(section) = wire.tracks {
                    let (items, more) = search_page(section, page)?;
                    results.tracks = items
                        .into_iter()
                        .map(track_from_wire)
                        .collect::<Result<_, _>>()?;
                    results.has_more = more;
                }
            }
            SearchKind::Albums => {
                if let Some(section) = wire.albums {
                    let (items, more) = search_page(section, page)?;
                    results.albums = items
                        .into_iter()
                        .map(album_from_wire)
                        .collect::<Result<_, _>>()?;
                    results.has_more = more;
                }
            }
            SearchKind::Artists => {
                if let Some(section) = wire.artists {
                    let (items, more) = search_page(section, page)?;
                    results.artists = items
                        .into_iter()
                        .map(artist_from_wire)
                        .collect::<Result<_, _>>()?;
                    results.has_more = more;
                }
            }
        }
        Ok(results)
    }
    pub fn track(&self, token: &AccessToken, id: &str) -> Result<Track, MusicError> {
        validate_id(id)?;
        let mut items = self.tracks(token, &[id.to_owned()])?;
        if items.len() != 1 {
            return Err(MusicError::InvalidResponse);
        }
        Ok(items.remove(0))
    }
    fn tracks(&self, token: &AccessToken, ids: &[String]) -> Result<Vec<Track>, MusicError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        if ids.len() > PAGE_SIZE {
            return Err(MusicError::InvalidInput);
        }
        let mut distinct = Vec::new();
        for id in ids {
            validate_id(id)?;
            if !distinct.contains(&id.as_str()) {
                distinct.push(id.as_str());
            }
        }
        // Use URL's form encoder without sending this temporary query URL.
        let mut url = self.origin.clone();
        {
            let mut pairs = url.query_pairs_mut();
            for id in &distinct {
                pairs.append_pair("track-ids", id);
            }
        }
        let form = Zeroizing::new(url.query().unwrap_or_default().as_bytes().to_vec());
        let wire: Vec<TrackWire> = self.request(token, &["tracks"], &[], Some(form))?;
        if wire.len() > distinct.len() {
            return Err(MusicError::InvalidResponse);
        }
        // Preserve requested order, including composite track:album IDs. Missing
        // records fail visibly rather than displaying invented metadata.
        let tracks = wire
            .into_iter()
            .map(track_from_wire)
            .collect::<Result<Vec<_>, _>>()?;
        let mut ordered = Vec::with_capacity(ids.len());
        for id in ids {
            let bare = id.split(':').next().ok_or(MusicError::InvalidInput)?;
            let track = tracks
                .iter()
                .find(|track| track.id == *id || track.id == bare)
                .ok_or(MusicError::InvalidResponse)?;
            ordered.push(track.clone());
        }
        Ok(ordered)
    }
    pub fn album(&self, token: &AccessToken, id: &str) -> Result<Album, MusicError> {
        validate_id(id)?;
        let wire: AlbumWire = self.get(token, &["albums", id, "with-tracks"], &[])?;
        if wire.volumes.is_none() {
            return Err(MusicError::InvalidResponse);
        }
        album_from_wire(wire)
    }
    /// Open an artist result through its own paged track endpoint, rather than
    /// silently turning the artist's display name into another catalog search.
    pub fn artist_tracks(
        &self,
        token: &AccessToken,
        id: &str,
        page: u32,
    ) -> Result<Page<Track>, MusicError> {
        validate_id(id)?;
        if page > 10_000 {
            return Err(MusicError::InvalidInput);
        }
        let page_text = page.to_string();
        let size = PAGE_SIZE.to_string();
        let wire: ArtistTracksWire = self.get(
            token,
            &["artists", id, "tracks"],
            &[("page", &page_text), ("page-size", &size)],
        )?;
        if wire.tracks.len() > MAX_SEARCH_PAGE {
            return Err(MusicError::ResponseTooLarge);
        }
        let has_more = match wire.pager {
            Some(pager) => {
                if pager.page.is_some_and(|index| index != u64::from(page)) {
                    return Err(MusicError::InvalidResponse);
                }
                page_has_more(
                    wire.tracks.len(),
                    pager.total,
                    pager.per_page.or(Some(PAGE_SIZE as u64)),
                    page,
                )?
            }
            None => wire.tracks.len() == PAGE_SIZE,
        };
        let items = wire
            .tracks
            .into_iter()
            .map(track_from_wire)
            .collect::<Result<_, _>>()?;
        Ok(Page { items, has_more })
    }
    fn get<T: DeserializeOwned>(
        &self,
        token: &AccessToken,
        path: &[&str],
        query: &[(&str, &str)],
    ) -> Result<T, MusicError> {
        self.request(token, path, query, None)
    }
    fn request<T: DeserializeOwned>(
        &self,
        token: &AccessToken,
        path: &[&str],
        query: &[(&str, &str)],
        form: Option<Zeroizing<Vec<u8>>>,
    ) -> Result<T, MusicError> {
        if token.is_expired() {
            return Err(MusicError::Unauthenticated);
        }
        let mut url = self.origin.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| MusicError::Configuration)?;
            segments.pop_if_empty();
            for part in path {
                if part.is_empty() || matches!(*part, "." | "..") {
                    return Err(MusicError::InvalidInput);
                }
                segments.push(part);
            }
        }
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query.iter().copied());
        }
        let authorization = Zeroizing::new(format!("OAuth {}", token.expose()));
        let mut header =
            HeaderValue::from_str(&authorization).map_err(|_| MusicError::Unauthenticated)?;
        header.set_sensitive(true);
        let mut request = if form.is_some() {
            self.client.post(url)
        } else {
            self.client.get(url)
        };
        request = request
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, header);
        if let Some(form) = form {
            let len = form.len() as u64;
            request = request
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::sized(Cursor::new(form), len));
        }
        let response = request.send().map_err(network_error)?;
        let status = response.status().as_u16();
        if status == 403 {
            return Err(read_body(response)
                .map(|body| forbidden_error(&body))
                .unwrap_or(MusicError::Forbidden));
        }
        if status != 200 {
            return Err(match status {
                401 => MusicError::AccessDenied,
                429 => MusicError::RateLimited,
                451 => MusicError::Restricted,
                _ => MusicError::Http(status),
            });
        }
        let body = read_body(response)?;
        serde_json::from_slice::<Envelope<T>>(&body)
            .map(|envelope| envelope.result)
            .map_err(|_| MusicError::InvalidResponse)
    }
}

fn network_error(error: reqwest::Error) -> MusicError {
    if error.is_timeout() {
        MusicError::Timeout
    } else {
        MusicError::Network
    }
}
fn read_body(mut response: Response) -> Result<Zeroizing<Vec<u8>>, MusicError> {
    if response
        .content_length()
        .is_some_and(|size| size > MAX_BODY as u64)
    {
        return Err(MusicError::ResponseTooLarge);
    }
    let mut body = Zeroizing::new(Vec::with_capacity(MAX_BODY + 1));
    let mut bytes = Zeroizing::new([0u8; 8192]);
    loop {
        let count = response.read(bytes.as_mut()).map_err(|error| {
            if error.kind() == std::io::ErrorKind::TimedOut
                || error
                    .get_ref()
                    .and_then(|source| source.downcast_ref::<reqwest::Error>())
                    .is_some_and(reqwest::Error::is_timeout)
            {
                MusicError::Timeout
            } else {
                MusicError::Network
            }
        })?;
        if count == 0 {
            return Ok(body);
        }
        if body.len() + count > MAX_BODY {
            return Err(MusicError::ResponseTooLarge);
        }
        body.extend_from_slice(&bytes[..count]);
    }
}
fn validate_id(value: &str) -> Result<(), MusicError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_:".contains(&byte))
    {
        Err(MusicError::InvalidInput)
    } else {
        Ok(())
    }
}
fn text(value: String) -> Result<String, MusicError> {
    if value.len() > 4096
        || value.chars().any(|character| {
            character == '\0' || (character.is_control() && !matches!(character, '\n' | '\t'))
        })
    {
        Err(MusicError::InvalidResponse)
    } else {
        Ok(value)
    }
}
fn local_page<T>(items: Vec<T>, page: u32) -> Result<(Vec<T>, bool), MusicError> {
    if items.len() > MAX_COLLECTION {
        return Err(MusicError::ResponseTooLarge);
    }
    let start = usize::try_from(page)
        .ok()
        .and_then(|page| page.checked_mul(PAGE_SIZE))
        .ok_or(MusicError::InvalidInput)?;
    let has_more = items.len() > start.saturating_add(PAGE_SIZE);
    Ok((
        items.into_iter().skip(start).take(PAGE_SIZE).collect(),
        has_more,
    ))
}
fn search_page<T>(page: SearchPageWire<T>, index: u32) -> Result<(Vec<T>, bool), MusicError> {
    if page.results.len() > MAX_SEARCH_PAGE {
        return Err(MusicError::ResponseTooLarge);
    }
    // A recognized empty page is valid. An arbitrary empty section, or a
    // nonempty total with no rows, must not create an endless Next loop.
    if page.results.is_empty() && page.total.is_none() && page.per_page.is_none() {
        return Err(MusicError::InvalidResponse);
    }
    let more = page_has_more(page.results.len(), page.total, page.per_page, index)?;
    Ok((page.results, more))
}
fn page_has_more(
    count: usize,
    total: Option<u64>,
    per_page: Option<u64>,
    index: u32,
) -> Result<bool, MusicError> {
    if per_page.is_some_and(|per_page| per_page > MAX_COLLECTION as u64 || per_page < count as u64)
        || total.is_some_and(|total| total < count as u64)
    {
        return Err(MusicError::InvalidResponse);
    }
    if count == 0 {
        return Ok(false);
    }
    let size = per_page.unwrap_or(count as u64);
    let consumed = (u64::from(index) + 1)
        .checked_mul(size)
        .ok_or(MusicError::InvalidResponse)?;
    Ok(total.map_or_else(|| count as u64 == size, |total| consumed < total))
}

#[derive(Deserialize)]
struct Envelope<T> {
    result: T,
    #[serde(default, rename = "error", deserialize_with = "reject_error")]
    _error: (),
}
fn reject_error<'de, D: serde::Deserializer<'de>>(_: D) -> Result<(), D::Error> {
    Err(serde::de::Error::custom("error response"))
}
#[derive(Clone)]
struct Id(String);
impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = Id;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded identifier")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Id, E> {
                validate_id(value).map_err(|_| E::custom("invalid identifier"))?;
                Ok(Id(value.to_owned()))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Id, E> {
                Ok(Id(value.to_string()))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Id, E> {
                if value < 0 {
                    Err(E::custom("invalid identifier"))
                } else {
                    Ok(Id(value.to_string()))
                }
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
#[derive(Deserialize)]
struct AccountWire {
    account: AccountFields,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountFields {
    uid: Id,
    display_name: Option<String>,
    full_name: Option<String>,
    login: Option<String>,
}
#[derive(Deserialize)]
struct LibraryWire {
    library: Option<LibraryFields>,
}
#[derive(Deserialize)]
struct LibraryFields {
    tracks: Vec<TrackReference>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackReference {
    id: Id,
    album_id: Option<Id>,
    track: Option<TrackWire>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistWire {
    owner: Option<OwnerWire>,
    uid: Option<Id>,
    kind: Option<Id>,
    title: Option<String>,
    track_count: Option<u64>,
    tracks: Option<Vec<TrackReference>>,
    #[serde(default, deserialize_with = "cover_object")]
    cover: Option<crate::artwork::CoverArt>,
}
#[derive(Deserialize)]
struct OwnerWire {
    uid: Option<Id>,
}
#[derive(Deserialize)]
struct ArtistWire {
    id: Option<Id>,
    name: Option<String>,
    #[serde(default, deserialize_with = "cover_object")]
    cover: Option<crate::artwork::CoverArt>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackWire {
    id: Id,
    title: Option<String>,
    #[serde(default)]
    artists: Vec<ArtistWire>,
    #[serde(default)]
    albums: Vec<AlbumWire>,
    duration_ms: Option<u64>,
    available: Option<bool>,
    #[serde(default, deserialize_with = "cover_template")]
    cover_uri: Option<crate::artwork::CoverArt>,
    #[serde(default, deserialize_with = "explicit_flag")]
    explicit: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlbumWire {
    id: Id,
    title: Option<String>,
    #[serde(default)]
    artists: Vec<ArtistWire>,
    volumes: Option<Vec<Vec<TrackWire>>>,
    #[serde(default, deserialize_with = "cover_template")]
    cover_uri: Option<crate::artwork::CoverArt>,
}
#[derive(Deserialize)]
struct SearchWire {
    text: Option<String>,
    tracks: Option<SearchPageWire<TrackWire>>,
    albums: Option<SearchPageWire<AlbumWire>>,
    artists: Option<SearchPageWire<ArtistWire>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchPageWire<T> {
    total: Option<u64>,
    per_page: Option<u64>,
    #[serde(default = "Vec::new")]
    results: Vec<T>,
}
#[derive(Deserialize)]
struct ArtistTracksWire {
    tracks: Vec<TrackWire>,
    pager: Option<PagerWire>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PagerWire {
    total: Option<u64>,
    page: Option<u64>,
    per_page: Option<u64>,
}
fn artist_from_wire(wire: ArtistWire) -> Result<Artist, MusicError> {
    Ok(Artist {
        id: wire.id.ok_or(MusicError::InvalidResponse)?.0,
        name: text(wire.name.ok_or(MusicError::InvalidResponse)?)?,
        cover: wire.cover,
    })
}
fn playlist_from_wire(wire: PlaylistWire) -> Result<Playlist, MusicError> {
    let owner = wire
        .owner
        .and_then(|owner| owner.uid)
        .or(wire.uid)
        .ok_or(MusicError::InvalidResponse)?;
    Ok(Playlist {
        id: PlaylistId {
            owner: owner.0,
            kind: wire.kind.ok_or(MusicError::InvalidResponse)?.0,
        },
        title: text(wire.title.unwrap_or_else(|| "Без названия".into()))?,
        track_count: wire.track_count.ok_or(MusicError::InvalidResponse)?,
        cover: wire.cover,
    })
}
fn artist_names(wire: Vec<ArtistWire>) -> Result<Vec<String>, MusicError> {
    // Embedded credits may have a name without an independently browsable ID.
    // Missing credits are omitted; they do not invalidate an otherwise real
    // track/album. Standalone artist results remain strictly identified above.
    wire.into_iter()
        .filter_map(|artist| artist.name)
        .map(text)
        .collect()
}
fn track_from_wire(wire: TrackWire) -> Result<Track, MusicError> {
    let album = wire.albums.into_iter().next();
    let cover = wire
        .cover_uri
        .or_else(|| album.as_ref().and_then(|album| album.cover_uri.clone()));
    Ok(Track {
        id: wire.id.0,
        title: text(wire.title.unwrap_or_else(|| "Без названия".into()))?,
        artists: artist_names(wire.artists)?,
        album_id: album.as_ref().map(|album| album.id.0.clone()),
        album_title: album.and_then(|album| album.title).map(text).transpose()?,
        duration_ms: wire.duration_ms,
        available: wire.available.unwrap_or(false),
        cover,
        explicit: wire.explicit,
    })
}
fn album_from_wire(wire: AlbumWire) -> Result<Album, MusicError> {
    let tracks = wire
        .volumes
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .take(MAX_COLLECTION + 1)
        .map(track_from_wire)
        .collect::<Result<Vec<_>, _>>()?;
    if tracks.len() > MAX_COLLECTION {
        return Err(MusicError::ResponseTooLarge);
    }
    Ok(Album {
        id: wire.id.0,
        title: text(wire.title.unwrap_or_else(|| "Без названия".into()))?,
        artists: artist_names(wire.artists)?,
        tracks,
        cover: wire.cover_uri,
    })
}

// Artwork is optional presentation metadata, not a network capability carried
// verbatim from the service. These transient values are bounded by the existing
// 1 MiB response limit and are never logged or retained in an application model.
// A malformed image field must not discard an otherwise usable track or page.
fn cover_template<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<crate::artwork::CoverArt>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_str().and_then(crate::artwork::CoverArt::parse))
}

fn cover_object<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<crate::artwork::CoverArt>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    let direct = value
        .get("uri")
        .and_then(serde_json::Value::as_str)
        .and_then(crate::artwork::CoverArt::parse);
    Ok(direct.or_else(|| {
        value
            .get("itemsUri")
            .and_then(serde_json::Value::as_array)?
            .iter()
            .filter_map(serde_json::Value::as_str)
            .find_map(crate::artwork::CoverArt::parse)
    }))
}

fn explicit_flag<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(serde_json::Value::deserialize(deserializer)?
        .as_bool()
        .unwrap_or(false))
}

#[derive(Deserialize)]
struct ForbiddenEnvelope {
    error: ForbiddenCode,
}
enum ForbiddenCode {
    Missing,
    Unknown,
}
impl<'de> Deserialize<'de> for ForbiddenCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ForbiddenCode;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("known error identifier")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(if value == "missing-required-scopes" {
                    ForbiddenCode::Missing
                } else {
                    ForbiddenCode::Unknown
                })
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut code = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "name" {
                        if code.is_some() {
                            return Err(serde::de::Error::duplicate_field("name"));
                        }
                        let value = Zeroizing::new(map.next_value::<String>()?);
                        code = Some(if value.as_str() == "missing-required-scopes" {
                            ForbiddenCode::Missing
                        } else {
                            ForbiddenCode::Unknown
                        });
                    } else {
                        map.next_value::<serde::de::IgnoredAny>()?;
                    }
                }
                Ok(code.unwrap_or(ForbiddenCode::Unknown))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
pub(crate) fn forbidden_error(body: &[u8]) -> MusicError {
    match serde_json::from_slice::<ForbiddenEnvelope>(body) {
        Ok(ForbiddenEnvelope {
            error: ForbiddenCode::Missing,
        }) => MusicError::MissingScopes,
        _ => MusicError::Forbidden,
    }
}

#[cfg(test)]
mod tests;
