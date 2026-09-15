use std::sync::Arc;

use crate::error::AppError;
use crate::id::MediaType;
use crate::services::retry::{self, MDBLIST_RETRY};
use serde::Deserialize;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct MdblistClient {
    api_key: Arc<Zeroizing<String>>,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
pub struct MdblistResponse {
    #[serde(default)]
    pub ratings: Vec<MdblistRating>,
    #[serde(default)]
    pub ids: MdblistIds,
    /// MDBList's own aggregated 0–100 score (rendered as the `mdblist` source).
    #[serde(default)]
    pub score: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct MdblistIds {
    pub imdb: Option<String>,
    pub tmdb: Option<u64>,
    pub tvdb: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct MdblistRating {
    pub source: String,
    pub value: Option<f64>,
    pub score: Option<f64>,
    pub votes: Option<i64>,
}

/// MDBList path segment for a media type. Movies are `movie`, series are `show`.
/// Episodes are unsupported — MDBList only has movie/show-level ratings.
fn mdblist_kind(media_type: &MediaType) -> Result<&'static str, AppError> {
    match media_type {
        MediaType::Movie => Ok("movie"),
        MediaType::Tv => Ok("show"),
        MediaType::Episode => Err(AppError::Other("mdblist does not support episode ratings".into())),
        MediaType::Season => Err(AppError::Other("mdblist does not support season ratings".into())),
    }
}

/// Build the MDBList ratings URL for an IMDb-keyed lookup.
fn imdb_ratings_url(kind: &str, imdb_id: &str) -> String {
    format!("https://api.mdblist.com/imdb/{kind}/{imdb_id}")
}

/// Build the MDBList ratings URL for a TMDB-keyed lookup.
///
/// Used as a fallback for titles (notably anime) that TMDB knows but hasn't
/// cross-referenced to IMDb. The TMDB endpoint returns the same full rating set
/// as the IMDb endpoint — including MyAnimeList — so titles with no IMDb id keep
/// their badges instead of collapsing to the TMDB vote_average alone. (issue #14)
fn tmdb_ratings_url(kind: &str, tmdb_id: u64) -> String {
    format!("https://api.mdblist.com/tmdb/{kind}/{tmdb_id}")
}

impl MdblistClient {
    pub fn new(api_key: String, http: reqwest::Client) -> Self {
        Self { api_key: Arc::new(Zeroizing::new(api_key)), http }
    }

    /// Fetch and deserialize an MDBList ratings response from a fully-built URL.
    async fn fetch(&self, url: &str) -> Result<MdblistResponse, AppError> {
        let resp = retry::send_with_retry(&MDBLIST_RETRY, || {
            self.http
                .get(url)
                .query(&[("apikey", self.api_key.as_str())])
                .send()
        })
        .await?
        .error_for_status()?;

        Ok(resp.json().await?)
    }

    /// Fetch ratings keyed by IMDb id.
    pub async fn get_ratings(
        &self,
        imdb_id: &str,
        media_type: &MediaType,
    ) -> Result<MdblistResponse, AppError> {
        let kind = mdblist_kind(media_type)?;
        self.fetch(&imdb_ratings_url(kind, imdb_id)).await
    }

    /// Fetch ratings keyed by TMDB id. Used when a title has no IMDb id so the
    /// IMDb-keyed endpoint can't be reached (issue #14).
    pub async fn get_ratings_by_tmdb(
        &self,
        tmdb_id: u64,
        media_type: &MediaType,
    ) -> Result<MdblistResponse, AppError> {
        let kind = mdblist_kind(media_type)?;
        self.fetch(&tmdb_ratings_url(kind, tmdb_id)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mdblist_kind_maps_movie_and_tv() {
        assert_eq!(mdblist_kind(&MediaType::Movie).unwrap(), "movie");
        assert_eq!(mdblist_kind(&MediaType::Tv).unwrap(), "show");
    }

    #[test]
    fn mdblist_kind_rejects_episode() {
        assert!(mdblist_kind(&MediaType::Episode).is_err());
    }

    #[test]
    fn imdb_ratings_url_format() {
        assert_eq!(imdb_ratings_url("show", "tt2560140"), "https://api.mdblist.com/imdb/show/tt2560140");
        assert_eq!(imdb_ratings_url("movie", "tt0111161"), "https://api.mdblist.com/imdb/movie/tt0111161");
    }

    #[test]
    fn tmdb_ratings_url_format() {
        // The TMDB fallback endpoint that restores ratings for IMDb-less titles.
        assert_eq!(tmdb_ratings_url("show", 1429), "https://api.mdblist.com/tmdb/show/1429");
        assert_eq!(tmdb_ratings_url("movie", 550), "https://api.mdblist.com/tmdb/movie/550");
    }
}
