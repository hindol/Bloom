//! BQL query cache — avoids re-executing queries on every render frame.
//!
//! Queries are cached by their text. The cache is invalidated when the index
//! changes (generation bump on `IndexComplete`). On cache miss, the query is
//! executed synchronously — BQL queries return in < 1ms so async is unnecessary.

use std::collections::HashMap;

use super::execute::QueryResult;

/// Cache for BQL query results, keyed by query text.
pub struct QueryCache {
    entries: HashMap<String, CacheEntry>,
    generation: u64,
}

struct CacheEntry {
    result: QueryResult,
    generation: u64,
    today: String,
}

impl QueryCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            generation: 0,
        }
    }

    /// Mark all entries as stale. Call on `IndexComplete`.
    pub fn invalidate(&mut self) {
        self.generation += 1;
    }

    /// Get a cached result if it's still fresh (same generation and same day).
    pub fn get(&self, query_text: &str) -> Option<&QueryResult> {
        let current_today = chrono::Local::now().format("%Y-%m-%d").to_string();
        self.entries.get(query_text).and_then(|entry| {
            if entry.generation == self.generation && entry.today == current_today {
                Some(&entry.result)
            } else {
                None
            }
        })
    }

    /// Store a freshly-computed result at the current generation.
    pub fn put(&mut self, query_text: String, result: QueryResult, today: &str) {
        self.entries.insert(
            query_text,
            CacheEntry {
                result,
                generation: self.generation,
                today: today.to_string(),
            },
        );
    }

    /// Current generation (for testing).
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::execute::{QueryResultKind, RowResult};
    use crate::query::parse::Source;

    fn dummy_result() -> QueryResult {
        QueryResult {
            source: Source::Tasks,
            kind: QueryResultKind::Rows(RowResult {
                columns: vec!["text".into()],
                rows: vec![],
            }),
        }
    }

    fn today() -> String {
        chrono::Local::now().format("%Y-%m-%d").to_string()
    }

    #[test]
    fn cache_hit_same_generation() {
        let mut cache = QueryCache::new();
        cache.put("tasks".into(), dummy_result(), &today());
        assert!(cache.get("tasks").is_some());
    }

    #[test]
    fn cache_miss_after_invalidate() {
        let mut cache = QueryCache::new();
        cache.put("tasks".into(), dummy_result(), &today());
        cache.invalidate();
        assert!(cache.get("tasks").is_none());
    }

    #[test]
    fn cache_fresh_after_re_put() {
        let mut cache = QueryCache::new();
        cache.put("tasks".into(), dummy_result(), &today());
        cache.invalidate();
        assert!(cache.get("tasks").is_none());
        cache.put("tasks".into(), dummy_result(), &today());
        assert!(cache.get("tasks").is_some());
    }

    #[test]
    fn cache_miss_unknown_key() {
        let cache = QueryCache::new();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn generation_increments() {
        let mut cache = QueryCache::new();
        assert_eq!(cache.generation(), 0);
        cache.invalidate();
        assert_eq!(cache.generation(), 1);
        cache.invalidate();
        assert_eq!(cache.generation(), 2);
    }
}
