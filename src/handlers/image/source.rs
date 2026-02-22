use super::locks;
use crate::caching::external::ExternalCache;
use crate::common::types::AppResult;
use crate::config::decode::{decode_limits, validate_decode_bounds};
use crate::observability::metrics::{self, ExternalCacheLayer, PipelineStage};
use crate::storage::object::ObjectStorageSource;
use image::ImageReader;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinHandle;

/// loaded source image with effective version token.
///
/// the effective version token may differ from the requested version
/// if the object was not found at the requested version (e.g., cache miss
/// on version lookup returned "unknown" token but fetch discovered version).
///
/// version token is used for:
/// - cache key construction (result cache invalidation on object change)
/// - canonical result key generation (when requested version differs from effective)
pub(super) struct LoadedSource {
    pub(super) image: Arc<image::DynamicImage>,
    pub(super) version_token: String,
}

/// resolves the version token for an object.
///
/// checks cache first, then fetches from object storage if needed.
/// falls back to `unknown_version_token` on fetch failure.
pub(super) async fn resolve_version_token(
    source: &ObjectStorageSource,
    cache: &ExternalCache,
    object_key: &str,
    unknown_version_token: &str,
) -> String {
    if let Some(version) = cache.get_cached_version(object_key).await {
        return version;
    }

    let lookup_start = Instant::now();
    let fetched = source.fetch_object_version(object_key).await;
    metrics::record_cache_get_latency(ExternalCacheLayer::Version, lookup_start.elapsed());

    match fetched {
        Ok(Some(version)) => {
            cache.set_cached_version(object_key, &version).await;
            version
        }
        Ok(None) => {
            metrics::record_cache_miss(ExternalCacheLayer::Version);
            unknown_version_token.to_string()
        }
        Err(_) => {
            metrics::record_cache_error(ExternalCacheLayer::Version);
            unknown_version_token.to_string()
        }
    }
}

