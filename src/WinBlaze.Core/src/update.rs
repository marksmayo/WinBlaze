//! Update-check logic: compare the running version against a release tag and
//! extract the tag from a GitHub "latest release" JSON body. Kept dependency-
//! free (no JSON crate) and pure so it is unit-tested here; the network fetch
//! and UI live in the C++ front-end, which calls this through the FFI.

/// Parses a `major.minor.patch` version, tolerating a leading `v`, a missing
/// patch/minor, and any `-pre`/`+build` suffix (which is ignored for ordering).
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let trimmed = text.trim().trim_start_matches(['v', 'V']);
    let core = trimmed.split(['-', '+']).next().unwrap_or(trimmed).trim();
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// True when `candidate` is a strictly newer version than `current`. Returns
/// false if either side is unparseable (fail safe: never nag on garbage).
pub fn is_newer(current: &str, candidate: &str) -> bool {
    match (parse_version(current), parse_version(candidate)) {
        (Some(current), Some(candidate)) => candidate > current,
        _ => false,
    }
}

/// Extracts the `tag_name` string (e.g. `"v0.9.0"`) from a GitHub
/// `releases/latest` JSON body without a JSON dependency. Returns None when the
/// field is absent or the value is malformed.
pub fn parse_release_tag(json: &str) -> Option<String> {
    parse_json_string_field(json, "tag_name")
}

/// Minimal extractor for a top-level `"field": "value"` JSON string. Handles
/// whitespace and the common escape sequences in the value; good enough for the
/// small, well-formed GitHub release payload (not a general JSON parser).
fn parse_json_string_field(json: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let after_key = &json[json.find(&needle)? + needle.len()..];

    let mut chars = after_key.chars().peekable();
    // Expect ':' after optional whitespace.
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
    if chars.next()? != ':' {
        return None;
    }
    // Expect the opening quote after optional whitespace.
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
    if chars.next()? != '"' {
        return None;
    }

    let mut value = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(value),
            '\\' => match chars.next()? {
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                '/' => value.push('/'),
                'n' => value.push('\n'),
                't' => value.push('\t'),
                other => {
                    value.push('\\');
                    value.push(other);
                }
            },
            other => value.push(other),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_newer_orders_semver_numerically() {
        assert!(is_newer("0.8.0", "0.9.0"));
        assert!(is_newer("v0.8.0", "v0.10.0")); // 10 > 9 numerically, not lexically
        assert!(is_newer("0.8.0", "1.0.0"));
        assert!(is_newer("0.8", "0.8.1")); // missing patch treated as 0
        assert!(!is_newer("0.9.0", "0.8.0"));
        assert!(!is_newer("0.8.0", "0.8.0")); // equal is not newer
        assert!(!is_newer("v1.2.3", "1.2.3"));
    }

    #[test]
    fn is_newer_ignores_prerelease_and_build_suffixes() {
        assert!(is_newer("0.8.0", "0.9.0-rc1"));
        assert!(!is_newer("0.9.0", "0.9.0+build.7"));
    }

    #[test]
    fn is_newer_fails_safe_on_garbage() {
        assert!(!is_newer("", "0.9.0"));
        assert!(!is_newer("0.8.0", "not-a-version"));
        assert!(!is_newer("garbage", "also-garbage"));
    }

    #[test]
    fn parse_release_tag_reads_github_shape() {
        let json = r#"{"url":"https://api.github.com/x","html_url":"https://github.com/x","tag_name":"v0.9.0","name":"v0.9.0 — Next"}"#;
        assert_eq!(parse_release_tag(json).as_deref(), Some("v0.9.0"));
    }

    #[test]
    fn parse_release_tag_tolerates_whitespace() {
        let json = "{ \"tag_name\" :   \"v1.2.3\" }";
        assert_eq!(parse_release_tag(json).as_deref(), Some("v1.2.3"));
    }

    #[test]
    fn parse_release_tag_absent_or_malformed_is_none() {
        assert_eq!(parse_release_tag("{}"), None);
        assert_eq!(parse_release_tag(r#"{"tag_name":"#), None);
        assert_eq!(parse_release_tag("not json at all"), None);
    }

    #[test]
    fn end_to_end_update_available() {
        let json = r#"{"tag_name":"v0.9.0"}"#;
        let tag = parse_release_tag(json).expect("tag");
        assert!(is_newer("0.8.0", &tag));
        assert!(!is_newer("0.9.0", &tag));
    }
}
