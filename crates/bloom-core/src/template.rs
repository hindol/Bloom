use chrono::Local;
use rand::Rng;
use std::path::Path;

/// A parsed placeholder in a template.
#[derive(Debug, Clone)]
pub struct Placeholder {
    /// Byte range in the original template string.
    pub range: std::ops::Range<usize>,
    /// Kind of placeholder.
    pub kind: PlaceholderKind,
}

#[derive(Debug, Clone)]
pub enum PlaceholderKind {
    /// `${1:title}` — user-fillable tab stop with description hint.
    TabStop { index: usize, description: String },
    /// `${AUTO:id}` — auto-generated 16-hex-char Bloom ID.
    AutoId,
    /// `${AUTO:date}` — today's date as YYYY-MM-DD.
    AutoDate,
    /// `${DATE}` — alias for `${AUTO:date}`.
    Date,
}

/// A parsed template ready for expansion.
#[derive(Debug, Clone)]
pub struct Template {
    pub name: String,
    pub raw: String,
    pub placeholders: Vec<Placeholder>,
}

/// Result of expanding a template.
#[derive(Debug, Clone)]
pub struct ExpandedTemplate {
    /// The expanded content with auto-fields filled and tab-stops replaced by their descriptions.
    pub content: String,
    /// Byte positions of tab stops in the expanded content, ordered by tab-stop index.
    pub tab_stops: Vec<TabStop>,
}

#[derive(Debug, Clone)]
pub struct TabStop {
    pub index: usize,
    pub start: usize,
    pub end: usize,
    pub description: String,
}

/// Scan `raw` for `${...}` placeholders and return a `Template`.
pub fn parse(name: &str, raw: &str) -> Template {
    let mut placeholders = Vec::new();
    let bytes = raw.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 1 < len {
        if bytes[i] == b'$' && bytes[i + 1] == b'{' {
            if let Some(close) = raw[i..].find('}') {
                let close_abs = i + close;
                let inner = &raw[i + 2..close_abs];
                let range = i..close_abs + 1;

                let kind = parse_inner(inner);
                if let Some(kind) = kind {
                    placeholders.push(Placeholder { range, kind });
                }
                i = close_abs + 1;
                continue;
            }
        }
        i += 1;
    }

    Template {
        name: name.to_string(),
        raw: raw.to_string(),
        placeholders,
    }
}

fn parse_inner(inner: &str) -> Option<PlaceholderKind> {
    let trimmed = inner.trim();
    if trimmed == "DATE" {
        return Some(PlaceholderKind::Date);
    }
    if trimmed == "AUTO:id" {
        return Some(PlaceholderKind::AutoId);
    }
    if trimmed == "AUTO:date" {
        return Some(PlaceholderKind::AutoDate);
    }
    // Tab stop: N:description
    if let Some(colon) = trimmed.find(':') {
        if let Ok(index) = trimmed[..colon].parse::<usize>() {
            let description = trimmed[colon + 1..].to_string();
            return Some(PlaceholderKind::TabStop { index, description });
        }
    }
    None
}

/// Replace AUTO placeholders with generated values, replace tab-stops with their
/// description text, and record cursor positions.
pub fn expand(template: &Template) -> ExpandedTemplate {
    let mut rng = rand::thread_rng();
    let today = Local::now().format("%Y-%m-%d").to_string();
    let auto_id: String = (0..16)
        .map(|_| format!("{:x}", rng.gen_range(0u8..16)))
        .collect();

    let raw = &template.raw;
    let mut content = String::with_capacity(raw.len());
    let mut tab_stops = Vec::new();
    let mut prev_end = 0;

    // Process placeholders in order of their position in the raw string.
    let mut sorted: Vec<&Placeholder> = template.placeholders.iter().collect();
    sorted.sort_by_key(|p| p.range.start);

    for ph in &sorted {
        content.push_str(&raw[prev_end..ph.range.start]);
        let replacement = match &ph.kind {
            PlaceholderKind::AutoId => auto_id.clone(),
            PlaceholderKind::AutoDate | PlaceholderKind::Date => today.clone(),
            PlaceholderKind::TabStop { index, description } => {
                let start = content.len();
                let end = start + description.len();
                tab_stops.push(TabStop {
                    index: *index,
                    start,
                    end,
                    description: description.clone(),
                });
                description.clone()
            }
        };
        content.push_str(&replacement);
        prev_end = ph.range.end;
    }
    content.push_str(&raw[prev_end..]);

    // Sort tab stops by index (stable sort preserves order for same index).
    tab_stops.sort_by_key(|ts| ts.index);

    ExpandedTemplate { content, tab_stops }
}

