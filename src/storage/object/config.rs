//! object storage configuration from environment variables.
//!
//! supports legacy `IMAGIK_OBJECT_*` and modern `IMAGIK_STORAGE_*` prefixes.
//! auto-detects mode based on configured variables (signed url template or s3 bucket).

use super::key::normalize_key_prefix;
use crate::common::types::AppResult;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 1_500;
const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_RETRY_BACKOFF_MS: u64 = 150;
const DEFAULT_MAX_OBJECT_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_CACHE_TTL_SECS: u64 = 600;
const DEFAULT_CACHE_MAX_BYTES: usize = 512 * 1024 * 1024;

/// object storage configuration parsed from environment.
pub(super) struct StorageSettings {
    pub(super) retry_attempts: u32,
    pub(super) retry_backoff: Duration,
    pub(super) max_object_bytes: usize,
    pub(super) cache: Option<DiskCacheSettings>,
    pub(super) key_prefix: Option<String>,
    pub(super) mode: ModeSettings,
}

/// local disk cache configuration for source bytes.
pub(super) struct DiskCacheSettings {
    pub(super) dir: PathBuf,
    pub(super) ttl: Duration,
    pub(super) max_bytes: usize,
}

/// storage access mode configuration.
pub(super) enum ModeSettings {
    SignedUrl {
        template: String,
        timeout: Duration,
        connect_timeout: Duration,
    },
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
        force_path_style: bool,
        connect_timeout: Duration,
        timeout: Duration,
        credentials: Option<StaticCredentials>,
    },
}

/// aws-style credentials for s3 access.
pub(super) struct StaticCredentials {
    pub(super) access_key: String,
    pub(super) secret_key: String,
    pub(super) session_token: Option<String>,
}

impl StorageSettings {
    /// parses storage configuration from environment variables.
    ///
    /// checks both legacy `IMAGIK_OBJECT_*` and modern `IMAGIK_STORAGE_*` prefixes.
    pub(super) fn from_env() -> AppResult<Self> {
        eprintln!("[OBJECT-STORAGE] Parsing storage settings from environment...");
        let retry_attempts = parse_env_u32_any(
            &["IMAGIK_STORAGE_MAX_RETRIES", "IMAGIK_OBJECT_MAX_RETRIES"],
            DEFAULT_MAX_RETRIES,
        )
        .max(1);
        let retry_backoff = Duration::from_millis(parse_env_u64_any(
            &[
                "IMAGIK_STORAGE_RETRY_BACKOFF_MS",
                "IMAGIK_OBJECT_RETRY_BACKOFF_MS",
            ],
            DEFAULT_RETRY_BACKOFF_MS,
        ));
        let max_object_bytes = parse_env_usize_any(
            &["IMAGIK_STORAGE_MAX_OBJECT_BYTES", "IMAGIK_OBJECT_MAX_BYTES"],
            DEFAULT_MAX_OBJECT_BYTES,
        )
        .max(1024);

        let cache =
            parse_env_optional_path_any(&["IMAGIK_STORAGE_CACHE_DIR", "IMAGIK_OBJECT_CACHE_DIR"])
                .map(|dir| DiskCacheSettings {
                    dir,
                    ttl: Duration::from_secs(parse_env_u64_any(
                        &[
                            "IMAGIK_STORAGE_CACHE_TTL_SECS",
                            "IMAGIK_OBJECT_CACHE_TTL_SECS",
                        ],
                        DEFAULT_CACHE_TTL_SECS,
                    )),
                    max_bytes: parse_env_usize_any(
                        &[
                            "IMAGIK_STORAGE_CACHE_MAX_BYTES",
                            "IMAGIK_OBJECT_CACHE_MAX_BYTES",
                        ],
                        DEFAULT_CACHE_MAX_BYTES,
                    ),
                });

        let key_prefix = parse_env_optional_string_any(&["IMAGIK_STORAGE_KEY_PREFIX"])
            .and_then(|value| normalize_key_prefix(&value));

        let mode = parse_mode()?;

        Ok(Self {
            retry_attempts,
            retry_backoff,
            max_object_bytes,
            cache,
            key_prefix,
            mode,
        })
    }
}

