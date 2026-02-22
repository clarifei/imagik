use crate::observability::metrics::{self, ScratchBuffer};
use crate::utils::parser::parse_positive_f32;
use crate::utils::resize::resize_rgba_into;
use fast_image_resize::FilterType;
use image::RgbaImage;
use std::cell::RefCell;
use std::time::{Duration, Instant};

#[derive(Default)]
struct CaScratch {
    downscaled: Vec<u8>,
    downscaled_meta: BufferMeta,
    working: Vec<u8>,
    working_meta: BufferMeta,
    full_result: Vec<u8>,
    full_result_meta: BufferMeta,
    request_index: u64,
}

#[derive(Default, Clone, Copy)]
struct BufferMeta {
    last_growth_request: u64,
    last_growth_at: Option<Instant>,
}

impl BufferMeta {
    #[allow(
        clippy::missing_const_for_fn,
        reason = "Uses `Instant`, which is runtime data and not meaningful in const context."
    )]
    fn note_growth(&mut self, request_index: u64, now: Instant) {
        self.last_growth_request = request_index;
        self.last_growth_at = Some(now);
    }
}

const ACTIVE_FLOOR_BYTES: usize = 256 * 1024;
const OPTIONAL_IDLE_RETAIN_BYTES: usize = 0;
const RETAIN_CEILING_BYTES: usize = 32 * 1024 * 1024;
const RETAIN_DOWNSHIFT_FACTOR: usize = 4;
const DECAY_REQUEST_WINDOW: u64 = 128;
const DECAY_IDLE_SECONDS: u64 = 20;

thread_local! {
    // thread-local scratch avoids lock contention and removes repeated
    // per-request allocations in CA-heavy chains.
    static CA_SCRATCH: RefCell<CaScratch> = RefCell::new(CaScratch::default());
}

