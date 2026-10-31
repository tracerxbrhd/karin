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
use serde_json::value::RawValue;
use zeroize::Zeroizing;

use crate::session::AccessToken;

pub mod cache_codec;
pub mod interactions;
pub mod playlist_creation;
pub mod wave;

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
    #[error("Music search failed at {stage} ({kind}, HTTP {http})")]
    SearchResponse {
        stage: SearchParseStage,
        kind: SearchFailureKind,
        http: u16,
    },
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
            Self::SearchResponse { kind, .. } => kind.category(),
            Self::InvalidInput => "invalid-input",
            Self::Http(_) => "http-rejected",
        }
    }
    /// Finite parsing stage, never a serde path, provider message or field value.
    pub fn diagnostic_stage(self) -> Option<&'static str> {
        match self {
            Self::SearchResponse { stage, .. } => Some(stage.as_str()),
            _ => None,
        }
    }
    /// A fixed schema-shape category; never a serde path or provider value.
    pub fn diagnostic_shape(self) -> Option<&'static str> {
        match self {
            Self::SearchResponse {
                stage: SearchParseStage::TrackWireShape(shape),
                ..
            } => Some(shape.as_str()),
            _ => None,
        }
    }
    pub fn http_status(self) -> Option<u16> {
        match self {
            Self::AccessDenied => Some(401),
            Self::Forbidden | Self::MissingScopes => Some(403),
            Self::Restricted => Some(451),
            Self::RateLimited => Some(429),
            Self::Http(status) | Self::SearchResponse { http: status, .. } => Some(status),
            _ => None,
        }
    }
}

