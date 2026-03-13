use std::sync::Arc;

use crate::error::AppError;
use crate::services::retry::{self, TMDB_API_RETRY, TMDB_CDN_RETRY};
use serde::de::DeserializeOwned;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct TmdbClient {
    api_key: Arc<Zeroizing<String>>,
    http: reqwest::Client,
}

impl TmdbClient {
    pub fn new(api_key: String, http: reqwest::Client) -> Self {
        Self { api_key: Arc::new(Zeroizing::new(api_key)), http }
    }

    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<T, AppError> {
        let url = format!("https://api.themoviedb.org/3{path}");
        let resp = retry::send_with_retry(&TMDB_API_RETRY, || {
            let mut req = self.http.get(&url).query(&[("api_key", self.api_key.as_str())]);
            if !params.is_empty() {
                req = req.query(params);
            }
            req.send()
        })
        .await?
        .error_for_status()?;
        Ok(resp.json().await?)
    }

    pub async fn fetch_poster_bytes(&self, poster_path: &str, tmdb_size: &str) -> Result<Vec<u8>, AppError> {
        let url = format!("https://image.tmdb.org/t/p/{tmdb_size}{poster_path}");
        let resp = retry::send_with_retry(&TMDB_CDN_RETRY, || self.http.get(&url).send())
            .await?
            .error_for_status()?;
        Ok(resp.bytes().await?.to_vec())
    }

    /// Fetch poster bytes with If-Modified-Since. Returns `None` on 304 Not Modified.
    pub async fn fetch_poster_bytes_conditional(
        &self,
        poster_path: &str,
        tmdb_size: &str,
        if_modified_since: Option<std::time::SystemTime>,
    ) -> Result<Option<Vec<u8>>, AppError> {
        let url = format!("https://image.tmdb.org/t/p/{tmdb_size}{poster_path}");
        let since_header = if_modified_since.map(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
        });
        let resp = retry::send_with_retry(&TMDB_CDN_RETRY, || {
            let mut r = self.http.get(&url);
            if let Some(ref h) = since_header {
                r = r.header(reqwest::header::IF_MODIFIED_SINCE, h.as_str());
            }
            r.send()
        })
        .await?;
        if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        let resp = resp.error_for_status()?;
        Ok(Some(resp.bytes().await?.to_vec()))
    }
}
