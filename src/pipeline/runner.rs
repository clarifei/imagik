use super::params::Params;
use crate::encoding::webp::{calculate_webp_quality, encode_rgba_image_to_webp, encode_to_webp};
use crate::observability::metrics::{self, PipelineStage};
use crate::transforms::aspect::apply_aspect_ratio;
use crate::transforms::debug::apply_debug_overlay;
use crate::transforms::effect::ca::apply_chromatic;
use crate::transforms::effect::grain::apply_grain;
use crate::transforms::resize::{resize_with_mode, resize_with_mode_rgba};
use crate::transforms::rotate::rotate;
use crate::utils::blur::apply_blur;
use crate::utils::color::{hsv_to_rgb, luminance, rgb_to_hsv};
use crate::utils::image::to_rgba;
use crate::utils::pixel::process_pixels_par;
use image::RgbaImage;
use std::time::Instant;

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::many_single_char_names,
    reason = "hot-path pixel math requires float/int conversions; values are explicitly clamped before narrowing"
)]
/// main image transformation pipeline.
///
/// optimization strategies:
/// 1. early exit: if no transforms needed, encode source directly without conversion
/// 2. chain geometric transforms in dynamicimage (minimize conversions)
/// 3. skip redundant aspect ratio crop when resize is present
/// 4. apply blur AFTER resize (much faster on smaller images)
/// 5. color filters in one pass with fast path for simple operations
/// 6. grain effects combined in single pass when both specified
/// 7. encode rgba directly without dynamicimage conversion
/// 8. conditional rgba conversion (only if pixel ops needed)
pub fn apply_transforms_and_convert(source: &image::DynamicImage, params: &Params) -> Vec<u8> {
    let start_time = Instant::now();
    let transform_stage_start = Instant::now();

    eprintln!(
        "[PIPELINE] Starting transform pipeline for {}x{} image",
        source.width(),
        source.height()
    );

    // early exit: format conversion only (no geometry/effects/overlay)
    if !params.has_transforms() {
        let encode_start = Instant::now();
        let encoded =
            encode_dynamicimage_to_webp(source, params.webp_quality, params.webp_lossless);
        metrics::record_stage_latency(PipelineStage::Encode, encode_start.elapsed());
        return encoded;
    }

    // apply geometric transforms - returns rgba directly to avoid conversions
    let geo_start = Instant::now();
    let mut rgba_img = apply_geometric_transforms(source, params);
    eprintln!(
        "[PIPELINE] Geometric transforms (resize/rotate): {:?}",
        geo_start.elapsed()
    );

    // check if we need pixel-level operations (blur, color filters, effects)
    let needs_pixel_ops = params.has_pixel_ops();

    // optimization: if only geometric transforms, encode directly
    if !needs_pixel_ops && !params.debug {
        metrics::record_stage_latency(PipelineStage::Transform, transform_stage_start.elapsed());
        let encode_start = Instant::now();
        let encoded = encode_rgba_to_webp(&rgba_img, params.webp_quality, params.webp_lossless);
        eprintln!("[PIPELINE] WebP encoding: {:?}", encode_start.elapsed());
        metrics::record_stage_latency(PipelineStage::Encode, encode_start.elapsed());
        return encoded;
    }

    // apply pixel-level operations in optimal order
    // blur first (on potentially smaller image after resize)
    if let Some(sigma) = params.blur {
        let blur_start = Instant::now();
        rgba_img = apply_blur(rgba_img, sigma);
        eprintln!(
            "[PIPELINE] Blur (sigma={}): {:?}",
            sigma,
            blur_start.elapsed()
        );
    }

    // color filters - one pass with fast path for simple operations
    let color_start = Instant::now();
    apply_color_filters(&mut rgba_img, params);
    eprintln!("[PIPELINE] Color filters: {:?}", color_start.elapsed());

    // grain effects - combined in single pass if both specified
    if params.grain.is_some() || params.grain_grayscale.is_some() {
        let grain_start = Instant::now();
        rgba_img = apply_grain(
            rgba_img,
            params.grain,
            params.grain_grayscale,
            params.grain_threshold,
        );
        eprintln!("[PIPELINE] Grain effect: {:?}", grain_start.elapsed());
    }

    // chromatic aberration - lens distortion effect
    if let Some(amount) = params.chromatic_aberration {
        let ca_start = Instant::now();
        apply_chromatic(&mut rgba_img, amount, 1.0);
        eprintln!("[PIPELINE] Chromatic aberration: {:?}", ca_start.elapsed());
    }

    // debug overlay (if enabled) and encoding
    if params.debug {
        let processing_time = start_time.elapsed().as_millis();
        apply_debug_overlay(&mut rgba_img, processing_time);
    }

    metrics::record_stage_latency(PipelineStage::Transform, transform_stage_start.elapsed());
    let encode_start = Instant::now();
    let encoded = encode_rgba_to_webp(&rgba_img, params.webp_quality, params.webp_lossless);
    eprintln!("[PIPELINE] WebP encoding: {:?}", encode_start.elapsed());
    metrics::record_stage_latency(PipelineStage::Encode, encode_start.elapsed());
    eprintln!("[PIPELINE] Total pipeline time: {:?}", start_time.elapsed());
    encoded
}

