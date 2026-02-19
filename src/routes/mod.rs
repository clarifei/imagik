use crate::handlers::metrics::metrics;
use crate::handlers::transform::transform_image;
use axum::{Router, routing::get};

/// creates the axum router with all image transformation routes.
///
/// routes:
/// - `/` : transforms with no params (just converts to webp)
/// - `/{*params}` : transforms with params like `w_500,h_300,c_fill`
///
/// the catch-all route lets us use the path itself as the param string,
/// which looks cleaner than query params for this use case.
///
/// example: `/w_500,h_300,c_fill` vs `/?w=500&h=300&c=fill`
pub fn create_routes() -> Router {
    Router::new()
        .route("/metrics", get(metrics))
        .route("/", get(transform_image))
        .route("/{*params}", get(transform_image))
}
