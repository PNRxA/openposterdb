use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait, Set, TransactionTrait};
use zeroize::Zeroizing;

use std::collections::HashMap;
use std::sync::Arc;

use crate::entity::{admin_user, api_key, api_key_settings, global_settings, refresh_token};
use crate::error::AppError;
use crate::services::ratings::RatingSource;

// --- Setting value constants ---

/// Implements `Serialize`, `Deserialize`, `Display` (via `as_str()`/`parse()`), for enums with `as_str` and `parse` methods.
macro_rules! impl_str_enum {
    ($ty:ty) => {
        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let s: String = serde::Deserialize::deserialize(deserializer)?;
                Self::parse(&s).map_err(serde::de::Error::custom)
            }
        }

        impl std::fmt::Display for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

/// Parses a setting value from a DB string, logging a warning and returning the default on failure.
fn parse_setting_or_default<T, F>(value: &str, key: &str, parse: F, default: T) -> T
where
    F: FnOnce(&str) -> Result<T, AppError>,
{
    match parse(value) {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!(key, value, "invalid setting value in DB, using default");
            default
        }
    }
}

// --- Setting value constants (private, only used by enum parse/as_str) ---

const SOURCE_TMDB: &str = "t";
const SOURCE_FANART: &str = "f";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeStyle {
    Horizontal,
    Vertical,
    Default,
}

impl BadgeStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Horizontal => STYLE_HORIZONTAL,
            Self::Vertical => STYLE_VERTICAL,
            Self::Default => STYLE_DEFAULT,
        }
    }

    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            STYLE_HORIZONTAL => Ok(Self::Horizontal),
            STYLE_VERTICAL => Ok(Self::Vertical),
            STYLE_DEFAULT => Ok(Self::Default),
            _ => Err(AppError::BadRequest(
                format!("badge_style must be '{STYLE_HORIZONTAL}', '{STYLE_VERTICAL}', or '{STYLE_DEFAULT}'"),
            )),
        }
    }

    /// Returns `true` if the style is vertical.
    /// `Default` is treated as non-vertical (horizontal is the safe fallback).
    /// Callers should resolve `Default` via `.resolve()` before calling this.
    pub fn is_vertical(self) -> bool {
        self == Self::Vertical
    }

    /// Resolve `Default` to match the resolved badge direction.
    pub fn resolve(self, direction: BadgeDirection) -> Self {
        if self != Self::Default {
            return self;
        }
        if direction.is_vertical() {
            Self::Vertical
        } else {
            Self::Horizontal
        }
    }

    /// The effective style once the badge shape is applied. A pill always renders
    /// as a horizontal lozenge, so its style is normalized to `Horizontal`
    /// regardless of the configured value. Both the renderer and the cache key
    /// go through this so pill+vertical and pill+horizontal don't produce two
    /// cache entries (or two renders) for byte-identical output.
    pub fn for_shape(self, shape: BadgeShape) -> Self {
        match shape {
            BadgeShape::Pill => Self::Horizontal,
            BadgeShape::Rounded => self,
        }
    }
}

impl_str_enum!(BadgeStyle);

const STYLE_HORIZONTAL: &str = "h";
const STYLE_VERTICAL: &str = "v";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeDirection {
    Horizontal,
    Vertical,
    Default,
}

impl BadgeDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Horizontal => STYLE_HORIZONTAL,
            Self::Vertical => STYLE_VERTICAL,
            Self::Default => DIRECTION_DEFAULT,
        }
    }

    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            STYLE_HORIZONTAL => Ok(Self::Horizontal),
            STYLE_VERTICAL => Ok(Self::Vertical),
            DIRECTION_DEFAULT => Ok(Self::Default),
            _ => Err(AppError::BadRequest(
                format!("badge_direction must be '{DIRECTION_DEFAULT}', '{STYLE_HORIZONTAL}', or '{STYLE_VERTICAL}'"),
            )),
        }
    }

    /// Returns `true` if the direction is vertical.
    /// `Default` is treated as non-vertical (horizontal is the safe fallback).
    /// Callers should resolve `Default` via `.resolve()` before calling this.
    pub fn is_vertical(self) -> bool {
        self == Self::Vertical
    }

    /// Resolve `Default` to `Horizontal` or `Vertical` based on poster position.
    pub fn resolve(self, position: BadgePosition) -> Self {
        if self != Self::Default {
            return self;
        }
        if position.is_center_horizontal() {
            Self::Horizontal
        } else {
            Self::Vertical
        }
    }
}

impl_str_enum!(BadgeDirection);

const DIRECTION_DEFAULT: &str = "d";
const STYLE_DEFAULT: &str = "d";

const LABEL_ICON: &str = "i";
const LABEL_TEXT: &str = "t";
const LABEL_OFFICIAL: &str = "o";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelStyle {
    Icon,
    Text,
    Official,
}

impl LabelStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Icon => LABEL_ICON,
            Self::Text => LABEL_TEXT,
            Self::Official => LABEL_OFFICIAL,
        }
    }

    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            LABEL_ICON => Ok(Self::Icon),
            LABEL_TEXT => Ok(Self::Text),
            LABEL_OFFICIAL => Ok(Self::Official),
            _ => Err(AppError::BadRequest(
                format!("label_style must be '{LABEL_TEXT}', '{LABEL_ICON}', or '{LABEL_OFFICIAL}'"),
            )),
        }
    }

    pub fn uses_icon(&self) -> bool {
        matches!(self, Self::Icon | Self::Official)
    }
}

impl_str_enum!(LabelStyle);

const SHAPE_ROUNDED: &str = "r";
const SHAPE_PILL: &str = "p";

/// Corner shape of a rating badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeShape {
    /// Slightly rounded rectangle (the original look).
    Rounded,
    /// Fully rounded ends — a "pill" (corner radius = half the short axis).
    Pill,
}

impl BadgeShape {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rounded => SHAPE_ROUNDED,
            Self::Pill => SHAPE_PILL,
        }
    }

    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            SHAPE_ROUNDED => Ok(Self::Rounded),
            SHAPE_PILL => Ok(Self::Pill),
            _ => Err(AppError::BadRequest(
                format!("badge_shape must be '{SHAPE_ROUNDED}' or '{SHAPE_PILL}'"),
            )),
        }
    }
}

impl_str_enum!(BadgeShape);

const BG_DEFAULT: &str = "d";
const BG_DARK: &str = "k";
const BG_TRANSPARENT: &str = "t";
const BG_NONE: &str = "n";

/// Background treatment behind a rating badge's label and value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeBackground {
    /// Source-coloured label section + dark value section (the original look).
    Default,
    /// Uniformly dark badge (no source colour).
    Dark,
    /// Semi-transparent background that lets the artwork show through.
    Transparent,
    /// No background at all — label/value drawn directly on the image with a
    /// subtle shadow for legibility.
    None,
}

impl BadgeBackground {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Default => BG_DEFAULT,
            Self::Dark => BG_DARK,
            Self::Transparent => BG_TRANSPARENT,
            Self::None => BG_NONE,
        }
    }

    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            BG_DEFAULT => Ok(Self::Default),
            BG_DARK => Ok(Self::Dark),
            BG_TRANSPARENT => Ok(Self::Transparent),
            BG_NONE => Ok(Self::None),
            _ => Err(AppError::BadRequest(
                format!("badge_background must be '{BG_DEFAULT}', '{BG_DARK}', '{BG_TRANSPARENT}', or '{BG_NONE}'"),
            )),
        }
    }
}

impl_str_enum!(BadgeBackground);

/// Visual appearance of a badge: its corner shape and background treatment.
/// Bundled so it can be threaded through the render pipeline as one value.
/// `BadgeAppearance::default()` reproduces the original badge look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadgeAppearance {
    pub shape: BadgeShape,
    pub background: BadgeBackground,
}

impl Default for BadgeAppearance {
    fn default() -> Self {
        Self {
            shape: BadgeShape::Rounded,
            background: BadgeBackground::Default,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSource {
    Tmdb,
    Fanart,
}

impl ImageSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tmdb => SOURCE_TMDB,
            Self::Fanart => SOURCE_FANART,
        }
    }

    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            SOURCE_TMDB => Ok(Self::Tmdb),
            SOURCE_FANART => Ok(Self::Fanart),
            _ => Err(AppError::BadRequest(
                format!("image_source must be '{SOURCE_TMDB}' or '{SOURCE_FANART}'"),
            )),
        }
    }

    pub fn is_fanart(self) -> bool {
        self == Self::Fanart
    }
}

