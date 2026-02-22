//! environment variable parsing utilities.
//!
//! provides typed parsing with defaults and minimum value enforcement.

use std::env;

/// parses an environment variable as usize with default and minimum.
pub fn parse_usize(key: &str, default: usize, min: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v >= min)
        .unwrap_or(default)
}

/// parses an environment variable as u64 with default and minimum.
pub fn parse_u64(key: &str, default: u64, min: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v >= min)
        .unwrap_or(default)
}
