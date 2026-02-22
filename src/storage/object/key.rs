//! object key handling and url construction.
//!
//! handles key prefixing, url encoding, and version token extraction from headers.

use super::ObjectStorageSource;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::header::{HeaderMap, ETAG, LAST_MODIFIED};

/// s3 version id header name.
const VERSION_ID_HEADER: &str = "x-amz-version-id";

impl ObjectStorageSource {
    /// applies configured key prefix to an object key.
    pub(super) fn storage_key(&self, object_key: &str) -> String {
        self.key_prefix.as_ref().map_or_else(
            || object_key.to_string(),
            |prefix| format!("{prefix}/{object_key}"),
        )
    }
}

/// extracts version identifiers from storage response headers.
///
/// prefers x-amz-version-id, falls back to etag or last-modified.
/// returns (`version_token`, `etag`).
pub(super) fn extract_version_headers(headers: &HeaderMap) -> (Option<String>, Option<String>) {
    let etag = headers
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(normalize_version_value);
    let version_id = headers
        .get(VERSION_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(normalize_version_value);
    let last_modified = headers
        .get(LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
        .map(normalize_version_value);

    let version_token = version_id.or_else(|| etag.clone()).or(last_modified);
    (version_token, etag)
}

/// builds version token from component parts.
///
/// prefers `version_id`, then `etag`, then `last_modified`.
pub(super) fn version_from_parts(
    version_id: Option<&str>,
    etag: Option<&str>,
    last_modified: Option<String>,
) -> Option<String> {
    version_id
        .map(normalize_version_value)
        .or_else(|| etag.map(normalize_version_value))
        .or(last_modified)
}

/// normalizes a version value by trimming quotes and whitespace.
pub(super) fn normalize_version_value(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

/// builds a signed url from template and object key.
///
/// supports {key} placeholder or appends key before query string.
pub(super) fn build_signed_url(template: &str, object_key: &str) -> String {
    let encoded_key = encode_key_for_url_path(object_key);

    if template.contains("{key}") {
        return template.replace("{key}", &encoded_key);
    }

    if let Some((base, query)) = template.split_once('?') {
        let separator = if base.ends_with('/') { "" } else { "/" };
        return format!("{base}{separator}{encoded_key}?{query}");
    }

    if template.ends_with('/') {
        format!("{template}{encoded_key}")
    } else {
        format!("{template}/{encoded_key}")
    }
}

/// encodes an object key for use in url path.
///
/// preserves path separators but encodes individual segments.
pub(super) fn encode_key_for_url_path(key: &str) -> String {
    let mut encoded = String::with_capacity(key.len() + 8);
    for (idx, segment) in key.split('/').enumerate() {
        if idx > 0 {
            encoded.push('/');
        }
        encoded.push_str(&utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string());
    }
    encoded
}

/// normalizes a key prefix by trimming whitespace and slashes.
pub(super) fn normalize_key_prefix(value: &str) -> Option<String> {
    let normalized = value.trim().trim_matches('/');
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_signed_url, encode_key_for_url_path, normalize_version_value, version_from_parts,
    };

    #[test]
    fn encodes_segments_but_keeps_path_separators() {
        let encoded = encode_key_for_url_path("photos/deer image.jpg");
        assert_eq!(encoded, "photos/deer%20image%2Ejpg");
    }

    #[test]
    fn signed_url_uses_placeholder_when_present() {
        let url = build_signed_url(
            "https://cdn.example.com/source/{key}?token=abc",
            "photos/deer image.jpg",
        );
        assert_eq!(
            url,
            "https://cdn.example.com/source/photos/deer%20image%2Ejpg?token=abc"
        );
    }

    #[test]
    fn signed_url_appends_key_before_query() {
        let url = build_signed_url(
            "https://cdn.example.com/source?token=abc",
            "photos/deer.jpg",
        );
        assert_eq!(
            url,
            "https://cdn.example.com/source/photos/deer%2Ejpg?token=abc"
        );
    }

    #[test]
    fn version_prefers_version_id_then_etag() {
        assert_eq!(
            version_from_parts(Some("v1"), Some("\"etag\""), Some("lm".to_string())),
            Some("v1".to_string())
        );
        assert_eq!(
            version_from_parts(None, Some("\"etag\""), None),
            Some("etag".to_string())
        );
        assert_eq!(
            version_from_parts(None, None, Some("lm".to_string())),
            Some("lm".to_string())
        );
    }

    #[test]
    fn normalizes_version_quotes() {
        assert_eq!(normalize_version_value("\"abc\""), "abc");
    }
}
