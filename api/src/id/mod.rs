use crate::error::AppError;
use crate::services::tmdb::TmdbClient;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdType {
    Imdb,
    Tmdb,
    Tvdb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Movie,
    Tv,
    Episode,
    Season,
}

#[derive(Debug, Clone)]
pub struct EpisodeInfo {
    pub show_tmdb_id: u64,
    pub season_number: u32,
    pub episode_number: u32,
    pub still_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SeasonInfo {
    pub show_tmdb_id: u64,
    pub season_number: u32,
    /// The season's own poster path on TMDB (before any series fallback).
    /// Used for fanart season-poster selection and cache scoping.
    pub poster_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedId {
    pub imdb_id: Option<String>,
    pub tmdb_id: u64,
    pub tvdb_id: Option<u64>,
    pub media_type: MediaType,
    pub poster_path: Option<String>,
    pub release_date: Option<String>,
    pub episode: Option<EpisodeInfo>,
    pub season: Option<SeasonInfo>,
}

pub fn format_tmdb_id_value(
    tmdb_id: u64,
    media_type: &MediaType,
    episode: Option<&EpisodeInfo>,
    season: Option<&SeasonInfo>,
) -> String {
    match media_type {
        MediaType::Movie => format!("movie-{tmdb_id}"),
        MediaType::Tv => format!("series-{tmdb_id}"),
        MediaType::Episode => match episode {
            Some(ep) => format!("episode-{}-S{}E{}", ep.show_tmdb_id, ep.season_number, ep.episode_number),
            None => format!("series-{tmdb_id}"),
        },
        MediaType::Season => match season {
            Some(s) => format!("season-{}-S{}", s.show_tmdb_id, s.season_number),
            None => format!("series-{tmdb_id}"),
        },
    }
}

impl IdType {
    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            "imdb" => Ok(IdType::Imdb),
            "tmdb" => Ok(IdType::Tmdb),
            "tvdb" => Ok(IdType::Tvdb),
            other => Err(AppError::InvalidIdType(other.to_string())),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Imdb => "imdb",
            Self::Tmdb => "tmdb",
            Self::Tvdb => "tvdb",
        }
    }
}

#[derive(Debug, Deserialize)]
struct FindResult {
    #[serde(default)]
    movie_results: Vec<FindEntry>,
    #[serde(default)]
    tv_results: Vec<FindEntry>,
    #[serde(default)]
    tv_episode_results: Vec<EpisodeFindEntry>,
}

