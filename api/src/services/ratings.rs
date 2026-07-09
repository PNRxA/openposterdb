use crate::error::AppError;
use crate::id::{MediaType, ResolvedId};
use crate::services::mdblist::{MdblistClient, MdblistResponse};
use crate::services::omdb::OmdbClient;
use crate::services::tmdb::TmdbClient;
use crate::services::trakt::TraktClient;
use image::Rgba;
use serde::Deserialize;

/// Threshold (ms) above which ratings fetches are logged as slow.
const SLOW_RATINGS_MS: u64 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatingSource {
    Imdb,
    Tmdb,
    Rt,
    RtAudience,
    Metacritic,
    Trakt,
    Letterboxd,
    Mal,
    Mdblist,
    Ebert,
}

impl RatingSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Imdb => "IMDb",
            Self::Tmdb => "TMDB",
            Self::Rt => "RTC",
            Self::RtAudience => "RTA",
            Self::Metacritic => "MC",
            Self::Trakt => "Trakt",
            Self::Letterboxd => "LB",
            Self::Mal => "MAL",
            Self::Mdblist => "MDB",
            Self::Ebert => "Ebert",
        }
    }

    /// Single-char identifier used in cache key suffixes.
    pub fn cache_char(&self) -> char {
        match self {
            Self::Mal => 'm',
            Self::Imdb => 'i',
            Self::Letterboxd => 'l',
            Self::Rt => 'r',
            Self::RtAudience => 'a',
            Self::Metacritic => 'c',
            Self::Tmdb => 't',
            Self::Trakt => 'k',
            Self::Mdblist => 'd',
            Self::Ebert => 'e',
        }
    }

    /// Reverse of `cache_char`.
    pub fn from_cache_char(c: char) -> Option<Self> {
        match c {
            'm' => Some(Self::Mal),
            'i' => Some(Self::Imdb),
            'l' => Some(Self::Letterboxd),
            'r' => Some(Self::Rt),
            'a' => Some(Self::RtAudience),
            'c' => Some(Self::Metacritic),
            't' => Some(Self::Tmdb),
            'k' => Some(Self::Trakt),
            'd' => Some(Self::Mdblist),
            'e' => Some(Self::Ebert),
            _ => None,
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            Self::Imdb => "imdb",
            Self::Tmdb => "tmdb",
            Self::Rt => "rt",
            Self::RtAudience => "rta",
            Self::Metacritic => "mc",
            Self::Trakt => "trakt",
            Self::Letterboxd => "lb",
            Self::Mal => "mal",
            Self::Mdblist => "mdblist",
            Self::Ebert => "ebert",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "imdb" => Some(Self::Imdb),
            "tmdb" => Some(Self::Tmdb),
            "rt" => Some(Self::Rt),
            "rta" => Some(Self::RtAudience),
            "mc" => Some(Self::Metacritic),
            "trakt" => Some(Self::Trakt),
            "lb" => Some(Self::Letterboxd),
            "mal" => Some(Self::Mal),
            "mdblist" => Some(Self::Mdblist),
            "ebert" => Some(Self::Ebert),
            _ => None,
        }
    }

    pub fn all_keys() -> Vec<&'static str> {
        vec!["imdb", "tmdb", "rt", "rta", "mc", "trakt", "lb", "mal", "mdblist", "ebert"]
    }

    pub fn color(&self) -> Rgba<u8> {
        match self {
            Self::Imdb => Rgba([180, 145, 15, 255]),       // gold
            Self::Tmdb => Rgba([1, 155, 88, 255]),         // green
            Self::Rt => Rgba([185, 35, 8, 255]),           // red
            Self::RtAudience => Rgba([185, 35, 8, 255]),   // same RT red
            Self::Metacritic => Rgba([75, 150, 38, 255]),  // metacritic green
            Self::Trakt => Rgba([175, 15, 45, 255]),       // trakt red
            Self::Letterboxd => Rgba([0, 155, 88, 255]),   // letterboxd green
            Self::Mal => Rgba([34, 60, 120, 255]),         // MAL blue
            Self::Mdblist => Rgba([66, 132, 202, 255]),    // mdblist blue (#4284CA)
            Self::Ebert => Rgba([232, 89, 12, 255]),       // Roger Ebert orange
        }
    }
}

#[derive(Debug, Clone)]
pub struct RatingBadge {
    pub source: RatingSource,
    pub value: String,
}

#[derive(Debug, Clone, Default)]
pub struct RatingsResult {
    pub badges: Vec<RatingBadge>,
    pub tmdb_id: Option<u64>,
    pub tvdb_id: Option<u64>,
    pub imdb_id: Option<String>,
}

pub async fn fetch_ratings(
    resolved: &ResolvedId,
    tmdb: &TmdbClient,
    omdb: Option<&OmdbClient>,
    mdblist: Option<&MdblistClient>,
    trakt: Option<&TraktClient>,
    cache: &moka::future::Cache<String, RatingsResult>,
) -> Result<RatingsResult, AppError> {
    let key = match resolved.media_type {
        MediaType::Movie => format!("{}/movie", resolved.tmdb_id),
        MediaType::Tv => format!("{}/tv", resolved.tmdb_id),
        MediaType::Episode => {
            let ep = resolved.episode.as_ref().ok_or_else(|| {
                AppError::Other(format!(
                    "episode media type but no EpisodeInfo for tmdb_id={}",
                    resolved.tmdb_id
                ))
            })?;
            format!("{}/episode/S{}E{}", ep.show_tmdb_id, ep.season_number, ep.episode_number)
        }
    };

    let resolved = resolved.clone();
    let tmdb = tmdb.clone();
    let omdb = omdb.cloned();
    let mdblist = mdblist.cloned();
    let trakt = trakt.cloned();

    let coalesced = cache
        .try_get_with(key, async move {
            let result =
                fetch_ratings_inner(&resolved, &tmdb, omdb.as_ref(), mdblist.as_ref(), trakt.as_ref()).await;
            Ok::<_, std::convert::Infallible>(result)
        })
        .await
        .unwrap_or_default();

    Ok(coalesced)
}