/// applies geometric transforms with minimal format conversions.
///
/// optimization: chains rotation and aspect crop in `DynamicImage` format,
/// converting to `RgbaImage` only once at the end. avoids back-and-forth
/// conversions that would occur if each transform converted independently.
///
/// aspect ratio crop is skipped if resize is present (resize handles aspect).
fn apply_geometric_transforms(source: &image::DynamicImage, params: &Params) -> RgbaImage {
    let needs_resize = params.width.is_some() || params.height.is_some();
    let needs_rotation = params.rotate.is_some();
    let needs_aspect = params.aspect.is_some() && !needs_resize;

    if !needs_rotation && !needs_aspect {
        let source_rgba = source.to_rgba8();

        if needs_resize {
            return resize_with_mode_rgba(
                &source_rgba,
                params.width,
                params.height,
                params.crop_mode,
                params.gravity,
                params.background,
            );
        }
        return source_rgba;
    }

    let mut img = source.clone();

    // rotation (in dynamicimage format)
    if let Some(angle) = params.rotate {
        img = rotate(img, angle);
    }

    // aspect ratio crop - only if NO resize specified
    if needs_aspect {
        let aspect = params.aspect.as_deref().expect("aspect should exist");
        img = apply_aspect_ratio(img, aspect, params.gravity);
    }

    // resize (converts to rgba internally, returns rgba directly)
    if needs_resize {
        resize_with_mode(
            &img,
            params.width,
            params.height,
            params.crop_mode,
            params.gravity,
            params.background,
        )
    } else {
        // no resize, just convert to rgba once
        to_rgba(img)
    }
}

