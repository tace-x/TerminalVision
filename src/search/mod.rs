//! Matching names, choosing what a search looks at, and knowing when to stop.
//!
//! Searching here is a question about names and paths: nothing in this module reads a
//! directory, opens a file or walks into a subdirectory. It offers the matching
//! rules — a basic substring rule, path-aware matching, extension filtering,
//! smart-case matching, and a lightweight fuzzy rule — the modes that
//! choose between them, and the handle a long search checks so that it can stop
//! when it is asked to.
//!
//! Walking a directory tree is filesystem work, so it lives in
//! [`crate::filesystem::navigation`], which uses the rules below.

use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Which entries a search looks at, and how it matches their names.
///
/// The two questions are independent: where the search looks — the listing the
/// pane has loaded, or everything below the pane's directory — and how it
/// compares a name with the query. Each mode is one answer to both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchMode {
    /// The loaded listing, matching names that contain the query.
    #[default]
    Basic,
    /// The loaded directory and everything below it, matching names that
    /// contain the query.
    Recursive,
    /// The loaded listing, matching names whose characters appear in the query
    /// order without having to be next to each other.
    Fuzzy,
    /// The loaded directory and everything below it, matching the query's
    /// characters in order.
    RecursiveFuzzy,
}

impl SearchMode {
    /// Every mode, in the order they are listed.
    pub const ALL: [SearchMode; 4] = [
        Self::Basic,
        Self::Recursive,
        Self::Fuzzy,
        Self::RecursiveFuzzy,
    ];

    /// Whether this mode searches below the pane's directory.
    pub fn is_recursive(self) -> bool {
        matches!(self, Self::Recursive | Self::RecursiveFuzzy)
    }

    /// Whether this mode matches the query as an ordered subsequence.
    pub fn is_fuzzy(self) -> bool {
        matches!(self, Self::Fuzzy | Self::RecursiveFuzzy)
    }

    /// User-facing display label for status headers.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Recursive => "Recursive",
            Self::Fuzzy => "Fuzzy",
            Self::RecursiveFuzzy => "Recursive + Fuzzy",
        }
    }
}

/// How closely a name matched the query, best first.
///
/// The ranks are comparable, which is what makes a fuzzy result list
/// deterministic: a name that is exactly the query comes before one that starts
/// with it, which comes before one that merely contains it, which comes before
/// one whose characters only appear in order somewhere in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchRank {
    /// The name is the query.
    Exact,
    /// The name starts with the query.
    Prefix,
    /// The query appears somewhere in the name.
    Substring,
    /// The query's characters appear in the name, in order.
    Subsequence,
}

/// A query, parsed and folded, ready to be compared with entries.
///
/// Features supported:
/// - Substring and fuzzy subsequence matching
/// - Case-insensitive folding with smart-case (case-sensitive if query contains uppercase)
/// - Extension filtering (`ext:rs` or `*.rs`)
/// - Subpath matching (when query contains `/` or `\`)
/// - Hidden-file detection
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matcher {
    raw_query: String,
    query: String,
    fuzzy: bool,
    case_sensitive: bool,
    extension_filter: Option<String>,
    is_path_query: bool,
}

impl Matcher {
    /// Prepares `query`, to be matched against names with [`Self::rank`] or paths with [`Self::rank_path`].
    pub fn new(query: &str, fuzzy: bool) -> Self {
        Self::new_with_options(query, fuzzy, false)
    }

