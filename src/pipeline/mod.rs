//! request pipeline: query parsing → normalized params → transform execution.
//!
//! stages:
//! 1. `keys`: query parameter name normalization (aliases → canonical keys)
//! 2. `query`: query string parsing with validation
//! 3. `params`: normalized parameter struct with cache signature generation
//! 4. `runner`: image transform application and encoding
//!
//! cache signature (`Params::cache_signature`):
//! - deterministic regardless of param order or alias usage
//! - used for result cache key construction
//! - includes all transform parameters for cache correctness

pub mod keys;
pub mod params;
pub mod query;
pub mod runner;