impl_str_enum!(ImageSource);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgePosition {
    BottomCenter,
    TopCenter,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl BadgePosition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BottomCenter => POS_BOTTOM_CENTER,
            Self::TopCenter => POS_TOP_CENTER,
            Self::Left => POS_LEFT,
            Self::Right => POS_RIGHT,
            Self::TopLeft => POS_TOP_LEFT,
            Self::TopRight => POS_TOP_RIGHT,
            Self::BottomLeft => POS_BOTTOM_LEFT,
            Self::BottomRight => POS_BOTTOM_RIGHT,
        }
    }

    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            POS_BOTTOM_CENTER => Ok(Self::BottomCenter),
            POS_TOP_CENTER => Ok(Self::TopCenter),
            POS_LEFT => Ok(Self::Left),
            POS_RIGHT => Ok(Self::Right),
            POS_TOP_LEFT => Ok(Self::TopLeft),
            POS_TOP_RIGHT => Ok(Self::TopRight),
            POS_BOTTOM_LEFT => Ok(Self::BottomLeft),
            POS_BOTTOM_RIGHT => Ok(Self::BottomRight),
            _ => Err(AppError::BadRequest(
                format!("poster_position must be '{POS_BOTTOM_CENTER}', '{POS_TOP_CENTER}', '{POS_LEFT}', '{POS_RIGHT}', '{POS_TOP_LEFT}', '{POS_TOP_RIGHT}', '{POS_BOTTOM_LEFT}', or '{POS_BOTTOM_RIGHT}'"),
            )),
        }
    }

    pub fn is_top(self) -> bool {
        matches!(self, Self::TopCenter | Self::TopLeft | Self::TopRight)
    }

    pub fn is_bottom(self) -> bool {
        matches!(self, Self::BottomCenter | Self::BottomLeft | Self::BottomRight)
    }

    pub fn is_left(self) -> bool {
        matches!(self, Self::Left | Self::TopLeft | Self::BottomLeft)
    }

    pub fn is_right(self) -> bool {
        matches!(self, Self::Right | Self::TopRight | Self::BottomRight)
    }

    pub fn is_center_horizontal(self) -> bool {
        matches!(self, Self::BottomCenter | Self::TopCenter)
    }

    /// The top and bottom anchors that preserve this position's horizontal
    /// alignment. Returns `(top_variant, bottom_variant)`.
    fn top_bottom_variants(self) -> (Self, Self) {
        if self.is_left() {
            (Self::TopLeft, Self::BottomLeft)
        } else if self.is_right() {
            (Self::TopRight, Self::BottomRight)
        } else {
            (Self::TopCenter, Self::BottomCenter)
        }
    }

    /// The left and right anchors that preserve this position's vertical
    /// alignment. Returns `(left_variant, right_variant)`.
    fn left_right_variants(self) -> (Self, Self) {
        if self.is_top() {
            (Self::TopLeft, Self::TopRight)
        } else if self.is_bottom() {
            (Self::BottomLeft, Self::BottomRight)
        } else {
            (Self::Left, Self::Right)
        }
    }

    /// Anchor positions for splitting badges across two opposite sides.
    ///
    /// Returns `(primary, opposite)`: the primary anchor keeps the side implied
    /// by this position (so badges that previously sat at the bottom still start
    /// at the bottom), and the opposite anchor is its mirror across the split
    /// axis. When `split_top_bottom` is true the split is vertical (top/bottom,
    /// used for horizontal badge rows); otherwise it is horizontal (left/right,
    /// used for a vertical badge column).
    pub fn split_anchors(self, split_top_bottom: bool) -> (Self, Self) {
        if split_top_bottom {
            let (top, bottom) = self.top_bottom_variants();
            if self.is_top() {
                (top, bottom)
            } else {
                (bottom, top)
            }
        } else {
            let (left, right) = self.left_right_variants();
            if self.is_right() {
                (right, left)
            } else {
                (left, right)
            }
        }
    }
}

impl_str_enum!(BadgePosition);

const POS_BOTTOM_CENTER: &str = "bc";
const POS_TOP_CENTER: &str = "tc";
const POS_LEFT: &str = "l";
const POS_RIGHT: &str = "r";
const POS_TOP_LEFT: &str = "tl";
const POS_TOP_RIGHT: &str = "tr";
const POS_BOTTOM_LEFT: &str = "bl";
const POS_BOTTOM_RIGHT: &str = "br";

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

    /// Target width for episode stills at this size.
    /// Smaller than backdrops since episodes are typically used as thumbnails.
    pub fn episode_target_width(self) -> u32 {
        match self {
            Self::Small => 480,
            Self::Medium => 780,
            Self::Large => 1280,
            Self::VeryLarge => 1920,
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
            crate::cache::ImageType::Episode => self.episode_target_width() as f32 / 780.0,
            // Seasons render as 2:3 posters, so they share the poster width/scale.
            crate::cache::ImageType::Season => self.poster_target_width() as f32 / 580.0,
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
            "imageSize must be 'small', 'medium', 'large', 'very-large', or 'verylarge'".into(),
        ))?;
    if size == ImageSize::Small && kind != crate::cache::ImageType::Backdrop && kind != crate::cache::ImageType::Episode {
        return Err(AppError::BadRequest(
            "imageSize 'small' is only valid for backdrops and episodes".into(),
        ));
    }
    Ok(size)
}

pub fn default_lang() -> String {
    "en".to_string()
}

pub fn default_ratings_limit() -> i32 {
    3
}

pub fn default_logo_backdrop_ratings_limit() -> i32 {
    5
}

pub fn default_ratings_order() -> String {
    "mal,imdb,lb,rt,mc,rta,tmdb,trakt,mdblist,ebert".to_string()
}

/// Default rating-source exclusion list: empty (exclude nothing).
pub fn default_ratings_exclude() -> String {
    String::new()
}

pub fn default_poster_position() -> BadgePosition {
    BadgePosition::BottomCenter
}

pub fn default_poster_badge_style() -> BadgeStyle {
    BadgeStyle::Default
}

pub fn default_logo_badge_style() -> BadgeStyle {
    BadgeStyle::Vertical
}

pub fn default_backdrop_badge_style() -> BadgeStyle {
    BadgeStyle::Vertical
}

pub fn default_label_style() -> LabelStyle {
    LabelStyle::Official
}

pub fn default_badge_shape() -> BadgeShape {
    BadgeShape::Rounded
}

pub fn default_badge_background() -> BadgeBackground {
    BadgeBackground::Default
}

pub fn default_poster_badge_direction() -> BadgeDirection {
    BadgeDirection::Default
}

pub fn default_episode_position() -> BadgePosition {
    BadgePosition::TopRight
}

pub fn default_episode_badge_style() -> BadgeStyle {
    BadgeStyle::Vertical
}

pub fn default_episode_badge_direction() -> BadgeDirection {
    BadgeDirection::Vertical
}

pub fn default_episode_badge_size() -> BadgeSize {
    BadgeSize::Large
}

pub fn default_episode_ratings_limit() -> i32 {
    1
}

// Season posters render through the 2:3 poster pipeline, so their badge
// defaults mirror the poster defaults rather than the episode (still) ones.
pub fn default_season_position() -> BadgePosition {
    BadgePosition::BottomCenter
}

pub fn default_season_badge_style() -> BadgeStyle {
    BadgeStyle::Default
}

pub fn default_season_badge_direction() -> BadgeDirection {
    BadgeDirection::Default
}

pub fn default_season_badge_size() -> BadgeSize {
    BadgeSize::Medium
}

pub fn default_season_ratings_limit() -> i32 {
    default_ratings_limit()
}

pub fn default_backdrop_position() -> BadgePosition {
    BadgePosition::TopRight
}

pub fn default_backdrop_badge_direction() -> BadgeDirection {
    BadgeDirection::Default
}

/// Maximum backdrop edge inset, expressed as a percentage of the image
/// dimension. Capped at half so the inset can never push past the centre.
pub const MAX_EDGE_INSET: i32 = 50;

pub fn default_backdrop_edge_inset() -> i32 {
    0
}

/// Clamp a stored/requested edge inset to the supported `0..=MAX_EDGE_INSET`
/// range so out-of-range values can never produce runaway cache keys or
/// nonsensical placement.
pub fn clamp_edge_inset(value: i32) -> i32 {
    value.clamp(0, MAX_EDGE_INSET)
}

// --- Badge size ---

