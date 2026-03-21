//! Picker item collection and selection.
//!
//! Populates [`GenericPickerItem`](crate::GenericPickerItem) lists for each
//! [`PickerKind`](crate::keymap::dispatch::PickerKind) (pages, buffers, tags,
//! templates, themes, …) and handles selection, confirmation, and cancellation.

use crate::*;
use bloom_md::parser::traits::DocumentParser;

/// Stable key for storing per-picker-kind state.
pub(crate) fn picker_kind_key(kind: &keymap::dispatch::PickerKind) -> String {
    use keymap::dispatch::PickerKind;
    match kind {
        PickerKind::FindPage => "find_page".into(),
        PickerKind::PagesOnly => "pages_only".into(),
        PickerKind::SwitchBuffer => "switch_buffer".into(),
        PickerKind::Search => "search".into(),
        PickerKind::Journal => "journal".into(),
        PickerKind::Tags => "tags".into(),
        PickerKind::Backlinks(_) => "backlinks".into(),
        PickerKind::UnlinkedMentions(_) => "unlinked_mentions".into(),
        PickerKind::AllCommands => "all_commands".into(),
        PickerKind::InlineLink => "inline_link".into(),
        PickerKind::Templates => "templates".into(),
        PickerKind::Theme => "theme".into(),
    }
}

