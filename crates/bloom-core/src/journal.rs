use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate};

use crate::store::{NoteStore, StoreError};

pub struct JournalService<'a, S: NoteStore> {
    store: &'a S,
    journal_dir: PathBuf,
}

impl<'a, S: NoteStore> JournalService<'a, S> {
    pub fn new(store: &'a S, journal_dir: PathBuf) -> Self {
        Self { store, journal_dir }
    }

    pub fn today_path(&self) -> PathBuf {
        journal_path_for_date(&self.journal_dir, Local::now().date_naive())
    }

    pub fn path_for_date(&self, date: NaiveDate) -> PathBuf {
        journal_path_for_date(&self.journal_dir, date)
    }

    pub fn quick_append_text(&self, text: &str) -> Result<PathBuf, StoreError> {
        let path = self.today_path();
        self.append_line(&path, text)?;
        Ok(path)
    }

    pub fn quick_append_task(&self, text: &str) -> Result<PathBuf, StoreError> {
        self.quick_append_text(&format!("- [ ] {}", text.trim()))
    }

    fn append_line(&self, path: &Path, line: &str) -> Result<(), StoreError> {
        let mut content = if self.store.exists(path) {
            self.store.read(path)?
        } else {
            String::new()
        };

        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }

        let line = line.trim_end_matches(|c| c == '\n' || c == '\r');
        content.push_str(line);
        content.push('\n');
        self.store.write(path, &content)
    }
}

pub fn journal_path_for_date(journal_dir: &Path, date: NaiveDate) -> PathBuf {
    journal_dir.join(format!("{}.md", date.format("%Y-%m-%d")))
}

pub fn prev_date(date: NaiveDate) -> NaiveDate {
    date.pred_opt().unwrap_or(date)
}

pub fn next_date(date: NaiveDate) -> NaiveDate {
    date.succ_opt().unwrap_or(date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{LocalFileStore, NoteStore};
    use tempfile::TempDir;

    fn make_store() -> (TempDir, LocalFileStore) {
        let tmp = TempDir::new().unwrap();
        let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        (tmp, store)
    }

    #[test]
    fn test_date_helpers_and_path_resolution() {
        let (_tmp, store) = make_store();
        let journal_dir = store.journal_dir();
        let service = JournalService::new(&store, journal_dir.clone());

        let date = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        assert_eq!(
            service.path_for_date(date),
            journal_dir.join("2026-03-01.md")
        );
        assert_eq!(
            prev_date(date),
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()
        );
        assert_eq!(
            next_date(date),
            NaiveDate::from_ymd_opt(2026, 3, 2).unwrap()
        );

        let today_name = service
            .today_path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(today_name, Local::now().format("%Y-%m-%d.md").to_string());
    }

    #[test]
    fn test_quick_append_text_entry() {
        let (_tmp, store) = make_store();
        let service = JournalService::new(&store, store.journal_dir());
        let today_path = service.today_path();

        assert!(!store.exists(&today_path));
        service.quick_append_text("first entry").unwrap();
        assert_eq!(store.read(&today_path).unwrap(), "first entry\n");

        store
            .write(&today_path, "existing without newline")
            .unwrap();
        service.quick_append_text("second entry").unwrap();
        assert_eq!(
            store.read(&today_path).unwrap(),
            "existing without newline\nsecond entry\n"
        );
    }

    #[test]
    fn test_quick_append_task_entry() {
        let (_tmp, store) = make_store();
        let service = JournalService::new(&store, store.journal_dir());
        let path = service.quick_append_task("Ship phase 1 journal").unwrap();
        assert_eq!(store.read(&path).unwrap(), "- [ ] Ship phase 1 journal\n");
    }
}