    /// Prepares `query` with explicit case-sensitivity control.
    pub fn new_with_options(query: &str, fuzzy: bool, case_sensitive: bool) -> Self {
        let trimmed = query.trim();

        // 1. Check for wildcard extension query (e.g. `*.rs` or `*.tar.gz`)
        if trimmed.starts_with("*.") && trimmed.len() > 2 && !trimmed.contains(' ') {
            let ext = trimmed[2..].to_lowercase();
            return Self {
                raw_query: query.to_string(),
                query: String::new(),
                fuzzy,
                case_sensitive: false,
                extension_filter: Some(ext),
                is_path_query: false,
            };
        }

        // 2. Check for `ext:<ext>` tag in query (e.g. `ext:rs main` or `state ext:rs`)
        let mut extension_filter = None;
        let mut cleaned_parts = Vec::new();

        if query.contains("ext:") {
            for part in query.split_whitespace() {
                if let Some(ext) = part.strip_prefix("ext:") {
                    if !ext.is_empty() {
                        extension_filter = Some(ext.to_lowercase());
                    }
                } else {
                    cleaned_parts.push(part);
                }
            }
        }

        let cleaned_query = if extension_filter.is_some() {
            cleaned_parts.join(" ")
        } else {
            query.to_string()
        };

        let is_path_query = cleaned_query.contains('/') || cleaned_query.contains('\\');

        let folded_query = if case_sensitive {
            cleaned_query
        } else {
            cleaned_query.to_lowercase()
        };

        Self {
            raw_query: query.to_string(),
            query: folded_query,
            fuzzy,
            case_sensitive,
            extension_filter,
            is_path_query,
        }
    }

    /// Whether this matcher explicitly targets hidden files (e.g. query starts with `.`).
    pub fn allows_hidden(&self) -> bool {
        let trimmed = self.raw_query.trim();
        trimmed.starts_with('.') || trimmed.contains("/.") || trimmed.contains("\\.")
    }

    /// Whether this matcher has an extension filter active.
    pub fn extension_filter(&self) -> Option<&str> {
        self.extension_filter.as_deref()
    }

    /// Whether this matcher is performing a path-based comparison.
    pub fn is_path_query(&self) -> bool {
        self.is_path_query
    }

    /// Whether `name` matches, and how closely.
    pub fn rank(&self, name: &OsStr) -> Option<MatchRank> {
        let name_str = name.to_string_lossy();

        // Check extension filter if present
        if let Some(ref ext_filter) = self.extension_filter {
            let has_ext = Path::new(&*name_str)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase() == *ext_filter)
                .unwrap_or(false);

            if !has_ext {
                return None;
            }
        }

        if self.query.is_empty() {
            return Some(MatchRank::Subsequence);
        }

        let candidate = if self.case_sensitive {
            name_str.to_string()
        } else {
            name_str.to_lowercase()
        };

        if candidate == self.query {
            return Some(MatchRank::Exact);
        }

        if candidate.starts_with(self.query.as_str()) {
            return Some(MatchRank::Prefix);
        }

        if candidate.contains(self.query.as_str()) {
            return Some(MatchRank::Substring);
        }

        if self.fuzzy && is_subsequence(&candidate, &self.query) {
            return Some(MatchRank::Subsequence);
        }

        None
    }

    /// Matches a relative path (and directory status) against this matcher.
    pub fn rank_path(&self, relative_path: &Path, is_dir: bool) -> Option<MatchRank> {
        // Extension filter applies strictly to files
        if let Some(ref ext_filter) = self.extension_filter {
            if is_dir {
                return None;
            }
            let has_ext = relative_path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase() == *ext_filter)
                .unwrap_or(false);

            if !has_ext {
                return None;
            }
        }

        if self.query.is_empty() {
            return Some(MatchRank::Subsequence);
        }

        let target_str = if self.is_path_query {
            relative_path.to_string_lossy()
        } else if let Some(file_name) = relative_path.file_name() {
            file_name.to_string_lossy()
        } else {
            relative_path.to_string_lossy()
        };

        let candidate = if self.case_sensitive {
            target_str.to_string()
        } else {
            target_str.to_lowercase()
        };

        // Normalize path separators in query and candidate for consistent matching
        let normalized_candidate = candidate.replace('\\', "/");
        let normalized_query = self.query.replace('\\', "/");

        if normalized_candidate == normalized_query {
            return Some(MatchRank::Exact);
        }

        if normalized_candidate.starts_with(&normalized_query) {
            return Some(MatchRank::Prefix);
        }

        if normalized_candidate.contains(&normalized_query) {
            return Some(MatchRank::Substring);
        }

        if self.fuzzy && is_subsequence(&normalized_candidate, &normalized_query) {
            return Some(MatchRank::Subsequence);
        }

        None
    }

    /// Whether `name` matches at all.
    pub fn matches(&self, name: &OsStr) -> bool {
        self.rank(name).is_some()
    }

    /// Whether `relative_path` matches at all.
    pub fn matches_path(&self, relative_path: &Path, is_dir: bool) -> bool {
        self.rank_path(relative_path, is_dir).is_some()
    }

    /// Whether this matcher uses fuzzy subsequence matching.
    pub fn is_fuzzy(&self) -> bool {
        self.fuzzy
    }
}

