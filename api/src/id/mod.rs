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
}

#[derive(Debug, Clone)]
pub struct ResolvedId {
    pub imdb_id: Option<String>,
    pub tmdb_id: u64,
    pub media_type: MediaType,
    pub poster_path: Option<String>,
    pub release_date: Option<String>,
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
}

#[derive(Debug, Deserialize)]
struct FindResult {
    #[serde(default)]
    movie_results: Vec<FindEntry>,
    #[serde(default)]
    tv_results: Vec<FindEntry>,
}

#[derive(Debug, Deserialize)]
struct FindEntry {
    id: u64,
    poster_path: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
}

pub async fn resolve(
    id_type: IdType,
    id_value: &str,
    tmdb: &TmdbClient,
) -> Result<ResolvedId, AppError> {
    match id_type {
        IdType::Imdb => resolve_imdb(id_value, tmdb).await,
        IdType::Tmdb => resolve_tmdb(id_value, tmdb).await,
        IdType::Tvdb => resolve_tvdb(id_value, tmdb).await,
    }
}

async fn resolve_imdb(imdb_id: &str, tmdb: &TmdbClient) -> Result<ResolvedId, AppError> {
    let result: FindResult = tmdb
        .get(&format!("/find/{imdb_id}"), &[("external_source", "imdb_id")])
        .await?;

    if let Some(movie) = result.movie_results.first() {
        return Ok(ResolvedId {
            imdb_id: Some(imdb_id.to_string()),
            tmdb_id: movie.id,
            media_type: MediaType::Movie,
            poster_path: movie.poster_path.clone(),
            release_date: movie.release_date.clone(),
        });
    }
    if let Some(tv) = result.tv_results.first() {
        return Ok(ResolvedId {
            imdb_id: Some(imdb_id.to_string()),
            tmdb_id: tv.id,
            media_type: MediaType::Tv,
            poster_path: tv.poster_path.clone(),
            release_date: tv.first_air_date.clone(),
        });
    }
    Err(AppError::IdNotFound(imdb_id.to_string()))
}

async fn resolve_tmdb(id_value: &str, tmdb: &TmdbClient) -> Result<ResolvedId, AppError> {
    let (media_type, tmdb_id) = if let Some(rest) = id_value.strip_prefix("movie-") {
        (MediaType::Movie, rest.parse::<u64>().map_err(|_| AppError::InvalidIdType(id_value.to_string()))?)
    } else if let Some(rest) = id_value.strip_prefix("series-") {
        (MediaType::Tv, rest.parse::<u64>().map_err(|_| AppError::InvalidIdType(id_value.to_string()))?)
    } else {
        return Err(AppError::InvalidIdType(format!(
            "tmdb id must be prefixed with movie- or series-: {id_value}"
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
    }

    let path = match media_type {
        MediaType::Movie => format!("/movie/{tmdb_id}"),
        MediaType::Tv => format!("/tv/{tmdb_id}?append_to_response=external_ids"),
    };
    let details: Details = tmdb.get(&path, &[]).await?;

    let imdb_id = details
        .imdb_id
        .or_else(|| details.external_ids.and_then(|e| e.imdb_id));

    let release_date = match media_type {
        MediaType::Movie => details.release_date,
        MediaType::Tv => details.first_air_date,
    };

    Ok(ResolvedId {
        imdb_id,
        tmdb_id,
        media_type,
        poster_path: details.poster_path,
        release_date,
    })
}

async fn resolve_tvdb(tvdb_id: &str, tmdb: &TmdbClient) -> Result<ResolvedId, AppError> {
    let result: FindResult = tmdb
        .get(&format!("/find/{tvdb_id}"), &[("external_source", "tvdb_id")])
        .await?;

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
                .and_then(|e| e.imdb_id),
            tmdb_id: tv.id,
            media_type: MediaType::Tv,
            poster_path: details.poster_path.or_else(|| tv.poster_path.clone()),
            release_date: details.first_air_date,
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
            imdb_id: details.imdb_id,
            tmdb_id: movie.id,
            media_type: MediaType::Movie,
            poster_path: details.poster_path.or_else(|| movie.poster_path.clone()),
            release_date: details.release_date,
        });
    }
    Err(AppError::IdNotFound(tvdb_id.to_string()))
}
