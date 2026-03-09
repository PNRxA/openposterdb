use crate::error::AppError;
use crate::id::MediaType;
use serde::Deserialize;

#[derive(Clone)]
pub struct MdblistClient {
    api_key: String,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
pub struct MdblistResponse {
    #[serde(default)]
    pub ratings: Vec<MdblistRating>,
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
        Self { api_key, http }
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

        let resp = self
            .http
            .get(&url)
            .query(&[("apikey", &self.api_key)])
            .send()
            .await?
            .error_for_status()?;

        Ok(resp.json().await?)
    }
}