/// Whether `name` matches `query`, and how closely.
pub fn match_rank(name: &OsStr, query: &str, fuzzy: bool) -> Option<MatchRank> {
    Matcher::new(query, fuzzy).rank(name)
}

/// Whether `name` matches `query` as a plain substring.
pub fn matches(name: &OsStr, query: &str) -> bool {
    Matcher::new(query, false).matches(name)
}

/// Whether `name` matches `query` as an ordered subsequence.
pub fn fuzzy_match(name: &OsStr, query: &str) -> Option<MatchRank> {
    Matcher::new(query, true).rank(name)
}

/// Whether every character of `query` appears in `name`, in that order.
fn is_subsequence(name: &str, query: &str) -> bool {
    let mut remaining = name.chars();

    query
        .chars()
        .all(|wanted| remaining.any(|character| character == wanted))
}

/// A handle a long search checks so that it can stop when it is asked to.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl PartialEq for CancelToken {
    fn eq(&self, other: &Self) -> bool {
        self.is_cancelled() == other.is_cancelled()
    }
}

impl Eq for CancelToken {}

impl CancelToken {
    /// A handle that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks a search using this handle to stop.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Whether it has been asked to stop.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn an_empty_query_matches_every_name() {
        for name in ["Cargo.toml", "src", "", "café.txt"] {
            assert!(
                matches(OsStr::new(name), ""),
                "{name:?} must match an empty query"
            );
        }
    }

    #[test]
    fn a_whole_name_matches() {
        assert!(matches(OsStr::new("Cargo.toml"), "Cargo.toml"));
        assert!(matches(OsStr::new("src"), "src"));
        assert!(!matches(OsStr::new("Cargo.toml"), "Cargo.toml "));
    }

    #[test]
    fn part_of_a_name_matches() {
        assert!(matches(OsStr::new("Cargo.toml"), "arg"));
        assert!(matches(OsStr::new("Cargo.toml"), "Cargo"));
        assert!(matches(OsStr::new("Cargo.toml"), ".toml"));
        assert!(matches(OsStr::new("main.rs"), "n.r"));
        assert!(!matches(OsStr::new("Cargo.toml"), "toml.Cargo"));
    }

    #[test]
    fn matching_ignores_case_when_query_is_lowercase() {
        assert!(matches(OsStr::new("Cargo.toml"), "car"));
        assert!(matches(OsStr::new("readme.md"), "read"));
        assert!(!matches(OsStr::new("README.md"), "not here"));
    }

    #[test]
    fn matching_case_sensitive_option() {
        let case_sensitive = Matcher::new_with_options("READ", false, true);
        assert!(case_sensitive.matches(OsStr::new("README.md")));
        assert!(!case_sensitive.matches(OsStr::new("readme.md")));

        let case_insensitive = Matcher::new("READ", false);
        assert!(case_insensitive.matches(OsStr::new("README.md")));
        assert!(case_insensitive.matches(OsStr::new("readme.md")));
    }

    #[test]
    fn extension_filter_matching() {
        let matcher = Matcher::new("*.rs", false);
        assert!(matcher.matches(OsStr::new("main.rs")));
        assert!(matcher.matches(OsStr::new("lib.rs")));
        assert!(!matcher.matches(OsStr::new("Cargo.toml")));

        let ext_matcher = Matcher::new("ext:rs state", false);
        assert!(ext_matcher.matches(OsStr::new("state.rs")));
        assert!(!ext_matcher.matches(OsStr::new("state.txt")));
        assert!(!ext_matcher.matches(OsStr::new("other.rs")));
    }

    #[test]
    fn path_matching_relative() {
        let matcher = Matcher::new("src/ui", false);
        assert!(matcher.is_path_query());
        assert!(matcher.matches_path(Path::new("src/ui/dialogs.rs"), false));
        assert!(matcher.matches_path(Path::new("src/ui"), true));
        assert!(!matcher.matches_path(Path::new("src/app/state.rs"), false));
    }

    #[test]
    fn unicode_names_are_matched_as_text() {
        assert!(matches(OsStr::new("café.txt"), "café"));
        assert!(matches(OsStr::new("café.txt"), "fé"));
        assert!(matches(OsStr::new("日本語.txt"), "日本"));
        assert!(matches(OsStr::new("Ωmega.txt"), "ωmega"));
    }

    #[test]
    fn names_with_spaces_match_as_they_are_written() {
        assert!(matches(OsStr::new("my file.txt"), "my file"));
        assert!(matches(OsStr::new("my file.txt"), " file"));
        assert!(!matches(OsStr::new("my-file.txt"), "my file"));
    }

    #[test]
    fn matching_is_deterministic() {
        let query = "cargo";
        let first = matches(OsStr::new("Cargo.toml"), query);

        for _ in 0..5 {
            assert_eq!(matches(OsStr::new("Cargo.toml"), query), first);
        }
    }

    #[test]
    fn the_rank_says_how_closely_a_name_matched() {
        assert_eq!(
            match_rank(OsStr::new("cargo"), "cargo", false),
            Some(MatchRank::Exact)
        );
        assert_eq!(
            match_rank(OsStr::new("Cargo.toml"), "cargo", false),
            Some(MatchRank::Prefix)
        );
        assert_eq!(
            match_rank(OsStr::new("Cargo.toml"), "go", false),
            Some(MatchRank::Substring)
        );
        assert_eq!(
            match_rank(OsStr::new("Cargo.toml"), "toml", false),
            Some(MatchRank::Substring)
        );
        assert_eq!(match_rank(OsStr::new("notes.txt"), "cargo", false), None);
        assert_eq!(
            match_rank(OsStr::new("anything"), "", false),
            Some(MatchRank::Subsequence),
        );
    }

    #[test]
    fn a_substring_match_is_not_a_fuzzy_match() {
        assert_eq!(match_rank(OsStr::new("cargo"), "crg", false), None,);
        assert_eq!(
            fuzzy_match(OsStr::new("cargo"), "crg"),
            Some(MatchRank::Subsequence)
        );
    }

    #[test]
    fn fuzzy_matching_takes_the_query_in_order() {
        assert_eq!(
            fuzzy_match(OsStr::new("README.md"), "rme"),
            Some(MatchRank::Subsequence)
        );
        assert_eq!(fuzzy_match(OsStr::new("README.md"), "emr"), None);
    }

    #[test]
    fn fuzzy_matching_ranks_a_whole_name_and_a_prefix_above_a_scattered_match() {
        assert_eq!(
            fuzzy_match(OsStr::new("cargo"), "cargo"),
            Some(MatchRank::Exact)
        );
        assert_eq!(
            fuzzy_match(OsStr::new("cargo.toml"), "cargo"),
            Some(MatchRank::Prefix)
        );
        assert_eq!(
            fuzzy_match(OsStr::new("my-cargo.toml"), "cargo"),
            Some(MatchRank::Substring)
        );
        assert_eq!(
            fuzzy_match(OsStr::new("c-a-r-g-o.txt"), "cargo"),
            Some(MatchRank::Subsequence)
        );

        assert!(MatchRank::Exact < MatchRank::Prefix);
        assert!(MatchRank::Prefix < MatchRank::Substring);
        assert!(MatchRank::Substring < MatchRank::Subsequence);
    }

    #[test]
    fn every_mode_says_where_it_looks_and_how_it_matches() {
        let expected = [
            (SearchMode::Basic, false, false),
            (SearchMode::Recursive, true, false),
            (SearchMode::Fuzzy, false, true),
            (SearchMode::RecursiveFuzzy, true, true),
        ];

        for (mode, recursive, fuzzy) in expected {
            assert_eq!(mode.is_recursive(), recursive, "{mode:?}");
            assert_eq!(mode.is_fuzzy(), fuzzy, "{mode:?}");
        }
    }

    #[test]
    fn test_cancel_token() {
        let token = CancelToken::new();
        assert!(!token.is_cancelled());

        token.cancel();
        assert!(token.is_cancelled());
        assert!(token.clone().is_cancelled());
    }
}
