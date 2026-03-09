use ab_glyph::FontArc;
use image::codecs::jpeg::JpegEncoder;
use image::{imageops, DynamicImage, RgbaImage};
use crate::cache;
use crate::error::AppError;
use crate::poster::badge;
use crate::services::ratings::RatingBadge;
use crate::services::tmdb::TmdbClient;

const BADGE_SPACING: u32 = 10;
const BADGE_BOTTOM_MARGIN: u32 = 20;
const BADGE_ROW_SPACING: u32 = 7;
const MAX_BADGES_PER_ROW: usize = 3;

pub struct PosterParams<'a> {
    pub poster_path: &'a str,
    pub badges: &'a [RatingBadge],
    pub tmdb: &'a TmdbClient,
    pub http: &'a reqwest::Client,
    pub font: &'a FontArc,
    pub quality: u8,
    pub cache_dir: &'a str,
    pub poster_stale_secs: u64,
}

pub async fn generate_poster(params: PosterParams<'_>) -> Result<Vec<u8>, AppError> {
    let PosterParams {
        poster_path,
        badges,
        tmdb,
        http,
        font,
        quality,
        cache_dir,
        poster_stale_secs,
    } = params;
    // Fetch base poster, using cache
    let poster_cache = cache::poster_cache_path(cache_dir, poster_path)?;
    let poster_bytes = if let Some(entry) = cache::read(&poster_cache, poster_stale_secs).await {
        if entry.is_stale {
            // Stale — refetch in foreground (rare with default 0 = never stale)
            let bytes = tmdb.fetch_poster_bytes(poster_path, http).await?;
            cache::write(&poster_cache, &bytes).await?;
            bytes
        } else {
            entry.bytes
        }
    } else {
        let bytes = tmdb.fetch_poster_bytes(poster_path, http).await?;
        cache::write(&poster_cache, &bytes).await?;
        bytes
    };

    // Move CPU-bound image processing to a blocking thread
    let badges = badges.to_vec();
    let font = font.clone();
    let buf = tokio::task::spawn_blocking(move || {
        render_poster_sync(&poster_bytes, &badges, &font, quality)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    Ok(buf)
}

fn render_poster_sync(
    poster_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    quality: u8,
) -> Result<Vec<u8>, AppError> {
    let base = image::load_from_memory(poster_bytes)
        .map_err(AppError::Image)?;
    let mut canvas: RgbaImage = base.to_rgba8();

    if !badges.is_empty() {
        // Render all badge images
        let badge_images: Vec<RgbaImage> = badges
            .iter()
            .map(|b| badge::render_badge(b, font))
            .collect();

        // Split into rows of MAX_BADGES_PER_ROW
        let rows: Vec<&[RgbaImage]> = badge_images
            .chunks(MAX_BADGES_PER_ROW)
            .collect();

        let badge_height = badge_images[0].height();
        let total_height = badge_height * rows.len() as u32
            + BADGE_ROW_SPACING * (rows.len() as u32).saturating_sub(1);

        let base_y = canvas.height() - BADGE_BOTTOM_MARGIN - total_height;

        for (row_idx, row) in rows.iter().enumerate() {
            let row_width: u32 = row.iter().map(|b| b.width()).sum::<u32>()
                + BADGE_SPACING * (row.len() as u32).saturating_sub(1);

            let start_x = (canvas.width().saturating_sub(row_width)) / 2;
            let y = base_y + row_idx as u32 * (badge_height + BADGE_ROW_SPACING);

            let mut x = start_x;
            for badge_img in *row {
                imageops::overlay(&mut canvas, badge_img, x as i64, y as i64);
                x += badge_img.width() + BADGE_SPACING;
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

/// Generate a 1x1 transparent placeholder JPEG
pub fn placeholder_jpeg() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(1, 1, image::Rgb([0, 0, 0]));
    let mut buf = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut buf, 50);
    img.write_with_encoder(encoder).ok();
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_jpeg_produces_valid_jpeg() {
        let bytes = placeholder_jpeg();
        assert!(!bytes.is_empty());
        // JPEG files start with FF D8
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xD8);
    }

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

        let result = render_poster_sync(&png_bytes, &[], &font, 85).unwrap();
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

        let result = render_poster_sync(&png_bytes, &badges, &font, 85).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_invalid_image_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(b"not an image", &[], &font, 85);
        assert!(result.is_err());
    }
}