async fn fetch_ratings_inner(
    resolved: &ResolvedId,
    tmdb: &TmdbClient,
    omdb: Option<&OmdbClient>,
    mdblist: Option<&MdblistClient>,
    trakt: Option<&TraktClient>,
) -> RatingsResult {
    let ratings_start = std::time::Instant::now();

    let tmdb_fut = async {
        let start = std::time::Instant::now();
        let result = fetch_tmdb_rating(resolved, tmdb).await;
        (result, start.elapsed())
    };
    let omdb_fut = async {
        let start = std::time::Instant::now();
        let result = fetch_omdb_ratings(resolved.imdb_id.as_deref(), omdb).await;
        (result, start.elapsed())
    };
    let mdblist_fut = async {
        let start = std::time::Instant::now();
        let result = fetch_mdblist_ratings(resolved, mdblist).await;
        (result, start.elapsed())
    };
    let trakt_fut = async {
        let start = std::time::Instant::now();
        let result = fetch_trakt_rating(resolved, tmdb, trakt).await;
        (result, start.elapsed())
    };

    let (
        (tmdb_badges, tmdb_dur),
        (omdb_badges, omdb_dur),
        (mdblist_raw, mdblist_dur),
        (trakt_badge, trakt_dur),
    ) = tokio::join!(tmdb_fut, omdb_fut, mdblist_fut, trakt_fut);

    let ratings_elapsed = ratings_start.elapsed().as_millis() as u64;
    if ratings_elapsed > SLOW_RATINGS_MS {
        tracing::warn!(
            tmdb_id = resolved.tmdb_id,
            imdb_id = ?resolved.imdb_id,
            total_ms = ratings_elapsed,
            tmdb_ms = tmdb_dur.as_millis() as u64,
            omdb_ms = omdb_dur.as_millis() as u64,
            mdblist_ms = mdblist_dur.as_millis() as u64,
            trakt_ms = trakt_dur.as_millis() as u64,
            "slow ratings fetch"
        );
    }

    let (mdblist_badges, mdb_tmdb_id, mdb_tvdb_id, mdb_imdb_id) = match mdblist_raw {
        Some((badges, tmdb_id, tvdb_id, imdb_id)) => (Some(badges), tmdb_id, tvdb_id, imdb_id),
        None => (None, None, None, None),
    };

    let find_omdb = |src: RatingSource| -> Option<RatingBadge> {
        omdb_badges
            .as_ref()?
            .iter()
            .find(|b| b.source == src)
            .cloned()
    };
    let find_mdb = |src: RatingSource| -> Option<RatingBadge> {
        mdblist_badges
            .as_ref()?
            .iter()
            .find(|b| b.source == src)
            .cloned()
    };

    // Badge order: IMDb, TMDB, RT, RT Audience, MC, Trakt, Letterboxd, MAL, MDBList, Ebert
    // MDBList preferred for overlapping sources; OMDb and direct Trakt fill gaps.
    // MDBList's own aggregate score and Roger Ebert come only from MDBList.
    let ordered: Vec<Option<RatingBadge>> = vec![
        find_mdb(RatingSource::Imdb).or_else(|| find_omdb(RatingSource::Imdb)),
        tmdb_badges,
        find_mdb(RatingSource::Rt).or_else(|| find_omdb(RatingSource::Rt)),
        find_mdb(RatingSource::RtAudience),
        find_mdb(RatingSource::Metacritic).or_else(|| find_omdb(RatingSource::Metacritic)),
        find_mdb(RatingSource::Trakt).or(trakt_badge),
        find_mdb(RatingSource::Letterboxd),
        find_mdb(RatingSource::Mal),
        find_mdb(RatingSource::Mdblist),
        find_mdb(RatingSource::Ebert),
    ];

    RatingsResult {
        badges: ordered.into_iter().flatten().collect(),
        tmdb_id: mdb_tmdb_id,
        tvdb_id: mdb_tvdb_id,
        imdb_id: mdb_imdb_id,
    }
}

async fn fetch_tmdb_rating(resolved: &ResolvedId, tmdb: &TmdbClient) -> Option<RatingBadge> {
    #[derive(Deserialize)]
    struct Details {
        vote_average: Option<f64>,
    }

    let path = match resolved.media_type {
        MediaType::Movie => format!("/movie/{}", resolved.tmdb_id),
        MediaType::Tv => format!("/tv/{}", resolved.tmdb_id),
        MediaType::Episode => {
            let ep = resolved.episode.as_ref()?;
            format!("/tv/{}/season/{}/episode/{}", ep.show_tmdb_id, ep.season_number, ep.episode_number)
        }
    };

    let details: Details = match tmdb.get(&path, &[]).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(tmdb_id = resolved.tmdb_id, "tmdb rating fetch failed: {e}");
            return None;
        }
    };
    let score = details.vote_average?;
    if score <= 0.0 {
        return None;
    }

    Some(RatingBadge {
        source: RatingSource::Tmdb,
        value: format!("{:.0}%", score * 10.0),
    })
}

async fn fetch_omdb_ratings(imdb_id: Option<&str>, omdb: Option<&OmdbClient>) -> Option<Vec<RatingBadge>> {
    let client = omdb?;
    let imdb_id = imdb_id?;
    let resp = match client.get_ratings(imdb_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(imdb_id, "omdb rating fetch failed: {e}");
            return None;
        }
    };
    let mut badges = Vec::new();

    // IMDb rating
    if let Some(ref rating) = resp.imdb_rating
        && rating != "N/A"
    {
        badges.push(RatingBadge {
            source: RatingSource::Imdb,
            value: rating.clone(),
        });
    }

    // Rotten Tomatoes from Ratings array
    for r in &resp.ratings {
        if r.source == "Rotten Tomatoes" && r.value != "N/A" {
            badges.push(RatingBadge {
                source: RatingSource::Rt,
                value: r.value.clone(),
            });
        }
    }

    // Metacritic
    if let Some(ref mc) = resp.metascore
        && mc != "N/A"
    {
        badges.push(RatingBadge {
            source: RatingSource::Metacritic,
            value: mc.clone(),
        });
    }

    Some(badges)
}

