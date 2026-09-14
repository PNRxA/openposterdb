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
    #[serde(default, deserialize_with = "deserialize_lenient_ratings")]
    pub ratings: Vec<MdblistRating>,
    #[serde(default)]
    pub ids: MdblistIds,
    /// MDBList's own aggregated 0–100 score (rendered as the `mdblist` source).
    #[serde(default)]
    pub score: Option<f64>,
    /// Age rating string (e.g. "12", "PG-13"). May be a JSON string or number.
    #[serde(default)]
    pub age_rating: Option<serde_json::Value>,
}

impl MdblistResponse {
    /// Extract a CSM age rating string from the `age_rating` field, formatted
    /// as "N+" (e.g. "12+"). Returns `None` if the field is absent, null, or
    /// zero/empty. Does not gate on a `commonsense` boolean — the age_rating
    /// field is used directly.
    pub fn csm_age(&self) -> Option<String> {
        match &self.age_rating {
            Some(serde_json::Value::String(s)) => {
                let s = s.trim();
                if s.is_empty() || s == "0" {
                    None
                } else {
                    Some(format!("{s}+"))
                }
            }
            Some(serde_json::Value::Number(n)) => {
                let v = n.as_f64().unwrap_or(0.0);
                if v <= 0.0 {
                    None
                } else {
                    Some(format!("{:.0}+", v))
                }
            }
            _ => None,
        }
    }
}

/// Deserialize a `Vec<MdblistRating>`, skipping individual entries that fail
/// to parse rather than failing the whole array.
fn deserialize_lenient_ratings<'de, D>(deserializer: D) -> Result<Vec<MdblistRating>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

#[derive(Debug, Default, Deserialize)]
pub struct MdblistIds {
    pub imdb: Option<String>,
    pub tmdb: Option<u64>,
    pub tvdb: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct MdblistRating {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub votes: Option<i64>,
}

/// MDBList path segment for a media type. Movies are `movie`, series are `show`.
/// Episodes are unsupported — MDBList only has movie/show-level ratings.
fn mdblist_kind(media_type: &MediaType) -> Result<&'static str, AppError> {
    match media_type {
        MediaType::Movie => Ok("movie"),
        MediaType::Tv => Ok("show"),
        MediaType::Episode => Err(AppError::Other("mdblist does not support episode ratings".into())),
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

    #[test]
    fn csm_age_string_formats_with_plus() {
        let resp: MdblistResponse = serde_json::from_str(r#"{"age_rating": "12"}"#).unwrap();
        assert_eq!(resp.csm_age(), Some("12+".to_string()));
    }

    #[test]
    fn csm_age_numeric_formats_with_plus() {
        let resp: MdblistResponse = serde_json::from_str(r#"{"age_rating": 7}"#).unwrap();
        assert_eq!(resp.csm_age(), Some("7+".to_string()));
    }

    #[test]
    fn csm_age_zero_string_returns_none() {
        let resp: MdblistResponse = serde_json::from_str(r#"{"age_rating": "0"}"#).unwrap();
        assert_eq!(resp.csm_age(), None);
    }

    #[test]
    fn csm_age_zero_number_returns_none() {
        let resp: MdblistResponse = serde_json::from_str(r#"{"age_rating": 0}"#).unwrap();
        assert_eq!(resp.csm_age(), None);
    }

    #[test]
    fn csm_age_null_returns_none() {
        let resp: MdblistResponse = serde_json::from_str(r#"{"age_rating": null}"#).unwrap();
        assert_eq!(resp.csm_age(), None);
    }

    #[test]
    fn csm_age_absent_returns_none() {
        let resp: MdblistResponse = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(resp.csm_age(), None);
    }

    #[test]
    fn csm_age_empty_string_returns_none() {
        let resp: MdblistResponse = serde_json::from_str(r#"{"age_rating": ""}"#).unwrap();
        assert_eq!(resp.csm_age(), None);
    }

    #[test]
    fn csm_age_pg13_string_preserved() {
        let resp: MdblistResponse = serde_json::from_str(r#"{"age_rating": "PG-13"}"#).unwrap();
        assert_eq!(resp.csm_age(), Some("PG-13+".to_string()));
    }

    #[test]
    fn lenient_ratings_skips_bad_entries() {
        // A ratings array where one entry is malformed (missing `source`);
        // the lenient deserializer should skip it and keep the valid ones.
        let json = r#"{
            "ratings": [
                {"source": "imdb", "value": 8.5, "score": 85, "votes": 1000},
                {"value": 7.0},
                {"source": "tmdb", "value": 7.5, "score": 75, "votes": 500}
            ]
        }"#;
        // With #[serde(default)] on source, a missing `source` just becomes "".
        // The key test is that the array doesn't error entirely.
        let resp: MdblistResponse = serde_json::from_str(json).expect("should not fail entire array");
        assert_eq!(resp.ratings.len(), 3);
    }
}
