// Logseq vault importer — non-destructive syntax mapping, namespace
// flattening, and import report generation.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::document::{BloomId, Frontmatter};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct ImportConfig {
    pub source_dir: PathBuf,
    pub target_dir: PathBuf,
    /// If true, don't write files — just generate the report.
    pub dry_run: bool,
}

#[derive(Debug, Default)]
pub struct ImportReport {
    pub pages_imported: usize,
    pub pages_skipped: usize,
    pub links_resolved: usize,
    pub links_unresolved: usize,
    pub tags_converted: usize,
    pub tasks_converted: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Source directory does not exist: {0}")]
    SourceNotFound(PathBuf),
    #[error("YAML serialization error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn import_logseq_vault(config: &ImportConfig) -> Result<ImportReport, ImportError> {
    if !config.source_dir.exists() {
        return Err(ImportError::SourceNotFound(config.source_dir.clone()));
    }

    let mut report = ImportReport::default();

    // Phase 1: Scan .md files from pages/ and journals/
    let pages_dir = config.source_dir.join("pages");
    let journals_dir = config.source_dir.join("journals");

    let page_files = scan_md_files(&pages_dir);
    let journal_files = scan_md_files(&journals_dir);

    // Phase 2: Build title→id map
    let title_to_id = build_title_map(&page_files, &journal_files);

    // Phase 3 & 4: Convert and (optionally) write each file
    let target_pages = config.target_dir.join("pages");
    let target_journals = config.target_dir.join("journal");

    if !config.dry_run {
        fs::create_dir_all(&target_pages)?;
        fs::create_dir_all(&target_journals)?;
    }

    for path in &page_files {
        convert_and_write(path, &target_pages, &title_to_id, config.dry_run, &mut report)?;
    }
    for path in &journal_files {
        convert_and_write(
            path,
            &target_journals,
            &title_to_id,
            config.dry_run,
            &mut report,
        )?;
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Phase 1 — scan
// ---------------------------------------------------------------------------

fn scan_md_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().map_or(false, |e| e == "md") {
                files.push(p);
            }
        }
    }
    files.sort();
    files
}

// ---------------------------------------------------------------------------
// Phase 2 — title→id map
// ---------------------------------------------------------------------------

fn build_title_map(page_files: &[PathBuf], journal_files: &[PathBuf]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for path in page_files.iter().chain(journal_files.iter()) {
        if let Some(title) = logseq_title_from_path(path) {
            let id = BloomId::new().0;
            map.insert(title, id);
        }
    }
    map
}

/// Derive the Logseq page title from a file path.
/// `pages/My Page.md` → `"My Page"`, `pages/ns___page.md` → `"ns/page"`
fn logseq_title_from_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    // Logseq encodes `/` as `___` in filenames
    Some(stem.replace("___", "/"))
}

// ---------------------------------------------------------------------------
// Phase 3 — convert a single file
// ---------------------------------------------------------------------------

fn convert_and_write(
    src: &Path,
    target_dir: &Path,
    title_to_id: &HashMap<String, String>,
    dry_run: bool,
    report: &mut ImportReport,
) -> Result<(), ImportError> {
    let raw = match fs::read_to_string(src) {
        Ok(s) => s,
        Err(_) => {
            report.pages_skipped += 1;
            return Ok(());
        }
    };

    let title = logseq_title_from_path(src).unwrap_or_default();
    let (flat_name, namespace) = flatten_namespace(&title);

    // Parse Logseq properties and body
    let (props, body) = parse_logseq_properties(&raw);

    // Build frontmatter
    let id = title_to_id
        .get(&title)
        .cloned()
        .unwrap_or_else(|| BloomId::new().0);

    let mut fm = Frontmatter::new(&flat_name);
    fm.id = id;
    if let Some(ns) = &namespace {
        if !fm.tags.contains(ns) {
            fm.tags.push(ns.clone());
        }
    }
    for (k, v) in &props {
        fm.extra
            .insert(k.clone(), serde_yaml::Value::String(v.clone()));
    }

    // Convert body lines
    let mut converted_lines: Vec<String> = Vec::new();
    for line in body.lines() {
        let c = convert_logseq_line(line, title_to_id, report);
        converted_lines.push(c);
    }

    let yaml = serde_yaml::to_string(&fm)?;
    let output = format!("---\n{}---\n{}", yaml, converted_lines.join("\n"));

    if !dry_run {
        let filename = format!("{}.md", slugify(&flat_name));
        let dest = target_dir.join(&filename);
        fs::write(&dest, &output)?;
    }

    report.pages_imported += 1;
    Ok(())
}

