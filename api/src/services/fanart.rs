use std::sync::Arc;

use crate::error::AppError;
use crate::services::retry::{self, FANART_RETRY};
use serde::Deserialize;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct FanartClient {
    api_key: Arc<Zeroizing<String>>,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FanartPoster {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub lang: String,
    #[serde(default)]
    pub likes: String,
}

/// Deserialize a `Vec<FanartPoster>`, skipping individual entries that fail
/// to parse rather than failing the whole array.
fn deserialize_lenient_posters<'de, D>(deserializer: D) -> Result<Vec<FanartPoster>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

/// All image types fetched from fanart.tv in a single API call.
#[derive(Debug, Clone)]
pub struct FanartImages {
    pub posters: Vec<FanartPoster>,
    pub logos: Vec<FanartPoster>,
    pub backdrops: Vec<FanartPoster>,
}

#[derive(Debug, Deserialize)]
struct MovieImages {
    #[serde(default, deserialize_with = "deserialize_lenient_posters")]
    movieposter: Vec<FanartPoster>,
    #[serde(default, deserialize_with = "deserialize_lenient_posters")]
    hdmovielogo: Vec<FanartPoster>,
    #[serde(default, deserialize_with = "deserialize_lenient_posters")]
    moviebackground: Vec<FanartPoster>,
}

#[derive(Debug, Deserialize)]
struct TvImages {
    #[serde(default, deserialize_with = "deserialize_lenient_posters")]
    tvposter: Vec<FanartPoster>,
    #[serde(default, deserialize_with = "deserialize_lenient_posters")]
    hdtvlogo: Vec<FanartPoster>,
    #[serde(default, deserialize_with = "deserialize_lenient_posters")]
    showbackground: Vec<FanartPoster>,
}

/// Which tier the selected poster came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosterMatch {
    Textless,
    Language,
}

impl FanartClient {
    pub fn new(api_key: String, http: reqwest::Client) -> Self {
        Self { api_key: Arc::new(Zeroizing::new(api_key)), http }
    }