/// applies color filters with multi-tier optimization.
///
/// path selection (fastest to slowest):
/// 1. no-op: no color adjustments requested
/// 2. simple: only grayscale/invert (no HSV conversion)
/// 3. rgb: brightness/contrast only (no HSV conversion)
/// 4. full: hue/saturation/vibrance requiring HSV conversion
///
/// all paths use `process_pixels_par` for parallel pixel processing.
#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    reason = "Color pipeline math runs in tight loops with explicit clamping before integer conversion."
)]
fn apply_color_filters(rgba_img: &mut RgbaImage, params: &Params) {
    let has_brightness = params.brightness.is_some();
    let has_contrast = params.contrast.is_some();
    let brightness_factor = params.brightness.map_or(1.0, |b| 1.0 + (b as f32 / 200.0));
    let contrast_factor = params.contrast.map_or(1.0, |c| 1.0 + (c / 100.0));
    let has_hue = params.hue_rotate.is_some();
    let has_saturation = params.saturation.is_some();
    let has_vibrance = params.vibrance.is_some();
    let hue_offset = params.hue_rotate.map_or(0.0, |h| h as f32);
    let saturation_factor = params.saturation.map_or(1.0, |s| 1.0 + (s / 100.0));
    let vibrance_factor = params.vibrance.map_or(0.0, |v| v / 100.0);
    // grayscale forces saturation to zero, so hue/saturation/vibrance become no-ops.
    let needs_hsv = !params.grayscale && (has_hue || has_saturation || has_vibrance);
    let needs_simple = params.grayscale || params.invert;
    let needs_rgb = has_brightness || has_contrast;

    if !needs_hsv && !needs_simple && !needs_rgb {
        return;
    }

    if !needs_hsv && !needs_rgb {
        apply_simple_filters(rgba_img, params.grayscale, params.invert);
        return;
    }

    if !needs_hsv {
        process_pixels_par(rgba_img, |pixel| {
            let mut r = f32::from(pixel[0]);
            let mut g = f32::from(pixel[1]);
            let mut b = f32::from(pixel[2]);

            if has_brightness {
                r = (r * brightness_factor).clamp(0.0, 255.0);
                g = (g * brightness_factor).clamp(0.0, 255.0);
                b = (b * brightness_factor).clamp(0.0, 255.0);
            }

            if has_contrast {
                r = (r - 127.5)
                    .mul_add(contrast_factor, 127.5)
                    .clamp(0.0, 255.0);
                g = (g - 127.5)
                    .mul_add(contrast_factor, 127.5)
                    .clamp(0.0, 255.0);
                b = (b - 127.5)
                    .mul_add(contrast_factor, 127.5)
                    .clamp(0.0, 255.0);
            }

            if params.grayscale {
                let gray = luminance(r as u8, g as u8, b as u8) as u8;
                pixel[0] = gray;
                pixel[1] = gray;
                pixel[2] = gray;
            } else {
                pixel[0] = r as u8;
                pixel[1] = g as u8;
                pixel[2] = b as u8;
            }

            if params.invert {
                pixel[0] = 255 - pixel[0];
                pixel[1] = 255 - pixel[1];
                pixel[2] = 255 - pixel[2];
            }
        });

        return;
    }

    process_pixels_par(rgba_img, |pixel| {
        let mut r = f32::from(pixel[0]);
        let mut g = f32::from(pixel[1]);
        let mut b = f32::from(pixel[2]);

        if has_brightness {
            r = (r * brightness_factor).clamp(0.0, 255.0);
            g = (g * brightness_factor).clamp(0.0, 255.0);
            b = (b * brightness_factor).clamp(0.0, 255.0);
        }

        if has_contrast {
            r = (r - 127.5)
                .mul_add(contrast_factor, 127.5)
                .clamp(0.0, 255.0);
            g = (g - 127.5)
                .mul_add(contrast_factor, 127.5)
                .clamp(0.0, 255.0);
            b = (b - 127.5)
                .mul_add(contrast_factor, 127.5)
                .clamp(0.0, 255.0);
        }

        if params.grayscale {
            let gray = luminance(r as u8, g as u8, b as u8) as f32;
            r = gray;
            g = gray;
            b = gray;
        }

        let (mut h, mut s, v) = rgb_to_hsv(r as u8, g as u8, b as u8);

        if has_hue {
            h = (h + hue_offset).rem_euclid(360.0);
        }
        if has_saturation {
            s = (s * saturation_factor).clamp(0.0, 1.0);
        }
        if has_vibrance {
            s = (s + vibrance_factor * (1.0 - s)).clamp(0.0, 1.0);
        }
        if params.grayscale {
            s = 0.0;
        }

        let (mut out_r, mut out_g, mut out_b) = hsv_to_rgb(h, s, v);

        if params.invert {
            out_r = 255 - out_r;
            out_g = 255 - out_g;
            out_b = 255 - out_b;
        }

        pixel[0] = out_r;
        pixel[1] = out_g;
        pixel[2] = out_b;
    });
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "Luminance is bounded to `[0,255]` before narrowing."
)]
/// fast path for simple filters without HSV conversion.
/// grayscale: average rgb channels using luminance weights
/// invert: flip all channels
fn apply_simple_filters(img: &mut RgbaImage, grayscale: bool, invert: bool) {
    process_pixels_par(img, |pixel| {
        if grayscale {
            let gray = luminance(pixel[0], pixel[1], pixel[2]) as u8;
            pixel[0] = gray;
            pixel[1] = gray;
            pixel[2] = gray;
        }

        if invert {
            pixel[0] = 255 - pixel[0];
            pixel[1] = 255 - pixel[1];
            pixel[2] = 255 - pixel[2];
        }
    });
}

/// encodes rgba image directly to webp with calculated quality.
fn encode_rgba_to_webp(img: &RgbaImage, webp_quality: Option<u32>, webp_lossless: bool) -> Vec<u8> {
    let quality = calculate_webp_quality(webp_quality, webp_lossless);
    encode_rgba_image_to_webp(img, quality)
}

/// encodes dynamicimage directly to webp without conversion.
/// optimization: avoids rgba conversion for simple format conversion.
fn encode_dynamicimage_to_webp(
    img: &image::DynamicImage,
    webp_quality: Option<u32>,
    webp_lossless: bool,
) -> Vec<u8> {
    let quality = calculate_webp_quality(webp_quality, webp_lossless);
    encode_to_webp(img, quality)
}