// ---------------------------------------------------------------------------
// Line-level conversion
// ---------------------------------------------------------------------------

fn convert_logseq_line(
    line: &str,
    title_to_id: &HashMap<String, String>,
    report: &mut ImportReport,
) -> String {
    let mut result = line.to_string();

    // Task markers: detect at start of line (possibly after `- `)
    result = convert_task_markers(&result, report);

    // Scheduling: DEADLINE / SCHEDULED
    result = convert_scheduling(&result);

    // Embeds: {{embed [[Page]]}} → ![[id|Page]]
    result = convert_embeds(&result, title_to_id, report);

    // Block references: ((ref)) → ![[ref|ref]]
    result = convert_block_refs(&result, report);

    // Wikilinks: [[Page Name]] → [[id|Page Name]]
    result = convert_wikilinks(&result, title_to_id, report);

    // Multi-word tags: #[[multi word tag]] → #multi-word-tag
    result = convert_multiword_tags(&result, report);

    result
}

// ---------------------------------------------------------------------------
// Wikilinks
// ---------------------------------------------------------------------------

fn convert_wikilinks(
    line: &str,
    title_to_id: &HashMap<String, String>,
    report: &mut ImportReport,
) -> String {
    let mut result = String::with_capacity(line.len());
    let mut chars = line.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == '[' {
            if let Some(&(_, '[')) = chars.peek() {
                chars.next(); // consume second '['
                // Find closing ]]
                let start = i;
                let content_start = i + 2;
                let mut depth = 1;
                let mut end = None;
                let mut content_end = content_start;
                while let Some((j, ch)) = chars.next() {
                    if ch == '[' {
                        if let Some(&(_, '[')) = chars.peek() {
                            depth += 1;
                            chars.next();
                            continue;
                        }
                    }
                    if ch == ']' {
                        if let Some(&(_, ']')) = chars.peek() {
                            depth -= 1;
                            if depth == 0 {
                                content_end = j;
                                chars.next(); // consume second ']'
                                end = Some(j + 2);
                                break;
                            }
                            chars.next();
                            continue;
                        }
                    }
                }
                if end.is_some() {
                    let page_name = &line[content_start..content_end];
                    // Skip if already has a pipe (already converted or alias)
                    if page_name.contains('|') {
                        result.push_str(&line[start..content_end + 2]);
                    } else if let Some(id) = title_to_id.get(page_name) {
                        result.push_str(&format!("[[{}|{}]]", id, page_name));
                        report.links_resolved += 1;
                    } else {
                        // Unresolved — keep original
                        result.push_str(&format!("[[{}]]", page_name));
                        report.links_unresolved += 1;
                    }
                } else {
                    // Malformed — keep as-is
                    result.push_str(&line[start..]);
                    return result;
                }
                continue;
            }
        }
        result.push(c);
    }
    result
}

// ---------------------------------------------------------------------------
// Block references
// ---------------------------------------------------------------------------

fn convert_block_refs(line: &str, report: &mut ImportReport) -> String {
    let mut result = String::new();
    let mut rest = line;

    while let Some(start) = rest.find("((") {
        result.push_str(&rest[..start]);
        let inner_start = start + 2;
        if let Some(end) = rest[inner_start..].find("))") {
            let ref_id = &rest[inner_start..inner_start + end];
            result.push_str(&format!("![[{}|{}]]", ref_id, ref_id));
            report.links_unresolved += 1; // best-effort
            rest = &rest[inner_start + end + 2..];
        } else {
            result.push_str(&rest[start..]);
            return result;
        }
    }
    result.push_str(rest);
    result
}

// ---------------------------------------------------------------------------
// Embeds
// ---------------------------------------------------------------------------