/// applies chromatic aberration lens distortion effect.
///
/// simulates lens distortion by offsetting color channels radially from center:
/// - red channel shifted outward
/// - blue channel shifted inward
/// - green channel stays centered
///
/// parameters:
/// - `amount`: effect strength, 0.0-0.02 recommended range
/// - `radial`: edge enhancement, 0.0=uniform, 1.0=radial falloff
///
/// performance optimization: processes at half resolution then upscales.
/// ca is a subtle artistic effect that works fine at reduced resolution.
/// memory bandwidth and computation reduced by ~4x.
#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "Scratch-buffer orchestration is intentionally linear to keep retention and trim decisions explicit."
)]
pub fn apply_chromatic(img: &mut RgbaImage, amount: f32, radial: f32) {
    if amount <= 0.0 {
        return;
    }

    let width = img.width();
    let height = img.height();

    // optimization: process ca at half resolution
    // ca is a subtle distortion effect that works fine at lower res
    let should_downscale = width > 512 && height > 512;

    CA_SCRATCH.with(|scratch_cell| {
        let mut scratch = scratch_cell.borrow_mut();
        let now = Instant::now();
        scratch.request_index = scratch.request_index.wrapping_add(1);
        let request_index = scratch.request_index;

        let mut downscaled_meta = scratch.downscaled_meta;
        let mut working_meta = scratch.working_meta;
        let mut full_result_meta = scratch.full_result_meta;
        let mut downscaled_buf = std::mem::take(&mut scratch.downscaled);
        let mut working_buf = std::mem::take(&mut scratch.working);
        let mut full_result_buf = std::mem::take(&mut scratch.full_result);

        if should_downscale {
            let half_w = (width / 2).max(1);
            let half_h = (height / 2).max(1);
            let work_len = (half_w * half_h * 4) as usize;

            if ensure_len(&mut downscaled_buf, work_len) {
                downscaled_meta.note_growth(request_index, now);
            }
            if ensure_len(&mut working_buf, work_len) {
                working_meta.note_growth(request_index, now);
            }

            {
                let downscaled = &mut downscaled_buf[..work_len];
                resize_rgba_into(
                    img.as_raw(),
                    width,
                    height,
                    downscaled,
                    half_w,
                    half_h,
                    FilterType::Bilinear,
                );
            }

            {
                let downscaled = &downscaled_buf[..work_len];
                let working = &mut working_buf[..work_len];
                apply_ca_raw(downscaled, working, half_w, half_h, amount * 2.0, radial);
            }

            resize_rgba_into(
                &working_buf[..work_len],
                half_w,
                half_h,
                img.as_mut(),
                width,
                height,
                FilterType::Bilinear,
            );

            maybe_trim_buffer(
                &mut downscaled_buf,
                downscaled_meta,
                work_len,
                ACTIVE_FLOOR_BYTES,
                ACTIVE_FLOOR_BYTES,
                false,
                request_index,
                now,
            );
            maybe_trim_buffer(
                &mut working_buf,
                working_meta,
                work_len,
                ACTIVE_FLOOR_BYTES,
                ACTIVE_FLOOR_BYTES,
                false,
                request_index,
                now,
            );
            maybe_trim_buffer(
                &mut full_result_buf,
                full_result_meta,
                0,
                ACTIVE_FLOOR_BYTES,
                OPTIONAL_IDLE_RETAIN_BYTES,
                true,
                request_index,
                now,
            );
        } else {
            let full_len = (width * height * 4) as usize;

            if ensure_len(&mut full_result_buf, full_len) {
                full_result_meta.note_growth(request_index, now);
            }
            let result = &mut full_result_buf[..full_len];
            apply_ca_raw(img.as_raw(), result, width, height, amount, radial);
            img.as_mut().copy_from_slice(result);

            maybe_trim_buffer(
                &mut downscaled_buf,
                downscaled_meta,
                0,
                ACTIVE_FLOOR_BYTES,
                OPTIONAL_IDLE_RETAIN_BYTES,
                true,
                request_index,
                now,
            );
            maybe_trim_buffer(
                &mut working_buf,
                working_meta,
                0,
                ACTIVE_FLOOR_BYTES,
                OPTIONAL_IDLE_RETAIN_BYTES,
                true,
                request_index,
                now,
            );
            maybe_trim_buffer(
                &mut full_result_buf,
                full_result_meta,
                full_len,
                ACTIVE_FLOOR_BYTES,
                ACTIVE_FLOOR_BYTES,
                false,
                request_index,
                now,
            );
        }

        metrics::record_scratch_capacity(ScratchBuffer::Downscaled, downscaled_buf.capacity());
        metrics::record_scratch_capacity(ScratchBuffer::Working, working_buf.capacity());
        metrics::record_scratch_capacity(ScratchBuffer::FullResult, full_result_buf.capacity());

        scratch.downscaled_meta = downscaled_meta;
        scratch.working_meta = working_meta;
        scratch.full_result_meta = full_result_meta;
        scratch.downscaled = downscaled_buf;
        scratch.working = working_buf;
        scratch.full_result = full_result_buf;
    });
}

fn ensure_len(buf: &mut Vec<u8>, len: usize) -> bool {
    if buf.len() < len {
        let old_capacity = buf.capacity();
        buf.resize(len, 0);
        return buf.capacity() > old_capacity;
    }
    false
}

#[allow(
    clippy::too_many_arguments,
    reason = "Buffer management requires 8 parameters for proper memory optimization (capacity, usage patterns, idle detection, etc.)."
)]
fn maybe_trim_buffer(
    buf: &mut Vec<u8>,
    meta: BufferMeta,
    needed_len: usize,
    active_floor: usize,
    idle_retain: usize,
    release_when_idle: bool,
    request_index: u64,
    now: Instant,
) {
    let capacity = buf.capacity();
    if needed_len == 0 {
        if release_when_idle && capacity > idle_retain {
            buf.clear();
            buf.shrink_to(idle_retain);
        }
        return;
    }

    let retain = needed_len.max(active_floor);
    let oversized_for_workload = needed_len.saturating_mul(RETAIN_DOWNSHIFT_FACTOR) <= capacity;
    let above_ceiling = capacity > RETAIN_CEILING_BYTES;
    let decay_due = should_decay(meta, request_index, now);
    if !(oversized_for_workload || above_ceiling || decay_due) || capacity <= retain {
        return;
    }

    if buf.len() > retain {
        buf.truncate(retain);
    }
    buf.shrink_to(retain);
}

