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