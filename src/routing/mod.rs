use crate::handlers::image::{transform_image, transform_image_missing_key};
use crate::handlers::metrics::metrics;
use axum::{Router, routing::get};

/// creates the axum router with all image transformation routes.
///
/// routes:
/// - `/metrics` : runtime and transform metrics
/// - `/image/{*object_key}` : transform object-storage image by key
pub fn create_routes() -> Router {
    Router::new()
        .route("/metrics", get(metrics))
        .route("/image", get(transform_image_missing_key))
        .route("/image/{*object_key}", get(transform_image))
}
