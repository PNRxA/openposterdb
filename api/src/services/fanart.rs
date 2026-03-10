use crate::error::AppError;
use serde::Deserialize;

#[derive(Clone)]
pub struct FanartClient {
    api_key: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FanartPoster {
    pub id: String,
    pub url: String,
    pub lang: String,
    pub likes: String,
}

#[derive(Debug, Deserialize)]
struct MovieImages {
    #[serde(default)]
    movieposter: Vec<FanartPoster>,
}

#[derive(Debug, Deserialize)]
struct TvImages {
    #[serde(default)]
    tvposter: Vec<FanartPoster>,
}

/// Which tier the selected poster came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosterMatch {
    Textless,
    Language,
}

impl FanartClient {
    pub fn new(api_key: String, http: reqwest::Client) -> Self {
        Self { api_key, http }
    }

    pub async fn get_movie_posters(&self, tmdb_id: u64) -> Result<Vec<FanartPoster>, AppError> {
        let url = format!(
            "https://webservice.fanart.tv/v3/movies/{tmdb_id}?api_key={}",
            self.api_key
        );
        let resp = self.http.get(&url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }
        let resp = resp.error_for_status()?;
        let images: MovieImages = resp.json().await?;
        Ok(images.movieposter)
    }

    /// Fetch TV posters. Fanart.tv accepts TVDB, TMDB, or IMDb IDs for TV shows.
    pub async fn get_tv_posters(&self, id: u64) -> Result<Vec<FanartPoster>, AppError> {
        let url = format!(
            "https://webservice.fanart.tv/v3/tv/{id}?api_key={}",
            self.api_key
        );
        let resp = self.http.get(&url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }
        let resp = resp.error_for_status()?;
        let images: TvImages = resp.json().await?;
        Ok(images.tvposter)
    }

    pub fn select_poster<'a>(
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
        // Fallback: if no match for requested language, try English
        if lang != "en" {
            if let Some(p) = find_best("en") {
                return Some((p, PosterMatch::Language));
            }
        }
        None
    }

    pub async fn fetch_poster_bytes(&self, url: &str) -> Result<Vec<u8>, AppError> {
        let resp = self.http.get(url).send().await?.error_for_status()?;
        Ok(resp.bytes().await?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_poster_by_lang() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "10".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "de".into(), likes: "20".into() },
            FanartPoster { id: "3".into(), url: "http://c".into(), lang: "en".into(), likes: "30".into() },
        ];
        let result = FanartClient::select_poster(&posters, "en", false);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "3"); // highest likes
    }

    #[test]
    fn select_poster_textless() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "10".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "00".into(), likes: "20".into() },
        ];
        let result = FanartClient::select_poster(&posters, "en", true);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.lang, "00");
    }

    #[test]
    fn select_poster_textless_fallback_to_lang() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "10".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "de".into(), likes: "20".into() },
        ];
        // No textless ("00") posters — should fall back to "en"
        let result = FanartClient::select_poster(&posters, "en", true);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "1");
    }

    #[test]
    fn select_poster_no_match() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "fr".into(), likes: "10".into() },
        ];
        // No "ja" posters and no "en" fallback available — should return None
        let result = FanartClient::select_poster(&posters, "ja", false);
        assert!(result.is_none());
    }

    #[test]
    fn select_poster_falls_back_to_english() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "10".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "fr".into(), likes: "20".into() },
        ];
        // No "de" posters — should fall back to "en"
        let result = FanartClient::select_poster(&posters, "de", false);
        assert!(result.is_some());
        let (poster, tier) = result.unwrap();
        assert_eq!(poster.id, "1");
        assert_eq!(tier, PosterMatch::Language);
    }

    #[test]
    fn select_poster_empty_list() {
        let posters: Vec<FanartPoster> = vec![];
        assert!(FanartClient::select_poster(&posters, "en", false).is_none());
        assert!(FanartClient::select_poster(&posters, "en", true).is_none());
    }

    #[test]
    fn select_poster_multiple_textless_picks_most_liked() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "00".into(), likes: "5".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "00".into(), likes: "50".into() },
            FanartPoster { id: "3".into(), url: "http://c".into(), lang: "00".into(), likes: "10".into() },
        ];
        let result = FanartClient::select_poster(&posters, "en", true);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "2");
    }

    #[test]
    fn select_poster_unparseable_likes_treated_as_zero() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "not_a_number".into() },
            FanartPoster { id: "2".into(), url: "http://b".into(), lang: "en".into(), likes: "5".into() },
        ];
        let result = FanartClient::select_poster(&posters, "en", false);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "2");
    }

    #[test]
    fn select_poster_zero_likes() {
        let posters = vec![
            FanartPoster { id: "1".into(), url: "http://a".into(), lang: "en".into(), likes: "0".into() },
        ];
        let result = FanartClient::select_poster(&posters, "en", false);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "1");
    }
}