async fn fetch_trakt_rating(
    resolved: &ResolvedId,
    tmdb: &TmdbClient,
    trakt: Option<&TraktClient>,
) -> Option<RatingBadge> {
    let client = trakt?;

    let resp = match resolved.media_type {
        MediaType::Movie => {
            let imdb_id = resolved.imdb_id.as_deref()?;
            client.get_movie_rating(imdb_id).await
        }
        MediaType::Tv => {
            let imdb_id = resolved.imdb_id.as_deref()?;
            client.get_show_rating(imdb_id).await
        }
        MediaType::Episode => {
            let ep = resolved.episode.as_ref()?;
            // Trakt's episode ratings endpoint is keyed by the show's IMDb ID.
            // Resolve it lazily here (only when Trakt is configured) so that
            // non-Trakt deployments never pay for the extra TMDB lookup.
            let show_imdb_id = fetch_show_imdb_id(tmdb, ep.show_tmdb_id).await?;
            client
                .get_episode_rating(&show_imdb_id, ep.season_number, ep.episode_number)
                .await
        }
    };

    let resp = match resp {
        Ok(Some(r)) => r,
        // 404 — the title/episode isn't on Trakt. Expected; no rating to show.
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(
                tmdb_id = resolved.tmdb_id,
                imdb_id = ?resolved.imdb_id,
                media_type = ?resolved.media_type,
                "trakt rating fetch failed: {e}"
            );
            return None;
        }
    };

    trakt_badge(resp.rating, resp.votes)
}

/// Build a Trakt rating badge from a raw 0–10 rating and its vote count.
///
/// Pure (no I/O) so the percent formatting (`rating * 10`) and the
/// no-rating/no-votes suppression can be unit-tested without a live Trakt
/// client. Returns `None` when there's nothing meaningful to show.
fn trakt_badge(rating: f64, votes: u64) -> Option<RatingBadge> {
    if rating <= 0.0 || votes == 0 {
        return None;
    }
    Some(RatingBadge {
        source: RatingSource::Trakt,
        value: format!("{:.0}%", rating * 10.0),
    })
}

/// Look up a show's IMDb ID from its TMDB ID via TMDB's `external_ids` endpoint.
/// Used only on the episode Trakt path, which keys ratings by show IMDb ID.
async fn fetch_show_imdb_id(tmdb: &TmdbClient, show_tmdb_id: u64) -> Option<String> {
    #[derive(Deserialize)]
    struct ShowExternalIds {
        imdb_id: Option<String>,
    }

    let ids: ShowExternalIds = match tmdb
        .get(&format!("/tv/{show_tmdb_id}/external_ids"), &[])
        .await
    {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(show_tmdb_id, "trakt: show external_ids fetch failed: {e}");
            return None;
        }
    };

    ids.imdb_id
}

/// Build a cache key suffix from actual rendered badges (post-filtering).
///
/// Unlike `ratings_cache_suffix` which predicts from user settings, this reflects
/// which sources actually have data for a given movie.
pub fn badges_cache_suffix(badges: &[RatingBadge]) -> String {
    let chars: String = badges.iter().map(|b| b.source.cache_char()).collect();
    format!("@{chars}")
}

/// Encode which rating sources have data for a movie as a compact string of
/// cache chars in canonical order (e.g. `"ilrt"`). Stored in SQLite so the hot
/// path can reconstruct the badges cache suffix without calling external APIs.
///
/// Canonical ordering ensures the stored value is deterministic regardless of
/// the order ratings arrive from upstream APIs.
pub fn available_sources_string(badges: &[RatingBadge]) -> String {
    let mut sources: Vec<RatingSource> = badges.iter().map(|b| b.source).collect();
    sources.sort_by_key(|s| {
        CANONICAL_ORDER.iter().position(|&k| RatingSource::from_key(k) == Some(*s)).unwrap_or(usize::MAX)
    });
    sources.dedup();
    sources.iter().map(|s| s.cache_char()).collect()
}

/// Canonical, order-independent cache token for an exclude set (e.g. `"k"` for
/// `"trakt"`, `"rt"` for both `"rt,tmdb"` and `"tmdb,rt"`).
///
/// `ratings_cache_suffix` can collapse two different exclude sets to the same
/// predicted suffix when an excluded source falls *beyond* the rating limit in
/// the canonical-padded prediction. The CDN `settings_hash` must still tell
/// those configs apart — they render differently for titles with partial source
/// availability — so it hashes this token alongside the predicted suffix.
/// Canonicalising (sort + dedup) keeps semantically-equal excludes hashing the
/// same, so it introduces no false cache misses.
pub fn exclude_cache_token(exclude: &str) -> String {
    let mut sources = parse_order(exclude);
    sources.sort_by_key(|s| {
        CANONICAL_ORDER.iter().position(|&k| RatingSource::from_key(k) == Some(*s)).unwrap_or(usize::MAX)
    });
    sources.dedup();
    sources.iter().map(|s| s.cache_char()).collect()
}

/// Canonical order of all rating sources, used for deterministic cache keys.
const CANONICAL_ORDER: &[&str] = &["mal", "imdb", "lb", "rt", "rta", "mc", "tmdb", "trakt", "mdblist", "ebert"];

/// Parse a comma-separated order string into a vec of known `RatingSource`s.
fn parse_order(order: &str) -> Vec<RatingSource> {
    order
        .split(',')
        .map(|k| k.trim())
        .filter_map(RatingSource::from_key)
        .collect()
}

/// Reorder `sources` according to `order`, appending any sources not mentioned
/// in `order` in their original order, then truncate to `limit` (0 = no ratings).
///
/// Sources named in `exclude` (a comma-separated list of rating source keys) are
/// dropped up front — *before* ordering and limiting — so that excluding a source
/// you don't care about frees its badge slot for the next preferred source rather
/// than just leaving a gap.
fn order_and_limit(sources: Vec<RatingSource>, order: &str, exclude: &str, limit: i32) -> Vec<RatingSource> {
    if limit == 0 {
        return Vec::new();
    }

    // Drop excluded sources first so the remaining preferred sources can fill
    // the available slots up to `limit`.
    let excluded = parse_order(exclude);
    let sources: Vec<RatingSource> = if excluded.is_empty() {
        sources
    } else {
        sources.into_iter().filter(|s| !excluded.contains(s)).collect()
    };

    let mut result = if order.is_empty() {
        sources
    } else {
        let preferred = parse_order(order);
        let mut ordered = Vec::with_capacity(sources.len());
        for src in &preferred {
            if sources.contains(src) {
                ordered.push(*src);
            }
        }
        for src in &sources {
            if !preferred.contains(src) {
                ordered.push(*src);
            }
        }
        ordered
    };

    debug_assert!(limit > 0, "negative limit is not supported");
    result.truncate(limit as usize);

    result
}

