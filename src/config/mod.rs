//! centralized configuration loading from environment.
//!
//! modules:
//! - `decode`: image decode limits (dimensions, memory)
//! - `env`: typed environment variable parsing
//! - `runtime`: tokio runtime configuration (blocking threads, keepalive)

pub mod decode;
pub mod env;
pub mod runtime;
