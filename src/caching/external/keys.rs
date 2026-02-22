//! cache key construction with consistent hashing.
//!
//! keys use blake3 hashes to avoid exposing internal identifiers.
//! consistent naming scheme: prefix:type:hash[:hash...]

use super::ExternalCache;
use crate::observability::metrics::ExternalCacheLayer;

impl ExternalCache {
    /// builds the cache key for a transform result.
    ///
    /// includes hashes of object key, version, and transform params.
    pub fn build_result_cache_key(
        &self,
        object_key: &str,
        version_token: &str,
        transform_signature: &str,
    ) -> String {
        let object_hash = hash_string(object_key);
        let version_hash = hash_string(version_token);
        let transform_hash = hash_string(transform_signature);
        format!(
            "{}:result:{}:{}:{}",
            self.config.prefix, object_hash, version_hash, transform_hash
        )
    }

    /// builds the lock key for result computation.
    pub fn build_result_lock_key(&self, result_cache_key: &str) -> String {
        let key_hash = hash_string(result_cache_key);
        format!("{}:lock:result:{}", self.config.prefix, key_hash)
    }

    /// builds the lock key for source byte fetching.
    pub fn build_source_lock_key(&self, object_key: &str, version_token: &str) -> String {
        let object_hash = hash_string(object_key);
        let version_hash = hash_string(version_token);
        format!(
            "{}:lock:source:{}:{}",
            self.config.prefix, object_hash, version_hash
        )
    }

    /// builds the cache key for source image bytes.
    pub(super) fn source_entry_key(&self, object_key: &str, version_token: &str) -> String {
        let object_hash = hash_string(object_key);
        let version_hash = hash_string(version_token);
        format!(
            "{}:source:{}:{}",
            self.config.prefix, object_hash, version_hash
        )
    }

    /// builds the cache key for version tokens.
    pub(super) fn version_key(&self, object_key: &str) -> String {
        let object_hash = hash_string(object_key);
        format!("{}:version:{}", self.config.prefix, object_hash)
    }

    /// builds the frequency tracking key for adaptive ttl.
    pub(super) fn frequency_key(&self, layer: ExternalCacheLayer, entry_key: &str) -> String {
        let layer_name = layer.as_name();
        let entry_hash = hash_string(entry_key);
        format!("{}:freq:{}:{}", self.config.prefix, layer_name, entry_hash)
    }
}

/// hashes a string using blake3 for consistent cache keys.
fn hash_string(value: &str) -> String {
    blake3::hash(value.as_bytes()).to_hex().to_string()
}