#[derive(Debug, Deserialize)]
struct EpisodeFindEntry {
    show_id: u64,
    season_number: u32,
    episode_number: u32,
    still_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FindEntry {
    id: u64,
    poster_path: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    #[serde(default)]
    popularity: f64,
}

pub async fn resolve(
    id_type: IdType,
    id_value: &str,
    tmdb: &TmdbClient,
    cache: &moka::future::Cache<String, ResolvedId>,
) -> Result<ResolvedId, AppError> {
    let id_type_str = match id_type {
        IdType::Imdb => "imdb",
        IdType::Tmdb => "tmdb",
        IdType::Tvdb => "tvdb",
    };
    let key = format!("{id_type_str}/{id_value}");
    let tmdb = tmdb.clone();
    let id_value = id_value.to_owned();
    cache
        .try_get_with(key, async move {
            match id_type {
                IdType::Imdb => resolve_imdb(&id_value, &tmdb).await,
                IdType::Tmdb => resolve_tmdb(&id_value, &tmdb).await,
                IdType::Tvdb => resolve_tvdb(&id_value, &tmdb).await,
            }
        })
        .await
        .map_err(AppError::from_cached)
}

async fn resolve_imdb(imdb_id: &str, tmdb: &TmdbClient) -> Result<ResolvedId, AppError> {
    // Handle episode format: episode-{series_imdb_id}-S{season}E{episode}
    if let Some(rest) = imdb_id.strip_prefix("episode-") {
        return resolve_imdb_episode(rest, imdb_id, tmdb).await;
    }

    // Handle season format: season-{series_imdb_id}-S{season}
    if let Some(rest) = imdb_id.strip_prefix("season-") {
        return resolve_imdb_season(rest, imdb_id, tmdb).await;
    }

    let result: FindResult = tmdb
        .get(&format!("/find/{imdb_id}"), &[("external_source", "imdb_id")])
        .await?;

    // Check episode results first — an episode IMDb ID is unambiguous.
    if let Some(ep) = result.tv_episode_results.first() {
        return resolve_episode_details(tmdb, ep.show_id, ep.season_number, ep.episode_number, ep.still_path.clone(), Some(imdb_id.to_string())).await;
    }

    // Pick the most popular entry across all movie and TV results.
    let best_movie = result.movie_results.iter().max_by(|a, b| a.popularity.total_cmp(&b.popularity));
    let best_tv = result.tv_results.iter().max_by(|a, b| a.popularity.total_cmp(&b.popularity));

    // When both exist, pick the one with higher popularity.
    // Fall back to movie-first ordering when popularity is equal.
    let pick_movie = match (best_movie, best_tv) {
        (Some(m), Some(t)) => m.popularity >= t.popularity,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => false,
    };

    if pick_movie {
        if let Some(movie) = best_movie {
            return Ok(ResolvedId {
                imdb_id: Some(imdb_id.to_string()),
                tmdb_id: movie.id,
                tvdb_id: None,
                media_type: MediaType::Movie,
                poster_path: movie.poster_path.clone(),
                release_date: movie.release_date.clone(),
                episode: None,
                season: None,
            });
        }
    }
    if let Some(tv) = best_tv {
        return Ok(ResolvedId {
            imdb_id: Some(imdb_id.to_string()),
            tmdb_id: tv.id,
            tvdb_id: None,
            media_type: MediaType::Tv,
            poster_path: tv.poster_path.clone(),
            release_date: tv.first_air_date.clone(),
            episode: None,
            season: None,
        });
    }
    Err(AppError::IdNotFound(format!("{imdb_id} (not found on TMDB)")))
}

async fn resolve_tmdb(id_value: &str, tmdb: &TmdbClient) -> Result<ResolvedId, AppError> {
    // Handle episode format: episode-{show_id}-S{season}E{episode}
    if let Some(rest) = id_value.strip_prefix("episode-") {
        return resolve_tmdb_episode(rest, id_value, tmdb).await;
    }

    // Handle season format: season-{show_id}-S{season}
    if let Some(rest) = id_value.strip_prefix("season-") {
        return resolve_tmdb_season(rest, id_value, tmdb).await;
    }

    let (media_type, tmdb_id) = if let Some(rest) = id_value.strip_prefix("movie-") {
        (MediaType::Movie, rest.parse::<u64>().map_err(|_| AppError::InvalidIdType(id_value.to_string()))?)
    } else if let Some(rest) = id_value.strip_prefix("series-") {
        (MediaType::Tv, rest.parse::<u64>().map_err(|_| AppError::InvalidIdType(id_value.to_string()))?)
    } else {
        return Err(AppError::InvalidIdType(format!(
            "tmdb id must be prefixed with movie-, series-, episode-, or season-: {id_value}"
        )));
    };

    #[derive(Deserialize)]
    struct Details {
        imdb_id: Option<String>,
        poster_path: Option<String>,
        release_date: Option<String>,
        first_air_date: Option<String>,
        #[serde(default)]
        external_ids: Option<ExternalIds>,
    }
    #[derive(Deserialize)]
    struct ExternalIds {
        imdb_id: Option<String>,
        tvdb_id: Option<u64>,
    }

    let path = match media_type {
        MediaType::Movie => format!("/movie/{tmdb_id}"),
        MediaType::Tv => format!("/tv/{tmdb_id}?append_to_response=external_ids"),
        MediaType::Episode => return Err(AppError::Other("unexpected Episode media type in resolve_tmdb".into())),
        MediaType::Season => return Err(AppError::Other("unexpected Season media type in resolve_tmdb".into())),
    };
    let details: Details = tmdb.get(&path, &[]).await?;

    // TMDB returns an empty string (not null) for titles it hasn't cross-referenced
    // to IMDb; treat that as absent so downstream MDBList/Trakt/OMDb lookups take
    // their TMDB-id fallback or skip cleanly instead of querying with a blank id.
    let imdb_id = details
        .imdb_id
        .or_else(|| details.external_ids.as_ref().and_then(|e| e.imdb_id.clone()))
        .filter(|s| !s.is_empty());

    let tvdb_id = details.external_ids.as_ref().and_then(|e| e.tvdb_id);

    let release_date = match media_type {
        MediaType::Movie => details.release_date,
        MediaType::Tv => details.first_air_date,
        MediaType::Episode => return Err(AppError::Other("unexpected Episode media type in resolve_tmdb".into())),
        MediaType::Season => return Err(AppError::Other("unexpected Season media type in resolve_tmdb".into())),
    };

    Ok(ResolvedId {
        imdb_id,
        tmdb_id,
        tvdb_id,
        media_type,
        poster_path: details.poster_path,
        release_date,
        episode: None,
        season: None,
    })
}

/// Parse the `{show_id}-S{season}E{episode}` portion of an episode ID.
/// Returns `(show_id, season, episode)` on success.
fn parse_episode_id(rest: &str, id_value: &str) -> Result<(u64, u32, u32), AppError> {
    let (id_str, season, episode) = parse_episode_external(rest, id_value)?;
    let show_id = id_str
        .parse::<u64>()
        .map_err(|_| AppError::InvalidIdType(id_value.to_string()))?;
    Ok((show_id, season, episode))
}

/// Parse `episode-{show_id}-S{season}E{episode}` and resolve via TMDB.
async fn resolve_tmdb_episode(rest: &str, id_value: &str, tmdb: &TmdbClient) -> Result<ResolvedId, AppError> {
    let (show_id, season, episode) = parse_episode_id(rest, id_value)?;
    resolve_episode_details(tmdb, show_id, season, episode, None, None).await
}

/// Shared helper: fetch episode details from TMDB and build a `ResolvedId`.
///
/// Called by all three resolvers (IMDb, TMDB, TVDB) after they determine the
/// show ID and season/episode numbers. `hint_still_path` and `hint_imdb_id` are
/// values already known from the `/find` response (if any) to avoid redundant lookups.
async fn resolve_episode_details(
    tmdb: &TmdbClient,
    show_tmdb_id: u64,
    season: u32,
    episode: u32,
    hint_still_path: Option<String>,
    hint_imdb_id: Option<String>,
) -> Result<ResolvedId, AppError> {
    #[derive(Deserialize)]
    struct EpDetails {
        still_path: Option<String>,
        air_date: Option<String>,
        #[serde(default)]
        external_ids: Option<EpExternalIds>,
    }
    #[derive(Deserialize)]
    struct EpExternalIds {
        imdb_id: Option<String>,
        tvdb_id: Option<u64>,
    }

    let details: EpDetails = tmdb
        .get(
            &format!("/tv/{show_tmdb_id}/season/{season}/episode/{episode}"),
            &[("append_to_response", "external_ids")],
        )
        .await?;

    let imdb_id = hint_imdb_id
        .or_else(|| details.external_ids.as_ref().and_then(|e| e.imdb_id.clone()))
        .filter(|s| !s.is_empty());
    let tvdb_id = details.external_ids.as_ref().and_then(|e| e.tvdb_id);
    let still_path = hint_still_path.or(details.still_path.clone());

    // Use still_path as poster_path; fallback to series poster if no still
    let poster_path = if still_path.is_some() {
        still_path.clone()
    } else {
        #[derive(Deserialize)]
        struct ShowInfo {
            poster_path: Option<String>,
        }
        let show: ShowInfo = tmdb.get(&format!("/tv/{show_tmdb_id}"), &[]).await?;
        show.poster_path
    };

    Ok(ResolvedId {
        imdb_id,
        tmdb_id: show_tmdb_id,
        tvdb_id,
        media_type: MediaType::Episode,
        poster_path,
        release_date: details.air_date,
        episode: Some(EpisodeInfo {
            show_tmdb_id,
            season_number: season,
            episode_number: episode,
            still_path,
        }),
        season: None,
    })
}

/// Parse the `{show_id}-S{season}` portion of a season ID.
/// Returns `(show_id, season)` on success.
fn parse_season_id(rest: &str, id_value: &str) -> Result<(u64, u32), AppError> {
    let (id_str, season) = parse_season_external(rest, id_value)?;
    let show_id = id_str
        .parse::<u64>()
        .map_err(|_| AppError::InvalidIdType(id_value.to_string()))?;
    Ok((show_id, season))
}

/// Parse `{external_id}-S{season}` for IMDb/TVDB season lookups.
/// Unlike `parse_season_id`, the ID portion is returned as a string slice
/// because IMDb IDs like `tt1234567` aren't purely numeric.
fn parse_season_external<'a>(rest: &'a str, id_value: &str) -> Result<(&'a str, u32), AppError> {
    let upper = rest.to_ascii_uppercase();
    let split_pos = upper.find("-S").ok_or_else(|| {
        AppError::InvalidIdType(format!(
            "season id must be season-{{id}}-S{{season}}: {id_value}"
        ))
    })?;
    let external_id = &rest[..split_pos];
    if external_id.is_empty() {
        return Err(AppError::InvalidIdType(format!(
            "season id must be season-{{id}}-S{{season}}: {id_value}"
        )));
    }
    // Everything after "-S" must be the season number; an `E…` suffix (episode
    // syntax) makes this parse fail, which correctly rejects episode IDs here.
    let season_str = &upper[split_pos + 2..];
    let season = season_str
        .parse::<u32>()
        .map_err(|_| AppError::InvalidIdType(id_value.to_string()))?;
    if season > 10_000 {
        return Err(AppError::BadRequest("season must be ≤ 10 000".into()));
    }
    Ok((external_id, season))
}