pub fn default_badge_size() -> BadgeSize {
    BadgeSize::Medium
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeSize {
    ExtraSmall,
    Small,
    Medium,
    Large,
    ExtraLarge,
}

impl BadgeSize {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExtraSmall => "xs",
            Self::Small => "s",
            Self::Medium => "m",
            Self::Large => "l",
            Self::ExtraLarge => "xl",
        }
    }

    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            "xs" => Ok(Self::ExtraSmall),
            "s" => Ok(Self::Small),
            "m" => Ok(Self::Medium),
            "l" => Ok(Self::Large),
            "xl" => Ok(Self::ExtraLarge),
            _ => Err(AppError::BadRequest(
                "badge_size must be 'xs', 's', 'm', 'l', or 'xl'".into(),
            )),
        }
    }

    pub fn scale_factor(self) -> f32 {
        match self {
            Self::ExtraSmall => 0.5,
            Self::Small => 0.75,
            Self::Medium => 1.0,
            Self::Large => 1.25,
            Self::ExtraLarge => 1.5,
        }
    }

    pub fn cache_suffix(self) -> &'static str {
        match self {
            Self::ExtraSmall => ".bxs",
            Self::Small => ".bs",
            Self::Medium => ".bm",
            Self::Large => ".bl",
            Self::ExtraLarge => ".bxl",
        }
    }
}

impl_str_enum!(BadgeSize);

// --- Poster aspect-ratio fit ---

const FIT_NATIVE: &str = "native";
const FIT_COVER: &str = "cover";
const FIT_PAD: &str = "pad";
const FIT_BLUR: &str = "blur";

/// How a poster is fit to the standard 2:3 output frame.
///
/// Downstream apps (Stremio addons, aiometadata) place posters in fixed 2:3
/// containers, so a non-2:3 source poster gets cropped by the client — cutting
/// off the title/logo baked into the art (issue #15). Every mode except
/// `Native` normalizes the output to an exact 2:3 frame so nothing is clipped
/// by the consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosterFit {
    /// Keep the source aspect ratio (legacy behavior, no normalization).
    Native,
    /// Scale to fill the 2:3 frame, center-cropping the overflow.
    Cover,
    /// Fit the whole poster inside the 2:3 frame, padding with solid black bars.
    Pad,
    /// Fit the whole poster inside the 2:3 frame, filling the bars with a
    /// blurred, zoomed copy of the poster.
    Blur,
}

impl PosterFit {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Native => FIT_NATIVE,
            Self::Cover => FIT_COVER,
            Self::Pad => FIT_PAD,
            Self::Blur => FIT_BLUR,
        }
    }

    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            FIT_NATIVE => Ok(Self::Native),
            FIT_COVER => Ok(Self::Cover),
            FIT_PAD => Ok(Self::Pad),
            FIT_BLUR => Ok(Self::Blur),
            _ => Err(AppError::BadRequest(format!(
                "poster_fit must be '{FIT_NATIVE}', '{FIT_COVER}', '{FIT_PAD}', or '{FIT_BLUR}'"
            ))),
        }
    }

    /// Cache key suffix. `Native` (the default) returns an empty string so that
    /// poster cache keys written before this feature existed — all rendered
    /// natively — remain valid and the default behavior reuses them with no
    /// re-render. Opting into `cover`/`pad`/`blur` emits a token, producing a
    /// distinct cache entry.
    pub fn cache_suffix(self) -> &'static str {
        match self {
            Self::Native => "",
            Self::Cover => ".fc",
            Self::Pad => ".fp",
            Self::Blur => ".fb",
        }
    }
}

impl_str_enum!(PosterFit);

/// Default poster fit: `Native` (keep the source aspect ratio — legacy
/// behavior). Normalization to 2:3 (`cover`/`pad`/`blur`) is opt-in.
pub fn default_poster_fit() -> PosterFit {
    PosterFit::Native
}

/// Validate ratings_limit is 0–10 (one slot per available rating source).
pub fn validate_ratings_limit(limit: i32) -> Result<(), AppError> {
    if (0..=10).contains(&limit) {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "ratings_limit must be between 0 and 10".into(),
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


/// Validate a language code: 2–5 ASCII alphanumeric chars or hyphens (e.g. "en", "pt-BR").
pub fn validate_lang(lang: &str) -> Result<(), AppError> {
    if lang.len() >= 2
        && lang.len() <= 5
        && lang
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "lang must be 2-5 ASCII alphanumeric characters (e.g. 'en', 'de', 'pt-BR')"
                .into(),
        ))
    }
}

/// Validate a comma-separated list of rating source keys to exclude from display.
/// Same rules as `validate_ratings_order` (known keys, no duplicates, empty OK).
pub fn validate_ratings_exclude(exclude: &str) -> Result<(), AppError> {
    validate_ratings_order(exclude)
}