/// Reconstruct a badges cache suffix from a stored available-sources string
/// and user preferences, without needing actual badge values.
///
/// Uses the same ordering logic as `apply_rating_preferences` +
/// `badges_cache_suffix` but operates on source chars instead of full
/// `RatingBadge` structs.
pub fn badges_suffix_from_available(available_sources: &str, order: &str, exclude: &str, limit: i32) -> String {
    let available: Vec<RatingSource> = available_sources
        .chars()
        .filter_map(RatingSource::from_cache_char)
        .collect();

    let ordered = order_and_limit(available, order, exclude, limit);

    let chars: String = ordered.iter().map(|s| s.cache_char()).collect();
    format!("@{chars}")
}

/// Compute a deterministic cache key suffix from rating preferences.
///
/// Parses `order` into known `RatingSource` keys, appends any missing sources
/// in canonical order for determinism, drops any sources named in `exclude`,
/// then truncates to `limit` if positive. Returns a compact string like `@mil`
/// (single-char per source, no commas).
///
/// Excluded sources MUST be folded in here (and removed before the `limit`
/// truncation, matching `order_and_limit`) so that two configurations differing
/// only in `exclude` produce different cache keys and never collide.
pub fn ratings_cache_suffix(order: &str, exclude: &str, limit: i32) -> String {
    if limit == 0 {
        return "@".to_string();
    }

    let mut sources = parse_order(order);

    // Append missing sources in canonical order
    for &canonical in CANONICAL_ORDER {
        if let Some(src) = RatingSource::from_key(canonical) {
            if !sources.contains(&src) {
                sources.push(src);
            }
        }
    }

    // Drop excluded sources before truncating so this predicted suffix matches
    // the badges produced by `order_and_limit` (which excludes before limiting).
    let excluded = parse_order(exclude);
    if !excluded.is_empty() {
        sources.retain(|s| !excluded.contains(s));
    }

    debug_assert!(limit > 0, "negative limit is not supported");
    sources.truncate(limit as usize);

    let chars: String = sources.iter().map(|s| s.cache_char()).collect();
    format!("@{chars}")
}

