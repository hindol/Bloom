use crate::picker::filter::PickerFilter;
use crate::picker::nucleo::fuzzy_score;
use crate::picker::source::PickerItem;
use std::collections::HashSet;

pub struct Picker<T: PickerItem> {
    all_items: Vec<T>,
    filtered: Vec<(usize, u32)>, // (index into all_items, score)
    query: String,
    selected_index: usize,
    filters: Vec<PickerFilter>,
    marked: HashSet<usize>,
}

impl<T: PickerItem> Picker<T> {
    pub fn new(items: Vec<T>) -> Self {
        let filtered: Vec<(usize, u32)> = items.iter().enumerate().map(|(i, _)| (i, 0)).collect();
        Picker {
            all_items: items,
            filtered,
            query: String::new(),
            selected_index: 0,
            filters: Vec::new(),
            marked: HashSet::new(),
        }
    }

    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.refilter();
    }

    fn refilter(&mut self) {
        if self.query.is_empty() {
            self.filtered = self
                .all_items
                .iter()
                .enumerate()
                .map(|(i, _)| (i, 0))
                .collect();
        } else {
            self.filtered = self
                .all_items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    fuzzy_score(&self.query, item.match_text()).map(|score| (i, score))
                })
                .collect();
            self.filtered.sort_by(|a, b| b.1.cmp(&a.1));
        }
        // Clamp selection
        if self.filtered.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= self.filtered.len() {
            self.selected_index = self.filtered.len() - 1;
        }
    }

    pub fn results(&self) -> Vec<&T> {
        self.filtered
            .iter()
            .map(|(i, _)| &self.all_items[*i])
            .collect()
    }

    pub fn selected(&self) -> Option<&T> {
        self.filtered
            .get(self.selected_index)
            .map(|(i, _)| &self.all_items[*i])
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.filtered.is_empty() {
            return;
        }
        let len = self.filtered.len() as i32;
        let new = (self.selected_index as i32 + delta).rem_euclid(len);
        self.selected_index = new as usize;
    }

    pub fn select_first(&mut self) {
        self.selected_index = 0;
    }

    pub fn select_last(&mut self) {
        self.selected_index = self.filtered.len().saturating_sub(1);
    }

    pub fn add_filter(&mut self, filter: PickerFilter) {
        self.filters.push(filter);
    }

    pub fn remove_filter(&mut self, index: usize) {
        if index < self.filters.len() {
            self.filters.remove(index);
        }
    }

    pub fn clear_filters(&mut self) {
        self.filters.clear();
    }

    pub fn active_filters(&self) -> &[PickerFilter] {
        &self.filters
    }

    pub fn toggle_mark(&mut self) {
        if let Some((idx, _)) = self.filtered.get(self.selected_index) {
            if !self.marked.remove(idx) {
                self.marked.insert(*idx);
            }
        }
    }

    pub fn marked_items(&self) -> Vec<&T> {
        self.marked
            .iter()
            .filter_map(|i| self.all_items.get(*i))
            .collect()
    }

    pub fn total_count(&self) -> usize {
        self.all_items.len()
    }

    pub fn filtered_count(&self) -> usize {
        self.filtered.len()
    }
}