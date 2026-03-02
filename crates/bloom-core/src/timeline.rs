use std::cmp::Ordering;
use std::path::PathBuf;

use chrono::{NaiveDate, NaiveTime};

use crate::index::{IndexError, SqliteIndex};

use super::Resolver;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    pub source_page_id: String,
    pub source_path: PathBuf,
    pub source_title: String,
    pub snippet: String,
    pub date: Option<NaiveDate>,
    pub time: Option<NaiveTime>,
}

pub struct TimelineService<'a> {
    resolver: Resolver<'a>,
    index: &'a SqliteIndex,
}

impl<'a> TimelineService<'a> {
    pub fn new(index: &'a SqliteIndex) -> Self {
        Self {
            resolver: Resolver::new(index),
            index,
        }
    }

    pub fn entries_for_page_id(
        &self,
        target_page_id: &str,
    ) -> Result<Vec<TimelineEntry>, IndexError> {
        let target_page_id = target_page_id.trim();
        if target_page_id.is_empty() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        for backlink in self.resolver.backlinks_for_page_id(target_page_id)? {
            let Some(source) = self.index.page_content_for_path(&backlink.source_path)? else {
                continue;
            };

            let (date, time) = pick_timestamp(&source.content)
                .map(|(date, time)| (Some(date), time))
                .unwrap_or((None, None));

            entries.push(TimelineEntry {
                source_page_id: backlink.source_page_id,
                source_path: backlink.source_path,
                source_title: source.title,
                snippet: link_snippet(&source.content, target_page_id),
                date,
                time,
            });
        }

        // If a source note has no parsed timestamps, keep ordering deterministic via path/page-id.
        entries.sort_by(compare_entries);
        Ok(entries)
    }
}

fn compare_entries(a: &TimelineEntry, b: &TimelineEntry) -> Ordering {
    match (a.date, b.date) {
        (Some(a_date), Some(b_date)) => a_date
            .cmp(&b_date)
            .then_with(|| time_or_midnight(a.time).cmp(&time_or_midnight(b.time)))
            .then_with(|| a.source_path.cmp(&b.source_path))
            .then_with(|| a.source_page_id.cmp(&b.source_page_id)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a
            .source_path
            .cmp(&b.source_path)
            .then_with(|| a.source_page_id.cmp(&b.source_page_id)),
    }
}

fn time_or_midnight(time: Option<NaiveTime>) -> NaiveTime {
    time.unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).expect("midnight is valid"))
}

fn link_snippet(content: &str, target_page_id: &str) -> String {
    let direct = format!("[[{target_page_id}");
    let embed = format!("![[{target_page_id}");
    let mut fallback: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if fallback.is_none() {
            fallback = Some(trimmed.to_string());
        }

        if trimmed.contains(&direct) || trimmed.contains(&embed) {
            return trimmed.to_string();
        }
    }

    fallback.unwrap_or_default()
}

fn pick_timestamp(content: &str) -> Option<(NaiveDate, Option<NaiveTime>)> {
    let mut selected: Option<(NaiveDate, Option<NaiveTime>)> = None;

    for (date, time) in parse_timestamps(content) {
        if selected
            .map(|(best_date, best_time)| {
                (date, time_or_midnight(time)) < (best_date, time_or_midnight(best_time))
            })
            .unwrap_or(true)
        {
            selected = Some((date, time));
        }
    }

    selected
}

fn parse_timestamps(content: &str) -> Vec<(NaiveDate, Option<NaiveTime>)> {
    let prefixes = ["@due(", "@start(", "@at("];
    let mut out = Vec::new();

    for line in content.lines() {
        for prefix in prefixes {
            let mut offset = 0usize;
            while offset < line.len() {
                let Some(match_rel) = line[offset..].find(prefix) else {
                    break;
                };

                let value_start = offset + match_rel + prefix.len();
                let Some(close_rel) = line[value_start..].find(')') else {
                    break;
                };
                let value_end = value_start + close_rel;

                if let Some(parsed) = parse_timestamp_value(&line[value_start..value_end]) {
                    out.push(parsed);
                }

                offset = value_end + 1;
            }
        }
    }

    out
}

