use crate::index::{AgendaFilters, Index};
use crate::types::{Task, Timestamp};
use chrono::NaiveDate;

pub struct Agenda {}

pub struct AgendaView {
    pub overdue: Vec<Task>,
    pub today: Vec<Task>,
    pub upcoming: Vec<Task>,
    pub total_open: usize,
    pub total_pages: usize,
}

impl Agenda {
    pub fn new() -> Self {
        Agenda {}
    }

    /// Build the agenda view by categorizing tasks from the index by due date.
    pub fn build(&self, today: NaiveDate, index: &Index, filters: &AgendaFilters) -> AgendaView {
        let tasks = index.tasks_filtered(filters);

        let mut overdue = Vec::new();
        let mut today_tasks = Vec::new();
        let mut upcoming = Vec::new();

        let mut page_ids = std::collections::HashSet::new();

        for task in &tasks {
            if task.done {
                continue;
            }
            page_ids.insert(task.source_page.clone());

            let due_date = task.timestamps.iter().find_map(|ts| match ts {
                Timestamp::Due(d) => Some(*d),
                _ => None,
            });

            match due_date {
                Some(d) if d < today => overdue.push(task.clone()),
                Some(d) if d == today => today_tasks.push(task.clone()),
                Some(_) => upcoming.push(task.clone()),
                None => upcoming.push(task.clone()),
            }
        }

        let total_open = tasks.len();
        let total_pages = page_ids.len();

        AgendaView {
            overdue,
            today: today_tasks,
            upcoming,
            total_open,
            total_pages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::*;
    use crate::types::*;
    use chrono::NaiveDate;

    // UC-43: Agenda categorization
    #[test]
    fn test_agenda_categorizes_tasks() {
        let mut idx = Index::open_in_memory().unwrap();
        let page_id = PageId::from_hex("aabbccdd").unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 3, 3).unwrap();
        let yesterday = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap();
        let tomorrow = NaiveDate::from_ymd_opt(2026, 3, 4).unwrap();

        let entry = IndexEntry {
            meta: PageMeta {
                id: page_id.clone(),
                title: "Tasks".into(),
                created: today,
                tags: vec![],
                path: "tasks.md".into(),
            },
            content: "tasks".into(),
            links: vec![],
            tags: vec![],
            tasks: vec![
                Task { text: "Overdue".into(), done: false, timestamps: vec![Timestamp::Due(yesterday)], source_page: page_id.clone(), line: 1 },
                Task { text: "Today".into(), done: false, timestamps: vec![Timestamp::Due(today)], source_page: page_id.clone(), line: 2 },
                Task { text: "Tomorrow".into(), done: false, timestamps: vec![Timestamp::Due(tomorrow)], source_page: page_id.clone(), line: 3 },
            ],
            block_ids: vec![],
        };
        idx.index_page(&entry).unwrap();

        let agenda = Agenda::new();
        let filters = AgendaFilters { tags: vec![], page: None, date_range: None };
        let view = agenda.build(today, &idx, &filters);
        assert_eq!(view.overdue.len(), 1);
        assert_eq!(view.today.len(), 1);
        assert_eq!(view.upcoming.len(), 1);
    }
}