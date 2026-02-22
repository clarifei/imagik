//! cache store operations for results, sources, and versions.
//!
//! all operations are async and handle disabled cache gracefully.
//! size limits enforced before storing to prevent cache bloat.
//! adaptive ttl extends expiration for frequently accessed entries.
//!
//! cache layers:
//! - `result`: transformed image outputs (largest, most valuable to cache)
//! - `source`: raw object bytes from storage (reduces storage egress costs)
//! - `version`: object version tokens (cheap, enables cache invalidation)

use super::ExternalCache;
use crate::observability::metrics::{self, ExternalCacheLayer};
use redis::AsyncCommands;
use std::time::Instant;

impl ExternalCache {
    /// gets a cached transform result from external cache.
    ///
    /// on hit: populates l1 hot cache for subsequent requests.
    /// on miss: returns `None` for cache miss path handling.
    ///
    /// metrics: records hit/miss/error per `ExternalCacheLayer::Result`.
    pub async fn get_result(&self, result_cache_key: &str) -> Option<Vec<u8>> {
        let payload = self
            .get_bytes(result_cache_key, ExternalCacheLayer::Result)
            .await?;
        self.set_hot_result(result_cache_key, &payload);
        Some(payload)
    }

    /// stores a transform result with optional ttl override.
    ///
    /// skips storage if payload exceeds size limits.
    /// uses adaptive ttl based on access frequency.
    pub async fn set_result_with_ttl_override(
        &self,
        result_cache_key: &str,
        payload: &[u8],
        ttl_override_secs: Option<u64>,
    ) {
        self.set_hot_result(result_cache_key, payload);

        if !self.enabled() {
            return;
        }

        if payload.len() > self.config.result_max_store_bytes {
            metrics::record_cache_skip_too_large(ExternalCacheLayer::Result, payload.len());
            return;
        }

        let ttl = ttl_override_secs.unwrap_or(
            self.effective_ttl_secs(ExternalCacheLayer::Result, result_cache_key)
                .await,
        );
        let set_start = Instant::now();

        match self.set_ex_raw(result_cache_key, payload, ttl).await {
            Ok(()) => {
                metrics::record_cache_store(ExternalCacheLayer::Result, payload.len());
                metrics::record_cache_set_latency(ExternalCacheLayer::Result, set_start.elapsed());
            }
            Err(_) => metrics::record_cache_error(ExternalCacheLayer::Result),
        }
    }

    /// gets cached source image bytes for an object version.
    pub async fn get_source_bytes(&self, object_key: &str, version_token: &str) -> Option<Vec<u8>> {
        if !self.enabled() {
            return None;
        }

        let source_key = self.source_entry_key(object_key, version_token);
        self.get_bytes(&source_key, ExternalCacheLayer::Source)
            .await
    }

    /// stores source image bytes for an object version.
    ///
    /// skips storage if payload exceeds size limits.
    pub async fn set_source_bytes(&self, object_key: &str, version_token: &str, payload: &[u8]) {
        if !self.enabled() {
            return;
        }

        if payload.len() > self.config.source_max_store_bytes {
            metrics::record_cache_skip_too_large(ExternalCacheLayer::Source, payload.len());
            return;
        }

        let source_key = self.source_entry_key(object_key, version_token);
        let ttl = self
            .effective_ttl_secs(ExternalCacheLayer::Source, &source_key)
            .await;
        let set_start = Instant::now();

        match self.set_ex_raw(&source_key, payload, ttl).await {
            Ok(()) => {
                metrics::record_cache_store(ExternalCacheLayer::Source, payload.len());
                metrics::record_cache_set_latency(ExternalCacheLayer::Source, set_start.elapsed());
            }
            Err(_) => metrics::record_cache_error(ExternalCacheLayer::Source),
        }
    }

    /// deletes cached source bytes for an object version.
    pub async fn delete_source_bytes(&self, object_key: &str, version_token: &str) {
        if !self.enabled() {
            return;
        }

        let source_key = self.source_entry_key(object_key, version_token);
        let _ = self.del_key(&source_key).await;
    }

