//! object storage layer with dual-mode access and local caching.
//!
//! access modes:
//! - `signed_url`: HTTP GET to pre-signed URLs (CDN-compatible)
//! - `s3`: direct AWS S3 API with configurable endpoint (minio-compatible)
//!
//! features:
//! - automatic retry with exponential backoff (`retry.rs`)
//! - local disk cache to reduce repeated fetches (`disk_cache.rs`)
//! - version token extraction for cache invalidation (`key.rs`)
//! - streaming response handling with size limits (`fetch.rs`)
//!
//! modules:
//! - `config`: env parsing and validation
//! - `client`: signed-url or S3 client construction
//! - `fetch`: request/retry/streaming fetch logic
//! - `disk_cache`: optional local source-byte cache (lru eviction)
//! - `key`: object-key/url/version token helpers
//! - `retry`: retry classification/backoff math

mod client;
mod config;
mod disk_cache;
mod fetch;
mod key;
mod retry;

use crate::common::types::AppResult;
use client::{SourceMode, build_mode};
use config::StorageSettings;
use disk_cache::DiskCache;
use std::time::Duration;

pub struct SourceObject {
    pub bytes: Vec<u8>,
    pub version_token: Option<String>,
}

pub struct ObjectStorageSource {
    mode: SourceMode,
    retry_attempts: u32,
    retry_backoff: Duration,
    max_object_bytes: usize,
    cache: Option<DiskCache>,
    cache_namespace: String,
    key_prefix: Option<String>,
}

impl ObjectStorageSource {
    pub async fn from_env() -> AppResult<Self> {
        eprintln!("[OBJECT-STORAGE] Initializing object storage client...");
        let settings = StorageSettings::from_env()?;
        let (mode, cache_namespace) = build_mode(&settings).await?;
        eprintln!("[OBJECT-STORAGE] Client initialized successfully");

        Ok(Self {
            mode,
            retry_attempts: settings.retry_attempts,
            retry_backoff: settings.retry_backoff,
            max_object_bytes: settings.max_object_bytes,
            cache: settings.cache.map(DiskCache::from_settings),
            cache_namespace,
            key_prefix: settings.key_prefix,
        })
    }
}
