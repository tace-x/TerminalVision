use std::cmp::Ordering;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::entry::{Entry, EntryKind, compare_by_name};
use super::error::ErrorCategory;
use super::metadata::{LinkTarget, MetadataError, read_metadata};
use crate::search::{CancelToken, MatchRank, Matcher, SearchMode};

/// A failure to discover the contents of a directory.
#[derive(Debug)]
pub struct DiscoveryError {
    path: PathBuf,
    source: io::Error,
}

impl DiscoveryError {
    fn new(path: &Path, source: io::Error) -> Self {
        Self {
            path: path.to_path_buf(),
            source,
        }
    }

    /// The path that could not be read or inspected.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Why the directory could not be read, as a kind a caller can act on.
    ///
    /// The operating system's own error is still carried by this error and
    /// still returned by [`std::error::Error::source`], so the category is a
    /// second answer to the same question, not a replacement for it.
    pub fn category(&self) -> ErrorCategory {
        ErrorCategory::of(self.source.kind())
    }
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "failed to read `{}`: {}",
            self.path.display(),
            self.source
        )
    }
}

impl Error for DiscoveryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

/// Returns the immediate children of `directory`, sorted by name.
///
/// Discovery is not recursive: only the children of `directory` are read, and
/// no file contents are read. Hidden entries are included. Symbolic links are
/// reported as links instead of as their targets. The order is by name, so it
/// does not depend on the operating system's directory order.
///
/// Only the directory that was asked for is read: nothing below it is looked
/// at, and no file is opened. Each entry is asked what kind it is, which the
/// operating system reports alongside the name on platforms that carry that
/// information, so a large directory costs one entry per child rather than one
/// call per child.
///
/// Every failure is returned instead of panicking, including a missing
/// directory, a denied permission, an invalid path and an entry that
/// disappears while the directory is being read.
pub fn read_directory(directory: &Path) -> Result<Vec<Entry>, DiscoveryError> {
    let mut entries = Vec::new();

    for item in fs::read_dir(directory).map_err(|source| DiscoveryError::new(directory, source))? {
        let item = item.map_err(|source| DiscoveryError::new(directory, source))?;
        let file_type = item
            .file_type()
            .map_err(|source| DiscoveryError::new(&item.path(), source))?;
        let kind = EntryKind::from_file_type(file_type);
        entries.push(Entry::new(item.file_name(), item.path(), kind));
    }

    // Discovery and the pane's Name mode agree on what one name sorts before
    // another, because both use the same comparison.
    entries.sort_by(compare_by_name);

    Ok(entries)
}

/// One path a recursive search found.
///
/// A result says where the entry is, what it is called, what kind it is and where it sits below
/// the directory the search started in, so a caller can show it without asking
/// the filesystem anything. No metadata is carried: a search is a question
/// about names and types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    path: PathBuf,
    name: OsString,
    relative: PathBuf,
    rank: MatchRank,
    kind: EntryKind,
}

impl SearchResult {
    /// Creates a new search result.
    pub fn new(
        path: PathBuf,
        name: OsString,
        relative: PathBuf,
        rank: MatchRank,
        kind: EntryKind,
    ) -> Self {
        Self {
            path,
            name,
            relative,
            rank,
            kind,
        }
    }

    /// The full path of the entry that matched.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The name of the entry, without any directory component.
    pub fn name(&self) -> &OsStr {
        &self.name
    }

    /// Where the entry sits below the directory the search started in.
    pub fn relative(&self) -> &Path {
        &self.relative
    }

    /// How closely the name matched the query.
    pub fn rank(&self) -> MatchRank {
        self.rank
    }

    /// What kind of entry matched.
    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    /// Whether this result is a directory.
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Directory
    }

    /// Whether this result is a regular file.
    pub fn is_file(&self) -> bool {
        self.kind == EntryKind::File
    }

    /// Whether this result is a symbolic link.
    pub fn is_symlink(&self) -> bool {
        self.kind == EntryKind::Symlink
    }
}

/// A path a recursive search could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchFailure {
    path: PathBuf,
    kind: io::ErrorKind,
}

impl SearchFailure {
    fn new(path: &Path, kind: io::ErrorKind) -> Self {
        Self {
            path: path.to_path_buf(),
            kind,
        }
    }

    /// The directory that could not be searched.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What the operating system reported.
    pub fn kind(&self) -> io::ErrorKind {
        self.kind
    }

    /// Why the directory could not be searched, as a kind a caller can act on.
    pub fn category(&self) -> ErrorCategory {
        ErrorCategory::of(self.kind)
    }
}

/// How a recursive search ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchOutcome {
    /// Every directory below the root was searched.
    Completed,
    /// The search was asked to stop before it had finished, so what it holds is
    /// only part of what is there.
    Cancelled,
    /// The directory the search started in could not be read, so there is
    /// nothing to report but the failure.
    Failed(SearchFailure),
}

/// What a recursive search found, and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeSearch {
    results: Vec<SearchResult>,
    outcome: SearchOutcome,
    skipped: usize,
}

impl TreeSearch {
    fn new(results: Vec<SearchResult>, outcome: SearchOutcome, skipped: usize) -> Self {
        Self {
            results,
            outcome,
            skipped,
        }
    }

    /// What was found, in a deterministic order.
    pub fn results(&self) -> &[SearchResult] {
        &self.results
    }

    /// How the search ended.
    pub fn outcome(&self) -> &SearchOutcome {
        &self.outcome
    }

    /// How many entries or directories were skipped because they could not be
    /// read while the search carried on.
    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// Consumes the search and returns what it found, how it ended and how much
    /// it had to skip.
    pub fn into_parts(self) -> (Vec<SearchResult>, SearchOutcome, usize) {
        (self.results, self.outcome, self.skipped)
    }
}

/// Searches `root` and everything below it for names matching `query`.
///
/// Only names and types are compared, and only entries are read: no file is opened, no file
/// content is read, and no expensive metadata is fetched. Inaccessible children are counted in
/// [`TreeSearch::skipped`] and the rest of the tree is still searched, so one
/// directory that cannot be read never ends the search. The root itself is the
/// exception: if it cannot be read there is nothing to search, which
/// [`SearchOutcome::Failed`] reports rather than hiding.
///
/// Directories waiting to be read are held on a heap stack rather than on the
/// call stack, so a deeply nested tree cannot overflow it. Each directory is
/// visited once. Symbolic links are matched by their own names and are never
/// followed, so a link to a directory is reported as a link and its target is
/// not searched, a broken link is just a name like any other, and a loop of
/// links cannot form because nothing is ever entered through one.
///
/// The order of the results never depends on the order the filesystem happened
/// to enumerate the tree in: they are ordered by how closely they matched and
/// then by their path, or by their path alone when the query is matched as a
/// substring.
///
/// `cancel` is checked before every directory and before every entry, so the
/// search stops at the first check after it is asked to. What it has found by
/// then stays in the result, and the outcome says the search did not finish.
///
/// The query is folded once for the whole walk rather than once for every
/// entry, so a walk over a directory of thousands of entries folds what was
/// asked for once: what it allocates is one result per match, and nothing per
/// entry that did not match.
pub fn search_tree(root: &Path, query: &str, mode: SearchMode, cancel: &CancelToken) -> TreeSearch {
    let matcher = Matcher::new(query, mode.is_fuzzy());
    let mut results = Vec::new();
    let mut skipped = 0;
    let mut pending = vec![root.to_path_buf()];

    while let Some(directory) = pending.pop() {
        if cancel.is_cancelled() {
            return TreeSearch::new(results, SearchOutcome::Cancelled, skipped);
        }

        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(source) if directory == root => {
                let failure = SearchFailure::new(root, source.kind());

                return TreeSearch::new(results, SearchOutcome::Failed(failure), skipped);
            }
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        for item in entries {
            if cancel.is_cancelled() {
                return TreeSearch::new(results, SearchOutcome::Cancelled, skipped);
            }

            let Ok(item) = item else {
                skipped += 1;
                continue;
            };
            let Ok(file_type) = item.file_type() else {
                skipped += 1;
                continue;
            };

            let name = item.file_name();
            let is_directory = file_type.is_dir();
            let kind = EntryKind::from_file_type(file_type);

            let path = item.path();
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

            // Skip hidden entries unless matcher explicitly allows hidden
            let is_hidden = name.to_str().map(|s| s.starts_with('.')).unwrap_or(false);
            if is_hidden && !matcher.allows_hidden() {
                if is_directory {
                    continue;
                }
                continue;
            }

            let rank = if matcher.is_path_query() {
                matcher.rank_path(&relative, is_directory)
            } else {
                matcher.rank(&name)
            };

            if rank.is_none() && !is_directory {
                continue;
            }

            if let Some(rank) = rank {
                results.push(SearchResult {
                    path: path.clone(),
                    name,
                    relative,
                    rank,
                    kind,
                });
            }

            // A symbolic link to a directory is a link here, never a directory,
            // so this is what keeps the walk out of a link's target.
            if is_directory {
                pending.push(path);
            }
        }
    }

    sort_results(&mut results, mode.is_fuzzy());

    TreeSearch::new(results, SearchOutcome::Completed, skipped)
}