fn parse_timestamp_value(raw: &str) -> Option<(NaiveDate, Option<NaiveTime>)> {
    let mut parts = raw.split_whitespace();
    let date_str = parts.next()?;
    let time_str = parts.next();

    if parts.next().is_some() {
        return None;
    }

    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
    let time = match time_str {
        Some(value) => Some(NaiveTime::parse_from_str(value, "%H:%M").ok()?),
        None => None,
    };
    Some((date, time))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;
    use crate::document::Document;
    use crate::parser::parse;

    fn make_index() -> (TempDir, SqliteIndex) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let index = SqliteIndex::open(&db_path).unwrap();
        (tmp, index)
    }

    fn make_doc(id: &str, title: &str, body: &str) -> Document {
        let raw = format!("---\nid: {id}\ntitle: \"{title}\"\ntags: []\n---\n\n{body}\n");
        parse(&raw).unwrap()
    }

    #[test]
    fn entries_sort_chronologically_and_fallback_to_path_order() {
        let (_tmp, mut index) = make_index();

        let target = make_doc("target01", "Target", "Base page.");
        let early = make_doc(
            "early001",
            "Early",
            "Kickoff @start(2026-03-01) [[target01|Target]].",
        );
        let late = make_doc(
            "late0001",
            "Late",
            "Follow up @at(2026-03-02 08:15) [[target01|Target]].",
        );
        let fallback_a = make_doc("falla001", "Fallback A", "Alpha note [[target01|Target]].");
        let fallback_z = make_doc("fallz001", "Fallback Z", "Zulu note [[target01|Target]].");

        index
            .index_document(Path::new("pages/target.md"), &target)
            .unwrap();
        index
            .index_document(Path::new("journal/2026-03-02.md"), &late)
            .unwrap();
        index
            .index_document(Path::new("journal/2026-03-01.md"), &early)
            .unwrap();
        index
            .index_document(Path::new("pages/a-note.md"), &fallback_a)
            .unwrap();
        index
            .index_document(Path::new("pages/z-note.md"), &fallback_z)
            .unwrap();

        let timeline = TimelineService::new(&index);
        let entries = timeline.entries_for_page_id("target01").unwrap();
        let ids: Vec<_> = entries
            .iter()
            .map(|entry| entry.source_page_id.as_str())
            .collect();
        assert_eq!(ids, vec!["early001", "late0001", "falla001", "fallz001"]);

        assert_eq!(entries[0].date, NaiveDate::from_ymd_opt(2026, 3, 1));
        assert!(entries[0].time.is_none());
        assert_eq!(entries[1].time, NaiveTime::from_hms_opt(8, 15, 0));
        assert!(entries[2].date.is_none());
        assert!(entries[3].date.is_none());
        assert_eq!(entries[3].snippet, "Zulu note [[target01|Target]].");
    }

    #[test]
    fn entries_filter_by_target_page_id() {
        let (_tmp, mut index) = make_index();

        let target_a = make_doc("targeta1", "Target A", "A");
        let target_b = make_doc("targetb1", "Target B", "B");
        let multi = make_doc(
            "multi001",
            "Multi",
            "A ref [[targeta1|Target A]]. B ref [[targetb1|Target B]].",
        );
        let only_a = make_doc("onlya001", "Only A", "Only [[targeta1|Target A]].");
        let only_b = make_doc("onlyb001", "Only B", "Only [[targetb1|Target B]].");

        index
            .index_document(Path::new("pages/target-a.md"), &target_a)
            .unwrap();
        index
            .index_document(Path::new("pages/target-b.md"), &target_b)
            .unwrap();
        index
            .index_document(Path::new("pages/multi.md"), &multi)
            .unwrap();
        index
            .index_document(Path::new("pages/only-a.md"), &only_a)
            .unwrap();
        index
            .index_document(Path::new("pages/only-b.md"), &only_b)
            .unwrap();

        let timeline = TimelineService::new(&index);
        let a_ids: Vec<_> = timeline
            .entries_for_page_id("targeta1")
            .unwrap()
            .into_iter()
            .map(|entry| entry.source_page_id)
            .collect();
        let b_ids: Vec<_> = timeline
            .entries_for_page_id("targetb1")
            .unwrap()
            .into_iter()
            .map(|entry| entry.source_page_id)
            .collect();

        assert_eq!(a_ids, vec!["multi001", "onlya001"]);
        assert_eq!(b_ids, vec!["multi001", "onlyb001"]);
        assert!(timeline.entries_for_page_id(" ").unwrap().is_empty());
    }
}