fn should_decay(meta: BufferMeta, request_index: u64, now: Instant) -> bool {
    if request_index.saturating_sub(meta.last_growth_request) >= DECAY_REQUEST_WINDOW {
        return true;
    }

    meta.last_growth_at.is_some_and(|last| {
        now.saturating_duration_since(last) >= Duration::from_secs(DECAY_IDLE_SECONDS)
    })
}

/// applies CA from source RGBA bytes into a preallocated output buffer.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::suboptimal_flops,
    reason = "Lens distortion sampling is float-heavy and conversion points are clamped before index narrowing."
)]
fn apply_ca_raw(
    source_raw: &[u8],
    result_raw: &mut [u8],
    width: u32,
    height: u32,
    amount: f32,
    radial: f32,
) {
    let width_f = width as f32;
    let height_f = height as f32;
    let row_stride = (width * 4) as usize;
    debug_assert!(source_raw.len() >= (width * height * 4) as usize);
    debug_assert!(result_raw.len() >= (width * height * 4) as usize);

    for (y, row) in result_raw.chunks_exact_mut(row_stride).enumerate() {
        let y = y as u32;
        let uv_y = y as f32 / height_f;

        for (x, pixel_bytes) in row.chunks_exact_mut(4).enumerate() {
            let x = x as u32;
            let src_idx = ((y * width + x) * 4) as usize;

            // copy channels that are not spatially shifted.
            pixel_bytes[1] = source_raw[src_idx + 1];
            pixel_bytes[3] = source_raw[src_idx + 3];

            // normalized uv coordinates (0.0 to 1.0)
            let uv_x = x as f32 / width_f;

            // center coordinates (-0.5 to 0.5)
            let center_x_norm = uv_x - 0.5;
            let center_y_norm = uv_y - 0.5;

            // distance from center (0.0 at center, ~0.7 at corners)
            let dist_sq = center_x_norm * center_x_norm + center_y_norm * center_y_norm;
            let dist = dist_sq.sqrt();

            // calculate shift amount (mix between uniform and radial)
            let shift = amount * (1.0 - radial + radial * dist * 2.0);

            // edge fade: reduce effect near image boundaries to avoid stretching artifacts
            let dist_to_edge_x = uv_x.min(1.0 - uv_x);
            let dist_to_edge_y = uv_y.min(1.0 - uv_y);
            let dist_to_edge = dist_to_edge_x.min(dist_to_edge_y);

            let fade_threshold = shift * 1.5;
            let edge_fade = if dist_to_edge >= fade_threshold {
                1.0
            } else {
                dist_to_edge / fade_threshold
            };

            let adjusted_shift = shift * edge_fade;

            if dist > 0.0001 && adjusted_shift > 0.0 {
                let dir_x = center_x_norm / dist;
                let dir_y = center_y_norm / dist;

                let r_x = uv_x + dir_x * adjusted_shift;
                let r_y = uv_y + dir_y * adjusted_shift;
                let b_x = uv_x - dir_x * adjusted_shift;
                let b_y = uv_y - dir_y * adjusted_shift;

                let r_px = (r_x * width_f).clamp(0.0, width_f - 1.0) as u32;
                let r_py = (r_y * height_f).clamp(0.0, height_f - 1.0) as u32;
                let r_idx = ((r_py * width + r_px) * 4) as usize;

                let b_px = (b_x * width_f).clamp(0.0, width_f - 1.0) as u32;
                let b_py = (b_y * height_f).clamp(0.0, height_f - 1.0) as u32;
                let b_idx = ((b_py * width + b_px) * 4) as usize;

                pixel_bytes[0] = source_raw[r_idx]; // red from shifted position
                pixel_bytes[2] = source_raw[b_idx + 2]; // blue from shifted position
            } else {
                // no shift needed, copy from original position
                pixel_bytes[0] = source_raw[src_idx]; // red
                pixel_bytes[2] = source_raw[src_idx + 2]; // blue
            }
        }
    }
}

/// parses chromatic aberration amount from string.
pub fn parse_chromatic(value: &str) -> Option<f32> {
    parse_positive_f32(value).filter(|&v| v <= 0.1)
}
