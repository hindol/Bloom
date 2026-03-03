use crate::error::BloomError;
use crate::parser::traits::DocumentParser;
use crate::store::traits::NoteStore;
use chrono::{Local, NaiveDate};
use std::path::{Path, PathBuf};

pub struct Journal {
    vault_root: PathBuf,
}

impl Journal {
    pub fn new(vault_root: &Path) -> Self {
        Journal {
            vault_root: vault_root.to_path_buf(),
        }
    }

    /// Returns `vault_root/journal/YYYY-MM-DD.md`
    pub fn path_for_date(&self, date: NaiveDate) -> PathBuf {
        self.vault_root
            .join("journal")
            .join(format!("{}.md", date.format("%Y-%m-%d")))
    }

    /// Get today's date.
    pub fn today() -> NaiveDate {
        Local::now().date_naive()
    }

    /// Append a line to a journal page. Creates the file with frontmatter if needed.
    pub fn append(
        &self,
        date: NaiveDate,
        line: &str,
        store: &dyn NoteStore,
        parser: &dyn DocumentParser,
    ) -> Result<(), BloomError> {
        let path = self.path_for_date(date);
        let content = if store.exists(&path) {
            store.read(&path)?
        } else {
            self.default_frontmatter(date, parser)
        };

        let mut new_content = content;
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(line);
        new_content.push('\n');
        store.write(&path, &new_content)
    }

    /// Append a task line (- [ ] text) to a journal page.
    pub fn append_task(
        &self,
        date: NaiveDate,
        text: &str,
        store: &dyn NoteStore,
        parser: &dyn DocumentParser,
    ) -> Result<(), BloomError> {
        let task_line = format!("- [ ] {}", text);
        self.append(date, &task_line, store, parser)
    }

    /// List all journal dates that have files.
    pub fn all_dates(&self, store: &dyn NoteStore) -> Result<Vec<NaiveDate>, BloomError> {
        let journals = store.list_journals()?;
        let mut dates: Vec<NaiveDate> = journals
            .iter()
            .filter_map(|p| {
                let stem = p.file_stem()?.to_str()?;
                NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
            })
            .collect();
        dates.sort();
        Ok(dates)
    }

    /// Navigate to the next journal date after `from`.
    pub fn next_date(&self, from: NaiveDate, store: &dyn NoteStore) -> Option<NaiveDate> {
        let dates = self.all_dates(store).ok()?;
        dates.into_iter().find(|d| *d > from)
    }

    /// Navigate to the previous journal date before `from`.
    pub fn prev_date(&self, from: NaiveDate, store: &dyn NoteStore) -> Option<NaiveDate> {
        let dates = self.all_dates(store).ok()?;
        dates.into_iter().rev().find(|d| *d < from)
    }

    fn default_frontmatter(&self, date: NaiveDate, parser: &dyn DocumentParser) -> String {
        use crate::parser::traits::Frontmatter;
        use crate::types::TagName;
        use std::collections::HashMap;

        let fm = Frontmatter {
            id: None,
            title: Some(date.format("%Y-%m-%d").to_string()),
            created: Some(date),
            tags: vec![TagName("journal".to_string())],
            extra: HashMap::new(),
        };
        let mut content = parser.serialize_frontmatter(&fm);
        content.push('\n');
        content
    }
}