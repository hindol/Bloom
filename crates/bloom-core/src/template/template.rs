use chrono::Local;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

pub struct TemplateEngine {
    templates_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Template {
    pub name: String,
    pub description: String,
    pub content: String,
    pub placeholders: Vec<Placeholder>,
}

#[derive(Debug, Clone)]
pub struct Placeholder {
    pub index: usize,
    pub description: String,
    pub occurrences: Vec<Range<usize>>,
}

#[derive(Debug, Clone)]
pub struct ExpandedTemplate {
    pub content: String,
    pub tab_stops: Vec<TabStop>,
}

#[derive(Debug, Clone)]
pub struct TabStop {
    pub index: usize,
    pub ranges: Vec<Range<usize>>,
    pub default_text: String,
}

#[derive(Debug, Clone)]
pub struct MirrorEdit {
    pub range: Range<usize>,
    pub new_text: String,
}

pub enum TemplateAdvanceResult {
    NextStop {
        cursor_target: Range<usize>,
        mirror_edits: Vec<MirrorEdit>,
    },
    FinalCursor {
        cursor_target: usize,
        mirror_edits: Vec<MirrorEdit>,
    },
    Done {
        mirror_edits: Vec<MirrorEdit>,
    },
}

pub struct TemplateModeState {
    tab_stops: Vec<TabStop>,
    current_index: usize,
    active: bool,
}

impl TemplateEngine {
    pub fn new(templates_dir: &Path) -> Self {
        TemplateEngine {
            templates_dir: templates_dir.to_path_buf(),
        }
    }

    /// List all available templates by reading .md files from the templates directory.
    pub fn list(&self) -> Vec<Template> {
        let dir = match std::fs::read_dir(&self.templates_dir) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };

        let mut templates = Vec::new();
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    let placeholders = Self::parse_placeholders(&content);
                    templates.push(Template {
                        name: name.clone(),
                        description: name,
                        content,
                        placeholders,
                    });
                }
            }
        }
        templates
    }

    /// Expand a template: fill ${AUTO}, ${DATE}, ${TITLE}, return content with tab stops.
    pub fn expand(
        &self,
        template: &Template,
        title: &str,
        values: &HashMap<usize, String>,
    ) -> ExpandedTemplate {
        let uuid_hex = crate::uuid::generate_hex_id().to_hex();
        let today = Local::now().format("%Y-%m-%d").to_string();

        // Phase 1: replace magic variables
        let mut content = template.content.clone();
        content = content.replace("${AUTO}", &uuid_hex);
        content = content.replace("${DATE}", &today);
        content = content.replace("${TITLE}", title);

        // Phase 2: replace numbered placeholders ${N:description} and $0
        let mut result = String::new();
        let mut tab_stops_vec: Vec<TabStop> = Vec::new();

        // Track tab stop positions after expansion
        let mut stop_map: HashMap<usize, (String, Vec<Range<usize>>)> = HashMap::new();

        let bytes = content.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            if bytes[i] == b'$' {
                // Check for ${N:description}
                if i + 1 < len && bytes[i + 1] == b'{' {
                    if let Some(close) = content[i..].find('}') {
                        let inner = &content[i + 2..i + close];
                        if let Some(colon_pos) = inner.find(':') {
                            if let Ok(idx) = inner[..colon_pos].parse::<usize>() {
                                let desc = &inner[colon_pos + 1..];
                                let replacement = values
                                    .get(&idx)
                                    .cloned()
                                    .unwrap_or_else(|| desc.to_string());
                                let start = result.len();
                                result.push_str(&replacement);
                                let end = result.len();

                                let entry = stop_map
                                    .entry(idx)
                                    .or_insert_with(|| (desc.to_string(), Vec::new()));
                                entry.1.push(start..end);

                                i += close + 1;
                                continue;
                            }
                        }
                    }
                }
                // Check for $0 (final cursor)
                if i + 1 < len && bytes[i + 1] == b'0' {
                    let pos = result.len();
                    let entry = stop_map
                        .entry(0)
                        .or_insert_with(|| (String::new(), Vec::new()));
                    entry.1.push(pos..pos);
                    i += 2;
                    continue;
                }
            }
            result.push(content.as_bytes()[i] as char);
            i += 1;
        }

        // Build sorted tab stops (1, 2, 3, ... then 0 at end)
        let mut indices: Vec<usize> = stop_map.keys().copied().collect();
        indices.sort_by_key(|&idx| if idx == 0 { usize::MAX } else { idx });

        for idx in indices {
            if let Some((desc, ranges)) = stop_map.remove(&idx) {
                tab_stops_vec.push(TabStop {
                    index: idx,
                    ranges,
                    default_text: desc,
                });
            }
        }

        ExpandedTemplate {
            content: result,
            tab_stops: tab_stops_vec,
        }
    }

    /// Get tab-stop positions in expanded content.
    pub fn tab_stops(&self, expanded: &ExpandedTemplate) -> Vec<TabStop> {
        expanded.tab_stops.clone()
    }

    /// Parse template content to find all placeholders.
    fn parse_placeholders(content: &str) -> Vec<Placeholder> {
        let mut placeholders: HashMap<usize, Placeholder> = HashMap::new();
        let bytes = content.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            if bytes[i] == b'$' {
                if i + 1 < len && bytes[i + 1] == b'{' {
                    if let Some(close) = content[i..].find('}') {
                        let inner = &content[i + 2..i + close];
                        if let Some(colon_pos) = inner.find(':') {
                            if let Ok(idx) = inner[..colon_pos].parse::<usize>() {
                                let desc = inner[colon_pos + 1..].to_string();
                                let range = i..i + close + 1;
                                let entry = placeholders.entry(idx).or_insert_with(|| {
                                    Placeholder {
                                        index: idx,
                                        description: desc.clone(),
                                        occurrences: Vec::new(),
                                    }
                                });
                                entry.occurrences.push(range);
                                i += close + 1;
                                continue;
                            }
                        }
                    }
                }
                // $0 final cursor
                if i + 1 < len && bytes[i + 1] == b'0' {
                    let range = i..i + 2;
                    let entry = placeholders.entry(0).or_insert_with(|| Placeholder {
                        index: 0,
                        description: String::new(),
                        occurrences: Vec::new(),
                    });
                    entry.occurrences.push(range);
                    i += 2;
                    continue;
                }
            }
            i += 1;
        }

        let mut result: Vec<Placeholder> = placeholders.into_values().collect();
        result.sort_by_key(|p| if p.index == 0 { usize::MAX } else { p.index });
        result
    }
}

