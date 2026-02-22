mod key;
mod locks;
mod source;

use crate::caching::external::ExternalCache;
use crate::common::types::AppResult;
use crate::config::runtime::configured_transform_concurrency;
use crate::observability::metrics::{self, TransformInFlightGuard};
use crate::pipeline::query::parse_query_params;
use crate::pipeline::runner::apply_transforms_and_convert;
use crate::storage::object::ObjectStorageSource;
use axum::{
    body::Bytes,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use tokio::sync::{OnceCell, Semaphore};

static SOURCE_CLIENT: OnceCell<ObjectStorageSource> = OnceCell::const_new();
static EXTERNAL_CACHE: OnceCell<ExternalCache> = OnceCell::const_new();
static TRANSFORM_SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();

const UNKNOWN_VERSION_TOKEN: &str = "unknown";

/// returns the global transform concurrency semaphore.
///
/// initialized once on first call with configured concurrency limit.
/// semaphore bounds concurrent `spawn_blocking` transform tasks
/// to prevent memory exhaustion from too many simultaneous image operations.
fn transform_semaphore() -> Arc<Semaphore> {
    Arc::clone(
        TRANSFORM_SEMAPHORE
            .get_or_init(|| Arc::new(Semaphore::new(configured_transform_concurrency()))),
    )
}

/// main HTTP handler for image transformations.
///
/// request pipeline flow:
/// 1. parse and validate query parameters
/// 2. decode and validate object key (URL percent-decoding, path traversal checks)
/// 3. resolve object version token (cache lookup → storage head request)
/// 4. check result cache (hot cache → external cache)
/// 5. acquire distributed lock for compute (deduplicates concurrent identical requests)
/// 6. wait for transform permit (bounded concurrency to protect memory/CPU)
/// 7. load source image (source cache → storage fetch → decode)
/// 8. apply transforms in blocking thread (resize, filters, effects, encode)
/// 9. store result in cache with ttl
/// 10. release lock and return response
///
/// concurrency controls:
/// - `TRANSFORM_SEMAPHORE`: limits concurrent transforms (memory/CPU protection)
/// - distributed locks: prevent duplicate work on cache miss
/// - `TransformInFlightGuard`: tracks active transforms for metrics
///
/// caching layers (checked in order):
/// - in-process hot cache (fastest, no serialization)
/// - external cache (redis/dragonfly, shared across instances)
/// - source cache (avoids repeated storage fetches)
///
/// error handling:
/// - 400: invalid parameters or object key
/// - 500: storage client initialization failure
/// - 503: concurrency limit exceeded or lock acquisition failed
#[allow(
    clippy::too_many_lines,
    clippy::large_futures,
    reason = "request orchestrator: splitting increases cross-module indirection on hot path"
)]
pub async fn transform_image(
    Path(raw_object_key): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let request_start = Instant::now();

    let parsed = match parse_query_params(&query) {
        Ok(parsed) => parsed,
        Err(error) => {
            return finalize_response((StatusCode::BAD_REQUEST, error), request_start, false);
        }
    };

    let object_key = match key::decode_and_validate_object_key(&raw_object_key) {
        Ok(object_key) => object_key,
        Err(error) => {
            return finalize_response((StatusCode::BAD_REQUEST, error), request_start, false);
        }
    };

    let Ok(source) = source_client().await else {
        return finalize_response(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to initialize object storage client",
            ),
            request_start,
            false,
        );
    };

    let cache = cache_client().await;

    eprintln!("[DEBUG] transform_image: object_key={}", object_key);
    let transform_signature = parsed.cache_signature();

    let resolved_version_token =
        source::resolve_version_token(source, cache, &object_key, UNKNOWN_VERSION_TOKEN).await;
    let unknown_version = resolved_version_token == UNKNOWN_VERSION_TOKEN;
    if unknown_version {
        metrics::record_unknown_version();
    }

    let result_key =
        cache.build_result_cache_key(&object_key, &resolved_version_token, &transform_signature);
    if let Some(cached) = lookup_final_result(cache, &result_key).await {
        return finalize_response(ok_webp(cached), request_start, true);
    }

    let result_lock_key = cache.build_result_lock_key(&result_key);
    let result_lock_token = cache.try_acquire_lock(&result_lock_key).await;

    let Some(result_lock_token) = result_lock_token else {
        if let Some(cached) = cache.wait_for_result(&result_key).await {
            return finalize_response(ok_webp(Bytes::from(cached)), request_start, true);
        }
        return finalize_response(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "transform in progress; retry shortly",
            ),
            request_start,
            false,
        );
    };

    let heartbeat = locks::spawn_result_lock_heartbeat(
        cache.clone(),
        result_lock_key.clone(),
        result_lock_token.clone(),
    );

    if let Some(cached) = lookup_final_result(cache, &result_key).await {
        heartbeat.abort();
        cache
            .release_lock(&result_lock_key, &result_lock_token)
            .await;
        return finalize_response(ok_webp(cached), request_start, true);
    }

    let queue_start = Instant::now();
    let Ok(permit) = transform_semaphore().acquire_owned().await else {
        heartbeat.abort();
        cache
            .release_lock(&result_lock_key, &result_lock_token)
            .await;
        return finalize_response(
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "transform concurrency limiter unavailable",
            ),
            request_start,
            false,
        );
    };
    metrics::record_queue_wait(queue_start.elapsed());

    let loaded_source = match source::load_source_image(
        source,
        cache,
        &object_key,
        &resolved_version_token,
        UNKNOWN_VERSION_TOKEN,
    )
    .await
    {
        Ok(source) => source,
        Err(e) => {
            eprintln!("[DEBUG] load_source_image failed: {}", e);
            drop(permit);
            heartbeat.abort();
            cache
                .release_lock(&result_lock_key, &result_lock_token)
                .await;
            return finalize_response(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to load source image: {}", e),
                ),
                request_start,
                false,
            );
        }
    };

    let effective_version_token = loaded_source.version_token.clone();
    let canonical_result_key = if effective_version_token == resolved_version_token {
        None
    } else {
        Some(cache.build_result_cache_key(
            &object_key,
            &effective_version_token,
            &transform_signature,
        ))
    };

    let _in_flight = TransformInFlightGuard::new();
    metrics::record_blocking_task_started();
    let processing_start = Instant::now();

    let source_image = loaded_source.image;
    let Ok(result_buffer) = tokio::task::spawn_blocking(move || {
        metrics::record_blocking_thread_usage();
        apply_transforms_and_convert(source_image.as_ref(), &parsed)
    })
    .await
    else {
        metrics::record_processing_latency(processing_start.elapsed());
        drop(permit);
        heartbeat.abort();
        cache
            .release_lock(&result_lock_key, &result_lock_token)
            .await;
        return finalize_response(
            (StatusCode::INTERNAL_SERVER_ERROR, "failed to process image"),
            request_start,
            false,
        );
    };
    metrics::record_processing_latency(processing_start.elapsed());
    drop(permit);

    let effective_unknown = effective_version_token == UNKNOWN_VERSION_TOKEN;
    let unknown_ttl = cache.unknown_result_ttl_secs();
    let ttl_override = if effective_unknown && unknown_ttl > 0 {
        Some(unknown_ttl)
    } else {
        None
    };
    if !effective_unknown || unknown_ttl != 0 {
        cache
            .set_result_with_ttl_override(&result_key, &result_buffer, ttl_override)
            .await;
    }

    if let Some(canonical_key) = canonical_result_key
        && canonical_key != result_key
    {
        cache
            .set_result_with_ttl_override(&canonical_key, &result_buffer, None)
            .await;
    }

    heartbeat.abort();
    cache
        .release_lock(&result_lock_key, &result_lock_token)
        .await;

    finalize_response(ok_webp(Bytes::from(result_buffer)), request_start, true)
}

