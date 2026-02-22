use percent_encoding::percent_decode_str;

const MAX_OBJECT_KEY_LEN: usize = 1024;

pub(super) fn decode_and_validate_object_key(raw: &str) -> Result<String, String> {
    let decoded = percent_decode_str(raw)
        .decode_utf8()
        .map_err(|_| "object key is not valid UTF-8".to_string())?;
    let normalized = decoded.trim_start_matches('/');

    if normalized.is_empty() {
        return Err("object key cannot be empty".to_string());
    }
    if normalized.len() > MAX_OBJECT_KEY_LEN {
        return Err(format!(
            "object key is too long (max {MAX_OBJECT_KEY_LEN} bytes)"
        ));
    }
    if normalized.chars().any(|ch| ch.is_control() || ch == '\\') {
        return Err("object key contains disallowed characters".to_string());
    }
    if normalized
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err("object key contains invalid path segments".to_string());
    }

    Ok(normalized.to_string())
}

#[cfg(test)]
mod tests {
    use super::decode_and_validate_object_key;

    #[test]
    fn accepts_nested_object_key() {
        let key = decode_and_validate_object_key("photos/deer.jpg").expect("valid key");
        assert_eq!(key, "photos/deer.jpg");
    }

    #[test]
    fn decodes_percent_encoding() {
        let key = decode_and_validate_object_key("photos/deer%20image.jpg").expect("valid key");
        assert_eq!(key, "photos/deer image.jpg");
    }

    #[test]
    fn rejects_traversal_segments() {
        let err = decode_and_validate_object_key("photos/../secret.jpg").expect_err("must reject");
        assert!(err.contains("invalid path segments"));
    }

    #[test]
    fn rejects_empty_path_segments() {
        let err = decode_and_validate_object_key("photos//deer.jpg").expect_err("must reject");
        assert!(err.contains("invalid path segments"));
    }
}