/// Orders results so that they never depend on the order of enumeration.
///
/// A fuzzy search puts the closer matches first and uses the shorter name to
/// separate two names that matched equally well. Either way the path decides
/// last, which is what makes the order total: two results can never compare
/// equal and so can never fall back to the order they arrived in.
fn sort_results(results: &mut [SearchResult], fuzzy: bool) {
    results.sort_by(|left, right| {
        let by_rank = if fuzzy {
            left.rank
                .cmp(&right.rank)
                .then_with(|| left.name.len().cmp(&right.name.len()))
        } else {
            Ordering::Equal
        };

        by_rank.then_with(|| left.path.cmp(&right.path))
    });
}

/// Returns the parent directory of `path`, or `None` when it has none.
///
/// The result is purely lexical: nothing is read from the filesystem and no
/// path is resolved or canonicalized. A trailing separator is ignored, and a
/// path that is only a file name has no directory to navigate to, so the empty
/// parent that `Path::parent` reports for such a path is reported as no parent.
pub fn parent_of(path: &Path) -> Option<PathBuf> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

/// Whether `path` can be entered as a directory.
///
/// Symbolic links are followed, so a link to a directory can be entered even
/// though [`Entry::kind`] reports the link itself: entering follows the link,
/// while display keeps the link visible. A link that points nowhere, such as a
/// broken link, is not enterable.
///
/// A path whose metadata cannot be read at all is reported as an error rather
/// than answered with `false`, so a caller can tell "not a directory" apart
/// from "could not be determined".
pub fn is_enterable_directory(path: &Path) -> Result<bool, MetadataError> {
    let metadata = read_metadata(path)?;

    Ok(match metadata.kind() {
        EntryKind::Directory => true,
        EntryKind::Symlink => matches!(
            metadata.target(),
            LinkTarget::Resolved(target) if target.kind() == EntryKind::Directory
        ),
        EntryKind::File | EntryKind::Other => false,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ErrorCategory, SearchOutcome, SearchResult, TreeSearch, is_enterable_directory, parent_of,
        read_directory, search_tree,
    };
    use crate::filesystem::entry::{Entry, EntryKind, compare_by_name};
    use crate::filesystem::test_support::{TempDir, assert_not_quadratic, fill, measure};
    use crate::search::{CancelToken, MatchRank, SearchMode};
    use std::cmp::Ordering;
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    /// Creates `count` empty files directly inside `directory`, named so that
    /// their order is the order they were created in rather than the order the
    /// filesystem happens to hand them back in.
    ///
    /// The files are empty: what is tested is how a listing behaves with a
    /// great many entries, not what is in them, and writing contents would only
    /// make the fixture slower.
    fn many_files(directory: &Path, count: usize, prefix: &str) {
        fill(directory, count, prefix);
    }

    /// The names of a listing, in the order it came in.
    fn names_of(entries: &[Entry]) -> Vec<OsString> {
        entries
            .iter()
            .map(|entry| entry.name().to_os_string())
            .collect()
    }

    /// Whether a listing holds every entry once, in a deterministic order.
    fn assert_listing_is_sound(entries: &[Entry], expected: usize, context: &str) {
        assert_eq!(entries.len(), expected, "{context}: every entry is listed");

        let names = names_of(entries);
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();

        assert_eq!(
            unique.len(),
            expected,
            "{context}: a repeated name would mean a duplicated entry"
        );
        assert_eq!(
            names, unique,
            "{context}: the listing must be in a deterministic order"
        );
    }

    /// Searches a directory that may not be cancelled.
    fn search(root: &Path, query: &str, mode: SearchMode) -> TreeSearch {
        search_tree(root, query, mode, &CancelToken::new())
    }

    /// The paths a search found, as they sit below the directory it started in.
    fn found(results: &[SearchResult]) -> Vec<PathBuf> {
        results
            .iter()
            .map(|result| result.relative().to_path_buf())
            .collect()
    }

    /// Writes a file, creating the directories above it.
    fn write_file(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("directories should be created");
        }
        fs::write(path, b"content").expect("file should be written");
    }

    /// A tree of `folders` directories holding `files` files each, all named
    /// after `needle`, used where a search has to have work to do.
    fn large_tree(root: &Path, folders: usize, files: usize, needle: &str) {
        for folder in 0..folders {
            let directory = root.join(format!("folder-{folder:03}"));
            fs::create_dir(&directory).expect("directory should be created");
            for file in 0..files {
                fs::write(
                    directory.join(format!("{needle}-{file:03}.txt")),
                    b"content",
                )
                .expect("file should be written");
            }
        }
    }
    fn entry_names(entries: &[crate::filesystem::entry::Entry]) -> Vec<OsString> {
        entries
            .iter()
            .map(|entry| entry.name().to_os_string())
            .collect()
    }

    #[test]
    fn empty_directory_yields_no_entries() {
        let directory = TempDir::new("empty");

        let entries = read_directory(directory.path()).expect("discovery should succeed");

        assert!(entries.is_empty());
    }

    #[test]
    fn regular_file_is_discovered_as_a_file() {
        let directory = TempDir::new("regular-file");
        let file = directory.path().join("file.txt");
        fs::write(&file, b"content").expect("file should be written");

        let entries = read_directory(directory.path()).expect("discovery should succeed");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name(), OsStr::new("file.txt"));
        assert_eq!(entries[0].path(), file);
        assert_eq!(entries[0].kind(), EntryKind::File);
        assert!(entries[0].is_file());
    }

    #[test]
    fn subdirectory_is_discovered_as_a_directory() {
        let directory = TempDir::new("subdirectory");
        fs::create_dir(directory.path().join("child")).expect("subdirectory should be created");

        let entries = read_directory(directory.path()).expect("discovery should succeed");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name(), OsStr::new("child"));
        assert_eq!(entries[0].kind(), EntryKind::Directory);
        assert!(entries[0].is_dir());
    }

    #[test]
    fn multiple_entries_are_returned_in_name_order() {
        let directory = TempDir::new("multiple");
        fs::create_dir(directory.path().join("delta")).expect("subdirectory should be created");
        for name in ["gamma.txt", "alpha.txt", "beta"] {
            fs::write(directory.path().join(name), b"content").expect("file should be written");
        }

        let entries = read_directory(directory.path()).expect("discovery should succeed");

        let expected = ["alpha.txt", "beta", "delta", "gamma.txt"].map(OsString::from);
        assert_eq!(entry_names(&entries), expected);
    }

    #[test]
    fn discovery_is_not_recursive() {
        let directory = TempDir::new("non-recursive");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("subdirectory should be created");
        fs::write(child.join("nested.txt"), b"content").expect("file should be written");

        let entries = read_directory(directory.path()).expect("discovery should succeed");

        assert_eq!(entry_names(&entries), [OsString::from("child")]);
    }

    #[test]
    fn unicode_file_name_is_preserved() {
        let directory = TempDir::new("unicode");
        fs::write(directory.path().join("café-日本語.txt"), b"content")
            .expect("file should be written");

        let entries = read_directory(directory.path()).expect("discovery should succeed");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name(), OsStr::new("café-日本語.txt"));
    }

    #[test]
    fn file_name_with_spaces_is_preserved() {
        let directory = TempDir::new("spaces");
        fs::write(directory.path().join("hello world.txt"), b"content")
            .expect("file should be written");

        let entries = read_directory(directory.path()).expect("discovery should succeed");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name(), OsStr::new("hello world.txt"));
    }

    #[test]
    fn hidden_entries_are_included() {
        let directory = TempDir::new("hidden");
        fs::write(directory.path().join(".hidden"), b"content").expect("file should be written");

        let entries = read_directory(directory.path()).expect("discovery should succeed");

        assert_eq!(entry_names(&entries), [OsString::from(".hidden")]);
    }

    #[test]
    fn missing_directory_is_an_error() {
        let path = TempDir::new("missing").path().to_path_buf();

        let error = read_directory(&path).expect_err("discovery should fail");

        assert_eq!(error.path(), path);
        assert_eq!(io_error(&error).kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn path_to_a_file_instead_of_a_directory_is_an_error() {
        let directory = TempDir::new("not-a-directory");
        let file = directory.path().join("file.txt");
        fs::write(&file, b"content").expect("file should be written");

        let error = read_directory(&file).expect_err("discovery should fail");

        assert_eq!(error.path(), file);
    }

    #[test]
    fn parent_of_an_absolute_path_is_its_directory() {
        assert_eq!(
            parent_of(Path::new("/home/user/report.txt")),
            Some(PathBuf::from("/home/user"))
        );
    }

    #[test]
    fn parent_of_a_nested_relative_path_is_its_directory() {
        assert_eq!(
            parent_of(Path::new("projects/report.txt")),
            Some(PathBuf::from("projects"))
        );
    }

    #[test]
    fn parent_of_a_trailing_separator_is_ignored() {
        assert_eq!(
            parent_of(Path::new("/home/user/")),
            Some(PathBuf::from("/home"))
        );
    }

    #[test]
    fn parent_of_a_root_has_no_parent() {
        assert_eq!(parent_of(Path::new("/")), None);
    }

    #[test]
    fn parent_of_a_bare_file_name_has_no_parent() {
        assert_eq!(parent_of(Path::new("report.txt")), None);
    }

    #[test]
    fn parent_of_an_empty_path_has_no_parent() {
        assert_eq!(parent_of(Path::new("")), None);
    }

    #[test]
    fn a_directory_is_enterable() {
        let directory = TempDir::new("enterable-directory");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");

        assert!(is_enterable_directory(&child).expect("enterability should be determined"));
    }

    #[test]
    fn a_regular_file_is_not_enterable() {
        let directory = TempDir::new("enterable-file");
        let file = directory.path().join("file.txt");
        fs::write(&file, b"content").expect("file should be written");

        assert!(!is_enterable_directory(&file).expect("enterability should be determined"));
    }

    #[test]
    fn an_unreadable_path_reports_an_error() {
        let path = TempDir::new("enterable-missing").path().join("missing");

        let error = is_enterable_directory(&path).expect_err("enterability should fail");

        assert_eq!(error.path(), path);
        assert_eq!(io_error(&error).kind(), io::ErrorKind::NotFound);
    }

    /// The I/O error a filesystem error wraps, whichever operation produced it.
    fn io_error(error: &dyn std::error::Error) -> &io::Error {
        std::error::Error::source(error)
            .and_then(|source| source.downcast_ref::<io::Error>())
            .expect("a filesystem error should wrap an I/O error")
    }

    // Symbolic links can be created without elevation on Unix, but not reliably
    // on Windows, so the symlink tests run on Unix only.
    #[cfg(unix)]
    mod symlinks {
        use super::{entry_names, is_enterable_directory, read_directory, search, write_file};
        use crate::filesystem::entry::EntryKind;
        use crate::filesystem::test_support::{TempDir, fill, measure};
        use crate::search::SearchMode;
        use std::ffi::{OsStr, OsString};
        use std::fs;
        use std::os::unix::fs::symlink;

        #[test]
        fn a_tree_full_of_links_is_listed_and_searched_without_entering_them() {
            let directory = TempDir::new("search-many-links");
            let root = directory.path().join("tree");
            let target = root.join("target");
            fs::create_dir_all(&target).expect("directory should be created");
            fill(&target, 200, "needle");
            write_file(&target.join("needle-here.txt"));

            // Every link points at the directory holding the matches, so a walk
            // that followed them would find them once per link as well.
            for index in 0..300 {
                symlink(&target, root.join(format!("link-{index:03}"))).expect("link");
            }

            let (entries, listing_time) = measure("list a tree of 501 entries", || {
                read_directory(&root).expect("the directory should list")
            });
            assert_eq!(entries.len(), 301, "one directory and three hundred links");
            assert_eq!(
                entries.iter().filter(|entry| entry.is_symlink()).count(),
                300
            );

            let (found, search_time) = measure("search a tree of 501 entries", || {
                search(&root, "needle", SearchMode::Recursive)
            });

            assert_eq!(found.outcome(), &super::SearchOutcome::Completed);
            assert_eq!(
                found.results().len(),
                201,
                "the entries under the target are found once, never through a link"
            );
            assert_eq!(found.skipped(), 0);
            println!(
                "listing the links took {listing_time:?} and searching the tree took {search_time:?}"
            );
        }

        #[test]
        fn symlink_to_a_file_is_reported_as_a_symlink() {
            let directory = TempDir::new("symlink-file");
            let target = directory.path().join("target.txt");
            fs::write(&target, b"content").expect("file should be written");
            symlink(&target, directory.path().join("link")).expect("symlink should be created");

            let entries = read_directory(directory.path()).expect("discovery should succeed");
            let link = entries
                .iter()
                .find(|entry| entry.name() == OsStr::new("link"))
                .expect("the symlink should be discovered");

            assert_eq!(link.kind(), EntryKind::Symlink);
            assert!(link.is_symlink());
            assert!(!link.is_file());
        }

        #[test]
        fn symlink_to_a_directory_is_not_reported_as_a_directory() {
            let directory = TempDir::new("symlink-directory");
            let target = directory.path().join("target");
            fs::create_dir(&target).expect("subdirectory should be created");
            symlink(&target, directory.path().join("link")).expect("symlink should be created");

            let entries = read_directory(directory.path()).expect("discovery should succeed");
            let link = entries
                .iter()
                .find(|entry| entry.name() == OsStr::new("link"))
                .expect("the symlink should be discovered");

            assert_eq!(link.kind(), EntryKind::Symlink);
            assert!(link.is_symlink());
            assert!(!link.is_dir());
        }

        #[test]
        fn a_symlink_to_a_directory_is_enterable_but_not_reported_as_a_directory() {
            let directory = TempDir::new("symlink-enterable");
            let target = directory.path().join("target");
            fs::create_dir(&target).expect("directory should be created");
            symlink(&target, directory.path().join("link")).expect("symlink should be created");
            let link = directory.path().join("link");

            let entries = read_directory(directory.path()).expect("discovery should succeed");
            let entry = entries
                .iter()
                .find(|entry| entry.name() == OsStr::new("link"))
                .expect("the symlink should be discovered");

            assert_eq!(entry.kind(), EntryKind::Symlink);
            assert!(
                is_enterable_directory(&link).expect("enterability should be determined"),
                "entering follows the link to its directory target"
            );
        }

        #[test]
        fn a_broken_symlink_is_not_enterable() {
            let directory = TempDir::new("symlink-broken-enterable");
            let link = directory.path().join("dangling");
            symlink(directory.path().join("missing"), &link).expect("symlink should be created");

            assert!(!is_enterable_directory(&link).expect("enterability should be determined"));
        }

        #[test]
        fn broken_symlink_is_discovered_without_failing() {
            let directory = TempDir::new("symlink-broken");
            symlink(
                directory.path().join("missing"),
                directory.path().join("dangling"),
            )
            .expect("symlink should be created");

            let entries = read_directory(directory.path()).expect("discovery should succeed");
            let link = entries
                .iter()
                .find(|entry| entry.name() == OsStr::new("dangling"))
                .expect("the broken symlink should be discovered");

            assert_eq!(link.kind(), EntryKind::Symlink);
            assert!(link.is_symlink());
            assert_eq!(entry_names(&entries), [OsString::from("dangling")]);
        }
    }

    #[test]
    fn a_recursive_search_finds_a_nested_file() {
        let directory = TempDir::new("search-nested-file");
        write_file(&directory.path().join("src").join("main.rs"));
        write_file(&directory.path().join("notes.txt"));

        let search = search(directory.path(), "main", SearchMode::Recursive);

        assert_eq!(search.outcome(), &SearchOutcome::Completed);
        assert_eq!(
            found(search.results()),
            [PathBuf::from("src").join("main.rs")]
        );
        assert_eq!(
            search.results()[0].path(),
            directory.path().join("src").join("main.rs"),
            "a result says where the entry is"
        );
        assert_eq!(search.results()[0].name(), OsStr::new("main.rs"));
        assert_eq!(
            search.results()[0].relative(),
            Path::new("src").join("main.rs")
        );
        assert_eq!(search.skipped(), 0);
    }

    #[test]
    fn a_recursive_search_finds_a_nested_directory_by_its_name() {
        let directory = TempDir::new("search-nested-directory");
        fs::create_dir_all(directory.path().join("crates").join("search-engine"))
            .expect("directories should be created");

        let search = search(directory.path(), "search-engine", SearchMode::Recursive);

        assert_eq!(search.outcome(), &SearchOutcome::Completed);
        assert_eq!(
            found(search.results()),
            [PathBuf::from("crates").join("search-engine")],
            "a directory matches on its name like any other entry"
        );
    }

    #[test]
    fn a_recursive_search_walks_every_level_of_the_tree() {
        let directory = TempDir::new("search-levels");
        write_file(&directory.path().join("needle.txt"));
        write_file(&directory.path().join("one").join("needle.txt"));
        write_file(&directory.path().join("one").join("two").join("needle.txt"));
        write_file(
            &directory
                .path()
                .join("one")
                .join("two")
                .join("three")
                .join("needle.txt"),
        );
        write_file(&directory.path().join("one").join("two").join("other.txt"));

        let search = search(directory.path(), "needle", SearchMode::Recursive);

        assert_eq!(
            found(search.results()),
            [
                PathBuf::from("needle.txt"),
                PathBuf::from("one").join("needle.txt"),
                PathBuf::from("one").join("two").join("needle.txt"),
                PathBuf::from("one")
                    .join("two")
                    .join("three")
                    .join("needle.txt"),
            ],
            "the results keep the tree's own order: by path"
        );
    }

    #[test]
    fn a_recursive_search_finds_several_matches() {
        let directory = TempDir::new("search-many-matches");
        write_file(&directory.path().join("report.txt"));
        write_file(&directory.path().join("reports").join("report.md"));
        write_file(&directory.path().join("reports").join("summary.txt"));
        write_file(&directory.path().join("archive").join("old-report.txt"));

        let search = search(directory.path(), "report", SearchMode::Recursive);

        assert_eq!(
            found(search.results()),
            [
                PathBuf::from("archive").join("old-report.txt"),
                PathBuf::from("report.txt"),
                PathBuf::from("reports"),
                PathBuf::from("reports").join("report.md"),
            ]
        );
        assert_eq!(search.results().len(), 4);
    }

    #[test]
    fn a_recursive_search_matches_unicode_names_and_names_with_spaces() {
        let directory = TempDir::new("search-unicode");
        write_file(&directory.path().join("日本語.txt"));
        write_file(&directory.path().join("dossier").join("café-notes.txt"));
        write_file(&directory.path().join("my notes").join("my file.txt"));

        let unicode = search(directory.path(), "日本", SearchMode::Recursive);
        assert_eq!(found(unicode.results()), [PathBuf::from("日本語.txt")]);

        let accented = search(directory.path(), "CAFÉ", SearchMode::Recursive);
        assert_eq!(
            found(accented.results()),
            [PathBuf::from("dossier").join("café-notes.txt")],
            "a search folds case in any script"
        );

        let spaced = search(directory.path(), "my file", SearchMode::Recursive);
        assert_eq!(
            found(spaced.results()),
            [PathBuf::from("my notes").join("my file.txt")]
        );
        assert_eq!(
            found(search(directory.path(), "my notes", SearchMode::Recursive).results()),
            [PathBuf::from("my notes")],
            "the directory with a space matches too"
        );
    }

    #[test]
    fn an_empty_directory_has_nothing_to_find() {
        let directory = TempDir::new("search-empty");

        let search = search(directory.path(), "anything", SearchMode::Recursive);

        assert_eq!(search.outcome(), &SearchOutcome::Completed);
        assert!(search.results().is_empty());
        assert_eq!(search.skipped(), 0);
    }

    #[test]
    fn the_directory_the_search_started_in_is_not_a_result() {
        let directory = TempDir::new("search-root");
        write_file(&directory.path().join("needle.txt"));

        let search = search(directory.path(), "search-root", SearchMode::Recursive);

        assert!(
            search.results().is_empty(),
            "the root is where the search starts, not something it found"
        );
    }

    #[test]
    fn a_recursive_search_never_reads_a_file() {
        let directory = TempDir::new("search-contents");
        write_file(&directory.path().join("notes.txt"));
        fs::write(
            directory.path().join("notes.txt"),
            b"needle in the contents",
        )
        .expect("file should be written");

        let search = search(directory.path(), "needle", SearchMode::Recursive);

        assert!(
            search.results().is_empty(),
            "a name is matched, never what a file holds"
        );
    }

    #[test]
    fn a_missing_root_is_reported() {
        let directory = TempDir::new("search-missing-root");
        let missing = directory.path().join("not-here");

        let search = search(&missing, "anything", SearchMode::Recursive);

        match search.outcome() {
            SearchOutcome::Failed(failure) => {
                assert_eq!(failure.path(), missing);
                assert_eq!(failure.kind(), io::ErrorKind::NotFound);
            }
            other => panic!("a missing root should fail, found {other:?}"),
        }
        assert!(search.results().is_empty());
    }

    #[test]
    fn a_root_that_is_a_file_is_reported() {
        let directory = TempDir::new("search-root-file");
        let file = directory.path().join("file.txt");
        fs::write(&file, b"content").expect("file should be written");

        let search = search(&file, "anything", SearchMode::Recursive);

        assert!(
            matches!(search.outcome(), SearchOutcome::Failed(_)),
            "a file is not a tree to search, found {:?}",
            search.outcome()
        );
        assert!(search.results().is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn an_inaccessible_child_is_skipped_and_the_rest_is_still_searched() {
        use std::os::unix::fs::PermissionsExt;

        /// Restores the original permissions when the test ends, so the
        /// temporary directory stays removable.
        struct RestorePermissions {
            path: PathBuf,
            mode: u32,
        }

        impl Drop for RestorePermissions {
            fn drop(&mut self) {
                let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(self.mode));
            }
        }

        let directory = TempDir::new("search-denied-child");
        let denied = directory.path().join("denied");
        write_file(&denied.join("needle-hidden.txt"));
        write_file(&directory.path().join("open").join("needle.txt"));

        let mode = fs::metadata(&denied)
            .expect("metadata should be read")
            .permissions()
            .mode();
        let _restore = RestorePermissions {
            path: denied.clone(),
            mode,
        };
        fs::set_permissions(&denied, fs::Permissions::from_mode(0o000))
            .expect("permissions should be set");

        let search = search(directory.path(), "needle", SearchMode::Recursive);
        assert_eq!(search.outcome(), &SearchOutcome::Completed);

        if fs::read_dir(&denied).is_ok() {
            // Elevated privileges, such as running as root, ignore the mode.
            assert_eq!(search.skipped(), 0);
            assert_eq!(search.results().len(), 2);
        } else {
            assert_eq!(search.skipped(), 1, "the unreadable directory is counted");
            assert_eq!(
                found(search.results()),
                [PathBuf::from("open").join("needle.txt")],
                "one unreadable child does not end the search"
            );
        }
    }

    #[test]
    fn a_deeply_nested_tree_is_searched_without_recursing_on_the_call_stack() {
        let directory = TempDir::new("search-deep");
        let mut deep = directory.path().to_path_buf();
        for level in 0..120 {
            deep.push(format!("l{level}"));
        }
        write_file(&deep.join("needle.txt"));

        let search = search(directory.path(), "needle", SearchMode::Recursive);

        assert_eq!(search.outcome(), &SearchOutcome::Completed);
        assert_eq!(search.results().len(), 1);
        assert_eq!(search.results()[0].path(), deep.join("needle.txt"));
        assert!(search.results()[0].relative().components().count() > 100);
    }

    #[test]
    fn the_order_of_the_results_does_not_depend_on_the_order_of_creation() {
        let first = TempDir::new("search-order-first");
        let second = TempDir::new("search-order-second");
        let names = [
            "beta.txt",
            "alpha.txt",
            "zeta.txt",
            "gamma.txt",
            "delta.txt",
        ];

        for name in names {
            write_file(&first.path().join(name));
            write_file(&first.path().join("nested").join(name));
        }
        for name in names.iter().rev() {
            write_file(&second.path().join("nested").join(name));
            write_file(&second.path().join(name));
        }

        let forward = search(first.path(), "a", SearchMode::Recursive);
        let backward = search(second.path(), "a", SearchMode::Recursive);

        assert_eq!(
            forward.results().len(),
            10,
            "five names at the root and five in the directory below it"
        );
        assert_eq!(
            found(forward.results()),
            found(backward.results()),
            "the same tree is found in the same order however it was written"
        );
    }

    #[test]
    fn a_fuzzy_search_ranks_the_closer_matches_first() {
        let directory = TempDir::new("search-fuzzy-rank");
        write_file(&directory.path().join("cargo"));
        write_file(&directory.path().join("cargo.toml"));
        write_file(&directory.path().join("my-cargo.toml"));
        write_file(&directory.path().join("c-a-r-g-o.txt"));
        write_file(&directory.path().join("nothing.txt"));

        let search = search(directory.path(), "cargo", SearchMode::RecursiveFuzzy);

        assert_eq!(
            found(search.results()),
            [
                PathBuf::from("cargo"),
                PathBuf::from("cargo.toml"),
                PathBuf::from("my-cargo.toml"),
                PathBuf::from("c-a-r-g-o.txt"),
            ]
        );
        assert_eq!(search.results()[0].rank(), MatchRank::Exact);
        assert_eq!(search.results()[1].rank(), MatchRank::Prefix);
        assert_eq!(search.results()[2].rank(), MatchRank::Substring);
        assert_eq!(search.results()[3].rank(), MatchRank::Subsequence);
    }

    #[test]
    fn a_fuzzy_search_orders_results_that_match_equally_well_by_path() {
        let directory = TempDir::new("search-fuzzy-ties");
        write_file(&directory.path().join("alpha-apple.txt"));
        write_file(&directory.path().join("gamma-apple.txt"));
        write_file(&directory.path().join("nested").join("beta1-apple.txt"));
        write_file(&directory.path().join("my-apple.txt"));

        let search = search(directory.path(), "apple", SearchMode::RecursiveFuzzy);

        assert_eq!(
            found(search.results()),
            [
                PathBuf::from("my-apple.txt"),
                PathBuf::from("alpha-apple.txt"),
                PathBuf::from("gamma-apple.txt"),
                PathBuf::from("nested").join("beta1-apple.txt"),
            ],
            "the shorter name comes first, and names of one length are ordered \
             by their paths"
        );
        assert!(
            search
                .results()
                .iter()
                .all(|result| result.rank() == MatchRank::Substring),
            "every one of them matched the same way, so only the tie-break decides"
        );
    }

    #[test]
    fn a_fuzzy_search_keeps_the_substring_rule_out_of_it() {
        let directory = TempDir::new("search-fuzzy-subsequence");
        write_file(&directory.path().join("README.md"));
        write_file(&directory.path().join("notes.txt"));

        assert_eq!(
            found(search(directory.path(), "rme", SearchMode::RecursiveFuzzy).results()),
            [PathBuf::from("README.md")]
        );
        assert!(
            search(directory.path(), "rme", SearchMode::Recursive)
                .results()
                .is_empty(),
            "the plain recursive search still wants the characters next to each other"
        );
    }

    #[test]
    fn a_search_can_be_cancelled_before_it_has_read_anything() {
        let directory = TempDir::new("search-cancel-first");
        write_file(&directory.path().join("needle.txt"));
        let cancel = CancelToken::new();
        cancel.cancel();

        let search = search_tree(directory.path(), "needle", SearchMode::Recursive, &cancel);

        assert_eq!(search.outcome(), &SearchOutcome::Cancelled);
        assert!(search.results().is_empty(), "nothing had been read yet");
        assert_eq!(search.skipped(), 0);
    }

    #[test]
    fn a_search_can_be_cancelled_while_it_is_walking() {
        let directory = TempDir::new("search-cancel-during");
        // Enough entries that the walk cannot finish before the request
        // arrives: every entry costs at least one call into the filesystem.
        large_tree(directory.path(), 150, 20, "needle");
        let total = 150 * 20;

        let root = directory.path().to_path_buf();
        let cancel = CancelToken::new();
        let remote = cancel.clone();

        let search = std::thread::scope(|scope| {
            let walking =
                scope.spawn(move || search_tree(&root, "needle", SearchMode::Recursive, &remote));

            std::thread::sleep(Duration::from_millis(1));
            cancel.cancel();

            walking.join().expect("the searching thread should finish")
        });

        assert_eq!(
            search.outcome(),
            &SearchOutcome::Cancelled,
            "a search that was stopped does not report that it finished"
        );
        assert!(
            search.results().len() < total,
            "the search stopped part way through: {} of {total}",
            search.results().len()
        );
    }

    #[test]
    fn a_cancelled_search_is_never_confused_with_a_finished_one() {
        let directory = TempDir::new("search-cancelled-outcome");
        write_file(&directory.path().join("needle.txt"));
        let cancel = CancelToken::new();
        cancel.cancel();

        let stopped = search_tree(directory.path(), "needle", SearchMode::Recursive, &cancel);

        assert_ne!(stopped.outcome(), &SearchOutcome::Completed);
        assert!(matches!(stopped.outcome(), SearchOutcome::Cancelled));

        // And a search that is not cancelled still finishes normally, so the
        // two outcomes are distinguishable in both directions.
        let completed = search(directory.path(), "needle", SearchMode::Recursive);
        assert_eq!(completed.outcome(), &SearchOutcome::Completed);
        assert_eq!(completed.results().len(), 1);
    }

    // Symbolic links can be created without elevation on Unix, but not reliably
    // on Windows, so the link tests run on Unix only.
    #[cfg(unix)]
    mod links {
        use super::{TempDir, found, search, write_file};
        use crate::filesystem::navigation::SearchOutcome;
        use crate::search::SearchMode;
        use std::fs;
        use std::os::unix::fs::symlink;
        use std::path::PathBuf;

        #[test]
        fn a_broken_link_is_a_name_like_any_other() {
            let directory = TempDir::new("search-broken-link");
            write_file(&directory.path().join("needle.txt"));
            symlink(
                directory.path().join("missing"),
                directory.path().join("dangling-needle"),
            )
            .expect("symlink should be created");

            let search = search(directory.path(), "needle", SearchMode::Recursive);

            assert_eq!(search.outcome(), &SearchOutcome::Completed);
            assert_eq!(
                search.skipped(),
                0,
                "a link that points nowhere is not an error to walk past"
            );
            assert_eq!(
                found(search.results()),
                [
                    PathBuf::from("dangling-needle"),
                    PathBuf::from("needle.txt"),
                ]
            );
        }

        #[test]
        fn a_directory_link_is_matched_and_never_entered() {
            let directory = TempDir::new("search-directory-link");
            let outside = TempDir::new("search-directory-link-target");
            write_file(&outside.path().join("needle-behind-the-link.txt"));
            write_file(&directory.path().join("needle-here.txt"));
            symlink(outside.path(), directory.path().join("linked-needle"))
                .expect("symlink should be created");

            let search = search(directory.path(), "needle", SearchMode::Recursive);

            assert_eq!(
                found(search.results()),
                [
                    PathBuf::from("linked-needle"),
                    PathBuf::from("needle-here.txt"),
                ],
                "the link is a result, the tree behind it is not searched"
            );
            assert_eq!(
                search.results()[0].rank(),
                crate::search::MatchRank::Substring
            );
        }

        #[test]
        fn a_link_to_a_directory_inside_the_tree_cannot_become_a_loop() {
            let directory = TempDir::new("search-link-loop");
            write_file(&directory.path().join("here").join("needle.txt"));
            // A link that points at a directory above itself. Following it
            // would walk the same tree again, and again.
            symlink(directory.path(), directory.path().join("here").join("back"))
                .expect("symlink should be created");

            let once = search(directory.path(), "needle", SearchMode::Recursive);

            assert_eq!(once.outcome(), &SearchOutcome::Completed);
            assert_eq!(
                found(once.results()),
                [PathBuf::from("here").join("needle.txt")],
                "the same file is found once, not once per way of reaching it"
            );

            let everything = search(directory.path(), "", SearchMode::Recursive);
            assert_eq!(everything.results().len(), 3, "here, its file and the link");
            assert!(
                everything
                    .results()
                    .iter()
                    .any(|result| { result.path() == directory.path().join("here").join("back") })
            );
        }

        #[test]
        fn a_link_is_not_asked_for_the_metadata_of_what_it_points_at() {
            let directory = TempDir::new("search-link-metadata");
            let pointed_at = TempDir::new("search-link-target");
            write_file(&pointed_at.path().join("needle.txt"));
            symlink(pointed_at.path(), directory.path().join("locked"))
                .expect("symlink should be created");
            let _ = fs::read_dir(directory.path());

            let search = search(directory.path(), "locked", SearchMode::Recursive);

            assert_eq!(search.outcome(), &SearchOutcome::Completed);
            assert_eq!(found(search.results()), [PathBuf::from("locked")]);
        }
    }

    #[test]
    fn a_hundred_entry_directory_is_listed_completely_and_in_order() {
        let directory = TempDir::new("listing-hundred");
        many_files(directory.path(), 100, "entry");

        let entries = read_directory(directory.path()).expect("the listing should succeed");

        assert_listing_is_sound(&entries, 100, "a directory of a hundred files");
        assert_eq!(
            entries.first().map(|entry| entry.name().to_os_string()),
            Some(OsString::from("entry-00000.txt"))
        );
        assert_eq!(
            entries.last().map(|entry| entry.name().to_os_string()),
            Some(OsString::from("entry-00099.txt"))
        );
        assert!(entries.iter().all(|entry| entry.is_file()));
    }

    #[test]
    fn a_thousand_entry_directory_is_listed_completely_and_in_order() {
        let directory = TempDir::new("listing-thousand");
        many_files(directory.path(), 1_000, "entry");

        let entries = read_directory(directory.path()).expect("the listing should succeed");

        assert_listing_is_sound(&entries, 1_000, "a directory of a thousand files");
        assert_eq!(
            names_of(&entries),
            (0..1_000)
                .map(|index| OsString::from(format!("entry-{index:05}.txt")))
                .collect::<Vec<OsString>>(),
            "every name is there exactly once, in name order"
        );
    }

    #[test]
    fn a_five_thousand_entry_directory_is_listed_completely_and_in_order() {
        let directory = TempDir::new("listing-five-thousand");
        many_files(directory.path(), 4_999, "entry");
        let last = directory.path().join("entry-99999-directory");
        fs::create_dir(&last).expect("directory should be created");

        let entries = read_directory(directory.path()).expect("the listing should succeed");

        assert_listing_is_sound(&entries, 5_000, "a directory of five thousand entries");
        assert_eq!(
            entries.last().map(|entry| entry.name().to_os_string()),
            Some(OsString::from("entry-99999-directory")),
            "the directory sorts by its name like every other entry"
        );
        assert_eq!(
            entries.iter().filter(|entry| entry.is_dir()).count(),
            1,
            "and its kind is read without opening anything"
        );

        // The same directory read again gives the same listing, in the same
        // order: nothing about it depends on how the filesystem enumerated it.
        let again = read_directory(directory.path()).expect("the listing should succeed");
        assert_eq!(names_of(&again), names_of(&entries));
    }

    #[test]
    fn listing_a_large_directory_does_not_look_inside_it() {
        let directory = TempDir::new("listing-not-recursive");
        let inner = directory.path().join("inner");
        fs::create_dir(&inner).expect("directory should be created");
        many_files(&inner, 1_000, "nested");
        many_files(directory.path(), 100, "entry");

        let entries = read_directory(directory.path()).expect("the listing should succeed");

        assert_eq!(
            entries.len(),
            101,
            "the hundred files and the directory itself, and nothing below it"
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.name() == OsStr::new("inner") && entry.is_dir())
        );
        assert!(
            !entries
                .iter()
                .any(|entry| entry.name().to_string_lossy().starts_with("nested-")),
            "nothing from inside the directory may be listed"
        );
    }

    #[test]
    fn a_large_directory_of_mixed_names_is_listed_with_every_kind_intact() {
        let directory = TempDir::new("listing-mixed");
        many_files(directory.path(), 100, "file");
        for index in 0..20 {
            fs::create_dir(directory.path().join(format!("dir-{index:03}")))
                .expect("directory should be created");
        }
        let long = format!("{}.txt", "long".repeat(60));
        for name in ["café.txt", "日本語.txt", "my file.txt", long.as_str()] {
            fs::write(directory.path().join(name), b"content").expect("file should be written");
        }

        let entries = read_directory(directory.path()).expect("the listing should succeed");

        assert_listing_is_sound(&entries, 124, "a directory of mixed entries");
        let find = |name: &str| {
            entries
                .iter()
                .find(|entry| entry.name() == OsStr::new(name))
                .unwrap_or_else(|| panic!("{name:?} should have been listed"))
        };

        assert!(find("café.txt").is_file());
        assert!(find("日本語.txt").is_file());
        assert!(find("my file.txt").is_file());
        assert!(find("dir-000").is_dir());
        assert_eq!(
            find(&long).name().len(),
            long.len(),
            "a long name arrives whole"
        );
        assert_eq!(
            entries.iter().filter(|entry| entry.is_dir()).count(),
            20,
            "the twenty directories are all there"
        );
    }

    #[test]
    fn listing_a_directory_while_entries_are_being_removed_never_breaks_a_listing() {
        let directory = TempDir::new("listing-disappearing");
        many_files(directory.path(), 500, "entry");

        let path = directory.path().to_path_buf();
        let stop = CancelToken::new();
        let remover = stop.clone();

        std::thread::scope(|scope| {
            scope.spawn(|| {
                for index in 0..500 {
                    if remover.is_cancelled() {
                        break;
                    }
                    let _ = fs::remove_file(path.join(format!("entry-{index:05}.txt")));
                }
            });

            // An entry that disappears between the moment the directory is
            // enumerated and the moment it is asked what it is can no longer be
            // read. Whichever way that turns out, the listing either holds what
            // was there or reports the entry it could not read: it never
            // panics, never repeats an entry and never invents one.
            for round in 0..20 {
                match read_directory(directory.path()) {
                    Ok(entries) => {
                        let names = names_of(&entries);
                        let mut unique = names.clone();
                        unique.sort();
                        unique.dedup();

                        assert_eq!(unique.len(), names.len(), "no entry is listed twice");
                        assert!(
                            names.iter().all(|name| {
                                let text = name.to_string_lossy();
                                text.starts_with("entry-") && text.ends_with(".txt")
                            }),
                            "round {round}: only entries that were created are listed"
                        );
                    }
                    Err(error) => assert!(
                        error.path().starts_with(directory.path()),
                        "round {round}: the failure names a path inside the directory"
                    ),
                }
            }

            stop.cancel();
        });

        // Once nothing is moving any more the directory lists cleanly again.
        let settled = read_directory(directory.path()).expect("the listing should succeed");
        assert_listing_is_sound(
            &settled,
            settled.len(),
            "the directory once the removals stopped",
        );
        assert!(settled.len() <= 500);
    }

    /// Creates a tree of `levels` levels, each directory holding `per_level`
    /// directories and `files` files, and returns how many files it holds.
    ///
    /// The names are the ones a real directory has to cope with: several
    /// levels deep, with spaces, in more than one script, and long enough to
    /// fill a path component. None of them contains `report`, so a search for
    /// that name can only be answered by the files.
    fn mixed_tree(root: &Path, levels: usize, per_level: usize, files: usize) -> usize {
        const DECORATIONS: [&str; 4] = ["café", "日本語", "my photos", "a-very-long-name"];

        let mut parents = vec![root.to_path_buf()];
        let mut created = 0;

        for level in 0..levels {
            let mut next = Vec::new();

            for (parent_index, parent) in parents.iter().enumerate() {
                for branch in 0..per_level {
                    let decoration =
                        DECORATIONS[(parent_index + branch + level) % DECORATIONS.len()];
                    let folder = parent.join(format!("{decoration}-{branch:02}-level{level}"));
                    fs::create_dir(&folder).expect("directory should be created");

                    for file in 0..files {
                        let name = format!(
                            "report-{file:03} {}.txt",
                            DECORATIONS[(file + level) % DECORATIONS.len()]
                        );
                        fs::write(folder.join(name), b"content").expect("file should be written");
                        created += 1;
                    }

                    next.push(folder);
                }
            }

            parents = next;
        }

        created
    }

    #[test]
    fn a_ten_thousand_entry_directory_is_listed_completely_and_in_order() {
        let directory = TempDir::new("listing-ten-thousand");
        let thousand = directory.path().join("thousand");
        let ten_thousand = directory.path().join("ten-thousand");
        fs::create_dir(&thousand).expect("directory should be created");
        fs::create_dir(&ten_thousand).expect("directory should be created");

        fill(&thousand, 1_000, "entry");
        // Five names a real directory has to cope with, and the rest padded so
        // that the order they were written in is not the order they sort in.
        let long = format!("{}.txt", "long".repeat(50));
        let special: [&str; 5] = [
            "café.txt",
            "日本語.txt",
            "my notes.txt",
            long.as_str(),
            "zzz.txt",
        ];
        fill(&ten_thousand, 10_000 - special.len(), "entry");
        for name in special {
            fs::write(ten_thousand.join(name), b"content").expect("file should be written");
        }

        let (thousand_entries, thousand_time) = measure("list 1 000 entries", || {
            read_directory(&thousand).expect("the directory should list")
        });
        let (entries, ten_thousand_time) = measure("list 10 000 entries", || {
            read_directory(&ten_thousand).expect("the directory should list")
        });

        assert_eq!(thousand_entries.len(), 1_000);
        assert_eq!(entries.len(), 10_000, "every entry is listed");
        assert_listing_is_sound(&entries, 10_000, "a directory of ten thousand entries");
        assert!(
            entries
                .windows(2)
                .all(|pair| compare_by_name(&pair[0], &pair[1]) != Ordering::Greater),
            "the listing is in name order"
        );
        for name in special {
            assert!(
                entries.iter().any(|entry| entry.name() == OsStr::new(name)),
                "{name:?} must be listed whole"
            );
        }

        assert_not_quadratic(
            thousand_time,
            ten_thousand_time,
            "listing ten times as many entries",
        );
    }

    #[test]
    fn a_large_tree_is_searched_completely_and_in_a_practical_time() {
        let directory = TempDir::new("search-large-tree");
        let small_root = directory.path().join("small");
        let large_root = directory.path().join("large");
        fs::create_dir(&small_root).expect("directory should be created");
        fs::create_dir(&large_root).expect("directory should be created");

        let small_files = mixed_tree(&small_root, 2, 4, 5);
        let large_files = mixed_tree(&large_root, 3, 4, 12);
        assert_eq!(small_files, 100, "4 + 16 directories of five files each");
        assert_eq!(
            large_files, 1_008,
            "4 + 16 + 64 directories of twelve files each"
        );

        let (small_search, small_time) = measure("search a tree of 120 entries", || {
            search(&small_root, "report", SearchMode::Recursive)
        });
        let (large_search, large_time) = measure("search a tree of 1 092 entries", || {
            search(&large_root, "report", SearchMode::Recursive)
        });

        assert_eq!(small_search.results().len(), small_files);
        assert_eq!(large_search.results().len(), large_files);
        assert_eq!(large_search.outcome(), &SearchOutcome::Completed);
        assert_eq!(
            large_search.skipped(),
            0,
            "nothing below the root was unreadable"
        );

        // No path is found twice, and every result is below the root and says
        // where it sits there.
        let paths: std::collections::BTreeSet<PathBuf> = large_search
            .results()
            .iter()
            .map(|result| result.path().to_path_buf())
            .collect();
        assert_eq!(paths.len(), large_files, "each match is reported once");
        for result in large_search.results() {
            assert!(result.path().starts_with(&large_root));
            assert!(
                result.name().to_string_lossy().contains("report"),
                "{:?} does not match the query",
                result.name()
            );
            assert!(
                result.relative().components().count() >= 2,
                "{:?} must say which directory below the root it sits in",
                result.relative()
            );
        }
        assert!(
            large_search
                .results()
                .iter()
                .any(|result| result.name().to_string_lossy().contains(' ')),
            "names with spaces are found too"
        );
        assert!(
            large_search
                .results()
                .iter()
                .any(|result| result.name().to_string_lossy().contains('é')),
            "names in other scripts are found too"
        );

        assert_not_quadratic(
            small_time,
            large_time,
            "searching ten times as large a tree",
        );
    }

    #[test]
    fn cancelling_a_large_walk_stops_it_before_it_finishes() {
        let directory = TempDir::new("search-cancel-large");
        let root = directory.path().join("tree");
        fs::create_dir(&root).expect("directory should be created");
        // Enough entries that a walk cannot read them all within a fraction of
        // the time the same walk takes when it is left alone.
        let files = mixed_tree(&root, 3, 5, 32);
        assert_eq!(
            files, 4_960,
            "5 + 25 + 125 directories of thirty-two files each"
        );

        // How long the whole walk takes on this machine, so that the request
        // arrives well before the walk could have finished, whatever the
        // machine's speed.
        let (whole, whole_time) = measure("search the whole tree", || {
            search(&root, "report", SearchMode::Recursive)
        });
        assert_eq!(whole.results().len(), files);

        let pause = (whole_time / 10).max(Duration::from_micros(50));
        let cancel = CancelToken::new();
        let remote = cancel.clone();
        let walked_root = root.clone();

        let (stopped, stopped_time) = measure("cancel the same walk part way", || {
            std::thread::scope(|scope| {
                let walking = scope.spawn(move || {
                    search_tree(&walked_root, "report", SearchMode::Recursive, &remote)
                });

                std::thread::sleep(pause);
                cancel.cancel();

                walking.join().expect("the searching thread should finish")
            })
        });

        assert_eq!(
            stopped.outcome(),
            &SearchOutcome::Cancelled,
            "a walk that was stopped does not report that it finished"
        );
        assert!(
            stopped.results().len() < files,
            "the walk stopped part way: {} of {files}",
            stopped.results().len()
        );
        println!(
            "the walk was stopped after {stopped_time:?} with {} of {files} results, against {whole_time:?} for all of them",
            stopped.results().len()
        );
    }

    #[test]
    fn searching_a_large_tree_holds_one_result_per_match_and_not_the_traversal() {
        // What a walk keeps is the results it found and the directories still
        // to read. A tree of directories that match nothing therefore costs
        // nothing to keep, and a query that matches nothing at all returns
        // nothing rather than a record of everything it looked at.
        let directory = TempDir::new("search-memory");
        let root = directory.path().join("tree");
        fs::create_dir(&root).expect("directory should be created");
        let files = mixed_tree(&root, 3, 5, 10);
        assert_eq!(files, 1_550);

        let (nothing, _) = measure("search a tree for a name that is not there", || {
            search(&root, "no-such-name-anywhere", SearchMode::Recursive)
        });

        assert!(nothing.results().is_empty());
        assert_eq!(nothing.outcome(), &SearchOutcome::Completed);
        assert_eq!(nothing.skipped(), 0);
    }

    #[test]
    fn a_missing_directory_is_categorized_as_not_found() {
        let directory = TempDir::new("listing-missing-category");
        let missing = directory.path().join("nowhere");

        let error = read_directory(&missing).expect_err("there is no such directory");

        assert_eq!(error.category(), ErrorCategory::NotFound);
        assert!(!error.category().is_conflict());
        assert_eq!(
            error.path(),
            missing.as_path(),
            "the report names what could not be read"
        );
    }

    #[test]
    fn a_file_where_a_directory_was_needed_is_not_reported_as_a_missing_directory() {
        let directory = TempDir::new("listing-file-category");
        let file = directory.path().join("file.txt");
        write_file(&file);

        let error = read_directory(&file).expect_err("a file is not a directory");

        assert_ne!(
            error.category(),
            ErrorCategory::NotFound,
            "the file is right there, so this is not a missing directory"
        );
        assert!(!error.category().is_conflict());
        // The platform says that a component of the path is not a directory.
        // Where it words that differently the category is still a failure of
        // its own rather than one of the ones above.
        #[cfg(unix)]
        assert_eq!(error.category(), ErrorCategory::NotDirectory);
    }

    #[test]
    fn an_empty_directory_still_lists_as_empty() {
        let directory = TempDir::new("listing-empty-large");

        let entries = read_directory(directory.path()).expect("the listing should succeed");

        assert!(entries.is_empty());
        assert_listing_is_sound(&entries, 0, "an empty directory");
    }

    // A directory the process cannot read, and a file it cannot open, are still
    // worth listing: their names are what a listing is about.
    #[cfg(unix)]
    mod permissions {
        use super::{
            ErrorCategory, SearchMode, SearchOutcome, TempDir, assert_listing_is_sound, many_files,
            read_directory, search, write_file,
        };
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::path::PathBuf;

        /// Restores the original permissions when the test ends, so the
        /// temporary directory stays removable.
        struct RestorePermissions {
            path: PathBuf,
            mode: u32,
        }

        impl Drop for RestorePermissions {
            fn drop(&mut self) {
                let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(self.mode));
            }
        }

        fn deny(path: &std::path::Path) -> RestorePermissions {
            let mode = fs::metadata(path)
                .expect("metadata should be read")
                .permissions()
                .mode();
            let restore = RestorePermissions {
                path: path.to_path_buf(),
                mode,
            };
            fs::set_permissions(path, fs::Permissions::from_mode(0o000))
                .expect("permissions should be set");

            restore
        }

        #[test]
        fn a_directory_that_may_not_be_read_is_categorized_as_permission_denied() {
            let directory = TempDir::new("listing-denied-category");
            let locked = directory.path().join("locked");
            fs::create_dir(&locked).expect("directory should be created");
            let _locked = deny(&locked);

            let error = read_directory(&locked).expect_err("the directory may not be read");

            assert_eq!(error.category(), ErrorCategory::PermissionDenied);
            assert!(!error.category().is_conflict());
        }

        #[test]
        fn a_denied_child_is_skipped_while_a_denied_root_is_reported() {
            let directory = TempDir::new("search-denied-root-and-child");
            write_file(&directory.path().join("needle.txt"));
            let locked = directory.path().join("locked");
            fs::create_dir(&locked).expect("directory should be created");
            write_file(&locked.join("needle-inside.txt"));
            let _locked = deny(&locked);

            // A child that cannot be read does not stop the walk: what was
            // found is reported, the child is counted as skipped, and the walk
            // is not reported as having failed.
            let walked = search(directory.path(), "needle", SearchMode::Recursive);
            assert_eq!(walked.outcome(), &SearchOutcome::Completed);
            assert_eq!(walked.skipped(), 1, "the locked directory is counted");
            assert_eq!(
                walked.results().len(),
                1,
                "the entry beside the locked directory is still found"
            );

            // The root is a different matter: nothing could be searched, so the
            // failure is reported instead of an empty result.
            let from_locked = search(&locked, "needle", SearchMode::Recursive);
            let SearchOutcome::Failed(failure) = from_locked.outcome() else {
                panic!("a root that cannot be read must be reported");
            };
            assert_eq!(failure.category(), ErrorCategory::PermissionDenied);
            assert_eq!(failure.path(), locked.as_path());
            assert!(
                from_locked.results().is_empty(),
                "nothing below a root that cannot be read is claimed to have been found"
            );
        }

        #[test]
        fn a_large_listing_survives_children_that_cannot_be_opened() {
            let directory = TempDir::new("listing-denied-children");
            many_files(directory.path(), 100, "entry");
            let locked = directory.path().join("locked");
            fs::create_dir(&locked).expect("directory should be created");
            fs::write(locked.join("inside.txt"), b"content").expect("file should be written");
            let secret = directory.path().join("secret.txt");
            fs::write(&secret, b"content").expect("file should be written");

            let _locked = deny(&locked);
            let _secret = deny(&secret);

            let entries = read_directory(directory.path()).expect("the listing should succeed");

            assert_listing_is_sound(&entries, 102, "a directory with two denied children");
            assert!(
                entries
                    .iter()
                    .any(|entry| entry.name() == std::ffi::OsStr::new("locked") && entry.is_dir()),
                "a directory that cannot be opened is still a directory in the listing"
            );
            assert!(
                entries
                    .iter()
                    .any(|entry| entry.name() == std::ffi::OsStr::new("secret.txt")
                        && entry.is_file()),
                "and a file that cannot be read is still a file"
            );
            assert!(
                !entries
                    .iter()
                    .any(|entry| entry.name() == std::ffi::OsStr::new("inside.txt")),
                "nothing from inside it is listed"
            );
        }
    }

    #[test]
    fn test_search_stress_and_performance() {
        use std::time::Instant;

        let directory = TempDir::new("search-stress");
        let root = directory.path().to_path_buf();

        // 1. 0 entries
        let empty_search = search_tree(
            &root,
            "anything",
            SearchMode::Recursive,
            &CancelToken::new(),
        );
        assert_eq!(empty_search.results().len(), 0);
        assert_eq!(empty_search.outcome(), &SearchOutcome::Completed);

        // 2. 1 entry
        let single_file = root.join("lone_file.txt");
        fs::write(&single_file, "content").unwrap();
        let match_search = search_tree(&root, "lone", SearchMode::Recursive, &CancelToken::new());
        assert_eq!(match_search.results().len(), 1);
        let no_match = search_tree(&root, "missing", SearchMode::Recursive, &CancelToken::new());
        assert_eq!(no_match.results().len(), 0);

        // 3. 1,000 entries (files, unicode, long names)
        for i in 0..1000 {
            let name = if i % 100 == 0 {
                format!("🦀_unicode_item_{i}.txt")
            } else if i % 50 == 0 {
                format!("very_long_name_item_with_many_characters_in_filename_stress_test_{i}.dat")
            } else {
                format!("item_{i:04}.txt")
            };
            fs::write(root.join(name), "data").unwrap();
        }

        // Measure search performance with std::time::Instant
        let start = Instant::now();
        let bulk_search = search_tree(&root, "unicode", SearchMode::Recursive, &CancelToken::new());
        let elapsed = start.elapsed();
        assert_eq!(bulk_search.results().len(), 10);
        assert_eq!(bulk_search.outcome(), &SearchOutcome::Completed);
        // Ensure search across 1,000 items is fast (well under 500ms)
        assert!(
            elapsed.as_millis() < 500,
            "1,000 file search took {elapsed:?}"
        );

        // Fuzzy search performance
        let start_fuzzy = Instant::now();
        let fuzzy_search = search_tree(
            &root,
            "uicd",
            SearchMode::RecursiveFuzzy,
            &CancelToken::new(),
        );
        let elapsed_fuzzy = start_fuzzy.elapsed();
        assert_eq!(fuzzy_search.results().len(), 10);
        assert!(
            elapsed_fuzzy.as_millis() < 500,
            "1,000 file fuzzy search took {elapsed_fuzzy:?}"
        );

        // 4. Deep directory nesting (20 levels deep)
        let mut deep_dir = root.clone();
        for lvl in 0..20 {
            deep_dir = deep_dir.join(format!("level_{lvl}"));
            fs::create_dir(&deep_dir).unwrap();
        }
        let deep_target = deep_dir.join("deep_treasure.txt");
        fs::write(&deep_target, "treasure").unwrap();

        let deep_search = search_tree(
            &root,
            "treasure",
            SearchMode::Recursive,
            &CancelToken::new(),
        );
        assert_eq!(deep_search.results().len(), 1);
        assert_eq!(
            deep_search.results()[0].name,
            std::ffi::OsStr::new("deep_treasure.txt")
        );

        // 5. Search cancellation
        let cancel = CancelToken::new();
        cancel.cancel();
        let cancelled_search = search_tree(&root, "item", SearchMode::Recursive, &cancel);
        assert_eq!(cancelled_search.outcome(), &SearchOutcome::Cancelled);

        // 6. Symlink inside directory tree (must not follow directory symlink)
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let symlink_to_root = root.join("loop_link");
            let _ = symlink(&root, &symlink_to_root);
            // Search should not follow loop_link indefinitely
            let search_with_sym = search_tree(
                &root,
                "deep_treasure",
                SearchMode::Recursive,
                &CancelToken::new(),
            );
            assert_eq!(search_with_sym.results().len(), 1);
            let _ = fs::remove_file(&symlink_to_root);
        }
    }
}