fn convert_embeds(
    line: &str,
    title_to_id: &HashMap<String, String>,
    report: &mut ImportReport,
) -> String {
    let mut result = String::new();
    let mut rest = line;

    while let Some(start) = rest.find("{{embed [[") {
        result.push_str(&rest[..start]);
        let inner_start = start + 10; // after "{{embed [["
        if let Some(end) = rest[inner_start..].find("]]}}") {
            let page_name = &rest[inner_start..inner_start + end];
            if let Some(id) = title_to_id.get(page_name) {
                result.push_str(&format!("![[{}|{}]]", id, page_name));
                report.links_resolved += 1;
            } else {
                result.push_str(&format!("![[{}]]", page_name));
                report.links_unresolved += 1;
            }
            rest = &rest[inner_start + end + 4..];
        } else {
            result.push_str(&rest[start..]);
            return result;
        }
    }
    result.push_str(rest);
    result
}

// ---------------------------------------------------------------------------
// Task markers
// ---------------------------------------------------------------------------

fn convert_task_markers(line: &str, report: &mut ImportReport) -> String {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];

    // Strip optional leading `- ` for matching
    let (prefix, core) = if trimmed.starts_with("- ") {
        ("- ", &trimmed[2..])
    } else {
        ("", trimmed)
    };

    let markers_unchecked = ["TODO ", "DOING ", "LATER ", "NOW "];
    let marker_checked = "DONE ";

    for m in &markers_unchecked {
        if core.starts_with(m) {
            report.tasks_converted += 1;
            let text = &core[m.len()..];
            return format!("{}{}- [ ] {}", indent, if prefix.is_empty() { "" } else { "" }, text);
        }
    }

    if core.starts_with(marker_checked) {
        report.tasks_converted += 1;
        let text = &core[marker_checked.len()..];
        return format!("{}- [x] {}", indent, text);
    }

    line.to_string()
}

// ---------------------------------------------------------------------------
// Scheduling (DEADLINE / SCHEDULED)
// ---------------------------------------------------------------------------

fn convert_scheduling(line: &str) -> String {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("DEADLINE:") {
        if let Some(date) = convert_logseq_date(rest.trim()) {
            return format!("@due({})", date);
        }
    } else if let Some(rest) = trimmed.strip_prefix("SCHEDULED:") {
        if let Some(date) = convert_logseq_date(rest.trim()) {
            return format!("@start({})", date);
        }
    }
    line.to_string()
}

