use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use image::{imageops, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;

use crate::image::icons;
use crate::services::db::{BadgeAppearance, BadgeBackground, BadgeShape, LabelStyle};
use crate::services::ratings::{RatingBadge, RatingSource};

const DARK_BG: Rgba<u8> = Rgba([0, 0, 0, 200]);
/// Alpha used for the `Transparent` background so the artwork shows through.
const TRANSPARENT_ALPHA: u8 = 120;
/// Shadow colour drawn behind text/icons when there is no badge background.
const SHADOW_COLOR: Rgba<u8> = Rgba([0, 0, 0, 160]);

/// Return `c` with its alpha channel replaced by `a`.
fn with_alpha(c: Rgba<u8>, a: u8) -> Rgba<u8> {
    Rgba([c[0], c[1], c[2], a])
}

/// Background fill colours for the (label, value) sections of a badge, or
/// `None` when the badge should have no background (drawn directly on the
/// image). The label section is source-coloured except for `Official` labels,
/// which always sit on a dark chip.
fn section_colors(background: BadgeBackground, label_style: LabelStyle, source: &RatingSource) -> Option<(Rgba<u8>, Rgba<u8>)> {
    let base_label = match label_style {
        LabelStyle::Official => DARK_BG,
        _ => source.color(),
    };
    match background {
        BadgeBackground::Default => Some((base_label, DARK_BG)),
        BadgeBackground::Dark => Some((DARK_BG, DARK_BG)),
        BadgeBackground::Transparent => Some((
            with_alpha(base_label, TRANSPARENT_ALPHA),
            with_alpha(DARK_BG, TRANSPARENT_ALPHA),
        )),
        BadgeBackground::None => None,
    }
}

/// Drop-shadow offset (px) for text/icons drawn without a background, scaled to
/// the badge dimension. Returns `None` when the badge has a background (no
/// shadow needed).
fn shadow_offset(background: BadgeBackground, badge_dim: u32) -> Option<i32> {
    if background == BadgeBackground::None {
        Some(((badge_dim as f32 / 29.0).round() as i32).max(1))
    } else {
        None
    }
}

/// Corner radius for a badge of the given short-axis length and shape.
/// `Pill` rounds fully (radius = half the short axis); `Rounded` uses `base`.
fn corner_radius(shape: BadgeShape, short_axis: u32, base: u32) -> u32 {
    match shape {
        BadgeShape::Pill => short_axis / 2,
        BadgeShape::Rounded => base,
    }
}

/// Build a same-size silhouette of `icon` filled with the shadow colour
/// (RGB→black, alpha scaled down), for drawing a soft drop shadow.
fn icon_shadow(icon: &RgbaImage) -> RgbaImage {
    let mut shadow = icon.clone();
    for px in shadow.pixels_mut() {
        let a = (px.0[3] as u16 * 3 / 5) as u8;
        *px = Rgba([0, 0, 0, a]);
    }
    shadow
}

/// Draw `text`, optionally preceded by a drop shadow offset by `shadow` px.
#[allow(clippy::too_many_arguments)]
fn draw_text_shadowed(
    img: &mut RgbaImage,
    color: Rgba<u8>,
    x: i32,
    y: i32,
    scale: PxScale,
    font: &FontArc,
    text: &str,
    shadow: Option<i32>,
) {
    if let Some(off) = shadow {
        draw_text_mut(img, SHADOW_COLOR, x + off, y + off, scale, font, text);
    }
    draw_text_mut(img, color, x, y, scale, font, text);
}

/// Overlay `icon` at (`x`, `y`), optionally preceded by a drop shadow offset by
/// `shadow` px (used when the badge has no background).
fn overlay_icon_shadowed(img: &mut RgbaImage, icon: &RgbaImage, x: i64, y: i64, shadow: Option<i32>) {
    if let Some(off) = shadow {
        let shadow_icon = icon_shadow(icon);
        imageops::overlay(img, &shadow_icon, x + off as i64, y + off as i64);
    }
    imageops::overlay(img, icon, x, y);
}
const BASE_BADGE_HEIGHT: u32 = 58;
const BASE_BADGE_PADDING_H: u32 = 14;
const BASE_TEXT_LABEL_PADDING_H: u32 = 8;
const BASE_BADGE_VALUE_PADDING_H: u32 = 10;
const BASE_BADGE_RADIUS: u32 = 10;
/// Extra padding added at a pill badge's rounded ends so the text/icons don't
/// crowd the fully-rounded caps. Applied to the long-axis ends only (left/right
/// for horizontal badges, top/bottom for vertical).
const BASE_PILL_PADDING: u32 = 10;
/// Extra height added to horizontal pill badges for a little more top/bottom
/// breathing room around the content.
const BASE_PILL_PADDING_V: u32 = 6;
const BASE_FONT_SIZE: f32 = 34.0;
const BASE_LABEL_FONT_SIZE: f32 = 26.0;
const BASE_ICON_HEIGHT: u32 = 48;
/// Minimum clearance (at 1x badge scale) kept between the text's full line box
/// (ascent + descent) and the top/bottom edge of a horizontal badge. Only
/// matters for enlarged font sizes: if the text would come closer than this,
/// the badge grows taller to fit it. At the default font size the badge height
/// is always the base height.
const BASE_TEXT_MIN_PAD_V: f32 = 3.0;

/// Height a horizontal badge needs so text with a line height of `font_px`
/// keeps at least `BASE_TEXT_MIN_PAD_V` clearance above and below.
fn text_fit_height(font_px: f32, badge_scale: f32) -> u32 {
    (font_px + 2.0 * BASE_TEXT_MIN_PAD_V * badge_scale).ceil() as u32
}

/// Compute the width of an icon when scaled to the given target height, preserving aspect ratio.
fn icon_scaled_width(icon: &RgbaImage, target_height: u32) -> u32 {
    if icon.height() == 0 {
        target_height
    } else {
        (icon.width() as f32 * target_height as f32 / icon.height() as f32).ceil() as u32
    }
}

/// Select the appropriate icon for a badge and compute its target (width, height) for the given
/// `label_style` and `icon_height`. For `Official`, the icon is fit within a square box;
/// otherwise it is scaled to the target height preserving aspect ratio.
fn badge_icon_and_size<'a>(badge: &'a RatingBadge, label_style: LabelStyle, icon_height: u32) -> (&'a RgbaImage, (u32, u32)) {
    let icon = match label_style {
        LabelStyle::Official => icons::official_icon_for_badge(badge),
        _ => icons::icon_for_source(&badge.source),
    };
    let dims = match label_style {
        LabelStyle::Official => icon_fit_in_box(icon, icon_height),
        _ => (icon_scaled_width(icon, icon_height), icon_height),
    };
    (icon, dims)
}

