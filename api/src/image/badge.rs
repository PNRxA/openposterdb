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
struct BadgeFonts<'a> {
    font: &'a FontArc,
    scale: PxScale,
    label_scale: PxScale,
    scaled: ab_glyph::PxScaleFont<&'a FontArc>,
    label_scaled: ab_glyph::PxScaleFont<&'a FontArc>,
}

impl<'a> BadgeFonts<'a> {
    fn new(font: &'a FontArc, badge_scale: f32) -> Self {
        let font_size = BASE_FONT_SIZE * badge_scale;
        let label_font_size = BASE_LABEL_FONT_SIZE * badge_scale;
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

    let fonts = BadgeFonts::new(font, badge_scale);
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
    let fonts = BadgeFonts::new(font, badge_scale);
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
    let badge_h = dims.badge_height + pill_pad_v;

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
pub fn render_vertical_badge(badge: &RatingBadge, font: &FontArc, label_style: LabelStyle, appearance: BadgeAppearance, badge_scale: f32) -> RgbaImage {
    let use_icon = label_style.uses_icon();
    let vert_label_font_size = BASE_VERT_LABEL_FONT_SIZE * badge_scale;
    let vert_value_font_size = BASE_VERT_VALUE_FONT_SIZE * badge_scale;
    let label_scale = PxScale::from(vert_label_font_size);
    let value_scale = PxScale::from(vert_value_font_size);
    let vert_badge_width = (BASE_VERT_BADGE_WIDTH as f32 * badge_scale).round() as u32;
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

// --- Overlay badges (quality tiers, language flag/code — issues #1 & #6) ---

/// White plate used behind brand/quality logos so a logo of any colour (e.g. a
/// black wordmark) stays legible regardless of the badge background setting.
const LOGO_PLATE: Rgba<u8> = Rgba([255, 255, 255, 255]);

/// Cap an overlay image's width relative to its height so a very wide wordmark
/// (e.g. the Dolby Vision logo) can't blow up a badge row.
const OVERLAY_IMAGE_MAX_ASPECT: f32 = 4.5;

/// A non-rating overlay badge rendered as a single cell — no label+value split.
/// Built in `serve.rs` from the quality (#1) and main-language (#6) settings and
/// appended after the rating badges so it bypasses ratings ordering/limiting.
#[derive(Debug, Clone)]
pub enum OverlayBadge {
    /// Uppercase token on a dark chip (a quality tier in text style, or the
    /// language ISO code).
    Text(String),
    /// A brand/quality logo rendered on a white plate.
    Logo(&'static RgbaImage),
    /// A country flag rendered on the configured chip background.
    Flag(&'static RgbaImage),
}

/// Single-fill background for an overlay cell (no label/value split), mirroring
/// [`section_colors`]'s value-section treatment for each background mode.
fn overlay_background(background: BadgeBackground) -> Option<Rgba<u8>> {
    match background {
        BadgeBackground::Default | BadgeBackground::Dark => Some(DARK_BG),
        BadgeBackground::Transparent => Some(with_alpha(DARK_BG, TRANSPARENT_ALPHA)),
        BadgeBackground::None => None,
    }
}

/// Fit an image within `max_w` × `max_h`, preserving aspect ratio.
fn image_fit(img: &RgbaImage, max_w: u32, max_h: u32) -> (u32, u32) {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return (max_w, max_h);
    }
    let scale = (max_w as f32 / w as f32).min(max_h as f32 / h as f32);
    ((w as f32 * scale).ceil().max(1.0) as u32, (h as f32 * scale).ceil().max(1.0) as u32)
}

/// Render a horizontal overlay badge (a single icon/logo/flag/text cell) sized
/// to match the horizontal rating badge height for the given scale.
pub fn render_overlay_badge(badge: &OverlayBadge, font: &FontArc, appearance: BadgeAppearance, badge_scale: f32) -> RgbaImage {
    let dims = ScaledDims::new(badge_scale);
    let fonts = BadgeFonts::new(font, badge_scale);
    let (pill_pad, pill_pad_v) = match appearance.shape {
        BadgeShape::Pill => (dims.pill_padding, dims.pill_padding_v),
        BadgeShape::Rounded => (0, 0),
    };
    let badge_h = dims.badge_height + pill_pad_v;

    // A logo always sits on a white plate (legibility); text and flags honor the
    // configured background. The plate/background spans the whole cell.
    let white_plate = matches!(badge, OverlayBadge::Logo(_));
    let plate = if white_plate { Some(LOGO_PLATE) } else { overlay_background(appearance.background) };

    let pad = match badge {
        OverlayBadge::Text(_) => dims.badge_value_padding_h,
        _ => dims.badge_padding_h,
    };

    // Resolve content (a scaled image, or text) and its drawn width.
    let scaled_image = match badge {
        OverlayBadge::Logo(img) | OverlayBadge::Flag(img) => {
            let max_w = (dims.icon_height as f32 * OVERLAY_IMAGE_MAX_ASPECT) as u32;
            let (w, h) = image_fit(img, max_w, dims.icon_height);
            Some(imageops::resize(*img, w, h, imageops::FilterType::Lanczos3))
        }
        OverlayBadge::Text(_) => None,
    };
    let content_w = match (badge, &scaled_image) {
        (OverlayBadge::Text(t), _) => text_width(t, &fonts.scaled),
        (_, Some(img)) => img.width(),
        _ => 0,
    };

    let total_width = pill_pad + pad + content_w + pad + pill_pad;
    let mut img = RgbaImage::new(total_width.max(1), badge_h);

    if let Some(color) = plate {
        draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(total_width.max(1), badge_h), color);
        round_corners(&mut img, corner_radius(appearance.shape, badge_h, dims.badge_radius));
    }
    // Shadow only when there is no plate/background (so content stays legible
    // drawn straight onto the artwork).
    let shadow = if plate.is_some() { None } else { shadow_offset(appearance.background, badge_h) };

    match (badge, scaled_image) {
        (OverlayBadge::Text(text), _) => {
            let x = pill_pad + pad;
            let y = (badge_h as i32 - fonts.scale.x as i32) / 2;
            draw_text_shadowed(&mut img, Rgba([255, 255, 255, 255]), x as i32, y, fonts.scale, fonts.font, text, shadow);
        }
        (_, Some(scaled)) => {
            let ix = pill_pad + pad;
            let iy = (badge_h.saturating_sub(scaled.height())) / 2;
            overlay_icon_shadowed(&mut img, &scaled, ix as i64, iy as i64, shadow);
        }
        _ => {}
    }

    img
}

/// Render a vertical overlay badge sized to the vertical rating badge width, so
/// it stacks cleanly with vertical rating badges (left/right positions).
pub fn render_overlay_badge_vertical(badge: &OverlayBadge, font: &FontArc, appearance: BadgeAppearance, badge_scale: f32) -> RgbaImage {
    let vert_badge_width = (BASE_VERT_BADGE_WIDTH as f32 * badge_scale).round() as u32;
    let pill_pad = match appearance.shape {
        BadgeShape::Pill => (BASE_PILL_PADDING as f32 * badge_scale).round() as u32,
        BadgeShape::Rounded => 0,
    };
    let pad_v = (BASE_VERT_BADGE_PADDING_V as f32 * badge_scale).round() as u32 + pill_pad;
    let icon_height = (BASE_ICON_HEIGHT as f32 * badge_scale).round() as u32;
    let badge_radius = (BASE_BADGE_RADIUS as f32 * badge_scale).round() as u32;
    let value_font_size = BASE_VERT_VALUE_FONT_SIZE * badge_scale;
    let value_scale = PxScale::from(value_font_size);

    let white_plate = matches!(badge, OverlayBadge::Logo(_));
    let plate = if white_plate { Some(LOGO_PLATE) } else { overlay_background(appearance.background) };

    // Fit content within the available width (minus padding).
    let inner_w = vert_badge_width.saturating_sub(2 * pad_v).max(1);
    let scaled_image = match badge {
        OverlayBadge::Logo(img) | OverlayBadge::Flag(img) => {
            let (w, h) = image_fit(img, inner_w, icon_height);
            Some(imageops::resize(*img, w, h, imageops::FilterType::Lanczos3))
        }
        OverlayBadge::Text(_) => None,
    };
    let content_h = match &scaled_image {
        Some(img) => img.height(),
        None => value_font_size as u32,
    };
    let total_height = pad_v + content_h + pad_v;
    let mut img = RgbaImage::new(vert_badge_width, total_height);

    if let Some(color) = plate {
        draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(vert_badge_width, total_height), color);
        round_corners(&mut img, corner_radius(appearance.shape, vert_badge_width, badge_radius));
    }
    let shadow = if plate.is_some() { None } else { shadow_offset(appearance.background, vert_badge_width) };

    match (badge, scaled_image) {
        (OverlayBadge::Text(text), _) => {
            let value_scaled_font = font.as_scaled(value_scale);
            let tw = text_width(text, &value_scaled_font);
            let x = (vert_badge_width.saturating_sub(tw)) / 2;
            let y = pad_v as i32;
            draw_text_shadowed(&mut img, Rgba([255, 255, 255, 255]), x as i32, y, value_scale, font, text, shadow);
        }
        (_, Some(scaled)) => {
            let ix = (vert_badge_width.saturating_sub(scaled.width())) / 2;
            let iy = pad_v;
            overlay_icon_shadowed(&mut img, &scaled, ix as i64, iy as i64, shadow);
        }
        _ => {}
    }

    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ratings::{RatingBadge, RatingSource};

    fn test_font() -> FontArc {
        FontArc::try_from_slice(crate::FONT_BYTES).unwrap()
    }

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
                let appearance = BadgeAppearance { shape, background };
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
            BadgeAppearance { shape: BadgeShape::Rounded, background: BadgeBackground::None });
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
            BadgeAppearance { shape: BadgeShape::Rounded, background: BadgeBackground::Default });
        let transparent = render_badge_appearance(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Rounded, background: BadgeBackground::Transparent });
        // Default keeps the opaque source colour; transparent is semi-opaque.
        assert!(default.get_pixel(probe.0, probe.1)[3] >= 200);
        assert_eq!(transparent.get_pixel(probe.0, probe.1)[3], TRANSPARENT_ALPHA);
    }

    #[test]
    fn pill_rounds_more_than_rounded() {
        let font = test_font();
        let badge = RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() };
        let rounded = render_badge_appearance(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Rounded, background: BadgeBackground::Default });
        let pill = render_badge_appearance(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Pill, background: BadgeBackground::Default });
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
            BadgeAppearance { shape: BadgeShape::Rounded, background: BadgeBackground::Default });
        let pill = render_badge_appearance(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Pill, background: BadgeBackground::Default });
        assert!(pill.width() > rounded.width(), "pill should pad its rounded left/right ends");
        // Vertical pills add padding at the top/bottom caps → taller than rounded.
        let rounded_v = render_vertical_badge(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Rounded, background: BadgeBackground::Default }, 1.0);
        let pill_v = render_vertical_badge(&badge, &font, LabelStyle::Text,
            BadgeAppearance { shape: BadgeShape::Pill, background: BadgeBackground::Default }, 1.0);
        assert!(pill_v.height() > rounded_v.height(), "vertical pill should pad its rounded top/bottom ends");
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

    #[test]
    fn render_overlay_text_badge_matches_badge_height() {
        let badge = OverlayBadge::Text("4K".to_string());
        let img = render_overlay_badge(&badge, &test_font(), BadgeAppearance::default(), 1.0);
        assert_eq!(img.height(), BASE_BADGE_HEIGHT);
        assert!(img.width() > 0);
    }

    #[test]
    fn render_overlay_logo_and_flag_badges() {
        let font = test_font();
        let logo = crate::image::icons::quality_logo_for(crate::services::db::QualityTier::Uhd4k).unwrap();
        let flag = crate::image::icons::flag_for_lang("en").unwrap();
        for badge in [OverlayBadge::Logo(logo), OverlayBadge::Flag(flag)] {
            let h = render_overlay_badge(&badge, &font, BadgeAppearance::default(), 1.0);
            assert_eq!(h.height(), BASE_BADGE_HEIGHT);
            assert!(h.width() > 0);
            // Drew something (logo on plate / flag on chip).
            assert!(h.pixels().any(|p| p[3] > 0));
            let v = render_overlay_badge_vertical(&badge, &font, BadgeAppearance::default(), 1.0);
            assert_eq!(v.width(), BASE_VERT_BADGE_WIDTH);
            assert!(v.height() > 0);
        }
    }

    #[test]
    fn render_overlay_badge_all_appearances() {
        let font = test_font();
        let badge = OverlayBadge::Text("EN".to_string());
        for shape in ALL_SHAPES {
            for background in ALL_BACKGROUNDS {
                let appearance = BadgeAppearance { shape, background };
                let img = render_overlay_badge(&badge, &font, appearance, 1.0);
                assert!(img.width() > 0 && img.height() > 0, "{shape:?}/{background:?}");
            }
        }
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