/// Read all `.tmpl` files from `<vault_root>/templates/` and parse them.
pub fn load_templates(vault_root: &Path) -> Vec<Template> {
    let dir = vault_root.join("templates");
    let mut templates = Vec::new();

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return templates,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("tmpl") {
            if let Ok(raw) = std::fs::read_to_string(&path) {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unnamed")
                    .to_string();
                templates.push(parse(&name, &raw));
            }
        }
    }

    templates.sort_by(|a, b| a.name.cmp(&b.name));
    templates
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
---
id: ${AUTO:id}
title: \"${1:Page Title}\"
created: ${AUTO:date}T00:00:00Z
tags: [${2:tags}]
---

# ${1:Page Title}

${3:Start writing...}
";

    #[test]
    fn parse_finds_all_placeholders() {
        let tpl = parse("test", SAMPLE);
        // 3 distinct tab-stop references (index 1 appears twice → 4 tab-stop placeholders)
        // plus AUTO:id and AUTO:date = 6 total
        assert_eq!(tpl.placeholders.len(), 6);

        let tab_stops: Vec<_> = tpl
            .placeholders
            .iter()
            .filter(|p| matches!(p.kind, PlaceholderKind::TabStop { .. }))
            .collect();
        assert_eq!(tab_stops.len(), 4); // ${1:...} x2, ${2:...}, ${3:...}

        let auto_ids: Vec<_> = tpl
            .placeholders
            .iter()
            .filter(|p| matches!(p.kind, PlaceholderKind::AutoId))
            .collect();
        assert_eq!(auto_ids.len(), 1);

        let auto_dates: Vec<_> = tpl
            .placeholders
            .iter()
            .filter(|p| matches!(p.kind, PlaceholderKind::AutoDate))
            .collect();
        assert_eq!(auto_dates.len(), 1);
    }

    #[test]
    fn parse_finds_date_alias() {
        let tpl = parse("d", "Today is ${DATE}.");
        assert_eq!(tpl.placeholders.len(), 1);
        assert!(matches!(tpl.placeholders[0].kind, PlaceholderKind::Date));
    }

    #[test]
    fn expand_replaces_auto_fields() {
        let tpl = parse("test", SAMPLE);
        let exp = expand(&tpl);

        // AUTO:id should be replaced with 16 hex chars
        assert!(!exp.content.contains("${AUTO:id}"));
        // Find the id line and check the value
        for line in exp.content.lines() {
            if line.starts_with("id: ") {
                let id_val = line.trim_start_matches("id: ");
                assert_eq!(id_val.len(), 16);
                assert!(id_val.chars().all(|c| c.is_ascii_hexdigit()));
            }
        }

        // AUTO:date should be YYYY-MM-DD
        assert!(!exp.content.contains("${AUTO:date}"));
        let today = Local::now().format("%Y-%m-%d").to_string();
        assert!(exp.content.contains(&today));
    }

    #[test]
    fn expand_preserves_tab_stop_order() {
        let tpl = parse("test", SAMPLE);
        let exp = expand(&tpl);

        // Tab stops must be sorted by index
        for window in exp.tab_stops.windows(2) {
            assert!(window[0].index <= window[1].index);
        }

        // We expect 4 tab-stop entries: index 1 (x2), 2, 3
        assert_eq!(exp.tab_stops.len(), 4);
        assert_eq!(exp.tab_stops[0].index, 1);
        assert_eq!(exp.tab_stops[1].index, 1);
        assert_eq!(exp.tab_stops[2].index, 2);
        assert_eq!(exp.tab_stops[3].index, 3);
    }

    #[test]
    fn expand_same_index_tab_stops_share_text() {
        let tpl = parse("test", SAMPLE);
        let exp = expand(&tpl);

        let ones: Vec<_> = exp.tab_stops.iter().filter(|t| t.index == 1).collect();
        assert_eq!(ones.len(), 2);
        assert_eq!(ones[0].description, ones[1].description);
        assert_eq!(ones[0].description, "Page Title");

        // Verify the expanded content at those positions matches
        let text0 = &exp.content[ones[0].start..ones[0].end];
        let text1 = &exp.content[ones[1].start..ones[1].end];
        assert_eq!(text0, "Page Title");
        assert_eq!(text1, "Page Title");
    }

    #[test]
    fn load_templates_from_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let tpl_dir = tmp.path().join("templates");
        std::fs::create_dir(&tpl_dir).unwrap();

        std::fs::write(
            tpl_dir.join("note.tmpl"),
            "# ${1:Title}\n${2:Body}\n",
        )
        .unwrap();
        std::fs::write(
            tpl_dir.join("journal.tmpl"),
            "date: ${AUTO:date}\n${1:Entry}\n",
        )
        .unwrap();
        // non-tmpl file should be ignored
        std::fs::write(tpl_dir.join("readme.md"), "ignore me").unwrap();

        let templates = load_templates(tmp.path());
        assert_eq!(templates.len(), 2);

        let names: Vec<&str> = templates.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"journal"));
        assert!(names.contains(&"note"));
    }
}
