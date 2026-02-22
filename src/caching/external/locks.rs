//! distributed locking for cache coherency and request coalescing.
//!
//! locking strategy: redis SET NX PX with unique token ownership.
//! - `try_acquire_lock`: non-blocking attempt, returns token on success
//! - `extend_lock`: heartbeat to prevent expiration during long operations
//! - `release_lock`: lua script verifies token before deletion (safe release)
//!
//! request coalescing: when lock is held, waiters poll for result appearance
//! instead of duplicating work. reduces thundering herd under load.
//!
//! lock timeouts configured via `lock_ttl_ms` and `lock_heartbeat_ms`.
//! heartbeat fires at `min(lock_ttl_ms/2, lock_heartbeat_ms)` intervals.

use super::ExternalCache;
use crate::observability::metrics::{self, ExternalCacheLayer};
use redis::{AsyncCommands, Script};
use std::time::{Duration, Instant};

impl ExternalCache {
    /// tries to acquire a distributed lock with token-based ownership.
    ///
    /// returns Some(token) on success, None if lock is held by another.
    /// lock expires after `lock_ttl_ms` if not extended.
    pub async fn try_acquire_lock(&self, lock_key: &str) -> Option<String> {
        let mut conn = self.manager.clone()?;
        let token = format!("{:016x}{:016x}", fastrand::u64(..), fastrand::u64(..));
        eprintln!("[LOCK] Attempting to acquire lock: key={}", lock_key);

        let start = Instant::now();
        let response = redis::cmd("SET")
            .arg(lock_key)
            .arg(&token)
            .arg("NX")
            .arg("PX")
            .arg(self.config.lock_ttl_ms)
            .query_async::<Option<String>>(&mut conn)
            .await;

        match response {
            Ok(Some(_)) => {
                eprintln!("[LOCK] Lock acquired: key={}, token={}", lock_key, &token[..16]);
                metrics::record_cache_lock_acquired(start.elapsed());
                Some(token)
            }
            Ok(None) => {
                eprintln!("[LOCK] Lock contended (already held): key={}", lock_key);
                metrics::record_cache_lock_contended();
                None
            }
            Err(e) => {
                eprintln!("[LOCK] Lock acquisition error: key={}, error={}", lock_key, e);
                metrics::record_cache_lock_error();
                None
            }
        }
    }

    /// releases a lock using token-based verification.
    ///
    /// only deletes if the token matches (prevents releasing another's lock).
    pub async fn release_lock(&self, lock_key: &str, token: &str) {
        let Some(mut conn) = self.manager.clone() else {
            return;
        };

        let script = Script::new(
            "if redis.call('get', KEYS[1]) == ARGV[1] then return redis.call('del', KEYS[1]) else return 0 end",
        );

        if script
            .key(lock_key)
            .arg(token)
            .invoke_async::<i32>(&mut conn)
            .await
            .is_err()
        {
            metrics::record_cache_lock_release_error();
        }
    }

    /// extends lock expiration time.
    ///
    /// returns true if extension succeeded (token matched).
    /// used by heartbeat tasks during long operations.
    pub async fn extend_lock(&self, lock_key: &str, token: &str) -> bool {
        let Some(mut conn) = self.manager.clone() else {
            return false;
        };

        let script = Script::new(
            "if redis.call('get', KEYS[1]) == ARGV[1] then return redis.call('pexpire', KEYS[1], ARGV[2]) else return 0 end",
        );

        script
            .key(lock_key)
            .arg(token)
            .arg(i64::try_from(self.config.lock_ttl_ms).unwrap_or(i64::MAX))
            .invoke_async::<i32>(&mut conn)
            .await
            .map_or_else(
                |_| {
                    metrics::record_cache_lock_error();
                    false
                },
                |result| result == 1,
            )
    }

    /// waits for a result to appear in cache.
    ///
    /// polls until timeout or key appears. populates hot cache on success.
    pub async fn wait_for_result(&self, result_cache_key: &str) -> Option<Vec<u8>> {
        eprintln!("[LOCK] Waiting for result to appear in cache: key={}", result_cache_key);
        let payload = self
            .wait_for_key(result_cache_key, ExternalCacheLayer::Result)
            .await?;
        self.set_hot_result(result_cache_key, &payload);
        eprintln!("[LOCK] Result found while waiting: key={}", result_cache_key);
        Some(payload)
    }

    /// waits for source bytes to appear in cache.
    pub async fn wait_for_source(&self, object_key: &str, version_token: &str) -> Option<Vec<u8>> {
        let source_key = self.source_entry_key(object_key, version_token);
        self.wait_for_key(&source_key, ExternalCacheLayer::Source)
            .await
    }

    /// polls for key appearance with timeout.
    ///
    /// used by `wait_for_result` and `wait_for_source`.
    async fn wait_for_key(&self, key: &str, layer: ExternalCacheLayer) -> Option<Vec<u8>> {
        let mut conn = self.manager.clone()?;
        let wait_start = Instant::now();
        let deadline = wait_start + Duration::from_millis(self.config.lock_wait_ms);

        while Instant::now() < deadline {
            if let Ok(value) = conn.get::<_, Option<Vec<u8>>>(key).await
                && let Some(payload) = value
            {
                metrics::record_cache_lock_wait(wait_start.elapsed());
                metrics::record_cache_hit(layer);
                return Some(payload);
            }
            tokio::time::sleep(Duration::from_millis(self.config.lock_poll_ms)).await;
        }

        metrics::record_cache_lock_timeout(wait_start.elapsed());
        eprintln!("[LOCK] Wait timeout: key={}, waited={:?}", key, wait_start.elapsed());
        None
    }
}
