use crate::utils::metrics::render_prometheus;
use axum::{
    http::{StatusCode, header::CONTENT_TYPE},
    response::IntoResponse,
};

pub async fn metrics() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        render_prometheus(),
    )
}
