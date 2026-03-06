/// Score a candidate against a query using simple fuzzy matching.
/// Returns Some(score) if all query chars match in order, None otherwise.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    let query_lower = query.to_lowercase();
    let candidate_lower = candidate.to_lowercase();

    let mut score: u32 = 0;
    let mut query_chars = query_lower.chars().peekable();
    let mut last_match_pos: Option<usize> = None;

    for (i, c) in candidate_lower.chars().enumerate() {
        if let Some(&qc) = query_chars.peek() {
            if c == qc {
                score += 1;
                // Bonus for consecutive matches
                if let Some(last) = last_match_pos {
                    if i == last + 1 {
                        score += 2;
                    }
                }
                // Bonus for matching at word boundary
                if i == 0
                    || candidate_lower
                        .as_bytes()
                        .get(i - 1)
                        .map(|&b| b == b' ' || b == b'_' || b == b'-' || b == b'/')
                        .unwrap_or(false)
                {
                    score += 3;
                }
                last_match_pos = Some(i);
                query_chars.next();
            }
        }
    }

    if query_chars.peek().is_some() {
        None // not all query chars matched
    } else {
        Some(score)
    }
}

/// Score a candidate using all-words substring matching.
/// Every whitespace-separated word in the query must appear as a contiguous
/// case-insensitive substring in the candidate. Returns None if any word is
/// missing. Score favours earlier match positions.
pub fn all_words_score(query: &str, candidate: &str) -> Option<u32> {
    let candidate_lower = candidate.to_lowercase();
    let mut score: u32 = 0;

    for word in query.split_whitespace() {
        if word.is_empty() {
            continue;
        }
        let word_lower = word.to_lowercase();
        match candidate_lower.find(&word_lower) {
            Some(pos) => {
                // Earlier matches score higher
                let pos_bonus = 100u32.saturating_sub(pos as u32);
                // Word boundary bonus
                let boundary = pos == 0
                    || candidate_lower.as_bytes().get(pos - 1)
                        .map(|&b| b == b' ' || b == b'_' || b == b'-' || b == b'/')
                        .unwrap_or(false);
                score += pos_bonus + if boundary { 50 } else { 0 };
            }
            None => return None,
        }
    }

    Some(score)
}

/// Score a candidate by fuzzy-matching each query word independently.
/// Every whitespace-separated word in the query must fuzzy-match (subsequence)
/// somewhere in the candidate. Returns None if any word fails to match.
/// "mem pat re" matches "Deep research on memory usage patterns".
pub fn fuzzy_words_score(query: &str, candidate: &str) -> Option<u32> {
    let mut total_score: u32 = 0;
    let mut word_count: u32 = 0;

    for word in query.split_whitespace() {
        if word.is_empty() {
            continue;
        }
        match fuzzy_score(word, candidate) {
            Some(s) => {
                total_score += s;
                word_count += 1;
            }
            None => return None,
        }
    }

    if word_count == 0 {
        return Some(0);
    }
    Some(total_score)
}

/// Fuzzy match a query against multiple items. Returns (index, score) pairs sorted by score desc.
pub fn fuzzy_match(query: &str, items: &[&str]) -> Vec<(usize, u32)> {
    let mut results: Vec<(usize, u32)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| fuzzy_score(query, item).map(|s| (i, s)))
        .collect();
    results.sort_by(|a, b| b.1.cmp(&a.1));
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_words_matches_single_word() {
        assert!(all_words_score("rust", "Written in Rust").is_some());
    }

    #[test]
    fn all_words_matches_multiple_words_any_order() {
        assert!(all_words_score("rust edi", "An EDItor for RUST").is_some());
        assert!(all_words_score("edi rust", "An EDItor for RUST").is_some());
    }

    #[test]
    fn all_words_rejects_missing_word() {
        assert!(all_words_score("rust python", "Written in Rust").is_none());
    }

    #[test]
    fn all_words_rejects_subsequence() {
        // "rust" as subsequence (r-u-s-t) but not contiguous
        assert!(all_words_score("rust", "Ropes use structures today").is_none());
    }

    #[test]
    fn all_words_case_insensitive() {
        assert!(all_words_score("RUST", "rust programming").is_some());
    }

    #[test]
    fn fuzzy_words_matches_partial_words() {
        // "mem pat re" should match — each word fuzzy-matches independently
        assert!(fuzzy_words_score("mem pat re", "Deep research on memory usage patterns").is_some());
    }

    #[test]
    fn fuzzy_words_matches_single_word() {
        assert!(fuzzy_words_score("rop", "Rope data structures are fast").is_some());
    }

    #[test]
    fn fuzzy_words_rejects_missing_word() {
        assert!(fuzzy_words_score("mem xyz", "Deep research on memory usage patterns").is_none());
    }

    #[test]
    fn fuzzy_words_empty_query() {
        assert!(fuzzy_words_score("", "anything").is_some());
    }
}