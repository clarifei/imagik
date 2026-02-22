//! atomic metric storage and lazy-initialized histograms.
//!
//! counters are static atomics for lock-free updates.
//! histograms use `OnceLock` for lazy allocation to avoid startup overhead.

use super::labels::{ExternalCacheLayer, PipelineStage, ScratchBuffer};
use super::latency::{CAPACITY_BUCKETS, LATENCY_BUCKETS};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;

pub(super) static REQUEST_TOTAL: AtomicU64 = AtomicU64::new(0);
pub(super) static REQUEST_ERRORS: AtomicU64 = AtomicU64::new(0);
pub(super) static BLOCKING_TASKS_TOTAL: AtomicU64 = AtomicU64::new(0);
pub(super) static TRANSFORM_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
pub(super) static TRANSFORM_IN_FLIGHT_PEAK: AtomicUsize = AtomicUsize::new(0);
pub(super) static MAX_BLOCKING_THREADS_CONFIG: AtomicUsize = AtomicUsize::new(0);
pub(super) static TRANSFORM_CONCURRENCY_CONFIG: AtomicUsize = AtomicUsize::new(0);
pub(super) static BLOCKING_KEEP_ALIVE_MS_CONFIG: AtomicU64 = AtomicU64::new(0);
pub(super) static RSS_SAMPLE_INTERVAL_MS_CONFIG: AtomicU64 = AtomicU64::new(0);

pub(super) static REQUEST_LATENCY_MS_HIST: [AtomicU64; LATENCY_BUCKETS] =
    [const { AtomicU64::new(0) }; LATENCY_BUCKETS];
pub(super) static QUEUE_WAIT_MS_HIST: [AtomicU64; LATENCY_BUCKETS] =
    [const { AtomicU64::new(0) }; LATENCY_BUCKETS];
pub(super) static PROCESSING_MS_HIST: [AtomicU64; LATENCY_BUCKETS] =
    [const { AtomicU64::new(0) }; LATENCY_BUCKETS];

pub(super) static RSS_BYTES: AtomicUsize = AtomicUsize::new(0);
pub(super) static RSS_BYTES_PEAK: AtomicUsize = AtomicUsize::new(0);
pub(super) static PROCESS_THREADS: AtomicUsize = AtomicUsize::new(0);
pub(super) static PROCESS_THREADS_PEAK: AtomicUsize = AtomicUsize::new(0);
pub(super) static BLOCKING_THREADS_UNIQUE: AtomicUsize = AtomicUsize::new(0);

pub(super) static EXTERNAL_CACHE_ENABLED: AtomicUsize = AtomicUsize::new(0);
pub(super) static EXTERNAL_CACHE_MEMORY_BYTES: AtomicUsize = AtomicUsize::new(0);
pub(super) static EXTERNAL_CACHE_EVICTED_KEYS: AtomicU64 = AtomicU64::new(0);

pub(super) static CACHE_HITS: [AtomicU64; ExternalCacheLayer::COUNT] =
    [const { AtomicU64::new(0) }; ExternalCacheLayer::COUNT];
pub(super) static CACHE_MISSES: [AtomicU64; ExternalCacheLayer::COUNT] =
    [const { AtomicU64::new(0) }; ExternalCacheLayer::COUNT];
pub(super) static CACHE_ERRORS: [AtomicU64; ExternalCacheLayer::COUNT] =
    [const { AtomicU64::new(0) }; ExternalCacheLayer::COUNT];
pub(super) static CACHE_STORES: [AtomicU64; ExternalCacheLayer::COUNT] =
    [const { AtomicU64::new(0) }; ExternalCacheLayer::COUNT];
pub(super) static CACHE_SKIPS_TOO_LARGE: [AtomicU64; ExternalCacheLayer::COUNT] =
    [const { AtomicU64::new(0) }; ExternalCacheLayer::COUNT];
pub(super) static CACHE_STORE_BYTES_TOTAL: [AtomicU64; ExternalCacheLayer::COUNT] =
    [const { AtomicU64::new(0) }; ExternalCacheLayer::COUNT];
pub(super) static CACHE_EVICTIONS: [AtomicU64; ExternalCacheLayer::COUNT] =
    [const { AtomicU64::new(0) }; ExternalCacheLayer::COUNT];

pub(super) static CACHE_LOCK_ACQUIRED_TOTAL: AtomicU64 = AtomicU64::new(0);
pub(super) static CACHE_LOCK_CONTENDED_TOTAL: AtomicU64 = AtomicU64::new(0);
pub(super) static CACHE_LOCK_TIMEOUT_TOTAL: AtomicU64 = AtomicU64::new(0);
pub(super) static CACHE_LOCK_ERRORS_TOTAL: AtomicU64 = AtomicU64::new(0);
pub(super) static CACHE_LOCK_RELEASE_ERRORS_TOTAL: AtomicU64 = AtomicU64::new(0);
pub(super) static STORAGE_RETRIES_TOTAL: AtomicU64 = AtomicU64::new(0);
pub(super) static UNKNOWN_VERSION_TOTAL: AtomicU64 = AtomicU64::new(0);

pub(super) static CACHE_LOCK_WAIT_MS_HIST: [AtomicU64; LATENCY_BUCKETS] =
    [const { AtomicU64::new(0) }; LATENCY_BUCKETS];

static CACHE_GET_HISTS: OnceLock<Vec<Vec<AtomicU64>>> = OnceLock::new();
static CACHE_SET_HISTS: OnceLock<Vec<Vec<AtomicU64>>> = OnceLock::new();
static STAGE_LATENCY_HISTS: OnceLock<Vec<Vec<AtomicU64>>> = OnceLock::new();

static CAPACITY_HISTS: OnceLock<Vec<Vec<AtomicU64>>> = OnceLock::new();
pub(super) static RSS_SAMPLER_STARTED: OnceLock<()> = OnceLock::new();

pub(super) fn cache_get_hists() -> &'static [Vec<AtomicU64>] {
    CACHE_GET_HISTS.get_or_init(|| {
        (0..ExternalCacheLayer::COUNT)
            .map(|_| {
                (0..LATENCY_BUCKETS)
                    .map(|_| AtomicU64::new(0))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    })
}

pub(super) fn cache_set_hists() -> &'static [Vec<AtomicU64>] {
    CACHE_SET_HISTS.get_or_init(|| {
        (0..ExternalCacheLayer::COUNT)
            .map(|_| {
                (0..LATENCY_BUCKETS)
                    .map(|_| AtomicU64::new(0))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    })
}

pub(super) fn stage_latency_hists() -> &'static [Vec<AtomicU64>] {
    STAGE_LATENCY_HISTS.get_or_init(|| {
        (0..PipelineStage::COUNT)
            .map(|_| {
                (0..LATENCY_BUCKETS)
                    .map(|_| AtomicU64::new(0))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    })
}

pub(super) fn capacity_hists() -> &'static [Vec<AtomicU64>] {
    CAPACITY_HISTS.get_or_init(|| {
        (0..ScratchBuffer::COUNT)
            .map(|_| {
                (0..CAPACITY_BUCKETS)
                    .map(|_| AtomicU64::new(0))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    })
}

#[inline]
pub(super) fn update_peak(peak: &AtomicUsize, value: usize) {
    let mut current = peak.load(Ordering::Relaxed);
    while value > current {
        match peak.compare_exchange(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
}