/// Compute dimensions to fit an icon within a `box_size x box_size` square, preserving aspect ratio.
fn icon_fit_in_box(icon: &RgbaImage, box_size: u32) -> (u32, u32) {
    let (w, h) = (icon.width(), icon.height());
    if w == 0 || h == 0 {
        return (box_size, box_size);
    }
    let scale = (box_size as f32 / w as f32).min(box_size as f32 / h as f32);
    (
        (w as f32 * scale).ceil() as u32,
        (h as f32 * scale).ceil() as u32,
    )
}

#[cfg(test)]
pub fn render_badge(badge: &RatingBadge, font: &FontArc, label_style: LabelStyle) -> RgbaImage {
    render_badge_with_widths(badge, font, None, None, label_style, BadgeAppearance::default(), 1.0)
}

#[cfg(test)]
pub fn render_badge_appearance(badge: &RatingBadge, font: &FontArc, label_style: LabelStyle, appearance: BadgeAppearance) -> RgbaImage {
    render_badge_with_widths(badge, font, None, None, label_style, appearance, 1.0)
}

/// Scaled badge dimensions for a given badge_scale factor.
struct ScaledDims {
    badge_scale: f32,
    badge_height: u32,
    badge_padding_h: u32,
    text_label_padding_h: u32,
    badge_value_padding_h: u32,
    badge_radius: u32,
    pill_padding: u32,
    pill_padding_v: u32,
    icon_height: u32,
}

impl ScaledDims {
    fn new(badge_scale: f32) -> Self {
        Self {
            badge_scale,
            badge_height: (BASE_BADGE_HEIGHT as f32 * badge_scale).round() as u32,
            badge_padding_h: (BASE_BADGE_PADDING_H as f32 * badge_scale).round() as u32,
            text_label_padding_h: (BASE_TEXT_LABEL_PADDING_H as f32 * badge_scale).round() as u32,
            badge_value_padding_h: (BASE_BADGE_VALUE_PADDING_H as f32 * badge_scale).round() as u32,
            badge_radius: (BASE_BADGE_RADIUS as f32 * badge_scale).round() as u32,
            pill_padding: (BASE_PILL_PADDING as f32 * badge_scale).round() as u32,
            pill_padding_v: (BASE_PILL_PADDING_V as f32 * badge_scale).round() as u32,
            icon_height: (BASE_ICON_HEIGHT as f32 * badge_scale).round() as u32,
        }
    }
}

/// Pre-compute scaled fonts for badge rendering (avoids redundant work).
/// `text_scale` is the combined badge scale × font-size factor; the icon and
/// paddings use the badge scale alone (see [`ScaledDims`]).
struct BadgeFonts<'a> {
    font: &'a FontArc,
    scale: PxScale,
    label_scale: PxScale,
    scaled: ab_glyph::PxScaleFont<&'a FontArc>,
    label_scaled: ab_glyph::PxScaleFont<&'a FontArc>,
}

impl<'a> BadgeFonts<'a> {
    fn new(font: &'a FontArc, text_scale: f32) -> Self {
        let font_size = BASE_FONT_SIZE * text_scale;
        let label_font_size = BASE_LABEL_FONT_SIZE * text_scale;
        let scale = PxScale::from(font_size);
        let label_scale = PxScale::from(label_font_size);
        Self {
            font,
            scale,
            label_scale,
            scaled: font.as_scaled(scale),
            label_scaled: font.as_scaled(label_scale),
        }
    }
}

/// Render all badges with uniform label and value section widths.
pub fn render_badges_uniform(badges: &[RatingBadge], font: &FontArc, label_style: LabelStyle, appearance: BadgeAppearance, badge_scale: f32) -> Vec<RgbaImage> {
    if badges.is_empty() {
        return vec![];
    }

    let fonts = BadgeFonts::new(font, badge_scale * appearance.font_size.scale_factor());
    let dims = ScaledDims::new(badge_scale);

    let max_label_width = match label_style {
        LabelStyle::Official => {
            // All official icons render within a fixed square box
            dims.icon_height
        }
        LabelStyle::Icon => {
            // For icon mode, use the max icon width (scaled to icon height)
            badges.iter()
                .map(|b| icon_scaled_width(icons::icon_for_source(&b.source), dims.icon_height))
                .max()
                .unwrap_or(dims.icon_height)
        }
        LabelStyle::Text => {
            badges.iter()
                .map(|b| text_width(b.source.label(), &fonts.label_scaled))
                .max()
                .unwrap_or(0)
        }
    };
    let max_value_width = badges.iter()
        .map(|b| text_width(&b.value, &fonts.scaled))
        .max()
        .unwrap_or(0);

    badges.iter()
        .map(|b| render_badge_inner(b, &fonts, &dims, Some(max_label_width), Some(max_value_width), label_style, appearance))
        .collect()
}

#[cfg(test)]
fn render_badge_with_widths(
    badge: &RatingBadge,
    font: &FontArc,
    uniform_label_width: Option<u32>,
    uniform_value_width: Option<u32>,
    label_style: LabelStyle,
    appearance: BadgeAppearance,
    badge_scale: f32,
) -> RgbaImage {
    let fonts = BadgeFonts::new(font, badge_scale * appearance.font_size.scale_factor());
    let dims = ScaledDims::new(badge_scale);
    render_badge_inner(badge, &fonts, &dims, uniform_label_width, uniform_value_width, label_style, appearance)
}

