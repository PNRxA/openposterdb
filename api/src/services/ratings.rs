use crate::id::{MediaType, ResolvedId};
use crate::services::mdblist::MdblistClient;
use crate::services::omdb::OmdbClient;
use crate::services::tmdb::TmdbClient;
use image::Rgba;
use serde::Deserialize;

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

    /// Single-char identifier used only in cache key suffixes (never parsed back).
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

pub async fn fetch_ratings(
    resolved: &ResolvedId,
    tmdb: &TmdbClient,
    omdb: Option<&OmdbClient>,
    mdblist: Option<&MdblistClient>,
    cache: &moka::future::Cache<String, Vec<RatingBadge>>,
) -> Vec<RatingBadge> {
    let media_type_str = match resolved.media_type {
        MediaType::Movie => "movie",
        MediaType::Tv => "tv",
    };
    let key = format!("{}/{media_type_str}", resolved.tmdb_id);

    let resolved = resolved.clone();
    let tmdb = tmdb.clone();
    let omdb = omdb.cloned();
    let mdblist = mdblist.cloned();

    cache
        .try_get_with(key, async move {
            let result = fetch_ratings_inner(&resolved, &tmdb, omdb.as_ref(), mdblist.as_ref()).await;
            Ok::<_, std::convert::Infallible>(result)
        })
        .await
        .unwrap_or_default()
}

async fn fetch_ratings_inner(
    resolved: &ResolvedId,
    tmdb: &TmdbClient,
    omdb: Option<&OmdbClient>,
    mdblist: Option<&MdblistClient>,
) -> Vec<RatingBadge> {
    let tmdb_fut = fetch_tmdb_rating(resolved, tmdb);
    let omdb_fut = fetch_omdb_ratings(resolved.imdb_id.as_deref(), omdb);
    let mdblist_fut = fetch_mdblist_ratings(resolved, mdblist);

    let (tmdb_badges, omdb_badges, mdblist_badges) =
        tokio::join!(tmdb_fut, omdb_fut, mdblist_fut);

    // Collect which sources OMDb already provided
    let omdb_has = |src: RatingSource| -> bool {
        omdb_badges
            .as_ref()
            .is_some_and(|list| list.iter().any(|b| b.source == src))
    };
    let has_rt = omdb_has(RatingSource::Rt);
    let has_mc = omdb_has(RatingSource::Metacritic);

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

    // Badge order: IMDb, TMDB, RT, RT Audience, MC, Trakt, Letterboxd
    let ordered: Vec<Option<RatingBadge>> = vec![
        find_omdb(RatingSource::Imdb).or_else(|| find_mdb(RatingSource::Imdb)),
        tmdb_badges,
        find_omdb(RatingSource::Rt).or_else(|| if !has_rt { find_mdb(RatingSource::Rt) } else { None }),
        find_mdb(RatingSource::RtAudience),
        find_omdb(RatingSource::Metacritic).or_else(|| if !has_mc { find_mdb(RatingSource::Metacritic) } else { None }),
        find_mdb(RatingSource::Trakt),
        find_mdb(RatingSource::Letterboxd),
        find_mdb(RatingSource::Mal),
    ];

    ordered.into_iter().flatten().collect()
}

