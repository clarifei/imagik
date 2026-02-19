use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{CpuExtensions, FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::RgbaImage;
use std::sync::OnceLock;

/// fast resize for rgba images using simd-optimized fast_image_resize.
///
/// this is the core resize function used by both transforms/resize.rs
/// and utils/blur.rs for consistent, optimized resizing.
///
/// automatically enables avx2 or sse4.1 simd extensions on x86_64.
pub fn resize_rgba_fast(img: &RgbaImage, width: u32, height: u32, filter: FilterType) -> RgbaImage {
    let src_width = img.width();
    let src_height = img.height();

    if src_width == width && src_height == height {
        return img.clone();
    }

    let mut dst = vec![0u8; (width * height * 4) as usize];
    resize_rgba_into(
        img.as_raw(),
        src_width,
        src_height,
        &mut dst,
        width,
        height,
        filter,
    );
    RgbaImage::from_raw(width, height, dst).unwrap()
}

/// convenience function for lanczos3 resize (most common use case).
pub fn resize_rgba(img: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    resize_rgba_fast(img, width, height, FilterType::Lanczos3)
}

/// resize raw RGBA bytes into a caller-provided destination buffer.
///
/// this avoids per-call heap allocations in hot paths where the caller can
/// reuse a preallocated `dst` buffer.
pub fn resize_rgba_into(
    src: &[u8],
    src_width: u32,
    src_height: u32,
    dst: &mut [u8],
    dst_width: u32,
    dst_height: u32,
    filter: FilterType,
) {
    let src_len = (src_width * src_height * 4) as usize;
    let dst_len = (dst_width * dst_height * 4) as usize;

    debug_assert!(src.len() >= src_len);
    debug_assert!(dst.len() >= dst_len);

    if src_width == dst_width && src_height == dst_height {
        dst[..src_len].copy_from_slice(&src[..src_len]);
        return;
    }

    let src_image = ImageRef::new(
        src_width,
        src_height,
        src,
        fast_image_resize::PixelType::U8x4,
    )
    .unwrap();
    let mut dst_image = Image::from_slice_u8(
        dst_width,
        dst_height,
        &mut dst[..dst_len],
        fast_image_resize::PixelType::U8x4,
    )
    .unwrap();

    let mut resizer = Resizer::new();
    unsafe {
        resizer.set_cpu_extensions(*detected_cpu_extensions());
    }
    resizer
        .resize(
            &src_image,
            &mut dst_image,
            Some(&ResizeOptions::new().resize_alg(ResizeAlg::Convolution(filter))),
        )
        .unwrap();
}

fn detected_cpu_extensions() -> &'static CpuExtensions {
    static CPU_EXTENSIONS: OnceLock<CpuExtensions> = OnceLock::new();
    CPU_EXTENSIONS.get_or_init(CpuExtensions::default)
}
