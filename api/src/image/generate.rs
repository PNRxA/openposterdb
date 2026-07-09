use std::sync::Arc;

use ab_glyph::FontArc;
use image::codecs::jpeg::JpegEncoder;
use image::{imageops, DynamicImage, ImageDecoder, ImageReader, Limits, RgbaImage};
use tokio::sync::Semaphore;

use crate::cache;
use crate::error::AppError;
use crate::image::badge;
use crate::services::db::{BadgeAppearance, BadgeDirection, BadgeSize, BadgeStyle, LabelStyle, BadgePosition, PosterFit};
use crate::services::ratings::RatingBadge;
use crate::services::tmdb::TmdbClient;

/// Threshold (ms) above which render is logged as slow.
const SLOW_RENDER_MS: u64 = 2000;

const BADGE_SPACING: u32 = 10;
const BADGE_BOTTOM_MARGIN: u32 = 10;
const BADGE_TOP_MARGIN: u32 = 20;
const BADGE_SIDE_MARGIN: u32 = 15;
const BADGE_ROW_SPACING: u32 = 7;
const BADGE_VERT_SPACING: u32 = 7;
const BACKDROP_SIDE_MARGIN: u32 = 20;
const MAX_BADGES_PER_ROW: usize = 3;
const MAX_VERT_BADGES_PER_ROW: usize = 5;

pub struct ImageParams<'a> {
    pub poster_path: &'a str,
    pub badges: &'a [RatingBadge],
    /// Non-rating overlay badges (quality tiers, language flag/code), each group
    /// placed at its own anchor position.
    pub overlay: &'a OverlaySpec,
    pub tmdb: &'a TmdbClient,

    pub font: &'a FontArc,
    pub quality: u8,
    pub cache_dir: &'a str,
    pub image_stale_secs: u64,
    pub poster_bytes_override: Option<Vec<u8>>,
    pub poster_position: BadgePosition,
    pub badge_style: BadgeStyle,
    pub label_style: LabelStyle,
    pub badge_appearance: BadgeAppearance,
    pub badge_direction: BadgeDirection,
    /// When true, split the badges across two opposite sides of the poster.
    pub poster_badge_split: bool,
    /// How a non-2:3 poster is fit to the standard 2:3 output frame.
    pub poster_fit: PosterFit,
    pub render_semaphore: Arc<Semaphore>,
    /// Target width for the output image. Defaults to 500 for posters.
    pub target_width: u32,
    /// Badge scale factor (1.0 = default size).
    pub badge_scale: f32,
    /// TMDB CDN size string (e.g. "w500", "w780", "original").
    pub tmdb_size: Arc<str>,
    /// Badge size — used to adjust max badges per row.
    pub badge_size: BadgeSize,
    /// When true, skip writing base poster images to disk (CDN handles caching).
    pub external_cache_only: bool,
}