    pub async fn get_movie_images(&self, tmdb_id: u64) -> Result<FanartImages, AppError> {
        let url = format!(
            "https://webservice.fanart.tv/v3/movies/{tmdb_id}?api_key={}",
            self.api_key.as_str()
        );
        let resp = retry::send_with_retry(&FANART_RETRY, || self.http.get(&url).send()).await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(FanartImages { posters: vec![], logos: vec![], backdrops: vec![] });
        }
        let resp = resp.error_for_status()?;
        let images: MovieImages = resp.json().await?;
        Ok(FanartImages {
            posters: images.movieposter,
            logos: images.hdmovielogo,
            backdrops: images.moviebackground,
        })
    }

    /// Fetch TV images. Fanart.tv accepts TVDB, TMDB, or IMDb IDs for TV shows.
    pub async fn get_tv_images(&self, id: u64) -> Result<FanartImages, AppError> {
        let url = format!(
            "https://webservice.fanart.tv/v3/tv/{id}?api_key={}",
            self.api_key.as_str()
        );
        let resp = retry::send_with_retry(&FANART_RETRY, || self.http.get(&url).send()).await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(FanartImages { posters: vec![], logos: vec![], backdrops: vec![] });
        }
        let resp = resp.error_for_status()?;
        let images: TvImages = resp.json().await?;
        Ok(FanartImages {
            posters: images.tvposter,
            logos: images.hdtvlogo,
            backdrops: images.showbackground,
        })
    }

    pub fn select_image<'a>(
        posters: &'a [FanartPoster],
        lang: &str,
        textless: bool,
    ) -> Option<(&'a FanartPoster, PosterMatch)> {
        let find_best = |target_lang: &str| -> Option<&FanartPoster> {
            posters
                .iter()
                .filter(|p| p.lang == target_lang)
                .max_by_key(|p| p.likes.parse::<i64>().unwrap_or(0))
        };

        if textless {
            if let Some(p) = find_best("00") {
                return Some((p, PosterMatch::Textless));
            }
        }
        if let Some(p) = find_best(lang) {
            return Some((p, PosterMatch::Language));
        }
        // Fallback: try base language for regional variants (e.g. pt-BR → pt)
        let base = super::lang::lang_base(lang);
        if base != lang {
            if let Some(p) = find_best(base) {
                return Some((p, PosterMatch::Language));
            }
        }
        // Fallback: if no match for requested language, try English
        if lang != "en" && base != "en" {
            if let Some(p) = find_best("en") {
                return Some((p, PosterMatch::Language));
            }
        }
        // Fallback: try empty-string lang (common for backdrops/logos)
        if let Some(p) = find_best("") {
            return Some((p, PosterMatch::Language));
        }
        None
    }

    pub async fn fetch_poster_bytes(&self, url: &str) -> Result<Vec<u8>, AppError> {
        let url = url.to_owned();
        let resp = retry::send_with_retry(&FANART_RETRY, || self.http.get(&url).send())
            .await?
            .error_for_status()?;
        Ok(resp.bytes().await?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_image_by_lang() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "10".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "de".into(), likes: "20".into() },
            FanartPoster { id: "3".into(), url: "http://c".into(), lang: "en".into(), likes: "30".into() },
        ];
        let result = FanartClient::select_image(&posters, "en", false);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "3"); // highest likes
    }

    #[test]
    fn select_image_textless() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "10".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "00".into(), likes: "20".into() },
        ];
        let result = FanartClient::select_image(&posters, "en", true);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.lang, "00");
    }

    #[test]
    fn select_image_textless_fallback_to_lang() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "10".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "de".into(), likes: "20".into() },
        ];
        // No textless ("00") posters — should fall back to "en"
        let result = FanartClient::select_image(&posters, "en", true);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "1");
    }

    #[test]
    fn select_image_no_match() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "fr".into(), likes: "10".into() },
        ];
        // No "ja" posters and no "en" fallback available — should return None
        let result = FanartClient::select_image(&posters, "ja", false);
        assert!(result.is_none());
    }

    #[test]
    fn select_image_falls_back_to_english() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "10".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "fr".into(), likes: "20".into() },
        ];
        // No "de" posters — should fall back to "en"
        let result = FanartClient::select_image(&posters, "de", false);
        assert!(result.is_some());
        let (poster, tier) = result.unwrap();
        assert_eq!(poster.id, "1");
        assert_eq!(tier, PosterMatch::Language);
    }

    #[test]
    fn select_image_empty_list() {
        let posters: Vec<FanartPoster> = vec![];
        assert!(FanartClient::select_image(&posters, "en", false).is_none());
        assert!(FanartClient::select_image(&posters, "en", true).is_none());
    }

    #[test]
    fn select_image_multiple_textless_picks_most_liked() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "00".into(), likes: "5".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "00".into(), likes: "50".into() },
            FanartPoster { id: "3".into(), url: "http://c".into(), lang: "00".into(), likes: "10".into() },
        ];
        let result = FanartClient::select_image(&posters, "en", true);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "2");
    }

    #[test]
    fn select_image_unparseable_likes_treated_as_zero() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "not_a_number".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "en".into(), likes: "5".into() },
        ];
        let result = FanartClient::select_image(&posters, "en", false);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "2");
    }

    #[test]
    fn select_image_zero_likes() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "0".into() },
        ];
        let result = FanartClient::select_image(&posters, "en", false);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "1");
    }

    #[test]
    fn select_image_regional_falls_back_to_base() {
        // Requesting "pt-BR" — no "pt-BR" poster, should fall back to "pt"
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "pt".into(), likes: "15".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "en".into(), likes: "10".into() },
        ];
        let result = FanartClient::select_image(&posters, "pt-BR", false);
        assert!(result.is_some());
        let (poster, tier) = result.unwrap();
        assert_eq!(poster.id, "1");
        assert_eq!(tier, PosterMatch::Language);
    }

    #[test]
    fn select_image_regional_falls_back_to_english() {
        // Requesting "zh-CN" — no "zh-CN" or "zh" poster, should fall back to "en"
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "10".into() },
        ];
        let result = FanartClient::select_image(&posters, "zh-CN", false);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "1");
    }

    #[test]
    fn select_image_english_regional_skips_redundant_fallback() {
        // Requesting "en-US" — should match "en" directly, not loop into English fallback
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "10".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "de".into(), likes: "20".into() },
        ];
        let result = FanartClient::select_image(&posters, "en-US", false);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "1");
    }
}
