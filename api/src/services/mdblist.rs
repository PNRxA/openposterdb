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
    pub score: Option<i32>,
    pub votes: Option<i64>,
}

impl MdblistClient {
    pub fn new(api_key: String, http: reqwest::Client) -> Self {
        Self { api_key: Arc::new(Zeroizing::new(api_key)), http }
    }

    pub async fn get_ratings(
        &self,
        imdb_id: &str,
        media_type: &MediaType,
    ) -> Result<MdblistResponse, AppError> {
        let kind = match media_type {
            MediaType::Movie => "movie",
            MediaType::Tv => "show",
        };

        let url = format!("https://api.mdblist.com/imdb/{kind}/{imdb_id}");

        let resp = retry::send_with_retry(&MDBLIST_RETRY, || {
            self.http
                .get(&url)
                .query(&[("apikey", self.api_key.as_str())])
                .send()
        })
        .await?
        .error_for_status()?;

        Ok(resp.json().await?)
    }
}
