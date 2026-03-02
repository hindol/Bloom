use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Local, NaiveDate};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::index::{IndexError, IndexedPage, SqliteIndex, TagCount};
use crate::resolver::{Resolver, UnlinkedMention};

/// Data provider for a picker surface (pages, buffers, tags, commands, ...).
pub trait PickerSource {
    type Item;

    /// Full candidate list before query/filter narrowing.
    fn items(&self) -> &[Self::Item];

    /// Main display text used for fuzzy matching.
    fn text(&self, item: &Self::Item) -> Cow<'_, str>;

    /// Optional right-side metadata.
    fn marginalia(&self, _item: &Self::Item) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    /// Optional `(id, display_text)` pair for the item (used by inline pickers).
    fn value(&self, _item: &Self::Item) -> Option<(String, String)> {
        None
    }

    /// Return `(source_path, source_page_id)` for batch promote.
    /// Only implemented by UnlinkedMentionsSource.
    fn batch_promote_data(&self, _item: &Self::Item) -> Option<(PathBuf, String)> {
        None
    }

    /// Return embedded preview lines for the given item, if available.
    fn preview_lines(&self, _item: &Self::Item) -> Option<Vec<String>> {
        None
    }
}

/// A composable filter chip that narrows picker results.
pub struct FilterPill<T> {
    pub kind: String,
    pub label: String,
    predicate: Arc<dyn Fn(&T) -> bool + Send + Sync + 'static>,
}

impl<T> FilterPill<T> {
    pub fn new(
        kind: impl Into<String>,
        label: impl Into<String>,
        predicate: impl Fn(&T) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind: kind.into(),
            label: label.into(),
            predicate: Arc::new(predicate),
        }
    }

    pub fn matches(&self, item: &T) -> bool {
        (self.predicate)(item)
    }
}

impl<T> Clone for FilterPill<T> {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind.clone(),
            label: self.label.clone(),
            predicate: Arc::clone(&self.predicate),
        }
    }
}

impl<T> std::fmt::Debug for FilterPill<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilterPill")
            .field("kind", &self.kind)
            .field("label", &self.label)
            .finish()
    }
}

/// A ranked picker row suitable for feeding into a UI frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerMatch {
    /// Index into the underlying source item list.
    pub source_index: usize,
    /// Fuzzy score from nucleo (higher is better).
    pub score: u32,
    /// Main display text.
    pub text: String,
    /// Optional right-aligned metadata.
    pub marginalia: String,
    /// Matched character indices in `text`.
    pub match_indices: Vec<usize>,
}

/// Generic picker core: query + fuzzy ranking + selection + composable filters.
pub struct Picker<T> {
    source: Box<dyn PickerSource<Item = T> + Send + Sync>,
    query: String,
    filters: Vec<FilterPill<T>>,
    results: Vec<PickerMatch>,
    selected: Option<usize>,
    matcher: Matcher,
}

impl<T> Picker<T> {
    pub fn new(source: impl PickerSource<Item = T> + Send + Sync + 'static) -> Self {
        let mut picker = Self {
            source: Box::new(source),
            query: String::new(),
            filters: Vec::new(),
            results: Vec::new(),
            selected: None,
            matcher: Matcher::new(Config::DEFAULT),
        };
        picker.refresh();
        picker
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.refresh();
    }

    pub fn add_filter(&mut self, filter: FilterPill<T>) {
        self.filters.push(filter);
        self.refresh();
    }

    pub fn remove_filter(&mut self, kind: &str, label: &str) -> bool {
        let before = self.filters.len();
        self.filters
            .retain(|pill| !(pill.kind == kind && pill.label == label));
        let removed = self.filters.len() != before;
        if removed {
            self.refresh();
        }
        removed
    }

    pub fn clear_filters(&mut self) {
        if self.filters.is_empty() {
            return;
        }
        self.filters.clear();
        self.refresh();
    }

    pub fn filters(&self) -> &[FilterPill<T>] {
        &self.filters
    }