/// Parse Logseq org-style dates: `<2026-03-02 Mon>` → `"2026-03-02"`
fn convert_logseq_date(date_str: &str) -> Option<String> {
    let s = date_str.trim().trim_start_matches('<').trim_end_matches('>');
    // Take first token (the YYYY-MM-DD part)
    let date_part = s.split_whitespace().next()?;
    // Validate shape
    if date_part.len() == 10
        && date_part.chars().nth(4) == Some('-')
        && date_part.chars().nth(7) == Some('-')
    {
        Some(date_part.to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Multi-word tags: #[[multi word tag]] → #multi-word-tag
// ---------------------------------------------------------------------------

fn convert_multiword_tags(line: &str, report: &mut ImportReport) -> String {
    let mut result = String::new();
    let mut rest = line;

    while let Some(start) = rest.find("#[[") {
        result.push_str(&rest[..start]);
        let inner_start = start + 3;
        if let Some(end) = rest[inner_start..].find("]]") {
            let tag_text = &rest[inner_start..inner_start + end];
            result.push('#');
            result.push_str(&slugify(tag_text));
            report.tags_converted += 1;
            rest = &rest[inner_start + end + 2..];
        } else {
            result.push_str(&rest[start..]);
            return result;
        }
    }
    result.push_str(rest);
    result
}

// ---------------------------------------------------------------------------
// Logseq properties block → HashMap
// ---------------------------------------------------------------------------

/// Parse the Logseq properties block at the top of a page.
/// Properties are lines matching `key:: value` before the first blank line or
/// non-property line.  Returns the properties and the remaining body text.
fn parse_logseq_properties(content: &str) -> (HashMap<String, String>, &str) {
    let mut props = HashMap::new();
    let mut offset = 0;

    for line in content.lines() {
        if let Some(idx) = line.find(":: ") {
            let key = line[..idx].trim();
            let value = line[idx + 3..].trim();
            if !key.is_empty() && !key.contains(' ') {
                props.insert(key.to_string(), value.to_string());
                offset += line.len() + 1; // +1 for newline
                continue;
            }
        }
        // Also handle `key::` with no value
        if line.trim_end().ends_with("::") && !line.trim().contains(' ') {
            let key = line.trim().trim_end_matches("::");
            if !key.is_empty() {
                props.insert(key.to_string(), String::new());
                offset += line.len() + 1;
                continue;
            }
        }
        break;
    }

    // Clamp offset to content length
    let offset = offset.min(content.len());
    (props, &content[offset..])
}

// ---------------------------------------------------------------------------
// Namespace flattening
// ---------------------------------------------------------------------------

/// Split a title like `"namespace/page"` into `("page", Some("namespace"))`.
/// Titles without `/` return `(title, None)`.
fn flatten_namespace(title: &str) -> (String, Option<String>) {
    if let Some(pos) = title.rfind('/') {
        let ns = &title[..pos];
        let page = &title[pos + 1..];
        (page.to_string(), Some(ns.to_string()))
    } else {
        (title.to_string(), None)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ---- wikilinks ----

    #[test]
    fn convert_wikilink_to_bloom_link() {
        let mut map = HashMap::new();
        map.insert("Page".to_string(), "abcd1234".to_string());
        let mut report = ImportReport::default();

        let result = convert_wikilinks("see [[Page]] here", &map, &mut report);
        assert_eq!(result, "see [[abcd1234|Page]] here");
        assert_eq!(report.links_resolved, 1);
    }

    #[test]
    fn convert_wikilink_unresolved() {
        let map = HashMap::new();
        let mut report = ImportReport::default();

        let result = convert_wikilinks("see [[Unknown]] here", &map, &mut report);
        assert_eq!(result, "see [[Unknown]] here");
        assert_eq!(report.links_unresolved, 1);
    }

    // ---- task markers ----

    #[test]
    fn convert_todo_states() {
        let mut report = ImportReport::default();

        assert_eq!(convert_task_markers("TODO buy milk", &mut report), "- [ ] buy milk");
        assert_eq!(convert_task_markers("DONE buy milk", &mut report), "- [x] buy milk");
        assert_eq!(convert_task_markers("DOING write code", &mut report), "- [ ] write code");
        assert_eq!(convert_task_markers("LATER read book", &mut report), "- [ ] read book");
        assert_eq!(convert_task_markers("NOW urgent task", &mut report), "- [ ] urgent task");
        assert_eq!(report.tasks_converted, 5);
    }

    #[test]
    fn convert_todo_with_bullet_prefix() {
        let mut report = ImportReport::default();
        assert_eq!(
            convert_task_markers("- TODO buy milk", &mut report),
            "- [ ] buy milk"
        );
        assert_eq!(report.tasks_converted, 1);
    }

    // ---- properties / frontmatter ----

    #[test]
    fn convert_properties_to_frontmatter() {
        let input = "title:: My Page\ntags:: foo, bar\n\nSome body text";
        let (props, body) = parse_logseq_properties(input);

        assert_eq!(props.get("title").unwrap(), "My Page");
        assert_eq!(props.get("tags").unwrap(), "foo, bar");
        assert_eq!(body, "\nSome body text");
    }

    #[test]
    fn properties_empty_value() {
        let input = "collapsed::\nkey:: val\nbody";
        let (props, body) = parse_logseq_properties(input);

        assert_eq!(props.get("collapsed").unwrap(), "");
        assert_eq!(props.get("key").unwrap(), "val");
        assert_eq!(body, "body");
    }

    // ---- scheduling ----

    #[test]
    fn convert_scheduling_deadline() {
        assert_eq!(
            convert_scheduling("DEADLINE: <2026-03-02 Mon>"),
            "@due(2026-03-02)"
        );
    }

    #[test]
    fn convert_scheduling_scheduled() {
        assert_eq!(
            convert_scheduling("SCHEDULED: <2026-03-02 Mon>"),
            "@start(2026-03-02)"
        );
    }

    #[test]
    fn convert_logseq_date_valid() {
        assert_eq!(
            convert_logseq_date("<2026-03-02 Mon>"),
            Some("2026-03-02".to_string())
        );
    }

    // ---- namespace flattening ----

    #[test]
    fn flatten_namespace_splits_correctly() {
        assert_eq!(
            flatten_namespace("projects/bloom"),
            ("bloom".to_string(), Some("projects".to_string()))
        );
        assert_eq!(
            flatten_namespace("plain-page"),
            ("plain-page".to_string(), None)
        );
        assert_eq!(
            flatten_namespace("a/b/c"),
            ("c".to_string(), Some("a/b".to_string()))
        );
    }

    // ---- multi-word tags ----

    #[test]
    fn convert_multiword_tag() {
        let mut report = ImportReport::default();
        let result = convert_multiword_tags("text #[[multi word tag]] more", &mut report);
        assert_eq!(result, "text #multi-word-tag more");
        assert_eq!(report.tags_converted, 1);
    }

    // ---- embeds ----

    #[test]
    fn convert_embed_resolved() {
        let mut map = HashMap::new();
        map.insert("Page".to_string(), "abc12345".to_string());
        let mut report = ImportReport::default();

        let result = convert_embeds("{{embed [[Page]]}}", &map, &mut report);
        assert_eq!(result, "![[abc12345|Page]]");
        assert_eq!(report.links_resolved, 1);
    }

    // ---- block references ----

    #[test]
    fn convert_block_ref() {
        let mut report = ImportReport::default();
        let result = convert_block_refs("see ((abc-123)) here", &mut report);
        assert_eq!(result, "see ![[abc-123|abc-123]] here");
    }

    // ---- integration: dry run ----

    #[test]
    fn import_dry_run_produces_report() {
        let src = TempDir::new().unwrap();
        let tgt = TempDir::new().unwrap();

        // Create Logseq vault structure
        let pages = src.path().join("pages");
        fs::create_dir(&pages).unwrap();
        fs::write(
            pages.join("My Page.md"),
            "title:: My Page\n- TODO something\n- see [[Other]]\n",
        )
        .unwrap();
        fs::write(pages.join("Other.md"), "title:: Other\nHello\n").unwrap();

        let config = ImportConfig {
            source_dir: src.path().to_path_buf(),
            target_dir: tgt.path().to_path_buf(),
            dry_run: true,
        };

        let report = import_logseq_vault(&config).unwrap();
        assert_eq!(report.pages_imported, 2);
        assert!(report.tasks_converted >= 1);
        // Dry run — target should be empty
        assert!(!tgt.path().join("pages").exists());
    }

    // ---- integration: write ----

    #[test]
    fn import_writes_converted_files() {
        let src = TempDir::new().unwrap();
        let tgt = TempDir::new().unwrap();

        let pages = src.path().join("pages");
        fs::create_dir(&pages).unwrap();
        fs::write(
            pages.join("Hello World.md"),
            "title:: Hello World\ntags:: demo\n- DONE first task\n",
        )
        .unwrap();

        let config = ImportConfig {
            source_dir: src.path().to_path_buf(),
            target_dir: tgt.path().to_path_buf(),
            dry_run: false,
        };

        let report = import_logseq_vault(&config).unwrap();
        assert_eq!(report.pages_imported, 1);
        assert_eq!(report.tasks_converted, 1);

        let written = fs::read_to_string(tgt.path().join("pages/hello-world.md")).unwrap();
        assert!(written.contains("---\n"));
        assert!(written.contains("title: Hello World"));
        assert!(written.contains("- [x] first task"));
    }

    // ---- integration: namespace flattening in import ----

    #[test]
    fn import_flattens_namespace() {
        let src = TempDir::new().unwrap();
        let tgt = TempDir::new().unwrap();

        let pages = src.path().join("pages");
        fs::create_dir(&pages).unwrap();
        // Logseq encodes ns/page as ns___page.md
        fs::write(pages.join("projects___bloom.md"), "body\n").unwrap();

        let config = ImportConfig {
            source_dir: src.path().to_path_buf(),
            target_dir: tgt.path().to_path_buf(),
            dry_run: false,
        };

        let report = import_logseq_vault(&config).unwrap();
        assert_eq!(report.pages_imported, 1);

        let written = fs::read_to_string(tgt.path().join("pages/bloom.md")).unwrap();
        assert!(written.contains("title: bloom"));
        assert!(written.contains("projects"));
    }
}