/// Shared helper: fetch season details from TMDB and build a `ResolvedId`.
///
/// Mirrors `resolve_episode_details`. The season endpoint returns the season's
/// own `poster_path` and `air_date`; we fall back to the series poster when the
/// season has none. Seasons don't carry their own IMDb id, so the IMDb id is
/// taken from the *show* (via `hint_imdb_id` or the show's `external_ids`) so
/// downstream OMDb/Trakt lookups resolve show-level ratings where season-scoped
/// data isn't available.
async fn resolve_season_details(
    tmdb: &TmdbClient,
    show_tmdb_id: u64,
    season: u32,
    hint_imdb_id: Option<String>,
) -> Result<ResolvedId, AppError> {
    #[derive(Deserialize)]
    struct SeasonDetails {
        poster_path: Option<String>,
        air_date: Option<String>,
    }

    let details: SeasonDetails = tmdb
        .get(&format!("/tv/{show_tmdb_id}/season/{season}"), &[])
        .await?;

    // Resolve the show's IMDb/TVDB ids and poster (used as a fallback when the
    // season has no art of its own).
    #[derive(Deserialize)]
    struct ShowInfo {
        poster_path: Option<String>,
        #[serde(default)]
        external_ids: Option<ShowExternalIds>,
    }
    #[derive(Deserialize)]
    struct ShowExternalIds {
        imdb_id: Option<String>,
        tvdb_id: Option<u64>,
    }
    let show: ShowInfo = tmdb
        .get(
            &format!("/tv/{show_tmdb_id}"),
            &[("append_to_response", "external_ids")],
        )
        .await?;

    let imdb_id = hint_imdb_id
        .or_else(|| show.external_ids.as_ref().and_then(|e| e.imdb_id.clone()))
        .filter(|s| !s.is_empty());
    let tvdb_id = show.external_ids.as_ref().and_then(|e| e.tvdb_id);

    let season_poster = details.poster_path.clone();
    // Use the season's own poster; fall back to the series poster if absent.
    let poster_path = season_poster.clone().or(show.poster_path);

    Ok(ResolvedId {
        imdb_id,
        tmdb_id: show_tmdb_id,
        tvdb_id,
        media_type: MediaType::Season,
        poster_path,
        release_date: details.air_date,
        episode: None,
        season: Some(SeasonInfo {
            show_tmdb_id,
            season_number: season,
            poster_path: season_poster,
        }),
    })
}

