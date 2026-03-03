use std::collections::HashMap;

use chrono::NaiveDate;
use serde::Deserialize;

use crate::types::{PageId, TagName};

use super::traits::Frontmatter;

/// Raw YAML structure for deserialization.
#[derive(Deserialize, Default)]
struct RawFrontmatter {
    id: Option<String>,
    title: Option<String>,
    created: Option<NaiveDate>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(flatten)]
    extra: HashMap<String, serde_yaml::Value>,
}

/// Extract the YAML frontmatter text between `---` delimiters at the start of a document.
/// Returns `(yaml_content, body_start_line)`. The body_start_line is the line index after the
/// closing `---`.
pub fn extract_frontmatter_text(text: &str) -> Option<(String, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        return None;
    }
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            let yaml = lines[1..i].join("\n");
            return Some((yaml, i + 1));
        }
    }
    None
}

/// Parse YAML frontmatter text into a Frontmatter struct.
pub fn parse_frontmatter(text: &str) -> Option<Frontmatter> {
    let (yaml, _) = extract_frontmatter_text(text)?;
    parse_yaml_frontmatter(&yaml)
}

fn parse_yaml_frontmatter(yaml: &str) -> Option<Frontmatter> {
    let raw: RawFrontmatter = serde_yaml::from_str(yaml).ok()?;
    let id = raw.id.and_then(|s| PageId::from_hex(&s));
    let tags = raw.tags.into_iter().map(|t| TagName(t)).collect();
    Some(Frontmatter {
        id,
        title: raw.title,
        created: raw.created,
        tags,
        extra: raw.extra,
    })
}

/// Serialize a Frontmatter struct back to YAML with `---` delimiters.
pub fn serialize_frontmatter(fm: &Frontmatter) -> String {
    let mut lines = Vec::new();
    lines.push("---".to_string());

    if let Some(ref id) = fm.id {
        lines.push(format!("id: {}", id.to_hex()));
    }
    if let Some(ref title) = fm.title {
        lines.push(format!("title: \"{}\"", title));
    }
    if let Some(ref created) = fm.created {
        lines.push(format!("created: {}", created));
    }
    if !fm.tags.is_empty() {
        let tag_strs: Vec<&str> = fm.tags.iter().map(|t| t.0.as_str()).collect();
        lines.push(format!("tags: [{}]", tag_strs.join(", ")));
    }

    // Serialize extra keys in sorted order for determinism
    let mut keys: Vec<&String> = fm.extra.keys().collect();
    keys.sort();
    for key in keys {
        let val = &fm.extra[key];
        if let Ok(s) = serde_yaml::to_string(val) {
            let s = s.trim().trim_end_matches('\n');
            // serde_yaml may produce `---\n` prefix — strip it
            let s = s.strip_prefix("---").map(|s| s.trim_start()).unwrap_or(s);
            lines.push(format!("{}: {}", key, s));
        }
    }

    lines.push("---".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PageId;

    // UC-07, UC-13: Frontmatter parsing
    #[test]
    fn test_parse_complete_frontmatter() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"Test\"\ncreated: 2026-01-01\ntags: [rust, editors]\n---\n\nBody text";
        let fm = parse_frontmatter(text).unwrap();
        assert_eq!(fm.id, Some(PageId::from_hex("8f3a1b2c").unwrap()));
        assert_eq!(fm.title.as_deref(), Some("Test"));
        assert_eq!(
            fm.created,
            Some(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
        );
        assert_eq!(fm.tags.len(), 2);
        assert_eq!(fm.tags[0].0, "rust");
        assert_eq!(fm.tags[1].0, "editors");
    }

    #[test]
    fn test_parse_frontmatter_missing_fields() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"Minimal\"\n---\n";
        let fm = parse_frontmatter(text).unwrap();
        assert!(fm.id.is_some());
        assert_eq!(fm.title.as_deref(), Some("Minimal"));
        assert!(fm.created.is_none());
        assert!(fm.tags.is_empty());
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        let text = "Just some text without frontmatter";
        assert!(parse_frontmatter(text).is_none());
    }

    #[test]
    fn test_serialize_frontmatter_roundtrip() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"Test\"\ncreated: 2026-01-01\ntags: [rust, editors]\n---\n";
        let fm = parse_frontmatter(text).unwrap();
        let serialized = serialize_frontmatter(&fm);
        let reparsed = parse_frontmatter(&format!("{}\n", serialized)).unwrap();
        assert_eq!(fm.id, reparsed.id);
        assert_eq!(fm.title, reparsed.title);
        assert_eq!(fm.created, reparsed.created);
        assert_eq!(fm.tags.len(), reparsed.tags.len());
    }

    #[test]
    fn test_extract_frontmatter_text() {
        let text = "---\nid: abc\ntitle: \"X\"\n---\nBody";
        let (yaml, body_start) = extract_frontmatter_text(text).unwrap();
        assert!(yaml.contains("id: abc"));
        assert_eq!(body_start, 4); // line after closing ---
    }

    #[test]
    fn test_extract_frontmatter_text_none_when_missing() {
        assert!(extract_frontmatter_text("No frontmatter").is_none());
        assert!(extract_frontmatter_text("").is_none());
    }
}