/// loads source image from cache or object storage with distributed locking.
///
/// concurrency control flow:
/// 1. check source cache for existing bytes (fast path)
/// 2. try acquire distributed lock for this (`object_key`, version) pair
/// 3. if lock held, wait for source to appear (another request fetching)
/// 4. if lock acquired, check cache again (race condition handling)
/// 5. fetch from object storage with retry logic
/// 6. cache fetched bytes and release lock
/// 7. decode image bytes to `DynamicImage`
///
/// lock ensures single fetch per (`object_key`, version) even under thundering herd.
/// heartbeat task extends lock during slow fetches to prevent premature expiration.
#[allow(
    clippy::too_many_lines,
    clippy::large_futures,
    reason = "fetch/decode lock flow is intentionally linear for correctness and hot-path traceability"
)]
pub(super) async fn load_source_image(
    source: &ObjectStorageSource,
    cache: &ExternalCache,
    object_key: &str,
    requested_version: &str,
    unknown_version_token: &str,
) -> AppResult<LoadedSource> {
    if let Some(bytes) = cache.get_source_bytes(object_key, requested_version).await {
        eprintln!("[SOURCE-CACHE] HIT: object_key={}", object_key);
        if let Ok(image) = decode_source_image(bytes).await {
            return Ok(LoadedSource {
                image,
                version_token: requested_version.to_string(),
            });
        }
        cache
            .delete_source_bytes(object_key, requested_version)
            .await;
    } else {
        eprintln!("[SOURCE-CACHE] MISS: object_key={}", object_key);
    }

    let source_lock_key = cache.build_source_lock_key(object_key, requested_version);
    let source_lock_token = cache.try_acquire_lock(&source_lock_key).await;
    let mut source_lock_heartbeat = source_lock_token.as_ref().map(|token| {
        locks::spawn_lock_heartbeat(cache.clone(), source_lock_key.clone(), token.clone())
    });

    if source_lock_token.is_none()
        && let Some(bytes) = cache.wait_for_source(object_key, requested_version).await
    {
        if let Ok(image) = decode_source_image(bytes).await {
            return Ok(LoadedSource {
                image,
                version_token: requested_version.to_string(),
            });
        }
        cache
            .delete_source_bytes(object_key, requested_version)
            .await;
    }

    if source_lock_token.is_some()
        && let Some(bytes) = cache.get_source_bytes(object_key, requested_version).await
    {
        if let Ok(image) = decode_source_image(bytes).await {
            if let Some(token) = &source_lock_token {
                release_source_lock(cache, &source_lock_key, token, &mut source_lock_heartbeat)
                    .await;
            }
            return Ok(LoadedSource {
                image,
                version_token: requested_version.to_string(),
            });
        }
        cache
            .delete_source_bytes(object_key, requested_version)
            .await;
    }

    let version_hint = if requested_version == unknown_version_token {
        None
    } else {
        Some(requested_version)
    };

    let fetch_start = Instant::now();
    let object = source.fetch_source_object(object_key, version_hint).await;
    metrics::record_stage_latency(PipelineStage::ObjectFetch, fetch_start.elapsed());
    let object = match object {
        Ok(object) => object,
        Err(err) => {
            if let Some(token) = &source_lock_token {
                release_source_lock(cache, &source_lock_key, token, &mut source_lock_heartbeat)
                    .await;
            }
            return Err(err);
        }
    };

    let version_token = object
        .version_token
        .unwrap_or_else(|| requested_version.to_string());

    if version_token != unknown_version_token {
        cache.set_cached_version(object_key, &version_token).await;
    }

    if version_token != requested_version
        && version_token != unknown_version_token
        && let Some(bytes) = cache.get_source_bytes(object_key, &version_token).await
    {
        if let Ok(image) = decode_source_image(bytes).await {
            if let Some(token) = &source_lock_token {
                release_source_lock(cache, &source_lock_key, token, &mut source_lock_heartbeat)
                    .await;
            }
            return Ok(LoadedSource {
                image,
                version_token,
            });
        }
        cache.delete_source_bytes(object_key, &version_token).await;
    }

    cache
        .set_source_bytes(object_key, &version_token, &object.bytes)
        .await;
    if version_token != requested_version {
        cache
            .set_source_bytes(object_key, requested_version, &object.bytes)
            .await;
    }

    let decode_result = decode_source_image(object.bytes).await;

    if let Some(token) = &source_lock_token {
        release_source_lock(cache, &source_lock_key, token, &mut source_lock_heartbeat).await;
    }

    let image = decode_result?;

    Ok(LoadedSource {
        image,
        version_token,
    })
}

/// decodes image bytes into a `DynamicImage`.
///
/// validates dimensions against configured limits before decoding.
/// runs in `spawn_blocking` to avoid blocking the async runtime.
async fn decode_source_image(bytes: Vec<u8>) -> AppResult<Arc<image::DynamicImage>> {
    let limits = decode_limits();
    metrics::record_blocking_task_started();
    let decode_start = Instant::now();
    let decoded = tokio::task::spawn_blocking(move || {
        metrics::record_blocking_thread_usage();
        let reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|_| "unsupported image format".to_string())?;
        let decoder = reader
            .into_decoder()
            .map_err(|_| "unsupported image format".to_string())?;
        let dimensions = image::ImageDecoder::dimensions(&decoder);
        validate_decode_bounds(dimensions.0, dimensions.1, limits)?;
        image::DynamicImage::from_decoder(decoder)
            .map_err(|_| "unsupported image format".to_string())
    })
    .await
    .map_err(|_| "failed to decode source image".to_string())??;
    metrics::record_stage_latency(PipelineStage::Decode, decode_start.elapsed());

    Ok(Arc::new(decoded))
}

/// releases a source lock and aborts its heartbeat.
///
/// idempotent: safe to call multiple times.
async fn release_source_lock(
    cache: &ExternalCache,
    lock_key: &str,
    token: &str,
    heartbeat: &mut Option<JoinHandle<()>>,
) {
    if let Some(handle) = heartbeat.take() {
        handle.abort();
    }
    cache.release_lock(lock_key, token).await;
}
