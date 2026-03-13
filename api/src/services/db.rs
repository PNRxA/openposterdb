use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait, Set, TransactionTrait};
use zeroize::Zeroizing;

use std::collections::HashMap;
use std::sync::Arc;

use crate::entity::{admin_user, api_key, api_key_settings, global_settings, refresh_token};
use crate::error::AppError;
use crate::services::ratings::RatingSource;

// --- Setting value constants ---

/// Poster source: TMDB
pub const SOURCE_TMDB: &str = "t";
/// Poster source: Fanart.tv
pub const SOURCE_FANART: &str = "f";

/// Badge style / badge direction: horizontal
pub const STYLE_HORIZONTAL: &str = "h";
/// Badge style / badge direction: vertical
pub const STYLE_VERTICAL: &str = "v";

/// Default value for direction/style: auto (resolves based on context)
pub const DIRECTION_DEFAULT: &str = "d";
/// Badge style: default (resolves to match badge direction)
pub const STYLE_DEFAULT: &str = DIRECTION_DEFAULT;

/// Label style: icon
pub const LABEL_ICON: &str = "i";
/// Label style: text
pub const LABEL_TEXT: &str = "t";

/// Poster position: bottom-center (default)
pub const POS_BOTTOM_CENTER: &str = "bc";
/// Poster position: top-center
pub const POS_TOP_CENTER: &str = "tc";
/// Poster position: left
pub const POS_LEFT: &str = "l";
/// Poster position: right
pub const POS_RIGHT: &str = "r";
/// Poster position: top-left
pub const POS_TOP_LEFT: &str = "tl";
/// Poster position: top-right
pub const POS_TOP_RIGHT: &str = "tr";
/// Poster position: bottom-left
pub const POS_BOTTOM_LEFT: &str = "bl";
/// Poster position: bottom-right
pub const POS_BOTTOM_RIGHT: &str = "br";

// --- Image size ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSize {
    Small,
    Medium,
    Large,
    VeryLarge,
}

impl ImageSize {
    pub fn from_query_str(s: &str) -> Option<Self> {
        match s {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            "very-large" | "verylarge" => Some(Self::VeryLarge),
            _ => None,
        }
    }

    /// Target width for posters at this size.
    ///
    /// Panics if called with `Small` — validation rejects it before reaching here.
    pub fn poster_target_width(self) -> u32 {
        match self {
            Self::Small => unreachable!("Small is not valid for posters — validate_image_size should reject it"),
            Self::Medium => 580,
            Self::Large => 1280,
            Self::VeryLarge => 2000,
        }
    }

    /// Target width for backdrops at this size.
    pub fn backdrop_target_width(self) -> u32 {
        match self {
            Self::Small => 1280,
            Self::Medium => 1920,
            Self::Large => 3840,
            Self::VeryLarge => 3840,
        }
    }

    /// Target width for logos at this size.
    ///
    /// Panics if called with `Small` — validation rejects it before reaching here.
    pub fn logo_target_width(self) -> u32 {
        match self {
            Self::Small => unreachable!("Small is not valid for logos — validate_image_size should reject it"),
            Self::Medium => 780,
            Self::Large => 1722,
            Self::VeryLarge => 2689,
        }
    }

    /// Badge scale factor relative to the medium (default) target width for each image kind.
    /// Base widths: poster=580, logo=780, backdrop=1920.
    pub fn badge_scale(self, kind: crate::cache::ImageType) -> f32 {
        match kind {
            crate::cache::ImageType::Poster => self.poster_target_width() as f32 / 580.0,
            crate::cache::ImageType::Logo => self.logo_target_width() as f32 / 780.0,
            crate::cache::ImageType::Backdrop => self.backdrop_target_width() as f32 / 1920.0,
        }
    }

    /// Cache key suffix for this image size.
    pub fn cache_suffix(self) -> &'static str {
        match self {
            Self::Small => ".zs",
            Self::Medium => ".zm",
            Self::Large => ".zl",
            Self::VeryLarge => ".zvl",
        }
    }

    /// Query string value for this image size.
    pub fn query_str(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
            Self::VeryLarge => "very-large",
        }
    }

    /// TMDB CDN size string for fetching source images.
    pub fn tmdb_size(self) -> &'static str {
        match self {
            Self::Small => "w780",
            Self::Medium => "w780",
            Self::Large => "original",
            Self::VeryLarge => "original",
        }
    }
}

pub fn validate_image_size(size_str: &str, kind: crate::cache::ImageType) -> Result<ImageSize, AppError> {
    let size = ImageSize::from_query_str(size_str)
        .ok_or_else(|| AppError::BadRequest(
            "imageSize must be 'small', 'medium', 'large', or 'very-large'".into(),
        ))?;
    if size == ImageSize::Small && kind != crate::cache::ImageType::Backdrop {
        return Err(AppError::BadRequest(
            "imageSize 'small' is only valid for backdrops".into(),
        ));
    }
    Ok(size)
}

pub fn default_fanart_lang() -> String {
    "en".to_string()
}

pub fn default_ratings_limit() -> i32 {
    3
}

pub fn default_logo_backdrop_ratings_limit() -> i32 {
    5
}

pub fn default_ratings_order() -> String {
    "mal,imdb,lb,rt,mc,rta,tmdb,trakt".to_string()
}

pub fn default_poster_position() -> String {
    POS_BOTTOM_CENTER.to_string()
}

pub fn default_poster_badge_style() -> String {
    STYLE_DEFAULT.to_string()
}

pub fn default_logo_badge_style() -> String {
    STYLE_VERTICAL.to_string()
}

pub fn default_backdrop_badge_style() -> String {
    STYLE_VERTICAL.to_string()
}

