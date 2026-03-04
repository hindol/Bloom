use crate::picker::filter::PickerFilter;
use crate::picker::nucleo::{fuzzy_score, all_words_score};
use crate::picker::source::PickerItem;
use std::collections::HashSet;

/// How the picker matches query against items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchMode {
    /// Subsequence fuzzy matching (default for FindPage, SwitchBuffer, etc.)
    Fuzzy,
    /// All whitespace-separated words must appear as contiguous substrings.
    AllWords,
}

pub struct Picker<T: PickerItem> {
    all_items: Vec<T>,
    filtered: Vec<(usize, u32)>, // (index into all_items, score)
    query: String,
    selected_index: usize,
    filters: Vec<PickerFilter>,
    marked: HashSet<usize>,
    match_mode: MatchMode,
}

impl<T: PickerItem> Picker<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self::with_match_mode(items, MatchMode::Fuzzy)
    }

    pub fn with_match_mode(items: Vec<T>, match_mode: MatchMode) -> Self {
        let filtered: Vec<(usize, u32)> = items.iter().enumerate().map(|(i, _)| (i, 0)).collect();
        Picker {
            all_items: items,
            filtered,
            query: String::new(),
            selected_index: 0,
            filters: Vec::new(),
            marked: HashSet::new(),
            match_mode,
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
            let scorer = match self.match_mode {
                MatchMode::Fuzzy => fuzzy_score as fn(&str, &str) -> Option<u32>,
                MatchMode::AllWords => all_words_score,
            };
            self.filtered = self
                .all_items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    scorer(&self.query, item.match_text()).map(|score| (i, score))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picker::source::*;

    #[derive(Clone)]
    struct TestItem {
        name: String,
    }

    impl PickerItem for TestItem {
        fn match_text(&self) -> &str {
            &self.name
        }
        fn display(&self) -> PickerRow {
            PickerRow {
                label: self.name.clone(),
                middle: None,
                right: None,
            }
        }
        fn preview(&self) -> Option<String> {
            None
        }
    }

    fn items(names: &[&str]) -> Vec<TestItem> {
        names
            .iter()
            .map(|n| TestItem {
                name: n.to_string(),
            })
            .collect()
    }

    // UC-08: Fuzzy matching
    #[test]
    fn test_empty_query_returns_all() {
        let picker = Picker::new(items(&["alpha", "beta", "gamma"]));
        assert_eq!(picker.results().len(), 3);
    }

    #[test]
    fn test_query_filters_results() {
        let mut picker = Picker::new(items(&["alpha", "beta", "gamma"]));
        picker.set_query("al");
        assert_eq!(picker.filtered_count(), 1);
        assert_eq!(picker.results()[0].name, "alpha");
    }

    #[test]
    fn test_no_match_returns_empty() {
        let mut picker = Picker::new(items(&["alpha", "beta"]));
        picker.set_query("xyz");
        assert_eq!(picker.filtered_count(), 0);
        assert!(picker.selected().is_none());
    }

    // UC-11: Selection navigation
    #[test]
    fn test_move_selection_wraps() {
        let mut picker = Picker::new(items(&["a", "b", "c"]));
        assert_eq!(picker.selected_index(), 0);
        picker.move_selection(1);
        assert_eq!(picker.selected_index(), 1);
        picker.move_selection(1);
        assert_eq!(picker.selected_index(), 2);
        picker.move_selection(1); // wraps
        assert_eq!(picker.selected_index(), 0);
    }

    #[test]
    fn test_move_selection_backwards() {
        let mut picker = Picker::new(items(&["a", "b", "c"]));
        picker.move_selection(-1); // wraps to last
        assert_eq!(picker.selected_index(), 2);
    }

    #[test]
    fn test_select_first_and_last() {
        let mut picker = Picker::new(items(&["a", "b", "c"]));
        picker.select_last();
        assert_eq!(picker.selected_index(), 2);
        picker.select_first();
        assert_eq!(picker.selected_index(), 0);
    }

    // UC-28: Batch marking
    #[test]
    fn test_toggle_mark() {
        let mut picker = Picker::new(items(&["a", "b", "c"]));
        picker.toggle_mark(); // mark "a"
        picker.move_selection(1);
        picker.toggle_mark(); // mark "b"
        assert_eq!(picker.marked_items().len(), 2);
    }

    #[test]
    fn test_toggle_mark_unmarks() {
        let mut picker = Picker::new(items(&["a", "b"]));
        picker.toggle_mark();
        assert_eq!(picker.marked_items().len(), 1);
        picker.toggle_mark(); // unmark
        assert_eq!(picker.marked_items().len(), 0);
    }

    #[test]
    fn test_total_and_filtered_count() {
        let mut picker = Picker::new(items(&["alpha", "beta", "gamma"]));
        assert_eq!(picker.total_count(), 3);
        picker.set_query("a");
        assert!(picker.filtered_count() <= 3);
        assert_eq!(picker.total_count(), 3); // total unchanged
    }

    // Fuzzy scoring
    #[test]
    fn test_fuzzy_score_filters_non_matching() {
        let mut picker = Picker::new(items(&["alphabet", "alpha", "zoo"]));
        picker.set_query("alpha");
        let results = picker.results();
        // "zoo" should not appear; both "alphabet" and "alpha" match
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.name.contains("alph")));
    }

    #[test]
    fn test_fuzzy_score_prefers_word_boundary() {
        let mut picker = Picker::new(items(&["xhello", "hello"]));
        picker.set_query("hello");
        let results = picker.results();
        // "hello" matches at word boundary (start), should score higher
        assert_eq!(results[0].name, "hello");
    }

    #[test]
    fn test_selected_returns_first_by_default() {
        let picker = Picker::new(items(&["a", "b", "c"]));
        assert_eq!(picker.selected().unwrap().name, "a");
    }

    #[test]
    fn test_empty_picker() {
        let picker = Picker::<TestItem>::new(vec![]);
        assert_eq!(picker.total_count(), 0);
        assert_eq!(picker.filtered_count(), 0);
        assert!(picker.selected().is_none());
        assert!(picker.results().is_empty());
    }

    #[test]
    fn test_move_selection_empty_picker() {
        let mut picker = Picker::<TestItem>::new(vec![]);
        picker.move_selection(1); // should not panic
        assert_eq!(picker.selected_index(), 0);
    }

    #[test]
    fn test_query_then_clear() {
        let mut picker = Picker::new(items(&["alpha", "beta", "gamma"]));
        picker.set_query("al");
        assert_eq!(picker.filtered_count(), 1);
        picker.set_query("");
        assert_eq!(picker.filtered_count(), 3);
    }

    #[test]
    fn test_selection_clamped_after_filter() {
        let mut picker = Picker::new(items(&["alpha", "beta", "gamma"]));
        picker.select_last(); // index 2
        picker.set_query("alpha"); // only 1 result
        assert_eq!(picker.selected_index(), 0);
    }

    #[test]
    fn test_case_insensitive_matching() {
        let mut picker = Picker::new(items(&["Alpha", "BETA", "gamma"]));
        picker.set_query("alpha");
        assert_eq!(picker.filtered_count(), 1);
        assert_eq!(picker.results()[0].name, "Alpha");
    }
}