/// Parse `season-{show_id}-S{season}` and resolve via TMDB.
async fn resolve_tmdb_season(rest: &str, id_value: &str, tmdb: &TmdbClient) -> Result<ResolvedId, AppError> {
    let (show_id, season) = parse_season_id(rest, id_value)?;
    resolve_season_details(tmdb, show_id, season, None).await
}

/// Resolve `season-{series_imdb_id}-S{season}` by first looking up the series
/// via TMDB's Find API, then fetching season details.
async fn resolve_imdb_season(
    rest: &str,
    id_value: &str,
    tmdb: &TmdbClient,
) -> Result<ResolvedId, AppError> {
    let (series_imdb_id, season): (&str, u32) = parse_season_external(rest, id_value)?;

    let result: FindResult = tmdb
        .get(
            &format!("/find/{series_imdb_id}"),
            &[("external_source", "imdb_id")],
        )
        .await?;

    let show_tmdb_id = result.tv_results.first().map(|tv| tv.id).ok_or_else(|| {
        AppError::IdNotFound(format!("{series_imdb_id} (not found as a TV series on TMDB)"))
    })?;

    resolve_season_details(tmdb, show_tmdb_id, season, Some(series_imdb_id.to_string())).await
}

/// Resolve `season-{series_tvdb_id}-S{season}` by first looking up the series
/// via TMDB's Find API, then fetching season details.
async fn resolve_tvdb_season(
    rest: &str,
    id_value: &str,
    tmdb: &TmdbClient,
) -> Result<ResolvedId, AppError> {
    let (series_tvdb_id, season) = parse_season_external(rest, id_value)?;

    let result: FindResult = tmdb
        .get(
            &format!("/find/{series_tvdb_id}"),
            &[("external_source", "tvdb_id")],
        )
        .await?;

    let show_tmdb_id = result.tv_results.first().map(|tv| tv.id).ok_or_else(|| {
        AppError::IdNotFound(format!(
            "{series_tvdb_id} (not found as a TV series on TMDB via TVDB lookup)"
        ))
    })?;

    resolve_season_details(tmdb, show_tmdb_id, season, None).await
}

