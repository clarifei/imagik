//! in-process lru hot cache for frequently accessed results.
//!
//! purpose: eliminates external cache round-trip for hot entries.
//! ~10μs access time vs ~1ms for redis (100x faster).
//!
//! limits: enforced separately by item count and total bytes.
//! - item limit: prevents unbounded memory growth
//! - byte limit: accommodates variable entry sizes
//!
//! eviction: lru policy when either limit exceeded.
//! uses `LruCache` from `lru` crate for O(1) operations.
//!
//! thread-safety: `Mutex` protects internal state.
//! contention is low because this is l1 cache (checked first).

use axum::body::Bytes;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

/// internal state tracking for the hot cache.
struct HotCacheState {
    lru: LruCache<String, Bytes>,
    current_bytes: usize,
}

/// thread-safe in-process LRU cache for hot entries.
pub(super) struct InProcessHotCache {
    state: Mutex<HotCacheState>,
    max_bytes: usize,
    max_entry_bytes: usize,
}

impl InProcessHotCache {
    /// creates a new hot cache with capacity limits.
    pub(super) fn new(max_items: usize, max_bytes: usize, max_entry_bytes: usize) -> Self {
        let capacity = NonZeroUsize::new(max_items).expect("hot cache capacity must be non-zero");
        Self {
            state: Mutex::new(HotCacheState {
                lru: LruCache::new(capacity),
                current_bytes: 0,
            }),
            max_bytes,
            max_entry_bytes,
        }
    }

    /// retrieves a value from the hot cache.
    pub(super) fn get(&self, key: &str) -> Option<Bytes> {
        let mut state = self.state.lock().ok()?;
        state.lru.get(key).cloned()
    }

    /// inserts a value into the hot cache, evicting if necessary.
    ///
    /// skips entries exceeding size limits.
    pub(super) fn insert(&self, key: &str, value: &[u8]) -> HotInsertResult {
        if value.len() > self.max_entry_bytes || value.len() > self.max_bytes {
            return HotInsertResult::Skipped;
        }

        let Ok(mut state) = self.state.lock() else {
            return HotInsertResult::Skipped;
        };

        if let Some(previous) = state
            .lru
            .put(key.to_string(), Bytes::copy_from_slice(value))
        {
            state.current_bytes = state.current_bytes.saturating_sub(previous.len());
        }
        state.current_bytes = state.current_bytes.saturating_add(value.len());

        let mut evictions = 0u64;
        while state.current_bytes > self.max_bytes {
            let Some((_evicted_key, evicted_value)) = state.lru.pop_lru() else {
                break;
            };
            state.current_bytes = state.current_bytes.saturating_sub(evicted_value.len());
            evictions += 1;
        }

        HotInsertResult::Stored {
            bytes: value.len(),
            evictions,
        }
    }
}

/// result of a hot cache insert operation.
pub(super) enum HotInsertResult {
    Stored { bytes: usize, evictions: u64 },
    Skipped,
}
