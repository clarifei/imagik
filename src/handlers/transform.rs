use crate::handlers::parser::parse_params;
use crate::handlers::pipeline::apply_transforms_and_convert;
use crate::utils::metrics::{self, TransformInFlightGuard};
use axum::{extract::Path, http::StatusCode, response::IntoResponse, response::Response};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use tokio::fs;
use tokio::sync::{OnceCell, Semaphore};

/// simple in-memory cache for the source image.
///
/// currently just holds a single decoded image (image.jpg) forever.
/// cuts out both disk io and decode work on every request.
static IMAGE_CACHE: OnceCell<Arc<image::DynamicImage>> = OnceCell::const_new();
static TRANSFORM_CONCURRENCY: OnceLock<usize> = OnceLock::new();
static TRANSFORM_SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();

const DEFAULT_TRANSFORM_CONCURRENCY_MULTIPLIER: usize = 2;
const MIN_TRANSFORM_CONCURRENCY: usize = 1;

pub(crate) fn configured_transform_concurrency() -> usize {
    *TRANSFORM_CONCURRENCY.get_or_init(|| {
        let cpu = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let default_limit = cpu.saturating_mul(DEFAULT_TRANSFORM_CONCURRENCY_MULTIPLIER);
        std::env::var("IMAGIK_TRANSFORM_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v >= MIN_TRANSFORM_CONCURRENCY)
            .unwrap_or(default_limit.max(MIN_TRANSFORM_CONCURRENCY))
    })
}

fn transform_semaphore() -> Arc<Semaphore> {
    Arc::clone(
        TRANSFORM_SEMAPHORE
            .get_or_init(|| Arc::new(Semaphore::new(configured_transform_concurrency()))),
    )
}

/// main http handler for image transformations.
///
/// flow:
/// 1. parse url params (returns 400 if invalid)
/// 2. load source image (from cache or disk)
/// 3. spawn blocking task for cpu-heavy image processing
/// 4. return webp response or appropriate error
///
/// why spawn_blocking? image processing is cpu-intensive and would
/// block the async runtime. tokio's spawn_blocking moves it to a
/// separate thread pool so we don't stall other requests.
pub async fn transform_image(params: Option<Path<String>>) -> impl IntoResponse {
    let request_start = Instant::now();
    let raw_params = params.as_ref().map_or("", |path| path.0.as_str());

    let parsed = match parse_params(raw_params) {
        Ok(p) => p,
        Err(e) => return finalize_response((StatusCode::BAD_REQUEST, e), request_start, false),
    };

    let source_image = match load_image().await {
        Ok(data) => data,
        Err(_) => {
            return finalize_response(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to load source image",
                ),
                request_start,
                false,
            );
        }
    };

    let queue_start = Instant::now();
    let permit = match transform_semaphore().acquire_owned().await {
        Ok(permit) => permit,
        Err(_) => {
            return finalize_response(
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "transform concurrency limiter unavailable",
                ),
                request_start,
                false,
            );
        }
    };
    metrics::record_queue_wait(queue_start.elapsed());

    let _in_flight = TransformInFlightGuard::new();
    metrics::record_blocking_task_started();
    let processing_start = Instant::now();

    let result_buffer = match tokio::task::spawn_blocking(move || {
        metrics::record_blocking_thread_usage();
        apply_transforms_and_convert(source_image, parsed)
    })
    .await
    {
        Ok(result) => result,
        Err(_) => {
            metrics::record_processing_latency(processing_start.elapsed());
            drop(permit);
            return finalize_response(
                (StatusCode::INTERNAL_SERVER_ERROR, "failed to process image"),
                request_start,
                false,
            );
        }
    };
    metrics::record_processing_latency(processing_start.elapsed());
    drop(permit);

    match result_buffer {
        Ok(buffer) => finalize_response(
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "image/webp")],
                buffer,
            ),
            request_start,
            true,
        ),
        Err(e) => finalize_response((StatusCode::INTERNAL_SERVER_ERROR, e), request_start, false),
    }
}

fn finalize_response<R: IntoResponse>(
    response: R,
    request_start: Instant,
    success: bool,
) -> Response {
    metrics::record_request_finished(request_start.elapsed(), success);
    response.into_response()
}

/// loads the source image from cache or disk.
///
/// `OnceCell` gives us cheap async-safe initialization — first call
/// reads from disk, subsequent calls get the cached Arc clone.
///
/// todo: make the source path configurable, support multiple images.
async fn load_image() -> Result<Arc<image::DynamicImage>, String> {
    let cached = IMAGE_CACHE
        .get_or_try_init(|| async {
            let data = fs::read("image.jpg")
                .await
                .map_err(|_| "failed to read image.jpg".to_string())?;
            metrics::record_blocking_task_started();
            let decoded = tokio::task::spawn_blocking(move || {
                metrics::record_blocking_thread_usage();
                image::load_from_memory(&data).map_err(|_| "unsupported image format".to_string())
            })
            .await
            .map_err(|_| "failed to decode source image".to_string())??;

            Ok::<Arc<image::DynamicImage>, String>(Arc::new(decoded))
        })
        .await?;

    Ok(Arc::clone(cached))
}
