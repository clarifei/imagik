use std::collections::HashSet;
use std::fmt::Write;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const LATENCY_BOUNDS_MS: [u64; 18] = [
    1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1_024, 2_048, 4_096, 8_192, 16_384, 32_768, 65_536,
    131_072,
];
const LATENCY_BUCKETS: usize = LATENCY_BOUNDS_MS.len() + 1;
const CAPACITY_BOUNDS_BYTES: [usize; 12] = [
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
const CAPACITY_BUCKETS: usize = CAPACITY_BOUNDS_BYTES.len() + 1;

#[derive(Clone, Copy)]
pub enum ScratchBuffer {
    Downscaled = 0,
    Working = 1,
    FullResult = 2,
}

impl ScratchBuffer {
    const COUNT: usize = 3;

    const ALL: [Self; Self::COUNT] = [Self::Downscaled, Self::Working, Self::FullResult];

    fn as_index(self) -> usize {
        self as usize
    }

    fn as_name(self) -> &'static str {
        match self {
            Self::Downscaled => "ca_downscaled",
            Self::Working => "ca_working",
            Self::FullResult => "ca_full_result",
        }
    }
}

pub struct TransformInFlightGuard;

impl TransformInFlightGuard {
    pub fn new() -> Self {
        let in_flight = TRANSFORM_IN_FLIGHT.fetch_add(1, Ordering::Relaxed) + 1;
        update_peak(&TRANSFORM_IN_FLIGHT_PEAK, in_flight);
        Self
    }
}

impl Drop for TransformInFlightGuard {
    fn drop(&mut self) {
        TRANSFORM_IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
    }
}

static REQUEST_TOTAL: AtomicU64 = AtomicU64::new(0);
static REQUEST_ERRORS: AtomicU64 = AtomicU64::new(0);
static BLOCKING_TASKS_TOTAL: AtomicU64 = AtomicU64::new(0);
static TRANSFORM_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
static TRANSFORM_IN_FLIGHT_PEAK: AtomicUsize = AtomicUsize::new(0);
static MAX_BLOCKING_THREADS_CONFIG: AtomicUsize = AtomicUsize::new(0);
static TRANSFORM_CONCURRENCY_CONFIG: AtomicUsize = AtomicUsize::new(0);
static BLOCKING_KEEP_ALIVE_MS_CONFIG: AtomicU64 = AtomicU64::new(0);
static RSS_SAMPLE_INTERVAL_MS_CONFIG: AtomicU64 = AtomicU64::new(0);

static REQUEST_LATENCY_MS_HIST: [AtomicU64; LATENCY_BUCKETS] =
    [const { AtomicU64::new(0) }; LATENCY_BUCKETS];
static QUEUE_WAIT_MS_HIST: [AtomicU64; LATENCY_BUCKETS] =
    [const { AtomicU64::new(0) }; LATENCY_BUCKETS];
static PROCESSING_MS_HIST: [AtomicU64; LATENCY_BUCKETS] =
    [const { AtomicU64::new(0) }; LATENCY_BUCKETS];

static RSS_BYTES: AtomicUsize = AtomicUsize::new(0);
static RSS_BYTES_PEAK: AtomicUsize = AtomicUsize::new(0);
static PROCESS_THREADS: AtomicUsize = AtomicUsize::new(0);
static PROCESS_THREADS_PEAK: AtomicUsize = AtomicUsize::new(0);

static BLOCKING_THREAD_IDS: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
static CAPACITY_HISTS: OnceLock<Vec<Vec<AtomicU64>>> = OnceLock::new();
static RSS_SAMPLER_STARTED: OnceLock<()> = OnceLock::new();

fn capacity_hists() -> &'static [Vec<AtomicU64>] {
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

fn blocking_threads() -> &'static Mutex<HashSet<u64>> {
    BLOCKING_THREAD_IDS.get_or_init(|| Mutex::new(HashSet::new()))
}

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

pub fn record_request_finished(duration: Duration, success: bool) {
    REQUEST_TOTAL.fetch_add(1, Ordering::Relaxed);
    if !success {
        REQUEST_ERRORS.fetch_add(1, Ordering::Relaxed);
    }
    record_latency_hist(&REQUEST_LATENCY_MS_HIST, duration);
}

pub fn record_queue_wait(duration: Duration) {
    record_latency_hist(&QUEUE_WAIT_MS_HIST, duration);
}

pub fn record_processing_latency(duration: Duration) {
    record_latency_hist(&PROCESSING_MS_HIST, duration);
}

pub fn record_blocking_task_started() {
    BLOCKING_TASKS_TOTAL.fetch_add(1, Ordering::Relaxed);
}

pub fn record_blocking_thread_usage() {
    let id = std::thread::current().id();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    let hashed = hasher.finish();

    if let Ok(mut set) = blocking_threads().lock() {
        set.insert(hashed);
    }
}

