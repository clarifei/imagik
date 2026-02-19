use crate::utils::color::luminance;
use crate::utils::parser::{parse_f32_range, parse_positive_f32};
use crate::utils::pixel::par_rows_mut;
use image::RgbaImage;
use std::sync::OnceLock;

// precomputed noise textures — generated once at first use, then reused.
const NOISE_SIZE: usize = 256;

static GRAYSCALE_NOISE: OnceLock<Vec<i8>> = OnceLock::new();
static COLOR_NOISE: OnceLock<Vec<(i8, i8, i8)>> = OnceLock::new();

fn init_noise() {
    GRAYSCALE_NOISE.get_or_init(|| {
        let mut rng = fastrand::Rng::with_seed(0x123456789abcdef);
        (0..(NOISE_SIZE * NOISE_SIZE))
            .map(|_| rng.i8(-51..=51))
            .collect()
    });

    COLOR_NOISE.get_or_init(|| {
        let mut rng = fastrand::Rng::with_seed(0xfedcba9876543210);
        (0..(NOISE_SIZE * NOISE_SIZE))
            .map(|_| (rng.i8(-51..=51), rng.i8(-51..=51), rng.i8(-51..=51)))
            .collect()
    });
}

/// applies film grain to rgba image directly.
///
/// optimized version that handles both color and grayscale grain in a single pass
/// when both are specified, using precomputed noise textures.
pub fn apply_grain(
    mut rgba_img: RgbaImage,
    color_grain: Option<f32>,
    gray_grain: Option<f32>,
    threshold: f32,
) -> RgbaImage {
    let has_color = color_grain.is_some_and(|v| v > 0.0);
    let has_gray = gray_grain.is_some_and(|v| v > 0.0);

    if !has_color && !has_gray {
        return rgba_img;
    }

    // if only one type, handle with optimized single-pass
    if has_color && !has_gray {
        return apply_single_grain(rgba_img, color_grain.unwrap(), false, threshold);
    }
    if !has_color && has_gray {
        return apply_single_grain(rgba_img, gray_grain.unwrap(), true, threshold);
    }

    // both types: combine in single pass
    init_noise();

    let color_scale = color_grain.unwrap() / 100.0;
    let gray_scale = gray_grain.unwrap() / 100.0;
    let luminance_threshold = (threshold.clamp(0.0, 1.0) * 255.0) as u32;

    let gray_noise = GRAYSCALE_NOISE.get().unwrap();
    let color_noise = COLOR_NOISE.get().unwrap();

    par_rows_mut(&mut rgba_img, |x, y, pixel_bytes| {
        let lum = luminance(pixel_bytes[0], pixel_bytes[1], pixel_bytes[2]);
        if lum <= luminance_threshold {
            return;
        }

        let noise_y = (y as usize) & 0xFF;
        let noise_x = (x as usize) & 0xFF;
        let noise_idx = noise_y * NOISE_SIZE + noise_x;

        // apply color grain
        let (nr, ng, nb) = color_noise[noise_idx];
        let r_val = (nr as f32 * color_scale) as i16;
        let g_val = (ng as f32 * color_scale) as i16;
        let b_val = (nb as f32 * color_scale) as i16;

        // apply grayscale grain (same value to all channels)
        let gray_val = (gray_noise[noise_idx] as f32 * gray_scale) as i16;

        pixel_bytes[0] = (pixel_bytes[0] as i16 + r_val + gray_val).clamp(0, 255) as u8;
        pixel_bytes[1] = (pixel_bytes[1] as i16 + g_val + gray_val).clamp(0, 255) as u8;
        pixel_bytes[2] = (pixel_bytes[2] as i16 + b_val + gray_val).clamp(0, 255) as u8;
    });

    rgba_img
}

/// parses grain intensity from string.
pub fn parse_grain(value: &str) -> Option<f32> {
    parse_positive_f32(value).filter(|&v| v <= 100.0)
}

/// parses grain threshold from string.
pub fn parse_grain_threshold(value: &str) -> Option<f32> {
    parse_f32_range(value, 0.0, 1.0)
}

/// internal: applies single type of grain (color or grayscale).
fn apply_single_grain(
    mut rgba_img: RgbaImage,
    intensity: f32,
    grayscale: bool,
    threshold: f32,
) -> RgbaImage {
    if intensity <= 0.0 {
        return rgba_img;
    }

    init_noise();

    let intensity = intensity.clamp(0.0, 100.0);
    let noise_scale = intensity / 100.0;
    let luminance_threshold = (threshold.clamp(0.0, 1.0) * 255.0) as u32;

    if grayscale {
        let gray_noise = GRAYSCALE_NOISE.get().unwrap();
        par_rows_mut(&mut rgba_img, |x, y, pixel_bytes| {
            let lum = luminance(pixel_bytes[0], pixel_bytes[1], pixel_bytes[2]);
            if lum <= luminance_threshold {
                return;
            }

            let noise_y = (y as usize) & 0xFF;
            let noise_x = (x as usize) & 0xFF;
            let noise_idx = noise_y * NOISE_SIZE + noise_x;
            let noise_val = (gray_noise[noise_idx] as f32 * noise_scale) as i16;

            pixel_bytes[0] = (pixel_bytes[0] as i16 + noise_val).clamp(0, 255) as u8;
            pixel_bytes[1] = (pixel_bytes[1] as i16 + noise_val).clamp(0, 255) as u8;
            pixel_bytes[2] = (pixel_bytes[2] as i16 + noise_val).clamp(0, 255) as u8;
        });
    } else {
        let color_noise = COLOR_NOISE.get().unwrap();
        par_rows_mut(&mut rgba_img, |x, y, pixel_bytes| {
            let lum = luminance(pixel_bytes[0], pixel_bytes[1], pixel_bytes[2]);
            if lum <= luminance_threshold {
                return;
            }

            let noise_y = (y as usize) & 0xFF;
            let noise_x = (x as usize) & 0xFF;
            let noise_idx = noise_y * NOISE_SIZE + noise_x;
            let (nr, ng, nb) = color_noise[noise_idx];

            pixel_bytes[0] =
                (pixel_bytes[0] as i16 + (nr as f32 * noise_scale) as i16).clamp(0, 255) as u8;
            pixel_bytes[1] =
                (pixel_bytes[1] as i16 + (ng as f32 * noise_scale) as i16).clamp(0, 255) as u8;
            pixel_bytes[2] =
                (pixel_bytes[2] as i16 + (nb as f32 * noise_scale) as i16).clamp(0, 255) as u8;
        });
    }

    rgba_img
}