pub async fn generate_poster(params: ImageParams<'_>) -> Result<Vec<u8>, AppError> {
    let ImageParams {
        poster_path,
        badges,
        overlay,
        tmdb,
        font,
        quality,
        cache_dir,
        image_stale_secs,
        poster_bytes_override,
        poster_position,
        badge_style,
        label_style,
        badge_appearance,
        badge_direction,
        poster_badge_split,
        poster_fit,
        render_semaphore,
        target_width,
        badge_scale,
        badge_size,
        tmdb_size,
        external_cache_only,
    } = params;

    let poster_bytes = if let Some(bytes) = poster_bytes_override {
        bytes
    } else if external_cache_only {
        // No filesystem cache — always fetch from TMDB
        tmdb.fetch_poster_bytes(poster_path, &tmdb_size).await?
    } else {
        // Fetch base poster from TMDB, using cache
        let poster_cache = cache::base_poster_path(cache_dir, poster_path, &tmdb_size)?;
        if let Some(entry) = cache::read(&poster_cache, image_stale_secs).await {
            if entry.is_stale {
                // Conditional fetch — send If-Modified-Since to avoid re-downloading unchanged images
                let modified = tokio::fs::metadata(&poster_cache).await.ok()
                    .and_then(|m| m.modified().ok());
                match tmdb.fetch_poster_bytes_conditional(poster_path, &tmdb_size, modified).await? {
                    Some(fresh_bytes) => {
                        cache::write(&poster_cache, &fresh_bytes).await?;
                        fresh_bytes
                    }
                    None => {
                        // 304 Not Modified — touch mtime without rewriting the file
                        if let Ok(f) = tokio::fs::File::open(&poster_cache).await {
                            let _ = f.into_std().await.set_modified(std::time::SystemTime::now());
                        }
                        entry.bytes
                    }
                }
            } else {
                entry.bytes
            }
        } else {
            let bytes = tmdb.fetch_poster_bytes(poster_path, &tmdb_size).await?;
            cache::write(&poster_cache, &bytes).await?;
            bytes
        }
    };

    // Acquire render permit before CPU-bound work — also bounds queue depth
    // so tasks waiting for a blocking thread still count against the limit.
    if render_semaphore.available_permits() == 0 {
        tracing::debug!("render queue full, waiting for permit");
    }
    let _permit = render_semaphore.acquire().await
        .map_err(|_| AppError::Other("render queue closed".into()))?;
    tracing::debug!("poster render started");
    let start = std::time::Instant::now();

    // Move CPU-bound image processing to a blocking thread
    let badges = badges.to_vec();
    let overlay = overlay.clone();
    let font = font.clone();
    let buf = tokio::task::spawn_blocking(move || {
        render_poster_sync(&poster_bytes, &badges, &overlay, &font, quality, poster_position, badge_style, label_style, badge_appearance, badge_direction, target_width, badge_scale, badge_size, poster_badge_split, poster_fit)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    let render_ms = start.elapsed().as_millis() as u64;
    tracing::debug!(elapsed_ms = render_ms, "poster render complete");
    if render_ms > SLOW_RENDER_MS {
        tracing::warn!(
            poster_path,
            render_ms,
            "slow generate_poster"
        );
    }
    Ok(buf)
}

/// Overlay badges in a vertical column, positioned according to `position`.
///
/// `extra_x` / `extra_y` are additional edge insets in pixels (already resolved
/// for this canvas) that push the badges further from the anchored edge. They
/// only take effect on the edge the position anchors to: `extra_y` on top/bottom
/// anchors, `extra_x` on left/right anchors. Centered axes ignore their inset.
/// Callers that don't want insets pass `0, 0`.
fn overlay_vertical_stack(canvas: &mut RgbaImage, badge_images: &[RgbaImage], position: BadgePosition, badge_scale: f32, side_margin_base: u32, extra_x: u32, extra_y: u32) {
    let vert_spacing = (BADGE_VERT_SPACING as f32 * badge_scale).round() as u32;
    let top_margin = (BADGE_TOP_MARGIN as f32 * badge_scale).round() as u32;
    let bottom_margin = (BADGE_BOTTOM_MARGIN as f32 * badge_scale).round() as u32;
    let side_margin = (side_margin_base as f32 * badge_scale).round() as u32;

    let total_badge_height: u32 = badge_images.iter().map(|b| b.height()).sum::<u32>()
        + vert_spacing * (badge_images.len() as u32).saturating_sub(1);
    let max_badge_width: u32 = badge_images.iter().map(|b| b.width()).max().unwrap_or(0);

    // Vertical anchor
    let start_y = if position.is_top() {
        top_margin + extra_y
    } else if position.is_bottom() {
        canvas.height().saturating_sub(total_badge_height + bottom_margin + extra_y)
    } else {
        // "l", "r" — vertically centered
        (canvas.height().saturating_sub(total_badge_height)) / 2
    };

    // Horizontal anchor
    let is_left = position.is_left();
    let is_right = position.is_right();

    let base_x = if is_left {
        side_margin + extra_x
    } else if is_right {
        canvas.width().saturating_sub(max_badge_width + side_margin + extra_x)
    } else {
        // center
        (canvas.width().saturating_sub(max_badge_width)) / 2
    };

    let mut y = start_y;
    for badge_img in badge_images {
        let bx = if is_left {
            base_x
        } else if is_right {
            base_x + max_badge_width.saturating_sub(badge_img.width())
        } else {
            base_x + (max_badge_width.saturating_sub(badge_img.width())) / 2
        };
        imageops::overlay(canvas, badge_img, bx as i64, y as i64);
        y += badge_img.height() + vert_spacing;
    }
}

/// Overlay badges in horizontal rows, positioned according to `position`.
///
/// `extra_x` / `extra_y` behave as in [`overlay_vertical_stack`]: edge insets in
/// resolved pixels that only apply to the anchored edge (centered axes ignore
/// them). Callers that don't want insets pass `0, 0`.
fn overlay_horizontal_rows(canvas: &mut RgbaImage, badge_images: &[RgbaImage], position: BadgePosition, max_per_row: usize, badge_scale: f32, side_margin_base: u32, extra_x: u32, extra_y: u32) {
    let spacing = (BADGE_SPACING as f32 * badge_scale).round() as u32;
    let row_spacing = (BADGE_ROW_SPACING as f32 * badge_scale).round() as u32;
    let top_margin = (BADGE_TOP_MARGIN as f32 * badge_scale).round() as u32;
    let bottom_margin = (BADGE_BOTTOM_MARGIN as f32 * badge_scale).round() as u32;
    let side_margin = (side_margin_base as f32 * badge_scale).round() as u32;

    let rows: Vec<&[RgbaImage]> = badge_images.chunks(max_per_row).collect();
    let badge_height = badge_images.iter().map(|b| b.height()).max().unwrap_or(0);
    let total_height = badge_height * rows.len() as u32
        + row_spacing * (rows.len() as u32).saturating_sub(1);

    // Vertical anchor
    let base_y = if position.is_top() {
        top_margin + extra_y
    } else if position.is_bottom() {
        canvas.height().saturating_sub(total_height + bottom_margin + extra_y)
    } else {
        // "l", "r" — vertically centered
        (canvas.height().saturating_sub(total_height)) / 2
    };

    // Horizontal alignment
    let is_left = position.is_left();
    let is_right = position.is_right();

    for (row_idx, row) in rows.iter().enumerate() {
        let row_width: u32 = row.iter().map(|b| b.width()).sum::<u32>()
            + spacing * (row.len() as u32).saturating_sub(1);
        let y = base_y + row_idx as u32 * (badge_height + row_spacing);

        let start_x = if is_left {
            side_margin + extra_x
        } else if is_right {
            canvas.width().saturating_sub(row_width + side_margin + extra_x)
        } else {
            (canvas.width().saturating_sub(row_width)) / 2
        };

        let mut x = start_x;
        for badge_img in *row {
            let by = y + (badge_height.saturating_sub(badge_img.height())) / 2;
            imageops::overlay(canvas, badge_img, x as i64, by as i64);
            x += badge_img.width() + spacing;
        }
    }
}

/// Placement of the non-rating overlay badges. Quality and language are laid out
/// as independent groups, each at its own anchor position — independent of the
/// rating badges and of each other (issues #1 / #6). Positions apply to
/// poster/backdrop/episode; logos ignore them and stack all badges below the
/// logo.
#[derive(Clone)]
pub struct OverlaySpec {
    pub quality: Vec<badge::OverlayBadge>,
    pub quality_position: BadgePosition,
    /// Layout direction for the quality group. `Default` (auto) resolves from
    /// `quality_position` (column at corners/sides, row at top/bottom-center).
    pub quality_direction: BadgeDirection,
    pub language: Vec<badge::OverlayBadge>,
    pub language_position: BadgePosition,
}

impl Default for OverlaySpec {
    fn default() -> Self {
        Self {
            quality: Vec::new(),
            quality_position: BadgePosition::TopRight,
            quality_direction: BadgeDirection::Default,
            language: Vec::new(),
            language_position: BadgePosition::TopLeft,
        }
    }
}

impl OverlaySpec {
    pub fn is_empty(&self) -> bool {
        self.quality.is_empty() && self.language.is_empty()
    }

    /// Quality + language badges combined, for the logo layout (no anchors).
    fn combined(&self) -> Vec<badge::OverlayBadge> {
        self.quality.iter().chain(&self.language).cloned().collect()
    }
}

/// Render the non-rating overlay badges (quality / language) into images sized
/// to match the rating badges' orientation, ready to place at their anchor.
fn render_overlay_images(
    overlay_badges: &[badge::OverlayBadge],
    font: &FontArc,
    vertical: bool,
    appearance: BadgeAppearance,
    badge_scale: f32,
) -> Vec<RgbaImage> {
    overlay_badges
        .iter()
        .map(|b| {
            if vertical {
                badge::render_overlay_badge_vertical(b, font, appearance, badge_scale)
            } else {
                badge::render_overlay_badge(b, font, appearance, badge_scale)
            }
        })
        .collect()
}

/// Maximum total pixels allowed for a decoded source image (width * height).
/// 8192x8192 = 67M pixels (~256 MB as RGBA) is generous for poster/backdrop art
/// while preventing OOM from crafted images with extreme dimensions.
const MAX_IMAGE_PIXELS: u64 = 8192 * 8192;

/// Decode an image from bytes, rejecting oversized images *before* full decode
/// to avoid OOM from crafted inputs (e.g. PNG bombs).
fn load_image_with_limits(bytes: &[u8]) -> Result<image::DynamicImage, AppError> {
    let reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| AppError::Other(format!("image format detection failed: {e}")))?;

    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_IMAGE_PIXELS * 4); // RGBA = 4 bytes/pixel
    let mut decoder = reader.into_decoder().map_err(AppError::Image)?;
    decoder.set_limits(limits).map_err(AppError::Image)?;

    image::DynamicImage::from_decoder(decoder).map_err(AppError::Image)
}

/// Standard movie-poster aspect ratio is 2:3 (width:height), so the output
/// height for a given width is `width * 3 / 2`.
fn poster_target_height(target_width: u32) -> u32 {
    ((target_width as f64) * 1.5).round() as u32
}

/// Blur strength for `PosterFit::Blur` scales with output width so the effect
/// looks consistent across image sizes. `fast_blur` approximates a Gaussian
/// blur cheaply enough for the largest (2000px) posters.
const BLUR_SIGMA_DIVISOR: f32 = 32.0;

/// Normalize a decoded source poster to the output frame per `fit`, returning
/// the RGBA canvas the badges are then composited onto.
///
/// Every mode except `Native` produces an exact 2:3 frame
/// (`target_width × target_width*3/2`) so downstream apps that place posters in
/// fixed 2:3 containers never crop the art (issue #15).
fn fit_poster(base: DynamicImage, target_width: u32, fit: PosterFit) -> RgbaImage {
    use image::imageops::FilterType::Lanczos3;
    match fit {
        // Preserve the source aspect ratio: scale to target_width only.
        PosterFit::Native => {
            if base.width() == target_width {
                base.to_rgba8()
            } else {
                let scale = target_width as f64 / base.width() as f64;
                let target_height = ((base.height() as f64 * scale).round() as u32).max(1);
                base.resize_exact(target_width, target_height, Lanczos3).to_rgba8()
            }
        }
        // Scale to fill the 2:3 frame, center-cropping the overflow.
        PosterFit::Cover => {
            let th = poster_target_height(target_width);
            base.resize_to_fill(target_width, th, Lanczos3).to_rgba8()
        }
        // Fit the whole poster inside 2:3, padding with solid black bars.
        PosterFit::Pad => {
            let th = poster_target_height(target_width);
            let fitted = base.resize(target_width, th, Lanczos3).to_rgba8();
            let mut canvas = RgbaImage::from_pixel(target_width, th, image::Rgba([0, 0, 0, 255]));
            let x = ((target_width.saturating_sub(fitted.width())) / 2) as i64;
            let y = ((th.saturating_sub(fitted.height())) / 2) as i64;
            imageops::overlay(&mut canvas, &fitted, x, y);
            canvas
        }
        // Fit the whole poster inside 2:3, filling the bars with a blurred,
        // zoomed copy of the poster.
        PosterFit::Blur => {
            let th = poster_target_height(target_width);
            let bg = base.resize_to_fill(target_width, th, Lanczos3).to_rgba8();
            let sigma = (target_width as f32 / BLUR_SIGMA_DIVISOR).max(1.0);
            let mut canvas = imageops::fast_blur(&bg, sigma);
            let fitted = base.resize(target_width, th, Lanczos3).to_rgba8();
            let x = ((target_width.saturating_sub(fitted.width())) / 2) as i64;
            let y = ((th.saturating_sub(fitted.height())) / 2) as i64;
            imageops::overlay(&mut canvas, &fitted, x, y);
            canvas
        }
    }
}

