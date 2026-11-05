//! The tiny YAML subset used by electron-builder metadata: `key: value` scalars on one line.

/// Splits a `key: value` line. Keys are restricted to plain identifiers; nested or malformed
/// lines yield `None`.
pub(crate) fn key_value(line: &str) -> Option<(&str, String)> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    let is_plain_key = !key.is_empty()
        && key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'));
    if !is_plain_key {
        return None;
    }
    // A colon only separates a key when whitespace (or the line end) follows it.
    if !value.is_empty() && !value.starts_with([' ', '\t']) {
        return None;
    }
    Some((key, scalar(value)))
}

/// Unquotes a single-line scalar and drops trailing comments from unquoted values.
fn scalar(value: &str) -> String {
    let value = value.trim();
    if let Some(inner) = value.strip_prefix('\'') {
        return inner
            .strip_suffix('\'')
            .map_or_else(|| value.to_owned(), |text| text.replace("''", "'"));
    }
    if let Some(inner) = value.strip_prefix('"') {
        return inner.strip_suffix('"').unwrap_or(value).to_owned();
    }
    let without_comment = value.split_once(" #").map_or(value, |(text, _)| text);
    without_comment.trim().to_owned()
}

pub(crate) fn is_ignorable(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.starts_with('#')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_quoted_scalars() {
        assert_eq!(
            key_value("owner: beekeeper"),
            Some(("owner", "beekeeper".into()))
        );
        assert_eq!(
            key_value("channel: '@spawn''s-updater'"),
            Some(("channel", "@spawn's-updater".into()))
        );
        assert_eq!(
            key_value("url: \"https://x.test\""),
            Some(("url", "https://x.test".into()))
        );
        assert_eq!(
            key_value("tagNamePrefix: v # note"),
            Some(("tagNamePrefix", "v".into()))
        );
        assert_eq!(key_value("files:"), Some(("files", String::new())));
    }

    #[test]
    fn keeps_urls_and_rejects_non_pairs() {
        assert_eq!(
            key_value("url: https://host.test:8080/a#b"),
            Some(("url", "https://host.test:8080/a#b".into()))
        );
        assert_eq!(key_value("https://host.test"), None);
        assert_eq!(key_value("no separator"), None);
        assert_eq!(key_value("two words: x"), None);
    }
}