fn render_badge_inner(
    badge: &RatingBadge,
    fonts: &BadgeFonts<'_>,
    dims: &ScaledDims,
    uniform_label_width: Option<u32>,
    uniform_value_width: Option<u32>,
    label_style: LabelStyle,
    appearance: BadgeAppearance,
) -> RgbaImage {
    let use_icon = label_style.uses_icon();

    let label = badge.source.label();
    let value = &badge.value;

    let label_width = match label_style {
        LabelStyle::Official => {
            // Official icons all fit within a fixed square box
            uniform_label_width.unwrap_or(dims.icon_height)
        }
        LabelStyle::Icon => {
            let actual_w = icon_scaled_width(icons::icon_for_source(&badge.source), dims.icon_height);
            uniform_label_width.unwrap_or(actual_w)
        }
        LabelStyle::Text => {
            uniform_label_width.unwrap_or_else(|| text_width(label, &fonts.label_scaled))
        }
    };
    let value_width = uniform_value_width.unwrap_or_else(|| text_width(value, &fonts.scaled));
    let label_pad = if use_icon { dims.badge_padding_h } else { dims.text_label_padding_h };
    // Pills get extra padding at their rounded left/right ends so the content
    // doesn't crowd the fully-rounded caps, plus a little extra height for
    // top/bottom breathing room. Rounded badges add nothing.
    let (pill_pad, pill_pad_v) = match appearance.shape {
        BadgeShape::Pill => (dims.pill_padding, dims.pill_padding_v),
        BadgeShape::Rounded => (0, 0),
    };
    // The badge keeps its base height unless an enlarged font needs more room
    // (the value text is always the taller of the two text runs).
    let text_h = text_fit_height(fonts.scale.y.max(fonts.label_scale.y), dims.badge_scale);
    let badge_h = dims.badge_height.max(text_h) + pill_pad_v;

    let value_x = pill_pad + label_width + label_pad * 2;
    let total_width = value_x + value_width + dims.badge_value_padding_h + dims.badge_value_padding_h / 2 + 2 + pill_pad;
    let mut img = RgbaImage::new(total_width, badge_h);

    // Draw the label + value backgrounds (unless the badge has no background).
    // Both sections are filled as plain rects, then the four outer corners are
    // rounded — leaving the inner join square for a clean seam at any radius.
    if let Some((label_bg, value_bg)) = section_colors(appearance.background, label_style, &badge.source) {
        draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(value_x, badge_h), label_bg);
        draw_filled_rect_mut(&mut img, Rect::at(value_x as i32, 0).of_size(total_width - value_x, badge_h), value_bg);
        round_corners(&mut img, corner_radius(appearance.shape, badge_h, dims.badge_radius));
    }

    let shadow = shadow_offset(appearance.background, badge_h);

    // Draw label (icon or text, centered within uniform label area)
    if use_icon {
        let (icon, (icon_w, icon_h)) = badge_icon_and_size(badge, label_style, dims.icon_height);
        let scaled_icon = if icon.width() == icon_w && icon.height() == icon_h {
            icon.clone()
        } else {
            imageops::resize(icon, icon_w, icon_h, imageops::FilterType::Lanczos3)
        };
        let ix = pill_pad + label_pad + (label_width.saturating_sub(icon_w)) / 2;
        let iy = (badge_h.saturating_sub(icon_h)) / 2;
        overlay_icon_shadowed(&mut img, &scaled_icon, ix as i64, iy as i64, shadow);
    } else {
        let actual_label_width = text_width(label, &fonts.label_scaled);
        let label_x = pill_pad + label_pad + (label_width.saturating_sub(actual_label_width)) / 2;
        let label_y = (badge_h as i32 - fonts.label_scale.x as i32) / 2;
        draw_text_shadowed(
            &mut img,
            Rgba([255, 255, 255, 255]),
            label_x as i32,
            label_y,
            fonts.label_scale,
            fonts.font,
            label,
            shadow,
        );
    }

    // Draw value text (centered within uniform value area)
    let actual_value_width = text_width(value, &fonts.scaled);
    let value_text_x = value_x + dims.badge_value_padding_h + (value_width.saturating_sub(actual_value_width)) / 2;
    let value_y = (badge_h as i32 - fonts.scale.x as i32) / 2;
    draw_text_shadowed(
        &mut img,
        Rgba([255, 255, 255, 255]),
        value_text_x as i32,
        value_y,
        fonts.scale,
        fonts.font,
        value,
        shadow,
    );

    img
}

const BASE_VERT_BADGE_WIDTH: u32 = 88;
const BASE_VERT_BADGE_PADDING_V: u32 = 8;
const BASE_VERT_LABEL_FONT_SIZE: f32 = 26.0;
const BASE_VERT_VALUE_FONT_SIZE: f32 = 34.0;