async fn fetch_tmdb_rating(resolved: &ResolvedId, tmdb: &TmdbClient) -> Option<RatingBadge> {
    #[derive(Deserialize)]
    struct Details {
        vote_average: Option<f64>,
    }

    let path = match resolved.media_type {
        MediaType::Movie => format!("/movie/{}", resolved.tmdb_id),
        MediaType::Tv => format!("/tv/{}", resolved.tmdb_id),
    };

    let details: Details = tmdb.get(&path, &[]).await.ok()?;
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
    let resp = client.get_ratings(imdb_id).await.ok()?;
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

/// Canonical order of all rating sources, used for deterministic cache keys.
const CANONICAL_ORDER: &[&str] = &["mal", "imdb", "lb", "rt", "rta", "mc", "tmdb", "trakt"];

/// Compute a deterministic cache key suffix from rating preferences.
///
/// Parses `order` into known `RatingSource` keys, appends any missing sources
/// in canonical order for determinism, then truncates to `limit` if positive.
/// Returns a compact string like `@mil` (single-char per source, no commas).
pub fn ratings_cache_suffix(order: &str, limit: i32) -> String {
    let mut sources: Vec<RatingSource> = order
        .split(',')
        .map(|k| k.trim())
        .filter_map(RatingSource::from_key)
        .collect();

    // Append missing sources in canonical order
    for &canonical in CANONICAL_ORDER {
        if let Some(src) = RatingSource::from_key(canonical) {
            if !sources.contains(&src) {
                sources.push(src);
            }
        }
    }

    if limit > 0 {
        sources.truncate(limit as usize);
    }

    let chars: String = sources.iter().map(|s| s.cache_char()).collect();
    format!("@{chars}")
}

/// Reorder and/or limit rating badges based on user preferences.
///
/// - If `order` is non-empty, badges are reordered to match the specified order.
///   Unmentioned sources are appended after in their original order.
/// - If `limit` > 0, the result is truncated to that many badges.
pub fn apply_rating_preferences(badges: Vec<RatingBadge>, order: &str, limit: i32) -> Vec<RatingBadge> {
    let mut result = if order.is_empty() {
        badges
    } else {
        let preferred: Vec<RatingSource> = order
            .split(',')
            .filter_map(|k| RatingSource::from_key(k.trim()))
            .collect();

        let mut ordered = Vec::with_capacity(badges.len());
        // Add badges in preferred order
        for src in &preferred {
            if let Some(badge) = badges.iter().find(|b| b.source == *src) {
                ordered.push(badge.clone());
            }
        }
        // Add remaining badges not in the preferred list
        for badge in &badges {
            if !preferred.contains(&badge.source) {
                ordered.push(badge.clone());
            }
        }
        ordered
    };

    if limit > 0 {
        result.truncate(limit as usize);
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
        let result = apply_rating_preferences(badges, "trakt,imdb", 0);
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
        assert_eq!(result.len(), 2);
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
        let suffix = ratings_cache_suffix("imdb,rt", 0);
        assert_eq!(suffix, "@irmlactk");
    }

    #[test]
    fn ratings_cache_suffix_limit_zero_includes_all() {
        let suffix = ratings_cache_suffix("mal,imdb,lb,rt,rta,mc,tmdb,trakt", 0);
        assert_eq!(suffix, "@milractk");
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
) -> Option<Vec<RatingBadge>> {
    let client = mdblist?;
    let imdb_id = resolved.imdb_id.as_deref()?;

    let resp = client
        .get_ratings(imdb_id, &resolved.media_type)
        .await
        .ok()?;

    let mut badges = Vec::new();

    for r in &resp.ratings {
        let badge = match r.source.as_str() {
            "imdb" => r.value.map(|v| RatingBadge {
                source: RatingSource::Imdb,
                value: format!("{v:.1}"),
            }),
            "trakt" => r.score.map(|s| RatingBadge {
                source: RatingSource::Trakt,
                value: format!("{s}%"),
            }),
            "letterboxd" => r.value.map(|v| RatingBadge {
                source: RatingSource::Letterboxd,
                value: format!("{v:.1}"),
            }),
            "popcorn" => r.score.map(|s| RatingBadge {
                source: RatingSource::RtAudience,
                value: format!("{s}%"),
            }),
            "tomatoes" => r.score.map(|s| RatingBadge {
                source: RatingSource::Rt,
                value: format!("{s}%"),
            }),
            "metacritic" => r.score.map(|s| RatingBadge {
                source: RatingSource::Metacritic,
                value: s.to_string(),
            }),
            "myanimelist" => r.score.map(|s| RatingBadge {
                source: RatingSource::Mal,
                value: format!("{:.2}", s as f64 / 10.0),
            }),
            _ => None,
        };

        if let Some(b) = badge {
            badges.push(b);
        }
    }

    Some(badges)
}
