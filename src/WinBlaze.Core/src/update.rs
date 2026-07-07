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

/// Extracts the SHA-256 of the update-manifest artifact with the given `kind`
/// (e.g. `"portable_zip"`). The manifest lists artifacts as flat objects
/// `{kind, file_name, bytes, sha256}`, so this finds the `kind` marker and
/// reads the `sha256` that follows it. Returns None if absent.
pub fn parse_manifest_sha256(manifest_json: &str, kind: &str) -> Option<String> {
    let marker = format!("\"{kind}\"");
    let after_kind = &manifest_json[manifest_json.find(&marker)? + marker.len()..];
    // Scope the search to this artifact object: stop at the next artifact's
    // "kind" so a missing/blank sha256 can never read a sibling's hash.
    let object = &after_kind[..after_kind.find("\"kind\"").unwrap_or(after_kind.len())];
    parse_json_string_field(object, "sha256").filter(|value| !value.trim().is_empty())
}

/// Case-insensitive hex comparison of two SHA-256 digests (ignoring
/// surrounding whitespace). Empty inputs never match.
pub fn hash_matches(expected: &str, actual: &str) -> bool {
    let expected = expected.trim();
    let actual = actual.trim();
    !expected.is_empty() && expected.eq_ignore_ascii_case(actual)
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
    fn parse_manifest_sha256_reads_portable_artifact() {
        let manifest = r#"{"schema_version":1,"product":"WinBlaze","version":"0.9.1","artifacts":[
            {"kind":"portable_zip","file_name":"WinBlaze-Release-x64-portable.zip","bytes":1660783,"sha256":"ABC123def456"},
            {"kind":"msi","file_name":"WinBlaze.msi","bytes":100,"sha256":"deadbeef"}]}"#;
        assert_eq!(
            parse_manifest_sha256(manifest, "portable_zip").as_deref(),
            Some("ABC123def456")
        );
        assert_eq!(
            parse_manifest_sha256(manifest, "msi").as_deref(),
            Some("deadbeef")
        );
        assert_eq!(parse_manifest_sha256(manifest, "appimage"), None);
    }

    #[test]
    fn parse_manifest_sha256_is_object_scoped_and_rejects_blank() {
        // portable_zip has NO sha256; the msi (next artifact) does. The parser
        // must NOT bleed into the sibling and return the msi's hash.
        let no_hash = r#"{"artifacts":[
            {"kind":"portable_zip","file_name":"p.zip","bytes":10},
            {"kind":"msi","file_name":"w.msi","sha256":"deadbeef"}]}"#;
        assert_eq!(parse_manifest_sha256(no_hash, "portable_zip"), None);
        assert_eq!(
            parse_manifest_sha256(no_hash, "msi").as_deref(),
            Some("deadbeef")
        );

        // A blank sha256 is "no usable hash" -> None (indeterminate), not "".
        let blank = r#"{"artifacts":[{"kind":"portable_zip","sha256":"  "}]}"#;
        assert_eq!(parse_manifest_sha256(blank, "portable_zip"), None);
    }

    #[test]
    fn hash_matches_is_case_insensitive_and_rejects_empty() {
        assert!(hash_matches("ABC123", "abc123"));
        assert!(hash_matches("  abc123  ", "ABC123"));
        assert!(!hash_matches("abc123", "abc124"));
        assert!(!hash_matches("", "abc123"));
        assert!(!hash_matches("abc123", ""));
    }

    #[test]
    fn end_to_end_update_available() {
        let json = r#"{"tag_name":"v0.9.0"}"#;
        let tag = parse_release_tag(json).expect("tag");
        assert!(is_newer("0.8.0", &tag));
        assert!(!is_newer("0.9.0", &tag));
    }
}