/// Render a vertical badge: source label on top, rating value below.
/// Used for left/right poster positions.
///
/// The text scales with `badge_scale × font_size`; the icon and paddings with
/// `badge_scale` alone. The badge's height follows its content, and its width
/// grows in proportion to an enlarged font (never shrinks for a smaller one) so
/// wider text still fits. Width depends only on the scales — not the text — so
/// every badge in a row/column stays the same width.
pub fn render_vertical_badge(badge: &RatingBadge, font: &FontArc, label_style: LabelStyle, appearance: BadgeAppearance, badge_scale: f32) -> RgbaImage {
    let use_icon = label_style.uses_icon();
    let font_factor = appearance.font_size.scale_factor();
    let text_scale = badge_scale * font_factor;
    let vert_label_font_size = BASE_VERT_LABEL_FONT_SIZE * text_scale;
    let vert_value_font_size = BASE_VERT_VALUE_FONT_SIZE * text_scale;
    let label_scale = PxScale::from(vert_label_font_size);
    let value_scale = PxScale::from(vert_value_font_size);
    let vert_badge_width = (BASE_VERT_BADGE_WIDTH as f32 * badge_scale * font_factor.max(1.0)).round() as u32;
    // Pills get extra top/bottom padding so the stacked label/value clear the
    // fully-rounded caps; rounded badges keep the base padding.
    let pill_pad = match appearance.shape {
        BadgeShape::Pill => (BASE_PILL_PADDING as f32 * badge_scale).round() as u32,
        BadgeShape::Rounded => 0,
    };
    let vert_badge_padding_v = (BASE_VERT_BADGE_PADDING_V as f32 * badge_scale).round() as u32 + pill_pad;
    let icon_height = (BASE_ICON_HEIGHT as f32 * badge_scale).round() as u32;
    let badge_radius = (BASE_BADGE_RADIUS as f32 * badge_scale).round() as u32;

    let label = badge.source.label();
    let value = &badge.value;

    let label_area_h = if use_icon { icon_height } else { vert_label_font_size as u32 };
    let gap = (4.0 * badge_scale).round() as u32;
    let total_height = vert_badge_padding_v
        + label_area_h
        + gap
        + vert_value_font_size as u32
        + vert_badge_padding_v;

    let mut img = RgbaImage::new(vert_badge_width, total_height);

    let value_area_y = vert_badge_padding_v + label_area_h + (gap / 2);

    // Draw the label (top) + value (bottom) backgrounds, unless the badge has no
    // background. Fill plain rects then round the four outer corners — a pill
    // shape uses radius = half the width (a vertical stadium).
    if let Some((label_bg, value_bg)) = section_colors(appearance.background, label_style, &badge.source) {
        draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(vert_badge_width, total_height), label_bg);
        let value_area_h = total_height - value_area_y;
        draw_filled_rect_mut(&mut img, Rect::at(0, value_area_y as i32).of_size(vert_badge_width, value_area_h), value_bg);
        round_corners(&mut img, corner_radius(appearance.shape, vert_badge_width, badge_radius));
    }

    let shadow = shadow_offset(appearance.background, vert_badge_width);

    // Center label (icon or text) within the colored label area
    if use_icon {
        let (icon, (icon_w, icon_h)) = badge_icon_and_size(badge, label_style, icon_height);
        let scaled_icon = if icon.width() == icon_w && icon.height() == icon_h {
            icon.clone()
        } else {
            imageops::resize(icon, icon_w, icon_h, imageops::FilterType::Lanczos3)
        };
        let ix = (vert_badge_width.saturating_sub(icon_w)) / 2;
        let iy = (value_area_y.saturating_sub(icon_h)) / 2;
        overlay_icon_shadowed(&mut img, &scaled_icon, ix as i64, iy as i64, shadow);
    } else {
        let label_scaled_font = font.as_scaled(label_scale);
        let lw = text_width(label, &label_scaled_font);
        let label_x = (vert_badge_width.saturating_sub(lw)) / 2;
        let label_y = (value_area_y.saturating_sub(vert_label_font_size as u32)) / 2;
        draw_text_shadowed(
            &mut img,
            Rgba([255, 255, 255, 255]),
            label_x as i32,
            label_y as i32,
            label_scale,
            font,
            label,
            shadow,
        );
    }

    // Center value text
    let value_scaled_font = font.as_scaled(value_scale);
    let vw = text_width(value, &value_scaled_font);
    let value_x = (vert_badge_width.saturating_sub(vw)) / 2;
    let value_y = (value_area_y + vert_badge_padding_v / 2) as i32;
    draw_text_shadowed(
        &mut img,
        Rgba([255, 255, 255, 255]),
        value_x as i32,
        value_y,
        value_scale,
        font,
        value,
        shadow,
    );

    img
}

