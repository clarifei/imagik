use super::labels::ScratchBuffer;
use super::latency::{capacity_bucket_index, capacity_bucket_upper_bound};
use super::registry::{
    BLOCKING_KEEP_ALIVE_MS_CONFIG, BLOCKING_THREADS_UNIQUE, MAX_BLOCKING_THREADS_CONFIG,
    PROCESS_THREADS, PROCESS_THREADS_PEAK, RSS_BYTES, RSS_BYTES_PEAK,
    RSS_SAMPLE_INTERVAL_MS_CONFIG, RSS_SAMPLER_STARTED, TRANSFORM_CONCURRENCY_CONFIG,
    capacity_hists, update_peak,
};
use std::cell::Cell;
use std::fmt::Write;
use std::sync::atomic::Ordering;
use std::time::Duration;

thread_local! {
    static BLOCKING_THREAD_SEEN: Cell<bool> = const { Cell::new(false) };
}

/// sets runtime configuration values for metrics exposure.
pub fn set_runtime_limits(
    max_blocking_threads: usize,
    transform_concurrency: usize,
    blocking_keep_alive_ms: u64,
    rss_sample_interval_ms: u64,
) {
    MAX_BLOCKING_THREADS_CONFIG.store(max_blocking_threads, Ordering::Relaxed);
    TRANSFORM_CONCURRENCY_CONFIG.store(transform_concurrency, Ordering::Relaxed);
    BLOCKING_KEEP_ALIVE_MS_CONFIG.store(blocking_keep_alive_ms, Ordering::Relaxed);
    RSS_SAMPLE_INTERVAL_MS_CONFIG.store(rss_sample_interval_ms, Ordering::Relaxed);
}

/// starts the RSS sampler background task.
///
/// samples `/proc/self/status` at the specified interval.
/// idempotent: only starts once even if called multiple times.
pub fn start_rss_sampler(interval: Duration) {
    if RSS_SAMPLER_STARTED.set(()).is_err() {
        return;
    }

    tokio::spawn(async move {
        loop {
            refresh_process_stats();
            tokio::time::sleep(interval).await;
        }
    });
}

/// records that the current thread is a blocking thread.
///
/// thread-local deduplication ensures each thread counted once.
pub fn record_blocking_thread_usage() {
    BLOCKING_THREAD_SEEN.with(|seen| {
        if !seen.get() {
            seen.set(true);
            BLOCKING_THREADS_UNIQUE.fetch_add(1, Ordering::Relaxed);
        }
    });
}

/// records scratch buffer capacity for memory tracking.
pub fn record_scratch_capacity(buffer: ScratchBuffer, bytes: usize) {
    let hists = capacity_hists();
    let idx = capacity_bucket_index(bytes);
    if let Some(hist) = hists.get(buffer.as_index())
        && let Some(bucket) = hist.get(idx)
    {
        bucket.fetch_add(1, Ordering::Relaxed);
    }
}

pub(super) fn refresh_process_stats() {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        let mut rss_bytes = None;
        let mut thread_count = None;

        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                let kb = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                rss_bytes = Some(kb.saturating_mul(1024));
            } else if let Some(rest) = line.strip_prefix("Threads:") {
                thread_count = rest.trim().parse::<usize>().ok();
            }
        }

        if let Some(rss) = rss_bytes {
            RSS_BYTES.store(rss, Ordering::Relaxed);
            update_peak(&RSS_BYTES_PEAK, rss);
        }
        if let Some(threads) = thread_count {
            PROCESS_THREADS.store(threads, Ordering::Relaxed);
            update_peak(&PROCESS_THREADS_PEAK, threads);
        }
    }
}

pub(super) fn append_runtime_config_metrics(out: &mut String) {
    let _ = writeln!(
        out,
        "imagik_config_max_blocking_threads {}",
        MAX_BLOCKING_THREADS_CONFIG.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_config_transform_concurrency {}",
        TRANSFORM_CONCURRENCY_CONFIG.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_config_blocking_keep_alive_ms {}",
        BLOCKING_KEEP_ALIVE_MS_CONFIG.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_config_rss_sample_interval_ms {}",
        RSS_SAMPLE_INTERVAL_MS_CONFIG.load(Ordering::Relaxed)
    );
}

pub(super) fn append_process_metrics(out: &mut String) {
    let _ = writeln!(
        out,
        "imagik_blocking_threads_unique {}",
        BLOCKING_THREADS_UNIQUE.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_process_rss_bytes {}",
        RSS_BYTES.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_process_rss_peak_bytes {}",
        RSS_BYTES_PEAK.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_process_threads {}",
        PROCESS_THREADS.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "imagik_process_threads_peak {}",
        PROCESS_THREADS_PEAK.load(Ordering::Relaxed)
    );
}

pub(super) fn append_capacity_metrics(out: &mut String) {
    for buffer in ScratchBuffer::ALL {
        let hist = &capacity_hists()[buffer.as_index()];
        for (idx, bucket) in hist.iter().enumerate() {
            let count = bucket.load(Ordering::Relaxed);
            let upper = capacity_bucket_upper_bound(idx);
            let _ = writeln!(
                out,
                "imagik_scratch_capacity_samples_total{{buffer=\"{}\",le=\"{}\"}} {}",
                buffer.as_name(),
                upper,
                count
            );
        }
    }
}
