use image::RgbaImage;

/// processes all pixels with coordinate awareness.
///
/// callback receives `(x, y, pixel_bytes: &mut [u8])`.
/// preserves row ordering for operations needing spatial context
/// (e.g., grain with position-dependent seeds).
///
/// intentionally single-threaded to avoid oversubscription.
/// request-level concurrency already bounded at handler level.
pub fn par_rows_mut<F>(img: &mut RgbaImage, f: F)
where
    F: Fn(u32, u32, &mut [u8]) + Send + Sync,
{
    let width = usize::try_from(img.width()).unwrap_or(usize::MAX);
    let width_u32 = img.width();
    for (y, row) in (0..img.height()).zip(img.chunks_mut(width * 4)) {
        for (x, pixel_bytes) in (0..width_u32).zip(row.chunks_exact_mut(4)) {
            f(x, y, pixel_bytes);
        }
    }
}

/// processes all pixels without row awareness.
///
/// callback receives `pixel_bytes`: &mut [u8].
/// faster for operations that don't need coordinate info.
///
/// this intentionally stays single-threaded to avoid nested parallelism
/// when request-level concurrency is already bounded at the handler level.
pub fn process_pixels_par<F>(img: &mut RgbaImage, f: F)
where
    F: Fn(&mut [u8]) + Send + Sync,
{
    for pixel_bytes in img.chunks_mut(4) {
        f(pixel_bytes);
    }
}

/// fills all pixels with a color value.
///
/// for uniform colors we use `fill`; otherwise write 4-byte RGBA pixels directly.
pub fn fill_solid(img: &mut RgbaImage, color: [u8; 4]) {
    let raw = img.as_mut();

    // for solid color fills, we can use more efficient approach
    // by filling the entire buffer at once with the pattern
    if color[0] == color[1] && color[1] == color[2] && color[2] == color[3] {
        // uniform color - can use fill()
        raw.fill(color[0]);
    } else {
        for pixel_bytes in raw.chunks_mut(4) {
            pixel_bytes[0] = color[0];
            pixel_bytes[1] = color[1];
            pixel_bytes[2] = color[2];
            pixel_bytes[3] = color[3];
        }
    }
}
