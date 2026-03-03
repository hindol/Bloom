use crate::index::Index;
use crate::types::{PageId, PageMeta};
use chrono::NaiveDate;

pub struct Timeline {}

pub struct TimelineEntry {
    pub source_page: PageMeta,
    pub date: NaiveDate,
    pub context: String,
    pub link_line: usize,
}

pub struct TimelineView {
    pub target_page: PageMeta,
    pub entries: Vec<TimelineEntry>,
}

impl Timeline {
    pub fn new() -> Self {
        Timeline {}
    }

    /// Build a timeline for a page using backlinks from the index, sorted chronologically.
    pub fn build(&self, page: &PageId, index: &Index) -> TimelineView {
        let target_page = index.find_page_by_id(page).unwrap_or_else(|| PageMeta {
            id: page.clone(),
            title: String::new(),
            created: NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
            tags: Vec::new(),
            path: std::path::PathBuf::new(),
        });

        let backlinks = index.backlinks_to(page);
        let mut entries: Vec<TimelineEntry> = backlinks
            .into_iter()
            .map(|bl| TimelineEntry {
                date: bl.source_page.created,
                context: bl.context,
                link_line: bl.line,
                source_page: bl.source_page,
            })
            .collect();

        // Sort chronologically by date
        entries.sort_by_key(|e| e.date);

        TimelineView {
            target_page,
            entries,
        }
    }
}