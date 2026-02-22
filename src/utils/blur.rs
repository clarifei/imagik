use crate::utils::resize::{resize_rgba, resize_rgba_fast_auto};
use image::RgbaImage;
use libblur::{
    BlurImage, BlurImageMut, ConvolutionMode, EdgeMode, EdgeMode2D, FastBlurChannels,
    GaussianBlurParams, ThreadingPolicy,
};

/// applies gaussian blur with automatic downscaling optimization.
///
/// automatically downscales large images for faster processing, then upscales back.
/// optimization: blur is O(n²) on dimensions, so blurring at 1/4 size is ~16x faster.
pub fn apply_blur(img: RgbaImage, sigma: f32) -> RgbaImage {
    if sigma <= 0.0 {
        return img;
    }

    let (width, height) = (img.width(), img.height());
    let max_dimension = width.max(height);

    // determine downscale factor based on image size
    let downscale_factor = if max_dimension > 2000 {
        4
    } else if max_dimension > 1000 {
        2
    } else {
        1
    };

    let (working_img, adjusted_sigma) = if downscale_factor > 1 {
        let new_width = width / downscale_factor;
        let new_height = height / downscale_factor;
        // Use fast auto-selected filter for downscaling
        let downscaled = resize_rgba_fast_auto(&img, new_width, new_height);
        let sigma_multiplier = match downscale_factor {
            4 => 4.0,
            2 => 2.0,
            _ => 1.0,
        };
        eprintln!(
            "[BLUR] Downscaled {}x{} -> {}x{} for faster processing",
            width, height, new_width, new_height
        );
        (downscaled, sigma * sigma_multiplier)
    } else {
        (img, sigma)
    };

    let blurred = apply_libblur_rgba(&working_img, adjusted_sigma.max(0.1));

    // only upscale if we downscaled
    if downscale_factor > 1 {
        resize_rgba(&blurred, width, height)
    } else {
        blurred
    }
}

/// parses blur sigma from string.
/// accepts any positive floating point number.
pub fn parse_blur(value: &str) -> Option<f32> {
    value.parse::<f32>().ok().filter(|&s| s > 0.0)
}

/// applies libblur gaussian blur to rgba image.
///
/// uses libblur for high-performance gaussian blur with adaptive threading.
/// this is a low-level function used by `apply_blur` after downscaling.
pub fn apply_libblur_rgba(img: &RgbaImage, sigma: f32) -> RgbaImage {
    let width = img.width();
    let height = img.height();
    let mut dst_buffer = vec![0u8; (width * height * 4) as usize];
    apply_libblur_rgba_into(img.as_raw(), width, height, sigma, &mut dst_buffer);

    RgbaImage::from_raw(width, height, dst_buffer).expect("valid rgba image")
}

/// blur raw RGBA bytes into a caller-provided destination buffer.
///
/// this allows hot effect chains to reuse working memory and avoid
/// per-request heap allocations.
pub fn apply_libblur_rgba_into(
    src_rgba: &[u8],
    width: u32,
    height: u32,
    sigma: f32,
    dst_rgba: &mut [u8],
) {
    let src = BlurImage::borrow(src_rgba, width, height, FastBlurChannels::Channels4);
    let mut dst = BlurImageMut::borrow(dst_rgba, width, height, FastBlurChannels::Channels4);
    let params = GaussianBlurParams::new_from_sigma(f64::from(sigma));

    libblur::gaussian_blur(
        &src,
        &mut dst,
        params,
        EdgeMode2D::new(EdgeMode::Reflect101),
        ThreadingPolicy::Adaptive,
        ConvolutionMode::FixedPoint,
    )
    .expect("gaussian blur should succeed");
}