/// Validate the remaining string-based render settings that aren't covered by enum deserialization.
pub fn validate_render_settings(
    lang: &str,
    ratings_limit: i32,
    ratings_order: &str,
    ratings_exclude: &str,
    logo_ratings_limit: i32,
    backdrop_ratings_limit: i32,
    episode_ratings_limit: i32,
    season_ratings_limit: i32,
) -> Result<(), AppError> {
    validate_lang(lang)?;
    validate_ratings_order(ratings_order)?;
    validate_ratings_exclude(ratings_exclude)?;
    for limit in [ratings_limit, logo_ratings_limit, backdrop_ratings_limit, episode_ratings_limit, season_ratings_limit] {
        validate_ratings_limit(limit)?;
    }
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
    fn default_lang_returns_en() {
        assert_eq!(default_lang(), "en");
    }

    #[test]
    fn validate_ratings_limit_accepts_valid() {
        for i in 0..=10 {
            assert!(validate_ratings_limit(i).is_ok(), "limit {i} should be valid");
        }
    }

    #[test]
    fn validate_ratings_limit_rejects_negative() {
        assert!(validate_ratings_limit(-1).is_err());
    }

    #[test]
    fn validate_ratings_limit_rejects_too_large() {
        assert!(validate_ratings_limit(11).is_err());
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
    fn validate_ratings_exclude_accepts_empty_and_valid() {
        assert!(validate_ratings_exclude("").is_ok());
        assert!(validate_ratings_exclude("rt").is_ok());
        assert!(validate_ratings_exclude("rt,tmdb,trakt").is_ok());
    }

    #[test]
    fn validate_ratings_exclude_rejects_unknown_and_duplicate() {
        assert!(validate_ratings_exclude("bogus").is_err());
        assert!(validate_ratings_exclude("rt,rt").is_err());
    }

    #[test]
    fn validate_lang_valid_codes() {
        assert!(validate_lang("en").is_ok());
        assert!(validate_lang("de").is_ok());
        assert!(validate_lang("fr").is_ok());
        assert!(validate_lang("ja").is_ok());
        assert!(validate_lang("pt-BR").is_ok());
        assert!(validate_lang("zh-CN").is_ok());
    }

    #[test]
    fn validate_lang_rejects_too_short() {
        assert!(validate_lang("e").is_err());
        assert!(validate_lang("").is_err());
    }

    #[test]
    fn validate_lang_rejects_too_long() {
        assert!(validate_lang("abcdef").is_err());
        assert!(validate_lang("toolongvalue").is_err());
    }

    #[test]
    fn validate_lang_rejects_special_chars() {
        assert!(validate_lang("../../").is_err());
        assert!(validate_lang("en\0").is_err());
        assert!(validate_lang("a b").is_err());
        assert!(validate_lang("en/de").is_err());
    }

    #[test]
    fn validate_image_source_accepts_valid() {
        assert!(ImageSource::parse("t").is_ok());
        assert!(ImageSource::parse("f").is_ok());
    }

    #[test]
    fn validate_image_source_rejects_invalid() {
        assert!(ImageSource::parse("tmdb").is_err());
        assert!(ImageSource::parse("fanart").is_err());
        assert!(ImageSource::parse("").is_err());
        assert!(ImageSource::parse("x").is_err());
    }

    #[test]
    fn validate_poster_position_accepts_valid() {
        assert!(BadgePosition::parse("bc").is_ok());
        assert!(BadgePosition::parse("tc").is_ok());
        assert!(BadgePosition::parse("l").is_ok());
        assert!(BadgePosition::parse("r").is_ok());
        assert!(BadgePosition::parse("tl").is_ok());
        assert!(BadgePosition::parse("tr").is_ok());
        assert!(BadgePosition::parse("bl").is_ok());
        assert!(BadgePosition::parse("br").is_ok());
    }

    #[test]
    fn validate_poster_position_rejects_invalid() {
        assert!(BadgePosition::parse("center").is_err());
        assert!(BadgePosition::parse("").is_err());
        assert!(BadgePosition::parse("bottom-center").is_err());
        assert!(BadgePosition::parse("middle").is_err());
    }

    #[test]
    fn default_poster_position_returns_bottom_center() {
        assert_eq!(default_poster_position(), BadgePosition::BottomCenter);
    }

    #[test]
    fn default_poster_badge_style_returns_default() {
        assert_eq!(default_poster_badge_style(), BadgeStyle::Default);
    }

    #[test]
    fn default_backdrop_badge_style_returns_vertical() {
        assert_eq!(default_backdrop_badge_style(), BadgeStyle::Vertical);
    }

    #[test]
    fn validate_badge_style_accepts_valid() {
        assert!(BadgeStyle::parse("h").is_ok());
        assert!(BadgeStyle::parse("v").is_ok());
        assert!(BadgeStyle::parse("d").is_ok());
    }

    #[test]
    fn validate_badge_style_rejects_invalid() {
        assert!(BadgeStyle::parse("diagonal").is_err());
        assert!(BadgeStyle::parse("").is_err());
    }

    #[test]
    fn resolve_badge_style_default_follows_direction() {
        assert_eq!(BadgeStyle::parse("d").unwrap().resolve(BadgeDirection::parse("h").unwrap()), BadgeStyle::Horizontal);
        assert_eq!(BadgeStyle::parse("d").unwrap().resolve(BadgeDirection::parse("v").unwrap()), BadgeStyle::Vertical);
    }

    #[test]
    fn resolve_badge_style_explicit_passes_through() {
        assert_eq!(BadgeStyle::parse("h").unwrap().resolve(BadgeDirection::parse("v").unwrap()), BadgeStyle::Horizontal);
        assert_eq!(BadgeStyle::parse("v").unwrap().resolve(BadgeDirection::parse("h").unwrap()), BadgeStyle::Vertical);
    }

    #[test]
    fn label_style_parse_accepts_valid() {
        assert!(LabelStyle::parse("t").is_ok());
        assert!(LabelStyle::parse("i").is_ok());
        assert!(LabelStyle::parse("o").is_ok());
    }

    #[test]
    fn label_style_parse_rejects_invalid() {
        assert!(LabelStyle::parse("emoji").is_err());
        assert!(LabelStyle::parse("").is_err());
    }

    #[test]
    fn default_label_style_returns_official() {
        assert_eq!(default_label_style(), LabelStyle::Official);
    }

    #[test]
    fn default_poster_badge_direction_returns_default() {
        assert_eq!(default_poster_badge_direction(), BadgeDirection::Default);
    }

    #[test]
    fn default_backdrop_badge_direction_returns_default() {
        assert_eq!(default_backdrop_badge_direction(), BadgeDirection::Default);
    }

    #[test]
    fn validate_badge_direction_accepts_default() {
        assert!(BadgeDirection::parse("d").is_ok());
        assert!(BadgeDirection::parse("h").is_ok());
        assert!(BadgeDirection::parse("v").is_ok());
    }

    #[test]
    fn validate_badge_direction_rejects_invalid() {
        assert!(BadgeDirection::parse("diagonal").is_err());
        assert!(BadgeDirection::parse("").is_err());
    }

    #[test]
    fn resolve_badge_direction_default_center_positions() {
        assert_eq!(BadgeDirection::parse("d").unwrap().resolve(BadgePosition::parse("bc").unwrap()), BadgeDirection::Horizontal);
        assert_eq!(BadgeDirection::parse("d").unwrap().resolve(BadgePosition::parse("tc").unwrap()), BadgeDirection::Horizontal);
    }

    #[test]
    fn resolve_badge_direction_default_side_positions() {
        assert_eq!(BadgeDirection::parse("d").unwrap().resolve(BadgePosition::parse("l").unwrap()), BadgeDirection::Vertical);
        assert_eq!(BadgeDirection::parse("d").unwrap().resolve(BadgePosition::parse("r").unwrap()), BadgeDirection::Vertical);
    }

    #[test]
    fn resolve_badge_direction_default_corner_positions() {
        assert_eq!(BadgeDirection::parse("d").unwrap().resolve(BadgePosition::parse("tl").unwrap()), BadgeDirection::Vertical);
        assert_eq!(BadgeDirection::parse("d").unwrap().resolve(BadgePosition::parse("tr").unwrap()), BadgeDirection::Vertical);
        assert_eq!(BadgeDirection::parse("d").unwrap().resolve(BadgePosition::parse("bl").unwrap()), BadgeDirection::Vertical);
        assert_eq!(BadgeDirection::parse("d").unwrap().resolve(BadgePosition::parse("br").unwrap()), BadgeDirection::Vertical);
    }

    #[test]
    fn resolve_badge_direction_explicit_passes_through() {
        assert_eq!(BadgeDirection::parse("h").unwrap().resolve(BadgePosition::parse("l").unwrap()), BadgeDirection::Horizontal);
        assert_eq!(BadgeDirection::parse("v").unwrap().resolve(BadgePosition::parse("bc").unwrap()), BadgeDirection::Vertical);
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

    // --- Enum round-trip serialization/deserialization tests ---

    #[test]
    fn badge_style_round_trip() {
        for variant in [BadgeStyle::Horizontal, BadgeStyle::Vertical, BadgeStyle::Default] {
            let s = variant.as_str();
            let parsed = BadgeStyle::parse(s).unwrap();
            assert_eq!(parsed, variant, "round-trip failed for {s}");
            // Serde round-trip
            let json = serde_json::to_string(&variant).unwrap();
            let deser: BadgeStyle = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, variant);
        }
    }

    #[test]
    fn badge_direction_round_trip() {
        for variant in [BadgeDirection::Horizontal, BadgeDirection::Vertical, BadgeDirection::Default] {
            let s = variant.as_str();
            let parsed = BadgeDirection::parse(s).unwrap();
            assert_eq!(parsed, variant);
            let json = serde_json::to_string(&variant).unwrap();
            let deser: BadgeDirection = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, variant);
        }
    }

    #[test]
    fn label_style_round_trip() {
        for variant in [LabelStyle::Icon, LabelStyle::Text, LabelStyle::Official] {
            let s = variant.as_str();
            let parsed = LabelStyle::parse(s).unwrap();
            assert_eq!(parsed, variant);
            let json = serde_json::to_string(&variant).unwrap();
            let deser: LabelStyle = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, variant);
        }
    }

    #[test]
    fn image_source_round_trip() {
        for variant in [ImageSource::Tmdb, ImageSource::Fanart] {
            let s = variant.as_str();
            let parsed = ImageSource::parse(s).unwrap();
            assert_eq!(parsed, variant);
            let json = serde_json::to_string(&variant).unwrap();
            let deser: ImageSource = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, variant);
        }
    }

    #[test]
    fn badge_position_round_trip() {
        let all = [
            BadgePosition::BottomCenter, BadgePosition::TopCenter,
            BadgePosition::Left, BadgePosition::Right,
            BadgePosition::TopLeft, BadgePosition::TopRight,
            BadgePosition::BottomLeft, BadgePosition::BottomRight,
        ];
        for variant in all {
            let s = variant.as_str();
            let parsed = BadgePosition::parse(s).unwrap();
            assert_eq!(parsed, variant, "round-trip failed for {s}");
            let json = serde_json::to_string(&variant).unwrap();
            let deser: BadgePosition = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, variant);
        }
    }

    #[test]
    fn badge_size_round_trip() {
        let all = [
            BadgeSize::ExtraSmall, BadgeSize::Small, BadgeSize::Medium,
            BadgeSize::Large, BadgeSize::ExtraLarge,
        ];
        for variant in all {
            let s = variant.as_str();
            let parsed = BadgeSize::parse(s).unwrap();
            assert_eq!(parsed, variant, "round-trip failed for {s}");
            let json = serde_json::to_string(&variant).unwrap();
            let deser: BadgeSize = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, variant);
            // Verify scale_factor and cache_suffix don't panic
            let _ = variant.scale_factor();
            let _ = variant.cache_suffix();
        }
    }

    #[test]
    fn badge_style_display_matches_as_str() {
        assert_eq!(format!("{}", BadgeStyle::Horizontal), "h");
        assert_eq!(format!("{}", BadgeStyle::Vertical), "v");
        assert_eq!(format!("{}", BadgeStyle::Default), "d");
    }

    #[test]
    fn badge_position_helper_methods() {
        assert!(BadgePosition::TopCenter.is_top());
        assert!(BadgePosition::TopLeft.is_top());
        assert!(BadgePosition::TopRight.is_top());
        assert!(!BadgePosition::BottomCenter.is_top());

        assert!(BadgePosition::BottomCenter.is_bottom());
        assert!(BadgePosition::BottomLeft.is_bottom());
        assert!(BadgePosition::BottomRight.is_bottom());
        assert!(!BadgePosition::TopCenter.is_bottom());

        assert!(BadgePosition::Left.is_left());
        assert!(BadgePosition::TopLeft.is_left());
        assert!(BadgePosition::BottomLeft.is_left());
        assert!(!BadgePosition::Right.is_left());

        assert!(BadgePosition::Right.is_right());
        assert!(BadgePosition::TopRight.is_right());
        assert!(BadgePosition::BottomRight.is_right());
        assert!(!BadgePosition::Left.is_right());
    }

    #[test]
    fn split_anchors_top_bottom_preserves_horizontal_alignment() {
        // Horizontal rows (split_top_bottom = true): primary keeps the
        // configured vertical side, opposite is the mirror; horizontal
        // alignment is preserved.
        assert_eq!(
            BadgePosition::BottomCenter.split_anchors(true),
            (BadgePosition::BottomCenter, BadgePosition::TopCenter)
        );
        assert_eq!(
            BadgePosition::TopCenter.split_anchors(true),
            (BadgePosition::TopCenter, BadgePosition::BottomCenter)
        );
        assert_eq!(
            BadgePosition::TopRight.split_anchors(true),
            (BadgePosition::TopRight, BadgePosition::BottomRight)
        );
        assert_eq!(
            BadgePosition::BottomLeft.split_anchors(true),
            (BadgePosition::BottomLeft, BadgePosition::TopLeft)
        );
        // Vertically-centered anchor (Left): defaults to bottom-primary.
        assert_eq!(
            BadgePosition::Left.split_anchors(true),
            (BadgePosition::BottomLeft, BadgePosition::TopLeft)
        );
    }

    #[test]
    fn split_anchors_left_right_preserves_vertical_alignment() {
        // Vertical column (split_top_bottom = false): primary keeps the
        // configured horizontal side, opposite is the mirror; vertical
        // alignment is preserved.
        assert_eq!(
            BadgePosition::Left.split_anchors(false),
            (BadgePosition::Left, BadgePosition::Right)
        );
        assert_eq!(
            BadgePosition::Right.split_anchors(false),
            (BadgePosition::Right, BadgePosition::Left)
        );
        assert_eq!(
            BadgePosition::TopRight.split_anchors(false),
            (BadgePosition::TopRight, BadgePosition::TopLeft)
        );
        assert_eq!(
            BadgePosition::BottomLeft.split_anchors(false),
            (BadgePosition::BottomLeft, BadgePosition::BottomRight)
        );
        // Horizontally-centered anchor (BottomCenter): defaults to left-primary.
        assert_eq!(
            BadgePosition::BottomCenter.split_anchors(false),
            (BadgePosition::BottomLeft, BadgePosition::BottomRight)
        );
    }

    #[test]
    fn label_style_uses_icon() {
        assert!(LabelStyle::Icon.uses_icon());
        assert!(LabelStyle::Official.uses_icon());
        assert!(!LabelStyle::Text.uses_icon());
    }

    #[test]
    fn image_source_is_fanart() {
        assert!(ImageSource::Fanart.is_fanart());
        assert!(!ImageSource::Tmdb.is_fanart());
    }

    #[test]
    fn badge_size_scale_factors_ordered() {
        let xs = BadgeSize::ExtraSmall.scale_factor();
        let s = BadgeSize::Small.scale_factor();
        let m = BadgeSize::Medium.scale_factor();
        let l = BadgeSize::Large.scale_factor();
        let xl = BadgeSize::ExtraLarge.scale_factor();
        assert!(xs < s);
        assert!(s < m);
        assert!(m < l);
        assert!(l < xl);
    }

    #[test]
    fn parse_global_render_settings_with_all_enum_fields() {
        let mut globals = HashMap::new();
        globals.insert("image_source".into(), "f".into());
        globals.insert("poster_position".into(), "tl".into());
        globals.insert("poster_badge_style".into(), "v".into());
        globals.insert("logo_badge_style".into(), "h".into());
        globals.insert("backdrop_badge_style".into(), "h".into());
        globals.insert("poster_label_style".into(), "t".into());
        globals.insert("logo_label_style".into(), "i".into());
        globals.insert("backdrop_label_style".into(), "o".into());
        globals.insert("poster_badge_direction".into(), "h".into());
        globals.insert("poster_badge_size".into(), "xl".into());
        globals.insert("logo_badge_size".into(), "xs".into());
        globals.insert("backdrop_badge_size".into(), "l".into());

        let settings = parse_global_render_settings(&globals);
        assert_eq!(settings.image_source, ImageSource::Fanart);
        assert_eq!(settings.poster_position, BadgePosition::TopLeft);
        assert_eq!(settings.poster_badge_style, BadgeStyle::Vertical);
        assert_eq!(settings.logo_badge_style, BadgeStyle::Horizontal);
        assert_eq!(settings.backdrop_badge_style, BadgeStyle::Horizontal);
        assert_eq!(settings.poster_label_style, LabelStyle::Text);
        assert_eq!(settings.logo_label_style, LabelStyle::Icon);
        assert_eq!(settings.backdrop_label_style, LabelStyle::Official);
        assert_eq!(settings.poster_badge_direction, BadgeDirection::Horizontal);
        assert_eq!(settings.poster_badge_size, BadgeSize::ExtraLarge);
        assert_eq!(settings.logo_badge_size, BadgeSize::ExtraSmall);
        assert_eq!(settings.backdrop_badge_size, BadgeSize::Large);
    }

    #[test]
    fn parse_global_render_settings_invalid_values_use_defaults() {
        let mut globals = HashMap::new();
        globals.insert("poster_badge_style".into(), "invalid".into());
        globals.insert("poster_position".into(), "nowhere".into());
        globals.insert("poster_badge_size".into(), "gigantic".into());

        let settings = parse_global_render_settings(&globals);
        // Invalid values should fall back to defaults
        assert_eq!(settings.poster_badge_style, BadgeStyle::Default);
        assert_eq!(settings.poster_position, BadgePosition::BottomCenter);
        assert_eq!(settings.poster_badge_size, BadgeSize::Medium);
    }

    #[test]
    fn render_settings_default_values() {
        let defaults = RenderSettings::default();
        assert_eq!(defaults.image_source, ImageSource::Tmdb);
        assert_eq!(defaults.poster_position, BadgePosition::BottomCenter);
        assert_eq!(defaults.poster_badge_style, BadgeStyle::Default);
        assert_eq!(defaults.logo_badge_style, BadgeStyle::Vertical);
        assert_eq!(defaults.backdrop_badge_style, BadgeStyle::Vertical);
        assert_eq!(defaults.poster_label_style, LabelStyle::Official);
        assert_eq!(defaults.poster_badge_direction, BadgeDirection::Default);
        assert!(!defaults.poster_badge_split);
        assert_eq!(defaults.poster_badge_size, BadgeSize::Medium);
        assert_eq!(defaults.logo_badge_size, BadgeSize::Medium);
        assert_eq!(defaults.backdrop_badge_size, BadgeSize::Medium);
        assert_eq!(defaults.poster_fit, PosterFit::Native);
        assert!(defaults.is_default);
    }

    #[test]
    fn poster_fit_parse_round_trip_and_tokens() {
        for f in [PosterFit::Native, PosterFit::Cover, PosterFit::Pad, PosterFit::Blur] {
            assert_eq!(PosterFit::parse(f.as_str()).unwrap(), f);
        }
        assert!(PosterFit::parse("bogus").is_err());
        assert_eq!(default_poster_fit(), PosterFit::Native);
        // Native must keep an empty token so pre-feature poster cache keys survive.
        assert_eq!(PosterFit::Native.cache_suffix(), "");
        assert_eq!(PosterFit::Cover.cache_suffix(), ".fc");
        assert_eq!(PosterFit::Pad.cache_suffix(), ".fp");
        assert_eq!(PosterFit::Blur.cache_suffix(), ".fb");
    }

    #[test]
    fn parse_global_render_settings_reads_poster_fit() {
        let mut globals = HashMap::new();
        globals.insert("poster_fit".into(), "pad".into());
        let settings = parse_global_render_settings(&globals);
        assert_eq!(settings.poster_fit, PosterFit::Pad);
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

// --- Image meta queries ---

pub async fn count_image_meta(db: &impl ConnectionTrait) -> Result<u64, AppError> {
    use crate::entity::image_meta;
    use sea_orm::PaginatorTrait;
    image_meta::Entity::find()
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

pub async fn list_image_meta_by_kind(
    db: &impl ConnectionTrait,
    image_type: crate::cache::ImageType,
    page: u64,
    page_size: u64,
) -> Result<(Vec<crate::entity::image_meta::Model>, u64), AppError> {
    use crate::entity::image_meta;
    use sea_orm::{PaginatorTrait, QueryFilter, QueryOrder, ColumnTrait};

    let paginator = image_meta::Entity::find()
        .filter(image_meta::Column::ImageType.eq(image_type.db_value()))
        .order_by_desc(image_meta::Column::CreatedAt)
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
    pub image_source: &'a str,
    pub lang: &'a str,
    pub textless: bool,
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
    pub poster_badge_split: bool,
    pub poster_fit: &'a str,
    pub poster_badge_size: &'a str,
    pub logo_badge_size: &'a str,
    pub backdrop_badge_size: &'a str,
    pub backdrop_position: &'a str,
    pub backdrop_badge_direction: &'a str,
    pub episode_ratings_limit: i32,
    pub episode_badge_style: &'a str,
    pub episode_label_style: &'a str,
    pub episode_badge_size: &'a str,
    pub episode_position: &'a str,
    pub episode_badge_direction: &'a str,
    pub episode_blur: bool,
    pub ratings_exclude: &'a str,
    pub poster_badge_shape: &'a str,
    pub logo_badge_shape: &'a str,
    pub backdrop_badge_shape: &'a str,
    pub episode_badge_shape: &'a str,
    pub poster_badge_background: &'a str,
    pub logo_badge_background: &'a str,
    pub backdrop_badge_background: &'a str,
    pub episode_badge_background: &'a str,
    pub backdrop_edge_inset_x: i32,
    pub backdrop_edge_inset_y: i32,
    pub season_ratings_limit: i32,
    pub season_badge_style: &'a str,
    pub season_label_style: &'a str,
    pub season_badge_size: &'a str,
    pub season_position: &'a str,
    pub season_badge_direction: &'a str,
    pub season_badge_shape: &'a str,
    pub season_badge_background: &'a str,
}

pub async fn upsert_api_key_settings(
    db: &impl ConnectionTrait,
    params: UpsertApiKeySettings<'_>,
) -> Result<(), AppError> {
    let model = api_key_settings::ActiveModel {
        api_key_id: Set(params.api_key_id),
        image_source: Set(params.image_source.to_string()),
        lang: Set(params.lang.to_string()),
        textless: Set(params.textless),
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
        poster_badge_split: Set(params.poster_badge_split),
        poster_fit: Set(params.poster_fit.to_string()),
        poster_badge_size: Set(params.poster_badge_size.to_string()),
        logo_badge_size: Set(params.logo_badge_size.to_string()),
        backdrop_badge_size: Set(params.backdrop_badge_size.to_string()),
        backdrop_position: Set(params.backdrop_position.to_string()),
        backdrop_badge_direction: Set(params.backdrop_badge_direction.to_string()),
        episode_ratings_limit: Set(params.episode_ratings_limit),
        episode_badge_style: Set(params.episode_badge_style.to_string()),
        episode_label_style: Set(params.episode_label_style.to_string()),
        episode_badge_size: Set(params.episode_badge_size.to_string()),
        episode_position: Set(params.episode_position.to_string()),
        episode_badge_direction: Set(params.episode_badge_direction.to_string()),
        episode_blur: Set(params.episode_blur),
        ratings_exclude: Set(params.ratings_exclude.to_string()),
        poster_badge_shape: Set(params.poster_badge_shape.to_string()),
        logo_badge_shape: Set(params.logo_badge_shape.to_string()),
        backdrop_badge_shape: Set(params.backdrop_badge_shape.to_string()),
        episode_badge_shape: Set(params.episode_badge_shape.to_string()),
        poster_badge_background: Set(params.poster_badge_background.to_string()),
        logo_badge_background: Set(params.logo_badge_background.to_string()),
        backdrop_badge_background: Set(params.backdrop_badge_background.to_string()),
        episode_badge_background: Set(params.episode_badge_background.to_string()),
        backdrop_edge_inset_x: Set(params.backdrop_edge_inset_x),
        backdrop_edge_inset_y: Set(params.backdrop_edge_inset_y),
        season_ratings_limit: Set(params.season_ratings_limit),
        season_badge_style: Set(params.season_badge_style.to_string()),
        season_label_style: Set(params.season_label_style.to_string()),
        season_badge_size: Set(params.season_badge_size.to_string()),
        season_position: Set(params.season_position.to_string()),
        season_badge_direction: Set(params.season_badge_direction.to_string()),
        season_badge_shape: Set(params.season_badge_shape.to_string()),
        season_badge_background: Set(params.season_badge_background.to_string()),
    };
    api_key_settings::Entity::insert(model)
        .on_conflict(
            sea_orm::sea_query::OnConflict::column(api_key_settings::Column::ApiKeyId)
                .update_columns([
                    api_key_settings::Column::ImageSource,
                    api_key_settings::Column::Lang,
                    api_key_settings::Column::Textless,
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
                    api_key_settings::Column::PosterBadgeSplit,
                    api_key_settings::Column::PosterFit,
                    api_key_settings::Column::PosterBadgeSize,
                    api_key_settings::Column::LogoBadgeSize,
                    api_key_settings::Column::BackdropBadgeSize,
                    api_key_settings::Column::BackdropPosition,
                    api_key_settings::Column::BackdropBadgeDirection,
                    api_key_settings::Column::EpisodeRatingsLimit,
                    api_key_settings::Column::EpisodeBadgeStyle,
                    api_key_settings::Column::EpisodeLabelStyle,
                    api_key_settings::Column::EpisodeBadgeSize,
                    api_key_settings::Column::EpisodePosition,
                    api_key_settings::Column::EpisodeBadgeDirection,
                    api_key_settings::Column::EpisodeBlur,
                    api_key_settings::Column::RatingsExclude,
                    api_key_settings::Column::PosterBadgeShape,
                    api_key_settings::Column::LogoBadgeShape,
                    api_key_settings::Column::BackdropBadgeShape,
                    api_key_settings::Column::EpisodeBadgeShape,
                    api_key_settings::Column::PosterBadgeBackground,
                    api_key_settings::Column::LogoBadgeBackground,
                    api_key_settings::Column::BackdropBadgeBackground,
                    api_key_settings::Column::EpisodeBadgeBackground,
                    api_key_settings::Column::BackdropEdgeInsetX,
                    api_key_settings::Column::BackdropEdgeInsetY,
                    api_key_settings::Column::SeasonRatingsLimit,
                    api_key_settings::Column::SeasonBadgeStyle,
                    api_key_settings::Column::SeasonLabelStyle,
                    api_key_settings::Column::SeasonBadgeSize,
                    api_key_settings::Column::SeasonPosition,
                    api_key_settings::Column::SeasonBadgeDirection,
                    api_key_settings::Column::SeasonBadgeShape,
                    api_key_settings::Column::SeasonBadgeBackground,
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

// --- Effective render settings ---

#[derive(Debug, Clone, serde::Serialize)]
pub struct RenderSettings {
    pub image_source: ImageSource,
    pub lang: Arc<str>,
    pub textless: bool,
    pub ratings_limit: i32,
    pub ratings_order: Arc<str>,
    pub ratings_exclude: Arc<str>,
    pub is_default: bool,
    pub poster_position: BadgePosition,
    pub logo_ratings_limit: i32,
    pub backdrop_ratings_limit: i32,
    pub poster_badge_style: BadgeStyle,
    pub logo_badge_style: BadgeStyle,
    pub backdrop_badge_style: BadgeStyle,
    pub poster_label_style: LabelStyle,
    pub logo_label_style: LabelStyle,
    pub backdrop_label_style: LabelStyle,
    pub poster_badge_direction: BadgeDirection,
    /// When true, poster badges are split evenly across two opposite sides
    /// (left/right for a vertical badge layout, top/bottom for horizontal rows).
    pub poster_badge_split: bool,
    /// How non-2:3 posters are fit to the standard 2:3 output frame.
    pub poster_fit: PosterFit,
    pub poster_badge_size: BadgeSize,
    pub logo_badge_size: BadgeSize,
    pub backdrop_badge_size: BadgeSize,
    pub backdrop_position: BadgePosition,
    pub backdrop_badge_direction: BadgeDirection,
    /// Inset (percent of width) of backdrop ratings from the anchored
    /// horizontal edge. Only applies to left/right positions.
    pub backdrop_edge_inset_x: i32,
    /// Inset (percent of height) of backdrop ratings from the anchored
    /// vertical edge. Only applies to top/bottom positions.
    pub backdrop_edge_inset_y: i32,
    pub episode_ratings_limit: i32,
    pub episode_badge_style: BadgeStyle,
    pub episode_label_style: LabelStyle,
    pub episode_badge_size: BadgeSize,
    pub episode_position: BadgePosition,
    pub episode_badge_direction: BadgeDirection,
    pub episode_blur: bool,
    pub poster_badge_shape: BadgeShape,
    pub logo_badge_shape: BadgeShape,
    pub backdrop_badge_shape: BadgeShape,
    pub episode_badge_shape: BadgeShape,
    pub poster_badge_background: BadgeBackground,
    pub logo_badge_background: BadgeBackground,
    pub backdrop_badge_background: BadgeBackground,
    pub episode_badge_background: BadgeBackground,
    pub season_ratings_limit: i32,
    pub season_badge_style: BadgeStyle,
    pub season_label_style: LabelStyle,
    pub season_badge_size: BadgeSize,
    pub season_position: BadgePosition,
    pub season_badge_direction: BadgeDirection,
    pub season_badge_shape: BadgeShape,
    pub season_badge_background: BadgeBackground,
}

impl RenderSettings {
    /// Badge appearance (shape + background) for posters.
    pub fn poster_appearance(&self) -> BadgeAppearance {
        BadgeAppearance { shape: self.poster_badge_shape, background: self.poster_badge_background }
    }
    /// Badge appearance (shape + background) for logos.
    pub fn logo_appearance(&self) -> BadgeAppearance {
        BadgeAppearance { shape: self.logo_badge_shape, background: self.logo_badge_background }
    }
    /// Badge appearance (shape + background) for backdrops.
    pub fn backdrop_appearance(&self) -> BadgeAppearance {
        BadgeAppearance { shape: self.backdrop_badge_shape, background: self.backdrop_badge_background }
    }
    /// Badge appearance (shape + background) for episode stills.
    pub fn episode_appearance(&self) -> BadgeAppearance {
        BadgeAppearance { shape: self.episode_badge_shape, background: self.episode_badge_background }
    }
    /// Badge appearance (shape + background) for season posters.
    pub fn season_appearance(&self) -> BadgeAppearance {
        BadgeAppearance { shape: self.season_badge_shape, background: self.season_badge_background }
    }
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            image_source: ImageSource::Tmdb,
            lang: Arc::from("en"),
            textless: false,
            ratings_limit: default_ratings_limit(),
            ratings_order: Arc::from(default_ratings_order()),
            ratings_exclude: Arc::from(""),
            is_default: true,
            poster_position: BadgePosition::BottomCenter,
            logo_ratings_limit: default_logo_backdrop_ratings_limit(),
            backdrop_ratings_limit: default_logo_backdrop_ratings_limit(),
            poster_badge_style: BadgeStyle::Default,
            logo_badge_style: BadgeStyle::Vertical,
            backdrop_badge_style: BadgeStyle::Vertical,
            poster_label_style: LabelStyle::Official,
            logo_label_style: LabelStyle::Official,
            backdrop_label_style: LabelStyle::Official,
            poster_badge_direction: BadgeDirection::Default,
            poster_badge_split: false,
            poster_fit: default_poster_fit(),
            poster_badge_size: BadgeSize::Medium,
            logo_badge_size: BadgeSize::Medium,
            backdrop_badge_size: BadgeSize::Medium,
            backdrop_position: default_backdrop_position(),
            backdrop_badge_direction: default_backdrop_badge_direction(),
            backdrop_edge_inset_x: default_backdrop_edge_inset(),
            backdrop_edge_inset_y: default_backdrop_edge_inset(),
            episode_ratings_limit: default_episode_ratings_limit(),
            episode_badge_style: default_episode_badge_style(),
            episode_label_style: default_label_style(),
            episode_badge_size: default_episode_badge_size(),
            episode_position: default_episode_position(),
            episode_badge_direction: default_episode_badge_direction(),
            episode_blur: false,
            poster_badge_shape: default_badge_shape(),
            logo_badge_shape: default_badge_shape(),
            backdrop_badge_shape: default_badge_shape(),
            episode_badge_shape: default_badge_shape(),
            poster_badge_background: default_badge_background(),
            logo_badge_background: default_badge_background(),
            backdrop_badge_background: default_badge_background(),
            episode_badge_background: default_badge_background(),
            season_ratings_limit: default_season_ratings_limit(),
            season_badge_style: default_season_badge_style(),
            season_label_style: default_label_style(),
            season_badge_size: default_season_badge_size(),
            season_position: default_season_position(),
            season_badge_direction: default_season_badge_direction(),
            season_badge_shape: default_badge_shape(),
            season_badge_background: default_badge_background(),
        }
    }
}

/// Parse raw global settings (key-value HashMap) into a `RenderSettings` struct.
pub fn parse_global_render_settings(globals: &HashMap<String, String>) -> RenderSettings {
    if globals.is_empty() {
        return RenderSettings::default();
    }
    let defaults = RenderSettings::default();
    let arc_or = |key: &str, default: Arc<str>| -> Arc<str> {
        globals.get(key).map(|s| Arc::from(s.as_str())).unwrap_or(default)
    };
    /// Look up `key` in `globals`, parse it with `parse`, warn + return `default` on failure.
    fn global_or<T: Copy>(globals: &HashMap<String, String>, key: &str, parse: fn(&str) -> Result<T, AppError>, default: T) -> T {
        globals.get(key).map(|s| parse_setting_or_default(s, key, parse, default)).unwrap_or(default)
    }
    RenderSettings {
        image_source: global_or(globals, "image_source", ImageSource::parse, defaults.image_source),
        lang: arc_or("lang", defaults.lang),
        textless: globals
            .get("textless")
            .map(|v| v == "true")
            .unwrap_or(defaults.textless),
        ratings_limit: globals
            .get("ratings_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.ratings_limit),
        ratings_order: arc_or("ratings_order", defaults.ratings_order),
        ratings_exclude: arc_or("ratings_exclude", defaults.ratings_exclude),
        is_default: true,
        poster_position: global_or(globals, "poster_position", BadgePosition::parse, defaults.poster_position),
        logo_ratings_limit: globals
            .get("logo_ratings_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.logo_ratings_limit),
        backdrop_ratings_limit: globals
            .get("backdrop_ratings_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.backdrop_ratings_limit),
        poster_badge_style: global_or(globals, "poster_badge_style", BadgeStyle::parse, defaults.poster_badge_style),
        logo_badge_style: global_or(globals, "logo_badge_style", BadgeStyle::parse, defaults.logo_badge_style),
        backdrop_badge_style: global_or(globals, "backdrop_badge_style", BadgeStyle::parse, defaults.backdrop_badge_style),
        poster_label_style: global_or(globals, "poster_label_style", LabelStyle::parse, defaults.poster_label_style),
        logo_label_style: global_or(globals, "logo_label_style", LabelStyle::parse, defaults.logo_label_style),
        backdrop_label_style: global_or(globals, "backdrop_label_style", LabelStyle::parse, defaults.backdrop_label_style),
        poster_badge_direction: global_or(globals, "poster_badge_direction", BadgeDirection::parse, defaults.poster_badge_direction),
        poster_badge_split: globals
            .get("poster_badge_split")
            .map(|v| v == "true")
            .unwrap_or(defaults.poster_badge_split),
        poster_fit: global_or(globals, "poster_fit", PosterFit::parse, defaults.poster_fit),
        poster_badge_size: global_or(globals, "poster_badge_size", BadgeSize::parse, defaults.poster_badge_size),
        logo_badge_size: global_or(globals, "logo_badge_size", BadgeSize::parse, defaults.logo_badge_size),
        backdrop_badge_size: global_or(globals, "backdrop_badge_size", BadgeSize::parse, defaults.backdrop_badge_size),
        backdrop_position: global_or(globals, "backdrop_position", BadgePosition::parse, defaults.backdrop_position),
        backdrop_badge_direction: global_or(globals, "backdrop_badge_direction", BadgeDirection::parse, defaults.backdrop_badge_direction),
        backdrop_edge_inset_x: globals
            .get("backdrop_edge_inset_x")
            .and_then(|v| v.parse().ok())
            .map(clamp_edge_inset)
            .unwrap_or(defaults.backdrop_edge_inset_x),
        backdrop_edge_inset_y: globals
            .get("backdrop_edge_inset_y")
            .and_then(|v| v.parse().ok())
            .map(clamp_edge_inset)
            .unwrap_or(defaults.backdrop_edge_inset_y),
        episode_ratings_limit: globals
            .get("episode_ratings_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.episode_ratings_limit),
        episode_badge_style: global_or(globals, "episode_badge_style", BadgeStyle::parse, defaults.episode_badge_style),
        episode_label_style: global_or(globals, "episode_label_style", LabelStyle::parse, defaults.episode_label_style),
        episode_badge_size: global_or(globals, "episode_badge_size", BadgeSize::parse, defaults.episode_badge_size),
        episode_position: global_or(globals, "episode_position", BadgePosition::parse, defaults.episode_position),
        episode_badge_direction: global_or(globals, "episode_badge_direction", BadgeDirection::parse, defaults.episode_badge_direction),
        episode_blur: globals
            .get("episode_blur")
            .map(|v| v == "true")
            .unwrap_or(defaults.episode_blur),
        poster_badge_shape: global_or(globals, "poster_badge_shape", BadgeShape::parse, defaults.poster_badge_shape),
        logo_badge_shape: global_or(globals, "logo_badge_shape", BadgeShape::parse, defaults.logo_badge_shape),
        backdrop_badge_shape: global_or(globals, "backdrop_badge_shape", BadgeShape::parse, defaults.backdrop_badge_shape),
        episode_badge_shape: global_or(globals, "episode_badge_shape", BadgeShape::parse, defaults.episode_badge_shape),
        poster_badge_background: global_or(globals, "poster_badge_background", BadgeBackground::parse, defaults.poster_badge_background),
        logo_badge_background: global_or(globals, "logo_badge_background", BadgeBackground::parse, defaults.logo_badge_background),
        backdrop_badge_background: global_or(globals, "backdrop_badge_background", BadgeBackground::parse, defaults.backdrop_badge_background),
        episode_badge_background: global_or(globals, "episode_badge_background", BadgeBackground::parse, defaults.episode_badge_background),
        season_ratings_limit: globals
            .get("season_ratings_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.season_ratings_limit),
        season_badge_style: global_or(globals, "season_badge_style", BadgeStyle::parse, defaults.season_badge_style),
        season_label_style: global_or(globals, "season_label_style", LabelStyle::parse, defaults.season_label_style),
        season_badge_size: global_or(globals, "season_badge_size", BadgeSize::parse, defaults.season_badge_size),
        season_position: global_or(globals, "season_position", BadgePosition::parse, defaults.season_position),
        season_badge_direction: global_or(globals, "season_badge_direction", BadgeDirection::parse, defaults.season_badge_direction),
        season_badge_shape: global_or(globals, "season_badge_shape", BadgeShape::parse, defaults.season_badge_shape),
        season_badge_background: global_or(globals, "season_badge_background", BadgeBackground::parse, defaults.season_badge_background),
    }
}

pub async fn get_effective_render_settings(
    db: &impl ConnectionTrait,
    api_key_id: i32,
    cached_globals: Option<&RenderSettings>,
) -> RenderSettings {
    // Check per-key settings first
    match get_api_key_settings(db, api_key_id).await {
        Ok(Some(s)) => {
            return RenderSettings {
                image_source: parse_setting_or_default(&s.image_source, "image_source", ImageSource::parse, ImageSource::Tmdb),
                lang: Arc::from(s.lang.as_str()),
                textless: s.textless,
                ratings_limit: s.ratings_limit,
                ratings_order: Arc::from(s.ratings_order.as_str()),
                ratings_exclude: Arc::from(s.ratings_exclude.as_str()),
                is_default: false,
                poster_position: parse_setting_or_default(&s.poster_position, "poster_position", BadgePosition::parse, BadgePosition::BottomCenter),
                logo_ratings_limit: s.logo_ratings_limit,
                backdrop_ratings_limit: s.backdrop_ratings_limit,
                poster_badge_style: parse_setting_or_default(&s.poster_badge_style, "poster_badge_style", BadgeStyle::parse, BadgeStyle::Default),
                logo_badge_style: parse_setting_or_default(&s.logo_badge_style, "logo_badge_style", BadgeStyle::parse, BadgeStyle::Vertical),
                backdrop_badge_style: parse_setting_or_default(&s.backdrop_badge_style, "backdrop_badge_style", BadgeStyle::parse, BadgeStyle::Vertical),
                poster_label_style: parse_setting_or_default(&s.poster_label_style, "poster_label_style", LabelStyle::parse, LabelStyle::Official),
                logo_label_style: parse_setting_or_default(&s.logo_label_style, "logo_label_style", LabelStyle::parse, LabelStyle::Official),
                backdrop_label_style: parse_setting_or_default(&s.backdrop_label_style, "backdrop_label_style", LabelStyle::parse, LabelStyle::Official),
                poster_badge_direction: parse_setting_or_default(&s.poster_badge_direction, "poster_badge_direction", BadgeDirection::parse, BadgeDirection::Default),
                poster_badge_split: s.poster_badge_split,
                poster_fit: parse_setting_or_default(&s.poster_fit, "poster_fit", PosterFit::parse, default_poster_fit()),
                poster_badge_size: parse_setting_or_default(&s.poster_badge_size, "poster_badge_size", BadgeSize::parse, BadgeSize::Medium),
                logo_badge_size: parse_setting_or_default(&s.logo_badge_size, "logo_badge_size", BadgeSize::parse, BadgeSize::Medium),
                backdrop_badge_size: parse_setting_or_default(&s.backdrop_badge_size, "backdrop_badge_size", BadgeSize::parse, BadgeSize::Medium),
                backdrop_position: parse_setting_or_default(&s.backdrop_position, "backdrop_position", BadgePosition::parse, default_backdrop_position()),
                backdrop_badge_direction: parse_setting_or_default(&s.backdrop_badge_direction, "backdrop_badge_direction", BadgeDirection::parse, default_backdrop_badge_direction()),
                backdrop_edge_inset_x: clamp_edge_inset(s.backdrop_edge_inset_x),
                backdrop_edge_inset_y: clamp_edge_inset(s.backdrop_edge_inset_y),
                episode_ratings_limit: s.episode_ratings_limit,
                episode_badge_style: parse_setting_or_default(&s.episode_badge_style, "episode_badge_style", BadgeStyle::parse, default_episode_badge_style()),
                episode_label_style: parse_setting_or_default(&s.episode_label_style, "episode_label_style", LabelStyle::parse, LabelStyle::Official),
                episode_badge_size: parse_setting_or_default(&s.episode_badge_size, "episode_badge_size", BadgeSize::parse, default_episode_badge_size()),
                episode_position: parse_setting_or_default(&s.episode_position, "episode_position", BadgePosition::parse, default_episode_position()),
                episode_badge_direction: parse_setting_or_default(&s.episode_badge_direction, "episode_badge_direction", BadgeDirection::parse, default_episode_badge_direction()),
                episode_blur: s.episode_blur,
                poster_badge_shape: parse_setting_or_default(&s.poster_badge_shape, "poster_badge_shape", BadgeShape::parse, default_badge_shape()),
                logo_badge_shape: parse_setting_or_default(&s.logo_badge_shape, "logo_badge_shape", BadgeShape::parse, default_badge_shape()),
                backdrop_badge_shape: parse_setting_or_default(&s.backdrop_badge_shape, "backdrop_badge_shape", BadgeShape::parse, default_badge_shape()),
                episode_badge_shape: parse_setting_or_default(&s.episode_badge_shape, "episode_badge_shape", BadgeShape::parse, default_badge_shape()),
                poster_badge_background: parse_setting_or_default(&s.poster_badge_background, "poster_badge_background", BadgeBackground::parse, default_badge_background()),
                logo_badge_background: parse_setting_or_default(&s.logo_badge_background, "logo_badge_background", BadgeBackground::parse, default_badge_background()),
                backdrop_badge_background: parse_setting_or_default(&s.backdrop_badge_background, "backdrop_badge_background", BadgeBackground::parse, default_badge_background()),
                episode_badge_background: parse_setting_or_default(&s.episode_badge_background, "episode_badge_background", BadgeBackground::parse, default_badge_background()),
                season_ratings_limit: s.season_ratings_limit,
                season_badge_style: parse_setting_or_default(&s.season_badge_style, "season_badge_style", BadgeStyle::parse, default_season_badge_style()),
                season_label_style: parse_setting_or_default(&s.season_label_style, "season_label_style", LabelStyle::parse, LabelStyle::Official),
                season_badge_size: parse_setting_or_default(&s.season_badge_size, "season_badge_size", BadgeSize::parse, default_season_badge_size()),
                season_position: parse_setting_or_default(&s.season_position, "season_position", BadgePosition::parse, default_season_position()),
                season_badge_direction: parse_setting_or_default(&s.season_badge_direction, "season_badge_direction", BadgeDirection::parse, default_season_badge_direction()),
                season_badge_shape: parse_setting_or_default(&s.season_badge_shape, "season_badge_shape", BadgeShape::parse, default_badge_shape()),
                season_badge_background: parse_setting_or_default(&s.season_badge_background, "season_badge_background", BadgeBackground::parse, default_badge_background()),
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
        Ok(ref globals) => parse_global_render_settings(globals),
        Err(e) => {
            tracing::warn!(error = %e, "failed to load global settings, using defaults");
            RenderSettings::default()
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

