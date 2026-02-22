//! tokio runtime and transform concurrency configuration.
//!
//! configures blocking thread pool and transform semaphore limits.

use crate::config::env::{parse_u64, parse_usize};
use std::sync::OnceLock;

const DEFAULT_TRANSFORM_CONCURRENCY_MULTIPLIER: usize = 2;
const MIN_TRANSFORM_CONCURRENCY: usize = 1;

/// tokio runtime configuration.
pub struct RuntimeConfig {
    /// maximum blocking threads in the runtime.
    pub max_blocking_threads: usize,
    /// milliseconds to keep idle blocking threads alive.
    pub blocking_keep_alive_ms: u64,
    /// milliseconds between RSS sampling.
    pub rss_sample_interval_ms: u64,
}

impl RuntimeConfig {
    /// parses runtime configuration from environment.
    pub fn from_env() -> Self {
        let cpu = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1);
        let max_blocking_threads = parse_usize(
            "IMAGIK_MAX_BLOCKING_THREADS",
            cpu.saturating_mul(2).max(1),
            1,
        );
        let blocking_keep_alive_ms = parse_u64("IMAGIK_BLOCKING_KEEP_ALIVE_MS", 3_000, 250);
        let rss_sample_interval_ms = parse_u64("IMAGIK_RSS_SAMPLE_INTERVAL_MS", 1_000, 100);

        Self {
            max_blocking_threads,
            blocking_keep_alive_ms,
            rss_sample_interval_ms,
        }
    }
}

/// returns the configured transform concurrency limit.
///
/// calculated from CPU count by default, override with `IMAGIK_TRANSFORM_CONCURRENCY`.
pub fn configured_transform_concurrency() -> usize {
    static TRANSFORM_CONCURRENCY: OnceLock<usize> = OnceLock::new();
    *TRANSFORM_CONCURRENCY.get_or_init(|| {
        let cpu = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1);
        let default_limit = cpu.saturating_mul(DEFAULT_TRANSFORM_CONCURRENCY_MULTIPLIER);
        std::env::var("IMAGIK_TRANSFORM_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v >= MIN_TRANSFORM_CONCURRENCY)
            .unwrap_or_else(|| default_limit.max(MIN_TRANSFORM_CONCURRENCY))
    })
}
