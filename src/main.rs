use std::net::SocketAddr;
use std::time::Duration;

mod caching;
mod common;
mod config;
mod encoding;
mod handlers;
mod models;
mod observability;
mod pipeline;
mod routing;
mod storage;
mod transforms;
mod utils;

#[cfg(all(feature = "mimalloc", feature = "jemalloc"))]
compile_error!("enable only one allocator feature: either `mimalloc` or `jemalloc`");

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// entry point for the imagik server.
///
/// overview:
/// - sets up axum routes on port 3000
/// - prints available params (dev convenience)
/// - serves image transformation requests
fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let runtime_config = config::runtime::RuntimeConfig::from_env();
    let transform_concurrency = config::runtime::configured_transform_concurrency();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(runtime_config.max_blocking_threads)
        .thread_keep_alive(Duration::from_millis(runtime_config.blocking_keep_alive_ms))
        .build()?;

    runtime.block_on(async_main(runtime_config, transform_concurrency))
}

async fn async_main(
    runtime_config: config::runtime::RuntimeConfig,
    transform_concurrency: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = routing::create_routes();

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("imagik server");
    println!("http://{addr}");
    println!(
        "runtime: max_blocking_threads={}, transform_concurrency={}, blocking_keep_alive_ms={}, rss_sample_interval_ms={}",
        runtime_config.max_blocking_threads,
        transform_concurrency,
        runtime_config.blocking_keep_alive_ms,
        runtime_config.rss_sample_interval_ms
    );
    println!();
    println!("transform query params:");
    println!("  w/h              - width/height");
    println!("  c                - crop: fill, fit, scale, pad");
    println!("  g                - gravity: center, n, s, e, w");
    println!("  ar               - aspect ratio (16:9)");
    println!("  r                - rotate: 0, 90, 180, 270");
    println!("  b                - background hex color");
    println!("  blur             - blur sigma (2.5)");
    println!("  wq               - webp quality 0-100");
    println!("  wl               - lossless mode");
    println!("  brightness       - -100 to 100");
    println!("  contrast         - -100 to 100");
    println!("  hue              - 0-360");
    println!("  grayscale        - true/false");
    println!("  invert           - true/false");
    println!("  saturation       - -100 to 100");
    println!("  vibrance         - -100 to 100");
    println!("  grain            - 0-100");
    println!("  graingray        - grayscale grain mode");
    println!("  grainthresh      - black threshold 0-1 (default 0.08)");
    println!("  ca               - chromatic aberration 0.0-0.1");
    println!("  format           - output format (webp only)");
    println!("  debug            - show timing overlay");
    println!();
    println!("example: curl \"http://{addr}/image/photos/deer.jpg?w=500&h=500&c=fill&wq=80\"");
    println!(
        "source: object storage via IMAGIK_STORAGE_* env vars (S3-compatible or signed-url template)"
    );
    println!(
        "cache: Redis/Dragonfly via IMAGIK_CACHE_URL (+ optional IMAGIK_HOT_CACHE_* in-process LRU)"
    );
    println!("metrics: curl http://{addr}/metrics");

    observability::metrics::set_runtime_limits(
        runtime_config.max_blocking_threads,
        transform_concurrency,
        runtime_config.blocking_keep_alive_ms,
        runtime_config.rss_sample_interval_ms,
    );
    observability::metrics::start_rss_sampler(Duration::from_millis(
        runtime_config.rss_sample_interval_ms,
    ));

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