/// The first failed layer in an otherwise successful search HTTP response.
/// This vocabulary contains no response field paths or account/catalog data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchParseStage {
    Envelope,
    Section,
    Pagination,
    TrackWire,
    TrackWireShape(SearchSchemaShape),
    TrackNormalization,
    TrackLookup,
    AlbumWire,
    AlbumNormalization,
    ArtistWire,
    ArtistNormalization,
    PlaylistWire,
    PlaylistNormalization,
}
impl SearchParseStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Envelope => "search-envelope",
            Self::Section => "search-section",
            Self::Pagination => "search-pagination",
            Self::TrackWire | Self::TrackWireShape(_) => "track-wire",
            Self::TrackNormalization => "track-normalization",
            Self::TrackLookup => "track-lookup",
            Self::AlbumWire => "album-wire",
            Self::AlbumNormalization => "album-normalization",
            Self::ArtistWire => "artist-wire",
            Self::ArtistNormalization => "artist-normalization",
            Self::PlaylistWire => "playlist-wire",
            Self::PlaylistNormalization => "playlist-normalization",
        }
    }
}
/// Nonpersonal categories for a rejected track record. Optional display credit
/// values are not retained here, even if the provider returned an error record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchSchemaShape {
    RecordNotObject,
    IdentityMissing,
    IdentityType,
    IdentityInvalid,
    AvailabilityType,
    FieldsUnsupported,
}
impl SearchSchemaShape {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RecordNotObject => "track-record-not-object",
            Self::IdentityMissing => "track-id-missing",
            Self::IdentityType => "track-id-type",
            Self::IdentityInvalid => "track-id-invalid",
            Self::AvailabilityType => "track-availability-type",
            Self::FieldsUnsupported => "track-fields-unsupported",
        }
    }
}
impl fmt::Display for SearchParseStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchFailureKind {
    Contract,
    ResponseTooLarge,
    Network,
    Timeout,
}
impl SearchFailureKind {
    fn category(self) -> &'static str {
        match self {
            Self::Contract => "response-contract",
            Self::ResponseTooLarge => "response-size",
            Self::Network => "network",
            Self::Timeout => "timeout",
        }
    }
}
impl fmt::Display for SearchFailureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.category())
    }
}
fn search_failure(error: MusicError, stage: SearchParseStage, http: u16) -> MusicError {
    let kind = match error {
        MusicError::InvalidResponse => SearchFailureKind::Contract,
        MusicError::ResponseTooLarge => SearchFailureKind::ResponseTooLarge,
        MusicError::Network => SearchFailureKind::Network,
        MusicError::Timeout => SearchFailureKind::Timeout,
        _ => return error,
    };
    MusicError::SearchResponse { stage, kind, http }
}
fn search_contract(stage: SearchParseStage, http: u16) -> MusicError {
    search_failure(MusicError::InvalidResponse, stage, http)
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
    /// Independently validated canonical artist credits, never inferred by name.
    pub artist_links: Vec<Artist>,
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
/// Search summaries do not always carry a track count. Unlike a validated
/// Library playlist, an omitted search count must not be presented as zero.
#[derive(Clone, PartialEq, Eq)]
pub struct SearchPlaylist {
    pub id: PlaylistId,
    pub title: String,
    pub track_count: Option<u64>,
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
    Playlists,
}
#[derive(Clone, PartialEq, Eq)]
pub struct SearchResults {
    /// Status actually observed for the successful search response.
    pub http_status: u16,
    pub tracks: Vec<Track>,
    pub albums: Vec<Album>,
    pub artists: Vec<Artist>,
    pub playlists: Vec<SearchPlaylist>,
    pub has_more: bool,
    /// Search positions whose normalized track was absent/null in the batch.
    /// Explicitly unavailable identified tracks remain visible and disabled.
    pub omitted_tracks: usize,
}
#[derive(Clone, PartialEq, Eq)]
pub struct SearchSection<T> {
    pub items: Vec<T>,
    pub has_more: bool,
    /// Positions omitted during normalization, not a fabricated item count.
    pub omitted: usize,
}
#[derive(Clone, PartialEq, Eq)]
pub struct UnifiedSearchResults {
    /// Status of the one observed `type=all` search response.
    pub http_status: u16,
    pub tracks: Result<SearchSection<Track>, MusicError>,
    pub albums: Result<SearchSection<Album>, MusicError>,
    pub artists: Result<SearchSection<Artist>, MusicError>,
    pub playlists: Result<SearchSection<SearchPlaylist>, MusicError>,
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
    SearchPlaylist,
    SearchResults,
    UnifiedSearchResults
);
impl<T> fmt::Debug for SearchSection<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SearchSection")
            .field("count", &self.items.len())
            .field("has_more", &self.has_more)
            .field("omitted", &self.omitted)
            .finish()
    }
}
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
struct SearchBinding {
    epoch: u64,
    token_key: [u8; 32],
    uid: String,
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
    fn search_binding(&self, token: &AccessToken) -> Result<SearchBinding, MusicError> {
        if token.is_expired() {
            return Err(MusicError::Unauthenticated);
        }
        let cache = self.account.lock().map_err(|_| MusicError::Configuration)?;
        let key = Self::token_key(token);
        let (_, uid) = cache
            .validated
            .as_ref()
            .filter(|(accepted, _)| *accepted == key)
            .ok_or(MusicError::Unauthenticated)?;
        Ok(SearchBinding {
            epoch: cache.epoch,
            token_key: key,
            uid: uid.clone(),
        })
    }
    fn check_search_binding(
        &self,
        token: &AccessToken,
        binding: &SearchBinding,
    ) -> Result<(), MusicError> {
        if token.is_expired() {
            return Err(MusicError::Unauthenticated);
        }
        let cache = self.account.lock().map_err(|_| MusicError::Configuration)?;
        if cache.epoch != binding.epoch
            || !cache
                .validated
                .as_ref()
                .is_some_and(|(key, uid)| *key == binding.token_key && *uid == binding.uid)
        {
            return Err(MusicError::Unauthenticated);
        }
        Ok(())
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
        validate_search_input(query, page)?;
        let binding = self.search_binding(token)?;
        let category = match kind {
            SearchKind::Tracks => "track",
            SearchKind::Albums => "album",
            SearchKind::Artists => "artist",
            SearchKind::Playlists => "playlist",
        };
        let (body, http) = self.search_body(token, query, category, page, &binding)?;
        let (results, hits) = parse_search(&body, kind, page, http)?;
        self.normalize_search_tracks(token, results, hits, http, &binding)
    }
    /// Initial unified search is one `type=all` request, plus at most one
    /// existing bounded track batch. A malformed section or failed track
    /// lookup does not erase successful sections from the same response.
    /// Per-section Show All/Retry uses `search` and its own page index.
    pub fn search_all(
        &self,
        token: &AccessToken,
        query: &str,
    ) -> Result<UnifiedSearchResults, MusicError> {
        validate_search_input(query, 0)?;
        let binding = self.search_binding(token)?;
        let (body, http) = self.search_body(token, query, "all", 0, &binding)?;
        let wire = parse_search_wire(&body, http)?;
        if !valid_search_echo(&wire.text)
            && wire.tracks.is_none()
            && wire.albums.is_none()
            && wire.artists.is_none()
            && wire.playlists.is_none()
        {
            return Err(search_contract(SearchParseStage::Section, http));
        }
        let albums = parse_search_section(wire.albums, SearchKind::Albums, 0, http, true).map(
            |(results, _)| SearchSection {
                items: results.albums,
                has_more: results.has_more,
                omitted: 0,
            },
        );
        let artists = parse_search_section(wire.artists, SearchKind::Artists, 0, http, true).map(
            |(results, _)| SearchSection {
                items: results.artists,
                has_more: results.has_more,
                omitted: 0,
            },
        );
        let playlists = parse_search_section(wire.playlists, SearchKind::Playlists, 0, http, true)
            .map(|(results, _)| SearchSection {
                items: results.playlists,
                has_more: results.has_more,
                omitted: 0,
            });
        let tracks = parse_search_section(wire.tracks, SearchKind::Tracks, 0, http, true)
            .and_then(|(results, hits)| {
                self.normalize_search_tracks(token, results, hits, http, &binding)
            })
            .map(|results| SearchSection {
                items: results.tracks,
                has_more: results.has_more,
                omitted: results.omitted_tracks,
            });
        // Errors above are section-local only while this exact accepted account
        // is still current. Never return old sections after logout/revalidation.
        self.check_search_binding(token, &binding)?;
        if let Err(error @ (MusicError::AccessDenied | MusicError::Unauthenticated)) = &tracks {
            return Err(*error);
        }
        Ok(UnifiedSearchResults {
            http_status: http,
            tracks,
            albums,
            artists,
            playlists,
        })
    }
    fn search_body(
        &self,
        token: &AccessToken,
        query: &str,
        category: &str,
        page: u32,
        binding: &SearchBinding,
    ) -> Result<(Zeroizing<Vec<u8>>, u16), MusicError> {
        let page_text = page.to_string();
        let response = self.request_response(
            token,
            &["search"],
            &[
                ("text", query),
                ("type", category),
                ("page", &page_text),
                ("nocorrect", "true"),
                ("playlist-in-best", "false"),
            ],
            None,
        );
        self.check_search_binding(token, binding)?;
        let response = response?;
        let http = response.status().as_u16();
        let body = read_body(response)
            .map_err(|error| search_failure(error, SearchParseStage::Envelope, http))?;
        self.check_search_binding(token, binding)?;
        Ok((body, http))
    }
    fn normalize_search_tracks(
        &self,
        token: &AccessToken,
        mut results: SearchResults,
        hits: Vec<SearchTrackHit>,
        http: u16,
        binding: &SearchBinding,
    ) -> Result<SearchResults, MusicError> {
        self.check_search_binding(token, binding)?;
        if !hits.is_empty() {
            // Bound both the HTTP batch and the number of owned normalized
            // Track clones retained for duplicate positions in one UI page.
            if hits.len() > PAGE_SIZE {
                return Err(search_failure(
                    MusicError::ResponseTooLarge,
                    SearchParseStage::TrackLookup,
                    http,
                ));
            }
            let mut distinct = Vec::new();
            for hit in &hits {
                if !distinct.contains(&hit.id.0) {
                    distinct.push(hit.id.0.clone());
                }
            }
            // Keep the existing /tracks batch bound. A larger provider page
            // cannot silently lose rows or turn into per-row HTTP requests.
            if distinct.len() > PAGE_SIZE {
                return Err(search_failure(
                    MusicError::ResponseTooLarge,
                    SearchParseStage::TrackLookup,
                    http,
                ));
            }
            let response = self.track_batch_response(token, &distinct);
            self.check_search_binding(token, binding)?;
            // Before headers, preserve transport errors without inventing a
            // successful POST status from the earlier search HTTP 200.
            let response = response?;
            let lookup_http = response.status().as_u16();
            let body = read_body(response).map_err(|error| {
                search_failure(error, SearchParseStage::TrackLookup, lookup_http)
            })?;
            let tracks = decode_track_batch(&body, distinct.len()).map_err(|error| {
                search_failure(error, SearchParseStage::TrackLookup, lookup_http)
            })?;
            self.check_search_binding(token, binding)?;
            let ordered = order_search_tracks(hits, tracks).map_err(|error| {
                search_failure(error, SearchParseStage::TrackLookup, lookup_http)
            })?;
            results.tracks = ordered.0;
            results.omitted_tracks = ordered.1;
        }
        self.check_search_binding(token, binding)?;
        Ok(results)
    }
    pub fn track(&self, token: &AccessToken, id: &str) -> Result<Track, MusicError> {
        validate_id(id)?;
        let mut items = self.tracks(token, &[id.to_owned()])?;
        if items.len() != 1 {
            return Err(MusicError::InvalidResponse);
        }
        let mut track = items.remove(0);
        // Keep a validated composite selector across search → detail → Play.
        // /tracks may normalize that selector to a bare canonical identity.
        track.id = id.to_owned();
        Ok(track)
    }
    fn tracks(&self, token: &AccessToken, ids: &[String]) -> Result<Vec<Track>, MusicError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let response = self.track_batch_response(token, ids)?;
        let body = read_body(response)?;
        let distinct_count = ids.iter().collect::<std::collections::BTreeSet<_>>().len();
        let tracks = decode_track_batch(&body, distinct_count)?;
        // Preserve the established strict lookup behavior for library/detail
        // callers; search alone can report missing positions as partial data.
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
    fn track_batch_response(
        &self,
        token: &AccessToken,
        ids: &[String],
    ) -> Result<Response, MusicError> {
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
        self.request_response(token, &["tracks"], &[], Some(form))
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
        let response = self.request_response(token, path, query, form)?;
        let body = read_body(response)?;
        serde_json::from_slice::<Envelope<T>>(&body)
            .map(|envelope| envelope.result)
            .map_err(|_| MusicError::InvalidResponse)
    }
    fn request_response(
        &self,
        token: &AccessToken,
        path: &[&str],
        query: &[(&str, &str)],
        form: Option<Zeroizing<Vec<u8>>>,
    ) -> Result<Response, MusicError> {
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
        Ok(response)
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
fn decode_track_batch(body: &[u8], requested: usize) -> Result<Vec<Track>, MusicError> {
    let wire = serde_json::from_slice::<Envelope<Vec<Option<TrackWire>>>>(body)
        .map_err(|_| MusicError::InvalidResponse)?
        .result;
    if wire.len() > requested {
        return Err(MusicError::InvalidResponse);
    }
    wire.into_iter().flatten().map(track_from_wire).collect()
}

/// A search row supplies identity only. No display field, optional availability
/// hint, nested credit, image, or unknown metadata is trusted as a Track wire.
#[derive(Deserialize)]
struct SearchTrackHit {
    id: Id,
}

fn order_search_tracks(
    hits: Vec<SearchTrackHit>,
    tracks: Vec<Track>,
) -> Result<(Vec<Track>, usize), MusicError> {
    for (index, track) in tracks.iter().enumerate() {
        if tracks[..index].iter().any(|other| other.id == track.id)
            || !hits.iter().any(|hit| {
                hit.id.0 == track.id || hit.id.0.split(':').next() == Some(track.id.as_str())
            })
        {
            return Err(MusicError::InvalidResponse);
        }
    }
    let mut ordered = Vec::with_capacity(hits.len());
    let mut omitted = 0;
    for hit in hits {
        // Exact composite results take precedence over a canonical bare alias,
        // regardless of response order. Duplicate requested positions survive.
        let exact = tracks.iter().find(|track| track.id == hit.id.0);
        let candidate = match exact {
            Some(track) => Some(track),
            None => {
                let bare = hit
                    .id
                    .0
                    .split(':')
                    .next()
                    .ok_or(MusicError::InvalidResponse)?;
                let mut aliases = tracks.iter().filter(|track| track.id == bare);
                let first = aliases.next();
                if aliases.next().is_some() {
                    return Err(MusicError::InvalidResponse);
                }
                first
            }
        };
        match candidate {
            Some(track) => {
                let mut track = track.clone();
                // Keep the caller's validated composite selector for subsequent
                // detail/play requests; do not invent or retarget album labels.
                track.id = hit.id.0;
                ordered.push(track);
            }
            None => omitted += 1,
        }
    }
    Ok((ordered, omitted))
}
fn validate_search_input(query: &str, page: u32) -> Result<(), MusicError> {
    if query.trim().is_empty()
        || query.len() > 512
        || query.chars().any(char::is_control)
        || page > 10_000
    {
        return Err(MusicError::InvalidInput);
    }
    Ok(())
}
fn parse_search_wire(body: &[u8], http: u16) -> Result<SearchWire, MusicError> {
    serde_json::from_slice::<Envelope<SearchWire>>(body)
        .map(|envelope| envelope.result)
        .map_err(|_| search_contract(SearchParseStage::Envelope, http))
}
fn valid_search_echo(echo: &Option<Box<RawValue>>) -> bool {
    echo.as_ref()
        .and_then(|echo| serde_json::from_str::<String>(echo.get()).ok())
        .and_then(|echo| text(echo).ok())
        .is_some()
}
fn parse_search(
    body: &[u8],
    kind: SearchKind,
    page: u32,
    http: u16,
) -> Result<(SearchResults, Vec<SearchTrackHit>), MusicError> {
    let wire = parse_search_wire(body, http)?;
    let has_echo = valid_search_echo(&wire.text);
    let section = match kind {
        SearchKind::Tracks => wire.tracks,
        SearchKind::Albums => wire.albums,
        SearchKind::Artists => wire.artists,
        SearchKind::Playlists => wire.playlists,
    };
    parse_search_section(section, kind, page, http, has_echo)
}
fn parse_search_section(
    section: Option<Box<RawValue>>,
    kind: SearchKind,
    page: u32,
    http: u16,
    has_echo: bool,
) -> Result<(SearchResults, Vec<SearchTrackHit>), MusicError> {
    if !has_echo && section.is_none() {
        return Err(search_contract(SearchParseStage::Section, http));
    }
    let mut results = SearchResults {
        http_status: http,
        tracks: Vec::new(),
        albums: Vec::new(),
        artists: Vec::new(),
        playlists: Vec::new(),
        has_more: false,
        omitted_tracks: 0,
    };
    let Some(section) = section else {
        return Ok((results, Vec::new()));
    };
    let section: SearchSectionWire = serde_json::from_str(section.get())
        .map_err(|_| search_contract(SearchParseStage::Section, http))?;
    let counter = |value: Option<Box<RawValue>>| {
        value
            .map(|value| serde_json::from_str::<u64>(value.get()))
            .transpose()
            .map_err(|_| search_contract(SearchParseStage::Pagination, http))
    };
    let section = SearchPageWire {
        total: counter(section.total)?,
        per_page: counter(section.per_page)?,
        results: section.results,
    };
    let (items, more) = search_page(section, page)
        .map_err(|error| search_failure(error, SearchParseStage::Pagination, http))?;
    results.has_more = more;
    let mut hits = Vec::new();
    match kind {
        SearchKind::Tracks => {
            hits = items
                .into_iter()
                .map(|item| {
                    if !item.get().trim_start().starts_with('{') {
                        return Err(search_track_shape(&item, http));
                    }
                    serde_json::from_str::<SearchTrackHit>(item.get())
                        .map_err(|_| search_track_shape(&item, http))
                })
                .collect::<Result<_, _>>()?;
        }
        SearchKind::Albums => {
            results.albums = search_items(
                items,
                SearchParseStage::AlbumWire,
                SearchParseStage::AlbumNormalization,
                http,
                album_from_wire,
            )?;
        }
        SearchKind::Artists => {
            results.artists = search_items(
                items,
                SearchParseStage::ArtistWire,
                SearchParseStage::ArtistNormalization,
                http,
                artist_from_wire,
            )?;
        }
        SearchKind::Playlists => {
            results.playlists = items
                .into_iter()
                .map(|item| {
                    if !item.get().trim_start().starts_with('{') {
                        return Err(search_contract(SearchParseStage::PlaylistWire, http));
                    }
                    let identity = serde_json::from_str::<SearchPlaylistIdentity>(item.get())
                        .map_err(|_| search_contract(SearchParseStage::PlaylistWire, http))?;
                    // Optional decorations are not playlist identity. A malformed
                    // field or duplicate display key must not hide a valid result.
                    let display = serde_json::from_str::<SearchPlaylistDisplay>(item.get())
                        .unwrap_or_default();
                    search_playlist_from_wire(identity, display).map_err(|error| {
                        search_failure(error, SearchParseStage::PlaylistNormalization, http)
                    })
                })
                .collect::<Result<_, _>>()?;
        }
    }
    Ok((results, hits))
}
fn search_track_shape(item: &RawValue, http: u16) -> MusicError {
    #[derive(Deserialize)]
    struct Shape {
        id: Option<Box<RawValue>>,
    }
    let shape = if !item.get().trim_start().starts_with('{') {
        SearchSchemaShape::RecordNotObject
    } else if let Ok(shape) = serde_json::from_str::<Shape>(item.get()) {
        match shape.id {
            None => SearchSchemaShape::IdentityMissing,
            Some(id) if serde_json::from_str::<Id>(id.get()).is_err() => {
                match id.get().trim_start().as_bytes().first() {
                    Some(b'"' | b'-' | b'0'..=b'9') => SearchSchemaShape::IdentityInvalid,
                    _ => SearchSchemaShape::IdentityType,
                }
            }
            Some(_) => SearchSchemaShape::FieldsUnsupported,
        }
    } else {
        SearchSchemaShape::FieldsUnsupported
    };
    search_contract(SearchParseStage::TrackWireShape(shape), http)
}
fn search_items<T: DeserializeOwned, U>(
    items: Vec<Box<RawValue>>,
    wire_stage: SearchParseStage,
    normalization_stage: SearchParseStage,
    http: u16,
    normalize: fn(T) -> Result<U, MusicError>,
) -> Result<Vec<U>, MusicError> {
    items
        .into_iter()
        .map(|item| {
            let wire =
                serde_json::from_str(item.get()).map_err(|_| search_contract(wire_stage, http))?;
            normalize(wire).map_err(|error| search_failure(error, normalization_stage, http))
        })
        .collect()
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
// Published yamuse 368c746 Search.playlists uses SearchResult<Playlist>; yamu
// 9803f319 also exposes SearchType::Playlist and the owner/kind selector. Keep
// search display/count optional without changing the working Library model.
#[derive(Deserialize)]
struct SearchPlaylistIdentity {
    owner: Option<Box<RawValue>>,
    uid: Option<Id>,
    kind: Id,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchPlaylistDisplay {
    #[serde(default, deserialize_with = "display_text")]
    title: Option<String>,
    #[serde(default, deserialize_with = "display_duration")]
    track_count: Option<u64>,
    #[serde(default, deserialize_with = "cover_object")]
    cover: Option<crate::artwork::CoverArt>,
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
    #[serde(default, deserialize_with = "display_text")]
    title: Option<String>,
    #[serde(default, deserialize_with = "display_credits")]
    artists: Vec<ArtistCreditWire>,
    #[serde(default, deserialize_with = "display_credits")]
    albums: Vec<AlbumCreditWire>,
    #[serde(default, deserialize_with = "display_duration")]
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
    // Upstream compact album credits may omit their independent catalog ID.
    // A standalone album still requires it during normalization below.
    id: Option<Id>,
    #[serde(default, deserialize_with = "display_text")]
    title: Option<String>,
    #[serde(default, deserialize_with = "display_credits")]
    artists: Vec<ArtistCreditWire>,
    volumes: Option<Vec<Vec<TrackWire>>>,
    #[serde(default, deserialize_with = "cover_template")]
    cover_uri: Option<crate::artwork::CoverArt>,
}

// Display-only nested credits are not standalone navigable entities. MarshalX
// 9ca132e5 base.py::de_list accepts null/non-list containers and skips non-object
// members, unlike the previous Vec<ArtistWire>/Vec<AlbumWire> coupling. Keep the
// required parent track ID strict and reject duplicate recognized credit keys.
#[derive(Deserialize)]
struct ArtistCreditWire {
    #[serde(default, deserialize_with = "display_text")]
    name: Option<String>,
    #[serde(default, deserialize_with = "display_album_id")]
    id: Option<Id>,
    #[serde(default, rename = "cover", deserialize_with = "ignored_display_value")]
    _cover: (),
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlbumCreditWire {
    #[serde(default, deserialize_with = "display_album_id")]
    id: Option<Id>,
    #[serde(default, deserialize_with = "display_text")]
    title: Option<String>,
    #[serde(default, deserialize_with = "cover_template")]
    cover_uri: Option<crate::artwork::CoverArt>,
    #[serde(
        default,
        rename = "artists",
        deserialize_with = "ignored_display_value"
    )]
    _artists: (),
    #[serde(
        default,
        rename = "volumes",
        deserialize_with = "ignored_display_value"
    )]
    _volumes: (),
}
fn ignored_display_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<(), D::Error> {
    let _ = Box::<RawValue>::deserialize(deserializer)?;
    Ok(())
}
fn display_text<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    let value = Box::<RawValue>::deserialize(deserializer)?;
    Ok(serde_json::from_str::<Option<String>>(value.get())
        .ok()
        .flatten()
        .and_then(|value| text(value).ok()))
}
fn display_duration<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    let value = Box::<RawValue>::deserialize(deserializer)?;
    Ok(serde_json::from_str::<Option<u64>>(value.get())
        .ok()
        .flatten())
}
fn display_album_id<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Id>, D::Error> {
    let value = Box::<RawValue>::deserialize(deserializer)?;
    Ok(serde_json::from_str::<Option<Id>>(value.get())
        .ok()
        .flatten())
}
fn display_credits<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned,
{
    let value = Box::<RawValue>::deserialize(deserializer)?;
    if !value.get().trim_start().starts_with('[') {
        return Ok(Vec::new());
    }
    let values = serde_json::from_str::<Vec<Box<RawValue>>>(value.get())
        .map_err(|_| <D::Error as serde::de::Error>::custom("unsupported display-credit list"))?;
    if values.len() > MAX_COLLECTION {
        return Err(<D::Error as serde::de::Error>::custom(
            "display-credit list exceeds bound",
        ));
    }
    values
        .into_iter()
        .filter(|value| value.get().trim_start().starts_with('{'))
        .map(|value| {
            serde_json::from_str(value.get()).map_err(|_| {
                <D::Error as serde::de::Error>::custom("ambiguous display-credit record")
            })
        })
        .collect()
}
#[derive(Deserialize)]
struct SearchWire {
    text: Option<Box<RawValue>>,
    // Decode only the requested result category. RawValue preserves duplicate
    // known keys for the next typed stage, unlike an intermediate Value tree.
    tracks: Option<Box<RawValue>>,
    albums: Option<Box<RawValue>>,
    artists: Option<Box<RawValue>>,
    playlists: Option<Box<RawValue>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchSectionWire {
    total: Option<Box<RawValue>>,
    per_page: Option<Box<RawValue>>,
    #[serde(default)]
    results: Vec<Box<RawValue>>,
}
struct SearchPageWire<T> {
    total: Option<u64>,
    per_page: Option<u64>,
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
fn search_playlist_from_wire(
    identity: SearchPlaylistIdentity,
    display: SearchPlaylistDisplay,
) -> Result<SearchPlaylist, MusicError> {
    let nested_owner = match identity.owner {
        Some(owner) => {
            if !owner.get().trim_start().starts_with('{') {
                return Err(MusicError::InvalidResponse);
            }
            serde_json::from_str::<OwnerWire>(owner.get())
                .map_err(|_| MusicError::InvalidResponse)?
                .uid
        }
        None => None,
    };
    if let (Some(nested), Some(direct)) = (&nested_owner, &identity.uid)
        && nested.0 != direct.0
    {
        return Err(MusicError::InvalidResponse);
    }
    let owner = nested_owner
        .or(identity.uid)
        .ok_or(MusicError::InvalidResponse)?;
    Ok(SearchPlaylist {
        id: PlaylistId {
            owner: owner.0,
            kind: identity.kind.0,
        },
        title: display.title.unwrap_or_else(|| "Без названия".into()),
        track_count: display.track_count,
        cover: display.cover,
    })
}
fn artist_names(wire: Vec<ArtistCreditWire>) -> Result<Vec<String>, MusicError> {
    // Embedded credits may have a name without an independently browsable ID.
    // Missing credits are omitted; they do not invalidate an otherwise real
    // track/album. Standalone artist results remain strictly identified above.
    wire.into_iter()
        .filter_map(|artist| artist.name)
        .map(text)
        .collect()
}
fn track_from_wire(wire: TrackWire) -> Result<Track, MusicError> {
    let mut artists = Vec::new();
    let mut artist_links = Vec::new();
    for credit in wire.artists {
        if let Some(name) = credit.name {
            let name = text(name)?;
            artists.push(name.clone());
            if let Some(id) = credit.id {
                artist_links.push(Artist {
                    id: id.0,
                    name,
                    cover: None,
                });
            }
        }
    }
    let album = wire
        .albums
        .into_iter()
        .find(|album| album.id.is_some() || album.title.is_some() || album.cover_uri.is_some());
    let cover = wire
        .cover_uri
        .or_else(|| album.as_ref().and_then(|album| album.cover_uri.clone()));
    Ok(Track {
        id: wire.id.0,
        title: text(wire.title.unwrap_or_else(|| "Без названия".into()))?,
        artists,
        artist_links,
        album_id: album
            .as_ref()
            .and_then(|album| album.id.as_ref().map(|id| id.0.clone())),
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
        id: wire.id.ok_or(MusicError::InvalidResponse)?.0,
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