pub fn render_poster_sync(
    poster_bytes: &[u8],
    badges: &[RatingBadge],
    overlay: &OverlaySpec,
    font: &FontArc,
    quality: u8,
    poster_position: BadgePosition,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_appearance: BadgeAppearance,
    badge_direction: BadgeDirection,
    target_width: u32,
    badge_scale: f32,
    badge_size: BadgeSize,
    poster_badge_split: bool,
    poster_fit: PosterFit,
) -> Result<Vec<u8>, AppError> {
    // A pill is a horizontal lozenge (icon/label left, value right) — never a
    // vertical stacked badge, even when the configured style is vertical.
    let badge_style = badge_style.for_shape(badge_appearance.shape);
    let base = load_image_with_limits(poster_bytes)?;

    // Normalize to the output frame (2:3 for every mode except Native) before
    // overlaying badges, so badges anchor to the final frame corners.
    let mut canvas: RgbaImage = fit_poster(base, target_width, poster_fit);

    let max_per_row = match badge_size {
        BadgeSize::Large | BadgeSize::ExtraLarge => {
            if badge_style == BadgeStyle::Horizontal { 2 } else { 4 }
        }
        _ => {
            if badge_style.is_vertical() { MAX_VERT_BADGES_PER_ROW } else { MAX_BADGES_PER_ROW }
        }
    };

    // Rating badges at their configured anchor (with optional split).
    if !badges.is_empty() {
        let badge_images: Vec<RgbaImage> = if badge_style.is_vertical() {
            badges.iter().map(|b| badge::render_vertical_badge(b, font, label_style, badge_appearance, badge_scale)).collect()
        } else {
            badge::render_badges_uniform(badges, font, label_style, badge_appearance, badge_scale)
        };

        if poster_badge_split && badge_images.len() >= 2 {
            // Split the badges across two opposite sides. A vertical badge layout
            // splits left/right; horizontal rows split top/bottom. The first
            // (higher-priority) half keeps the configured anchor side.
            let split_top_bottom = !badge_direction.is_vertical();
            let (primary, opposite) = poster_position.split_anchors(split_top_bottom);
            let mid = badge_images.len().div_ceil(2);
            let (first, second) = badge_images.split_at(mid);
            overlay_poster_group(&mut canvas, first, primary, badge_direction, max_per_row, badge_scale);
            overlay_poster_group(&mut canvas, second, opposite, badge_direction, max_per_row, badge_scale);
        } else {
            overlay_poster_group(&mut canvas, &badge_images, poster_position, badge_direction, max_per_row, badge_scale);
        }
    }

    // Quality and language overlay groups, each at its own independent anchor.
    // Quality has its own layout direction; auto (Default) resolves from the
    // quality badge's own anchor — a column at corner/side positions, a row at
    // top/bottom-center (the same rule the rating badges use). Language (a single
    // badge) just follows the rating badges' direction.
    let quality_dir = overlay.quality_direction.resolve(overlay.quality_position);
    for (group, position, dir) in [
        (&overlay.quality, overlay.quality_position, quality_dir),
        (&overlay.language, overlay.language_position, badge_direction),
    ] {
        if !group.is_empty() {
            let imgs = render_overlay_images(group, font, badge_style.is_vertical(), badge_appearance, badge_scale);
            overlay_poster_group(&mut canvas, &imgs, position, dir, max_per_row, badge_scale);
        }
    }

    // Encode as JPEG
    let dynamic = DynamicImage::ImageRgba8(canvas);
    let rgb = dynamic.to_rgb8();
    let mut buf = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    rgb.write_with_encoder(encoder)?;

    Ok(buf)
}

/// Overlay one group of poster badges at `position`, using the configured
/// stacking direction. Vertical direction stacks a column; horizontal direction
/// lays out rows of up to `max_per_row` badges.
fn overlay_poster_group(
    canvas: &mut RgbaImage,
    badge_images: &[RgbaImage],
    position: BadgePosition,
    badge_direction: BadgeDirection,
    max_per_row: usize,
    badge_scale: f32,
) {
    if badge_direction.is_vertical() {
        overlay_vertical_stack(canvas, badge_images, position, badge_scale, BADGE_SIDE_MARGIN, 0, 0);
    } else {
        overlay_horizontal_rows(canvas, badge_images, position, max_per_row, badge_scale, BADGE_SIDE_MARGIN, 0, 0);
    }
}

const LOGO_BADGE_ROW_SPACING: u32 = 7;
const LOGO_BADGE_SPACING: u32 = 10;
const LOGO_MAX_BADGES_PER_ROW: usize = 3;
const LOGO_SPACING_BELOW: u32 = 15;

