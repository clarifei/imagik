use std::net::SocketAddr;
use std::time::Duration;

mod aliases;
mod handlers;
mod models;
mod routes;
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

struct RuntimeConfig {
    max_blocking_threads: usize,
    blocking_keep_alive_ms: u64,
    rss_sample_interval_ms: u64,
}

impl RuntimeConfig {
    fn from_env() -> Self {
        let cpu = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let max_blocking_threads = parse_env_usize(
            "IMAGIK_MAX_BLOCKING_THREADS",
            cpu.saturating_mul(2).max(1),
            1,
        );
        let blocking_keep_alive_ms =
            parse_env_u64("IMAGIK_BLOCKING_KEEP_ALIVE_MS", 3_000, 250).max(250);
        let rss_sample_interval_ms =
            parse_env_u64("IMAGIK_RSS_SAMPLE_INTERVAL_MS", 1_000, 100).max(100);

        Self {
            max_blocking_threads,
            blocking_keep_alive_ms,
            rss_sample_interval_ms,
        }
    }
}

fn parse_env_usize(key: &str, default: usize, min: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v >= min)
        .unwrap_or(default)
}

fn parse_env_u64(key: &str, default: u64, min: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v >= min)
        .unwrap_or(default)
}

/// entry point for the imagik server.
///
/// quick overview of the flow:
/// - sets up axum routes on port 3000
/// - prints available params (mostly for dev convenience)
/// - serves image transformation requests
///
/// note: currently hardcoded to use `image.jpg` as the source image.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime_config = RuntimeConfig::from_env();
    let transform_concurrency = handlers::transform::configured_transform_concurrency();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(runtime_config.max_blocking_threads)
        .thread_keep_alive(Duration::from_millis(runtime_config.blocking_keep_alive_ms))
        .build()?;

    runtime.block_on(async_main(runtime_config, transform_concurrency))
}

async fn async_main(
    runtime_config: RuntimeConfig,
    transform_concurrency: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = routes::create_routes();

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("imagik server");
    println!("http://{}", addr);
    println!(
        "runtime: max_blocking_threads={}, transform_concurrency={}, blocking_keep_alive_ms={}, rss_sample_interval_ms={}",
        runtime_config.max_blocking_threads,
        transform_concurrency,
        runtime_config.blocking_keep_alive_ms,
        runtime_config.rss_sample_interval_ms
    );
    println!();
    println!("params:");
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
    println!("  debug            - show timing overlay");
    println!();
    println!("example: curl http://{}/w_500,h_500,c_fill,wq_80", addr);
    println!("metrics: curl http://{}/metrics", addr);

    utils::metrics::set_runtime_limits(
        runtime_config.max_blocking_threads,
        transform_concurrency,
        runtime_config.blocking_keep_alive_ms,
        runtime_config.rss_sample_interval_ms,
    );
    utils::metrics::start_rss_sampler(Duration::from_millis(runtime_config.rss_sample_interval_ms));

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