/// Parse `{external_id}-S{season}E{episode}` for IMDb/TVDB episode lookups.
/// Unlike `parse_episode_id`, the ID portion is returned as a string slice
/// because IMDb IDs like `tt1234567` aren't purely numeric.
fn parse_episode_external<'a>(rest: &'a str, id_value: &str) -> Result<(&'a str, u32, u32), AppError> {
    let upper = rest.to_ascii_uppercase();
    let split_pos = upper.find("-S").ok_or_else(|| {
        AppError::InvalidIdType(format!(
            "episode id must be episode-{{id}}-S{{season}}E{{episode}}: {id_value}"
        ))
    })?;
    let external_id = &rest[..split_pos];
    if external_id.is_empty() {
        return Err(AppError::InvalidIdType(format!(
            "episode id must be episode-{{id}}-S{{season}}E{{episode}}: {id_value}"
        )));
    }
    let se_str = &upper[split_pos + 2..]; // skip "-S"
    let se_parts: Vec<&str> = se_str.splitn(2, 'E').collect();
    if se_parts.len() != 2 {
        return Err(AppError::InvalidIdType(format!(
            "episode id must be episode-{{id}}-S{{season}}E{{episode}}: {id_value}"
        )));
    }
    let season = se_parts[0]
        .parse::<u32>()
        .map_err(|_| AppError::InvalidIdType(id_value.to_string()))?;
    let episode = se_parts[1]
        .parse::<u32>()
        .map_err(|_| AppError::InvalidIdType(id_value.to_string()))?;
    if season > 10_000 || episode > 100_000 {
        return Err(AppError::BadRequest(
            "season must be ≤ 10 000 and episode must be ≤ 100 000".into(),
        ));
    }
    Ok((external_id, season, episode))
}

/// Resolve `episode-{series_imdb_id}-S{season}E{episode}` by first looking up
/// the series via TMDB's Find API, then fetching episode details.
async fn resolve_imdb_episode(
    rest: &str,
    id_value: &str,
    tmdb: &TmdbClient,
) -> Result<ResolvedId, AppError> {
    let (series_imdb_id, season, episode): (&str, u32, u32) =
        parse_episode_external(rest, id_value)?;

    let result: FindResult = tmdb
        .get(
            &format!("/find/{series_imdb_id}"),
            &[("external_source", "imdb_id")],
        )
        .await?;

    let show_tmdb_id = result
        .tv_results
        .first()
        .map(|tv| tv.id)
        .ok_or_else(|| {
            AppError::IdNotFound(format!(
                "{series_imdb_id} (not found as a TV series on TMDB)"
            ))
        })?;

    resolve_episode_details(tmdb, show_tmdb_id, season, episode, None, None).await
}

