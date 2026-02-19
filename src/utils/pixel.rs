use image::RgbaImage;
use rayon::prelude::*;

/// processes all pixels in parallel with row awareness.
///
/// callback receives (x, y, pixel_bytes: &mut [u8; 4]).
/// this preserves row ordering for tiling operations like grain.
pub fn par_rows_mut<F>(img: &mut RgbaImage, f: F)
where
    F: Fn(u32, u32, &mut [u8]) + Send + Sync,
{
    let width = img.width() as usize;
    let f = &f;

    img.par_chunks_mut(width * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, pixel_bytes) in row.chunks_exact_mut(4).enumerate() {
                f(x as u32, y as u32, pixel_bytes);
            }
        });
}

/// processes all pixels in parallel without row awareness.
///
/// callback receives pixel_bytes: &mut [u8].
/// faster for operations that don't need coordinate info.
///
/// optimized: uses larger chunk size (4096 bytes = 1024 pixels) to reduce
/// thread scheduling overhead while maintaining good parallelism.
pub fn process_pixels_par<F>(img: &mut RgbaImage, f: F)
where
    F: Fn(&mut [u8]) + Send + Sync,
{
    // process in chunks of 4096 bytes (1024 pixels)
    // this balances parallelism vs overhead for typical image sizes
    img.par_chunks_mut(4096).for_each(|chunk| {
        for pixel_bytes in chunk.chunks_exact_mut(4) {
            f(pixel_bytes);
        }
    });
}

/// fills all pixels with a color value.
///
/// optimized: uses the same chunk size as process_pixels_par for consistency
/// and fills entire chunks at once when possible using memset-like operations.
pub fn fill_solid(img: &mut RgbaImage, color: [u8; 4]) {
    let raw = img.as_mut();

    // for solid color fills, we can use more efficient approach
    // by filling the entire buffer at once with the pattern
    if color[0] == color[1] && color[1] == color[2] && color[2] == color[3] {
        // uniform color - can use fill()
        raw.fill(color[0]);
    } else {
        // non-uniform color - fill in parallel chunks
        raw.par_chunks_mut(4096).for_each(|chunk| {
            for pixel_bytes in chunk.chunks_exact_mut(4) {
                pixel_bytes[0] = color[0];
                pixel_bytes[1] = color[1];
                pixel_bytes[2] = color[2];
                pixel_bytes[3] = color[3];
            }
        });
    }
}
