//! histogram bucket definitions and percentile calculations.
//!
//! latency buckets cover 1ms to ~131s with exponential spacing.
//! capacity buckets cover 64KB to 128MB for scratch buffer tracking.

use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub(super) const LATENCY_BOUNDS_MS: [u64; 18] = [
    1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1_024, 2_048, 4_096, 8_192, 16_384, 32_768, 65_536,
    131_072,
];
pub(super) const LATENCY_BUCKETS: usize = LATENCY_BOUNDS_MS.len() + 1;

pub(super) const CAPACITY_BOUNDS_BYTES: [usize; 12] = [
    64 * 1024,
    128 * 1024,
    256 * 1024,
    512 * 1024,
    1024 * 1024,
    2 * 1024 * 1024,
    4 * 1024 * 1024,
    8 * 1024 * 1024,
    16 * 1024 * 1024,
    32 * 1024 * 1024,
    64 * 1024 * 1024,
    128 * 1024 * 1024,
];
pub(super) const CAPACITY_BUCKETS: usize = CAPACITY_BOUNDS_BYTES.len() + 1;

#[inline]
pub(super) fn record_latency_hist(hist: &[AtomicU64; LATENCY_BUCKETS], duration: Duration) {
    let ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    let idx = latency_bucket_index(ms);
    hist[idx].fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub(super) fn record_hist_slice(hist: &[AtomicU64], duration: Duration) {
    let millis = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    let idx = latency_bucket_index(millis);
    if let Some(bucket) = hist.get(idx) {
        bucket.fetch_add(1, Ordering::Relaxed);
    }
}

#[inline]
pub(super) fn latency_bucket_index(ms: u64) -> usize {
    LATENCY_BOUNDS_MS.partition_point(|bound| ms > *bound)
}

#[inline]
pub(super) fn capacity_bucket_index(bytes: usize) -> usize {
    CAPACITY_BOUNDS_BYTES.partition_point(|bound| bytes > *bound)
}

#[inline]
pub(super) fn capacity_bucket_upper_bound(idx: usize) -> usize {
    CAPACITY_BOUNDS_BYTES
        .get(idx)
        .copied()
        .unwrap_or(usize::MAX)
}

pub(super) fn append_latency_percentiles(
    out: &mut String,
    kind: &str,
    hist: &[AtomicU64; LATENCY_BUCKETS],
) {
    let p50 = percentile_from_hist(hist, 50, 100);
    let p95 = percentile_from_hist(hist, 95, 100);
    let p99 = percentile_from_hist(hist, 99, 100);
    let _ = writeln!(
        out,
        "imagik_latency_ms{{kind=\"{kind}\",quantile=\"0.50\"}} {p50}"
    );
    let _ = writeln!(
        out,
        "imagik_latency_ms{{kind=\"{kind}\",quantile=\"0.95\"}} {p95}"
    );
    let _ = writeln!(
        out,
        "imagik_latency_ms{{kind=\"{kind}\",quantile=\"0.99\"}} {p99}"
    );
}

pub(super) fn percentile_from_hist(
    hist: &[AtomicU64; LATENCY_BUCKETS],
    numerator: u64,
    denominator: u64,
) -> u64 {
    let total = hist
        .iter()
        .map(|bucket| bucket.load(Ordering::Relaxed))
        .sum::<u64>();
    if total == 0 {
        return 0;
    }

    let rank = percentile_rank(total, numerator, denominator);
    let mut cumulative = 0u64;
    for (idx, bucket) in hist.iter().enumerate() {
        cumulative += bucket.load(Ordering::Relaxed);
        if cumulative >= rank {
            return latency_bucket_upper_bound(idx);
        }
    }

    latency_bucket_upper_bound(LATENCY_BUCKETS - 1)
}

pub(super) fn percentile_from_hist_slice(
    hist: &[AtomicU64],
    numerator: u64,
    denominator: u64,
) -> u64 {
    let total = hist
        .iter()
        .map(|bucket| bucket.load(Ordering::Relaxed))
        .sum::<u64>();
    if total == 0 {
        return 0;
    }

    let rank = percentile_rank(total, numerator, denominator);
    let mut cumulative = 0u64;
    for (idx, bucket) in hist.iter().enumerate() {
        cumulative += bucket.load(Ordering::Relaxed);
        if cumulative >= rank {
            return latency_bucket_upper_bound(idx);
        }
    }

    latency_bucket_upper_bound(hist.len().saturating_sub(1))
}

#[inline]
fn latency_bucket_upper_bound(idx: usize) -> u64 {
    LATENCY_BOUNDS_MS
        .get(idx)
        .copied()
        .unwrap_or(LATENCY_BOUNDS_MS[LATENCY_BOUNDS_MS.len() - 1] * 2)
}

#[inline]
fn percentile_rank(total: u64, numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        return total;
    }

    let scaled = total.saturating_mul(numerator);
    let rank = (scaled.saturating_add(denominator - 1)) / denominator;
    rank.clamp(1, total)
}