pub fn render_logo_sync(
    logo_bytes: &[u8],
    badges: &[RatingBadge],
    overlay: &OverlaySpec,
    font: &FontArc,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_appearance: BadgeAppearance,
    target_width: u32,
    badge_scale: f32,
) -> Result<Vec<u8>, AppError> {
    // A pill is a horizontal lozenge (icon/label left, value right) — never a
    // vertical stacked badge, even when the configured style is vertical.
    let badge_style = badge_style.for_shape(badge_appearance.shape);
    // Logos have no anchor positions — quality/language badges stack below the
    // logo alongside the ratings (the position settings are ignored here).
    let overlay_badges = overlay.combined();
    let base = load_image_with_limits(logo_bytes)?;

    let base = if base.width() != target_width {
        let scale = target_width as f64 / base.width() as f64;
        let target_height = (base.height() as f64 * scale).round() as u32;
        base.resize_exact(target_width, target_height, image::imageops::FilterType::Lanczos3)
    } else {
        base
    };

    let logo_img = base.to_rgba8();

    if badges.is_empty() && overlay_badges.is_empty() {
        // No badges — just encode the logo as PNG
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            logo_img.as_raw(),
            logo_img.width(),
            logo_img.height(),
            image::ExtendedColorType::Rgba8,
        )?;
        return Ok(buf);
    }

    let logo_badge_spacing = (LOGO_BADGE_SPACING as f32 * badge_scale).round() as u32;
    let logo_badge_row_spacing = (LOGO_BADGE_ROW_SPACING as f32 * badge_scale).round() as u32;
    let logo_spacing_below = (LOGO_SPACING_BELOW as f32 * badge_scale).round() as u32;

    if badge_style.is_vertical() {
        // Vertical badge shapes arranged in rows below the logo
        let mut badge_images: Vec<RgbaImage> = badges
                .iter()
                .map(|b| badge::render_vertical_badge(b, font, label_style, badge_appearance, badge_scale))
                .collect();
            badge_images.extend(render_overlay_images(&overlay_badges, font, true, badge_appearance, badge_scale));

            let rows: Vec<&[RgbaImage]> = badge_images.chunks(MAX_VERT_BADGES_PER_ROW).collect();
            let max_badge_height: u32 = badge_images.iter().map(|b| b.height()).max().unwrap_or(0);
            let total_badge_height = max_badge_height * rows.len() as u32
                + logo_badge_row_spacing * (rows.len() as u32).saturating_sub(1);

            let max_row_width: u32 = rows
                .iter()
                .map(|row| {
                    row.iter().map(|b| b.width()).sum::<u32>()
                        + logo_badge_spacing * (row.len() as u32).saturating_sub(1)
                })
                .max()
                .unwrap_or(0);

            let canvas_width = logo_img.width().max(max_row_width);
            let canvas_height = logo_img.height() + logo_spacing_below + total_badge_height;

            let mut canvas = RgbaImage::new(canvas_width, canvas_height);

            let logo_x = (canvas_width.saturating_sub(logo_img.width())) / 2;
            imageops::overlay(&mut canvas, &logo_img, logo_x as i64, 0);

            let badges_start_y = logo_img.height() + logo_spacing_below;
            for (row_idx, row) in rows.iter().enumerate() {
                let row_width: u32 = row.iter().map(|b| b.width()).sum::<u32>()
                    + logo_badge_spacing * (row.len() as u32).saturating_sub(1);
                let start_x = (canvas_width.saturating_sub(row_width)) / 2;
                let y = badges_start_y + row_idx as u32 * (max_badge_height + logo_badge_row_spacing);

                let mut x = start_x;
                for badge_img in *row {
                    let by = y + (max_badge_height.saturating_sub(badge_img.height())) / 2;
                    imageops::overlay(&mut canvas, badge_img, x as i64, by as i64);
                    x += badge_img.width() + logo_badge_spacing;
                }
            }

            // Encode as PNG
            let mut buf = Vec::new();
            let encoder = image::codecs::png::PngEncoder::new(&mut buf);
            image::ImageEncoder::write_image(
                encoder,
                canvas.as_raw(),
                canvas.width(),
                canvas.height(),
                image::ExtendedColorType::Rgba8,
            )?;
            Ok(buf)
    } else {
        // Horizontal badge images (default) — uniform widths, arranged in rows below the logo
            let mut badge_images = badge::render_badges_uniform(badges, font, label_style, badge_appearance, badge_scale);
            badge_images.extend(render_overlay_images(&overlay_badges, font, false, badge_appearance, badge_scale));

            let rows: Vec<&[RgbaImage]> = badge_images.chunks(LOGO_MAX_BADGES_PER_ROW).collect();
            let badge_height = badge_images[0].height();
            let total_badge_height =
                badge_height * rows.len() as u32 + logo_badge_row_spacing * (rows.len() as u32).saturating_sub(1);

            // Compute row widths to determine canvas width
            let max_row_width: u32 = rows
                .iter()
                .map(|row| {
                    row.iter().map(|b| b.width()).sum::<u32>()
                        + logo_badge_spacing * (row.len() as u32).saturating_sub(1)
                })
                .max()
                .unwrap_or(0);

            let canvas_width = logo_img.width().max(max_row_width);
            let canvas_height = logo_img.height() + logo_spacing_below + total_badge_height;

            let mut canvas = RgbaImage::new(canvas_width, canvas_height);

            // Center logo at top
            let logo_x = (canvas_width.saturating_sub(logo_img.width())) / 2;
            imageops::overlay(&mut canvas, &logo_img, logo_x as i64, 0);

            // Center badge rows below logo
            let badges_start_y = logo_img.height() + logo_spacing_below;
            for (row_idx, row) in rows.iter().enumerate() {
                let row_width: u32 = row.iter().map(|b| b.width()).sum::<u32>()
                    + logo_badge_spacing * (row.len() as u32).saturating_sub(1);
                let start_x = (canvas_width.saturating_sub(row_width)) / 2;
                let y = badges_start_y + row_idx as u32 * (badge_height + logo_badge_row_spacing);

                let mut x = start_x;
                for badge_img in *row {
                    imageops::overlay(&mut canvas, badge_img, x as i64, y as i64);
                    x += badge_img.width() + logo_badge_spacing;
                }
            }

            // Encode as PNG (preserves transparency)
            let mut buf = Vec::new();
            let encoder = image::codecs::png::PngEncoder::new(&mut buf);
            image::ImageEncoder::write_image(
                encoder,
                canvas.as_raw(),
                canvas.width(),
                canvas.height(),
                image::ExtendedColorType::Rgba8,
            )?;
            Ok(buf)
    }
}

pub async fn generate_logo(
    logo_bytes: Vec<u8>,
    badges: Vec<RatingBadge>,
    overlay: OverlaySpec,
    font: FontArc,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_appearance: BadgeAppearance,
    render_semaphore: Arc<Semaphore>,
    target_width: u32,
    badge_scale: f32,
) -> Result<Vec<u8>, AppError> {
    if render_semaphore.available_permits() == 0 {
        tracing::debug!("render queue full, waiting for permit");
    }
    let _permit = render_semaphore.acquire().await
        .map_err(|_| AppError::Other("render queue closed".into()))?;
    tracing::debug!("logo render started");
    let start = std::time::Instant::now();

    let buf = tokio::task::spawn_blocking(move || render_logo_sync(&logo_bytes, &badges, &overlay, &font, badge_style, label_style, badge_appearance, target_width, badge_scale))
        .await
        .map_err(|e| AppError::Other(e.to_string()))??;

    tracing::debug!(elapsed_ms = start.elapsed().as_millis() as u64, "logo render complete");
    Ok(buf)
}

pub fn render_backdrop_sync(
    backdrop_bytes: &[u8],
    badges: &[RatingBadge],
    overlay: &OverlaySpec,
    font: &FontArc,
    quality: u8,
    position: BadgePosition,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_appearance: BadgeAppearance,
    badge_direction: BadgeDirection,
    target_width: u32,
    badge_scale: f32,
    _badge_size: BadgeSize,
    edge_inset_x: i32,
    edge_inset_y: i32,
) -> Result<Vec<u8>, AppError> {
    // A pill is a horizontal lozenge (icon/label left, value right) — never a
    // vertical stacked badge, even when the configured style is vertical.
    let badge_style = badge_style.for_shape(badge_appearance.shape);
    let base = load_image_with_limits(backdrop_bytes)?;

    let base = if base.width() != target_width {
        let scale = target_width as f64 / base.width() as f64;
        let target_height = (base.height() as f64 * scale).round() as u32;
        base.resize_exact(target_width, target_height, image::imageops::FilterType::Lanczos3)
    } else {
        base
    };

    let mut canvas: RgbaImage = base.to_rgba8();

    // Edge insets are a percentage of the canvas dimension, so they stay
    // proportional across image sizes (small/medium/large). They apply to the
    // rating badges only; the overlay groups use the default margin.
    // Clamp to the same MAX_EDGE_INSET the settings/cache-key paths use so the
    // rendered inset can never diverge from the value baked into the cache key.
    let inset_pct_x = crate::services::db::clamp_edge_inset(edge_inset_x) as f32 / 100.0;
    let inset_pct_y = crate::services::db::clamp_edge_inset(edge_inset_y) as f32 / 100.0;
    let extra_x = (canvas.width() as f32 * inset_pct_x).round() as u32;
    let extra_y = (canvas.height() as f32 * inset_pct_y).round() as u32;

    // Rating badges at the configured backdrop anchor.
    if !badges.is_empty() {
        let badge_images: Vec<RgbaImage> = if badge_style.is_vertical() {
            badges.iter().map(|b| badge::render_vertical_badge(b, font, label_style, badge_appearance, badge_scale)).collect()
        } else {
            badge::render_badges_uniform(badges, font, label_style, badge_appearance, badge_scale)
        };
        if badge_direction.is_vertical() {
            overlay_vertical_stack(&mut canvas, &badge_images, position, badge_scale, BACKDROP_SIDE_MARGIN, extra_x, extra_y);
        } else {
            // Backdrops are wide (16:9) so all badges fit in a single row
            overlay_horizontal_rows(&mut canvas, &badge_images, position, badge_images.len(), badge_scale, BACKDROP_SIDE_MARGIN, extra_x, extra_y);
        }
    }

    // Quality and language overlay groups, each at its own independent anchor.
    // Quality has its own layout direction; auto (Default) resolves from its own
    // anchor (column at corners/sides, row at top/bottom-center).
    let quality_dir = overlay.quality_direction.resolve(overlay.quality_position);
    for (group, gpos, gdir) in [
        (&overlay.quality, overlay.quality_position, quality_dir),
        (&overlay.language, overlay.language_position, badge_direction),
    ] {
        if !group.is_empty() {
            let imgs = render_overlay_images(group, font, badge_style.is_vertical(), badge_appearance, badge_scale);
            if gdir.is_vertical() {
                overlay_vertical_stack(&mut canvas, &imgs, gpos, badge_scale, BACKDROP_SIDE_MARGIN, 0, 0);
            } else {
                overlay_horizontal_rows(&mut canvas, &imgs, gpos, imgs.len(), badge_scale, BACKDROP_SIDE_MARGIN, 0, 0);
            }
        }
    }

    // Encode as JPEG
    let dynamic = DynamicImage::ImageRgba8(canvas);
    let rgb = dynamic.to_rgb8();
    let mut buf = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    rgb.write_with_encoder(encoder)?;
    Ok(buf)
}

