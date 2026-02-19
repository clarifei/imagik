use image::DynamicImage;

/// rotates image by 90-degree increments.
///
/// only handles 0, 90, 180, 270 — arbitrary angles would require
/// transparent backgrounds and more complex math. for a web image
/// service, right angles cover 99% of use cases anyway.
///
/// uses `rem_euclid` to normalize angles (handles negative inputs
/// and angles > 360 gracefully).
pub fn rotate(img: DynamicImage, angle: i32) -> DynamicImage {
    match angle.rem_euclid(360) {
        0 => img,
        90 => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        _ => img, // shouldn't happen due to parser validation, but safe fallback
    }
}
