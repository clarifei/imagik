use super::latency::{append_latency_percentiles, record_latency_hist};
use super::registry::{
    update_peak, BLOCKING_TASKS_TOTAL, PROCESSING_MS_HIST, QUEUE_WAIT_MS_HIST, REQUEST_ERRORS,
    REQUEST_LATENCY_MS_HIST, REQUEST_TOTAL, TRANSFORM_IN_FLIGHT, TRANSFORM_IN_FLIGHT_PEAK,
};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// guard to track concurrent transforms.
///
/// increments `TRANSFORM_IN_FLIGHT` counter on creation.
/// decrements counter on drop via `Drop` impl.
/// updates `TRANSFORM_IN_FLIGHT_PEAK` if current count exceeds previous peak.
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

/// records request completion with total latency and success status.
///
/// increments `REQUEST_TOTAL` always.
/// increments `REQUEST_ERRORS` on failure.
/// records latency distribution for p50/p95/p99 calculation.
pub fn record_request_finished(duration: Duration, success: bool) {
    REQUEST_TOTAL.fetch_add(1, Ordering::Relaxed);
    if !success {
        REQUEST_ERRORS.fetch_add(1, Ordering::Relaxed);
    }
    record_latency_hist(&REQUEST_LATENCY_MS_HIST, duration);
}

/// records time spent waiting for a transform permit.
pub fn record_queue_wait(duration: Duration) {
    record_latency_hist(&QUEUE_WAIT_MS_HIST, duration);
}

/// records image processing time (excluding queue wait).
pub fn record_processing_latency(duration: Duration) {
    record_latency_hist(&PROCESSING_MS_HIST, duration);
}

/// records that a blocking task was started.
pub fn record_blocking_task_started() {
    BLOCKING_TASKS_TOTAL.fetch_add(1, Ordering::Relaxed);
}

pub(super) fn append_request_latency_metrics(out: &mut String) {
    append_latency_percentiles(out, "request", &REQUEST_LATENCY_MS_HIST);
    append_latency_percentiles(out, "queue_wait", &QUEUE_WAIT_MS_HIST);
    append_latency_percentiles(out, "processing", &PROCESSING_MS_HIST);
}
