use image::{DynamicImage, RgbaImage};

/// ensures we have an rgba8 image to work with.
///
/// if the image is already rgba8, returns it directly.
/// otherwise, converts to rgba8 format.
///
/// most image processing operations need consistent channel layout,
/// and rgba8 is the common denominator.
pub fn to_rgba(img: DynamicImage) -> RgbaImage {
    match img {
        DynamicImage::ImageRgba8(rgba) => rgba,
        _ => img.to_rgba8(),
    }
}
