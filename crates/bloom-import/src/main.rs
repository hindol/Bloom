use bloom_core::parser::{BloomMarkdownParser, traits::DocumentParser};
use bloom_core::uuid::generate_hex_id;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: bloom-import <logseq-dir> <bloom-vault>");
        std::process::exit(1);
    }
    let logseq_dir = Path::new(&args[1]);
    let bloom_vault = Path::new(&args[2]);

    match import_logseq(logseq_dir, bloom_vault) {
        Ok(stats) => {
            println!("✓ Import complete: {} pages, {} journals", stats.pages, stats.journals);
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

struct ImportStats {
    pages: usize,
    journals: usize,
}

fn import_logseq(logseq_dir: &Path, bloom_vault: &Path) -> Result<ImportStats, String> {
    let pages_src = logseq_dir.join("pages");
    let journals_src = logseq_dir.join("journals");

    let pages_dst = bloom_vault.join("pages");
    let journals_dst = bloom_vault.join("journal");

    std::fs::create_dir_all(&pages_dst).map_err(|e| format!("Create pages dir: {e}"))?;
    std::fs::create_dir_all(&journals_dst).map_err(|e| format!("Create journal dir: {e}"))?;

    let parser = BloomMarkdownParser::new();
    let mut title_to_id: HashMap<String, String> = HashMap::new();
    let mut stats = ImportStats { pages: 0, journals: 0 };

    // Pass 1: Assign IDs to all pages
    if pages_src.exists() {
        if let Ok(entries) = std::fs::read_dir(&pages_src) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
                let title = path.file_stem().unwrap_or_default()
                    .to_string_lossy()
                    .replace("%2F", "/")
                    .replace("___", "/");
                let id = generate_hex_id();
                title_to_id.insert(title.to_string(), id.to_hex());
            }
        }
    }

    // Pass 2: Convert and write pages
    if pages_src.exists() {
        if let Ok(entries) = std::fs::read_dir(&pages_src) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
                let title = path.file_stem().unwrap_or_default()
                    .to_string_lossy()
                    .replace("%2F", "/")
                    .replace("___", "/");
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Read {}: {e}", path.display()))?;
                let id_hex = title_to_id.get(&title.to_string()).unwrap();

                // Convert Logseq format to Bloom format
                let converted = convert_logseq_content(&content, &title_to_id);
                let bloom_content = format!(
                    "---\nid: {id_hex}\ntitle: \"{title}\"\ncreated: {}\ntags: []\n---\n\n{converted}",
                    chrono::Local::now().format("%Y-%m-%d")
                );

                let filename = format!("{}-{id_hex}.md",
                    title.to_lowercase().replace(' ', "-").replace('/', "-"));
                let dst_path = pages_dst.join(&filename);

                // Idempotent: skip if file exists with same ID
                if !dst_path.exists() {
                    std::fs::write(&dst_path, &bloom_content)
                        .map_err(|e| format!("Write {}: {e}", dst_path.display()))?;
                }
                stats.pages += 1;
            }
        }
    }

    // Pass 3: Convert journal entries
    if journals_src.exists() {
        if let Ok(entries) = std::fs::read_dir(&journals_src) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
                let stem = path.file_stem().unwrap_or_default().to_string_lossy();

                // Logseq journals: YYYY_MM_DD.md → YYYY-MM-DD.md
                let date_str = stem.replace('_', "-");
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Read {}: {e}", path.display()))?;
                let converted = convert_logseq_content(&content, &title_to_id);
                let bloom_content = format!(
                    "---\ntitle: \"{date_str}\"\ncreated: {date_str}\ntags: [journal]\n---\n\n{converted}"
                );

                let dst_path = journals_dst.join(format!("{date_str}.md"));
                if !dst_path.exists() {
                    std::fs::write(&dst_path, &bloom_content)
                        .map_err(|e| format!("Write {}: {e}", dst_path.display()))?;
                }
                stats.journals += 1;
            }
        }
    }

    Ok(stats)
}

/// Convert Logseq Markdown to Bloom Markdown.
/// - Convert `[[Page Name]]` links to `[[id|Page Name]]`
/// - Convert `TODO`/`DONE` to `- [ ]`/`- [x]`
/// - Strip Logseq-specific properties
fn convert_logseq_content(content: &str, title_to_id: &HashMap<String, String>) -> String {
    let mut result = String::new();
    for line in content.lines() {
        let trimmed = line.trim();

        // Skip Logseq property lines (key:: value)
        if trimmed.contains(":: ") && !trimmed.starts_with('-') && !trimmed.starts_with('#') {
            continue;
        }

        let mut converted = line.to_string();

        // Convert TODO/DONE markers
        if converted.contains("TODO ") {
            converted = converted.replacen("TODO ", "[ ] ", 1);
        }
        if converted.contains("DONE ") {
            converted = converted.replacen("DONE ", "[x] ", 1);
        }

        // Convert [[Page Name]] links to [[id|Page Name]]
        let mut output = String::new();
        let mut chars = converted.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '[' && chars.peek() == Some(&'[') {
                chars.next(); // consume second [
                let mut link_text = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ']' {
                        chars.next();
                        if chars.peek() == Some(&']') {
                            chars.next();
                            break;
                        }
                        link_text.push(']');
                    } else {
                        link_text.push(c);
                        chars.next();
                    }
                }
                if let Some(id) = title_to_id.get(&link_text) {
                    output.push_str(&format!("[[{id}|{link_text}]]"));
                } else {
                    output.push_str(&format!("[[{link_text}]]"));
                }
            } else {
                output.push(ch);
            }
        }

        result.push_str(&output);
        result.push('\n');
    }
    result
}