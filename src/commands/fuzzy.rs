//! Lightweight, high-performance fuzzy matching and ranking.
//!
//! Provides fast substring, word-boundary, acronym, and subsequence matching
//! without external dependencies. Used by the Command Center, Quick Switcher,
//! and File Search systems.

/// Computes a fuzzy match score between `query` and `target`.
///
/// Returns `Some(score)` if `query` matches `target`, where higher scores indicate
/// better relevance. Returns `None` if `query` does not match `target`.
pub fn fuzzy_match(query: &str, target: &str) -> Option<i32> {
    let q = query.trim().to_lowercase();
    let t = target.to_lowercase();

    if q.is_empty() {
        return Some(0);
    }

    if t.is_empty() {
        return None;
    }

    // 1. Exact match gets maximum score
    if t == q {
        return Some(10_000);
    }

    // 2. Exact prefix match
    if t.starts_with(&q) {
        let penalty = (t.len().saturating_sub(q.len())) as i32;
        return Some(5_000 - penalty);
    }

    // 3. Substring match
    if let Some(pos) = t.find(&q) {
        let is_boundary = pos == 0
            || t.as_bytes()
                .get(pos.saturating_sub(1))
                .is_some_and(|b| is_word_separator(*b));
        let base = if is_boundary { 3_500 } else { 2_500 };
        let pos_penalty = (pos as i32) * 10;
        let len_penalty = (t.len() as i32) * 2;
        return Some(base - pos_penalty - len_penalty);
    }

    // 4. Word initials / Acronym match & Subsequence matching
    let mut best_score = fuzzy_match_subsequence(&q, &t);

    // Also test matching starting from each word boundary (prevents earlier words from causing sub-optimal greedy matches)
    for (idx, b) in t.bytes().enumerate() {
        if idx > 0
            && is_word_separator(b)
            && !t[idx + 1..].is_empty()
            && let Some(s) = fuzzy_match_subsequence(&q, &t[idx + 1..])
        {
            best_score = Some(best_score.map_or(s, |curr| curr.max(s)));
        }
    }

    best_score
}

fn fuzzy_match_subsequence(q: &str, t: &str) -> Option<i32> {
    let mut score = 1_000;
    let mut t_chars = t.char_indices().peekable();
    let mut first_match_idx: Option<usize> = None;
    let mut last_match_idx: Option<usize> = None;
    let mut prev_match_idx: Option<usize> = None;
    let mut consecutive_count = 0;

    for q_char in q.chars() {
        let mut found = false;
        for (idx, c) in t_chars.by_ref() {
            if c == q_char {
                found = true;
                if first_match_idx.is_none() {
                    first_match_idx = Some(idx);
                }
                last_match_idx = Some(idx);

                let is_boundary = idx == 0
                    || t.as_bytes()
                        .get(idx.saturating_sub(1))
                        .is_some_and(|b| is_word_separator(*b));

                if is_boundary {
                    score += 250;
                }

                if let Some(prev) = prev_match_idx {
                    if idx == prev + 1 {
                        consecutive_count += 1;
                        score += 50 * consecutive_count;
                    } else {
                        consecutive_count = 0;
                        let gap = (idx - prev) as i32;
                        score -= gap.min(40);
                    }
                }

                prev_match_idx = Some(idx);
                break;
            }
        }

        if !found {
            return None;
        }
    }

    if let (Some(first), Some(last)) = (first_match_idx, last_match_idx) {
        let span = last.saturating_sub(first) + 1;
        let extra = span.saturating_sub(q.len()) as i32;
        let compactness_bonus = (120 - extra * 15).max(0);
        score += compactness_bonus;
    }

    let length_penalty = (t.len().saturating_sub(q.len())) as i32 * 2;
    score -= length_penalty;

    Some(score.max(1))
}

/// Matches `query` against multiple target strings, returning the highest match score.
pub fn fuzzy_match_multi(query: &str, targets: &[&str]) -> Option<i32> {
    let mut best_score = None;
    for target in targets {
        if let Some(score) = fuzzy_match(query, target) {
            best_score = Some(best_score.map_or(score, |b: i32| b.max(score)));
        }
    }
    best_score
}

fn is_word_separator(b: u8) -> bool {
    b.is_ascii_whitespace()
        || b == b'-'
        || b == b'_'
        || b == b'.'
        || b == b'/'
        || b == b'\\'
        || b == b':'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_query_matches_all() {
        assert_eq!(fuzzy_match("", "anything"), Some(0));
        assert_eq!(fuzzy_match("   ", "anything"), Some(0));
    }

    #[test]
    fn test_exact_match_highest_score() {
        let exact = fuzzy_match("rename", "Rename").unwrap();
        let prefix = fuzzy_match("ren", "Rename").unwrap();
        let fuzzy = fuzzy_match("rn", "Rename").unwrap();

        assert!(exact > prefix);
        assert!(prefix > fuzzy);
    }

    #[test]
    fn test_prefix_matches() {
        assert!(fuzzy_match("ren", "Rename").is_some());
        assert!(fuzzy_match("ref", "Refresh Directory").is_some());
        assert!(fuzzy_match("cop", "Copy selected files").is_some());
        assert!(fuzzy_match("term", "~/Projects/TerminalVision").is_some());
        assert!(fuzzy_match("down", "~/Downloads").is_some());
    }

    #[test]
    fn test_fuzzy_subsequence_matches() {
        assert!(fuzzy_match("shcut", "Show Shortcuts").is_some());
        assert!(fuzzy_match("cmd", "Command Center").is_some());
        assert!(fuzzy_match("gstat", "Git Status Panel").is_some());
        assert!(fuzzy_match("main", "src/main.rs").is_some());
    }

    #[test]
    fn test_non_matching_returns_none() {
        assert_eq!(fuzzy_match("xyz", "Rename"), None);
        assert_eq!(fuzzy_match("abc", "Refresh"), None);
    }

    #[test]
    fn test_multi_target_matching() {
        let targets = ["Rename", "Rename selected entry", "Files"];
        assert!(fuzzy_match_multi("ren", &targets).is_some());
        assert!(fuzzy_match_multi("entry", &targets).is_some());
        assert_eq!(fuzzy_match_multi("zzzz", &targets), None);
    }
}
