//! local disk cache for source image bytes.
//!
//! purpose: reduces repeated object storage fetches for popular images.
//! particularly effective when external cache is disabled or cold.
//!
//! design:
//! - file-based storage with hashed keys (flat directory structure)
//! - ttl-based expiration checked on each read
//! - lru eviction triggered when capacity exceeded
//! - write-then-rename for atomic updates (prevents partial reads)
//!
//! eviction strategy:
//! - scans directory when `current_bytes > max_bytes`
//! - sorts by modification time, removes oldest first
//! - amortized: scan only runs every `EVICTION_SCAN_INTERVAL_MS`
//!
//! performance: local SSD access is ~100x faster than S3 round-trip.

use super::config::DiskCacheSettings;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};
use tokio::fs;

/// interval between eviction scans to amortize cleanup cost.
const EVICTION_SCAN_INTERVAL_MS: u64 = 5_000;

/// on-disk cache for source image bytes.
pub(super) struct DiskCache {
    dir: PathBuf,
    ttl: Duration,
    max_bytes: usize,
    next_eviction_scan_ms: AtomicU64,
}

impl DiskCache {
    /// creates a disk cache from settings.
    pub(super) fn from_settings(settings: DiskCacheSettings) -> Self {
        Self {
            dir: settings.dir,
            ttl: settings.ttl,
            max_bytes: settings.max_bytes,
            next_eviction_scan_ms: AtomicU64::new(0),
        }
    }

    /// reads cached bytes if present and not expired.
    ///
    /// returns None if file missing, expired, or unreadable.
    pub(super) async fn try_read(&self, cache_key: &str) -> Option<Vec<u8>> {
        let path = self.cache_path(cache_key);
        let metadata = fs::metadata(&path).await.ok()?;
        let modified = metadata.modified().ok()?;
        let age = SystemTime::now().duration_since(modified).ok()?;
        if age > self.ttl {
            eprintln!("[DISK-CACHE] EXPIRED: removing {}, age={:?}", path.display(), age);
            let _ = fs::remove_file(path).await;
            return None;
        }
        
        eprintln!("[DISK-CACHE] HIT: {} (age={:?})", path.display(), age);
        fs::read(path).await.ok()
    }

    /// writes bytes to cache with atomic replace.
    ///
    /// writes to temp file then renames for atomicity.
    /// triggers eviction scan if over capacity.
    pub(super) async fn write(&self, cache_key: &str, bytes: &[u8]) {
        if fs::create_dir_all(&self.dir).await.is_err() {
            eprintln!("[DISK-CACHE] ERROR: Failed to create cache directory {}", self.dir.display());
            return;
        }

        let path = self.cache_path(cache_key);
        let tmp = path.with_extension("tmp");

        if fs::write(&tmp, bytes).await.is_err() {
            eprintln!("[DISK-CACHE] ERROR: Failed to write temp file {}", tmp.display());
            return;
        }

        if fs::rename(&tmp, &path).await.is_err() {
            let _ = fs::remove_file(&tmp).await;
            eprintln!("[DISK-CACHE] ERROR: Failed to rename temp file to {}", path.display());
            return;
        }
        
        eprintln!("[DISK-CACHE] WRITE: {} ({} bytes)", path.display(), bytes.len());

        if self.max_bytes > 0 && self.should_scan_for_eviction() {
            self.enforce_capacity().await;
        }
    }

    /// computes the filesystem path for a cache key.
    fn cache_path(&self, cache_key: &str) -> PathBuf {
        let mut key_hasher = DefaultHasher::new();
        cache_key.hash(&mut key_hasher);
        let key_hash = key_hasher.finish();
        self.dir.join(format!("{key_hash:016x}.bin"))
    }

    /// evicts oldest files until under capacity.
    ///
    /// sorts by modification time and removes oldest first.
    async fn enforce_capacity(&self) {
        let Ok(mut entries) = fs::read_dir(&self.dir).await else {
            return;
        };

        let mut files = Vec::new();
        let mut total_bytes: u64 = 0;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let metadata = match entry.metadata().await {
                Ok(metadata) if metadata.is_file() => metadata,
                _ => continue,
            };

            let len = metadata.len();
            total_bytes = total_bytes.saturating_add(len);
            let modified = metadata.modified().ok();
            files.push((entry.path(), len, modified));
        }

        let max_bytes_u64 = u64::try_from(self.max_bytes).unwrap_or(u64::MAX);
        if total_bytes <= max_bytes_u64 {
            return;
        }

        files.sort_by_key(|(_, _, modified)| *modified);

        for (path, len, _) in files {
            if total_bytes <= max_bytes_u64 {
                break;
            }
            if fs::remove_file(path).await.is_ok() {
                total_bytes = total_bytes.saturating_sub(len);
            }
        }
    }

    /// checks if eviction scan should run.
    ///
    /// amortized: only runs every `EVICTION_SCAN_INTERVAL_MS`.
    fn should_scan_for_eviction(&self) -> bool {
        // Eviction scans are amortized to avoid O(n files) work on every write.
        let now_ms = unix_time_ms();
        let next_ms = self.next_eviction_scan_ms.load(Ordering::Relaxed);
        if now_ms < next_ms {
            return false;
        }

        self.next_eviction_scan_ms
            .compare_exchange(
                next_ms,
                now_ms.saturating_add(EVICTION_SCAN_INTERVAL_MS),
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_ok()
    }
}

/// returns current unix time in milliseconds.
fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