/// parses storage mode from environment (signed url or s3).
///
/// auto-detects based on available configuration.
fn parse_mode() -> AppResult<ModeSettings> {
    let mode = parse_env_optional_string_any(&["IMAGIK_STORAGE_MODE"])
        .unwrap_or_else(|| "auto".to_string())
        .to_ascii_lowercase();
    eprintln!("[OBJECT-STORAGE] Storage mode from env: {}", mode);

    let signed_template = parse_env_optional_string_any(&[
        "IMAGIK_STORAGE_SIGNED_URL_TEMPLATE",
        "IMAGIK_OBJECT_SIGNED_URL",
    ]);

    let use_signed_url = match mode.as_str() {
        "signed" | "signed_url" | "signed-url" => true,
        "s3" => false,
        "auto" => signed_template.is_some(),
        _ => {
            return Err(
                "invalid IMAGIK_STORAGE_MODE (expected auto, s3, or signed_url)".to_string(),
            );
        }
    };
    eprintln!("[OBJECT-STORAGE] Using signed URL mode: {}", use_signed_url);

    if use_signed_url {
        let template = signed_template.ok_or_else(|| {
            "missing signed-url config: set IMAGIK_STORAGE_SIGNED_URL_TEMPLATE".to_string()
        })?;
        let timeout = Duration::from_millis(parse_env_u64_any(
            &["IMAGIK_STORAGE_TIMEOUT_MS", "IMAGIK_OBJECT_TIMEOUT_MS"],
            DEFAULT_TIMEOUT_MS,
        ));
        let connect_timeout = Duration::from_millis(parse_env_u64_any(
            &[
                "IMAGIK_STORAGE_CONNECT_TIMEOUT_MS",
                "IMAGIK_OBJECT_CONNECT_TIMEOUT_MS",
            ],
            DEFAULT_CONNECT_TIMEOUT_MS,
        ));

        return Ok(ModeSettings::SignedUrl {
            template,
            timeout,
            connect_timeout,
        });
    }

    let bucket = parse_env_optional_string_any(&["IMAGIK_STORAGE_BUCKET", "IMAGIK_OBJECT_BUCKET"])
        .ok_or_else(|| {
            "missing object storage config: set IMAGIK_STORAGE_BUCKET (or IMAGIK_OBJECT_BUCKET)"
                .to_string()
        })?;
    let region = parse_env_optional_string_any(&["IMAGIK_STORAGE_REGION", "IMAGIK_OBJECT_REGION"])
        .unwrap_or_else(|| "auto".to_string());
    let endpoint =
        parse_env_optional_string_any(&["IMAGIK_STORAGE_ENDPOINT", "IMAGIK_OBJECT_ENDPOINT"]);
    let force_path_style = parse_env_bool_any(
        &[
            "IMAGIK_STORAGE_FORCE_PATH_STYLE",
            "IMAGIK_OBJECT_FORCE_PATH_STYLE",
        ],
        true,
    );

    let connect_timeout = Duration::from_millis(parse_env_u64_any(
        &[
            "IMAGIK_STORAGE_CONNECT_TIMEOUT_MS",
            "IMAGIK_OBJECT_CONNECT_TIMEOUT_MS",
        ],
        DEFAULT_CONNECT_TIMEOUT_MS,
    ));
    let timeout = Duration::from_millis(parse_env_u64_any(
        &["IMAGIK_STORAGE_TIMEOUT_MS", "IMAGIK_OBJECT_TIMEOUT_MS"],
        DEFAULT_TIMEOUT_MS,
    ));

    let access_key = parse_env_optional_string_any(&[
        "IMAGIK_STORAGE_ACCESS_KEY_ID",
        "IMAGIK_OBJECT_ACCESS_KEY_ID",
    ]);
    let secret_key = parse_env_optional_string_any(&[
        "IMAGIK_STORAGE_SECRET_ACCESS_KEY",
        "IMAGIK_OBJECT_SECRET_ACCESS_KEY",
    ]);
    let session_token = parse_env_optional_string_any(&[
        "IMAGIK_STORAGE_SESSION_TOKEN",
        "IMAGIK_OBJECT_SESSION_TOKEN",
    ]);

    eprintln!(
        "[OBJECT-STORAGE] S3 config: bucket={}, region={}, endpoint={:?}, force_path_style={}",
        bucket, region, endpoint, force_path_style
    );
    eprintln!(
        "[OBJECT-STORAGE] Credentials configured: {}",
        if access_key.is_some() && secret_key.is_some() {
            "yes"
        } else {
            "no (will use default chain)"
        }
    );

    let credentials = match (access_key, secret_key) {
        (Some(access), Some(secret)) => Some(StaticCredentials {
            access_key: access,
            secret_key: secret,
            session_token,
        }),
        (None, None) => None,
        _ => {
            return Err(
                "invalid object storage credentials: both access key and secret are required"
                    .to_string(),
            );
        }
    };

    Ok(ModeSettings::S3 {
        bucket,
        region,
        endpoint,
        force_path_style,
        connect_timeout,
        timeout,
        credentials,
    })
}

fn parse_env_u32_any(keys: &[&str], default: u32) -> u32 {
    parse_env_optional_string_any(keys)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(default)
}

fn parse_env_u64_any(keys: &[&str], default: u64) -> u64 {
    parse_env_optional_string_any(keys)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

fn parse_env_usize_any(keys: &[&str], default: usize) -> usize {
    parse_env_optional_string_any(keys)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn parse_env_optional_path_any(keys: &[&str]) -> Option<PathBuf> {
    parse_env_optional_string_any(keys).and_then(|value| {
        let path = Path::new(value.trim());
        if path.as_os_str().is_empty() {
            None
        } else {
            Some(path.to_path_buf())
        }
    })
}

fn parse_env_bool_any(keys: &[&str], default: bool) -> bool {
    parse_env_optional_string_any(keys).map_or(default, |value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn parse_env_optional_string_any(keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Ok(value) = env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}
