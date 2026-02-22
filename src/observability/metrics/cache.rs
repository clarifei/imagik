use super::labels::ExternalCacheLayer;
use super::latency::{append_latency_percentiles, percentile_from_hist_slice, record_hist_slice};
use super::registry::{
    cache_get_hists, cache_set_hists, CACHE_ERRORS, CACHE_EVICTIONS, CACHE_HITS,
    CACHE_LOCK_ACQUIRED_TOTAL, CACHE_LOCK_CONTENDED_TOTAL, CACHE_LOCK_ERRORS_TOTAL,
    CACHE_LOCK_RELEASE_ERRORS_TOTAL, CACHE_LOCK_TIMEOUT_TOTAL, CACHE_LOCK_WAIT_MS_HIST,
    CACHE_MISSES, CACHE_SKIPS_TOO_LARGE, CACHE_STORES, CACHE_STORE_BYTES_TOTAL,
    EXTERNAL_CACHE_ENABLED, EXTERNAL_CACHE_EVICTED_KEYS, EXTERNAL_CACHE_MEMORY_BYTES,
    STORAGE_RETRIES_TOTAL, UNKNOWN_VERSION_TOTAL,
};
use std::fmt::Write;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// sets whether external cache (redis/dragonfly) is connected.
pub fn set_external_cache_enabled(enabled: bool) {
    EXTERNAL_CACHE_ENABLED.store(usize::from(enabled), Ordering::Relaxed);
}

/// updates external cache memory and eviction stats from info sampler.
pub fn set_external_cache_stats(memory_bytes: usize, evicted_keys: u64) {
    EXTERNAL_CACHE_MEMORY_BYTES.store(memory_bytes, Ordering::Relaxed);
    EXTERNAL_CACHE_EVICTED_KEYS.store(evicted_keys, Ordering::Relaxed);
}

/// records a cache hit for the specified layer.
///
/// increment is atomic with `Ordering::Relaxed` for minimal overhead.
/// counter indexed by `layer.as_index()` for cache-friendly access.
pub fn record_cache_hit(layer: ExternalCacheLayer) {
    CACHE_HITS[layer.as_index()].fetch_add(1, Ordering::Relaxed);
}

/// records a cache miss for the specified layer.
///
/// paired with `record_cache_hit` to calculate hit ratio.
/// counters are separate to avoid contention on single atomic.
pub fn record_cache_miss(layer: ExternalCacheLayer) {
    CACHE_MISSES[layer.as_index()].fetch_add(1, Ordering::Relaxed);
}

/// records a cache operation error for the specified layer.
pub fn record_cache_error(layer: ExternalCacheLayer) {
    CACHE_ERRORS[layer.as_index()].fetch_add(1, Ordering::Relaxed);
}

/// records a cache store operation with byte count.
pub fn record_cache_store(layer: ExternalCacheLayer, bytes: usize) {
    CACHE_STORES[layer.as_index()].fetch_add(1, Ordering::Relaxed);
    CACHE_STORE_BYTES_TOTAL[layer.as_index()].fetch_add(bytes as u64, Ordering::Relaxed);
}

/// records cache evictions for the specified layer.
pub fn record_cache_evictions(layer: ExternalCacheLayer, count: u64) {
    CACHE_EVICTIONS[layer.as_index()].fetch_add(count, Ordering::Relaxed);
}

/// records that a cache entry was skipped due to size limits.
pub fn record_cache_skip_too_large(layer: ExternalCacheLayer, _bytes: usize) {
    CACHE_SKIPS_TOO_LARGE[layer.as_index()].fetch_add(1, Ordering::Relaxed);
}

/// records get operation latency for the specified layer.
pub fn record_cache_get_latency(layer: ExternalCacheLayer, duration: Duration) {
    if let Some(hist) = cache_get_hists().get(layer.as_index()) {
        record_hist_slice(hist, duration);
    }
}

/// records set operation latency for the specified layer.
pub fn record_cache_set_latency(layer: ExternalCacheLayer, duration: Duration) {
    if let Some(hist) = cache_set_hists().get(layer.as_index()) {
        record_hist_slice(hist, duration);
    }
}

/// records successful lock acquisition with wait time.
///
/// tracks both success count and wait time distribution.
/// wait times recorded in milliseconds for histogram bucketing.
pub fn record_cache_lock_acquired(wait: Duration) {
    CACHE_LOCK_ACQUIRED_TOTAL.fetch_add(1, Ordering::Relaxed);
    record_hist_slice(&CACHE_LOCK_WAIT_MS_HIST, wait);
}

/// records that a lock acquisition attempt found the lock held.
///
/// indicates concurrent requests for same cache key.
/// high contention suggests need for longer lock ttl or request coalescing.
pub fn record_cache_lock_contended() {
    CACHE_LOCK_CONTENDED_TOTAL.fetch_add(1, Ordering::Relaxed);
}

/// records a lock acquisition timeout with wait time.
pub fn record_cache_lock_timeout(wait: Duration) {
    CACHE_LOCK_TIMEOUT_TOTAL.fetch_add(1, Ordering::Relaxed);
    record_hist_slice(&CACHE_LOCK_WAIT_MS_HIST, wait);
}