pub fn default_label_style() -> String {
    LABEL_ICON.to_string()
}

pub fn default_poster_badge_direction() -> String {
    DIRECTION_DEFAULT.to_string()
}

/// Resolve a badge direction of `"default"` to `"h"` or `"v"`
/// based on the poster position. Center positions use horizontal; everything
/// else (left, right, corners) uses vertical. Non-default values pass through.
pub fn resolve_badge_direction(direction: &str, position: &str) -> Arc<str> {
    if direction != DIRECTION_DEFAULT {
        return Arc::from(direction);
    }
    match position {
        POS_BOTTOM_CENTER | POS_TOP_CENTER => Arc::from(STYLE_HORIZONTAL),
        _ => Arc::from(STYLE_VERTICAL),
    }
}

/// Validate that poster_source is a known value.
pub fn validate_poster_source(source: &str) -> Result<(), AppError> {
    if source == SOURCE_TMDB || source == SOURCE_FANART {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            format!("poster_source must be '{SOURCE_TMDB}' or '{SOURCE_FANART}'"),
        ))
    }
}

/// Validate ratings_limit is 0–8.
pub fn validate_ratings_limit(limit: i32) -> Result<(), AppError> {
    if (0..=8).contains(&limit) {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "ratings_limit must be between 0 and 8".into(),
        ))
    }
}

/// Validate a comma-separated list of rating source keys (no duplicates).
pub fn validate_ratings_order(order: &str) -> Result<(), AppError> {
    if order.is_empty() {
        return Ok(());
    }
    let mut seen = std::collections::HashSet::new();
    for key in order.split(',') {
        let key = key.trim();
        if RatingSource::from_key(key).is_none() {
            return Err(AppError::BadRequest(format!(
                "unknown rating source key: '{key}'. Valid keys: {}",
                RatingSource::all_keys().join(", ")
            )));
        }
        if !seen.insert(key) {
            return Err(AppError::BadRequest(format!(
                "duplicate rating source key: '{key}'"
            )));
        }
    }
    Ok(())
}

pub fn validate_badge_style(style: &str) -> Result<(), AppError> {
    match style {
        STYLE_HORIZONTAL | STYLE_VERTICAL | STYLE_DEFAULT => Ok(()),
        _ => Err(AppError::BadRequest(
            format!("badge_style must be '{STYLE_HORIZONTAL}', '{STYLE_VERTICAL}', or '{STYLE_DEFAULT}'"),
        )),
    }
}

/// Resolve a badge style of `"d"` (default) to match the resolved badge direction.
/// Non-default values pass through unchanged.
pub fn resolve_badge_style(style: &str, resolved_direction: &str) -> Arc<str> {
    if style == STYLE_DEFAULT {
        Arc::from(resolved_direction)
    } else {
        Arc::from(style)
    }
}

pub fn validate_label_style(style: &str) -> Result<(), AppError> {
    match style {
        LABEL_TEXT | LABEL_ICON => Ok(()),
        _ => Err(AppError::BadRequest(
            format!("label_style must be '{LABEL_TEXT}' or '{LABEL_ICON}'"),
        )),
    }
}

pub fn validate_badge_direction(dir: &str) -> Result<(), AppError> {
    match dir {
        DIRECTION_DEFAULT | STYLE_HORIZONTAL | STYLE_VERTICAL => Ok(()),
        _ => Err(AppError::BadRequest(
            format!("badge_direction must be '{DIRECTION_DEFAULT}', '{STYLE_HORIZONTAL}', or '{STYLE_VERTICAL}'"),
        )),
    }
}

/// Validate that poster_position is a known value.
pub fn validate_poster_position(pos: &str) -> Result<(), AppError> {
    match pos {
        POS_BOTTOM_CENTER | POS_TOP_CENTER | POS_LEFT | POS_RIGHT
        | POS_TOP_LEFT | POS_TOP_RIGHT | POS_BOTTOM_LEFT | POS_BOTTOM_RIGHT => Ok(()),
        _ => Err(AppError::BadRequest(
            format!("poster_position must be '{POS_BOTTOM_CENTER}', '{POS_TOP_CENTER}', '{POS_LEFT}', '{POS_RIGHT}', '{POS_TOP_LEFT}', '{POS_TOP_RIGHT}', '{POS_BOTTOM_LEFT}', or '{POS_BOTTOM_RIGHT}'"),
        )),
    }
}

/// Validate a fanart language code: 2–5 ASCII alphanumeric chars or hyphens (e.g. "en", "pt-BR").
pub fn validate_fanart_lang(lang: &str) -> Result<(), AppError> {
    if lang.len() >= 2
        && lang.len() <= 5
        && lang
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "fanart_lang must be 2-5 ASCII alphanumeric characters (e.g. 'en', 'de', 'pt-BR')"
                .into(),
        ))
    }
}

/// Common poster settings fields shared between per-key and global update requests.
pub trait PosterSettingsInput {
    fn poster_source(&self) -> &str;
    fn fanart_lang(&self) -> &str;
    fn ratings_limit(&self) -> i32;
    fn ratings_order(&self) -> &str;
    fn poster_position(&self) -> &str;
    fn logo_ratings_limit(&self) -> i32;
    fn backdrop_ratings_limit(&self) -> i32;
    fn poster_badge_style(&self) -> &str;
    fn logo_badge_style(&self) -> &str;
    fn backdrop_badge_style(&self) -> &str;
    fn poster_label_style(&self) -> &str;
    fn logo_label_style(&self) -> &str;
    fn backdrop_label_style(&self) -> &str;
    fn poster_badge_direction(&self) -> &str;
}