/// Resolve `episode-{series_tvdb_id}-S{season}E{episode}` by first looking up
/// the series via TMDB's Find API, then fetching episode details.
async fn resolve_tvdb_episode(
    rest: &str,
    id_value: &str,
    tmdb: &TmdbClient,
) -> Result<ResolvedId, AppError> {
    let (series_tvdb_id, season, episode) = parse_episode_external(rest, id_value)?;

    let result: FindResult = tmdb
        .get(
            &format!("/find/{series_tvdb_id}"),
            &[("external_source", "tvdb_id")],
        )
        .await?;

    let show_tmdb_id = result
        .tv_results
        .first()
        .map(|tv| tv.id)
        .ok_or_else(|| {
            AppError::IdNotFound(format!(
                "{series_tvdb_id} (not found as a TV series on TMDB via TVDB lookup)"
            ))
        })?;

    resolve_episode_details(tmdb, show_tmdb_id, season, episode, None, None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_imdb() {
        assert_eq!(IdType::parse("imdb").unwrap(), IdType::Imdb);
    }

    #[test]
    fn parse_tmdb() {
        assert_eq!(IdType::parse("tmdb").unwrap(), IdType::Tmdb);
    }

    #[test]
    fn parse_tvdb() {
        assert_eq!(IdType::parse("tvdb").unwrap(), IdType::Tvdb);
    }

    #[test]
    fn parse_invalid_id_type() {
        assert!(IdType::parse("invalid").is_err());
    }

    #[test]
    fn parse_empty_string() {
        assert!(IdType::parse("").is_err());
    }

    #[test]
    fn parse_case_sensitive() {
        // Should not accept uppercase
        assert!(IdType::parse("IMDB").is_err());
        assert!(IdType::parse("Tmdb").is_err());
    }

    #[test]
    fn format_tmdb_id_value_movie() {
        assert_eq!(format_tmdb_id_value(278, &MediaType::Movie, None, None), "movie-278");
    }

    #[test]
    fn format_tmdb_id_value_tv() {
        assert_eq!(format_tmdb_id_value(1396, &MediaType::Tv, None, None), "series-1396");
    }

    #[test]
    fn format_tmdb_id_value_episode() {
        let ep = EpisodeInfo {
            show_tmdb_id: 1396,
            season_number: 1,
            episode_number: 1,
            still_path: None,
        };
        assert_eq!(
            format_tmdb_id_value(1396, &MediaType::Episode, Some(&ep), None),
            "episode-1396-S1E1"
        );
    }

    #[test]
    fn format_tmdb_id_value_episode_large_numbers() {
        let ep = EpisodeInfo {
            show_tmdb_id: 456,
            season_number: 12,
            episode_number: 24,
            still_path: Some("/still.jpg".into()),
        };
        assert_eq!(
            format_tmdb_id_value(456, &MediaType::Episode, Some(&ep), None),
            "episode-456-S12E24"
        );
    }

    #[test]
    fn format_tmdb_id_value_episode_none_falls_back() {
        // Episode with missing EpisodeInfo should not panic, falls back to series format
        assert_eq!(
            format_tmdb_id_value(1396, &MediaType::Episode, None, None),
            "series-1396"
        );
    }

    #[test]
    fn format_tmdb_id_value_season() {
        let s = SeasonInfo {
            show_tmdb_id: 1396,
            season_number: 2,
            poster_path: None,
        };
        assert_eq!(
            format_tmdb_id_value(1396, &MediaType::Season, None, Some(&s)),
            "season-1396-S2"
        );
    }

    #[test]
    fn format_tmdb_id_value_season_large_numbers() {
        let s = SeasonInfo {
            show_tmdb_id: 456,
            season_number: 12,
            poster_path: Some("/poster.jpg".into()),
        };
        assert_eq!(
            format_tmdb_id_value(456, &MediaType::Season, None, Some(&s)),
            "season-456-S12"
        );
    }

    #[test]
    fn format_tmdb_id_value_season_none_falls_back() {
        // Season with missing SeasonInfo should not panic, falls back to series format
        assert_eq!(
            format_tmdb_id_value(1396, &MediaType::Season, None, None),
            "series-1396"
        );
    }

    // --- parse_season_id ---

    #[test]
    fn parse_season_id_valid() {
        let (show, season) = parse_season_id("1396-S1", "season-1396-S1").unwrap();
        assert_eq!(show, 1396);
        assert_eq!(season, 1);
    }

    #[test]
    fn parse_season_id_multi_digit() {
        let (show, season) = parse_season_id("456-S12", "season-456-S12").unwrap();
        assert_eq!(show, 456);
        assert_eq!(season, 12);
    }

    #[test]
    fn parse_season_id_missing_season() {
        assert!(parse_season_id("1396", "season-1396").is_err());
    }

    #[test]
    fn parse_season_id_non_numeric_show() {
        assert!(parse_season_id("abc-S1", "season-abc-S1").is_err());
    }

    #[test]
    fn parse_season_id_non_numeric_season() {
        assert!(parse_season_id("1396-Sa", "season-1396-Sa").is_err());
    }

    #[test]
    fn parse_season_id_rejects_episode_syntax() {
        // An episode-style id must not parse as a season.
        assert!(parse_season_id("1396-S1E1", "season-1396-S1E1").is_err());
    }

    #[test]
    fn parse_season_id_lowercase() {
        let (show, season) = parse_season_id("1396-s1", "season-1396-s1").unwrap();
        assert_eq!(show, 1396);
        assert_eq!(season, 1);
    }

    // --- parse_season_external ---

    #[test]
    fn parse_season_external_imdb() {
        let (id, season) = parse_season_external("tt14786934-S1", "season-tt14786934-S1").unwrap();
        assert_eq!(id, "tt14786934");
        assert_eq!(season, 1);
    }

    #[test]
    fn parse_season_external_tvdb_numeric() {
        let (id, season) = parse_season_external("81189-S3", "season-81189-S3").unwrap();
        assert_eq!(id, "81189");
        assert_eq!(season, 3);
    }

    #[test]
    fn parse_season_external_lowercase() {
        let (id, season) = parse_season_external("tt14786934-s2", "season-tt14786934-s2").unwrap();
        assert_eq!(id, "tt14786934");
        assert_eq!(season, 2);
    }

    #[test]
    fn parse_season_external_zero_value() {
        let (id, season) = parse_season_external("tt14786934-S0", "season-tt14786934-S0").unwrap();
        assert_eq!(id, "tt14786934");
        assert_eq!(season, 0);
    }

    #[test]
    fn parse_season_external_at_boundary() {
        let (id, season) = parse_season_external("81189-S10000", "season-81189-S10000").unwrap();
        assert_eq!(id, "81189");
        assert_eq!(season, 10_000);
    }

    #[test]
    fn parse_season_external_over_boundary() {
        assert!(parse_season_external("81189-S10001", "season-81189-S10001").is_err());
    }

    #[test]
    fn parse_season_external_missing_season() {
        assert!(parse_season_external("tt14786934", "season-tt14786934").is_err());
    }

    #[test]
    fn parse_season_external_empty_id() {
        assert!(parse_season_external("-S1", "season--S1").is_err());
    }

    #[test]
    fn parse_season_id_rejects_imdb_style_id() {
        assert!(parse_season_id("tt1234-S1", "season-tt1234-S1").is_err());
    }

    // --- parse_episode_id ---

    #[test]
    fn parse_episode_id_valid() {
        let (show, season, ep) = parse_episode_id("1396-S1E1", "episode-1396-S1E1").unwrap();
        assert_eq!(show, 1396);
        assert_eq!(season, 1);
        assert_eq!(ep, 1);
    }

    #[test]
    fn parse_episode_id_multi_digit() {
        let (show, season, ep) = parse_episode_id("456-S12E24", "episode-456-S12E24").unwrap();
        assert_eq!(show, 456);
        assert_eq!(season, 12);
        assert_eq!(ep, 24);
    }

    #[test]
    fn parse_episode_id_missing_season() {
        assert!(parse_episode_id("1396", "episode-1396").is_err());
    }

    #[test]
    fn parse_episode_id_missing_episode() {
        assert!(parse_episode_id("1396-S1", "episode-1396-S1").is_err());
    }

    #[test]
    fn parse_episode_id_non_numeric_show() {
        assert!(parse_episode_id("abc-S1E1", "episode-abc-S1E1").is_err());
    }

    #[test]
    fn parse_episode_id_non_numeric_season() {
        assert!(parse_episode_id("1396-SaE1", "episode-1396-SaE1").is_err());
    }

    #[test]
    fn parse_episode_id_non_numeric_episode() {
        assert!(parse_episode_id("1396-S1Eb", "episode-1396-S1Eb").is_err());
    }

    #[test]
    fn parse_episode_id_lowercase() {
        let (show, season, ep) = parse_episode_id("1396-s1e1", "episode-1396-s1e1").unwrap();
        assert_eq!(show, 1396);
        assert_eq!(season, 1);
        assert_eq!(ep, 1);
    }

    #[test]
    fn parse_episode_id_mixed_case() {
        let (show, season, ep) = parse_episode_id("456-s12E24", "episode-456-s12E24").unwrap();
        assert_eq!(show, 456);
        assert_eq!(season, 12);
        assert_eq!(ep, 24);
    }

    // --- parse_episode_external ---

    #[test]
    fn parse_episode_external_imdb() {
        let (id, season, ep) =
            parse_episode_external("tt14786934-S1E1", "episode-tt14786934-S1E1").unwrap();
        assert_eq!(id, "tt14786934");
        assert_eq!(season, 1);
        assert_eq!(ep, 1);
    }

    #[test]
    fn parse_episode_external_tvdb_numeric() {
        let (id, season, ep) =
            parse_episode_external("81189-S3E5", "episode-81189-S3E5").unwrap();
        assert_eq!(id, "81189");
        assert_eq!(season, 3);
        assert_eq!(ep, 5);
    }

    #[test]
    fn parse_episode_external_lowercase() {
        let (id, season, ep) =
            parse_episode_external("tt14786934-s2e10", "episode-tt14786934-s2e10").unwrap();
        assert_eq!(id, "tt14786934");
        assert_eq!(season, 2);
        assert_eq!(ep, 10);
    }

    #[test]
    fn parse_episode_external_missing_season() {
        assert!(parse_episode_external("tt14786934", "episode-tt14786934").is_err());
    }

    #[test]
    fn parse_episode_external_missing_episode() {
        assert!(parse_episode_external("tt14786934-S1", "episode-tt14786934-S1").is_err());
    }

    #[test]
    fn parse_episode_external_zero_values() {
        let (id, season, ep) =
            parse_episode_external("tt14786934-S0E0", "episode-tt14786934-S0E0").unwrap();
        assert_eq!(id, "tt14786934");
        assert_eq!(season, 0);
        assert_eq!(ep, 0);
    }

    #[test]
    fn parse_episode_external_at_boundary() {
        let (id, season, ep) =
            parse_episode_external("81189-S10000E100000", "episode-81189-S10000E100000").unwrap();
        assert_eq!(id, "81189");
        assert_eq!(season, 10_000);
        assert_eq!(ep, 100_000);
    }

    #[test]
    fn parse_episode_external_over_boundary() {
        assert!(parse_episode_external("81189-S10001E1", "episode-81189-S10001E1").is_err());
        assert!(parse_episode_external("81189-S1E100001", "episode-81189-S1E100001").is_err());
    }

    #[test]
    fn parse_episode_external_empty_id() {
        assert!(parse_episode_external("-S1E1", "episode--S1E1").is_err());
    }

    #[test]
    fn parse_episode_id_rejects_imdb_style_id() {
        assert!(parse_episode_id("tt1234-S1E1", "episode-tt1234-S1E1").is_err());
    }
}

async fn resolve_tvdb(tvdb_id: &str, tmdb: &TmdbClient) -> Result<ResolvedId, AppError> {
    // Handle episode format: episode-{series_tvdb_id}-S{season}E{episode}
    if let Some(rest) = tvdb_id.strip_prefix("episode-") {
        return resolve_tvdb_episode(rest, tvdb_id, tmdb).await;
    }

    // Handle season format: season-{series_tvdb_id}-S{season}
    if let Some(rest) = tvdb_id.strip_prefix("season-") {
        return resolve_tvdb_season(rest, tvdb_id, tmdb).await;
    }

    let tvdb_id_num = tvdb_id.parse::<u64>().ok();
    let result: FindResult = tmdb
        .get(&format!("/find/{tvdb_id}"), &[("external_source", "tvdb_id")])
        .await?;

    // Check episode results first — a TVDB episode ID is unambiguous.
    if let Some(ep) = result.tv_episode_results.first() {
        return resolve_episode_details(tmdb, ep.show_id, ep.season_number, ep.episode_number, ep.still_path.clone(), None).await;
    }

    if let Some(tv) = result.tv_results.first() {
        // We need to fetch details to get the imdb_id
        #[derive(Deserialize)]
        struct TvDetails {
            external_ids: Option<TvExternalIds>,
            poster_path: Option<String>,
            first_air_date: Option<String>,
        }
        #[derive(Deserialize)]
        struct TvExternalIds {
            imdb_id: Option<String>,
        }
        let details: TvDetails = tmdb
            .get(
                &format!("/tv/{}", tv.id),
                &[("append_to_response", "external_ids")],
            )
            .await?;
        return Ok(ResolvedId {
            imdb_id: details
                .external_ids
                .and_then(|e| e.imdb_id)
                .filter(|s| !s.is_empty()),
            tmdb_id: tv.id,
            tvdb_id: tvdb_id_num,
            media_type: MediaType::Tv,
            poster_path: details.poster_path.or_else(|| tv.poster_path.clone()),
            release_date: details.first_air_date,
            episode: None,
            season: None,
        });
    }
    if let Some(movie) = result.movie_results.first() {
        #[derive(Deserialize)]
        struct MovieDetails {
            imdb_id: Option<String>,
            poster_path: Option<String>,
            release_date: Option<String>,
        }
        let details: MovieDetails = tmdb
            .get(&format!("/movie/{}", movie.id), &[])
            .await?;
        return Ok(ResolvedId {
            imdb_id: details.imdb_id.filter(|s| !s.is_empty()),
            tmdb_id: movie.id,
            tvdb_id: tvdb_id_num,
            media_type: MediaType::Movie,
            poster_path: details.poster_path.or_else(|| movie.poster_path.clone()),
            release_date: details.release_date,
            episode: None,
            season: None,
        });
    }
    Err(AppError::IdNotFound(format!("{tvdb_id} (not found on TMDB via TVDB lookup)")))
}
