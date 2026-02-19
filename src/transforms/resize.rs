use crate::models::{CropMode, Gravity};
use crate::utils::pixel::fill_solid;
use crate::utils::resize::resize_rgba_fast;
use fast_image_resize::FilterType;
use image::{DynamicImage, GenericImageView, RgbaImage};
use std::borrow::Cow;

/// resizes image with specified crop mode - returns rgba directly.
///
/// optimized: skips the dynamicimage wrapper entirely.
/// use this when you need rgba output for pixel operations.
pub fn resize_with_mode(
    img: &DynamicImage,
    width: Option<u32>,
    height: Option<u32>,
    mode: CropMode,
    gravity: Gravity,
    background: Option<[u8; 4]>,
) -> RgbaImage {
    let rgba = source_rgba(img);
    resize_with_mode_rgba(rgba.as_ref(), width, height, mode, gravity, background)
}

pub fn resize_with_mode_rgba(
    img: &RgbaImage,
    width: Option<u32>,
    height: Option<u32>,
    mode: CropMode,
    gravity: Gravity,
    background: Option<[u8; 4]>,
) -> RgbaImage {
    match mode {
        CropMode::Fill | CropMode::Crop => resize_fill(img, width, height, gravity),
        CropMode::Fit => resize_fit(img, width, height),
        CropMode::Scale => resize_scale(img, width, height),
        CropMode::Pad => {
            let bg = background.unwrap_or([255, 255, 255, 255]);
            resize_pad(img, width, height, bg)
        }
    }
}

/// crops image to exact dimensions using gravity for positioning.
pub fn crop_to_dimensions(
    img: DynamicImage,
    target_width: u32,
    target_height: u32,
    gravity: Gravity,
) -> DynamicImage {
    if !is_valid_size(target_width, target_height) {
        return img;
    }

    let (img_width, img_height) = img.dimensions();

    if target_width >= img_width && target_height >= img_height {
        return img;
    }

    let crop_width = target_width.min(img_width);
    let crop_height = target_height.min(img_height);

    let x = calculate_gravity_offset(img_width, crop_width, gravity, true);
    let y = calculate_gravity_offset(img_height, crop_height, gravity, false);

    img.crop_imm(x, y, crop_width, crop_height)
}

fn resize_fill(
    img: &RgbaImage,
    width: Option<u32>,
    height: Option<u32>,
    gravity: Gravity,
) -> RgbaImage {
    let (target_w, target_h) = resolve_dimensions((img.width(), img.height()), width, height);

    if !is_valid_size(target_w, target_h) {
        return img.clone();
    }

    let (resize_w, resize_h) =
        calculate_cover_dimensions((img.width(), img.height()), target_w, target_h);
    let resized = resize_rgba_fast(img, resize_w, resize_h, FilterType::Lanczos3);

    // crop to exact dimensions
    let (img_w, img_h) = (resized.width(), resized.height());
    if target_w >= img_w && target_h >= img_h {
        return resized;
    }

    let crop_w = target_w.min(img_w);
    let crop_h = target_h.min(img_h);
    let x = calculate_gravity_offset(img_w, crop_w, gravity, true);
    let y = calculate_gravity_offset(img_h, crop_h, gravity, false);

    crop(&resized, x, y, crop_w, crop_h)
}

fn resize_fit(img: &RgbaImage, width: Option<u32>, height: Option<u32>) -> RgbaImage {
    let (target_w, target_h) = resolve_dimensions((img.width(), img.height()), width, height);

    if !is_valid_size(target_w, target_h) {
        return img.clone();
    }

    let (new_w, new_h) =
        calculate_contain_dimensions((img.width(), img.height()), target_w, target_h);
    resize_rgba_fast(img, new_w.max(1), new_h.max(1), FilterType::Lanczos3)
}

fn resize_scale(img: &RgbaImage, width: Option<u32>, height: Option<u32>) -> RgbaImage {
    let (target_w, target_h) = resolve_dimensions((img.width(), img.height()), width, height);

    if !is_valid_size(target_w, target_h) {
        return img.clone();
    }

    resize_rgba_fast(img, target_w, target_h, FilterType::Lanczos3)
}

fn resize_pad(
    img: &RgbaImage,
    width: Option<u32>,
    height: Option<u32>,
    background: [u8; 4],
) -> RgbaImage {
    let (target_w, target_h) = resolve_dimensions((img.width(), img.height()), width, height);

    if !is_valid_size(target_w, target_h) {
        return img.clone();
    }

    let (new_w, new_h) =
        calculate_contain_dimensions((img.width(), img.height()), target_w, target_h);
    let resized = resize_rgba_fast(img, new_w.max(1), new_h.max(1), FilterType::Lanczos3);
    let (resized_w, resized_h) = (resized.width(), resized.height());

    let mut output = RgbaImage::new(target_w, target_h);
    fill_solid(&mut output, background);

    let offset_x = (target_w - resized_w) / 2;
    let offset_y = (target_h - resized_h) / 2;

    overlay_rgba(&mut output, &resized, offset_x as i64, offset_y as i64);

    output
}