impl TemplateModeState {
    pub fn new(tab_stops: Vec<TabStop>) -> Self {
        TemplateModeState {
            active: !tab_stops.is_empty(),
            tab_stops,
            current_index: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn current_stop(&self) -> Option<&TabStop> {
        if !self.active {
            return None;
        }
        self.tab_stops.get(self.current_index)
    }

    /// Advance to the next tab stop. Returns mirror edits for all other
    /// occurrences of the current stop.
    pub fn advance(&mut self, typed_text: &str) -> TemplateAdvanceResult {
        if !self.active || self.current_index >= self.tab_stops.len() {
            self.active = false;
            return TemplateAdvanceResult::Done {
                mirror_edits: Vec::new(),
            };
        }

        // Build mirror edits for the current stop (all occurrences except the first)
        let current = &self.tab_stops[self.current_index];
        let mirror_edits: Vec<MirrorEdit> = current
            .ranges
            .iter()
            .skip(1)
            .map(|r| MirrorEdit {
                range: r.clone(),
                new_text: typed_text.to_string(),
            })
            .collect();

        self.current_index += 1;

        // Check if next stop is $0 (final cursor)
        if self.current_index < self.tab_stops.len() {
            let next = &self.tab_stops[self.current_index];
            if next.index == 0 {
                self.active = false;
                let cursor = next.ranges.first().map(|r| r.start).unwrap_or(0);
                return TemplateAdvanceResult::FinalCursor {
                    cursor_target: cursor,
                    mirror_edits,
                };
            }
            let cursor_target = next
                .ranges
                .first()
                .cloned()
                .unwrap_or(0..0);
            TemplateAdvanceResult::NextStop {
                cursor_target,
                mirror_edits,
            }
        } else {
            self.active = false;
            TemplateAdvanceResult::Done { mirror_edits }
        }
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }
}