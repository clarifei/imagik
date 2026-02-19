use crate::models::Gravity;
use crate::transforms::resize::crop_to_dimensions;
use image::{DynamicImage, GenericImageView};

/// crops image to a target aspect ratio (e.g., "16:9", "1:1", "4:3").
///
/// flow:
/// 1. parse the aspect ratio string (format: "width:height")
/// 2. calculate target dimensions that match the ratio
/// 3. use the existing crop_to_dimensions to do the actual cropping
///
/// edge cases:
/// - invalid format: returns original image unchanged
/// - zero/negative ratios: returns original image
/// - already correct ratio (within 1%): returns original image (no-op)
///
/// uses `gravity` to determine which part of the image to keep when cropping.
pub fn apply_aspect_ratio(img: DynamicImage, aspect_ratio: &str, gravity: Gravity) -> DynamicImage {
    let mut parts = aspect_ratio.splitn(2, ':');
    let (width_str, height_str) = match (parts.next(), parts.next()) {
        (Some(w), Some(h)) => (w, h),
        _ => return img,
    };

    let Ok(width_ratio) = width_str.parse::<f32>() else {
        return img;
    };
    let Ok(height_ratio) = height_str.parse::<f32>() else {
        return img;
    };

    if height_ratio <= 0.0 || width_ratio <= 0.0 {
        return img;
    }

    let target_ratio = width_ratio / height_ratio;
    let (current_width, current_height) = img.dimensions();

    if current_height == 0 {
        return img;
    }

    let current_ratio = current_width as f32 / current_height as f32;

    // 0.01 tolerance avoids unnecessary crops when ratios are "close enough"
    // (e.g., 16:9 vs 1.777... floating point weirdness)
    if (current_ratio - target_ratio).abs() <= 0.01 {
        return img;
    }

    // calculate the largest rectangle with the target ratio that fits inside
    // the current image — we never upscale for aspect ratio, only crop
    let (target_w, target_h) = if current_ratio > target_ratio {
        (
            (current_height as f32 * target_ratio) as u32,
            current_height,
        )
    } else {
        (current_width, (current_width as f32 / target_ratio) as u32)
    };

    crop_to_dimensions(img, target_w.max(1), target_h.max(1), gravity)
}