    /// gets the cached version token for an object.
    pub async fn get_cached_version(&self, object_key: &str) -> Option<String> {
        if !self.enabled() {
            return None;
        }
        let key = self.version_key(object_key);
        let bytes = self.get_bytes(&key, ExternalCacheLayer::Version).await?;
        String::from_utf8(bytes)
            .ok()
            .filter(|value| !value.is_empty())
    }

    /// stores the version token for an object.
    pub async fn set_cached_version(&self, object_key: &str, version_token: &str) {
        if !self.enabled() {
            return;
        }

        let key = self.version_key(object_key);
        let value = version_token.as_bytes();
        let ttl = self.config.version_ttl_secs;
        let set_start = Instant::now();

        if self.set_ex_raw(&key, value, ttl).await.is_ok() {
            metrics::record_cache_store(ExternalCacheLayer::Version, value.len());
            metrics::record_cache_set_latency(ExternalCacheLayer::Version, set_start.elapsed());
        } else {
            metrics::record_cache_error(ExternalCacheLayer::Version);
        }
    }

    /// low-level byte retrieval with metrics.
    async fn get_bytes(&self, key: &str, layer: ExternalCacheLayer) -> Option<Vec<u8>> {
        let mut conn = self.manager.clone()?;
        let get_start = Instant::now();

        match conn.get::<_, Option<Vec<u8>>>(key).await {
            Ok(Some(bytes)) => {
                metrics::record_cache_hit(layer);
                metrics::record_cache_get_latency(layer, get_start.elapsed());
                self.note_access(layer, key).await;
                Some(bytes)
            }
            Ok(None) => {
                metrics::record_cache_miss(layer);
                metrics::record_cache_get_latency(layer, get_start.elapsed());
                None
            }
            Err(_) => {
                metrics::record_cache_error(layer);
                None
            }
        }
    }

    /// stores bytes with expiration.
    async fn set_ex_raw(&self, key: &str, payload: &[u8], ttl_secs: u64) -> redis::RedisResult<()> {
        let Some(mut conn) = self.manager.clone() else {
            return Ok(());
        };
        conn.set_ex::<_, _, ()>(key, payload, ttl_secs).await
    }

    /// deletes a key from cache.
    async fn del_key(&self, key: &str) -> redis::RedisResult<()> {
        let Some(mut conn) = self.manager.clone() else {
            return Ok(());
        };
        conn.del::<_, i32>(key).await.map(|_| ())
    }

    /// records an access for adaptive ttl.
    ///
    /// increments frequency counter for the entry.
    async fn note_access(&self, layer: ExternalCacheLayer, entry_key: &str) {
        if !self.config.adaptive_ttl {
            return;
        }

        let Some(mut conn) = self.manager.clone() else {
            return;
        };

        let key = self.frequency_key(layer, entry_key);
        if conn.incr::<_, _, i64>(&key, 1).await.is_ok() {
            let _ = conn
                .expire::<_, ()>(
                    &key,
                    i64::try_from(self.config.freq_window_secs).unwrap_or(i64::MAX),
                )
                .await;
        }
    }

    /// calculates effective ttl based on access frequency.
    ///
    /// hot entries get extended ttl. cold entries use base ttl.
    async fn effective_ttl_secs(&self, layer: ExternalCacheLayer, entry_key: &str) -> u64 {
        let base_ttl = match layer {
            ExternalCacheLayer::Source => self.config.source_ttl_secs,
            ExternalCacheLayer::Result | ExternalCacheLayer::HotResult => {
                self.config.result_ttl_secs
            }
            ExternalCacheLayer::Version => self.config.version_ttl_secs,
        };

        if !self.config.adaptive_ttl {
            return base_ttl;
        }

        let Some(mut conn) = self.manager.clone() else {
            return base_ttl;
        };

        let key = self.frequency_key(layer, entry_key);
        let count = conn.incr::<_, _, i64>(&key, 1).await.ok().unwrap_or(0);
        let _ = conn
            .expire::<_, ()>(
                &key,
                i64::try_from(self.config.freq_window_secs).unwrap_or(i64::MAX),
            )
            .await;

        if u64::try_from(count).unwrap_or(0) >= self.config.hot_threshold {
            base_ttl.saturating_mul(self.config.hot_ttl_multiplier)
        } else {
            base_ttl
        }
    }
}
