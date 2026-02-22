//! metrics collection and prometheus export.
//!
//! design: lock-free atomic counters and histograms for minimal overhead.
//! all metrics use `Ordering::Relaxed` for maximum performance on hot paths.
//!
//! module responsibilities:
//! - `labels`: metric label enums (`ExternalCacheLayer`, `PipelineStage`, etc.)
//! - `registry`: atomic counter/histogram storage with lazy initialization
//! - `latency`: histogram bucket math and percentile calculation
//! - `request`: request-level metrics (latency, queue wait, processing time)
//! - `cache`: cache hit/miss/store metrics by layer
//! - `pipeline`: per-stage latency tracking (fetch, decode, transform, encode)
//! - `system`: runtime metrics (RSS, threads, scratch buffer capacities)
//! - `export`: prometheus text format rendering

mod cache;
mod export;
mod labels;
mod latency;
mod pipeline;
mod registry;
mod request;
mod system;

pub use cache::{
    record_cache_error, record_cache_evictions, record_cache_get_latency, record_cache_hit,
    record_cache_lock_acquired, record_cache_lock_contended, record_cache_lock_error,
    record_cache_lock_release_error, record_cache_lock_timeout, record_cache_lock_wait,
    record_cache_miss, record_cache_set_latency, record_cache_skip_too_large, record_cache_store,
    record_storage_retry, record_unknown_version, set_external_cache_enabled,
    set_external_cache_stats,
};
pub use export::render_prometheus;
pub use labels::{ExternalCacheLayer, PipelineStage, ScratchBuffer};
pub use pipeline::record_stage_latency;
pub use request::{
    TransformInFlightGuard, record_blocking_task_started, record_processing_latency,
    record_queue_wait, record_request_finished,
};
pub use system::{
    record_blocking_thread_usage, record_scratch_capacity, set_runtime_limits, start_rss_sampler,
};