fn crop(img: &RgbaImage, x: u32, y: u32, width: u32, height: u32) -> RgbaImage {
    let mut result = RgbaImage::new(width, height);
    let img_width = img.width();
    let img_raw = img.as_raw();
    let result_raw = result.as_mut();

    for dy in 0..height {
        let src_y = y + dy;
        if src_y >= img.height() {
            break;
        }

        // copy entire row at once using copy_from_slice (much faster!)
        let src_start = ((src_y * img_width + x) * 4) as usize;
        let src_end = src_start + (width * 4) as usize;
        let dst_start = (dy * width * 4) as usize;
        let dst_end = dst_start + (width * 4) as usize;

        if src_end <= img_raw.len() && dst_end <= result_raw.len() {
            result_raw[dst_start..dst_end].copy_from_slice(&img_raw[src_start..src_end]);
        }
    }

    result
}

fn overlay_rgba(dest: &mut RgbaImage, src: &RgbaImage, x: i64, y: i64) {
    let (src_w, src_h) = (src.width() as i64, src.height() as i64);
    let (dest_w, dest_h) = (dest.width() as i64, dest.height() as i64);
    let src_raw = src.as_raw();
    let dest_raw = dest.as_mut();

    // sequential processing with slice operations for efficiency
    // parallelization overhead not worth it for typical pad operations
    for sy in 0..src_h {
        let src_row_start = (sy * src_w * 4) as usize;
        let dy = y + sy;

        if dy < 0 || dy >= dest_h {
            continue;
        }

        let dest_row_start = (dy * dest_w * 4) as usize;
        let row_start_dx = x.max(0);
        let row_end_dx = (x + src_w).min(dest_w);

        if row_start_dx >= row_end_dx {
            continue;
        }

        // calculate valid range for this row
        let src_start_col = (row_start_dx - x).max(0) as usize;
        let src_end_col = (row_end_dx - x).min(src_w) as usize;
        let dest_start_col = row_start_dx as usize;

        if src_start_col >= src_end_col {
            continue;
        }

        let bytes_to_copy = (src_end_col - src_start_col) * 4;
        let src_idx = src_row_start + src_start_col * 4;
        let dest_idx = dest_row_start + dest_start_col * 4;

        if src_idx + bytes_to_copy <= src_raw.len() && dest_idx + bytes_to_copy <= dest_raw.len() {
            dest_raw[dest_idx..dest_idx + bytes_to_copy]
                .copy_from_slice(&src_raw[src_idx..src_idx + bytes_to_copy]);
        }
    }
}

fn resolve_dimensions(
    (img_width, img_height): (u32, u32),
    target_width: Option<u32>,
    target_height: Option<u32>,
) -> (u32, u32) {
    match (target_width, target_height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => {
            let aspect = img_height as f32 / img_width.max(1) as f32;
            (w, (w as f32 * aspect) as u32)
        }
        (None, Some(h)) => {
            let aspect = img_width as f32 / img_height.max(1) as f32;
            ((h as f32 * aspect) as u32, h)
        }
        (None, None) => (img_width, img_height),
    }
}

fn is_valid_size(w: u32, h: u32) -> bool {
    w > 0 && h > 0
}

fn calculate_cover_dimensions(
    (img_w, img_h): (u32, u32),
    target_w: u32,
    target_h: u32,
) -> (u32, u32) {
    let img_aspect = img_w as f32 / img_h as f32;
    let target_aspect = target_w as f32 / target_h as f32;

    if img_aspect > target_aspect {
        ((target_h as f32 * img_aspect) as u32, target_h)
    } else {
        (target_w, (target_w as f32 / img_aspect) as u32)
    }
}

fn calculate_contain_dimensions(
    (img_w, img_h): (u32, u32),
    target_w: u32,
    target_h: u32,
) -> (u32, u32) {
    let img_aspect = img_w as f32 / img_h as f32;
    let target_aspect = target_w as f32 / target_h as f32;

    if img_aspect > target_aspect {
        (target_w, (target_w as f32 / img_aspect) as u32)
    } else {
        ((target_h as f32 * img_aspect) as u32, target_h)
    }
}

fn calculate_gravity_offset(
    img_size: u32,
    crop_size: u32,
    gravity: Gravity,
    is_horizontal: bool,
) -> u32 {
    if crop_size >= img_size {
        return 0;
    }

    match (gravity, is_horizontal) {
        (Gravity::West, true) | (Gravity::North, false) => 0,
        (Gravity::East, true) | (Gravity::South, false) => img_size - crop_size,
        _ => (img_size - crop_size) / 2,
    }
}

fn source_rgba(img: &DynamicImage) -> Cow<'_, RgbaImage> {
    if let Some(rgba) = img.as_rgba8() {
        Cow::Borrowed(rgba)
    } else {
        Cow::Owned(img.to_rgba8())
    }
}
