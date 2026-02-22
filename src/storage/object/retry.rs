//! retry logic for storage requests.
//!
//! classifies retryable errors and calculates exponential backoff.

use reqwest::StatusCode;
use std::time::Duration;

/// checks if an HTTP status code warrants a retry.
///
/// retries on 429 (too many requests) and 5xx server errors.
pub(super) fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// calculates exponential backoff delay.
///
/// base * 2^attempt, capped at 2^5 (32x) to avoid excessive delays.
pub(super) fn backoff_delay(base: Duration, attempt: u32) -> Duration {
    let shift = attempt.min(5);
    base.saturating_mul(1u32 << shift)
}
