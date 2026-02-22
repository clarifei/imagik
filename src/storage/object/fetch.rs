use super::client::SourceMode;
use super::key::{build_signed_url, extract_version_headers, version_from_parts};
use super::retry::{backoff_delay, is_retryable_status};
use super::{ObjectStorageSource, SourceObject};
use crate::common::types::AppResult;
use crate::observability::metrics;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use tokio::io::AsyncReadExt;

use std::time::Instant;

impl ObjectStorageSource {
    /// fetches object version metadata without retrieving body.
    ///
    /// `signed_url` mode: issues HEAD request, extracts version from headers.
    /// `s3` mode: calls head_object API, extracts version_id/e_tag/last_modified.
    ///
    /// retry logic: retries on transient failures up to `retry_attempts`.
    /// backoff: exponential with `retry_backoff` base delay.
    #[allow(
        clippy::large_futures,
        reason = "returning owned bytes keeps object path single-pass without extra boxing/indirection"
    )]
    pub async fn fetch_object_version(&self, object_key: &str) -> AppResult<Option<String>> {
        let storage_key = self.storage_key(object_key);
        eprintln!("[DEBUG] fetch_object_version: object_key={}, storage_key={}", object_key, storage_key);

        match &self.mode {
            SourceMode::SignedUrl { client, template } => {
                let url = build_signed_url(template, &storage_key);
                let headers = self.fetch_signed_url_headers(client, &url).await?;
                let (version_token, _) = extract_version_headers(&headers);
                Ok(version_token)
            }
            SourceMode::S3 { client, bucket } => {
                let mut last_error = String::new();

                for attempt in 0..=self.retry_attempts {
                    match client
                        .head_object()
                        .bucket(bucket)
                        .key(&storage_key)
                        .send()
                        .await
                    {
                        Ok(output) => {
                            return Ok(version_from_parts(
                                output.version_id(),
                                output.e_tag(),
                                output.last_modified().map(ToString::to_string),
                            ));
                        }
                        Err(e) => {
                            eprintln!("[DEBUG] S3 head_object error (attempt {}): {:?}", attempt, e);
                            last_error = format!("s3 head_object request failed: {}", e);
                        }
                    }
                    if attempt < self.retry_attempts {
                        metrics::record_storage_retry();
                        tokio::time::sleep(backoff_delay(self.retry_backoff, attempt))
                            .await;
                    }
                }

                Err(last_error)
            }
        }
    }

    #[allow(
        clippy::large_futures,
        reason = "Returning owned fetched bytes intentionally keeps the object path single-pass without extra boxing/indirection."
    )]
    pub async fn fetch_source_object(
        &self,
        object_key: &str,
        version_hint: Option<&str>,
    ) -> AppResult<SourceObject> {
        let storage_key = self.storage_key(object_key);
        let cache_key = version_hint.map_or_else(
            || format!("{}:{storage_key}", self.cache_namespace),
            |version| format!("{}:{storage_key}:{version}", self.cache_namespace),
        );

        if let Some(cache) = &self.cache
            && let Some(bytes) = cache.try_read(&cache_key).await
        {
            return Ok(SourceObject {
                bytes,
                version_token: version_hint.map(str::to_owned),
            });
        }

        let object = match &self.mode {
            SourceMode::SignedUrl { client, template } => {
                self.fetch_signed_url_object(client, template, &storage_key)
                    .await?
            }
            SourceMode::S3 { client, bucket } => {
                self.fetch_s3_object(client, bucket, &storage_key).await?
            }
        };

        if let Some(cache) = &self.cache {
            cache.write(&cache_key, &object.bytes).await;
        }

        Ok(object)
    }

    /// fetches headers from a signed url for version detection.
    ///
    /// returns empty headers on `405 method not allowed` (some storage providers).
    async fn fetch_signed_url_headers(
        &self,
        client: &reqwest::Client,
        url: &str,
    ) -> AppResult<HeaderMap> {
        let mut last_error = String::new();

        for attempt in 0..=self.retry_attempts {
            if let Ok(response) = client.head(url).send().await {
                if response.status().is_success() {
                    return Ok(response.headers().clone());
                }

                if response.status() == StatusCode::METHOD_NOT_ALLOWED {
                    return Ok(HeaderMap::new());
                }

                last_error = format!(
                    "signed-url version lookup failed with status {}",
                    response.status()
                );
                if attempt < self.retry_attempts && is_retryable_status(response.status()) {
                    metrics::record_storage_retry();
                    tokio::time::sleep(backoff_delay(self.retry_backoff, attempt)).await;
                }
            } else {
                last_error = "signed-url version lookup network error".to_string();
                if attempt < self.retry_attempts {
                    metrics::record_storage_retry();
                    tokio::time::sleep(backoff_delay(self.retry_backoff, attempt)).await;
                }
            }
        }

        Err(last_error)
    }

    /// fetches object bytes from a signed url.
    ///
    /// streams response body with size limit enforcement.
    /// extracts version headers for cache invalidation.
    async fn fetch_signed_url_object(
        &self,
        client: &reqwest::Client,
        template: &str,
        object_key: &str,
    ) -> AppResult<SourceObject> {
        let url = build_signed_url(template, object_key);
        let mut last_error = String::new();

        for attempt in 0..=self.retry_attempts {
            if let Ok(mut response) = client.get(&url).send().await {
                if response.status().is_success() {
                    let headers = response.headers().clone();
                    let (version_token, _) = extract_version_headers(&headers);
                    let bytes = read_http_body_streaming(
                        &mut response,
                        self.max_object_bytes,
                        "signed-url response",
                    )
                    .await?;
                    return Ok(SourceObject {
                        bytes,
                        version_token,
                    });
                }

                last_error = format!(
                    "signed-url request failed with status {}",
                    response.status()
                );
                if attempt < self.retry_attempts && is_retryable_status(response.status()) {
                    metrics::record_storage_retry();
                    tokio::time::sleep(backoff_delay(self.retry_backoff, attempt)).await;
                }
                return Err(last_error);
            }
            last_error = "signed-url request failed due to network error".to_string();
            if attempt < self.retry_attempts {
                metrics::record_storage_retry();
                tokio::time::sleep(backoff_delay(self.retry_backoff, attempt)).await;
            }
        }

        Err(last_error)
    }

    /// fetches object bytes from s3.
    ///
    /// validates content-length before streaming.
    /// aborts if object exceeds `max_object_bytes`.
    async fn fetch_s3_object(
        &self,
        client: &aws_sdk_s3::Client,
        bucket: &str,
        key: &str,
    ) -> AppResult<SourceObject> {
        let start = Instant::now();
        eprintln!("[DEBUG] fetch_s3_object: bucket={}, key={}", bucket, key);
        let output = match client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
        {
            Ok(output) => output,
            Err(e) => {
                eprintln!("[DEBUG] S3 get_object error: {:?}", e);
                return Err(format!("s3 get_object request failed: {}", e));
            }
        };
        let content_length = output.content_length();

        if let Some(length) = content_length {
            let max_bytes_i64 = i64::try_from(self.max_object_bytes).unwrap_or(i64::MAX);
            if length > max_bytes_i64 {
                return Err(format!(
                    "object exceeded max configured size ({} bytes)",
                    self.max_object_bytes
                ));
            }
        }

        let version_token = version_from_parts(
            output.version_id(),
            output.e_tag(),
            output.last_modified().map(ToString::to_string),
        );

        let mut reader = output.body.into_async_read();
        let mut bytes = content_length.map_or_else(Vec::new, |length| {
            let len = usize::try_from(length).unwrap_or(usize::MAX);
            Vec::with_capacity(len.min(self.max_object_bytes))
        });
        let mut chunk = [0u8; 16 * 1024];

        loop {
            let read = reader
                .read(&mut chunk)
                .await
                .map_err(|_| "failed while streaming object storage response".to_string())?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            if bytes.len() > self.max_object_bytes {
                return Err(format!(
                    "object exceeded max configured size ({} bytes)",
                    self.max_object_bytes
                ));
            }
        }

        let duration = start.elapsed();
        eprintln!("[S3-FETCH] Downloaded {} bytes in {:?}", bytes.len(), duration);

        Ok(SourceObject {
            bytes,
            version_token,
        })
    }
}

/// streams http response body into a buffer with size limit.
///
/// pre-allocates buffer from content-length header when available.
/// aborts with error if body exceeds `max_bytes`.
async fn read_http_body_streaming(
    response: &mut reqwest::Response,
    max_bytes: usize,
    source_name: &str,
) -> AppResult<Vec<u8>> {
    let mut bytes = response.content_length().map_or_else(Vec::new, |length| {
        let len = usize::try_from(length).unwrap_or(usize::MAX);
        Vec::with_capacity(len.min(max_bytes))
    });

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| format!("failed while streaming {source_name}"))?
    {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > max_bytes {
            return Err(format!(
                "object exceeded max configured size ({max_bytes} bytes)"
            ));
        }
    }

    Ok(bytes)
}