fn text_width(text: &str, font: &ab_glyph::PxScaleFont<&FontArc>) -> u32 {
    let width: f32 = text
        .chars()
        .map(|c| font.h_advance(font.glyph_id(c)))
        .sum();
    width.ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ratings::{RatingBadge, RatingSource};

    fn test_font() -> FontArc {
        FontArc::try_from_slice(crate::FONT_BYTES).unwrap()
    }

    static TEST_FONT: std::sync::LazyLock<FontArc> = std::sync::LazyLock::new(test_font);

    #[test]
    fn render_badge_correct_height() {
        let badge = RatingBadge {
            source: RatingSource::Imdb,
            value: "8.5".to_string(),
        };
        let img = render_badge(&badge, &test_font(), LabelStyle::Text);
        assert_eq!(img.height(), BASE_BADGE_HEIGHT);
        assert!(img.width() > 0);
    }

    #[test]
    fn render_badge_all_sources_produce_valid_images() {
        let font = test_font();
        let sources = [
            RatingSource::Imdb,
            RatingSource::Tmdb,
            RatingSource::Rt,
            RatingSource::RtAudience,
            RatingSource::Metacritic,
            RatingSource::Trakt,
            RatingSource::Letterboxd,
            RatingSource::Mal,
            RatingSource::Mdblist,
            RatingSource::Ebert,
        ];

        for source in sources {
            let badge = RatingBadge {
                source,
                value: "75%".to_string(),
            };
            let img = render_badge(&badge, &font, LabelStyle::Text);
            assert_eq!(img.height(), BASE_BADGE_HEIGHT, "wrong height for {:?}", source);
            assert!(img.width() > 0, "zero width for {:?}", source);
        }
    }

    #[test]
    fn render_badge_icon_all_sources() {
        let font = test_font();
        let sources = [
            RatingSource::Imdb,
            RatingSource::Tmdb,
            RatingSource::Rt,
            RatingSource::RtAudience,
            RatingSource::Metacritic,
            RatingSource::Trakt,
            RatingSource::Letterboxd,
            RatingSource::Mal,
            RatingSource::Mdblist,
            RatingSource::Ebert,
        ];

        for source in sources {
            let badge = RatingBadge {
                source,
                value: "75%".to_string(),
            };
            let img = render_badge(&badge, &font, LabelStyle::Icon);
            assert_eq!(img.height(), BASE_BADGE_HEIGHT, "wrong height for {:?}", source);
            assert!(img.width() > 0, "zero width for {:?}", source);
        }
    }

    #[test]
    fn render_badge_width_scales_with_value_length() {
        let font = test_font();
        let short = RatingBadge {
            source: RatingSource::Imdb,
            value: "5".to_string(),
        };
        let long = RatingBadge {
            source: RatingSource::Imdb,
            value: "100%".to_string(),
        };

        let short_img = render_badge(&short, &font, LabelStyle::Text);
        let long_img = render_badge(&long, &font, LabelStyle::Text);

        assert!(
            long_img.width() > short_img.width(),
            "longer value should produce wider badge"
        );
    }

    #[test]
    fn render_vertical_badge_correct_dimensions() {
        let badge = RatingBadge {
            source: RatingSource::Imdb,
            value: "8.5".to_string(),
        };
        let img = render_vertical_badge(&badge, &test_font(), LabelStyle::Text, BadgeAppearance::default(), 1.0);
        assert_eq!(img.width(), BASE_VERT_BADGE_WIDTH);
        assert!(img.height() > 0);
    }

    #[test]
    fn render_vertical_badge_all_sources() {
        let font = test_font();
        let sources = [
            RatingSource::Imdb,
            RatingSource::Tmdb,
            RatingSource::Rt,
            RatingSource::RtAudience,
            RatingSource::Metacritic,
            RatingSource::Trakt,
            RatingSource::Letterboxd,
            RatingSource::Mal,
            RatingSource::Mdblist,
            RatingSource::Ebert,
        ];

        for source in sources {
            let badge = RatingBadge {
                source,
                value: "75%".to_string(),
            };
            let img = render_vertical_badge(&badge, &font, LabelStyle::Text, BadgeAppearance::default(), 1.0);
            assert_eq!(img.width(), BASE_VERT_BADGE_WIDTH, "wrong width for {:?}", source);
            assert!(img.height() > 0, "zero height for {:?}", source);
        }
    }

    #[test]
    fn render_vertical_badge_icon_all_sources() {
        let font = test_font();
        let sources = [
            RatingSource::Imdb,
            RatingSource::Tmdb,
            RatingSource::Rt,
            RatingSource::RtAudience,
            RatingSource::Metacritic,
            RatingSource::Trakt,
            RatingSource::Letterboxd,
            RatingSource::Mal,
            RatingSource::Mdblist,
            RatingSource::Ebert,
        ];

        for source in sources {
            let badge = RatingBadge {
                source,
                value: "75%".to_string(),
            };
            let img = render_vertical_badge(&badge, &font, LabelStyle::Icon, BadgeAppearance::default(), 1.0);
            assert_eq!(img.width(), BASE_VERT_BADGE_WIDTH, "wrong width for {:?}", source);
            assert!(img.height() > 0, "zero height for {:?}", source);
        }
    }

    #[test]
    fn render_badge_empty_value() {
        let font = test_font();
        let badge = RatingBadge {
            source: RatingSource::Tmdb,
            value: String::new(),
        };
        // Should not panic
        let img = render_badge(&badge, &font, LabelStyle::Text);
        assert_eq!(img.height(), BASE_BADGE_HEIGHT);
    }

    #[test]
    fn render_badge_scaled_2x_doubles_height() {
        let font = test_font();
        let badge = RatingBadge {
            source: RatingSource::Imdb,
            value: "8.5".to_string(),
        };
        let img = render_badge_with_widths(&badge, &font, None, None, LabelStyle::Text, BadgeAppearance::default(), 2.0);
        assert_eq!(img.height(), BASE_BADGE_HEIGHT * 2);
    }

    #[test]
    fn render_vertical_badge_scaled_2x_doubles_width() {
        let font = test_font();
        let badge = RatingBadge {
            source: RatingSource::Imdb,
            value: "8.5".to_string(),
        };
        let img = render_vertical_badge(&badge, &font, LabelStyle::Text, BadgeAppearance::default(), 2.0);
        assert_eq!(img.width(), BASE_VERT_BADGE_WIDTH * 2);
    }

    #[test]
    fn render_badges_uniform_scaled() {
        let font = test_font();
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];
        let images = render_badges_uniform(&badges, &font, LabelStyle::Text, BadgeAppearance::default(), 2.0);
        assert_eq!(images.len(), 2);
        // All badges should have doubled height
        for img in &images {
            assert_eq!(img.height(), BASE_BADGE_HEIGHT * 2);
        }
        // Uniform width: all badges same width
        assert_eq!(images[0].width(), images[1].width());
    }

    const ALL_SHAPES: [BadgeShape; 2] = [BadgeShape::Rounded, BadgeShape::Pill];
    const ALL_BACKGROUNDS: [BadgeBackground; 4] = [
        BadgeBackground::Default,
        BadgeBackground::Dark,
        BadgeBackground::Transparent,
        BadgeBackground::None,
    ];

    #[test]
    fn all_shape_background_combos_render_valid_badges() {
        let font = test_font();
        let badge = RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() };
        for shape in ALL_SHAPES {
            for background in ALL_BACKGROUNDS {
                let appearance = BadgeAppearance { shape, background, ..Default::default() };
                // Horizontal badge height: base, plus the pill's extra vertical padding.
                let expected_h = match shape {
                    BadgeShape::Pill => BASE_BADGE_HEIGHT + BASE_PILL_PADDING_V,
                    BadgeShape::Rounded => BASE_BADGE_HEIGHT,
                };
                let h = render_badge_appearance(&badge, &font, LabelStyle::Text, appearance);
                assert_eq!(h.height(), expected_h, "h height for {shape:?}/{background:?}");
                assert!(h.width() > 0, "h width for {shape:?}/{background:?}");
                // Icon labels exercise the icon/shadow path.
                let hi = render_badge_appearance(&badge, &font, LabelStyle::Official, appearance);
                assert_eq!(hi.height(), expected_h, "h-icon height for {shape:?}/{background:?}");
                // Vertical: appearance must not change the badge width.
                let v = render_vertical_badge(&badge, &font, LabelStyle::Text, appearance, 1.0);
                assert_eq!(v.width(), BASE_VERT_BADGE_WIDTH, "v width for {shape:?}/{background:?}");
                assert!(v.height() > 0, "v height for {shape:?}/{background:?}");
            }
        }
    }

    #[test]
    fn none_background_is_transparent_but_draws_content() {
        let font = test_font();
        let badge = RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() };
        let none = render_badge_appearance(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Rounded, background: BadgeBackground::None, ..Default::default() });
        // A background pixel left of the label text is fully transparent.
        assert_eq!(none.get_pixel(2, BASE_BADGE_HEIGHT / 2)[3], 0);
        // But the badge still draws something (the label/value text).
        assert!(none.pixels().any(|p| p[3] > 0), "none badge should still draw text");
    }

    #[test]
    fn background_modes_control_label_opacity() {
        let font = test_font();
        let badge = RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() };
        let probe = (2, BASE_BADGE_HEIGHT / 2); // label-section background, left of text
        let default = render_badge_appearance(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Rounded, background: BadgeBackground::Default, ..Default::default() });
        let transparent = render_badge_appearance(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Rounded, background: BadgeBackground::Transparent, ..Default::default() });
        // Default keeps the opaque source colour; transparent is semi-opaque.
        assert!(default.get_pixel(probe.0, probe.1)[3] >= 200);
        assert_eq!(transparent.get_pixel(probe.0, probe.1)[3], TRANSPARENT_ALPHA);
    }

    #[test]
    fn pill_rounds_more_than_rounded() {
        let font = test_font();
        let badge = RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() };
        let rounded = render_badge_appearance(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Rounded, background: BadgeBackground::Default, ..Default::default() });
        let pill = render_badge_appearance(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Pill, background: BadgeBackground::Default, ..Default::default() });
        assert!(rounded.width() > 30);
        // A point near the top edge, beyond the small rounded radius, is filled
        // for `rounded` but cleared by the pill's much larger corner arc.
        assert!(rounded.get_pixel(15, 2)[3] > 0, "rounded fills near-top edge");
        assert_eq!(pill.get_pixel(15, 2)[3], 0, "pill clears more of the top edge");
        // Both still clear the very corner.
        assert_eq!(rounded.get_pixel(0, 0)[3], 0);
        assert_eq!(pill.get_pixel(0, 0)[3], 0);
    }

    #[test]
    fn pill_pads_its_rounded_ends() {
        let font = test_font();
        let badge = RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() };
        // Horizontal pills add padding at the left/right caps → wider than rounded.
        let rounded = render_badge_appearance(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Rounded, background: BadgeBackground::Default, ..Default::default() });
        let pill = render_badge_appearance(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Pill, background: BadgeBackground::Default, ..Default::default() });
        assert!(pill.width() > rounded.width(), "pill should pad its rounded left/right ends");
        // Vertical pills add padding at the top/bottom caps → taller than rounded.
        let rounded_v = render_vertical_badge(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Rounded, background: BadgeBackground::Default, ..Default::default() }, 1.0);
        let pill_v = render_vertical_badge(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Pill, background: BadgeBackground::Default, ..Default::default() }, 1.0);
        assert!(pill_v.height() > rounded_v.height(), "vertical pill should pad its rounded top/bottom ends");
    }

    use crate::services::db::FontSize;

    const ALL_FONT_SIZES: [FontSize; 5] = [
        FontSize::ExtraSmall,
        FontSize::Small,
        FontSize::Medium,
        FontSize::Large,
        FontSize::ExtraLarge,
    ];

    fn with_font(font_size: FontSize) -> BadgeAppearance {
        BadgeAppearance { font_size, ..Default::default() }
    }

    /// Bounding box (min_x, min_y, max_x, max_y) of near-white pixels (the
    /// rating text) at or right of `from_x`, or `None` if there are none.
    fn white_bbox(img: &RgbaImage, from_x: u32) -> Option<(u32, u32, u32, u32)> {
        let mut bb: Option<(u32, u32, u32, u32)> = None;
        for (x, y, p) in img.enumerate_pixels() {
            if x < from_x || p[3] < 200 || p[0] < 230 || p[1] < 230 || p[2] < 230 {
                continue;
            }
            bb = Some(match bb {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            });
        }
        bb
    }

    /// `WxH:` + the first 8 bytes (hex) of the SHA-256 of the raw RGBA pixels.
    fn pixel_digest(img: &RgbaImage) -> String {
        use sha2::{Digest, Sha256};
        let d = Sha256::digest(img.as_raw());
        let hex: String = d.iter().take(8).map(|b| format!("{b:02x}")).collect();
        format!("{}x{}:{hex}", img.width(), img.height())
    }

    /// Golden pixel digests of default-appearance badges, captured by rendering
    /// the same inputs with the code at `61803c1` (release v1.3.0, before the
    /// font-size setting existed) through the public `render_badges_uniform` /
    /// `render_vertical_badge` API. Columns: shape, label style, badge scale,
    /// horizontal badge 0 (IMDb "8.5"), horizontal badge 1 (MAL "10.00"; both
    /// share a uniform width), vertical badge (MAL "10.00").
    ///
    /// Rasterisation (ab_glyph outlines, Lanczos icon resizing) is plain f32
    /// arithmetic with no platform-specific code paths, so the digests are
    /// expected to be stable across platforms; if a dependency bump changes
    /// glyph rasterisation these values must be regenerated from a pre-feature
    /// revision, not from the current code.
    const PRE_FEATURE_GOLDENS: &[(BadgeShape, LabelStyle, f32, &str, &str, &str)] = &[
        (BadgeShape::Rounded, LabelStyle::Text, 0.5, "85x29:86d284d16b9e3d97", "85x29:2ff21b9014c52d27", "44x40:0e7bbebca7a3f457"),
        (BadgeShape::Rounded, LabelStyle::Text, 1.0, "168x58:8f22aef64f63db2a", "168x58:b3a0a55ce14beb53", "88x80:6477d2f4953e0510"),
        (BadgeShape::Rounded, LabelStyle::Text, 1.25, "208x73:26cf419ac58af07d", "208x73:d603233740d6378f", "110x99:c2b457852334ee8d"),
        (BadgeShape::Rounded, LabelStyle::Text, 2.2, "366x128:cea9a4f190536017", "366x128:ec78dd6ff2ae9494", "194x176:915b84d4adc5bbac"),
        (BadgeShape::Rounded, LabelStyle::Official, 0.5, "87x29:183f0c7086e997aa", "87x29:92c6dcd50a5b60ff", "44x51:a0fe9931394faf6d"),
        (BadgeShape::Rounded, LabelStyle::Official, 1.0, "172x58:2faaabeac9f61551", "172x58:83e8a1b342e7ad59", "88x102:533618a916c1b7ce"),
        (BadgeShape::Rounded, LabelStyle::Official, 1.25, "215x73:cdcbb51921d3c1e7", "215x73:e0aa1a98bb0e9ab0", "110x127:f18d5e45ba993326"),
        (BadgeShape::Rounded, LabelStyle::Official, 2.2, "376x128:f3fc6d08ba4d5af8", "376x128:fafe7cc71e2353b0", "194x225:ffea58feb8fd85d9"),
        (BadgeShape::Pill, LabelStyle::Text, 0.5, "95x32:211d45d5869a23f2", "95x32:b25c07fc08722e67", "44x50:4c0635afdc976ef1"),
        (BadgeShape::Pill, LabelStyle::Text, 1.0, "188x64:b4fe2ab51349ca7e", "188x64:14e135a2247551c9", "88x100:1882d8f4a36361e9"),
        (BadgeShape::Pill, LabelStyle::Text, 1.25, "234x81:35e2f563b98e4b1b", "234x81:145c6e038ea54736", "110x125:4c4853b1e8a169bc"),
        (BadgeShape::Pill, LabelStyle::Text, 2.2, "410x141:6b5b9a27a0863bd6", "410x141:5f9a50e787a587ec", "194x220:fa0a6b46f07cd9fc"),
        (BadgeShape::Pill, LabelStyle::Official, 0.5, "97x32:47ff8d9557407875", "97x32:153ac726fa3d6dba", "44x61:3f379c566a072119"),
        (BadgeShape::Pill, LabelStyle::Official, 1.0, "192x64:dc956e173a548f75", "192x64:dc146ac5c83aa762", "88x122:e35080cdf75e129c"),
        (BadgeShape::Pill, LabelStyle::Official, 1.25, "241x81:0ab6b8e921d9474a", "241x81:3f5879a27008fb40", "110x153:a161d39f7aa746a4"),
        (BadgeShape::Pill, LabelStyle::Official, 2.2, "420x141:07b006c13b780cb4", "420x141:0c80eedd5e91b893", "194x269:661dc227855baf42"),
    ];

    #[test]
    fn default_font_size_is_byte_identical_to_pre_feature_output() {
        let font = test_font();
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Mal, value: "10.00".to_string() },
        ];
        // Both the implicit default and an explicit Medium must reproduce the
        // pre-feature pixels exactly.
        for font_size in [BadgeAppearance::default().font_size, FontSize::Medium] {
            for &(shape, label_style, scale, h0, h1, v) in PRE_FEATURE_GOLDENS {
                let appearance = BadgeAppearance { shape, font_size, ..Default::default() };
                let ctx = format!("{shape:?}/{label_style:?}/{scale}/{font_size:?}");
                let hz = render_badges_uniform(&badges, &font, label_style, appearance, scale);
                assert_eq!(pixel_digest(&hz[0]), h0, "horizontal badge 0: {ctx}");
                assert_eq!(pixel_digest(&hz[1]), h1, "horizontal badge 1: {ctx}");
                let vert = render_vertical_badge(&badges[1], &font, label_style, appearance, scale);
                assert_eq!(pixel_digest(&vert), v, "vertical badge: {ctx}");
            }
        }
        // Sanity: a non-default font size does change the pixels.
        let xl = BadgeAppearance { font_size: FontSize::ExtraLarge, ..Default::default() };
        let hz = render_badges_uniform(&badges, &font, LabelStyle::Text, xl, 1.0);
        assert_ne!(pixel_digest(&hz[0]), PRE_FEATURE_GOLDENS[1].3);
    }

    #[test]
    fn font_size_scales_text_not_badge_height() {
        let font = test_font();
        let badge = RatingBadge { source: RatingSource::Imdb, value: "100%".to_string() };
        for label_style in [LabelStyle::Text, LabelStyle::Icon, LabelStyle::Official] {
            let widths: Vec<u32> = ALL_FONT_SIZES.iter().map(|&fs| {
                let img = render_badge_appearance(&badge, &font, label_style, with_font(fs));
                // The base badge height already fits every font size, so the
                // badge (and therefore the row) keeps its height.
                assert_eq!(img.height(), BASE_BADGE_HEIGHT, "{label_style:?}/{fs:?}");
                img.width()
            }).collect();
            // Bigger text → wider badge, strictly increasing.
            for w in widths.windows(2) {
                assert!(w[0] < w[1], "{label_style:?}: widths should grow with font size: {widths:?}");
            }
        }
    }

    #[test]
    fn font_size_text_stays_vertically_centered_and_unclipped() {
        let font = test_font();
        let badge = RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() };
        for shape in ALL_SHAPES {
            for scale in [0.5, 1.0, 1.5] {
                for fs in ALL_FONT_SIZES {
                    let appearance = BadgeAppearance { shape, font_size: fs, ..Default::default() };
                    let img = render_badge_with_widths(&badge, &font, None, None, LabelStyle::Official, appearance, scale);
                    // Value text sits right of the icon label area.
                    let (_, y0, x1, y1) = white_bbox(&img, img.width() / 2)
                        .unwrap_or_else(|| panic!("no value text for {shape:?}/{scale}/{fs:?}"));
                    let top_gap = y0 as i32;
                    let bottom_gap = img.height() as i32 - 1 - y1 as i32;
                    assert!(top_gap > 0 && bottom_gap > 0, "text clipped vertically: {shape:?}/{scale}/{fs:?}");
                    assert!(x1 < img.width() - 1, "text clipped horizontally: {shape:?}/{scale}/{fs:?}");
                    assert!((top_gap - bottom_gap).abs() <= 2,
                        "text not vertically centered ({top_gap} vs {bottom_gap}): {shape:?}/{scale}/{fs:?}");
                }
            }
        }
    }

    #[test]
    fn font_size_grows_vertical_badges_and_keeps_text_inside() {
        let font = test_font();
        let badge = RatingBadge { source: RatingSource::Mal, value: "10.00".to_string() };
        let mut prev_h = 0;
        for fs in ALL_FONT_SIZES {
            let img = render_vertical_badge(&badge, &font, LabelStyle::Official, with_font(fs), 1.0);
            // Width grows only for enlarged fonts and depends on the scale alone,
            // so every badge in a row/column shares it.
            let expected_w = (BASE_VERT_BADGE_WIDTH as f32 * fs.scale_factor().max(1.0)).round() as u32;
            assert_eq!(img.width(), expected_w, "{fs:?}");
            // Height follows the (taller) text.
            assert!(img.height() > prev_h, "{fs:?}: vertical badge should grow with font size");
            prev_h = img.height();
            if fs.scale_factor() >= 1.0 {
                // Larger fonts never push the text closer to the edges than the default.
                let (x0, _, x1, y1) = white_bbox(&img, 0).expect("value text drawn");
                assert!(x0 > 0 && x1 < img.width() - 1, "{fs:?}: value text clipped horizontally");
                assert!(y1 < img.height() - 1, "{fs:?}: value text clipped vertically");
            }
        }
        // Text labels scale too.
        let text_m = render_vertical_badge(&badge, &font, LabelStyle::Text, with_font(FontSize::Medium), 1.0);
        let text_xl = render_vertical_badge(&badge, &font, LabelStyle::Text, with_font(FontSize::ExtraLarge), 1.0);
        assert!(text_xl.height() > text_m.height());
    }

    #[test]
    fn text_fit_height_never_exceeds_base_height_for_supported_sizes() {
        use crate::cache::ImageType;
        use crate::services::db::{BadgeSize, ImageSize};
        // Every badge scale production can produce (image size × badge size, in
        // the same multiplication order as the handlers), combined with every
        // font size the way `BadgeFonts::new` combines them. The text always
        // fits in the base badge height, so font size never changes row heights
        // or vertical anchoring.
        let kinds = [ImageType::Poster, ImageType::Logo, ImageType::Backdrop, ImageType::Episode];
        let image_sizes = [ImageSize::Small, ImageSize::Medium, ImageSize::Large, ImageSize::VeryLarge];
        let badge_sizes = [BadgeSize::ExtraSmall, BadgeSize::Small, BadgeSize::Medium, BadgeSize::Large, BadgeSize::ExtraLarge];
        let mut checked = 0;
        for kind in kinds {
            for image_size in image_sizes {
                // `Small` is rejected for posters/logos before rendering.
                if image_size == ImageSize::Small && matches!(kind, ImageType::Poster | ImageType::Logo) {
                    continue;
                }
                for badge_size in badge_sizes {
                    let badge_scale = image_size.badge_scale(kind) * badge_size.scale_factor();
                    let base_h = ScaledDims::new(badge_scale).badge_height;
                    for fs in ALL_FONT_SIZES {
                        let fonts = BadgeFonts::new(&TEST_FONT, badge_scale * fs.scale_factor());
                        let font_px = fonts.scale.y.max(fonts.label_scale.y);
                        assert!(
                            text_fit_height(font_px, badge_scale) <= base_h,
                            "{kind:?}/{image_size:?}/{badge_size:?}/{fs:?}: text needs {} > base {base_h}",
                            text_fit_height(font_px, badge_scale),
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, (4 * 4 - 2) * 5 * 5);
        // But an even larger font would grow the badge instead of clipping.
        assert!(text_fit_height(BASE_FONT_SIZE * 2.0, 1.0) > BASE_BADGE_HEIGHT);
    }

    #[test]
    fn scaled_dims_at_1x() {
        let dims = ScaledDims::new(1.0);
        assert_eq!(dims.badge_height, BASE_BADGE_HEIGHT);
        assert_eq!(dims.badge_padding_h, BASE_BADGE_PADDING_H);
        assert_eq!(dims.text_label_padding_h, BASE_TEXT_LABEL_PADDING_H);
        assert_eq!(dims.badge_radius, BASE_BADGE_RADIUS);
        assert_eq!(dims.icon_height, BASE_ICON_HEIGHT);
    }
}

/// Round the four outer corners of the full image to radius `r` by clearing
/// pixels outside each corner arc to transparent. `r` is clamped so the arcs
/// never overlap, so passing `r` = half the short axis yields a pill / stadium
/// shape. The badge image spans exactly one badge, so its corners are the
/// badge's outer corners — any inner seam between label and value stays square.
fn round_corners(img: &mut RgbaImage, r: u32) {
    let (w, h) = (img.width(), img.height());
    let r = r.min(w / 2).min(h / 2);
    if r == 0 {
        return;
    }
    let transparent = Rgba([0, 0, 0, 0]);
    for dy in 0..r {
        for dx in 0..r {
            let dist_sq = (r - dx) * (r - dx) + (r - dy) * (r - dy);
            if dist_sq > r * r {
                img.put_pixel(dx, dy, transparent); // top-left
                img.put_pixel(w - 1 - dx, dy, transparent); // top-right
                img.put_pixel(dx, h - 1 - dy, transparent); // bottom-left
                img.put_pixel(w - 1 - dx, h - 1 - dy, transparent); // bottom-right
            }
        }
    }
}
