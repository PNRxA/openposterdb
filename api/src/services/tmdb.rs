use std::sync::Arc;

use crate::error::AppError;
use crate::services::retry::{self, TMDB_API_RETRY, TMDB_CDN_RETRY};
use serde::de::DeserializeOwned;
use zeroize::Zeroizing;

use super::lang::{lang_base, lang_region};

/// Standard movie-poster aspect ratio (width:height = 2:3 ≈ 0.667).
const POSTER_TARGET_RATIO: f64 = 2.0 / 3.0;
/// Tolerance around [`POSTER_TARGET_RATIO`] for treating a poster as "standard".
/// TMDB posters are 0.667 (2:3) or occasionally 0.675; this admits both while
/// rejecting square (1.0) or otherwise off-ratio art.
const POSTER_RATIO_TOL: f64 = 0.05;

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
        self.fetch_image_bytes(poster_path, tmdb_size).await
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

    pub async fn get_images(&self, media_type: &str, tmdb_id: u64, lang: &str) -> Result<TmdbImagesResponse, AppError> {
        let path = format!("/{media_type}/{tmdb_id}/images");
        let base = lang_base(lang);
        let include_lang = if base.is_empty() {
            "null".to_string()
        } else {
            format!("{base},null")
        };
        self.get(&path, &[("include_image_language", &include_lang)]).await
    }

    /// Select the best image from a list of TMDB images.
    ///
    /// When `textless` is true, only null-language images (no text overlay) are
    /// considered — returns `None` if none exist so the caller can fall back to
    /// another source (e.g. fanart.tv).
    ///
    /// When `textless` is false and `lang` is non-empty: try requested lang,
    /// then English fallback. Returns `None` if neither matches so the caller
    /// uses the default image (e.g. `resolved.poster_path`) rather than
    /// silently returning a textless image.
    ///
    /// When `lang` is empty (e.g. backdrops): returns the best null-language
    /// image, since backdrops are inherently language-agnostic.
    pub fn select_image<'a>(images: &'a [TmdbImage], lang: &str, textless: bool) -> Option<&'a TmdbImage> {
        Self::select_image_ranked(images, lang, textless, None)
    }

    /// Like [`select_image`], but for posters: among the candidates in each
    /// language tier, prefer art close to the standard 2:3 aspect ratio before
    /// falling back to highest vote. This keeps non-2:3 source posters (which
    /// downstream apps crop, cutting off title art — issue #15) from being
    /// chosen when a 2:3 poster is available. Falls back to vote-only ranking
    /// when no candidate is near 2:3.
    pub fn select_poster<'a>(images: &'a [TmdbImage], lang: &str, textless: bool) -> Option<&'a TmdbImage> {
        Self::select_image_ranked(images, lang, textless, Some(POSTER_TARGET_RATIO))
    }

    fn select_image_ranked<'a>(
        images: &'a [TmdbImage],
        lang: &str,
        textless: bool,
        target_ratio: Option<f64>,
    ) -> Option<&'a TmdbImage> {
        // Rank candidates by (near-target aspect ratio, then vote_average).
        // `max_by` picks a standard-ratio image over any off-ratio one, and the
        // highest vote among equals. With `target_ratio = None` (logos,
        // backdrops) the ratio key is always false, so ranking is vote-only —
        // identical to the previous behavior.
        let rank = |img: &TmdbImage| -> (bool, f64) {
            let standard = match target_ratio {
                Some(r) => img.aspect_ratio > 0.0 && (img.aspect_ratio - r).abs() <= POSTER_RATIO_TOL,
                None => false,
            };
            (standard, img.vote_average)
        };
        let cmp = |a: &&TmdbImage, b: &&TmdbImage| {
            rank(a).partial_cmp(&rank(b)).unwrap_or(std::cmp::Ordering::Equal)
        };
        let find_best = |target: Option<&str>| -> Option<&TmdbImage> {
            images
                .iter()
                .filter(|img| img.iso_639_1.as_deref() == target)
                .max_by(cmp)
        };

        if textless {
            return find_best(None);
        }
        // Language-agnostic request (e.g. backdrops) — return best null-language image
        if lang.is_empty() {
            return find_best(None);
        }

        let base = lang_base(lang);
        let region = lang_region(lang);

        // 1. Exact regional match (e.g. iso_639_1="pt" AND iso_3166_1="BR" for lang="pt-BR")
        if let Some(region) = region {
            let regional = images
                .iter()
                .filter(|img| img.iso_639_1.as_deref() == Some(base) && img.iso_3166_1.as_deref() == Some(region))
                .max_by(cmp);
            if let Some(img) = regional {
                return Some(img);
            }
        }
        // 2. Base language match (any region)
        if let Some(img) = find_best(Some(base)) {
            return Some(img);
        }
        // 3. English fallback
        if base != "en" {
            if let Some(img) = find_best(Some("en")) {
                return Some(img);
            }
        }
        // No match — return None so caller uses the default image
        None
    }

    /// Fetch image bytes from the TMDB CDN for any image type (poster, logo, backdrop).
    pub async fn fetch_image_bytes(&self, file_path: &str, size: &str) -> Result<Vec<u8>, AppError> {
        let url = format!("https://image.tmdb.org/t/p/{size}{file_path}");
        let resp = retry::send_with_retry(&TMDB_CDN_RETRY, || self.http.get(&url).send())
            .await?
            .error_for_status()?;
        Ok(resp.bytes().await?.to_vec())
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TmdbImage {
    pub file_path: String,
    pub iso_639_1: Option<String>,
    #[serde(default)]
    pub iso_3166_1: Option<String>,
    #[serde(default)]
    pub vote_average: f64,
    /// Source aspect ratio (width/height) as reported by TMDB. Defaults to 0.0
    /// when absent, which the poster selector treats as "unknown" (never
    /// preferred as a standard-ratio candidate).
    #[serde(default)]
    pub aspect_ratio: f64,
}

/// Deserialize a `Vec<TmdbImage>`, skipping individual entries that fail
/// to parse rather than failing the whole array. TMDB can return large payloads
/// with occasional malformed entries; a single bad entry must not drop all images.
fn deserialize_lenient_images<'de, D>(deserializer: D) -> Result<Vec<TmdbImage>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let raw: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TmdbImagesResponse {
    #[serde(default, deserialize_with = "deserialize_lenient_images")]
    pub backdrops: Vec<TmdbImage>,
    #[serde(default, deserialize_with = "deserialize_lenient_images")]
    pub logos: Vec<TmdbImage>,
    #[serde(default, deserialize_with = "deserialize_lenient_images")]
    pub posters: Vec<TmdbImage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(path: &str, lang: Option<&str>, vote: f64) -> TmdbImage {
        // Default to a standard 2:3 ratio; the language-selection tests use
        // `select_image` (no ratio preference), so this value is irrelevant there.
        img_ar(path, lang, vote, 2.0 / 3.0)
    }

    fn img_ar(path: &str, lang: Option<&str>, vote: f64, aspect_ratio: f64) -> TmdbImage {
        TmdbImage {
            file_path: path.to_string(),
            iso_639_1: lang.map(|s| s.to_string()),
            iso_3166_1: None,
            vote_average: vote,
            aspect_ratio,
        }
    }

    fn img_regional(path: &str, lang: &str, region: &str, vote: f64) -> TmdbImage {
        TmdbImage {
            file_path: path.to_string(),
            iso_639_1: Some(lang.to_string()),
            iso_3166_1: Some(region.to_string()),
            vote_average: vote,
            aspect_ratio: 2.0 / 3.0,
        }
    }

    #[test]
    fn select_image_empty_list() {
        assert!(TmdbClient::select_image(&[], "en", false).is_none());
        assert!(TmdbClient::select_image(&[], "en", true).is_none());
    }

    #[test]
    fn select_image_exact_lang_match() {
        let images = vec![
            img("/en.jpg", Some("en"), 7.0),
            img("/de.jpg", Some("de"), 8.0),
        ];
        let selected = TmdbClient::select_image(&images, "de", false).unwrap();
        assert_eq!(selected.file_path, "/de.jpg");
    }

    #[test]
    fn select_image_falls_back_to_english() {
        let images = vec![
            img("/en.jpg", Some("en"), 6.0),
            img("/null.jpg", None, 9.0),
        ];
        // Requesting French, no French available → should fall back to English
        let selected = TmdbClient::select_image(&images, "fr", false).unwrap();
        assert_eq!(selected.file_path, "/en.jpg");
    }

    #[test]
    fn select_image_no_match_returns_none() {
        // Only textless (null-lang) images — should NOT be returned when textless=false
        let images = vec![
            img("/null.jpg", None, 9.0),
        ];
        assert!(TmdbClient::select_image(&images, "de", false).is_none());
    }

    #[test]
    fn select_image_english_request_no_english_returns_none() {
        // Requesting English, only null-lang available
        let images = vec![
            img("/null.jpg", None, 9.0),
        ];
        assert!(TmdbClient::select_image(&images, "en", false).is_none());
    }

    #[test]
    fn select_image_textless_returns_null_lang() {
        let images = vec![
            img("/en.jpg", Some("en"), 9.0),
            img("/null.jpg", None, 5.0),
        ];
        let selected = TmdbClient::select_image(&images, "en", true).unwrap();
        assert_eq!(selected.file_path, "/null.jpg");
    }

    #[test]
    fn select_image_textless_no_null_lang() {
        let images = vec![
            img("/en.jpg", Some("en"), 9.0),
        ];
        assert!(TmdbClient::select_image(&images, "en", true).is_none());
    }

    #[test]
    fn select_image_picks_highest_vote() {
        let images = vec![
            img("/en_low.jpg", Some("en"), 3.0),
            img("/en_high.jpg", Some("en"), 8.0),
            img("/en_mid.jpg", Some("en"), 5.0),
        ];
        let selected = TmdbClient::select_image(&images, "en", false).unwrap();
        assert_eq!(selected.file_path, "/en_high.jpg");
    }

    #[test]
    fn select_image_empty_lang_returns_null_lang() {
        // Backdrops use empty lang — should return null-language images
        let images = vec![
            img("/en.jpg", Some("en"), 9.0),
            img("/null.jpg", None, 5.0),
        ];
        let selected = TmdbClient::select_image(&images, "", false).unwrap();
        assert_eq!(selected.file_path, "/null.jpg");
    }

    #[test]
    fn select_image_empty_lang_no_null_returns_none() {
        let images = vec![
            img("/en.jpg", Some("en"), 9.0),
        ];
        assert!(TmdbClient::select_image(&images, "", false).is_none());
    }

    #[test]
    fn select_image_regional_exact_match() {
        let images = vec![
            img_regional("/pt_br.jpg", "pt", "BR", 5.0),
            img_regional("/pt_pt.jpg", "pt", "PT", 7.0),
        ];
        let selected = TmdbClient::select_image(&images, "pt-BR", false).unwrap();
        assert_eq!(selected.file_path, "/pt_br.jpg");
    }

    #[test]
    fn select_image_regional_falls_back_to_base_lang() {
        // No BR-specific image, but a PT one exists — should match via base "pt"
        let images = vec![
            img_regional("/pt_pt.jpg", "pt", "PT", 7.0),
        ];
        let selected = TmdbClient::select_image(&images, "pt-BR", false).unwrap();
        assert_eq!(selected.file_path, "/pt_pt.jpg");
    }

    #[test]
    fn select_image_regional_falls_back_to_english() {
        // No Portuguese images at all — should fall back to English
        let images = vec![
            img("/en.jpg", Some("en"), 6.0),
        ];
        let selected = TmdbClient::select_image(&images, "pt-BR", false).unwrap();
        assert_eq!(selected.file_path, "/en.jpg");
    }

    #[test]
    fn select_image_base_lang_gets_both_regions() {
        // Requesting "pt" (no region) — should pick highest-voted regardless of region
        let images = vec![
            img_regional("/pt_br.jpg", "pt", "BR", 5.0),
            img_regional("/pt_pt.jpg", "pt", "PT", 7.0),
        ];
        let selected = TmdbClient::select_image(&images, "pt", false).unwrap();
        assert_eq!(selected.file_path, "/pt_pt.jpg");
    }

    #[test]
    fn select_image_english_regional_skips_redundant_fallback() {
        // Requesting "en-US" — should match iso_639_1="en" directly, not attempt English fallback
        let images = vec![
            img("/en.jpg", Some("en"), 7.0),
            img("/de.jpg", Some("de"), 9.0),
        ];
        let selected = TmdbClient::select_image(&images, "en-US", false).unwrap();
        assert_eq!(selected.file_path, "/en.jpg");
    }

    #[test]
    fn select_image_english_regional_exact_match() {
        // Requesting "en-GB" — should prefer iso_3166_1="GB" over generic English
        let images = vec![
            img("/en.jpg", Some("en"), 9.0),
            img_regional("/en_gb.jpg", "en", "GB", 5.0),
        ];
        let selected = TmdbClient::select_image(&images, "en-GB", false).unwrap();
        assert_eq!(selected.file_path, "/en_gb.jpg");
    }

    #[test]
    fn select_poster_prefers_standard_ratio_over_higher_vote() {
        // A square poster with a higher vote should lose to a true 2:3 poster.
        let images = vec![
            img_ar("/square.jpg", Some("en"), 9.0, 1.0),
            img_ar("/poster.jpg", Some("en"), 6.0, 2.0 / 3.0),
        ];
        let selected = TmdbClient::select_poster(&images, "en", false).unwrap();
        assert_eq!(selected.file_path, "/poster.jpg");
    }

    #[test]
    fn select_poster_prefers_highest_vote_among_standard_ratio() {
        let images = vec![
            img_ar("/p_low.jpg", Some("en"), 5.0, 2.0 / 3.0),
            img_ar("/p_high.jpg", Some("en"), 8.0, 0.675), // within tolerance of 2:3
            img_ar("/square.jpg", Some("en"), 9.5, 1.0),
        ];
        let selected = TmdbClient::select_poster(&images, "en", false).unwrap();
        assert_eq!(selected.file_path, "/p_high.jpg");
    }

    #[test]
    fn select_poster_falls_back_to_vote_when_no_standard_ratio() {
        let images = vec![
            img_ar("/square_low.jpg", Some("en"), 3.0, 1.0),
            img_ar("/square_high.jpg", Some("en"), 8.0, 1.0),
        ];
        let selected = TmdbClient::select_poster(&images, "en", false).unwrap();
        assert_eq!(selected.file_path, "/square_high.jpg");
    }

    #[test]
    fn select_image_ignores_aspect_ratio() {
        // The generic selector (logos/backdrops) stays vote-only regardless of ratio.
        let images = vec![
            img_ar("/wide_high.jpg", Some("en"), 9.0, 1.78),
            img_ar("/poster_low.jpg", Some("en"), 4.0, 2.0 / 3.0),
        ];
        let selected = TmdbClient::select_image(&images, "en", false).unwrap();
        assert_eq!(selected.file_path, "/wide_high.jpg");
    }
}