pub async fn generate_backdrop(
    backdrop_bytes: Vec<u8>,
    badges: Vec<RatingBadge>,
    overlay: OverlaySpec,
    font: FontArc,
    quality: u8,
    position: BadgePosition,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_appearance: BadgeAppearance,
    badge_direction: BadgeDirection,
    render_semaphore: Arc<Semaphore>,
    target_width: u32,
    badge_scale: f32,
    badge_size: BadgeSize,
    edge_inset_x: i32,
    edge_inset_y: i32,
) -> Result<Vec<u8>, AppError> {
    if render_semaphore.available_permits() == 0 {
        tracing::debug!("render queue full, waiting for permit");
    }
    let _permit = render_semaphore.acquire().await
        .map_err(|_| AppError::Other("render queue closed".into()))?;
    tracing::debug!("backdrop render started");
    let start = std::time::Instant::now();

    let buf = tokio::task::spawn_blocking(move || render_backdrop_sync(&backdrop_bytes, &badges, &overlay, &font, quality, position, badge_style, label_style, badge_appearance, badge_direction, target_width, badge_scale, badge_size, edge_inset_x, edge_inset_y))
        .await
        .map_err(|e| AppError::Other(e.to_string()))??;

    tracing::debug!(elapsed_ms = start.elapsed().as_millis() as u64, "backdrop render complete");
    Ok(buf)
}

