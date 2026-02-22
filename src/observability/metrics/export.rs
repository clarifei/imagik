//! prometheus metrics export in text format.
//!
//! renders all metrics as prometheus exposition format.
//! refreshes process stats (RSS, threads) before rendering.

use super::cache::{
    append_cache_lock_latency_metrics, append_cache_metrics, append_cache_summary_metrics,
    append_storage_metrics,
};
use super::pipeline::append_stage_latency_metrics;
use super::registry::{
    BLOCKING_TASKS_TOTAL, REQUEST_ERRORS, REQUEST_TOTAL, TRANSFORM_IN_FLIGHT,
    TRANSFORM_IN_FLIGHT_PEAK,
};
use super::request::append_request_latency_metrics;
use super::system::{
    append_capacity_metrics, append_process_metrics, append_runtime_config_metrics,
    refresh_process_stats,
};
use std::fmt::Write;
use std::sync::atomic::Ordering;

/// renders all metrics as prometheus text format.
pub fn render_prometheus() -> String {
    refresh_process_stats();

    let request_total = REQUEST_TOTAL.load(Ordering::Relaxed);
    let request_errors = REQUEST_ERRORS.load(Ordering::Relaxed);
    let blocking_total = BLOCKING_TASKS_TOTAL.load(Ordering::Relaxed);
    let in_flight = TRANSFORM_IN_FLIGHT.load(Ordering::Relaxed);
    let in_flight_peak = TRANSFORM_IN_FLIGHT_PEAK.load(Ordering::Relaxed);

    let mut output = String::new();

    append_runtime_config_metrics(&mut output);

    let _ = writeln!(output, "imagik_requests_total {request_total}");
    let _ = writeln!(output, "imagik_requests_errors_total {request_errors}");
    let _ = writeln!(output, "imagik_blocking_tasks_total {blocking_total}");
    let _ = writeln!(output, "imagik_transform_in_flight {in_flight}");
    let _ = writeln!(output, "imagik_transform_in_flight_peak {in_flight_peak}");
    append_process_metrics(&mut output);

    append_cache_summary_metrics(&mut output);
    append_request_latency_metrics(&mut output);
    append_cache_lock_latency_metrics(&mut output);
    append_stage_latency_metrics(&mut output);
    append_cache_metrics(&mut output);
    append_capacity_metrics(&mut output);
    append_storage_metrics(&mut output);

    output
}