pub async fn transform_image_missing_key() -> impl IntoResponse {
    (
        StatusCode::BAD_REQUEST,
        "missing object key (expected /image/{object_key})",
    )
}

/// finalizes response with metrics recording.
///
/// records total request latency and success/failure status.
/// always called exactly once per request before returning.
fn finalize_response<R: IntoResponse>(
    response: R,
    request_start: Instant,
    success: bool,
) -> Response {
    metrics::record_request_finished(request_start.elapsed(), success);
    response.into_response()
}

fn ok_webp(buffer: Bytes) -> impl IntoResponse {
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "image/webp")],
        buffer,
    )
}

async fn source_client() -> AppResult<&'static ObjectStorageSource> {
    SOURCE_CLIENT
        .get_or_try_init(|| async { ObjectStorageSource::from_env().await })
        .await
}

async fn cache_client() -> &'static ExternalCache {
    EXTERNAL_CACHE
        .get_or_init(|| async { ExternalCache::from_env().await })
        .await
}

/// looks up final transformed result in cache layers.
///
/// checks in order:
/// 1. in-process hot cache (fastest, no async/await)
/// 2. external cache (redis/dragonfly)
///
/// on external cache hit, populates hot cache for subsequent requests.
async fn lookup_final_result(cache: &ExternalCache, result_key: &str) -> Option<Bytes> {
    if let Some(bytes) = cache.get_hot_result(result_key) {
        return Some(bytes);
    }
    cache.get_result(result_key).await.map(Bytes::from)
}
