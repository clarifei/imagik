//! object storage client construction.
//!
//! supports two modes: signed url templates and s3-compatible api.
//! s3 mode uses aws sdk with configurable endpoint, region, and credentials.
//! signed url mode uses reqwest for http get requests.

use super::config::{ModeSettings, StaticCredentials, StorageSettings};
use crate::common::types::AppResult;
use aws_config::BehaviorVersion;
use aws_config::Region;
use aws_config::retry::RetryConfig;
use aws_config::timeout::TimeoutConfig;
use aws_credential_types::Credentials;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_sdk_s3::Client as S3Client;

/// storage access mode: signed urls or direct s3.
pub(super) enum SourceMode {
    SignedUrl {
        client: reqwest::Client,
        template: String,
    },
    S3 {
        client: S3Client,
        bucket: String,
    },
}

/// builds the source mode and cache namespace from settings.
///
/// returns (mode, namespace) where namespace identifies the storage backend for cache isolation.
pub(super) async fn build_mode(settings: &StorageSettings) -> AppResult<(SourceMode, String)> {
    match &settings.mode {
        ModeSettings::SignedUrl {
            template,
            timeout,
            connect_timeout,
        } => {
            let client = reqwest::Client::builder()
                .timeout(*timeout)
                .connect_timeout(*connect_timeout)
                .build()
                .map_err(|_| "failed to build signed-url http client".to_string())?;

            Ok((
                SourceMode::SignedUrl {
                    client,
                    template: template.clone(),
                },
                format!("signed:{template}"),
            ))
        }
        ModeSettings::S3 {
            bucket,
            region,
            endpoint,
            force_path_style,
            connect_timeout,
            timeout,
            credentials,
        } => {
            let timeout_config = TimeoutConfig::builder()
                .connect_timeout(*connect_timeout)
                .operation_timeout(*timeout)
                .build();
            let retry_config =
                RetryConfig::standard().with_max_attempts(settings.retry_attempts + 1);

            let mut loader = aws_config::defaults(BehaviorVersion::latest())
                .region(Region::new(region.clone()))
                .timeout_config(timeout_config)
                .retry_config(retry_config);

            if let Some(static_credentials) = credentials {
                loader = loader.credentials_provider(SharedCredentialsProvider::new(
                    build_credentials(static_credentials),
                ));
            }

            let shared = loader.load().await;
            let mut config_builder = aws_sdk_s3::config::Builder::from(&shared);
            if let Some(url) = endpoint.clone() {
                config_builder = config_builder.endpoint_url(url);
            }
            config_builder = config_builder.force_path_style(*force_path_style);
            let client = S3Client::from_conf(config_builder.build());

            Ok((
                SourceMode::S3 {
                    client,
                    bucket: bucket.clone(),
                },
                format!("s3:{}:{}", endpoint.clone().unwrap_or_default(), bucket),
            ))
        }
    }
}

/// builds aws credentials from static config.
fn build_credentials(config: &StaticCredentials) -> Credentials {
    Credentials::new(
        config.access_key.clone(),
        config.secret_key.clone(),
        config.session_token.clone(),
        None,
        "imagik-env-object-storage",
    )
}