impl BloomEditor {
    pub(crate) fn open_picker(&mut self, kind: keymap::dispatch::PickerKind) {
        use keymap::dispatch::PickerKind;
        let (title, status_noun, items) = match &kind {
            PickerKind::FindPage => (
                "Find File".to_string(),
                "files".to_string(),
                self.collect_page_items(),
            ),
            PickerKind::PagesOnly => (
                "Find Page".to_string(),
                "pages".to_string(),
                self.collect_pages_only_items(),
            ),
            PickerKind::SwitchBuffer => {
                let items: Vec<GenericPickerItem> = self
                    .writer.buffers()
                    .open_buffers()
                    .iter()
                    .map(|info| {
                        let is_dirty = self.writer.buffers().get(&info.page_id)
                            .is_some_and(|b| b.is_dirty());
                        GenericPickerItem {
                            id: info.page_id.to_hex(),
                            label: info.title.clone(),
                            middle: if is_dirty { Some("[+]".to_string()) } else { None },
                            right: Some(info.path.display().to_string()),
                            preview_text: None,
                            score_boost: 0,
                        }
                    })
                    .collect();
                (
                    "Switch Buffer".to_string(),
                    "open buffers".to_string(),
                    items,
                )
            }
            PickerKind::Search => ("Search".to_string(), "matches".to_string(), Vec::new()),
            PickerKind::Journal => (
                "Journal".to_string(),
                "journal entries".to_string(),
                self.collect_journal_items(),
            ),
            PickerKind::Tags => {
                let items = if let Some(idx) = &self.index {
                    idx.all_tags()
                        .into_iter()
                        .map(|(tag, count)| GenericPickerItem {
                            id: tag.0.clone(),
                            label: format!("#{}", tag.0),
                            middle: None,
                            right: Some(format!("{count} pages")),
                            preview_text: None,
                            score_boost: 0,
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                ("Tags".to_string(), "tags".to_string(), items)
            }
            PickerKind::AllCommands => {
                let items: Vec<GenericPickerItem> = vec![
                    ("find_page", "Find page", "SPC f f"),
                    ("switch_buffer", "Switch buffer", "SPC b b"),
                    ("journal_today", "Journal today", "SPC j j"),
                    ("search", "Search", "SPC s s"),
                    ("search_tags", "Search tags", "SPC s t"),
                    ("split_vertical", "Split vertical", "SPC w v"),
                    ("split_horizontal", "Split horizontal", "SPC w s"),
                    ("agenda", "Agenda", "SPC a a"),
                    ("undo_tree", "Undo tree", "SPC u u"),
                    ("theme_selector", "Theme selector", "SPC T t"),
                    ("new_from_template", "New from template", "SPC n"),
                    ("rebuild_index", "Rebuild index", "SPC h r"),
                ]
                .into_iter()
                .map(|(id, label, keys)| GenericPickerItem {
                    id: id.to_string(),
                    label: label.to_string(),
                    middle: Some(keys.to_string()),
                    right: None,
                    preview_text: None,
                    score_boost: 0,
                })
                .collect();
                ("All Commands".to_string(), "commands".to_string(), items)
            }
            PickerKind::Templates => {
                let mut items: Vec<GenericPickerItem> =
                    template::builtins::builtin_templates()
                        .into_iter()
                        .map(|t| {
                            let placeholder_count =
                                t.placeholders.iter().filter(|p| p.index != 0).count();
                            GenericPickerItem {
                                id: t.name.clone(),
                                label: t.name.clone(),
                                middle: Some(t.description.clone()),
                                right: if placeholder_count > 0 {
                                    Some(format!("{placeholder_count} fields"))
                                } else {
                                    None
                                },
                                preview_text: Some(t.content.clone()),
                                score_boost: 1, // sort above user templates
                            }
                        })
                        .collect();
                if let Some(engine) = &self.template_engine {
                    items.extend(engine.list().into_iter().map(|t| {
                        let placeholder_count = t.placeholders.len();
                        GenericPickerItem {
                            id: t.name.clone(),
                            label: t.name.clone(),
                            middle: Some(t.description.clone()),
                            right: if placeholder_count > 0 {
                                Some(format!("{placeholder_count} fields"))
                            } else {
                                None
                            },
                            preview_text: Some(t.content.clone()),
                            score_boost: 0,
                        }
                    }));
                }
                ("Templates".to_string(), "templates".to_string(), items)
            }
            PickerKind::Backlinks(page_id) => {
                let items = if let Some(idx) = &self.index {
                    idx.backlinks_to(page_id)
                        .into_iter()
                        .map(|bl| GenericPickerItem {
                            id: bl.source_page.id.to_hex(),
                            label: bl.source_page.title.clone(),
                            middle: Some(bl.context.clone()),
                            right: Some(format!("line {}", bl.line)),
                            preview_text: Some(bl.context),
                            score_boost: 0,
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                ("Backlinks".to_string(), "backlinks".to_string(), items)
            }
            PickerKind::UnlinkedMentions(page_id) => {
                let items = if let Some(idx) = &self.index {
                    let title = idx
                        .find_page_by_id(page_id)
                        .map(|m| m.title.clone())
                        .unwrap_or_default();
                    if title.is_empty() {
                        Vec::new()
                    } else {
                        idx.unlinked_mentions(&title)
                            .into_iter()
                            .map(|um| GenericPickerItem {
                                id: format!("{}:{}", um.source_page.path.display(), um.line),
                                label: um.source_page.title.clone(),
                                middle: Some(um.context.clone()),
                                right: Some(format!("line {}", um.line)),
                                preview_text: Some(um.context),
                                score_boost: 0,
                            })
                            .collect()
                    }
                } else {
                    Vec::new()
                };
                (
                    "Unlinked Mentions".to_string(),
                    "mentions".to_string(),
                    items,
                )
            }
            PickerKind::InlineLink => (
                "Insert Link".to_string(),
                "pages".to_string(),
                self.collect_page_items(),
            ),
            PickerKind::Theme => {
                // Theme picker is handled by open_theme_picker()
                self.open_theme_picker();
                return;
            }
        };
        let min_query_len = match &kind {
            PickerKind::Search => 2,
            _ => 0,
        };
        let match_mode = match &kind {
            PickerKind::Search => picker::MatchMode::AllWords,
            _ => picker::MatchMode::Fuzzy,
        };
        let action = match &kind {
            PickerKind::FindPage | PickerKind::PagesOnly | PickerKind::Journal => {
                crate::PickerAction::OpenPage
            }
            PickerKind::SwitchBuffer => crate::PickerAction::SwitchBuffer,
            PickerKind::Search => crate::PickerAction::SearchJump,
            PickerKind::AllCommands => crate::PickerAction::ExecuteCommand,
            PickerKind::InlineLink => crate::PickerAction::InsertLink,
            PickerKind::Templates => crate::PickerAction::ExpandTemplate,
            PickerKind::Tags => crate::PickerAction::Noop,
            PickerKind::Backlinks(_) | PickerKind::UnlinkedMentions(_) => {
                crate::PickerAction::OpenPage
            }
            PickerKind::Theme => crate::PickerAction::ApplyTheme,
        };
        self.picker_state = Some(ActivePicker {
            kind: kind.clone(),
            action,
            picker: picker::Picker::with_match_mode(items, match_mode),
            title,
            query: String::new(),
            status_noun,
            min_query_len,
            previous_theme: None,
            query_selected: false,
        });

        // Restore last query for this picker kind (select-all so typing replaces)
        let key = picker_kind_key(&kind);
        if let Some(last_query) = self.last_picker_queries.get(&key).cloned() {
            if !last_query.is_empty() {
                if let Some(ap) = &mut self.picker_state {
                    ap.query = last_query;
                    ap.query_selected = true;
                    ap.picker.set_query(&ap.query);
                }
                // Search uses FTS re-query instead of client-side filtering
                if matches!(kind, PickerKind::Search) {
                    self.refresh_search_picker();
                }
            }
        }
    }

    pub(crate) fn collect_page_items(&self) -> Vec<GenericPickerItem> {
        // Prefer the SQLite index (instant) over disk scan (reads every file).
        if let Some(idx) = &self.index {
            // Zero-query: show frecency-ordered pages (recently used first).
            // The picker's fuzzy filter re-ranks when the user types.
            let frecent = idx.frecency_top(20);
            let frecent_ids: std::collections::HashSet<_> =
                frecent.iter().map(|m| m.id.clone()).collect();

            let mut items: Vec<GenericPickerItem> = frecent
                .into_iter()
                .map(|meta| {
                    let tags = meta
                        .tags
                        .iter()
                        .map(|t| format!("#{}", t.0))
                        .collect::<Vec<_>>()
                        .join(" ");
                    let date = Some(meta.created.format("%b %d").to_string());
                    GenericPickerItem {
                        id: meta.id.to_hex(),
                        label: meta.title,
                        middle: if tags.is_empty() { None } else { Some(tags) },
                        right: date,
                        preview_text: None,
                        score_boost: 0,
                    }
                })
                .collect();

            // Append remaining pages after the frecent ones
            let all_pages = idx.list_pages(None);
            for meta in all_pages {
                if frecent_ids.contains(&meta.id) {
                    continue;
                }
                let tags = meta
                    .tags
                    .iter()
                    .map(|t| format!("#{}", t.0))
                    .collect::<Vec<_>>()
                    .join(" ");
                let date = Some(meta.created.format("%b %d").to_string());
                items.push(GenericPickerItem {
                    id: meta.id.to_hex(),
                    label: meta.title,
                    middle: if tags.is_empty() { None } else { Some(tags) },
                    right: date,
                    preview_text: None,
                    score_boost: 0,
                });
            }

            if !items.is_empty() {
                return items;
            }
        }

        // Fallback: scan disk (index not ready yet or empty)
        if let Some(root) = &self.vault_root {
            let pages_dir = root.join("pages");
            if pages_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&pages_dir) {
                    return entries
                        .filter_map(|e| {
                            let e = e.ok()?;
                            let path = e.path();
                            if path.extension()?.to_str()? != "md" {
                                return None;
                            }
                            let content = std::fs::read_to_string(&path).ok()?;
                            let fm = self.parser.parse_frontmatter(&content)?;
                            let title = fm.title.unwrap_or_else(|| {
                                path.file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string()
                            });
                            let tags = fm
                                .tags
                                .iter()
                                .map(|t| format!("#{}", t.0))
                                .collect::<Vec<_>>()
                                .join(" ");
                            let preview = content.lines().take(10).collect::<Vec<_>>().join("\n");
                            let date = fm.created.map(|d| d.format("%b %d").to_string());
                            Some(GenericPickerItem {
                                id: path.to_string_lossy().to_string(),
                                label: title,
                                middle: if tags.is_empty() { None } else { Some(tags) },
                                right: date,
                                preview_text: Some(preview),
                                score_boost: 0,
                            })
                        })
                        .collect();
                }
            }
        }
        Vec::new()
    }

    /// Collect only pages/ items (exclude journal/).
    pub(crate) fn collect_pages_only_items(&self) -> Vec<GenericPickerItem> {
        self.collect_page_items()
            .into_iter()
            .filter(|item| {
                // Exclude journal entries: they have labels matching YYYY-MM-DD
                !item.label.starts_with("20")
                    || chrono::NaiveDate::parse_from_str(&item.label, "%Y-%m-%d").is_err()
            })
            .collect()
    }

    pub(crate) fn collect_journal_items(&self) -> Vec<GenericPickerItem> {
        // Prefer index — journal pages are tagged "journal" and stored in journal/ path.
        if let Some(idx) = &self.index {
            let tag = types::TagName("journal".to_string());
            let pages = idx.pages_with_tag(&tag);
            if !pages.is_empty() {
                let mut items: Vec<GenericPickerItem> = pages
                    .into_iter()
                    .map(|meta| GenericPickerItem {
                        id: meta.id.to_hex(),
                        label: meta.title,
                        middle: None,
                        right: Some("journal".to_string()),
                        preview_text: None,
                        score_boost: 0,
                    })
                    .collect();
                items.sort_by(|a, b| b.label.cmp(&a.label)); // most recent first
                return items;
            }
        }

        // Fallback: scan disk
        if let Some(root) = &self.vault_root {
            let journal_dir = root.join("journal");
            if journal_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&journal_dir) {
                    let mut items: Vec<GenericPickerItem> = entries
                        .filter_map(|e| {
                            let e = e.ok()?;
                            let path = e.path();
                            if path.extension()?.to_str()? != "md" {
                                return None;
                            }
                            let stem = path.file_stem()?.to_string_lossy().to_string();
                            let preview = std::fs::read_to_string(&path)
                                .ok()
                                .map(|c| c.lines().take(8).collect::<Vec<_>>().join("\n"));
                            Some(GenericPickerItem {
                                id: path.to_string_lossy().to_string(),
                                label: stem,
                                middle: None,
                                right: Some("journal".to_string()),
                                preview_text: preview,
                                score_boost: 0,
                            })
                        })
                        .collect();
                    items.sort_by(|a, b| b.label.cmp(&a.label)); // most recent first
                    return items;
                }
            }
        }
        Vec::new()
    }

    pub(crate) fn collect_search_items(&self) -> (String, Vec<GenericPickerItem>) {
        // Collect content lines from pages and journals for full-text search.
        // Each item is a single line with source page title as right column.
        let mut items = Vec::new();
        let mut page_titles = std::collections::HashSet::new();
        if let Some(root) = &self.vault_root {
            for subdir in &["pages", "journal"] {
                let is_journal = *subdir == "journal";
                let dir = root.join(subdir);
                if !dir.exists() {
                    continue;
                }
                let entries = match std::fs::read_dir(&dir) {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("md") {
                        continue;
                    }
                    let content = match std::fs::read_to_string(&path) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    let fm = self.parser.parse_frontmatter(&content);
                    let title = fm.and_then(|f| f.title).unwrap_or_else(|| {
                        path.file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    });
                    let display_title = if is_journal {
                        format!("{} (journal)", title)
                    } else {
                        title.clone()
                    };

                    let lines: Vec<&str> = content.lines().collect();

                    // Skip frontmatter region
                    let body_start = if lines.first().is_some_and(|l| l.trim() == "---") {
                        lines
                            .iter()
                            .skip(1)
                            .position(|l| l.trim() == "---")
                            .map(|i| i + 2) // +1 for skip(1), +1 to get past closing ---
                            .unwrap_or(0)
                    } else {
                        0
                    };

                    let mut page_has_items = false;
                    for (line_num, line_text) in lines.iter().enumerate() {
                        if line_num < body_start {
                            continue;
                        }
                        let trimmed = line_text.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        // Build ±5 lines of context with ❯ marker on matching line
                        let ctx_start = line_num.saturating_sub(5);
                        let ctx_end = (line_num + 6).min(lines.len());
                        let preview: String = lines[ctx_start..ctx_end]
                            .iter()
                            .enumerate()
                            .map(|(i, l)| {
                                if ctx_start + i == line_num {
                                    format!("❯ {l}")
                                } else {
                                    format!("  {l}")
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n");

                        items.push(GenericPickerItem {
                            id: format!("{}:{}", path.display(), line_num),
                            label: trimmed.to_string(),
                            middle: None,
                            right: Some(display_title.clone()),
                            preview_text: Some(preview),
                            score_boost: 0,
                        });
                        page_has_items = true;
                    }
                    if page_has_items {
                        page_titles.insert(title);
                    }
                }
            }
        }
        let page_count = page_titles.len();
        let noun = format!(
            "matches across {} {}",
            page_count,
            if page_count == 1 { "page" } else { "pages" }
        );
        (noun, items)
    }

    /// Re-query the FTS index for the current search picker query.
    /// Called on each keystroke instead of client-side filtering.
    pub(crate) fn refresh_search_picker(&mut self) {
        let query = match &self.picker_state {
            Some(ap) => ap.query.clone(),
            None => return,
        };

        if query.len() < 2 {
            if let Some(ap) = &mut self.picker_state {
                ap.picker.replace_items(Vec::new());
                ap.status_noun = "matches".to_string();
            }
            return;
        }

        // Two-phase search: FTS5 prefix candidates → nucleo fuzzy scoring
        let items = if let Some(idx) = &self.index {
            let vault_root = self.vault_root.clone().unwrap_or_default();

            // Phase 1: FTS5 prefix query for candidate pages + content
            let candidates = idx.search_candidates(&query);

            // Phase 2: Fuzzy-score individual lines with nucleo
            let mut scored: Vec<(GenericPickerItem, u32)> = Vec::new();
            let mut page_set = std::collections::HashSet::new();

            for (meta, content) in &candidates {
                let full_path = vault_root.join(&meta.path);
                let lines: Vec<&str> = content.lines().collect();

                // Skip frontmatter
                let body_start = if lines.first().is_some_and(|l| l.trim() == "---") {
                    lines
                        .iter()
                        .skip(1)
                        .position(|l| l.trim() == "---")
                        .map(|i| i + 2)
                        .unwrap_or(0)
                } else {
                    0
                };

                for (line_num, line_text) in lines.iter().enumerate() {
                    if line_num < body_start || line_text.trim().is_empty() {
                        continue;
                    }
                    if let Some(score) = picker::nucleo::fuzzy_words_score(&query, line_text) {
                        // Boost score for lines containing exact substring matches
                        let lower_line = line_text.to_lowercase();
                        let exact_bonus: u32 = query
                            .split_whitespace()
                            .filter(|w| lower_line.contains(&w.to_lowercase()))
                            .count() as u32
                            * 1000;
                        page_set.insert(meta.id.clone());
                        scored.push((
                            GenericPickerItem {
                                id: format!("{}:{}", full_path.display(), line_num),
                                label: line_text.trim().to_string(),
                                middle: Some(format!("L{}", line_num + 1)),
                                right: Some(meta.title.clone()),
                                preview_text: None,
                                score_boost: 0,
                            },
                            score + exact_bonus,
                        ));
                    }
                }
            }

            // Sort by score descending, cap at 500
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            scored.truncate(500);

            let page_count = page_set.len();
            if let Some(ap) = &mut self.picker_state {
                ap.status_noun = format!(
                    "matches across {} {}",
                    page_count,
                    if page_count == 1 { "page" } else { "pages" },
                );
            }
            scored.into_iter().map(|(item, _)| item).collect()
        } else {
            // Index not ready — fall back to disk scan
            let (_noun, items) = self.collect_search_items();
            if let Some(ap) = &mut self.picker_state {
                ap.status_noun = _noun;
            }
            items
        };

        if let Some(ap) = &mut self.picker_state {
            ap.picker.replace_items(items);
        }
    }

    pub(crate) fn open_theme_picker(&mut self) {
        let current_name = self.active_theme.name;
        let previous_theme = self.active_theme;
        let sample = "## Preview\n\n- [ ] Sample task @due(2026-03-05)\n- [x] Completed task\nSee [[abc123|Text Editor Theory]].\n#rust #editors";
        let items: Vec<GenericPickerItem> = bloom_md::theme::THEME_NAMES
            .iter()
            .map(|name| {
                let current_marker = if *name == current_name {
                    "(current)"
                } else {
                    ""
                };
                let desc = bloom_md::theme::theme_description(name);
                let right = if current_marker.is_empty() {
                    if desc.is_empty() {
                        None
                    } else {
                        Some(desc.to_string())
                    }
                } else {
                    Some(format!(
                        "{}{}{}",
                        desc,
                        if desc.is_empty() { "" } else { "  " },
                        current_marker,
                    ))
                };
                GenericPickerItem {
                    id: name.to_string(),
                    label: name.to_string(),
                    middle: None,
                    right,
                    preview_text: Some(sample.to_string()),
                    score_boost: 0,
                }
            })
            .collect();
        let current_idx = bloom_md::theme::THEME_NAMES
            .iter()
            .position(|n| *n == current_name)
            .unwrap_or(0);
        let mut picker = picker::Picker::new(items);
        // Pre-select the current theme
        for _ in 0..current_idx {
            picker.move_selection(1);
        }
        self.picker_state = Some(ActivePicker {
            kind: keymap::dispatch::PickerKind::Theme,
            action: crate::PickerAction::ApplyTheme,
            picker,
            title: "Theme".to_string(),
            query: String::new(),
            status_noun: "themes".to_string(),
            min_query_len: 0,
            previous_theme: Some(previous_theme),
            query_selected: false,
        });
    }

    /// Live-preview theme on picker selection change.
    pub(crate) fn theme_picker_preview_current(&mut self) {
        if let Some(ap) = &self.picker_state {
            if matches!(ap.kind, keymap::dispatch::PickerKind::Theme) {
                if let Some(item) = ap.picker.selected() {
                    if let Some(palette) = bloom_md::theme::palette_by_name(&item.id) {
                        self.active_theme = palette;
                    }
                }
            }
        }
    }

    pub(crate) fn theme_picker_confirm(&mut self) {
        if let Some(ap) = self.picker_state.take() {
            if !matches!(ap.kind, keymap::dispatch::PickerKind::Theme) {
                return;
            }
            if let Some(selected) = ap.picker.selected() {
                let name = selected.id.clone();
                self.set_theme(&name);
                self.persist_theme_to_config();
            }
        }
    }

    pub(crate) fn theme_picker_cancel(&mut self) {
        if let Some(ap) = self.picker_state.take() {
            if let Some(prev) = ap.previous_theme {
                self.active_theme = prev;
            }
        }
    }

    pub(crate) fn handle_picker_selection(
        &mut self,
        action: &crate::PickerAction,
        item: GenericPickerItem,
    ) {
        use crate::PickerAction;
        match action {
            PickerAction::OpenPage => {
                // item.id is either a PageId hex (from index) or a full path (disk fallback).
                if let Some(page_id) = types::PageId::from_hex(&item.id) {
                    // Index-based: use the real page ID
                    if self.writer.buffers().is_open(&page_id) {
                        // Already open — just switch to it
                        self.set_active_page(Some(page_id));
                        self.set_cursor(0);
                        return;
                    }
                    if let Some(idx) = &self.index {
                        if let Some(meta) = idx.find_page_by_id(&page_id) {
                            let full = self
                                .vault_root
                                .as_ref()
                                .map(|r| r.join(&meta.path))
                                .unwrap_or_else(|| meta.path.clone());
                            if let Ok(content) = std::fs::read_to_string(&full) {
                                self.open_page_with_content(&page_id, &meta.title, &full, &content);
                            }
                        }
                    }
                } else {
                    // Disk fallback: parse frontmatter for the real ID
                    let path = std::path::PathBuf::from(&item.id);
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let fm = self.parser.parse_frontmatter(&content);
                        let title = fm
                            .as_ref()
                            .and_then(|f| f.title.clone())
                            .unwrap_or_else(|| item.label.clone());
                        let id = fm
                            .and_then(|f| f.id)
                            .unwrap_or_else(crate::uuid::generate_hex_id);
                        if self.writer.buffers().is_open(&id) {
                            self.set_active_page(Some(id));
                            self.set_cursor(0);
                        } else {
                            self.open_page_with_content(&id, &title, &path, &content);
                        }
                    }
                }
            }
            PickerAction::SearchJump => {
                // item.id is "path:line_number" — open page and jump to line
                if let Some(colon) = item.id.rfind(':') {
                    let path_str = &item.id[..colon];
                    let line_num: usize = item.id[colon + 1..].parse().unwrap_or(0);
                    let path = std::path::PathBuf::from(path_str);
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let fm = self.parser.parse_frontmatter(&content);
                        let title = fm
                            .as_ref()
                            .and_then(|f| f.title.clone())
                            .unwrap_or_else(|| item.label.clone());
                        let id = fm
                            .and_then(|f| f.id)
                            .unwrap_or_else(crate::uuid::generate_hex_id);
                        if self.writer.buffers().is_open(&id) {
                            self.set_active_page(Some(id));
                        } else {
                            self.open_page_with_content(&id, &title, &path, &content);
                        }
                        // Jump to line
                        if let Some(page_id) = self.active_page().cloned() {
                            if let Some(buf) = self.writer.buffers().get(&page_id) {
                                let target_char = buf
                                    .text()
                                    .line_to_char(line_num.min(buf.len_lines().saturating_sub(1)));
                                self.set_cursor(target_char);
                            }
                        }
                    }
                }
            }
            PickerAction::SwitchBuffer => {
                if let Some(page_id) = types::PageId::from_hex(&item.id) {
                    self.set_active_page(Some(page_id));
                    self.set_cursor(0);
                }
            }
            PickerAction::ExecuteCommand => {
                let actions = self.action_id_to_actions(&item.id);
                let _ = self.execute_actions(actions);
            }
            PickerAction::ExpandTemplate => {
                // Look up built-in templates first, then fall back to user templates.
                let builtin = template::builtins::builtin_templates()
                    .into_iter()
                    .find(|t| t.name == item.id);

                if let Some(bt) = builtin {
                    let values = std::collections::HashMap::new();
                    let title = "Untitled";
                    let expanded =
                        template::TemplateEngine::expand_content(&bt.content, title, &values);
                    let id = crate::uuid::generate_hex_id();
                    let path = self
                        .vault_root
                        .as_ref()
                        .map(|r| r.join("pages").join(format!("{}.md", title)))
                        .unwrap_or_else(|| std::path::PathBuf::from(format!("{}.md", title)));
                    self.open_page_with_content(&id, title, &path, &expanded.content);
                    if !expanded.tab_stops.is_empty() {
                        self.template_mode =
                            Some(template::TemplateModeState::new(expanded.tab_stops));
                    }
                } else if let Some(engine) = &self.template_engine {
                    let templates = engine.list();
                    if let Some(tmpl) = templates.iter().find(|t| t.name == item.id) {
                        let values = std::collections::HashMap::new();
                        let title = tmpl.name.clone();
                        let expanded = engine.expand(tmpl, &title, &values);
                        let id = crate::uuid::generate_hex_id();
                        let path = self
                            .vault_root
                            .as_ref()
                            .map(|r| r.join("pages").join(format!("{}.md", title)))
                            .unwrap_or_else(|| std::path::PathBuf::from(format!("{}.md", title)));
                        self.open_page_with_content(&id, &title, &path, &expanded.content);
                        if !expanded.tab_stops.is_empty() {
                            self.template_mode =
                                Some(template::TemplateModeState::new(expanded.tab_stops));
                        }
                    }
                }
            }
            PickerAction::InsertLink => {
                if let Some(page_id) = self.active_page().cloned() {
                    if self.writer.buffers().get(&page_id).is_some() {
                        let link_text = format!("[[{}|{}]]", item.id, item.label);
                        self.insert_text_at_cursor(&link_text);
                    }
                }
            }
            PickerAction::ApplyTheme | PickerAction::Noop => {}
        }
    }
}