/// Reorder and/or limit rating badges based on user preferences.
///
/// - If `order` is non-empty, badges are reordered to match the specified order.
///   Unmentioned sources are appended after in their original order.
/// - Sources named in `exclude` (comma-separated keys) are dropped before
///   ordering and limiting.
/// - If `limit` is 0, an empty list is returned (no ratings).
/// - If `limit` > 0, the result is truncated to that many badges.
pub fn apply_rating_preferences(badges: Vec<RatingBadge>, order: &str, exclude: &str, limit: i32) -> Vec<RatingBadge> {
    let sources: Vec<RatingSource> = badges.iter().map(|b| b.source).collect();
    let ordered_sources = order_and_limit(sources, order, exclude, limit);

    let mut result = Vec::with_capacity(ordered_sources.len());
    for src in &ordered_sources {
        if let Some(badge) = badges.iter().find(|b| b.source == *src) {
            result.push(badge.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rating_source_labels() {
        assert_eq!(RatingSource::Imdb.label(), "IMDb");
        assert_eq!(RatingSource::Tmdb.label(), "TMDB");
        assert_eq!(RatingSource::Rt.label(), "RTC");
        assert_eq!(RatingSource::RtAudience.label(), "RTA");
        assert_eq!(RatingSource::Metacritic.label(), "MC");
        assert_eq!(RatingSource::Trakt.label(), "Trakt");
        assert_eq!(RatingSource::Letterboxd.label(), "LB");
        assert_eq!(RatingSource::Mal.label(), "MAL");
        assert_eq!(RatingSource::Mdblist.label(), "MDB");
        assert_eq!(RatingSource::Ebert.label(), "Ebert");
    }

    #[test]
    fn rating_source_colors_unique_per_source() {
        assert_eq!(RatingSource::Imdb.color(), Rgba([180, 145, 15, 255]));
        assert_eq!(RatingSource::Tmdb.color(), Rgba([1, 155, 88, 255]));
        assert_eq!(RatingSource::Rt.color(), Rgba([185, 35, 8, 255]));
        assert_eq!(RatingSource::Metacritic.color(), Rgba([75, 150, 38, 255]));
        assert_eq!(RatingSource::Trakt.color(), Rgba([175, 15, 45, 255]));
        assert_eq!(RatingSource::Letterboxd.color(), Rgba([0, 155, 88, 255]));
        assert_eq!(RatingSource::Mal.color(), Rgba([34, 60, 120, 255]));
        assert_eq!(RatingSource::Mdblist.color(), Rgba([66, 132, 202, 255]));
        assert_eq!(RatingSource::Ebert.color(), Rgba([232, 89, 12, 255]));
    }

    #[test]
    fn rating_source_key_roundtrip() {
        let sources = [
            RatingSource::Imdb,
            RatingSource::Tmdb,
            RatingSource::Rt,
            RatingSource::RtAudience,
            RatingSource::Metacritic,
            RatingSource::Trakt,
            RatingSource::Letterboxd,
            RatingSource::Mal,
            RatingSource::Mdblist,
            RatingSource::Ebert,
        ];
        for src in sources {
            assert_eq!(RatingSource::from_key(src.key()), Some(src));
        }
    }

    #[test]
    fn from_key_unknown_returns_none() {
        assert_eq!(RatingSource::from_key("unknown"), None);
    }

    #[test]
    fn apply_rating_preferences_reorder() {
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.0".into() },
            RatingBadge { source: RatingSource::Tmdb, value: "75%".into() },
            RatingBadge { source: RatingSource::Trakt, value: "80%".into() },
        ];
        let result = apply_rating_preferences(badges, "trakt,imdb", "", 8);
        assert_eq!(result[0].source, RatingSource::Trakt);
        assert_eq!(result[1].source, RatingSource::Imdb);
        assert_eq!(result[2].source, RatingSource::Tmdb);
    }

    #[test]
    fn apply_rating_preferences_limit() {
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.0".into() },
            RatingBadge { source: RatingSource::Tmdb, value: "75%".into() },
            RatingBadge { source: RatingSource::Trakt, value: "80%".into() },
        ];
        let result = apply_rating_preferences(badges, "", "", 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].source, RatingSource::Imdb);
        assert_eq!(result[1].source, RatingSource::Tmdb);
    }

    #[test]
    fn apply_rating_preferences_reorder_and_limit() {
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.0".into() },
            RatingBadge { source: RatingSource::Tmdb, value: "75%".into() },
            RatingBadge { source: RatingSource::Mal, value: "8.50".into() },
            RatingBadge { source: RatingSource::Trakt, value: "80%".into() },
        ];
        let result = apply_rating_preferences(badges, "mal,imdb,rta,trakt", "", 3);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].source, RatingSource::Mal);
        assert_eq!(result[1].source, RatingSource::Imdb);
        assert_eq!(result[2].source, RatingSource::Trakt);
    }

    #[test]
    fn apply_rating_preferences_empty_order_zero_limit() {
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.0".into() },
            RatingBadge { source: RatingSource::Tmdb, value: "75%".into() },
        ];
        let result = apply_rating_preferences(badges.clone(), "", "", 0);
        assert_eq!(result.len(), 0, "limit=0 should return no ratings");
    }

    #[test]
    fn apply_rating_preferences_excludes_source() {
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.0".into() },
            RatingBadge { source: RatingSource::Rt, value: "95%".into() },
            RatingBadge { source: RatingSource::Tmdb, value: "75%".into() },
        ];
        // Exclude RT critics — the user doesn't care about it.
        let result = apply_rating_preferences(badges, "imdb,rt,tmdb", "rt", 8);
        let sources: Vec<RatingSource> = result.iter().map(|b| b.source).collect();
        assert_eq!(sources, vec![RatingSource::Imdb, RatingSource::Tmdb]);
    }

    #[test]
    fn apply_rating_preferences_exclude_frees_limit_slot() {
        // exclude is applied BEFORE the limit, so dropping RT lets TMDB take its
        // slot rather than leaving the user with fewer badges than `limit`.
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.0".into() },
            RatingBadge { source: RatingSource::Rt, value: "95%".into() },
            RatingBadge { source: RatingSource::Tmdb, value: "75%".into() },
        ];
        let result = apply_rating_preferences(badges, "imdb,rt,tmdb", "rt", 2);
        let sources: Vec<RatingSource> = result.iter().map(|b| b.source).collect();
        assert_eq!(sources, vec![RatingSource::Imdb, RatingSource::Tmdb]);
    }

    #[test]
    fn apply_rating_preferences_exclude_multiple() {
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.0".into() },
            RatingBadge { source: RatingSource::Rt, value: "95%".into() },
            RatingBadge { source: RatingSource::Tmdb, value: "75%".into() },
            RatingBadge { source: RatingSource::Mal, value: "8.50".into() },
        ];
        let result = apply_rating_preferences(badges, "", "rt,tmdb", 8);
        let sources: Vec<RatingSource> = result.iter().map(|b| b.source).collect();
        assert_eq!(sources, vec![RatingSource::Imdb, RatingSource::Mal]);
    }

    #[test]
    fn ratings_cache_suffix_exclude_changes_suffix() {
        // The cache-collision guard: same order+limit but different exclusions
        // MUST yield different suffixes, or cached images would collide.
        let none = ratings_cache_suffix("imdb,tmdb,rt", "", 3);
        let excl = ratings_cache_suffix("imdb,tmdb,rt", "rt", 3);
        assert_ne!(none, excl, "excluding a source must change the cache suffix");
        assert_eq!(none, "@itr");
        // rt dropped before truncate; the next canonical source fills the slot.
        assert_eq!(excl, "@itm");
    }

    #[test]
    fn ratings_cache_suffix_exclude_matches_apply_pipeline() {
        // The predicted suffix must equal the suffix of the actually-rendered
        // badges for the same order/exclude/limit.
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.0".into() },
            RatingBadge { source: RatingSource::Rt, value: "95%".into() },
            RatingBadge { source: RatingSource::Tmdb, value: "75%".into() },
        ];
        let available = available_sources_string(&badges); // "irt"
        let filtered = apply_rating_preferences(badges, "imdb,rt,tmdb", "rt", 3);
        let expected = badges_cache_suffix(&filtered);
        let from_available = badges_suffix_from_available(&available, "imdb,rt,tmdb", "rt", 3);
        assert_eq!(from_available, expected);
    }

    #[test]
    fn exclude_cache_token_canonical_and_order_independent() {
        assert_eq!(exclude_cache_token(""), "");
        assert_eq!(exclude_cache_token("trakt"), "k");
        // Order-independent + deduped to canonical source order (rt before tmdb).
        assert_eq!(exclude_cache_token("rt,tmdb"), exclude_cache_token("tmdb,rt"));
        assert_eq!(exclude_cache_token("tmdb,rt"), "rt");
        // Different exclude sets yield different tokens.
        assert_ne!(exclude_cache_token("rt"), exclude_cache_token("trakt"));
        // Unknown keys are ignored.
        assert_eq!(exclude_cache_token("bogus"), "");
    }

    #[test]
    fn cache_char_unique_per_source() {
        let sources = [
            (RatingSource::Mal, 'm'),
            (RatingSource::Imdb, 'i'),
            (RatingSource::Letterboxd, 'l'),
            (RatingSource::Rt, 'r'),
            (RatingSource::RtAudience, 'a'),
            (RatingSource::Metacritic, 'c'),
            (RatingSource::Tmdb, 't'),
            (RatingSource::Trakt, 'k'),
            (RatingSource::Mdblist, 'd'),
            (RatingSource::Ebert, 'e'),
        ];
        for (src, expected) in &sources {
            assert_eq!(src.cache_char(), *expected, "cache_char mismatch for {:?}", src);
        }
        // All chars must be unique
        let chars: Vec<char> = sources.iter().map(|(s, _)| s.cache_char()).collect();
        let mut deduped = chars.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(chars.len(), deduped.len(), "cache_char values are not unique");
    }

    #[test]
    fn ratings_cache_suffix_default_order_limit_3() {
        let suffix = ratings_cache_suffix("mal,imdb,lb,rt,rta,mc,tmdb,trakt", "", 3);
        assert_eq!(suffix, "@mil");
    }

    #[test]
    fn ratings_cache_suffix_custom_order() {
        let suffix = ratings_cache_suffix("trakt,imdb,rt", "", 3);
        assert_eq!(suffix, "@kir");
    }

    #[test]
    fn ratings_cache_suffix_partial_order_normalized() {
        // Only two sources specified — missing ones appended in canonical order
        let suffix = ratings_cache_suffix("imdb,rt", "", 8);
        assert_eq!(suffix, "@irmlactk");
    }

    #[test]
    fn ratings_cache_suffix_includes_all_ten_sources() {
        // With limit 10 every source — including mdblist (d) and ebert (e) — fits.
        // Order: imdb,rt then canonical-appended mal,lb,rta,mc,tmdb,trakt,mdblist,ebert.
        let suffix = ratings_cache_suffix("imdb,rt", "", 10);
        assert_eq!(suffix, "@irmlactkde");
    }

    #[test]
    fn ratings_cache_suffix_mdblist_and_ebert_ordering() {
        // New sources are addressable by key and ordered like any other.
        let suffix = ratings_cache_suffix("mdblist,ebert,imdb", "", 3);
        assert_eq!(suffix, "@dei");
    }

    #[test]
    fn ratings_cache_suffix_limit_zero_shows_none() {
        let suffix = ratings_cache_suffix("mal,imdb,lb,rt,rta,mc,tmdb,trakt", "", 0);
        assert_eq!(suffix, "@");
    }

    #[test]
    fn ratings_cache_suffix_empty_order() {
        let suffix = ratings_cache_suffix("", "", 3);
        assert_eq!(suffix, "@mil");
    }

    #[test]
    fn ratings_cache_suffix_invalid_sources_ignored() {
        let suffix = ratings_cache_suffix("imdb,bogus,rt,fake", "", 3);
        assert_eq!(suffix, "@irm");
    }

    #[test]
    fn badges_cache_suffix_from_actual_badges() {
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.0".into() },
            RatingBadge { source: RatingSource::Letterboxd, value: "4.2".into() },
            RatingBadge { source: RatingSource::Rt, value: "95%".into() },
        ];
        assert_eq!(badges_cache_suffix(&badges), "@ilr");
    }

    #[test]
    fn badges_cache_suffix_empty() {
        let badges: Vec<RatingBadge> = vec![];
        assert_eq!(badges_cache_suffix(&badges), "@");
    }

    #[test]
    fn badges_cache_suffix_single() {
        let badges = vec![
            RatingBadge { source: RatingSource::Mal, value: "8.50".into() },
        ];
        assert_eq!(badges_cache_suffix(&badges), "@m");
    }

    #[test]
    fn from_cache_char_roundtrip() {
        for src in [
            RatingSource::Mal, RatingSource::Imdb, RatingSource::Letterboxd,
            RatingSource::Rt, RatingSource::RtAudience, RatingSource::Metacritic,
            RatingSource::Tmdb, RatingSource::Trakt, RatingSource::Mdblist, RatingSource::Ebert,
        ] {
            assert_eq!(RatingSource::from_cache_char(src.cache_char()), Some(src));
        }
        assert_eq!(RatingSource::from_cache_char('z'), None);
    }

    #[test]
    fn badges_suffix_from_available_matches_full_pipeline() {
        // Simulate: movie has imdb, rt, tmdb data. User orders "imdb,rt,tmdb" limit 3.
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.0".into() },
            RatingBadge { source: RatingSource::Rt, value: "95%".into() },
            RatingBadge { source: RatingSource::Tmdb, value: "7.5".into() },
        ];
        let available = available_sources_string(&badges);
        assert_eq!(available, "irt");

        // Full pipeline: apply_rating_preferences then badges_cache_suffix
        let filtered = apply_rating_preferences(badges, "imdb,rt,tmdb", "", 3);
        let expected = badges_cache_suffix(&filtered);

        // SQLite fast path: badges_suffix_from_available
        let actual = badges_suffix_from_available(&available, "imdb,rt,tmdb", "", 3);
        assert_eq!(actual, expected);
    }

    #[test]
    fn badges_suffix_from_available_matches_pipeline_with_new_sources() {
        // Cache invariant for the two new sources (mdblist 'd', ebert 'e'):
        // the SQLite fast-path suffix must equal the full-pipeline suffix.
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.0".into() },
            RatingBadge { source: RatingSource::Mdblist, value: "89".into() },
            RatingBadge { source: RatingSource::Ebert, value: "3.5".into() },
        ];
        let available = available_sources_string(&badges);
        // Canonical order places imdb(i) before mdblist(d) before ebert(e).
        assert_eq!(available, "ide");

        let order = "ebert,mdblist,imdb";
        let filtered = apply_rating_preferences(badges, order, "", 3);
        let expected = badges_cache_suffix(&filtered);
        let from_available = badges_suffix_from_available(&available, order, "", 3);
        assert_eq!(from_available, expected);
        assert_eq!(expected, "@edi");
    }

    #[test]
    fn available_sources_string_includes_new_sources_in_canonical_order() {
        let badges = vec![
            RatingBadge { source: RatingSource::Ebert, value: "3.5".into() },
            RatingBadge { source: RatingSource::Mdblist, value: "89".into() },
            RatingBadge { source: RatingSource::Mal, value: "8.50".into() },
        ];
        // Canonical: mal(m) ... mdblist(d) ... ebert(e)
        assert_eq!(available_sources_string(&badges), "mde");
    }

    #[test]
    fn badges_suffix_from_available_respects_limit() {
        let suffix = badges_suffix_from_available("irt", "imdb,rt,tmdb", "", 2);
        assert_eq!(suffix, "@ir");
    }

    #[test]
    fn badges_suffix_from_available_respects_order() {
        let suffix = badges_suffix_from_available("irt", "tmdb,rt,imdb", "", 3);
        assert_eq!(suffix, "@tri");
    }

    #[test]
    fn badges_suffix_from_available_empty() {
        assert_eq!(badges_suffix_from_available("", "imdb,rt", "", 3), "@");
    }

    #[test]
    fn available_sources_string_canonical_order() {
        // Badges arriving in non-canonical order should be stored canonically
        let badges = vec![
            RatingBadge { source: RatingSource::Tmdb, value: "7.5".into() },
            RatingBadge { source: RatingSource::Imdb, value: "8.0".into() },
            RatingBadge { source: RatingSource::Rt, value: "95%".into() },
        ];
        // Canonical order is: mal, imdb, lb, rt, rta, mc, tmdb, trakt
        // So imdb(i) < rt(r) < tmdb(t)
        assert_eq!(available_sources_string(&badges), "irt");
    }

    #[test]
    fn rt_and_rt_audience_share_color() {
        assert_eq!(RatingSource::Rt.color(), RatingSource::RtAudience.color());
    }

    #[test]
    fn rating_source_equality() {
        assert_eq!(RatingSource::Imdb, RatingSource::Imdb);
        assert_ne!(RatingSource::Imdb, RatingSource::Tmdb);
    }

    #[test]
    fn mdblist_badges_parses_score_int_and_rogerebert() {
        // Representative shape from the live MDBList API: top-level `score` is an
        // integer (89), rogerebert exposes only `value` (score/votes null), and
        // RT audience arrives as "popcorn".
        let json = r#"{
            "score": 89,
            "ids": { "imdb": "tt0111161", "tmdb": 278 },
            "ratings": [
                { "source": "imdb", "value": 9.3, "score": 93, "votes": 3000000 },
                { "source": "popcorn", "value": 98, "score": 98, "votes": 53000 },
                { "source": "rogerebert", "value": 3.5, "score": null, "votes": null }
            ]
        }"#;
        let resp: MdblistResponse = serde_json::from_str(json).expect("valid mdblist response");
        // Int `score` must deserialize into the Option<f64> field, not fail the
        // whole response (which would silently drop every MDBList rating).
        assert_eq!(resp.score, Some(89.0));

        let badges = mdblist_badges(&resp);
        let find = |s: RatingSource| badges.iter().find(|b| b.source == s).map(|b| b.value.as_str());
        assert_eq!(find(RatingSource::Imdb), Some("9.3"));
        assert_eq!(find(RatingSource::RtAudience), Some("98%"));
        assert_eq!(find(RatingSource::Ebert), Some("3.5"), "rogerebert value rendered as bare stars");
        assert_eq!(find(RatingSource::Mdblist), Some("89"), "top-level score rendered as a bare integer");
    }

    #[test]
    fn mdblist_badges_suppresses_missing_and_zero() {
        // score null -> no MDBList badge; rogerebert value 0 -> dropped;
        // an unmapped source ("metacriticuser") is ignored entirely.
        let json = r#"{
            "score": null,
            "ids": {},
            "ratings": [
                { "source": "rogerebert", "value": 0, "score": null, "votes": null },
                { "source": "metacriticuser", "value": 9.0, "score": 90, "votes": 10 }
            ]
        }"#;
        let resp: MdblistResponse = serde_json::from_str(json).expect("valid mdblist response");
        assert_eq!(resp.score, None);
        let badges = mdblist_badges(&resp);
        assert!(badges.iter().all(|b| b.source != RatingSource::Mdblist), "null score yields no MDBList badge");
        assert!(badges.iter().all(|b| b.source != RatingSource::Ebert), "0-star Ebert is suppressed");
        assert!(badges.is_empty(), "no mapped sources present");
    }

    #[test]
    fn mdblist_badges_absent_score_field_omits_mdblist() {
        // A response missing `score` entirely (serde default) must not panic and
        // must not emit an MDBList badge.
        let json = r#"{ "ids": {}, "ratings": [] }"#;
        let resp: MdblistResponse = serde_json::from_str(json).expect("valid mdblist response");
        assert_eq!(resp.score, None);
        assert!(mdblist_badges(&resp).is_empty());
    }

    #[test]
    fn mdblist_badges_suppresses_zero_score_sources() {
        // A 0 score is MDBList's "no data" sentinel for score-based sources; it
        // must be dropped rather than rendered as "0%"/"0", matching Ebert and the
        // aggregate score.
        let json = r#"{
            "score": null,
            "ids": {},
            "ratings": [
                { "source": "tomatoes", "value": null, "score": 0, "votes": 0 },
                { "source": "popcorn", "value": null, "score": 0, "votes": 0 },
                { "source": "metacritic", "value": null, "score": 0, "votes": 0 },
                { "source": "trakt", "value": null, "score": 0, "votes": 0 }
            ]
        }"#;
        let resp: MdblistResponse = serde_json::from_str(json).expect("valid mdblist response");
        assert!(mdblist_badges(&resp).is_empty(), "zero score-based sources are suppressed");
    }

    // --- trakt_badge (percent formatting + suppression) ---

    #[test]
    fn trakt_badge_formats_rating_as_percent() {
        // Trakt returns a 0–10 rating; we render it as a percentage (×10).
        assert_eq!(trakt_badge(7.5, 1200).map(|b| b.value), Some("75%".to_string()));
        assert_eq!(trakt_badge(10.0, 5).map(|b| b.value), Some("100%".to_string()));
        assert_eq!(
            trakt_badge(8.36, 42).map(|b| b.value),
            Some("84%".to_string()),
            "rounds to whole percent"
        );
    }

    #[test]
    fn trakt_badge_suppresses_zero_rating_or_no_votes() {
        assert!(trakt_badge(0.0, 1000).is_none(), "no rating");
        assert!(trakt_badge(7.5, 0).is_none(), "no votes");
    }

    #[test]
    fn trakt_badge_uses_trakt_source() {
        assert_eq!(trakt_badge(6.0, 1).map(|b| b.source), Some(RatingSource::Trakt));
    }

    // --- mdblist_lookup_for (issue #14) ---

    fn resolved(imdb: Option<&str>, tmdb_id: u64, media_type: MediaType) -> ResolvedId {
        ResolvedId {
            imdb_id: imdb.map(|s| s.to_string()),
            tmdb_id,
            tvdb_id: None,
            media_type,
            poster_path: None,
            release_date: None,
            episode: None,
            original_language: None,
        }
    }

    #[test]
    fn mdblist_lookup_prefers_imdb_when_present() {
        let r = resolved(Some("tt2560140"), 1429, MediaType::Tv);
        assert_eq!(mdblist_lookup_for(&r), Some(MdblistLookup::Imdb("tt2560140")));
    }

    #[test]
    fn mdblist_lookup_falls_back_to_tmdb_when_no_imdb() {
        // The anime case: TMDB knows the title but has no IMDb cross-reference.
        // Without the fallback, every MDBList-sourced badge (incl. MAL) is lost.
        let r = resolved(None, 1429, MediaType::Tv);
        assert_eq!(mdblist_lookup_for(&r), Some(MdblistLookup::Tmdb(1429)));
    }

    #[test]
    fn mdblist_lookup_falls_back_to_tmdb_for_movies_too() {
        let r = resolved(None, 550, MediaType::Movie);
        assert_eq!(mdblist_lookup_for(&r), Some(MdblistLookup::Tmdb(550)));
    }

    #[test]
    fn mdblist_lookup_falls_back_to_tmdb_when_imdb_empty() {
        // TMDB returns "" (not null) for titles with no IMDb cross-reference.
        // An empty id must be treated as absent so the TMDB fallback still fires —
        // otherwise the anime/no-IMDb case (issue #14) loses every MDBList badge.
        let r = resolved(Some(""), 1429, MediaType::Tv);
        assert_eq!(mdblist_lookup_for(&r), Some(MdblistLookup::Tmdb(1429)));
    }

    #[test]
    fn mdblist_lookup_none_for_episodes() {
        // MDBList has no episode-level ratings regardless of which ids are present.
        let with_imdb = resolved(Some("tt2560140"), 1429, MediaType::Episode);
        let without_imdb = resolved(None, 1429, MediaType::Episode);
        assert_eq!(mdblist_lookup_for(&with_imdb), None);
        assert_eq!(mdblist_lookup_for(&without_imdb), None);
    }

}

