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