/// records lock wait time without acquisition result.
pub fn record_cache_lock_wait(wait: Duration) {
    record_hist_slice(&CACHE_LOCK_WAIT_MS_HIST, wait);
}

/// records a lock operation error.
pub fn record_cache_lock_error() {
    CACHE_LOCK_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
}

/// records a lock release error.
pub fn record_cache_lock_release_error() {
    CACHE_LOCK_RELEASE_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
}

/// records an object storage retry.
pub fn record_storage_retry() {
    STORAGE_RETRIES_TOTAL.fetch_add(1, Ordering::Relaxed);
}

/// records that a version lookup returned unknown.
pub fn record_unknown_version() {
    UNKNOWN_VERSION_TOTAL.fetch_add(1, Ordering::Relaxed);
}

pub(super) fn append_cache_summary_metrics(out: &mut String) {
    let _ = writeln!(
        out,
        "imagik_external_cache_enabled {}",
        EXTERNAL_CACHE_ENABLED.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_external_cache_memory_bytes {}",
        EXTERNAL_CACHE_MEMORY_BYTES.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_external_cache_evicted_keys_total {}",
        EXTERNAL_CACHE_EVICTED_KEYS.load(Ordering::Relaxed)
    );
}

pub(super) fn append_cache_lock_latency_metrics(out: &mut String) {
    append_latency_percentiles(out, "cache_lock_wait", &CACHE_LOCK_WAIT_MS_HIST);
}

pub(super) fn append_cache_metrics(out: &mut String) {
    for layer in ExternalCacheLayer::ALL {
        let idx = layer.as_index();
        let _ = writeln!(
            out,
            "imagik_cache_ops_total{{layer=\"{}\",op=\"get\",result=\"hit\"}} {}",
            layer.as_name(),
            CACHE_HITS[idx].load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "imagik_cache_ops_total{{layer=\"{}\",op=\"get\",result=\"miss\"}} {}",
            layer.as_name(),
            CACHE_MISSES[idx].load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "imagik_cache_ops_total{{layer=\"{}\",op=\"set\",result=\"stored\"}} {}",
            layer.as_name(),
            CACHE_STORES[idx].load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "imagik_cache_ops_total{{layer=\"{}\",op=\"set\",result=\"skipped_too_large\"}} {}",
            layer.as_name(),
            CACHE_SKIPS_TOO_LARGE[idx].load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "imagik_cache_errors_total{{layer=\"{}\"}} {}",
            layer.as_name(),
            CACHE_ERRORS[idx].load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "imagik_cache_store_bytes_total{{layer=\"{}\"}} {}",
            layer.as_name(),
            CACHE_STORE_BYTES_TOTAL[idx].load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "imagik_cache_evictions_total{{layer=\"{}\"}} {}",
            layer.as_name(),
            CACHE_EVICTIONS[idx].load(Ordering::Relaxed)
        );

        append_cache_latency_percentiles(out, layer, cache_get_hists(), "get");
        append_cache_latency_percentiles(out, layer, cache_set_hists(), "set");
    }

    let _ = writeln!(
        out,
        "imagik_cache_lock_total{{result=\"acquired\"}} {}",
        CACHE_LOCK_ACQUIRED_TOTAL.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_cache_lock_total{{result=\"contended\"}} {}",
        CACHE_LOCK_CONTENDED_TOTAL.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_cache_lock_total{{result=\"timeout\"}} {}",
        CACHE_LOCK_TIMEOUT_TOTAL.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_cache_lock_total{{result=\"error\"}} {}",
        CACHE_LOCK_ERRORS_TOTAL.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_cache_lock_total{{result=\"release_error\"}} {}",
        CACHE_LOCK_RELEASE_ERRORS_TOTAL.load(Ordering::Relaxed)
    );
}

pub(super) fn append_storage_metrics(out: &mut String) {
    let _ = writeln!(
        out,
        "imagik_storage_retries_total {}",
        STORAGE_RETRIES_TOTAL.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_unknown_version_total {}",
        UNKNOWN_VERSION_TOTAL.load(Ordering::Relaxed)
    );
}

fn append_cache_latency_percentiles(
    out: &mut String,
    layer: ExternalCacheLayer,
    hists: &[Vec<std::sync::atomic::AtomicU64>],
    op: &str,
) {
    let Some(hist) = hists.get(layer.as_index()) else {
        return;
    };
    let p50 = percentile_from_hist_slice(hist, 50, 100);
    let p95 = percentile_from_hist_slice(hist, 95, 100);
    let p99 = percentile_from_hist_slice(hist, 99, 100);

    let _ = writeln!(
        out,
        "imagik_cache_latency_ms{{layer=\"{}\",op=\"{}\",quantile=\"0.50\"}} {}",
        layer.as_name(),
        op,
        p50
    );
    let _ = writeln!(
        out,
        "imagik_cache_latency_ms{{layer=\"{}\",op=\"{}\",quantile=\"0.95\"}} {}",
        layer.as_name(),
        op,
        p95
    );
    let _ = writeln!(
        out,
        "imagik_cache_latency_ms{{layer=\"{}\",op=\"{}\",quantile=\"0.99\"}} {}",
        layer.as_name(),
        op,
        p99
    );
}