/// Which MDBList endpoint to use for a resolved title.
///
/// Prefer the IMDb-keyed lookup. When a title resolved without an IMDb id —
/// common for anime and other titles TMDB knows but hasn't cross-referenced to
/// IMDb — fall back to the TMDB id (always present), so the title keeps every
/// MDBList-sourced badge (IMDb, RT, Metacritic, Trakt, Letterboxd, MyAnimeList,
/// the MDBList score, Roger Ebert) instead of collapsing to the TMDB
/// vote_average alone. Episodes have no MDBList ratings. (issue #14)
#[derive(Debug, PartialEq, Eq)]
enum MdblistLookup<'a> {
    Imdb(&'a str),
    Tmdb(u64),
}

fn mdblist_lookup_for(resolved: &ResolvedId) -> Option<MdblistLookup<'_>> {
    // mdblist only supports movie/show level ratings, not individual episodes
    if resolved.media_type == MediaType::Episode {
        return None;
    }
    // Treat an empty imdb_id as absent. Resolved ids are normalised at the source
    // (id/mod.rs), but guard here too so the TMDB fallback can never be defeated by
    // a blank id reaching this function directly.
    match resolved.imdb_id.as_deref().filter(|s| !s.is_empty()) {
        Some(imdb_id) => Some(MdblistLookup::Imdb(imdb_id)),
        None => Some(MdblistLookup::Tmdb(resolved.tmdb_id)),
    }
}