pub fn record_scratch_capacity(buffer: ScratchBuffer, bytes: usize) {
    let hists = capacity_hists();
    let idx = capacity_bucket_index(bytes);
    if let Some(hist) = hists.get(buffer.as_index())
        && let Some(bucket) = hist.get(idx)
    {
        bucket.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn render_prometheus() -> String {
    refresh_process_stats();

    let request_total = REQUEST_TOTAL.load(Ordering::Relaxed);
    let request_errors = REQUEST_ERRORS.load(Ordering::Relaxed);
    let blocking_total = BLOCKING_TASKS_TOTAL.load(Ordering::Relaxed);
    let in_flight = TRANSFORM_IN_FLIGHT.load(Ordering::Relaxed);
    let in_flight_peak = TRANSFORM_IN_FLIGHT_PEAK.load(Ordering::Relaxed);
    let blocking_threads = blocking_threads().lock().map_or(0, |set| set.len());
    let rss = RSS_BYTES.load(Ordering::Relaxed);
    let rss_peak = RSS_BYTES_PEAK.load(Ordering::Relaxed);
    let threads = PROCESS_THREADS.load(Ordering::Relaxed);
    let threads_peak = PROCESS_THREADS_PEAK.load(Ordering::Relaxed);

    let mut output = String::new();
    let _ = writeln!(
        output,
        "imagik_config_max_blocking_threads {}",
        MAX_BLOCKING_THREADS_CONFIG.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        output,
        "imagik_config_transform_concurrency {}",
        TRANSFORM_CONCURRENCY_CONFIG.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        output,
        "imagik_config_blocking_keep_alive_ms {}",
        BLOCKING_KEEP_ALIVE_MS_CONFIG.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        output,
        "imagik_config_rss_sample_interval_ms {}",
        RSS_SAMPLE_INTERVAL_MS_CONFIG.load(Ordering::Relaxed)
    );
    let _ = writeln!(output, "imagik_requests_total {}", request_total);
    let _ = writeln!(output, "imagik_requests_errors_total {}", request_errors);
    let _ = writeln!(output, "imagik_blocking_tasks_total {}", blocking_total);
    let _ = writeln!(output, "imagik_transform_in_flight {}", in_flight);
    let _ = writeln!(output, "imagik_transform_in_flight_peak {}", in_flight_peak);
    let _ = writeln!(
        output,
        "imagik_blocking_threads_unique {}",
        blocking_threads
    );
    let _ = writeln!(output, "imagik_process_rss_bytes {}", rss);
    let _ = writeln!(output, "imagik_process_rss_peak_bytes {}", rss_peak);
    let _ = writeln!(output, "imagik_process_threads {}", threads);
    let _ = writeln!(output, "imagik_process_threads_peak {}", threads_peak);

    append_latency_percentiles(&mut output, "request", &REQUEST_LATENCY_MS_HIST);
    append_latency_percentiles(&mut output, "queue_wait", &QUEUE_WAIT_MS_HIST);
    append_latency_percentiles(&mut output, "processing", &PROCESSING_MS_HIST);
    append_capacity_histograms(&mut output);

    output
}

fn append_latency_percentiles(out: &mut String, kind: &str, hist: &[AtomicU64; LATENCY_BUCKETS]) {
    let p50 = percentile_from_hist(hist, 0.50);
    let p95 = percentile_from_hist(hist, 0.95);
    let p99 = percentile_from_hist(hist, 0.99);
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

fn append_capacity_histograms(out: &mut String) {
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

fn record_latency_hist(hist: &[AtomicU64; LATENCY_BUCKETS], duration: Duration) {
    let ms = duration.as_millis() as u64;
    let idx = latency_bucket_index(ms);
    hist[idx].fetch_add(1, Ordering::Relaxed);
}

fn latency_bucket_index(ms: u64) -> usize {
    LATENCY_BOUNDS_MS.partition_point(|bound| ms > *bound)
}

fn capacity_bucket_index(bytes: usize) -> usize {
    CAPACITY_BOUNDS_BYTES.partition_point(|bound| bytes > *bound)
}

fn capacity_bucket_upper_bound(idx: usize) -> usize {
    CAPACITY_BOUNDS_BYTES
        .get(idx)
        .copied()
        .unwrap_or(usize::MAX)
}

fn percentile_from_hist(hist: &[AtomicU64; LATENCY_BUCKETS], quantile: f64) -> u64 {
    let total = hist
        .iter()
        .map(|bucket| bucket.load(Ordering::Relaxed))
        .sum::<u64>();
    if total == 0 {
        return 0;
    }

    let rank = (total as f64 * quantile).ceil() as u64;
    let mut cumulative = 0u64;
    for (idx, bucket) in hist.iter().enumerate() {
        cumulative += bucket.load(Ordering::Relaxed);
        if cumulative >= rank {
            return latency_bucket_upper_bound(idx);
        }
    }

    latency_bucket_upper_bound(LATENCY_BUCKETS - 1)
}

fn latency_bucket_upper_bound(idx: usize) -> u64 {
    LATENCY_BOUNDS_MS
        .get(idx)
        .copied()
        .unwrap_or(LATENCY_BOUNDS_MS[LATENCY_BOUNDS_MS.len() - 1] * 2)
}

fn refresh_process_stats() {
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

fn update_peak(peak: &AtomicUsize, value: usize) {
    let mut current = peak.load(Ordering::Relaxed);
    while value > current {
        match peak.compare_exchange(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
}