/// Episode still rendering with configurable position, direction, and optional blur.
/// Reuses the poster badge overlay functions but with episode-specific settings.
// NOTE: episodes intentionally do NOT render the quality/language overlay
// badges (only posters, backdrops, and logos do), so this takes no OverlaySpec.
pub fn render_episode_sync(
    image_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    quality: u8,
    position: BadgePosition,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_appearance: BadgeAppearance,
    badge_direction: BadgeDirection,
    target_width: u32,
    badge_scale: f32,
    badge_size: BadgeSize,
    blur: bool,
) -> Result<Vec<u8>, AppError> {
    // A pill is a horizontal lozenge (icon/label left, value right) — never a
    // vertical stacked badge, even when the configured style is vertical.
    let badge_style = badge_style.for_shape(badge_appearance.shape);
    let base = image::load_from_memory(image_bytes)
        .map_err(AppError::Image)?;

    let base = if base.width() != target_width {
        let scale = target_width as f64 / base.width() as f64;
        let target_height = (base.height() as f64 * scale).round() as u32;
        base.resize_exact(target_width, target_height, image::imageops::FilterType::Lanczos3)
    } else {
        base
    };

    let mut canvas: RgbaImage = base.to_rgba8();

    // Apply blur for spoiler protection (before badge overlay).
    // Downscale → small blur → upscale is much faster than a large-radius Gaussian
    // and visually equivalent for spoiler hiding.
    if blur && canvas.width() >= 8 && canvas.height() >= 8 {
        let (w, h) = (canvas.width(), canvas.height());
        let small = image::imageops::resize(&canvas, w / 8, h / 8, image::imageops::FilterType::Triangle);
        let small = imageproc::filter::gaussian_blur_f32(&small, 3.0);
        canvas = image::imageops::resize(&small, w, h, image::imageops::FilterType::Triangle);
    }

    let max_per_row = match badge_size {
        BadgeSize::Large | BadgeSize::ExtraLarge => {
            if badge_style == BadgeStyle::Horizontal { 2 } else { 4 }
        }
        _ => {
            if badge_style.is_vertical() { MAX_VERT_BADGES_PER_ROW } else { MAX_BADGES_PER_ROW }
        }
    };

    // Rating badges at the configured episode anchor.
    if !badges.is_empty() {
        let badge_images: Vec<RgbaImage> = if badge_style.is_vertical() {
            badges.iter().map(|b| badge::render_vertical_badge(b, font, label_style, badge_appearance, badge_scale)).collect()
        } else {
            badge::render_badges_uniform(badges, font, label_style, badge_appearance, badge_scale)
        };
        if badge_direction.is_vertical() {
            overlay_vertical_stack(&mut canvas, &badge_images, position, badge_scale, BADGE_SIDE_MARGIN, 0, 0);
        } else {
            overlay_horizontal_rows(&mut canvas, &badge_images, position, max_per_row, badge_scale, BADGE_SIDE_MARGIN, 0, 0);
        }
    }

    // Episodes deliberately omit the quality/language overlay badges.

    // Encode as JPEG
    let dynamic = DynamicImage::ImageRgba8(canvas);
    let rgb = dynamic.to_rgb8();
    let mut buf = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    rgb.write_with_encoder(encoder)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_poster_no_badges() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        // Create a minimal valid PNG in memory
        let img = image::RgbaImage::from_pixel(100, 150, image::Rgba([128, 128, 128, 255]));
        let mut png_bytes = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            100,
            150,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();

        let result = render_poster_sync(&png_bytes, &[], &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        assert!(!result.is_empty());
        // Should be valid JPEG
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_with_badges() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let img = image::RgbaImage::from_pixel(500, 750, image::Rgba([128, 128, 128, 255]));
        let mut png_bytes = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            500,
            750,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();

        let badges = vec![
            RatingBadge {
                source: RatingSource::Imdb,
                value: "8.5".to_string(),
            },
            RatingBadge {
                source: RatingSource::Tmdb,
                value: "85%".to_string(),
            },
            RatingBadge {
                source: RatingSource::Rt,
                value: "92%".to_string(),
            },
            RatingBadge {
                source: RatingSource::Metacritic,
                value: "78".to_string(),
            },
        ];

        let result = render_poster_sync(&png_bytes, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_invalid_image_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(b"not an image", &[], &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native);
        assert!(result.is_err());
    }

    /// Helper: create a minimal PNG in memory.
    fn test_png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([128, 128, 128, 255]));
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        buf
    }

    #[test]
    fn render_logo_no_badges() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(200, 80);
        let result = render_logo_sync(&png, &[], &OverlaySpec::default(), &font, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), 500, 1.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_logo_with_badges() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(400, 100);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];
        let result = render_logo_sync(&png, &badges, &OverlaySpec::default(), &font, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), 500, 1.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn pill_renders_horizontally_regardless_of_style() {
        use crate::services::db::{BadgeBackground, BadgeShape};
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(400, 100);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];
        let pill = BadgeAppearance { shape: BadgeShape::Pill, background: BadgeBackground::Default };
        let rounded = BadgeAppearance::default();

        // A pill ignores the vertical style and always renders horizontally, so
        // vertical+pill and horizontal+pill produce identical output.
        let v_pill = render_logo_sync(&png, &badges, &OverlaySpec::default(), &font, BadgeStyle::Vertical, LabelStyle::Official, pill, 500, 1.0).unwrap();
        let h_pill = render_logo_sync(&png, &badges, &OverlaySpec::default(), &font, BadgeStyle::Horizontal, LabelStyle::Official, pill, 500, 1.0).unwrap();
        assert_eq!(v_pill, h_pill, "pill should render horizontally even when the style is vertical");

        // Rounded badges still honour the style, so the two layouts differ.
        let v_round = render_logo_sync(&png, &badges, &OverlaySpec::default(), &font, BadgeStyle::Vertical, LabelStyle::Official, rounded, 500, 1.0).unwrap();
        let h_round = render_logo_sync(&png, &badges, &OverlaySpec::default(), &font, BadgeStyle::Horizontal, LabelStyle::Official, rounded, 500, 1.0).unwrap();
        assert_ne!(v_round, h_round, "rounded badges should still differ between vertical and horizontal styles");
    }

    #[test]
    fn render_logo_downscales_wide_image() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        // Create a logo wider than TARGET_WIDTH (500)
        let png = test_png(1000, 200);
        let result = render_logo_sync(&png, &[], &OverlaySpec::default(), &font, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), 500, 1.0).unwrap();
        assert!(!result.is_empty());
        // Verify the output is valid PNG and was produced (implicitly downscaled)
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_logo_invalid_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_logo_sync(b"not an image", &[], &OverlaySpec::default(), &font, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), 500, 1.0);
        assert!(result.is_err());
    }

    #[test]
    fn render_backdrop_no_badges() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let result = render_backdrop_sync(&png, &[], &OverlaySpec::default(), &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium, 0, 0).unwrap();
        assert!(!result.is_empty());
        // Backdrop outputs JPEG
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_with_badges() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "9.0".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "95%".to_string() },
        ];
        let result = render_backdrop_sync(&png, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium, 0, 0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_downscales_wide_image() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        // Create a backdrop wider than TARGET_WIDTH (1280)
        let png = test_png(2560, 1440);
        let result = render_backdrop_sync(&png, &[], &OverlaySpec::default(), &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium, 0, 0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_invalid_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_backdrop_sync(b"not an image", &[], &OverlaySpec::default(), &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium, 0, 0);
        assert!(result.is_err());
    }

    #[test]
    fn render_backdrop_horizontal_bottom_center() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "9.0".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "95%".to_string() },
        ];
        let result = render_backdrop_sync(&png, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 1280, 1.0, BadgeSize::Medium, 0, 0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_horizontal_large_badges() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "9.0".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "95%".to_string() },
            RatingBadge { source: RatingSource::RtAudience, value: "80%".to_string() },
        ];
        // Large + Horizontal style — all badges in a single row on wide backdrop
        let result = render_backdrop_sync(&png, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 1280, 1.0, BadgeSize::Large, 0, 0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_vertical_bottom_left() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
        ];
        let result = render_backdrop_sync(&png, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::BottomLeft, BadgeStyle::Vertical, LabelStyle::Icon, BadgeAppearance::default(), BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium, 0, 0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_edge_inset_shifts_only_active_axis() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "9.0".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "95%".to_string() },
        ];
        let render = |pos: BadgePosition, x: i32, y: i32| {
            render_backdrop_sync(&png, &badges, &OverlaySpec::default(), &font, 85, pos, BadgeStyle::Vertical, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium, x, y).unwrap()
        };

        // TopRight anchors to both a horizontal and vertical edge, so both insets
        // move the badges (the JPEG encoder is deterministic, so bytes differ).
        assert_ne!(render(BadgePosition::TopRight, 0, 0), render(BadgePosition::TopRight, 10, 0));
        assert_ne!(render(BadgePosition::TopRight, 0, 0), render(BadgePosition::TopRight, 0, 10));

        // TopCenter is horizontally centered → the horizontal inset is a no-op,
        // but the vertical inset still applies.
        assert_eq!(render(BadgePosition::TopCenter, 0, 0), render(BadgePosition::TopCenter, 10, 0));
        assert_ne!(render(BadgePosition::TopCenter, 0, 0), render(BadgePosition::TopCenter, 0, 10));

        // Right is vertically centered → the vertical inset is a no-op, but the
        // horizontal inset still applies.
        assert_eq!(render(BadgePosition::Right, 0, 0), render(BadgePosition::Right, 0, 10));
        assert_ne!(render(BadgePosition::Right, 0, 0), render(BadgePosition::Right, 10, 0));
    }

    // --- Episode rendering tests ---

    #[test]
    fn render_episode_no_badges() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(780, 439);
        let result = render_episode_sync(&png, &[], &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Official, BadgeAppearance::default(), BadgeDirection::Vertical, 780, 1.0, BadgeSize::Large, false).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_episode_with_badges() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(780, 439);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];
        let result = render_episode_sync(&png, &badges, &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Official, BadgeAppearance::default(), BadgeDirection::Vertical, 780, 1.0, BadgeSize::Large, false).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_episode_with_blur() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(780, 439);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
        ];
        let result = render_episode_sync(&png, &badges, &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Official, BadgeAppearance::default(), BadgeDirection::Vertical, 780, 1.0, BadgeSize::Large, true).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_episode_downscales_wide_image() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1920, 1080);
        let result = render_episode_sync(&png, &[], &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Official, BadgeAppearance::default(), BadgeDirection::Vertical, 780, 1.0, BadgeSize::Large, false).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_episode_invalid_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_episode_sync(b"not an image", &[], &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Official, BadgeAppearance::default(), BadgeDirection::Vertical, 780, 1.0, BadgeSize::Large, false);
        assert!(result.is_err());
    }

    #[test]
    fn render_poster_top_center_produces_valid_jpeg() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png_bytes = test_png(500, 750);
        let badges = vec![
            RatingBadge {
                source: RatingSource::Imdb,
                value: "8.5".to_string(),
            },
        ];
        let result = render_poster_sync(&png_bytes, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::TopCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }



    #[test]
    fn render_poster_left_position_produces_valid_jpeg() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png_bytes = test_png(500, 750);
        let badges = vec![
            RatingBadge {
                source: RatingSource::Imdb,
                value: "8.5".to_string(),
            },
        ];
        let result = render_poster_sync(&png_bytes, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::Left, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_right_position_produces_valid_jpeg() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png_bytes = test_png(500, 750);
        let badges = vec![
            RatingBadge {
                source: RatingSource::Imdb,
                value: "8.5".to_string(),
            },
        ];
        let result = render_poster_sync(&png_bytes, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::Right, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_with_icon_label_style() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png_bytes = test_png(500, 750);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
        ];
        let result = render_poster_sync(&png_bytes, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Icon, BadgeAppearance::default(), BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_vertical_badge_direction() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png_bytes = test_png(500, 750);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
        ];
        // vertical direction at bottom-center
        let result = render_poster_sync(&png_bytes, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);

        // vertical direction at top-left corner
        let result = render_poster_sync(&png_bytes, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::TopLeft, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);

        // vertical direction at bottom-right corner
        let result = render_poster_sync(&png_bytes, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::BottomRight, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_logo_with_icon_label_style() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(400, 100);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];
        let result = render_logo_sync(&png, &badges, &OverlaySpec::default(), &font, BadgeStyle::Horizontal, LabelStyle::Icon, BadgeAppearance::default(), 500, 1.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_backdrop_with_icon_label_style() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "9.0".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "95%".to_string() },
        ];
        let result = render_backdrop_sync(&png, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Icon, BadgeAppearance::default(), BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium, 0, 0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_with_official_label_style() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png_bytes = test_png(500, 750);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
            RatingBadge { source: RatingSource::RtAudience, value: "45%".to_string() },
        ];
        let result = render_poster_sync(&png_bytes, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Official, BadgeAppearance::default(), BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_logo_with_official_label_style() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(400, 100);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];
        let result = render_logo_sync(&png, &badges, &OverlaySpec::default(), &font, BadgeStyle::Horizontal, LabelStyle::Official, BadgeAppearance::default(), 500, 1.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_backdrop_with_official_label_style() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "9.0".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "95%".to_string() },
            RatingBadge { source: RatingSource::RtAudience, value: "40%".to_string() },
        ];
        let result = render_backdrop_sync(&png, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Official, BadgeAppearance::default(), BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium, 0, 0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    fn dummy_tmdb() -> crate::services::tmdb::TmdbClient {
        crate::services::tmdb::TmdbClient::new("test".to_string(), reqwest::Client::new())
    }

    #[tokio::test]
    async fn generate_poster_acquires_semaphore() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(100, 150);
        let sem = Arc::new(Semaphore::new(2));
        let tmdb = dummy_tmdb();

        let result = generate_poster(ImageParams {
            poster_bytes_override: Some(png),
            poster_path: "",
            badges: &[],
            overlay: &OverlaySpec::default(),
            tmdb: &tmdb,
            font: &font,
            quality: 85,
            cache_dir: "/tmp",
            image_stale_secs: 3600,
            poster_position: BadgePosition::BottomCenter,
            badge_style: BadgeStyle::Horizontal,
            label_style: LabelStyle::Text,
            badge_appearance: BadgeAppearance::default(),
            badge_direction: BadgeDirection::Horizontal,
            poster_badge_split: false,
            poster_fit: PosterFit::Native,
            render_semaphore: sem.clone(),
            target_width: 500,
            badge_scale: 1.0,
            tmdb_size: Arc::from("w500"),
            badge_size: BadgeSize::Medium,
            external_cache_only: false,
        })
        .await;

        assert!(result.is_ok());
        // Permit should be released after render
        assert_eq!(sem.available_permits(), 2);
    }

    #[tokio::test]
    async fn generate_poster_closed_semaphore_returns_error() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(100, 150);
        let sem = Arc::new(Semaphore::new(1));
        sem.close();
        let tmdb = dummy_tmdb();

        let result = generate_poster(ImageParams {
            poster_bytes_override: Some(png),
            poster_path: "",
            badges: &[],
            overlay: &OverlaySpec::default(),
            tmdb: &tmdb,
            font: &font,
            quality: 85,
            cache_dir: "/tmp",
            image_stale_secs: 3600,
            poster_position: BadgePosition::BottomCenter,
            badge_style: BadgeStyle::Horizontal,
            label_style: LabelStyle::Text,
            badge_appearance: BadgeAppearance::default(),
            badge_direction: BadgeDirection::Horizontal,
            poster_badge_split: false,
            poster_fit: PosterFit::Native,
            render_semaphore: sem,
            target_width: 500,
            badge_scale: 1.0,
            tmdb_size: Arc::from("w500"),
            badge_size: BadgeSize::Medium,
            external_cache_only: false,
        })
        .await;

        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("render queue closed"), "expected 'render queue closed', got: {err_msg}");
    }

    #[tokio::test]
    async fn generate_logo_acquires_and_releases_semaphore() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(200, 80);
        let sem = Arc::new(Semaphore::new(1));

        let result = generate_logo(png, vec![], OverlaySpec::default(), font, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), sem.clone(), 500, 1.0).await;
        assert!(result.is_ok());
        assert_eq!(sem.available_permits(), 1);
    }

    #[tokio::test]
    async fn generate_logo_closed_semaphore_returns_error() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(200, 80);
        let sem = Arc::new(Semaphore::new(1));
        sem.close();

        let result = generate_logo(png, vec![], OverlaySpec::default(), font, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), sem, 500, 1.0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn generate_backdrop_acquires_and_releases_semaphore() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let sem = Arc::new(Semaphore::new(1));

        let result = generate_backdrop(png, vec![], OverlaySpec::default(), font, 85, BadgePosition::TopRight, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Vertical, sem.clone(), 1280, 1.0, BadgeSize::Medium, 0, 0).await;
        assert!(result.is_ok());
        assert_eq!(sem.available_permits(), 1);
    }

    #[tokio::test]
    async fn generate_backdrop_closed_semaphore_returns_error() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let sem = Arc::new(Semaphore::new(1));
        sem.close();

        let result = generate_backdrop(png, vec![], OverlaySpec::default(), font, 85, BadgePosition::TopRight, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Vertical, sem, 1280, 1.0, BadgeSize::Medium, 0, 0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn semaphore_bounds_concurrent_logo_renders() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let sem = Arc::new(Semaphore::new(1));

        // Acquire the only permit
        let permit = sem.clone().acquire_owned().await.unwrap();

        let png = test_png(200, 80);
        let sem2 = sem.clone();
        let font2 = font.clone();

        // Spawn a render that should block waiting for the permit
        let handle = tokio::spawn(async move {
            generate_logo(png, vec![], OverlaySpec::default(), font2, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), sem2, 500, 1.0).await
        });

        // Give the task a moment to start and block
        tokio::task::yield_now().await;
        assert_eq!(sem.available_permits(), 0, "permit should still be held");

        // Release the permit so the render can proceed
        drop(permit);
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert_eq!(sem.available_permits(), 1);
    }

    #[test]
    fn render_poster_at_large_target_width() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(500, 750);
        // Render at large size (1280 width, ~2.2x badge scale)
        let result = render_poster_sync(&png, &[], &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 1280, 2.2, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
        // Verify the output is wider than a default-size render
        let img = image::load_from_memory(&result).unwrap();
        assert_eq!(img.width(), 1280);
    }

    #[test]
    fn render_poster_with_badges_at_large_scale() {
        use crate::services::ratings::{RatingBadge, RatingSource};
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(500, 750);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
        ];
        let result = render_poster_sync(&png, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 1280, 2.2, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        let img = image::load_from_memory(&result).unwrap();
        assert_eq!(img.width(), 1280);
    }

    #[test]
    fn render_poster_with_large_badge_size() {
        use crate::services::ratings::{RatingBadge, RatingSource};
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(500, 750);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];
        // Large badge size with horizontal style — should use max 2 per row
        let result = render_poster_sync(&png, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Large, false, PosterFit::Native).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);

        // Extra-large badge size with vertical style — should use max 4 per row
        let result = render_poster_sync(&png, &badges, &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Vertical, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 500, 1.0, BadgeSize::ExtraLarge, false, PosterFit::Native).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    /// Compute the centroid (center of mass) of non-background pixels in a
    /// rendered poster JPEG. The base image is uniform gray (128,128,128), so
    /// any pixel that deviates significantly is part of a badge overlay.
    /// Returns (centroid_x, centroid_y) as fractions of image dimensions (0.0–1.0).
    fn badge_centroid(jpeg_bytes: &[u8]) -> (f64, f64) {
        let img = image::load_from_memory(jpeg_bytes).unwrap().to_rgba8();
        let (w, h) = (img.width() as f64, img.height() as f64);
        let (mut sx, mut sy, mut count) = (0u64, 0u64, 0u64);
        for (x, y, px) in img.enumerate_pixels() {
            // JPEG compression blurs edges, so use a generous threshold
            let diff = (px[0] as i32 - 128).abs()
                .max((px[1] as i32 - 128).abs())
                .max((px[2] as i32 - 128).abs());
            if diff > 20 {
                sx += x as u64;
                sy += y as u64;
                count += 1;
            }
        }
        assert!(count > 0, "no badge pixels found");
        (sx as f64 / count as f64 / w, sy as f64 / count as f64 / h)
    }

    fn position_test_badges() -> Vec<crate::services::ratings::RatingBadge> {
        use crate::services::ratings::{RatingBadge, RatingSource};
        vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
        ]
    }

    #[test]
    fn badge_position_top_center_places_badges_in_top_region() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(&test_png(500, 750), &position_test_badges(), &OverlaySpec::default(), &font, 85,
            BadgePosition::TopCenter, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeAppearance::default(),
            BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        let (cx, cy) = badge_centroid(&result);
        assert!(cy < 0.33, "TopCenter: badge y-centroid {cy:.2} should be in top third");
        assert!(cx > 0.3 && cx < 0.7, "TopCenter: badge x-centroid {cx:.2} should be centered");
    }

    #[test]
    fn badge_position_bottom_center_places_badges_in_bottom_region() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(&test_png(500, 750), &position_test_badges(), &OverlaySpec::default(), &font, 85,
            BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeAppearance::default(),
            BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        let (cx, cy) = badge_centroid(&result);
        assert!(cy > 0.67, "BottomCenter: badge y-centroid {cy:.2} should be in bottom third");
        assert!(cx > 0.3 && cx < 0.7, "BottomCenter: badge x-centroid {cx:.2} should be centered");
    }

    #[test]
    fn badge_position_left_places_badges_on_left_side() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(&test_png(500, 750), &position_test_badges(), &OverlaySpec::default(), &font, 85,
            BadgePosition::Left, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeAppearance::default(),
            BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        let (cx, cy) = badge_centroid(&result);
        assert!(cx < 0.5, "Left: badge x-centroid {cx:.2} should be in left half");
        assert!(cy > 0.25 && cy < 0.75, "Left: badge y-centroid {cy:.2} should be vertically centered");
    }

    #[test]
    fn badge_position_right_places_badges_on_right_side() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(&test_png(500, 750), &position_test_badges(), &OverlaySpec::default(), &font, 85,
            BadgePosition::Right, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeAppearance::default(),
            BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        let (cx, cy) = badge_centroid(&result);
        assert!(cx > 0.5, "Right: badge x-centroid {cx:.2} should be in right half");
        assert!(cy > 0.25 && cy < 0.75, "Right: badge y-centroid {cy:.2} should be vertically centered");
    }

    #[test]
    fn badge_position_top_left_places_badges_in_top_left_quadrant() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(&test_png(500, 750), &position_test_badges(), &OverlaySpec::default(), &font, 85,
            BadgePosition::TopLeft, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeAppearance::default(),
            BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        let (cx, cy) = badge_centroid(&result);
        assert!(cx < 0.5, "TopLeft: badge x-centroid {cx:.2} should be in left half");
        assert!(cy < 0.5, "TopLeft: badge y-centroid {cy:.2} should be in top half");
    }

    #[test]
    fn badge_position_bottom_right_places_badges_in_bottom_right_quadrant() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(&test_png(500, 750), &position_test_badges(), &OverlaySpec::default(), &font, 85,
            BadgePosition::BottomRight, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeAppearance::default(),
            BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        let (cx, cy) = badge_centroid(&result);
        assert!(cx > 0.5, "BottomRight: badge x-centroid {cx:.2} should be in right half");
        assert!(cy > 0.5, "BottomRight: badge y-centroid {cy:.2} should be in bottom half");
    }

    /// Count non-background badge pixels in each half of a rendered poster.
    /// Returns (top, bottom, left, right) counts. Base art is uniform gray.
    fn badge_pixel_halves(jpeg_bytes: &[u8]) -> (u64, u64, u64, u64) {
        let img = image::load_from_memory(jpeg_bytes).unwrap().to_rgba8();
        let (w, h) = (img.width(), img.height());
        let (mut top, mut bottom, mut left, mut right) = (0u64, 0u64, 0u64, 0u64);
        for (x, y, px) in img.enumerate_pixels() {
            let diff = (px[0] as i32 - 128).abs()
                .max((px[1] as i32 - 128).abs())
                .max((px[2] as i32 - 128).abs());
            if diff > 20 {
                if y < h / 2 { top += 1; } else { bottom += 1; }
                if x < w / 2 { left += 1; } else { right += 1; }
            }
        }
        (top, bottom, left, right)
    }

    fn split_test_badges() -> Vec<crate::services::ratings::RatingBadge> {
        use crate::services::ratings::{RatingBadge, RatingSource};
        vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
            RatingBadge { source: RatingSource::Metacritic, value: "78".to_string() },
        ]
    }

    #[test]
    fn split_horizontal_places_badges_top_and_bottom() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        // Horizontal rows at bottom-center, split on → half top, half bottom.
        let result = render_poster_sync(&test_png(500, 750), &split_test_badges(), &OverlaySpec::default(), &font, 85,
            BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeAppearance::default(),
            BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, true, PosterFit::Native).unwrap();
        let (top, bottom, _l, _r) = badge_pixel_halves(&result);
        assert!(top > 0, "split: expected badge pixels in the top half, got {top}");
        assert!(bottom > 0, "split: expected badge pixels in the bottom half, got {bottom}");

        // Without split, the same config keeps every badge in the bottom half.
        let unsplit = render_poster_sync(&test_png(500, 750), &split_test_badges(), &OverlaySpec::default(), &font, 85,
            BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeAppearance::default(),
            BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, PosterFit::Native).unwrap();
        let (top_u, _b, _l, _r) = badge_pixel_halves(&unsplit);
        assert_eq!(top_u, 0, "no-split: expected no badge pixels in the top half, got {top_u}");
    }

    #[test]
    fn split_vertical_places_badges_left_and_right() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        // Vertical column, split on → half left, half right.
        let result = render_poster_sync(&test_png(500, 750), &split_test_badges(), &OverlaySpec::default(), &font, 85,
            BadgePosition::Left, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeAppearance::default(),
            BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium, true, PosterFit::Native).unwrap();
        let (_t, _b, left, right) = badge_pixel_halves(&result);
        assert!(left > 0, "split: expected badge pixels in the left half, got {left}");
        assert!(right > 0, "split: expected badge pixels in the right half, got {right}");
    }

    #[test]
    fn split_single_badge_does_not_split() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let badges = vec![crate::services::ratings::RatingBadge {
            source: crate::services::ratings::RatingSource::Imdb,
            value: "8.5".to_string(),
        }];
        // Split requested but only one badge → behaves like no split (bottom only).
        let result = render_poster_sync(&test_png(500, 750), &badges, &OverlaySpec::default(), &font, 85,
            BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeAppearance::default(),
            BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, true, PosterFit::Native).unwrap();
        let (top, bottom, _l, _r) = badge_pixel_halves(&result);
        assert_eq!(top, 0, "single badge: nothing should move to the top half, got {top}");
        assert!(bottom > 0, "single badge: badge should remain in the bottom half, got {bottom}");
    }

    /// Decode a rendered poster and return its (width, height).
    fn rendered_dims(bytes: &[u8]) -> (u32, u32) {
        let img = image::load_from_memory(bytes).unwrap();
        (img.width(), img.height())
    }

    fn render_with_fit(src: &[u8], target_width: u32, fit: PosterFit) -> Vec<u8> {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        render_poster_sync(
            src, &[], &OverlaySpec::default(), &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal,
            LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal,
            target_width, 1.0, BadgeSize::Medium, false, fit,
        )
        .unwrap()
    }

    #[test]
    fn poster_fit_cover_normalizes_non_2_3_to_2_3() {
        // A square (1:1) source is the kind of art that downstream apps crop.
        let out = render_with_fit(&test_png(600, 600), 580, PosterFit::Cover);
        assert_eq!(rendered_dims(&out), (580, 870), "cover must produce an exact 2:3 frame");
    }

    #[test]
    fn poster_fit_pad_normalizes_non_2_3_to_2_3() {
        let out = render_with_fit(&test_png(900, 600), 580, PosterFit::Pad);
        assert_eq!(rendered_dims(&out), (580, 870), "pad must produce an exact 2:3 frame");
    }

    #[test]
    fn poster_fit_blur_normalizes_non_2_3_to_2_3() {
        let out = render_with_fit(&test_png(600, 600), 580, PosterFit::Blur);
        assert_eq!(rendered_dims(&out), (580, 870), "blur must produce an exact 2:3 frame");
    }

    #[test]
    fn poster_fit_native_preserves_source_ratio() {
        // Native keeps the legacy behavior: scale to width, keep the source ratio.
        let square = render_with_fit(&test_png(600, 600), 580, PosterFit::Native);
        assert_eq!(rendered_dims(&square), (580, 580), "native keeps a 1:1 source 1:1");
        let wide = render_with_fit(&test_png(900, 600), 580, PosterFit::Native);
        // round(600 * 580 / 900) = 387
        assert_eq!(rendered_dims(&wide), (580, 387), "native keeps a 3:2 source ratio");
    }

    #[test]
    fn poster_fit_already_2_3_unchanged_dims_across_modes() {
        for fit in [PosterFit::Native, PosterFit::Cover, PosterFit::Pad, PosterFit::Blur] {
            let out = render_with_fit(&test_png(500, 750), 580, fit);
            assert_eq!(rendered_dims(&out), (580, 870), "2:3 source stays 2:3 for {fit:?}");
        }
    }

    #[test]
    fn render_logo_at_large_target_width() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(200, 80);
        let result = render_logo_sync(&png, &[], &OverlaySpec::default(), &font, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), 1722, 2.2).unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_backdrop_at_large_target_width() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let result = render_backdrop_sync(&png, &[], &OverlaySpec::default(), &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Vertical, 3840, 2.0, BadgeSize::Medium, 0, 0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }
}
