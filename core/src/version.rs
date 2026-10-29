pub fn newer(candidate: &str, current: &str) -> bool {
    let parse = |version: &str| -> Vec<u64> { version.split('.').map(|part| part.parse().unwrap_or(0)).chain([0; 3]).take(3).collect() };
    parse(candidate) > parse(current)
}

/// The string value of a top-level `key` in GitHub's JSON, without a JSON parser for one field.
pub fn field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let rest = &json[json.find(&format!("\"{key}\""))? + key.len() + 2..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    Some(&rest[..rest.find('"')?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("0.4.0", "0.3.1"));
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("0.4.0", "0.4.0"));
        assert!(!newer("0.3.9", "0.4.0"));
    }

    #[test]
    fn reads_a_field() {
        assert_eq!(field(r#"{"url":"x","tag_name": "v0.4.0","name":"Snowhush 0.4.0"}"#, "tag_name"), Some("v0.4.0"));
        assert_eq!(field(r#"{"name":"x"}"#, "tag_name"), None);
    }
}
