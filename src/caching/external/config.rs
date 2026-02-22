//! cache configuration from environment variables.
//!
//! all timeouts and thresholds are configurable via env vars with sensible defaults.
//! adaptive TTL feature extends expiration for frequently accessed entries.

use std::env;

const DEFAULT_SOURCE_TTL_SECS: u64 = 300;
const DEFAULT_RESULT_TTL_SECS: u64 = 3_600;
const DEFAULT_UNKNOWN_RESULT_TTL_SECS: u64 = 5;
const DEFAULT_VERSION_TTL_SECS: u64 = 30;
const DEFAULT_SOURCE_MAX_STORE_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_RESULT_MAX_STORE_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_LOCK_TTL_MS: u64 = 60_000;
const DEFAULT_LOCK_WAIT_MS: u64 = 20_000;
const DEFAULT_LOCK_POLL_MS: u64 = 50;
const DEFAULT_LOCK_HEARTBEAT_MS: u64 = 5_000;
const DEFAULT_INFO_SAMPLE_INTERVAL_MS: u64 = 5_000;
const DEFAULT_FREQ_WINDOW_SECS: u64 = 300;
const DEFAULT_HOT_THRESHOLD: u64 = 16;
const DEFAULT_HOT_TTL_MULTIPLIER: u64 = 3;
const DEFAULT_HOT_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_HOT_CACHE_MAX_ENTRY_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_HOT_CACHE_MAX_ITEMS: usize = 512;

/// cache configuration parsed from environment variables.
#[derive(Clone)]
pub(super) struct CacheConfig {
    pub(super) prefix: String,
    pub(super) source_ttl_secs: u64,
    pub(super) result_ttl_secs: u64,
    pub(super) unknown_result_ttl_secs: u64,
    pub(super) version_ttl_secs: u64,
    pub(super) source_max_store_bytes: usize,
    pub(super) result_max_store_bytes: usize,
    pub(super) lock_ttl_ms: u64,
    pub(super) lock_wait_ms: u64,
    pub(super) lock_poll_ms: u64,
    pub(super) lock_heartbeat_ms: u64,
    pub(super) info_sample_interval_ms: u64,
    pub(super) adaptive_ttl: bool,
    pub(super) freq_window_secs: u64,
    pub(super) hot_threshold: u64,
    pub(super) hot_ttl_multiplier: u64,
    pub(super) hot_cache_enabled: bool,
    pub(super) hot_cache_max_bytes: usize,
    pub(super) hot_cache_max_entry_bytes: usize,
    pub(super) hot_cache_max_items: usize,
}

impl CacheConfig {
    /// parses cache configuration from environment variables.
    pub(super) fn from_env() -> Self {
        let hot_cache_max_items =
            parse_env_usize("IMAGIK_HOT_CACHE_MAX_ITEMS", DEFAULT_HOT_CACHE_MAX_ITEMS).max(1);
        Self {
            prefix: parse_env_string("IMAGIK_CACHE_PREFIX", "imagik:v1"),
            source_ttl_secs: parse_env_u64("IMAGIK_CACHE_SOURCE_TTL_SECS", DEFAULT_SOURCE_TTL_SECS)
                .max(1),
            result_ttl_secs: parse_env_u64("IMAGIK_CACHE_RESULT_TTL_SECS", DEFAULT_RESULT_TTL_SECS)
                .max(1),
            unknown_result_ttl_secs: parse_env_u64(
                "IMAGIK_CACHE_UNKNOWN_RESULT_TTL_SECS",
                DEFAULT_UNKNOWN_RESULT_TTL_SECS,
            ),
            version_ttl_secs: parse_env_u64(
                "IMAGIK_CACHE_VERSION_TTL_SECS",
                DEFAULT_VERSION_TTL_SECS,
            )
            .max(1),
            source_max_store_bytes: parse_env_usize(
                "IMAGIK_CACHE_SOURCE_MAX_STORE_BYTES",
                DEFAULT_SOURCE_MAX_STORE_BYTES,
            )
            .max(1024),
            result_max_store_bytes: parse_env_usize(
                "IMAGIK_CACHE_RESULT_MAX_STORE_BYTES",
                DEFAULT_RESULT_MAX_STORE_BYTES,
            )
            .max(1024),
            lock_ttl_ms: parse_env_u64("IMAGIK_CACHE_LOCK_TTL_MS", DEFAULT_LOCK_TTL_MS).max(500),
            lock_wait_ms: parse_env_u64("IMAGIK_CACHE_LOCK_WAIT_MS", DEFAULT_LOCK_WAIT_MS).max(50),
            lock_poll_ms: parse_env_u64("IMAGIK_CACHE_LOCK_POLL_MS", DEFAULT_LOCK_POLL_MS).max(10),
            lock_heartbeat_ms: parse_env_u64(
                "IMAGIK_CACHE_LOCK_HEARTBEAT_MS",
                DEFAULT_LOCK_HEARTBEAT_MS,
            )
            .max(100),
            info_sample_interval_ms: parse_env_u64(
                "IMAGIK_CACHE_INFO_SAMPLE_INTERVAL_MS",
                DEFAULT_INFO_SAMPLE_INTERVAL_MS,
            )
            .max(250),
            adaptive_ttl: parse_env_bool("IMAGIK_CACHE_ADAPTIVE_TTL", false),
            freq_window_secs: parse_env_u64(
                "IMAGIK_CACHE_FREQ_WINDOW_SECS",
                DEFAULT_FREQ_WINDOW_SECS,
            )
            .max(30),
            hot_threshold: parse_env_u64("IMAGIK_CACHE_HOT_THRESHOLD", DEFAULT_HOT_THRESHOLD)
                .max(2),
            hot_ttl_multiplier: parse_env_u64(
                "IMAGIK_CACHE_HOT_TTL_MULTIPLIER",
                DEFAULT_HOT_TTL_MULTIPLIER,
            )
            .max(2),
            hot_cache_enabled: parse_env_bool("IMAGIK_HOT_CACHE_ENABLED", true),
            hot_cache_max_bytes: parse_env_usize(
                "IMAGIK_HOT_CACHE_MAX_BYTES",
                DEFAULT_HOT_CACHE_MAX_BYTES,
            )
            .max(1024),
            hot_cache_max_entry_bytes: parse_env_usize(
                "IMAGIK_HOT_CACHE_MAX_ENTRY_BYTES",
                DEFAULT_HOT_CACHE_MAX_ENTRY_BYTES,
            )
            .max(1024),
            hot_cache_max_items,
        }
    }
}

fn parse_env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn parse_env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn parse_env_string(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn parse_env_bool(key: &str, default: bool) -> bool {
    env::var(key).ok().map_or(default, |value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}