    pub fn results(&self) -> &[PickerMatch] {
        &self.results
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    pub fn selected(&self) -> Option<&PickerMatch> {
        self.selected.and_then(|idx| self.results.get(idx))
    }

    pub fn selected_item(&self) -> Option<&T> {
        self.selected()
            .and_then(|result| self.source.items().get(result.source_index))
    }

    /// Return `(id, display_text)` if the source supports it.
    pub fn selected_value(&self) -> Option<(String, String)> {
        self.selected_item()
            .and_then(|item| self.source.value(item))
    }

    /// Return `(source_path, source_page_id)` for items at the given source indices.
    pub fn items_for_batch_promote(&self, source_indices: &HashSet<usize>) -> Vec<(PathBuf, String)> {
        let items = self.source.items();
        source_indices
            .iter()
            .filter_map(|&idx| {
                items.get(idx).and_then(|item| self.source.batch_promote_data(item))
            })
            .collect()
    }

    /// Return preview lines embedded in the selected item, if the source provides them.
    pub fn selected_preview_lines(&self) -> Option<Vec<String>> {
        self.selected_item()
            .and_then(|item| self.source.preview_lines(item))
    }

    pub fn move_down(&mut self) {
        let len = self.results.len();
        if len == 0 {
            self.selected = None;
            return;
        }
        self.selected = Some(match self.selected {
            Some(idx) => (idx + 1) % len,
            None => 0,
        });
    }

    pub fn move_up(&mut self) {
        let len = self.results.len();
        if len == 0 {
            self.selected = None;
            return;
        }
        self.selected = Some(match self.selected {
            Some(0) | None => len - 1,
            Some(idx) => idx - 1,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }

    pub fn refresh(&mut self) {
        let previously_selected_source = self
            .selected
            .and_then(|idx| self.results.get(idx))
            .map(|entry| entry.source_index);

        let pattern = Pattern::parse(self.query.trim(), CaseMatching::Smart, Normalization::Smart);

        let mut next_results = Vec::new();
        let mut haystack_buf = Vec::new();

        for (source_index, item) in self.source.items().iter().enumerate() {
            if !self.filters.iter().all(|pill| pill.matches(item)) {
                continue;
            }

            let text = self.source.text(item).into_owned();
            let marginalia = self.source.marginalia(item).into_owned();
            let mut raw_indices = Vec::new();
            let score = pattern.indices(
                Utf32Str::new(&text, &mut haystack_buf),
                &mut self.matcher,
                &mut raw_indices,
            );

            if let Some(score) = score {
                raw_indices.sort_unstable();
                raw_indices.dedup();
                next_results.push(PickerMatch {
                    source_index,
                    score,
                    text,
                    marginalia,
                    match_indices: raw_indices.into_iter().map(|idx| idx as usize).collect(),
                });
            }
        }

        next_results.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.source_index.cmp(&b.source_index))
        });

        self.results = next_results;
        self.selected = if self.results.is_empty() {
            None
        } else if let Some(source_idx) = previously_selected_source {
            self.results
                .iter()
                .position(|entry| entry.source_index == source_idx)
                .or(Some(0))
        } else {
            Some(0)
        };
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindPageItem {
    pub page_id: String,
    pub path: PathBuf,
    pub title: String,
    pub tags: Vec<String>,
    pub date_label: Option<String>,
}

impl From<IndexedPage> for FindPageItem {
    fn from(page: IndexedPage) -> Self {
        Self {
            page_id: page.page_id,
            path: page.path,
            title: page.title,
            tags: Vec::new(),
            date_label: None,
        }
    }
}

pub struct FindPagesSource {
    items: Vec<FindPageItem>,
}

impl FindPagesSource {
    pub fn empty() -> Self {
        Self { items: Vec::new() }
    }

    pub fn from_index(index: &SqliteIndex) -> Result<Self, IndexError> {
        let pages = index.list_pages()?;
        let mut items = Vec::with_capacity(pages.len());
        for page in pages {
            let date_label = date_label_for_path(&page.path);
            let tags = index.tags_for_path(&page.path.to_string_lossy())?;
            items.push(FindPageItem {
                page_id: page.page_id,
                path: page.path,
                title: page.title,
                tags,
                date_label,
            });
        }
        Ok(Self { items })
    }
}

impl PickerSource for FindPagesSource {
    type Item = FindPageItem;

    fn items(&self) -> &[Self::Item] {
        &self.items
    }

    fn text(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(item.title.clone())
    }

    fn marginalia(&self, item: &Self::Item) -> Cow<'_, str> {
        let tags = if item.tags.is_empty() {
            String::new()
        } else {
            item
                .tags
                .iter()
                .map(|t| format!("#{t}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let date = item.date_label.clone().unwrap_or_default();
        Cow::Owned(merge_marginalia(&tags, &date))
    }

    fn value(&self, item: &Self::Item) -> Option<(String, String)> {
        Some((item.page_id.clone(), item.title.clone()))
    }
}

// ---------------------------------------------------------------------------
// FullTextSearch picker source — line-level FTS results
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullTextSearchItem {
    pub page_id: String,
    pub path: PathBuf,
    pub page_title: String,
    pub snippet: String,
    /// ±5 context lines around the match with `❯` marker on the matching line.
    pub context_preview: Vec<String>,
}

pub struct FullTextSearchSource {
    items: Vec<FullTextSearchItem>,
}

impl FullTextSearchSource {
    pub fn empty() -> Self {
        Self { items: Vec::new() }
    }

    pub fn from_index(index: &SqliteIndex, query: &str) -> Result<Self, IndexError> {
        let hits = index.search(query)?;
        let items = hits
            .into_iter()
            .map(|hit| {
                let context_preview = index
                    .content_for_page_id(&hit.page_id)
                    .ok()
                    .flatten()
                    .map(|content| build_context_preview(&content, &hit.snippet, 5))
                    .unwrap_or_default();
                FullTextSearchItem {
                    page_id: hit.page_id,
                    path: hit.path,
                    page_title: hit.title,
                    snippet: hit.snippet,
                    context_preview,
                }
            })
            .collect();
        Ok(Self { items })
    }
}

impl PickerSource for FullTextSearchSource {
    type Item = FullTextSearchItem;

    fn items(&self) -> &[Self::Item] {
        &self.items
    }

    fn text(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(item.snippet.clone())
    }

    fn marginalia(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(item.page_title.clone())
    }

    fn value(&self, item: &Self::Item) -> Option<(String, String)> {
        Some((item.page_id.clone(), item.page_title.clone()))
    }

    fn preview_lines(&self, item: &Self::Item) -> Option<Vec<String>> {
        if item.context_preview.is_empty() {
            None
        } else {
            Some(item.context_preview.clone())
        }
    }
}

// ---------------------------------------------------------------------------
// DrillDown picker source — sections & blocks within a single page
// ---------------------------------------------------------------------------

/// A drilldown item representing either the whole page, a heading, or a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrillDownItem {
    /// Sub-ID to append after `#` (empty for whole-page embed).
    pub sub_id: String,
    /// Display text shown in the picker.
    pub display: String,
    /// Parent page ID.
    pub page_id: String,
    /// Parent page title.
    pub page_title: String,
}

pub struct DrillDownSource {
    items: Vec<DrillDownItem>,
}

impl DrillDownSource {
    /// Build a drill-down source by parsing the raw markdown content of a page.
    pub fn from_content(page_id: &str, page_title: &str, content: &str) -> Self {
        let mut items = Vec::new();
        // First item: embed whole page.
        items.push(DrillDownItem {
            sub_id: String::new(),
            display: format!("Embed whole page"),
            page_id: page_id.to_string(),
            page_title: page_title.to_string(),
        });

        let in_frontmatter = std::cell::Cell::new(false);
        let frontmatter_done = std::cell::Cell::new(false);

        for line in content.lines() {
            let trimmed = line.trim();
            // Skip YAML frontmatter.
            if !frontmatter_done.get() {
                if trimmed == "---" {
                    if in_frontmatter.get() {
                        frontmatter_done.set(true);
                    } else {
                        in_frontmatter.set(true);
                    }
                    continue;
                }
                if in_frontmatter.get() {
                    continue;
                }
            }

            // Detect headings: `# Heading`, `## Heading`, etc.
            if let Some(rest) = trimmed.strip_prefix('#') {
                // Count additional '#' chars to find the end of the marker.
                let hashes = 1 + rest.chars().take_while(|&c| c == '#').count();
                let heading_text = rest.trim_start_matches('#').trim();
                if !heading_text.is_empty() {
                    let slug = slugify(heading_text);
                    items.push(DrillDownItem {
                        sub_id: slug,
                        display: format!("§ {heading_text}"),
                        page_id: page_id.to_string(),
                        page_title: page_title.to_string(),
                    });
                }
                // A heading line might also have a block ID — fall through.
            }

            // Detect block IDs: `^block-id` at end of line.
            if let Some(pos) = trimmed.rfind('^') {
                let candidate = &trimmed[pos + 1..];
                if !candidate.is_empty()
                    && candidate
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    // Make sure the ^ is preceded by whitespace or is at start.
                    let before = &trimmed[..pos];
                    if before.is_empty() || before.ends_with(' ') {
                        let context: String = before.chars().take(40).collect();
                        let display_text = if context.trim().is_empty() {
                            format!("¶ ^{candidate}")
                        } else {
                            format!("¶ {context} ^{candidate}")
                        };
                        items.push(DrillDownItem {
                            sub_id: candidate.to_string(),
                            display: display_text,
                            page_id: page_id.to_string(),
                            page_title: page_title.to_string(),
                        });
                    }
                }
            }
        }
        Self { items }
    }
}

impl PickerSource for DrillDownSource {
    type Item = DrillDownItem;

    fn items(&self) -> &[Self::Item] {
        &self.items
    }

    fn text(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(item.display.clone())
    }

    fn value(&self, item: &Self::Item) -> Option<(String, String)> {
        Some((item.sub_id.clone(), item.display.clone()))
    }
}

/// Convert heading text into a URL-safe slug (lowercase, spaces → hyphens).
fn slugify(text: &str) -> String {
    text.chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                Some(c.to_ascii_lowercase())
            } else if c == ' ' || c == '-' || c == '_' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagPickerItem {
    pub tag: String,
    pub count: usize,
    pub preview_lines: Vec<String>,
}

impl From<TagCount> for TagPickerItem {
    fn from(tag: TagCount) -> Self {
        Self {
            tag: tag.tag,
            count: tag.count,
            preview_lines: Vec::new(),
        }
    }
}

pub struct SearchTagsSource {
    items: Vec<TagPickerItem>,
}

impl SearchTagsSource {
    pub fn from_index(index: &SqliteIndex) -> Result<Self, IndexError> {
        let mut preview_map: HashMap<String, Vec<String>> = HashMap::new();
        for page in index.list_pages()? {
            let path = page.path.to_string_lossy();
            let page_tags = index.tags_for_path(&path)?;
            if page_tags.is_empty() {
                continue;
            }
            let mut row = page.title.clone();
            if let Some(date) = date_label_for_path(&page.path) {
                row = format!("{row}  {date}");
            }
            for tag in page_tags {
                let entry = preview_map.entry(tag).or_default();
                if entry.len() < 20 {
                    entry.push(row.clone());
                }
            }
        }

        let mut items: Vec<_> = index
            .list_tags()?
            .into_iter()
            .map(|tag| {
                let mut item = TagPickerItem::from(tag);
                item.preview_lines = preview_map.remove(&item.tag).unwrap_or_default();
                item
            })
            .collect();
        items.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
        Ok(Self { items })
    }
}

impl PickerSource for SearchTagsSource {
    type Item = TagPickerItem;

    fn items(&self) -> &[Self::Item] {
        &self.items
    }

    fn text(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(format!("#{}", item.tag))
    }

    fn marginalia(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(format_note_count(item.count))
    }

    fn value(&self, item: &Self::Item) -> Option<(String, String)> {
        Some((item.tag.clone(), item.tag.clone()))
    }

    fn preview_lines(&self, item: &Self::Item) -> Option<Vec<String>> {
        if item.preview_lines.is_empty() {
            None
        } else {
            Some(item.preview_lines.clone())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntryPickerItem {
    pub page_id: String,
    pub path: PathBuf,
    pub title: String,
    pub date: NaiveDate,
    pub item_count: usize,
    pub tags: Vec<String>,
}

pub fn journal_date_from_path(path: &Path) -> Option<NaiveDate> {
    if !path
        .components()
        .any(|component| component.as_os_str().to_str() == Some("journal"))
    {
        return None;
    }

    let stem = path.file_stem()?.to_str()?;
    NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

pub struct SearchJournalSource {
    items: Vec<JournalEntryPickerItem>,
}

impl SearchJournalSource {
    pub fn from_index(index: &SqliteIndex) -> Result<Self, IndexError> {
        let mut items = Vec::new();
        for page in index.list_pages()? {
            let Some(date) = journal_date_from_path(&page.path) else {
                continue;
            };
            let path = page.path.to_string_lossy();
            let tags = index.tags_for_path(&path)?;
            let item_count = index
                .page_content_for_path(&page.path)?
                .map(|content| count_journal_items(&content.content))
                .unwrap_or(0);

            items.push(JournalEntryPickerItem {
                page_id: page.page_id,
                path: page.path,
                title: page.title,
                date,
                item_count,
                tags,
            });
        }

        items.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.path.cmp(&b.path)));
        Ok(Self { items })
    }
}

impl PickerSource for SearchJournalSource {
    type Item = JournalEntryPickerItem;

    fn items(&self) -> &[Self::Item] {
        &self.items
    }

    fn text(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(item.date.format("%Y-%m-%d").to_string())
    }

    fn marginalia(&self, item: &Self::Item) -> Cow<'_, str> {
        let item_label = if item.item_count == 1 {
            "1 item".to_string()
        } else {
            format!("{} items", item.item_count)
        };
        let tags = item
            .tags
            .iter()
            .take(3)
            .map(|tag| format!("#{tag}"))
            .collect::<Vec<_>>()
            .join(" ");
        Cow::Owned(merge_marginalia(&item_label, &tags))
    }

    fn value(&self, item: &Self::Item) -> Option<(String, String)> {
        Some((item.page_id.clone(), item.title.clone()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklinkPickerItem {
    pub source_page_id: String,
    pub source_path: PathBuf,
    pub source_title: String,
    pub context_snippet: String,
    /// Context preview: source page lines with linking line marked `❯`.
    pub context_preview: Vec<String>,
}

pub struct BacklinksSource {
    items: Vec<BacklinkPickerItem>,
}

impl BacklinksSource {
    pub fn for_page_id(resolver: &Resolver<'_>, page_id: &str) -> Result<Self, IndexError> {
        let mut items = Vec::new();
        let link_needle = format!("[[{page_id}");
        for backlink in resolver.backlinks_for_page_id(page_id)? {
            let source_title = resolver
                .resolve_page_id(&backlink.source_page_id)?
                .map(|page| page.title)
                .unwrap_or_else(|| backlink.source_page_id.clone());

            let (context_snippet, context_preview) = resolver
                .page_content_for_path(&backlink.source_path)?
                .map(|pc| {
                    let snippet = extract_link_context(&pc.content, page_id);
                    let preview = build_context_preview(&pc.content, &link_needle, 5);
                    (snippet, preview)
                })
                .unwrap_or_default();

            items.push(BacklinkPickerItem {
                source_page_id: backlink.source_page_id,
                source_path: backlink.source_path,
                source_title,
                context_snippet,
                context_preview,
            });
        }
        Ok(Self { items })
    }
}

impl PickerSource for BacklinksSource {
    type Item = BacklinkPickerItem;

    fn items(&self) -> &[Self::Item] {
        &self.items
    }

    fn text(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(item.source_title.clone())
    }

    fn marginalia(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(item.context_snippet.clone())
    }

    fn value(&self, item: &Self::Item) -> Option<(String, String)> {
        Some((item.source_page_id.clone(), item.source_title.clone()))
    }

    fn preview_lines(&self, item: &Self::Item) -> Option<Vec<String>> {
        if item.context_preview.is_empty() {
            None
        } else {
            Some(item.context_preview.clone())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlinkedMentionPickerItem {
    pub source_page_id: String,
    pub source_path: PathBuf,
    pub source_title: String,
    pub snippet: String,
    /// Context preview: source page lines with mention line marked `❯`.
    pub context_preview: Vec<String>,
}

pub struct UnlinkedMentionsSource {
    items: Vec<UnlinkedMentionPickerItem>,
}

impl UnlinkedMentionsSource {
    pub fn for_page_title(resolver: &Resolver<'_>, page_title: &str) -> Result<Self, IndexError> {
        let mentions = resolver.unlinked_mentions_for_title(page_title)?;
        let items = mentions
            .into_iter()
            .map(|mention| {
                let context_preview = resolver
                    .page_content_for_path(&mention.source_path)
                    .ok()
                    .flatten()
                    .map(|pc| {
                        build_context_preview(&pc.content, page_title, 5)
                    })
                    .unwrap_or_default();
                UnlinkedMentionPickerItem {
                    source_page_id: mention.source_page_id,
                    source_path: mention.source_path,
                    source_title: mention.source_title,
                    snippet: mention.snippet,
                    context_preview,
                }
            })
            .collect();
        Ok(Self { items })
    }
}

impl PickerSource for UnlinkedMentionsSource {
    type Item = UnlinkedMentionPickerItem;

    fn items(&self) -> &[Self::Item] {
        &self.items
    }

    fn text(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(item.source_title.clone())
    }

    fn marginalia(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(item.snippet.clone())
    }

    fn batch_promote_data(&self, item: &Self::Item) -> Option<(PathBuf, String)> {
        Some((item.source_path.clone(), item.source_page_id.clone()))
    }

    fn preview_lines(&self, item: &Self::Item) -> Option<Vec<String>> {
        if item.context_preview.is_empty() {
            None
        } else {
            Some(item.context_preview.clone())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPickerItem {
    pub command_id: String,
    pub name: String,
    pub keybinding: Option<String>,
    pub category: String,
    pub description: String,
}

pub struct CommandListSource {
    items: Vec<CommandPickerItem>,
}

impl CommandListSource {
    pub fn new(items: Vec<CommandPickerItem>) -> Self {
        Self { items }
    }
}

impl PickerSource for CommandListSource {
    type Item = CommandPickerItem;

    fn items(&self) -> &[Self::Item] {
        &self.items
    }

    fn text(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(item.name.clone())
    }

    fn marginalia(&self, item: &Self::Item) -> Cow<'_, str> {
        if let Some(keybinding) = item
            .keybinding
            .as_deref()
            .filter(|binding| !binding.is_empty())
        {
            Cow::Owned(format!("{keybinding:<15}{}", item.category))
        } else {
            Cow::Owned(item.category.clone())
        }
    }

    fn value(&self, item: &Self::Item) -> Option<(String, String)> {
        Some((item.command_id.clone(), item.name.clone()))
    }

    fn preview_lines(&self, item: &Self::Item) -> Option<Vec<String>> {
        if item.description.is_empty() {
            None
        } else {
            Some(item.description.lines().map(|line| line.to_string()).collect())
        }
    }
}

// ---------------------------------------------------------------------------
// BufferPicker source — open buffers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferPickerItem {
    pub title: String,
    pub path: PathBuf,
    pub dirty: bool,
    pub active: bool,
    pub is_journal: bool,
    /// First N lines of the buffer content for preview.
    pub preview_lines: Vec<String>,
}

pub struct BufferPickerSource {
    items: Vec<BufferPickerItem>,
}

impl BufferPickerSource {
    pub fn new(items: Vec<BufferPickerItem>) -> Self {
        Self { items }
    }
}

impl PickerSource for BufferPickerSource {
    type Item = BufferPickerItem;

    fn items(&self) -> &[Self::Item] {
        &self.items
    }

    fn text(&self, item: &Self::Item) -> Cow<'_, str> {
        if item.is_journal {
            Cow::Owned(format!("{} (journal)", item.title))
        } else {
            Cow::Owned(item.title.clone())
        }
    }

    fn marginalia(&self, item: &Self::Item) -> Cow<'_, str> {
        let mut parts = Vec::new();
        if item.dirty {
            parts.push("[+]");
        }
        if item.active {
            parts.push("active");
        }
        Cow::Owned(parts.join("  "))
    }

    fn value(&self, item: &Self::Item) -> Option<(String, String)> {
        Some((item.path.to_string_lossy().into_owned(), item.title.clone()))
    }

    fn preview_lines(&self, item: &Self::Item) -> Option<Vec<String>> {
        if item.preview_lines.is_empty() {
            None
        } else {
            Some(item.preview_lines.clone())
        }
    }
}

// ---------------------------------------------------------------------------
// TemplatePicker source — vault templates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplatePickerItem {
    pub name: String,
    pub path: PathBuf,
}

pub struct TemplatePickerSource {
    items: Vec<TemplatePickerItem>,
}

impl TemplatePickerSource {
    pub fn from_vault(vault_root: &Path) -> Self {
        let templates = crate::template::load_templates(vault_root);
        let dir = vault_root.join("templates");
        let items = templates
            .into_iter()
            .map(|t| TemplatePickerItem {
                path: dir.join(format!("{}.tmpl", t.name)),
                name: t.name,
            })
            .collect();
        Self { items }
    }
}

impl PickerSource for TemplatePickerSource {
    type Item = TemplatePickerItem;

    fn items(&self) -> &[Self::Item] {
        &self.items
    }

    fn text(&self, item: &Self::Item) -> Cow<'_, str> {
        Cow::Owned(item.name.clone())
    }

    fn value(&self, item: &Self::Item) -> Option<(String, String)> {
        Some((item.path.to_string_lossy().into_owned(), item.name.clone()))
    }
}

fn format_note_count(count: usize) -> String {
    if count == 1 {
        "1 note".to_string()
    } else {
        format!("{count} notes")
    }
}

fn count_journal_items(content: &str) -> usize {
    content
        .lines()
        .filter(|line| line.trim_start().starts_with("- "))
        .count()
}

fn date_label_for_path(path: &Path) -> Option<String> {
    if let Some(date) = journal_date_from_path(path) {
        return Some(date.format("%b %d").to_string());
    }
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let dt: DateTime<Local> = modified.into();
    Some(dt.format("%b %d").to_string())
}

fn merge_marginalia(left: &str, right: &str) -> String {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => String::new(),
        (false, true) => left.to_string(),
        (true, false) => right.to_string(),
        (false, false) => format!("{left}  {right}"),
    }
}

fn extract_link_context(content: &str, target_page_id: &str) -> String {
    let needle = format!("[[{target_page_id}");
    if let Some(pos) = content.find(&needle) {
        let line_start = content[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_end = content[pos..]
            .find('\n')
            .map(|i| pos + i)
            .unwrap_or(content.len());
        let line = content[line_start..line_end].trim();
        let char_count = line.chars().count();
        if char_count > 30 {
            let truncated: String = line.chars().take(30).collect();
            format!("\"{truncated}…\"")
        } else {
            format!("\"{line}\"")
        }
    } else {
        String::new()
    }
}

/// Build a ±`radius` context preview around the first line containing `needle`.
/// The matching line is prefixed with `❯ `, other lines with `  `.
fn build_context_preview(content: &str, needle: &str, radius: usize) -> Vec<String> {
    let needle_lower = needle.to_lowercase();
    // Strip FTS5 highlight markers ([ and ]) from needle for plain matching.
    let plain_needle: String = needle_lower
        .chars()
        .filter(|c| *c != '[' && *c != ']')
        .collect();
    let lines: Vec<&str> = content.lines().collect();
    let match_idx = lines
        .iter()
        .position(|line| line.to_lowercase().contains(&plain_needle));
    let Some(idx) = match_idx else {
        return Vec::new();
    };
    let start = idx.saturating_sub(radius);
    let end = (idx + radius + 1).min(lines.len());
    lines[start..end]
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let actual_idx = start + i;
            if actual_idx == idx {
                format!("❯ {line}")
            } else {
                format!("  {line}")
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// DynPicker — type-erased wrapper so EditorState can hold any Picker<T>
// ---------------------------------------------------------------------------

/// Type-erased picker interface for storage in EditorState.
pub trait DynPicker: Send + Sync {
    fn query(&self) -> &str;
    fn set_query(&mut self, query: String);
    fn move_up(&mut self);
    fn move_down(&mut self);
    fn selected_index(&self) -> Option<usize>;
    fn results(&self) -> Vec<PickerMatch>;
    fn is_empty(&self) -> bool;
    /// Return `(id, display_text)` for the selected item, if the source supports it.
    fn selected_value(&self) -> Option<(String, String)> {
        None
    }
    /// Return `(source_path, source_page_id)` for items at the given source indices.
    /// Used by batch promote for unlinked mentions.
    fn items_for_batch_promote(&self, _source_indices: &HashSet<usize>) -> Vec<(PathBuf, String)> {
        Vec::new()
    }
    /// Return preview lines for the currently selected item, if the source embeds them.
    fn selected_preview_lines(&self) -> Option<Vec<String>> {
        None
    }
}

impl<T: 'static> DynPicker for Picker<T>
where
    T: Send + Sync,
{
    fn query(&self) -> &str {
        Picker::query(self)
    }
    fn set_query(&mut self, query: String) {
        Picker::set_query(self, query);
    }
    fn move_up(&mut self) {
        Picker::move_up(self);
    }
    fn move_down(&mut self) {
        Picker::move_down(self);
    }
    fn selected_index(&self) -> Option<usize> {
        Picker::selected_index(self)
    }
    fn results(&self) -> Vec<PickerMatch> {
        Picker::results(self).to_vec()
    }
    fn is_empty(&self) -> bool {
        Picker::is_empty(self)
    }
    fn selected_value(&self) -> Option<(String, String)> {
        Picker::selected_value(self)
    }
    fn items_for_batch_promote(&self, source_indices: &HashSet<usize>) -> Vec<(PathBuf, String)> {
        Picker::items_for_batch_promote(self, source_indices)
    }
    fn selected_preview_lines(&self) -> Option<Vec<String>> {
        Picker::selected_preview_lines(self)
    }
}

/// Default action menu items for pickers.
pub const DEFAULT_ACTION_MENU_ITEMS: &[&str] = &[
    "Open",
    "Open in split",
    "Copy link",
    "Copy page ID",
];

/// Action menu items for Find Page picker.
pub const FIND_PAGE_ACTION_MENU_ITEMS: &[&str] = &[
    "Open",
    "Open in split",
    "Rename",
    "Delete",
    "Copy link",
    "Copy page ID",
];

/// Action menu items for the Switch Buffer picker.
pub const BUFFER_ACTION_MENU_ITEMS: &[&str] = &[
    "Open",
    "Open in split",
    "Close buffer",
    "Save",
    "Diff unsaved changes",
];

/// Action menu items for the Full-Text Search picker.
pub const FTS_ACTION_MENU_ITEMS: &[&str] = &[
    "Open at line",
    "Open in split",
    "Copy block link",
    "Embed block",
];

/// Action menu items for the Search Tags picker.
pub const TAGS_ACTION_MENU_ITEMS: &[&str] = &[
    "Open",
    "Rename tag",
    "Delete tag",
];

/// Action menu items for the Backlinks picker.
pub const BACKLINKS_ACTION_MENU_ITEMS: &[&str] = &[
    "Open",
    "Open in split",
    "Copy block link",
    "Copy page ID",
];

/// Action menu items for the Unlinked Mentions picker.
pub const UNLINKED_ACTION_MENU_ITEMS: &[&str] = &[
    "Promote to link",
    "Open",
    "Open in split",
    "Ignore",
];

/// Action menu items for the Search Journal picker.
pub const JOURNAL_ACTION_MENU_ITEMS: &[&str] = &[
    "Open",
    "Open in split",
];

/// Distinguishes picker behaviour on Enter (e.g. two-step tag drill-down).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PickerKind {
    #[default]
    Default,
    /// First step of the two-step tag flow: Enter transitions to a
    /// filtered Find Page picker instead of closing.
    SearchTags,
}

/// An active picker stored in EditorState.
pub struct ActivePicker {
    /// Determines Enter behaviour (close vs. drill-down).
    pub kind: PickerKind,
    pub title: String,
    pub inner: Box<dyn DynPicker>,
    /// True when this picker was triggered inline (e.g. `[[` in insert mode).
    pub inline: bool,
    /// Number of trigger characters to replace on selection (e.g. 2 for `[[`, 3 for `![[`).
    pub inline_trigger_len: usize,
    /// True when the inline trigger was `![[` (embed) rather than `[[` (link).
    pub is_embed: bool,
    /// Stacked filter pills `(kind, label)` that narrow results.
    pub filter_pills: Vec<(String, String)>,
    /// The text the user has typed (separate from filter pills).
    pub typed_query: String,
    /// Preview content for the selected item.
    pub preview: Option<Vec<crate::render::RenderedLine>>,
    /// Marked item indices (source_index from PickerMatch) for batch selection.
    pub marked: HashSet<usize>,
    /// True when this picker supports batch selection (e.g. Unlinked Mentions).
    pub supports_batch_select: bool,
    /// Whether the action menu popup is open.
    pub action_menu_open: bool,
    /// Currently selected action menu index.
    pub action_menu_selected: usize,
    /// Available action menu items.
    pub action_menu_items: Vec<String>,
    /// When in drill-down mode, the page ID we're drilling into.
    pub drill_down_page_id: Option<String>,
    /// When in drill-down mode, the page title we're drilling into.
    pub drill_down_page_title: Option<String>,
}

impl ActivePicker {
    /// Build the effective query by combining pill labels with typed text.
    fn effective_query(&self) -> String {
        let mut parts: Vec<&str> = self
            .filter_pills
            .iter()
            .map(|(_, label)| label.as_str())
            .collect();
        if !self.typed_query.is_empty() {
            parts.push(&self.typed_query);
        }
        parts.join(" ")
    }

    /// Sync the inner picker query from pills + typed text.
    pub fn sync_query(&mut self) {
        let eq = self.effective_query();
        self.inner.set_query(eq);
    }

    /// Append a character to the typed query and refresh.
    pub fn push_char(&mut self, c: char) {
        self.typed_query.push(c);
        self.sync_query();
    }

    /// Remove the last typed character, or pop the last filter pill if the
    /// typed query is empty (restoring its label as the typed query).
    pub fn pop_char(&mut self) {
        if self.typed_query.pop().is_some() {
            self.sync_query();
        } else if let Some((_kind, label)) = self.filter_pills.pop() {
            self.typed_query = label;
            self.sync_query();
        }
    }

    /// Move the current typed query into a filter pill of the given kind.
    /// Returns `true` if a pill was created.
    pub fn extract_filter(&mut self, kind: &str) -> bool {
        let text = self.typed_query.trim().to_string();
        if text.is_empty() {
            return false;
        }
        self.filter_pills.push((kind.to_string(), text));
        self.typed_query.clear();
        self.sync_query();
        true
    }
}

impl std::fmt::Debug for ActivePicker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivePicker")
            .field("title", &self.title)
            .field("filter_pills", &self.filter_pills)
            .field("marked", &self.marked)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;
    use crate::document::Document;
    use crate::parser::parse;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestItem {
        title: &'static str,
        kind: &'static str,
        starred: bool,
    }

    struct TestSource {
        items: Vec<TestItem>,
    }

    impl TestSource {
        fn sample() -> Self {
            Self {
                items: vec![
                    TestItem {
                        title: "abc",
                        kind: "page",
                        starred: true,
                    },
                    TestItem {
                        title: "axbyc",
                        kind: "page",
                        starred: false,
                    },
                    TestItem {
                        title: "buffer",
                        kind: "buffer",
                        starred: true,
                    },
                ],
            }
        }
    }

    impl PickerSource for TestSource {
        type Item = TestItem;

        fn items(&self) -> &[Self::Item] {
            &self.items
        }

        fn text(&self, item: &Self::Item) -> Cow<'_, str> {
            Cow::Borrowed(item.title)
        }

        fn marginalia(&self, item: &Self::Item) -> Cow<'_, str> {
            Cow::Borrowed(item.kind)
        }
    }

    fn make_index() -> (TempDir, SqliteIndex) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let index = SqliteIndex::open(&db_path).unwrap();
        (tmp, index)
    }

    fn make_doc(id: &str, title: &str, front_tags: &[&str], body: &str) -> Document {
        let tags = if front_tags.is_empty() {
            String::from("[]")
        } else {
            let joined = front_tags
                .iter()
                .map(|tag| format!("\"{tag}\""))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{joined}]")
        };

        let raw = format!("---\nid: {id}\ntitle: \"{title}\"\ntags: {tags}\n---\n\n{body}\n");
        parse(&raw).unwrap()
    }

    #[test]
    fn ranking_prefers_tighter_fuzzy_matches() {
        let mut picker = Picker::new(TestSource::sample());
        picker.set_query("abc");

        assert_eq!(picker.query(), "abc");
        assert_eq!(picker.results().len(), 2);
        assert_eq!(picker.results()[0].text, "abc");
        assert!(picker.results()[0].score >= picker.results()[1].score);
        assert_eq!(picker.results()[0].match_indices, vec![0, 1, 2]);
    }

    #[test]
    fn selection_moves_up_and_down_with_wrap() {
        let mut picker = Picker::new(TestSource::sample());

        assert_eq!(picker.selected_index(), Some(0));
        assert_eq!(picker.selected_item().unwrap().title, "abc");

        picker.move_down();
        assert_eq!(picker.selected_item().unwrap().title, "axbyc");
        picker.move_down();
        assert_eq!(picker.selected_item().unwrap().title, "buffer");
        picker.move_down();
        assert_eq!(picker.selected_item().unwrap().title, "abc");

        picker.move_up();
        assert_eq!(picker.selected_item().unwrap().title, "buffer");
    }

    #[test]
    fn filter_pills_compose() {
        let mut picker = Picker::new(TestSource::sample());

        picker.add_filter(FilterPill::new("kind", "page", |item: &TestItem| {
            item.kind == "page"
        }));
        assert_eq!(picker.results().len(), 2);

        picker.add_filter(FilterPill::new("starred", "yes", |item: &TestItem| {
            item.starred
        }));
        assert_eq!(picker.results().len(), 1);
        assert_eq!(picker.results()[0].text, "abc");
        assert_eq!(picker.filters().len(), 2);

        assert!(picker.remove_filter("kind", "page"));
        assert_eq!(picker.results().len(), 2);
    }

    #[test]
    fn empty_state_when_query_or_filters_exclude_everything() {
        let mut picker = Picker::new(TestSource::sample());

        picker.set_query("zzzz");
        assert!(picker.is_empty());
        assert_eq!(picker.selected_index(), None);

        picker.move_down();
        picker.move_up();
        assert_eq!(picker.selected_index(), None);

        picker.set_query("");
        picker.clear_filters();
        assert!(!picker.is_empty());
        assert_eq!(picker.selected_index(), Some(0));
    }

    #[test]
    fn find_pages_source_shape_and_query_filtering() {
        let (_tmp, mut index) = make_index();
        let doc_a = make_doc("pagea001", "Text Editor Theory", &[], "Body A.");
        let doc_b = make_doc("pageb001", "Rust Programming", &[], "Body B.");

        index
            .index_document(Path::new("pages/theory.md"), &doc_a)
            .unwrap();
        index
            .index_document(Path::new("pages/rust.md"), &doc_b)
            .unwrap();

        let source = FindPagesSource::from_index(&index).unwrap();
        assert_eq!(source.items().len(), 2);
        assert_eq!(source.items()[0].title, "Rust Programming");
        assert_eq!(
            source.marginalia(&source.items()[0]).as_ref(),
            ""
        );

        let mut picker = Picker::new(source);
        picker.set_query("edt");
        assert_eq!(picker.results().len(), 1);
        assert_eq!(picker.results()[0].text, "Text Editor Theory");
    }

    #[test]
    fn find_pages_marginalia_includes_tags_and_date_when_available() {
        let (_tmp, mut index) = make_index();
        let doc = make_doc("jrnl0001", "2026-03-02", &["rust"], "- item");
        index
            .index_document(Path::new("journal/2026-03-02.md"), &doc)
            .unwrap();
        let source = FindPagesSource::from_index(&index).unwrap();
        let item = source.items().iter().find(|item| item.page_id == "jrnl0001").unwrap();
        let marginalia = source.marginalia(item);
        assert!(marginalia.contains("#rust"));
        assert!(marginalia.contains("Mar 02"));
    }

    #[test]
    fn search_tags_source_shape_and_query_filtering() {
        let (_tmp, mut index) = make_index();
        let doc_a = make_doc("taga0001", "Tagged A", &["rust", "docs"], "body one");
        let doc_b = make_doc("tagb0001", "Tagged B", &["rust"], "body two");

        index
            .index_document(Path::new("pages/tag-a.md"), &doc_a)
            .unwrap();
        index
            .index_document(Path::new("pages/tag-b.md"), &doc_b)
            .unwrap();

        let source = SearchTagsSource::from_index(&index).unwrap();
        assert_eq!(source.items().len(), 2);
        assert_eq!(source.items()[0].tag, "rust");
        assert_eq!(source.items()[0].count, 2);
        assert_eq!(source.text(&source.items()[0]).as_ref(), "#rust");
        assert_eq!(source.marginalia(&source.items()[0]).as_ref(), "2 notes");
        let preview = source.preview_lines(&source.items()[0]).unwrap();
        assert!(preview.iter().any(|line| line.contains("Tagged")));

        let mut picker = Picker::new(source);
        picker.set_query("doc");
        assert_eq!(picker.results().len(), 1);
        assert_eq!(picker.results()[0].text, "#docs");
    }

    #[test]
    fn search_journal_source_shape_and_query_filtering() {
        let (_tmp, mut index) = make_index();
        let doc_a = make_doc("jrnl0001", "Journal A", &[], "entry a");
        let doc_b = make_doc(
            "jrnl0002",
            "Journal B",
            &["rust"],
            "- note one\n- [ ] task two #planning",
        );
        let page_like_date = make_doc("page0001", "Not Journal", &[], "entry c");

        index
            .index_document(Path::new("journal/2026-03-01.md"), &doc_a)
            .unwrap();
        index
            .index_document(Path::new("journal/2026-03-02.md"), &doc_b)
            .unwrap();
        index
            .index_document(Path::new("pages/2026-03-01.md"), &page_like_date)
            .unwrap();

        let source = SearchJournalSource::from_index(&index).unwrap();
        assert_eq!(source.items().len(), 2);
        assert_eq!(
            source.items()[0].date,
            NaiveDate::from_ymd_opt(2026, 3, 2).unwrap()
        );
        assert_eq!(source.text(&source.items()[0]).as_ref(), "2026-03-02");
        let marginalia = source.marginalia(&source.items()[0]).into_owned();
        assert!(marginalia.contains("2 items"));
        assert!(marginalia.contains("#rust"));
        assert!(journal_date_from_path(Path::new("pages/2026-03-01.md")).is_none());

        let mut picker = Picker::new(source);
        picker.set_query("2026-03-01");
        assert_eq!(picker.results().len(), 1);
        assert_eq!(picker.results()[0].text, "2026-03-01");
    }

    #[test]
    fn backlinks_source_shape_and_query_filtering() {
        let (_tmp, mut index) = make_index();
        let target = make_doc("target01", "Target", &[], "target body");
        let source_a = make_doc("srca0001", "Source Alpha", &[], "See [[target01|Target]].");
        let source_b = make_doc(
            "srcb0001",
            "Source Beta",
            &[],
            "Embed ![[target01|Target]].",
        );

        index
            .index_document(Path::new("pages/target.md"), &target)
            .unwrap();
        index
            .index_document(Path::new("pages/source-a.md"), &source_a)
            .unwrap();
        index
            .index_document(Path::new("pages/source-b.md"), &source_b)
            .unwrap();

        let resolver = Resolver::new(&index);
        let source = BacklinksSource::for_page_id(&resolver, "target01").unwrap();
        assert_eq!(source.items().len(), 2);
        assert_eq!(source.items()[0].source_title, "Source Alpha");
        assert_eq!(
            source.marginalia(&source.items()[0]).as_ref(),
            "\"See [[target01|Target]].\""
        );

        let mut picker = Picker::new(source);
        picker.set_query("beta");
        assert_eq!(picker.results().len(), 1);
        assert_eq!(picker.results()[0].text, "Source Beta");
    }

    #[test]
    fn unlinked_mentions_source_shape_and_query_filtering() {
        let (_tmp, mut index) = make_index();
        let target = make_doc("target01", "Rust Notes", &[], "target body");
        let linked = make_doc(
            "linked01",
            "Already Linked",
            &[],
            "See [[target01|Rust Notes]] for details.",
        );
        let mention_a = make_doc("mention01", "Mention One", &[], "I reviewed rust notes.");
        let mention_b = make_doc("mention02", "Mention Two", &[], "RUST NOTES are useful.");

        index
            .index_document(Path::new("pages/target.md"), &target)
            .unwrap();
        index
            .index_document(Path::new("pages/linked.md"), &linked)
            .unwrap();
        index
            .index_document(Path::new("pages/mention-a.md"), &mention_a)
            .unwrap();
        index
            .index_document(Path::new("pages/mention-b.md"), &mention_b)
            .unwrap();

        let resolver = Resolver::new(&index);
        let source = UnlinkedMentionsSource::for_page_title(&resolver, "Rust Notes").unwrap();
        assert_eq!(source.items().len(), 2);
        assert!(
            !source
                .items()
                .iter()
                .any(|item| item.source_page_id == "linked01")
        );
        assert!(
            source
                .marginalia(&source.items()[0])
                .as_ref()
                .to_lowercase()
                .contains("rust")
        );

        let mut picker = Picker::new(source);
        picker.set_query("two");
        assert_eq!(picker.results().len(), 1);
        assert_eq!(picker.results()[0].text, "Mention Two");
    }

    #[test]
    fn command_list_source_shape_and_filtering() {
        let source = CommandListSource::new(vec![
            CommandPickerItem {
                command_id: "window.split.vertical".to_string(),
                name: "Window: Vertical Split".to_string(),
                keybinding: Some("SPC w v".to_string()),
                category: "window".to_string(),
                description: "Split the current window vertically.".to_string(),
            },
            CommandPickerItem {
                command_id: "window.split.horizontal".to_string(),
                name: "Window: Horizontal Split".to_string(),
                keybinding: Some("SPC w s".to_string()),
                category: "window".to_string(),
                description: "Split the current window horizontally.".to_string(),
            },
            CommandPickerItem {
                command_id: "refactor.split".to_string(),
                name: "Refactor: Split Page".to_string(),
                keybinding: Some("SPC r s".to_string()),
                category: "refactor".to_string(),
                description: "Split one page into multiple pages.".to_string(),
            },
        ]);

        assert_eq!(source.items().len(), 3);
        assert_eq!(
            source.marginalia(&source.items()[0]).as_ref(),
            "SPC w v        window"
        );

        let mut picker = Picker::new(source);
        picker.add_filter(FilterPill::new(
            "category",
            "window",
            |item: &CommandPickerItem| item.category == "window",
        ));
        assert_eq!(picker.results().len(), 2);

        picker.set_query("horizontal");
        assert_eq!(picker.results().len(), 1);
        assert_eq!(picker.results()[0].text, "Window: Horizontal Split");
    }
}
