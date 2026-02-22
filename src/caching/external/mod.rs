//! external cache implementation with layered caching strategy.
//!
//! architecture: three-tier caching for optimal latency and resource usage
//! - l1: in-process hot cache (`hot_cache.rs`) - fastest, no serialization
//! - l2: external cache (`store.rs`) - shared across instances (redis/dragonfly)
//! - l3: source cache (`store.rs`) - caches raw object bytes to reduce storage calls
//!
//! distributed locking (`locks.rs`):
//! - prevents thundering herd on cache miss
//! - token-based ownership with heartbeat extension
//! - wait loops for lock holders to complete
//!
//! ttl strategy:
//! - base ttl per layer (source, result, version tokens)
//! - adaptive ttl extension for hot entries (frequency-based)
//! - unknown version results use shortened ttl
//!
//! modules:
//! - `config`: environment-driven cache policy
//! - `keys`: cache/lock key construction with namespace isolation
//! - `hot_cache`: optional in-process lru for hot final results
//! - `store`: get/set/version/ttl operations with metrics
//! - `locks`: distributed locking and request coalescing
//! - `info_sampler`: redis/dragonfly INFO telemetry polling

mod config;
mod hot_cache;
mod info_sampler;
mod keys;
mod locks;
mod store;

use crate::observability::metrics::{self, ExternalCacheLayer};
use axum::body::Bytes;
use config::CacheConfig;
use hot_cache::{HotInsertResult, InProcessHotCache};
use redis::aio::ConnectionManager;
use std::env;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct ExternalCache {
    manager: Option<ConnectionManager>,
    config: CacheConfig,
    hot_cache: Option<Arc<InProcessHotCache>>,
}

impl ExternalCache {
    pub async fn from_env() -> Self {
        let config = CacheConfig::from_env();
        eprintln!("[EXTERNAL-CACHE] Initializing external cache...");

        let hot_cache = if config.hot_cache_enabled {
            eprintln!("[EXTERNAL-CACHE] Hot cache enabled");
            Some(Arc::new(InProcessHotCache::new(
                config.hot_cache_max_items,
                config.hot_cache_max_bytes,
                config.hot_cache_max_entry_bytes,
            )))
        } else {
            eprintln!("[EXTERNAL-CACHE] Hot cache disabled");
            None
        };

        let cache_url = match env::var("IMAGIK_CACHE_URL") {
            Ok(value) if !value.trim().is_empty() => {
                eprintln!("[EXTERNAL-CACHE] Cache URL configured: {}", value.split('@').last().unwrap_or("hidden"));
                value
            },
            _ => {
                eprintln!("[EXTERNAL-CACHE] No cache URL configured, external cache disabled");
                metrics::set_external_cache_enabled(false);
                return Self {
                    manager: None,
                    config,
                    hot_cache,
                };
            }
        };

        let manager = match redis::Client::open(cache_url.clone()) {
            Ok(client) => {
                eprintln!("[EXTERNAL-CACHE] Redis client created for URL: {}", 
                    if cache_url.contains(":") { 
                        cache_url.split('@').last().unwrap_or("hidden").to_string()
                    } else { 
                        cache_url.clone() 
                    });
                eprintln!("[EXTERNAL-CACHE] Attempting to get connection manager with 5s timeout...");
                match tokio::time::timeout(Duration::from_secs(5), client.get_connection_manager()).await {
                    Ok(Ok(conn)) => {
                        eprintln!("[EXTERNAL-CACHE] Redis connection established successfully");
                        Some(conn)
                    },
                    Ok(Err(e)) => {
                        eprintln!("[EXTERNAL-CACHE] Failed to get connection manager: {:?}", e);
                        eprintln!("[EXTERNAL-CACHE] This usually means:");
                        eprintln!("[EXTERNAL-CACHE]   - Wrong password or authentication failed");
                        eprintln!("[EXTERNAL-CACHE]   - Network connectivity issue");
                        eprintln!("[EXTERNAL-CACHE]   - TLS/SSL issue (try redis:// vs rediss://)");
                        eprintln!("[EXTERNAL-CACHE]   - Firewall blocking the connection");
                        None
                    }
                    Err(_) => {
                        eprintln!("[EXTERNAL-CACHE] Connection timeout after 5 seconds");
                        eprintln!("[EXTERNAL-CACHE] This usually means:");
                        eprintln!("[EXTERNAL-CACHE]   - Host not reachable (wrong host or port)");
                        eprintln!("[EXTERNAL-CACHE]   - Firewall blocking the connection");
                        eprintln!("[EXTERNAL-CACHE]   - Dragonfly/Redis is not running on the specified address");
                        None
                    }
                }
            },
            Err(e) => {
                eprintln!("[EXTERNAL-CACHE] Failed to open Redis client (invalid URL format): {}", e);
                None
            }
        };

        metrics::set_external_cache_enabled(manager.is_some());

        if let Some(conn) = &manager {
            info_sampler::start_cache_info_sampler(conn.clone(), config.info_sample_interval_ms);
        }

        Self {
            manager,
            config,
            hot_cache,
        }
    }

    pub const fn enabled(&self) -> bool {
        self.manager.is_some()
    }

    pub fn lock_heartbeat_ms(&self) -> u64 {
        self.config
            .lock_heartbeat_ms
            .min(self.config.lock_ttl_ms / 2)
            .max(100)
    }

    pub const fn unknown_result_ttl_secs(&self) -> u64 {
        self.config.unknown_result_ttl_secs
    }

    pub fn get_hot_result(&self, result_cache_key: &str) -> Option<Bytes> {
        let hot = self.hot_cache.as_ref()?;
        let bytes = hot.get(result_cache_key);
        if bytes.is_some() {
            metrics::record_cache_hit(ExternalCacheLayer::HotResult);
        } else {
            metrics::record_cache_miss(ExternalCacheLayer::HotResult);
        }
        bytes
    }

    pub fn set_hot_result(&self, result_cache_key: &str, payload: &[u8]) {
        let Some(hot) = &self.hot_cache else {
            return;
        };

        match hot.insert(result_cache_key, payload) {
            HotInsertResult::Stored { bytes, evictions } => {
                metrics::record_cache_store(ExternalCacheLayer::HotResult, bytes);
                if evictions > 0 {
                    metrics::record_cache_evictions(ExternalCacheLayer::HotResult, evictions);
                }
            }
            HotInsertResult::Skipped => {
                metrics::record_cache_skip_too_large(ExternalCacheLayer::HotResult, payload.len());
            }
        }
    }
}
