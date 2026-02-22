//! redis INFO sampler for cache telemetry.
//!
//! polls INFO command periodically to track `used_memory` and `evicted_keys`.
//! idempotent: only starts once even if called multiple times.

use crate::observability::metrics;
use redis::aio::ConnectionManager;
use std::sync::OnceLock;
use std::time::Duration;

static CACHE_INFO_SAMPLER_STARTED: OnceLock<()> = OnceLock::new();

/// starts the INFO sampler background task.
///
/// samples redis INFO at the specified interval and updates metrics.
/// idempotent: subsequent calls are ignored.
pub(super) fn start_cache_info_sampler(manager: ConnectionManager, interval_ms: u64) {
    if CACHE_INFO_SAMPLER_STARTED.set(()).is_err() {
        return;
    }

    tokio::spawn(async move {
        loop {
            let mut conn = manager.clone();
            if let Ok(info) = redis::cmd("INFO").query_async::<String>(&mut conn).await {
                let used_memory =
                    usize::try_from(parse_info_u64(&info, "used_memory").unwrap_or(0))
                        .unwrap_or(usize::MAX);
                let evicted_keys = parse_info_u64(&info, "evicted_keys").unwrap_or(0);
                metrics::set_external_cache_stats(used_memory, evicted_keys);
            }

            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        }
    });
}

/// extracts a numeric value from redis INFO output.
///
/// pattern matches `key:value` format.
fn parse_info_u64(info: &str, key: &str) -> Option<u64> {
    let pattern = format!("{key}:");
    info.lines().find_map(|line| {
        line.strip_prefix(&pattern)
            .and_then(|value| value.trim().parse::<u64>().ok())
    })
}

#[cfg(test)]
mod tests {
    use super::parse_info_u64;

    #[test]
    fn parse_info_extracts_numbers() {
        let info = "used_memory:123\nevicted_keys:42\n";
        assert_eq!(parse_info_u64(info, "used_memory"), Some(123));
        assert_eq!(parse_info_u64(info, "evicted_keys"), Some(42));
        assert_eq!(parse_info_u64(info, "missing"), None);
    }
}
