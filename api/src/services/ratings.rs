use crate::error::AppError;
use crate::id::{MediaType, ResolvedId};
use crate::services::mdblist::MdblistClient;
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
            _ => None,
        }
    }

    pub fn all_keys() -> Vec<&'static str> {
        vec!["imdb", "tmdb", "rt", "rta", "mc", "trakt", "lb", "mal"]
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

    // Badge order: IMDb, TMDB, RT, RT Audience, MC, Trakt, Letterboxd, MAL
    // MDBList preferred for overlapping sources; OMDb and direct Trakt fill gaps
    let ordered: Vec<Option<RatingBadge>> = vec![
        find_mdb(RatingSource::Imdb).or_else(|| find_omdb(RatingSource::Imdb)),
        tmdb_badges,
        find_mdb(RatingSource::Rt).or_else(|| find_omdb(RatingSource::Rt)),
        find_mdb(RatingSource::RtAudience),
        find_mdb(RatingSource::Metacritic).or_else(|| find_omdb(RatingSource::Metacritic)),
        find_mdb(RatingSource::Trakt).or(trakt_badge),
        find_mdb(RatingSource::Letterboxd),
        find_mdb(RatingSource::Mal),
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

    if resp.rating <= 0.0 || resp.votes == 0 {
        return None;
    }

    Some(RatingBadge {
        source: RatingSource::Trakt,
        value: format!("{:.0}%", resp.rating * 10.0),
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

/// Canonical order of all rating sources, used for deterministic cache keys.
const CANONICAL_ORDER: &[&str] = &["mal", "imdb", "lb", "rt", "rta", "mc", "tmdb", "trakt"];

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
fn order_and_limit(sources: Vec<RatingSource>, order: &str, limit: i32) -> Vec<RatingSource> {
    if limit == 0 {
        return Vec::new();
    }

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
pub fn badges_suffix_from_available(available_sources: &str, order: &str, limit: i32) -> String {
    let available: Vec<RatingSource> = available_sources
        .chars()
        .filter_map(RatingSource::from_cache_char)
        .collect();

    let ordered = order_and_limit(available, order, limit);

    let chars: String = ordered.iter().map(|s| s.cache_char()).collect();
    format!("@{chars}")
}

/// Compute a deterministic cache key suffix from rating preferences.
///
/// Parses `order` into known `RatingSource` keys, appends any missing sources
/// in canonical order for determinism, then truncates to `limit` if positive.
/// Returns a compact string like `@mil` (single-char per source, no commas).
pub fn ratings_cache_suffix(order: &str, limit: i32) -> String {
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

    debug_assert!(limit > 0, "negative limit is not supported");
    sources.truncate(limit as usize);

    let chars: String = sources.iter().map(|s| s.cache_char()).collect();
    format!("@{chars}")
}

/// Reorder and/or limit rating badges based on user preferences.
///
/// - If `order` is non-empty, badges are reordered to match the specified order.
///   Unmentioned sources are appended after in their original order.
/// - If `limit` is 0, an empty list is returned (no ratings).
/// - If `limit` > 0, the result is truncated to that many badges.
pub fn apply_rating_preferences(badges: Vec<RatingBadge>, order: &str, limit: i32) -> Vec<RatingBadge> {
    let sources: Vec<RatingSource> = badges.iter().map(|b| b.source).collect();
    let ordered_sources = order_and_limit(sources, order, limit);

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
        let result = apply_rating_preferences(badges, "trakt,imdb", 8);
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
        let result = apply_rating_preferences(badges, "", 2);
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
        let result = apply_rating_preferences(badges, "mal,imdb,rta,trakt", 3);
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
        let result = apply_rating_preferences(badges.clone(), "", 0);
        assert_eq!(result.len(), 0, "limit=0 should return no ratings");
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
        let suffix = ratings_cache_suffix("mal,imdb,lb,rt,rta,mc,tmdb,trakt", 3);
        assert_eq!(suffix, "@mil");
    }

    #[test]
    fn ratings_cache_suffix_custom_order() {
        let suffix = ratings_cache_suffix("trakt,imdb,rt", 3);
        assert_eq!(suffix, "@kir");
    }

    #[test]
    fn ratings_cache_suffix_partial_order_normalized() {
        // Only two sources specified — missing ones appended in canonical order
        let suffix = ratings_cache_suffix("imdb,rt", 8);
        assert_eq!(suffix, "@irmlactk");
    }

    #[test]
    fn ratings_cache_suffix_limit_zero_shows_none() {
        let suffix = ratings_cache_suffix("mal,imdb,lb,rt,rta,mc,tmdb,trakt", 0);
        assert_eq!(suffix, "@");
    }

    #[test]
    fn ratings_cache_suffix_empty_order() {
        let suffix = ratings_cache_suffix("", 3);
        assert_eq!(suffix, "@mil");
    }

    #[test]
    fn ratings_cache_suffix_invalid_sources_ignored() {
        let suffix = ratings_cache_suffix("imdb,bogus,rt,fake", 3);
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
            RatingSource::Tmdb, RatingSource::Trakt,
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
        let filtered = apply_rating_preferences(badges, "imdb,rt,tmdb", 3);
        let expected = badges_cache_suffix(&filtered);

        // SQLite fast path: badges_suffix_from_available
        let actual = badges_suffix_from_available(&available, "imdb,rt,tmdb", 3);
        assert_eq!(actual, expected);
    }

    #[test]
    fn badges_suffix_from_available_respects_limit() {
        let suffix = badges_suffix_from_available("irt", "imdb,rt,tmdb", 2);
        assert_eq!(suffix, "@ir");
    }

    #[test]
    fn badges_suffix_from_available_respects_order() {
        let suffix = badges_suffix_from_available("irt", "tmdb,rt,imdb", 3);
        assert_eq!(suffix, "@tri");
    }

    #[test]
    fn badges_suffix_from_available_empty() {
        assert_eq!(badges_suffix_from_available("", "imdb,rt", 3), "@");
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

}

async fn fetch_mdblist_ratings(
    resolved: &ResolvedId,
    mdblist: Option<&MdblistClient>,
) -> Option<(Vec<RatingBadge>, Option<u64>, Option<u64>, Option<String>)> {
    // mdblist only supports movie/show level ratings, not individual episodes
    if resolved.media_type == MediaType::Episode {
        return None;
    }
    let client = mdblist?;
    let imdb_id = resolved.imdb_id.as_deref()?;

    let resp = match client.get_ratings(imdb_id, &resolved.media_type).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(imdb_id, media_type = ?resolved.media_type, "mdblist rating fetch failed: {e}");
            return None;
        }
    };

    let mut badges = Vec::new();

    for r in &resp.ratings {
        let badge = match r.source.as_str() {
            "imdb" => r.value.map(|v| RatingBadge {
                source: RatingSource::Imdb,
                value: format!("{v:.1}"),
            }),
            "trakt" => r.score.map(|s| RatingBadge {
                source: RatingSource::Trakt,
                value: format!("{:.0}%", s),
            }),
            "letterboxd" => r.value.map(|v| RatingBadge {
                source: RatingSource::Letterboxd,
                value: format!("{v:.1}"),
            }),
            "popcorn" => r.score.map(|s| RatingBadge {
                source: RatingSource::RtAudience,
                value: format!("{:.0}%", s),
            }),
            "tomatoes" => r.score.map(|s| RatingBadge {
                source: RatingSource::Rt,
                value: format!("{:.0}%", s),
            }),
            "metacritic" => r.score.map(|s| RatingBadge {
                source: RatingSource::Metacritic,
                value: format!("{:.0}", s),
            }),
            "myanimelist" => r.score.map(|s| RatingBadge {
                source: RatingSource::Mal,
                value: format!("{:.2}", s / 10.0),
            }),
            _ => None,
        };

        if let Some(b) = badge {
            badges.push(b);
        }
    }

    Some((badges, resp.ids.tmdb, resp.ids.tvdb, resp.ids.imdb))
}