/// Validate all poster settings fields at once.
pub fn validate_poster_settings_input(input: &dyn PosterSettingsInput) -> Result<(), AppError> {
    validate_poster_source(input.poster_source())?;
    validate_fanart_lang(input.fanart_lang())?;
    validate_ratings_limit(input.ratings_limit())?;
    validate_ratings_order(input.ratings_order())?;
    validate_poster_position(input.poster_position())?;
    validate_ratings_limit(input.logo_ratings_limit())?;
    validate_ratings_limit(input.backdrop_ratings_limit())?;
    validate_badge_style(input.poster_badge_style())?;
    validate_badge_style(input.logo_badge_style())?;
    validate_badge_style(input.backdrop_badge_style())?;
    validate_label_style(input.poster_label_style())?;
    validate_label_style(input.logo_label_style())?;
    validate_label_style(input.backdrop_label_style())?;
    validate_badge_direction(input.poster_badge_direction())?;
    Ok(())
}

fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

// --- Secret loading from env ---

pub fn load_secret_from_env(env_var: &str) -> Zeroizing<Vec<u8>> {
    match std::env::var(env_var) {
        Ok(hex) if !hex.is_empty() => {
            let bytes =
                hex_to_bytes(&hex).unwrap_or_else(|e| panic!("{env_var} is not valid hex: {e}"));
            if bytes.len() != 32 {
                panic!(
                    "{env_var} must be 32 bytes (64 hex chars), got {}",
                    bytes.len()
                );
            }
            tracing::info!("{env_var} loaded from environment");
            Zeroizing::new(bytes)
        }
        _ => {
            panic!(
                "{env_var} is not set. This is required.\n\
                 Generate one with: openssl rand -hex 32\n\
                 Then add it to your .env file."
            );
        }
    }
}

fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("Odd-length hex string".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn hex_to_bytes_valid() {
        assert_eq!(hex_to_bytes("abcd").unwrap(), vec![0xab, 0xcd]);
    }

    #[test]
    fn hex_to_bytes_empty() {
        assert_eq!(hex_to_bytes("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn hex_to_bytes_full_32_bytes() {
        let hex = "00".repeat(32);
        let result = hex_to_bytes(&hex).unwrap();
        assert_eq!(result.len(), 32);
        assert!(result.iter().all(|&b| b == 0));
    }

    #[test]
    fn hex_to_bytes_odd_length() {
        assert!(hex_to_bytes("abc").is_err());
    }

    #[test]
    fn hex_to_bytes_invalid_chars() {
        assert!(hex_to_bytes("gg").is_err());
    }

    #[test]
    fn hex_to_bytes_uppercase() {
        assert_eq!(hex_to_bytes("ABCD").unwrap(), vec![0xab, 0xcd]);
    }

    #[test]
    fn hex_to_bytes_mixed_case() {
        assert_eq!(hex_to_bytes("aBcD").unwrap(), vec![0xab, 0xcd]);
    }

    #[test]
    #[serial]
    #[should_panic(expected = "is not set")]
    fn load_secret_missing_env_var() {
        load_secret_from_env("OPENPOSTERDB_TEST_NONEXISTENT_SECRET_VAR");
    }

    #[test]
    #[serial]
    #[should_panic(expected = "must be 32 bytes")]
    fn load_secret_wrong_length() {
        let var_name = "OPENPOSTERDB_TEST_SHORT_SECRET";
        unsafe { std::env::set_var(var_name, "abcd") };
        let result = std::panic::catch_unwind(|| load_secret_from_env(var_name));
        unsafe { std::env::remove_var(var_name) };
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    #[serial]
    fn load_secret_valid_32_bytes() {
        let var_name = "OPENPOSTERDB_TEST_VALID_SECRET";
        let hex = "ab".repeat(32);
        unsafe { std::env::set_var(var_name, &hex) };
        let secret = load_secret_from_env(var_name);
        unsafe { std::env::remove_var(var_name) };
        assert_eq!(secret.len(), 32);
    }

    #[test]
    fn default_fanart_lang_returns_en() {
        assert_eq!(default_fanart_lang(), "en");
    }

    #[test]
    fn validate_ratings_limit_accepts_valid() {
        for i in 0..=8 {
            assert!(validate_ratings_limit(i).is_ok(), "limit {i} should be valid");
        }
    }

    #[test]
    fn validate_ratings_limit_rejects_negative() {
        assert!(validate_ratings_limit(-1).is_err());
    }

    #[test]
    fn validate_ratings_limit_rejects_too_large() {
        assert!(validate_ratings_limit(9).is_err());
        assert!(validate_ratings_limit(100).is_err());
    }

    #[test]
    fn validate_ratings_order_accepts_empty() {
        assert!(validate_ratings_order("").is_ok());
    }

    #[test]
    fn validate_ratings_order_accepts_valid_keys() {
        assert!(validate_ratings_order("imdb,tmdb,rt").is_ok());
        assert!(validate_ratings_order("mal,imdb,rta,trakt,lb,mc,tmdb,rt").is_ok());
    }

    #[test]
    fn validate_ratings_order_rejects_unknown_keys() {
        assert!(validate_ratings_order("imdb,bogus").is_err());
        assert!(validate_ratings_order("unknown").is_err());
    }

    #[test]
    fn validate_ratings_order_rejects_duplicates() {
        assert!(validate_ratings_order("imdb,imdb").is_err());
        assert!(validate_ratings_order("rt,tmdb,rt").is_err());
    }

    #[test]
    fn validate_fanart_lang_valid_codes() {
        assert!(validate_fanart_lang("en").is_ok());
        assert!(validate_fanart_lang("de").is_ok());
        assert!(validate_fanart_lang("fr").is_ok());
        assert!(validate_fanart_lang("ja").is_ok());
        assert!(validate_fanart_lang("pt-BR").is_ok());
        assert!(validate_fanart_lang("zh-CN").is_ok());
    }

    #[test]
    fn validate_fanart_lang_rejects_too_short() {
        assert!(validate_fanart_lang("e").is_err());
        assert!(validate_fanart_lang("").is_err());
    }

    #[test]
    fn validate_fanart_lang_rejects_too_long() {
        assert!(validate_fanart_lang("abcdef").is_err());
        assert!(validate_fanart_lang("toolongvalue").is_err());
    }

    #[test]
    fn validate_fanart_lang_rejects_special_chars() {
        assert!(validate_fanart_lang("../../").is_err());
        assert!(validate_fanart_lang("en\0").is_err());
        assert!(validate_fanart_lang("a b").is_err());
        assert!(validate_fanart_lang("en/de").is_err());
    }

    #[test]
    fn validate_poster_source_accepts_valid() {
        assert!(validate_poster_source("t").is_ok());
        assert!(validate_poster_source("f").is_ok());
    }

    #[test]
    fn validate_poster_source_rejects_invalid() {
        assert!(validate_poster_source("tmdb").is_err());
        assert!(validate_poster_source("fanart").is_err());
        assert!(validate_poster_source("").is_err());
        assert!(validate_poster_source("x").is_err());
    }

    #[test]
    fn validate_poster_position_accepts_valid() {
        assert!(validate_poster_position("bc").is_ok());
        assert!(validate_poster_position("tc").is_ok());
        assert!(validate_poster_position("l").is_ok());
        assert!(validate_poster_position("r").is_ok());
        assert!(validate_poster_position("tl").is_ok());
        assert!(validate_poster_position("tr").is_ok());
        assert!(validate_poster_position("bl").is_ok());
        assert!(validate_poster_position("br").is_ok());
    }

    #[test]
    fn validate_poster_position_rejects_invalid() {
        assert!(validate_poster_position("center").is_err());
        assert!(validate_poster_position("").is_err());
        assert!(validate_poster_position("bottom-center").is_err());
        assert!(validate_poster_position("middle").is_err());
    }

    #[test]
    fn default_poster_position_returns_bottom_center() {
        assert_eq!(default_poster_position(), "bc");
    }

    #[test]
    fn default_poster_badge_style_returns_default() {
        assert_eq!(default_poster_badge_style(), "d");
    }

    #[test]
    fn default_backdrop_badge_style_returns_vertical() {
        assert_eq!(default_backdrop_badge_style(), "v");
    }

    #[test]
    fn validate_badge_style_accepts_valid() {
        assert!(validate_badge_style("h").is_ok());
        assert!(validate_badge_style("v").is_ok());
        assert!(validate_badge_style("d").is_ok());
    }

    #[test]
    fn validate_badge_style_rejects_invalid() {
        assert!(validate_badge_style("diagonal").is_err());
        assert!(validate_badge_style("").is_err());
    }

    #[test]
    fn resolve_badge_style_default_follows_direction() {
        assert_eq!(&*resolve_badge_style("d", "h"), "h");
        assert_eq!(&*resolve_badge_style("d", "v"), "v");
    }

    #[test]
    fn resolve_badge_style_explicit_passes_through() {
        assert_eq!(&*resolve_badge_style("h", "v"), "h");
        assert_eq!(&*resolve_badge_style("v", "h"), "v");
    }

    #[test]
    fn validate_label_style_accepts_valid() {
        assert!(validate_label_style("t").is_ok());
        assert!(validate_label_style("i").is_ok());
    }

    #[test]
    fn validate_label_style_rejects_invalid() {
        assert!(validate_label_style("emoji").is_err());
        assert!(validate_label_style("").is_err());
    }

    #[test]
    fn default_label_style_returns_icon() {
        assert_eq!(default_label_style(), "i");
    }

    #[test]
    fn default_poster_badge_direction_returns_default() {
        assert_eq!(default_poster_badge_direction(), "d");
    }

    #[test]
    fn validate_badge_direction_accepts_default() {
        assert!(validate_badge_direction("d").is_ok());
        assert!(validate_badge_direction("h").is_ok());
        assert!(validate_badge_direction("v").is_ok());
    }

    #[test]
    fn validate_badge_direction_rejects_invalid() {
        assert!(validate_badge_direction("diagonal").is_err());
        assert!(validate_badge_direction("").is_err());
    }

    #[test]
    fn resolve_badge_direction_default_center_positions() {
        assert_eq!(&*resolve_badge_direction("d", "bc"), "h");
        assert_eq!(&*resolve_badge_direction("d", "tc"), "h");
    }

    #[test]
    fn resolve_badge_direction_default_side_positions() {
        assert_eq!(&*resolve_badge_direction("d", "l"), "v");
        assert_eq!(&*resolve_badge_direction("d", "r"), "v");
    }

    #[test]
    fn resolve_badge_direction_default_corner_positions() {
        assert_eq!(&*resolve_badge_direction("d", "tl"), "v");
        assert_eq!(&*resolve_badge_direction("d", "tr"), "v");
        assert_eq!(&*resolve_badge_direction("d", "bl"), "v");
        assert_eq!(&*resolve_badge_direction("d", "br"), "v");
    }

    #[test]
    fn resolve_badge_direction_explicit_passes_through() {
        assert_eq!(&*resolve_badge_direction("h", "l"), "h");
        assert_eq!(&*resolve_badge_direction("v", "bc"), "v");
    }

    // --- ImageSize tests ---

    #[test]
    fn image_size_from_query_str_valid() {
        assert_eq!(ImageSize::from_query_str("small"), Some(ImageSize::Small));
        assert_eq!(ImageSize::from_query_str("medium"), Some(ImageSize::Medium));
        assert_eq!(ImageSize::from_query_str("large"), Some(ImageSize::Large));
        assert_eq!(ImageSize::from_query_str("very-large"), Some(ImageSize::VeryLarge));
    }

    #[test]
    fn image_size_from_query_str_invalid() {
        assert_eq!(ImageSize::from_query_str(""), None);
        assert_eq!(ImageSize::from_query_str("huge"), None);
        assert_eq!(ImageSize::from_query_str("MEDIUM"), None);
        assert_eq!(ImageSize::from_query_str("very_large"), None);
    }

    #[test]
    fn image_size_poster_target_widths() {
        assert_eq!(ImageSize::Medium.poster_target_width(), 580);
        assert_eq!(ImageSize::Large.poster_target_width(), 1280);
        assert_eq!(ImageSize::VeryLarge.poster_target_width(), 2000);
    }

    #[test]
    fn image_size_logo_target_widths() {
        assert_eq!(ImageSize::Medium.logo_target_width(), 780);
        assert_eq!(ImageSize::Large.logo_target_width(), 1722);
        assert_eq!(ImageSize::VeryLarge.logo_target_width(), 2689);
    }

    #[test]
    fn image_size_backdrop_target_widths() {
        assert_eq!(ImageSize::Small.backdrop_target_width(), 1280);
        assert_eq!(ImageSize::Medium.backdrop_target_width(), 1920);
        assert_eq!(ImageSize::Large.backdrop_target_width(), 3840);
    }

    #[test]
    fn image_size_badge_scale_medium_is_baseline() {
        // Medium is the default — badge scale should be 1.0 for all kinds
        let scale = ImageSize::Medium.badge_scale(crate::cache::ImageType::Poster);
        assert!((scale - 1.0).abs() < 0.01);

        let scale = ImageSize::Medium.badge_scale(crate::cache::ImageType::Logo);
        assert!((scale - 1.0).abs() < 0.01);

        let scale = ImageSize::Medium.badge_scale(crate::cache::ImageType::Backdrop);
        assert!((scale - 1.0).abs() < 0.01);
    }

    #[test]
    fn image_size_badge_scale_increases_with_size() {
        let medium = ImageSize::Medium.badge_scale(crate::cache::ImageType::Poster);
        let large = ImageSize::Large.badge_scale(crate::cache::ImageType::Poster);
        let very_large = ImageSize::VeryLarge.badge_scale(crate::cache::ImageType::Poster);
        assert!(large > medium);
        assert!(very_large > large);
    }

    #[test]
    fn image_size_cache_suffixes() {
        assert_eq!(ImageSize::Small.cache_suffix(), ".zs");
        assert_eq!(ImageSize::Medium.cache_suffix(), ".zm");
        assert_eq!(ImageSize::Large.cache_suffix(), ".zl");
        assert_eq!(ImageSize::VeryLarge.cache_suffix(), ".zvl");
    }

    #[test]
    fn image_size_tmdb_sizes() {
        assert_eq!(ImageSize::Small.tmdb_size(), "w780");
        assert_eq!(ImageSize::Medium.tmdb_size(), "w780");
        assert_eq!(ImageSize::Large.tmdb_size(), "original");
        assert_eq!(ImageSize::VeryLarge.tmdb_size(), "original");
    }

    #[test]
    fn validate_image_size_accepts_valid_poster_sizes() {
        assert!(validate_image_size("medium", crate::cache::ImageType::Poster).is_ok());
        assert!(validate_image_size("large", crate::cache::ImageType::Poster).is_ok());
        assert!(validate_image_size("very-large", crate::cache::ImageType::Poster).is_ok());
    }

    #[test]
    fn validate_image_size_rejects_small_for_poster() {
        assert!(validate_image_size("small", crate::cache::ImageType::Poster).is_err());
    }

    #[test]
    fn validate_image_size_rejects_small_for_logo() {
        assert!(validate_image_size("small", crate::cache::ImageType::Logo).is_err());
    }

    #[test]
    fn validate_image_size_accepts_small_for_backdrop() {
        assert!(validate_image_size("small", crate::cache::ImageType::Backdrop).is_ok());
    }

    #[test]
    fn validate_image_size_rejects_unknown() {
        assert!(validate_image_size("huge", crate::cache::ImageType::Poster).is_err());
        assert!(validate_image_size("", crate::cache::ImageType::Backdrop).is_err());
    }
}

// --- Admin user CRUD ---

pub async fn count_admin_users(db: &impl ConnectionTrait) -> Result<u64, AppError> {
    use sea_orm::PaginatorTrait;
    admin_user::Entity::find()
        .count(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn create_admin_user(
    db: &impl ConnectionTrait,
    username: &str,
    password_hash: &str,
) -> Result<admin_user::Model, AppError> {
    let model = admin_user::ActiveModel {
        id: Default::default(),
        username: Set(username.to_owned()),
        password_hash: Set(password_hash.to_owned()),
        created_at: Set(now_utc()),
    };

    let result = admin_user::Entity::insert(model)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    admin_user::Entity::find_by_id(result.last_insert_id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?
        .ok_or_else(|| AppError::DbError("Failed to retrieve created user".into()))
}

pub async fn create_first_admin_user(
    db: &DatabaseConnection,
    username: &str,
    password_hash: &str,
) -> Result<admin_user::Model, AppError> {
    use sea_orm::PaginatorTrait;

    let txn = db
        .begin()
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    let count = admin_user::Entity::find()
        .count(&txn)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    if count > 0 {
        txn.rollback()
            .await
            .map_err(|e| AppError::DbError(e.to_string()))?;
        return Err(AppError::Forbidden("Setup already completed".into()));
    }

    let model = admin_user::ActiveModel {
        id: Default::default(),
        username: Set(username.to_owned()),
        password_hash: Set(password_hash.to_owned()),
        created_at: Set(now_utc()),
    };

    let result = admin_user::Entity::insert(model)
        .exec(&txn)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    let user = admin_user::Entity::find_by_id(result.last_insert_id)
        .one(&txn)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?
        .ok_or_else(|| AppError::DbError("Failed to retrieve created user".into()))?;

    txn.commit()
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    Ok(user)
}

pub async fn find_admin_user_by_username(
    db: &impl ConnectionTrait,
    username: &str,
) -> Result<Option<admin_user::Model>, AppError> {
    use sea_orm::ColumnTrait;
    use sea_orm::QueryFilter;
    admin_user::Entity::find()
        .filter(admin_user::Column::Username.eq(username))
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn find_admin_user_by_id(
    db: &impl ConnectionTrait,
    id: i32,
) -> Result<Option<admin_user::Model>, AppError> {
    admin_user::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

// --- Refresh token CRUD ---

pub async fn create_refresh_token(
    db: &impl ConnectionTrait,
    user_id: i32,
    token_hash: &str,
    expires_at: &str,
) -> Result<refresh_token::Model, AppError> {
    let model = refresh_token::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        token_hash: Set(token_hash.to_owned()),
        expires_at: Set(expires_at.to_owned()),
        created_at: Set(now_utc()),
    };

    let result = refresh_token::Entity::insert(model)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    refresh_token::Entity::find_by_id(result.last_insert_id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?
        .ok_or_else(|| AppError::DbError("Failed to retrieve created refresh token".into()))
}

pub async fn find_refresh_token_by_hash(
    db: &impl ConnectionTrait,
    token_hash: &str,
) -> Result<Option<refresh_token::Model>, AppError> {
    use sea_orm::ColumnTrait;
    use sea_orm::QueryFilter;
    refresh_token::Entity::find()
        .filter(refresh_token::Column::TokenHash.eq(token_hash))
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn delete_refresh_token(db: &impl ConnectionTrait, id: i32) -> Result<(), AppError> {
    refresh_token::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    Ok(())
}

pub async fn delete_refresh_tokens_for_user(
    db: &impl ConnectionTrait,
    user_id: i32,
) -> Result<(), AppError> {
    use sea_orm::ColumnTrait;
    use sea_orm::QueryFilter;
    refresh_token::Entity::delete_many()
        .filter(refresh_token::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    Ok(())
}

pub async fn delete_expired_refresh_tokens(db: &impl ConnectionTrait) -> Result<u64, AppError> {
    let now = now_utc();
    use sea_orm::ColumnTrait;
    use sea_orm::QueryFilter;
    let result = refresh_token::Entity::delete_many()
        .filter(refresh_token::Column::ExpiresAt.lt(now))
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    Ok(result.rows_affected)
}

// --- API key CRUD ---

pub async fn create_api_key(
    db: &impl ConnectionTrait,
    name: &str,
    key_hash: &str,
    key_prefix: &str,
    created_by: i32,
) -> Result<api_key::Model, AppError> {
    let model = api_key::ActiveModel {
        id: Default::default(),
        name: Set(name.to_owned()),
        key_hash: Set(key_hash.to_owned()),
        key_prefix: Set(key_prefix.to_owned()),
        created_by: Set(created_by),
        created_at: Set(now_utc()),
        last_used_at: Set(None),
    };

    let result = api_key::Entity::insert(model)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;

    api_key::Entity::find_by_id(result.last_insert_id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?
        .ok_or_else(|| AppError::DbError("Failed to retrieve created API key".into()))
}

pub async fn find_api_key_by_hash(
    db: &impl ConnectionTrait,
    key_hash: &str,
) -> Result<Option<api_key::Model>, AppError> {
    use sea_orm::ColumnTrait;
    use sea_orm::QueryFilter;
    api_key::Entity::find()
        .filter(api_key::Column::KeyHash.eq(key_hash))
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn list_api_keys(db: &impl ConnectionTrait) -> Result<Vec<api_key::Model>, AppError> {
    api_key::Entity::find()
        .all(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn find_api_key_by_id(
    db: &impl ConnectionTrait,
    id: i32,
) -> Result<Option<api_key::Model>, AppError> {
    api_key::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn delete_api_key(db: &impl ConnectionTrait, id: i32) -> Result<(), AppError> {
    api_key::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(())
}

// --- Poster meta queries ---

pub async fn count_poster_meta(db: &impl ConnectionTrait) -> Result<u64, AppError> {
    use crate::entity::poster_meta;
    use sea_orm::PaginatorTrait;
    poster_meta::Entity::find()
        .count(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn count_api_keys(db: &impl ConnectionTrait) -> Result<u64, AppError> {
    use sea_orm::PaginatorTrait;
    api_key::Entity::find()
        .count(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub async fn list_poster_meta_by_kind(
    db: &impl ConnectionTrait,
    image_type: crate::cache::ImageType,
    page: u64,
    page_size: u64,
) -> Result<(Vec<crate::entity::poster_meta::Model>, u64), AppError> {
    use crate::entity::poster_meta;
    use sea_orm::{PaginatorTrait, QueryFilter, ColumnTrait};

    let paginator = poster_meta::Entity::find()
        .filter(poster_meta::Column::ImageType.eq(image_type.db_value()))
        .paginate(db, page_size);
    let total = paginator.num_items().await.map_err(|e| AppError::DbError(e.to_string()))?;
    let items = paginator
        .fetch_page(page.saturating_sub(1))
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok((items, total))
}

pub async fn batch_update_last_used(
    db: &impl ConnectionTrait,
    ids: &[i32],
) -> Result<(), AppError> {
    if ids.is_empty() {
        return Ok(());
    }
    let now = now_utc();
    use sea_orm::{ColumnTrait, QueryFilter, sea_query::Expr};
    for chunk in ids.chunks(100) {
        api_key::Entity::update_many()
            .col_expr(api_key::Column::LastUsedAt, Expr::value(now.clone()))
            .filter(api_key::Column::Id.is_in(chunk.iter().copied()))
            .exec(db)
            .await
            .map_err(|e| AppError::DbError(e.to_string()))?;
    }
    Ok(())
}

// --- Global settings ---

pub async fn get_global_settings(
    db: &impl ConnectionTrait,
) -> Result<HashMap<String, String>, AppError> {
    let rows = global_settings::Entity::find()
        .all(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(rows.into_iter().map(|r| (r.key, r.value)).collect())
}

pub async fn set_global_setting(
    db: &impl ConnectionTrait,
    key: &str,
    value: &str,
) -> Result<(), AppError> {
    let model = global_settings::ActiveModel {
        key: Set(key.to_string()),
        value: Set(value.to_string()),
    };
    global_settings::Entity::insert(model)
        .on_conflict(
            sea_orm::sea_query::OnConflict::column(global_settings::Column::Key)
                .update_column(global_settings::Column::Value)
                .to_owned(),
        )
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(())
}

// --- Per-key settings ---

pub async fn get_api_key_settings(
    db: &impl ConnectionTrait,
    api_key_id: i32,
) -> Result<Option<api_key_settings::Model>, AppError> {
    api_key_settings::Entity::find_by_id(api_key_id)
        .one(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))
}

pub struct UpsertApiKeySettings<'a> {
    pub api_key_id: i32,
    pub poster_source: &'a str,
    pub fanart_lang: &'a str,
    pub fanart_textless: bool,
    pub ratings_limit: i32,
    pub ratings_order: &'a str,
    pub poster_position: &'a str,
    pub logo_ratings_limit: i32,
    pub backdrop_ratings_limit: i32,
    pub poster_badge_style: &'a str,
    pub logo_badge_style: &'a str,
    pub backdrop_badge_style: &'a str,
    pub poster_label_style: &'a str,
    pub logo_label_style: &'a str,
    pub backdrop_label_style: &'a str,
    pub poster_badge_direction: &'a str,
}

pub async fn upsert_api_key_settings(
    db: &impl ConnectionTrait,
    params: UpsertApiKeySettings<'_>,
) -> Result<(), AppError> {
    let model = api_key_settings::ActiveModel {
        api_key_id: Set(params.api_key_id),
        poster_source: Set(params.poster_source.to_string()),
        fanart_lang: Set(params.fanart_lang.to_string()),
        fanart_textless: Set(params.fanart_textless),
        ratings_limit: Set(params.ratings_limit),
        ratings_order: Set(params.ratings_order.to_string()),
        poster_position: Set(params.poster_position.to_string()),
        logo_ratings_limit: Set(params.logo_ratings_limit),
        backdrop_ratings_limit: Set(params.backdrop_ratings_limit),
        poster_badge_style: Set(params.poster_badge_style.to_string()),
        logo_badge_style: Set(params.logo_badge_style.to_string()),
        backdrop_badge_style: Set(params.backdrop_badge_style.to_string()),
        poster_label_style: Set(params.poster_label_style.to_string()),
        logo_label_style: Set(params.logo_label_style.to_string()),
        backdrop_label_style: Set(params.backdrop_label_style.to_string()),
        poster_badge_direction: Set(params.poster_badge_direction.to_string()),
    };
    api_key_settings::Entity::insert(model)
        .on_conflict(
            sea_orm::sea_query::OnConflict::column(api_key_settings::Column::ApiKeyId)
                .update_columns([
                    api_key_settings::Column::PosterSource,
                    api_key_settings::Column::FanartLang,
                    api_key_settings::Column::FanartTextless,
                    api_key_settings::Column::RatingsLimit,
                    api_key_settings::Column::RatingsOrder,
                    api_key_settings::Column::PosterPosition,
                    api_key_settings::Column::LogoRatingsLimit,
                    api_key_settings::Column::BackdropRatingsLimit,
                    api_key_settings::Column::PosterBadgeStyle,
                    api_key_settings::Column::LogoBadgeStyle,
                    api_key_settings::Column::BackdropBadgeStyle,
                    api_key_settings::Column::PosterLabelStyle,
                    api_key_settings::Column::LogoLabelStyle,
                    api_key_settings::Column::BackdropLabelStyle,
                    api_key_settings::Column::PosterBadgeDirection,
                ])
                .to_owned(),
        )
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(())
}

pub async fn delete_api_key_settings(
    db: &impl ConnectionTrait,
    api_key_id: i32,
) -> Result<(), AppError> {
    api_key_settings::Entity::delete_by_id(api_key_id)
        .exec(db)
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(())
}

// --- Effective poster settings ---

#[derive(Debug, Clone, serde::Serialize)]
pub struct PosterSettings {
    pub poster_source: Arc<str>,
    pub fanart_lang: Arc<str>,
    pub fanart_textless: bool,
    pub ratings_limit: i32,
    pub ratings_order: Arc<str>,
    pub is_default: bool,
    pub poster_position: Arc<str>,
    pub logo_ratings_limit: i32,
    pub backdrop_ratings_limit: i32,
    pub poster_badge_style: Arc<str>,
    pub logo_badge_style: Arc<str>,
    pub backdrop_badge_style: Arc<str>,
    pub poster_label_style: Arc<str>,
    pub logo_label_style: Arc<str>,
    pub backdrop_label_style: Arc<str>,
    pub poster_badge_direction: Arc<str>,
    /// Set when `?lang=` query param overrides the stored fanart_lang at request time.
    #[serde(skip)]
    pub lang_override: bool,
}

impl Default for PosterSettings {
    fn default() -> Self {
        Self {
            poster_source: Arc::from(SOURCE_TMDB),
            fanart_lang: Arc::from("en"),
            fanart_textless: false,
            ratings_limit: default_ratings_limit(),
            ratings_order: Arc::from("mal,imdb,lb,rt,mc,rta,tmdb,trakt"),
            is_default: true,
            poster_position: Arc::from(POS_BOTTOM_CENTER),
            logo_ratings_limit: default_logo_backdrop_ratings_limit(),
            backdrop_ratings_limit: default_logo_backdrop_ratings_limit(),
            poster_badge_style: Arc::from(STYLE_DEFAULT),
            logo_badge_style: Arc::from(STYLE_VERTICAL),
            backdrop_badge_style: Arc::from(STYLE_VERTICAL),
            poster_label_style: Arc::from(LABEL_ICON),
            logo_label_style: Arc::from(LABEL_ICON),
            backdrop_label_style: Arc::from(LABEL_ICON),
            poster_badge_direction: Arc::from(DIRECTION_DEFAULT),
            lang_override: false,
        }
    }
}

/// Parse raw global settings (key-value HashMap) into a PosterSettings struct.
pub fn parse_global_poster_settings(globals: &HashMap<String, String>) -> PosterSettings {
    if globals.is_empty() {
        return PosterSettings::default();
    }
    let defaults = PosterSettings::default();
    let arc_or = |key: &str, default: Arc<str>| -> Arc<str> {
        globals.get(key).map(|s| Arc::from(s.as_str())).unwrap_or(default)
    };
    PosterSettings {
        poster_source: arc_or("poster_source", defaults.poster_source),
        fanart_lang: arc_or("fanart_lang", defaults.fanart_lang),
        fanart_textless: globals
            .get("fanart_textless")
            .map(|v| v == "true")
            .unwrap_or(defaults.fanart_textless),
        ratings_limit: globals
            .get("ratings_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.ratings_limit),
        ratings_order: arc_or("ratings_order", defaults.ratings_order),
        is_default: true,
        poster_position: arc_or("poster_position", defaults.poster_position),
        logo_ratings_limit: globals
            .get("logo_ratings_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.logo_ratings_limit),
        backdrop_ratings_limit: globals
            .get("backdrop_ratings_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.backdrop_ratings_limit),
        poster_badge_style: arc_or("poster_badge_style", defaults.poster_badge_style),
        logo_badge_style: arc_or("logo_badge_style", defaults.logo_badge_style),
        backdrop_badge_style: arc_or("backdrop_badge_style", defaults.backdrop_badge_style),
        poster_label_style: arc_or("poster_label_style", defaults.poster_label_style),
        logo_label_style: arc_or("logo_label_style", defaults.logo_label_style),
        backdrop_label_style: arc_or("backdrop_label_style", defaults.backdrop_label_style),
        poster_badge_direction: arc_or("poster_badge_direction", defaults.poster_badge_direction),
        lang_override: false,
    }
}

pub async fn get_effective_poster_settings(
    db: &impl ConnectionTrait,
    api_key_id: i32,
    cached_globals: Option<&PosterSettings>,
) -> PosterSettings {
    // Check per-key settings first
    match get_api_key_settings(db, api_key_id).await {
        Ok(Some(s)) => {
            return PosterSettings {
                poster_source: Arc::from(s.poster_source.as_str()),
                fanart_lang: Arc::from(s.fanart_lang.as_str()),
                fanart_textless: s.fanart_textless,
                ratings_limit: s.ratings_limit,
                ratings_order: Arc::from(s.ratings_order.as_str()),
                is_default: false,
                poster_position: Arc::from(s.poster_position.as_str()),
                logo_ratings_limit: s.logo_ratings_limit,
                backdrop_ratings_limit: s.backdrop_ratings_limit,
                poster_badge_style: Arc::from(s.poster_badge_style.as_str()),
                logo_badge_style: Arc::from(s.logo_badge_style.as_str()),
                backdrop_badge_style: Arc::from(s.backdrop_badge_style.as_str()),
                poster_label_style: Arc::from(s.poster_label_style.as_str()),
                logo_label_style: Arc::from(s.logo_label_style.as_str()),
                backdrop_label_style: Arc::from(s.backdrop_label_style.as_str()),
                poster_badge_direction: Arc::from(s.poster_badge_direction.as_str()),
                lang_override: false,
            };
        }
        Ok(None) => {} // no per-key override, fall through
        Err(e) => {
            tracing::warn!(error = %e, api_key_id, "failed to load per-key settings, falling back");
        }
    }
    // Use cached global settings if provided
    if let Some(globals) = cached_globals {
        return globals.clone();
    }
    // Otherwise load from DB
    match get_global_settings(db).await {
        Ok(ref globals) => parse_global_poster_settings(globals),
        Err(e) => {
            tracing::warn!(error = %e, "failed to load global settings, using defaults");
            PosterSettings::default()
        }
    }
}

pub async fn set_global_settings_batch(
    db: &DatabaseConnection,
    settings: &[(&str, &str)],
) -> Result<(), AppError> {
    let txn = db
        .begin()
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    for (key, value) in settings {
        set_global_setting(&txn, key, value).await?;
    }
    txn.commit()
        .await
        .map_err(|e| AppError::DbError(e.to_string()))?;
    Ok(())
}