async fn fetch_mdblist_ratings(
    resolved: &ResolvedId,
    mdblist: Option<&MdblistClient>,
) -> Option<(Vec<RatingBadge>, Option<u64>, Option<u64>, Option<String>)> {
    let client = mdblist?;

    let resp = match mdblist_lookup_for(resolved)? {
        MdblistLookup::Imdb(imdb_id) => match client.get_ratings(imdb_id, &resolved.media_type).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(imdb_id, media_type = ?resolved.media_type, "mdblist rating fetch failed: {e}");
                return None;
            }
        },
        MdblistLookup::Tmdb(tmdb_id) => match client.get_ratings_by_tmdb(tmdb_id, &resolved.media_type).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(tmdb_id, media_type = ?resolved.media_type, "mdblist tmdb rating fetch failed: {e}");
                return None;
            }
        },
    };

    Some((mdblist_badges(&resp), resp.ids.tmdb, resp.ids.tvdb, resp.ids.imdb))
}

/// Map a raw MDBList response into rating badges.
///
/// Pure (no I/O) so the source→badge mapping, value formatting, and `<= 0`
/// suppression can be unit-tested directly without an HTTP client.
fn mdblist_badges(resp: &MdblistResponse) -> Vec<RatingBadge> {
    let mut badges = Vec::new();

    for r in &resp.ratings {
        let badge = match r.source.as_str() {
            "imdb" => r.value.map(|v| RatingBadge {
                source: RatingSource::Imdb,
                value: format!("{v:.1}"),
            }),
            // Score-based sources: suppress a 0 (MDBList's "no data" sentinel),
            // matching the zero-suppression on Ebert, the aggregate score, and the
            // direct Trakt/TMDB paths so a missing source never renders as 0%.
            "trakt" => r.score.filter(|s| *s > 0.0).map(|s| RatingBadge {
                source: RatingSource::Trakt,
                value: format!("{:.0}%", s),
            }),
            "letterboxd" => r.value.map(|v| RatingBadge {
                source: RatingSource::Letterboxd,
                value: format!("{v:.1}"),
            }),
            "popcorn" => r.score.filter(|s| *s > 0.0).map(|s| RatingBadge {
                source: RatingSource::RtAudience,
                value: format!("{:.0}%", s),
            }),
            "tomatoes" => r.score.filter(|s| *s > 0.0).map(|s| RatingBadge {
                source: RatingSource::Rt,
                value: format!("{:.0}%", s),
            }),
            "metacritic" => r.score.filter(|s| *s > 0.0).map(|s| RatingBadge {
                source: RatingSource::Metacritic,
                value: format!("{:.0}", s),
            }),
            "myanimelist" => r.score.filter(|s| *s > 0.0).map(|s| RatingBadge {
                source: RatingSource::Mal,
                value: format!("{:.2}", s / 10.0),
            }),
            // Roger Ebert's classic 0–4 star rating (issue #35). MDBList exposes
            // only `value` for this source (`score`/`votes` are null), so render
            // the bare star value, matching the IMDb/Letterboxd decimal style.
            "rogerebert" => r.value.filter(|v| *v > 0.0).map(|v| RatingBadge {
                source: RatingSource::Ebert,
                value: format!("{v:.1}"),
            }),
            _ => None,
        };

        if let Some(b) = badge {
            badges.push(b);
        }
    }

    // MDBList's own aggregated score (issue #42) is a 0–100 meta-score returned
    // at the top level of the response, not inside the `ratings` array. Render it
    // as a bare integer to match MDBList's own presentation (and Metacritic).
    if let Some(score) = resp.score.filter(|s| *s > 0.0) {
        badges.push(RatingBadge {
            source: RatingSource::Mdblist,
            value: format!("{score:.0}"),
        });
    }

    badges
}
