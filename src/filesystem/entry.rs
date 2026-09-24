use std::cmp::Ordering;
use std::ffi::{OsStr, OsString};
use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::metadata::Metadata;

/// The kind of a filesystem entry.
///
/// The kind describes the entry itself, not a symbolic link's target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A symbolic link, whether it points at a file, a directory or nothing.
    Symlink,
    /// Anything else, such as a socket, a FIFO or a device.
    Other,
}

impl EntryKind {
    /// Classifies a file type as reported by the operating system.
    ///
    /// Symbolic links are checked first, so a link is classified as a link and
    /// never as its target. Classify the file type of non-following metadata to
    /// keep that guarantee.
    pub fn from_file_type(file_type: FileType) -> Self {
        if file_type.is_symlink() {
            Self::Symlink
        } else if file_type.is_dir() {
            Self::Directory
        } else if file_type.is_file() {
            Self::File
        } else {
            Self::Other
        }
    }
}

/// One immediate child of a directory.
///
/// An entry carries identity only: its name, its path and its kind. Metadata is
/// retrieved separately through `Entry::metadata`, which is defined in the
/// `metadata` module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    name: OsString,
    path: PathBuf,
    kind: EntryKind,
}

impl Entry {
    /// Creates an entry from its name, full path and kind.
    pub fn new(name: OsString, path: PathBuf, kind: EntryKind) -> Self {
        Self { name, path, kind }
    }

    /// The file name of the entry, without any directory component.
    pub fn name(&self) -> &OsStr {
        &self.name
    }

    /// The full path of the entry, as produced by the operating system.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The kind of the entry.
    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    /// Whether the entry is a directory. A symbolic link is not a directory.
    pub fn is_dir(&self) -> bool {
        matches!(self.kind, EntryKind::Directory)
    }

    /// Whether the entry is a regular file. A symbolic link is not a file.
    pub fn is_file(&self) -> bool {
        matches!(self.kind, EntryKind::File)
    }

    /// Whether the entry is a symbolic link.
    pub fn is_symlink(&self) -> bool {
        matches!(self.kind, EntryKind::Symlink)
    }
}

/// The order a pane displays the entries it has already loaded in.
///
/// A mode is one of five fixed orderings, not a string: there is nothing to
/// spell at runtime and nothing to configure. The variants are the whole
/// vocabulary of the type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortMode {
    /// Names in ascending order.
    #[default]
    Name,
    /// Names in descending order.
    ReverseName,
    /// Known sizes, smallest first.
    Size,
    /// Known modification times, oldest first.
    Modified,
    /// Extensions in ascending order.
    Extension,
}

impl SortMode {
    /// Every mode, in the order [`SortMode::next`] visits them.
    pub const ALL: [SortMode; 5] = [
        Self::Name,
        Self::ReverseName,
        Self::Size,
        Self::Modified,
        Self::Extension,
    ];

    /// The mode one step further round the cycle.
    ///
    /// The cycle is closed: the mode after the last one is the first, so
    /// advancing it repeatedly visits every mode once and returns to where it
    /// started.
    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::ReverseName,
            Self::ReverseName => Self::Size,
            Self::Size => Self::Modified,
            Self::Modified => Self::Extension,
            Self::Extension => Self::Name,
        }
    }
}

/// What is already known about an entry besides its identity.
///
/// A key is built from metadata that was read earlier and reads nothing itself,
/// so it can be kept and reused for as many comparisons as wanted. A size or a
/// modification time that is not known is `None`, which the ordering treats as
/// "not known" rather than as a value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EntryKey {
    size: Option<u64>,
    modified: Option<SystemTime>,
}

impl EntryKey {
    /// The key of an entry nothing further is known about.
    pub fn unknown() -> Self {
        Self::default()
    }

    /// Creates a key from the size and the modification time that are known.
    pub fn new(size: Option<u64>, modified: Option<SystemTime>) -> Self {
        Self { size, modified }
    }

    /// The key of an entry whose metadata was already read.
    ///
    /// Nothing is read here either. A directory's size is not known, as
    /// [`Metadata`] reports it, and neither is any other value the metadata
    /// could not provide.
    pub fn from_metadata(metadata: &Metadata) -> Self {
        Self::new(metadata.size(), metadata.modified())
    }
}

/// The lexical order of two entries by name.
///
/// Names are compared as they are stored, without folding case and without
/// consulting a locale, so the same two names are ordered the same way on every
/// platform and on every run.
pub fn compare_by_name(left: &Entry, right: &Entry) -> Ordering {
    left.name().cmp(right.name())
}

/// Orders `entries` in place, using `keys` for what is known about them.
///
/// `keys[index]` describes `entries[index]`; an entry that has no key, or whose
/// size or time is not known, is ordered as one whose value is not known. Keys
/// live and die with their entries: when `keys` is as long as `entries`, it is
/// reordered by the same ordering, so a caller that keeps it can go on reading
/// it. Nothing is read from the filesystem, so sorting cannot fail, cannot
/// rescan a directory and never follows a symbolic link: the order is decided
/// from the entries and the keys alone.
///
/// Directories come before every other entry in every mode, and a symbolic link
/// is ordered as a link rather than as the directory it may point at. Inside
/// one group the mode decides, a value that is not known comes after a known
/// one, and both equal values and unknown values fall back to the name and then
/// to the path. Two runs over the same entries therefore produce the same
/// listing, whatever order the entries arrived in.
///
/// The ordering is worked out as positions rather than as values and then
/// applied to the entries themselves, so no entry is copied: a directory of
/// thousands of entries costs a few numbers per entry and no new names or
/// paths, however large the directory is.
pub fn sort_entries(entries: &mut [Entry], keys: &mut [EntryKey], mode: SortMode) {
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|&left, &right| {
        compare(
            mode,
            &entries[left],
            key_at(keys, left),
            &entries[right],
            key_at(keys, right),
        )
    });

    apply_ordering(entries, &order);

    if keys.len() == order.len() {
        apply_ordering(keys, &order);
    }
}

/// Moves the items of `items` to the positions `order` gives them.
///
/// `order[i]` is the position of the item that belongs at `i`, as the ordering
/// decided it. Each item is carried straight to its position by following the
/// cycles of the ordering, so nothing is copied out and back in: the only
/// allocation is one position per item, and every item is moved exactly once.
///
/// An ordering that is already the identity — which is what a listing that is
/// already sorted produces — does no work at all.
fn apply_ordering<T>(items: &mut [T], order: &[usize]) {
    assert_eq!(order.len(), items.len(), "an ordering of every item");

    // Where the item at each position has to go. The loop below consumes this
    // by swapping entries as it swaps items, which is what makes each cycle
    // need no extra memory.
    let mut destination: Vec<usize> = vec![0; items.len()];
    for (position, &source) in order.iter().enumerate() {
        destination[source] = position;
    }

    for index in 0..items.len() {
        while destination[index] != index {
            let target = destination[index];

            items.swap(index, target);
            destination.swap(index, target);
        }
    }
}

/// Compares two entries, with what is known about each, in `mode`.
fn compare(
    mode: SortMode,
    left: &Entry,
    left_key: EntryKey,
    right: &Entry,
    right_key: EntryKey,
) -> Ordering {
    directory_rank(left)
        .cmp(&directory_rank(right))
        .then_with(|| match mode {
            SortMode::Name => compare_by_name(left, right),
            SortMode::ReverseName => compare_by_name(right, left),
            SortMode::Size => compare_unknown_last(left_key.size, right_key.size),
            SortMode::Modified => compare_unknown_last(left_key.modified, right_key.modified),
            SortMode::Extension => compare_unknown_last(extension_of(left), extension_of(right)),
        })
        .then_with(|| compare_by_name(left, right))
        .then_with(|| left.path().cmp(right.path()))
}

/// The group an entry belongs to: directories first, everything else after.
///
/// A symbolic link is in the second group even when it points at a directory,
/// because following it would order the entry by a target it is not.
fn directory_rank(entry: &Entry) -> u8 {
    if entry.is_dir() { 0 } else { 1 }
}

/// The order of two values that may be unknown.
///
/// A known value comes first and two unknown values are equal, so a size or a
/// time that is not known is ordered rather than guessed at.
fn compare_unknown_last<T: Ord>(left: Option<T>, right: Option<T>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// The extension of an entry, taken from the name it already has.
///
/// The name is split by the path rules of the platform, so a name without a dot
/// has no extension, a leading dot starts a hidden name rather than an
/// extension, and the name itself is never changed. A directory is never asked,
/// because directories are ordered as their own group.
fn extension_of(entry: &Entry) -> Option<&OsStr> {
    Path::new(entry.name()).extension()
}

/// The key of the entry at `index`, as far as it is known.
fn key_at(keys: &[EntryKey], index: usize) -> EntryKey {
    keys.get(index).copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        Entry, EntryKey, EntryKind, SortMode, apply_ordering, compare_by_name, sort_entries,
    };
    use crate::filesystem::test_support::{TempDir, assert_not_quadratic, measure};
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    fn entry(kind: EntryKind) -> Entry {
        Entry::new(
            OsString::from("name"),
            PathBuf::from("directory").join("name"),
            kind,
        )
    }

    /// An entry of `kind` named `name`, in a directory that is never created: a
    /// sort that reached the filesystem would have nothing to read.
    fn unreachable(name: &str, kind: EntryKind) -> Entry {
        Entry::new(
            OsString::from(name),
            Path::new("directory").join(name),
            kind,
        )
    }

    /// An entry named `name` in a directory that does not exist.
    fn file(name: &str) -> Entry {
        unreachable(name, EntryKind::File)
    }

    /// A directory entry named `name` in a directory that does not exist.
    fn directory(name: &str) -> Entry {
        unreachable(name, EntryKind::Directory)
    }

    /// The names of `entries`, in the order they are held in.
    fn names(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| entry.name().to_string_lossy().into_owned())
            .collect()
    }

    /// The names of `entries` after they were sorted in `mode` with `keys`.
    fn sorted_names(mut entries: Vec<Entry>, keys: &[EntryKey], mode: SortMode) -> Vec<String> {
        let mut keys = keys.to_vec();
        sort_entries(&mut entries, &mut keys, mode);
        names(&entries)
    }

    /// A modification time `seconds` after the epoch.
    fn time(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    /// A file entry and the key that says how large it is.
    fn sized(name: &str, size: u64) -> (Entry, EntryKey) {
        (file(name), EntryKey::new(Some(size), None))
    }

    /// Splits paired entries into the two lists [`sort_entries`] takes.
    fn split(paired: Vec<(Entry, EntryKey)>) -> (Vec<Entry>, Vec<EntryKey>) {
        paired.into_iter().unzip()
    }

    #[test]
    fn each_kind_satisfies_only_its_own_predicate() {
        assert!(entry(EntryKind::Directory).is_dir());
        assert!(entry(EntryKind::File).is_file());
        assert!(entry(EntryKind::Symlink).is_symlink());

        let other = entry(EntryKind::Other);
        assert!(!other.is_dir());
        assert!(!other.is_file());
        assert!(!other.is_symlink());
    }

    #[test]
    fn a_symlink_is_neither_a_file_nor_a_directory() {
        let link = entry(EntryKind::Symlink);
        assert!(!link.is_dir());
        assert!(!link.is_file());
    }

    #[test]
    fn name_and_path_are_stored_unchanged() {
        let entry = Entry::new(
            OsString::from("café.txt"),
            PathBuf::from("directory").join("café.txt"),
            EntryKind::File,
        );

        assert_eq!(entry.name(), OsStr::new("café.txt"));
        assert_eq!(entry.path(), Path::new("directory").join("café.txt"));
        assert_eq!(entry.kind(), EntryKind::File);
    }

    #[test]
    fn file_type_of_a_file_and_a_directory_is_classified() {
        let directory = TempDir::new("entry-kind");
        let file = directory.path().join("file.txt");
        fs::write(&file, b"content").expect("file should be written");

        let file_metadata = fs::metadata(&file).expect("metadata should be read");
        let directory_metadata = fs::metadata(directory.path()).expect("metadata should be read");

        assert_eq!(
            EntryKind::from_file_type(file_metadata.file_type()),
            EntryKind::File
        );
        assert_eq!(
            EntryKind::from_file_type(directory_metadata.file_type()),
            EntryKind::Directory
        );
    }

    // Symbolic links can be created without elevation on Unix, but not reliably
    // on Windows, so this test runs on Unix only.
    #[cfg(unix)]
    #[test]
    fn file_type_classification_does_not_follow_a_symlink() {
        use std::os::unix::fs::symlink;

        let directory = TempDir::new("entry-kind-symlink");
        let target = directory.path().join("target.txt");
        fs::write(&target, b"content").expect("file should be written");
        let link = directory.path().join("link");
        symlink(&target, &link).expect("symlink should be created");

        let own = fs::symlink_metadata(&link).expect("metadata should be read");
        let followed = fs::metadata(&link).expect("metadata of the target should be read");

        assert_eq!(
            EntryKind::from_file_type(own.file_type()),
            EntryKind::Symlink,
            "the link itself must be classified as a link"
        );
        assert_eq!(
            EntryKind::from_file_type(followed.file_type()),
            EntryKind::File,
            "following the link yields the target's kind"
        );
    }

    #[test]
    fn compare_by_name_orders_names_and_looks_at_nothing_else() {
        assert_eq!(
            compare_by_name(&file("alpha"), &file("beta")),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_by_name(&directory("alpha"), &file("beta")),
            std::cmp::Ordering::Less,
            "the kind must not take part in the comparison"
        );
        assert_eq!(
            compare_by_name(&file("beta"), &file("alpha")),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_by_name(&file("same"), &file("same")),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn name_mode_orders_names_upper_case_first_and_locale_free() {
        let entries = vec![file("zeta"), file("Alpha"), file("beta"), file("Zeta")];

        assert_eq!(
            sorted_names(entries, &[], SortMode::Name),
            ["Alpha", "Zeta", "beta", "zeta"]
        );
    }

    #[test]
    fn names_with_spaces_punctuation_and_numbers_are_ordered_without_natural_sorting() {
        let entries = vec![
            file("file 9.txt"),
            file("file 10.txt"),
            file("file-1.txt"),
            file("file 1.txt"),
        ];

        // Digits are compared one by one, as characters: "10" comes before "9".
        assert_eq!(
            sorted_names(entries, &[], SortMode::Name),
            ["file 1.txt", "file 10.txt", "file 9.txt", "file-1.txt"]
        );
    }

    #[test]
    fn unicode_and_long_names_are_ordered_deterministically() {
        let long = format!("{}.txt", "long".repeat(80));
        let entries = vec![
            file("日本語"),
            file("café.txt"),
            file("cafe.txt"),
            file("Ωmega"),
            file(&long),
            file("ßeta"),
        ];

        // The order follows the stored bytes of each name, so it is the same on
        // every platform, in every locale and in every run.
        let expected = [
            "cafe.txt",
            "café.txt",
            long.as_str(),
            "ßeta",
            "Ωmega",
            "日本語",
        ];
        assert_eq!(sorted_names(entries.clone(), &[], SortMode::Name), expected);

        let mut rotated = entries;
        rotated.rotate_left(2);
        assert_eq!(
            sorted_names(rotated, &[], SortMode::Name),
            expected,
            "the order must not depend on the order the entries were given in"
        );
    }

    #[test]
    fn reverse_name_mode_reverses_the_names() {
        let entries = vec![file("a.txt"), file("b.txt"), file("c.txt")];

        assert_eq!(
            sorted_names(entries, &[], SortMode::ReverseName),
            ["c.txt", "b.txt", "a.txt"]
        );
    }

    #[test]
    fn directories_come_first_in_every_mode() {
        let (entries, keys) = split(vec![
            (file("a.txt"), EntryKey::new(Some(9), Some(time(9)))),
            (directory("z-dir"), EntryKey::unknown()),
            (file("b.rs"), EntryKey::new(Some(1), Some(time(1)))),
        ]);

        for mode in SortMode::ALL {
            let sorted = sorted_names(entries.clone(), &keys, mode);
            assert_eq!(sorted[0], "z-dir", "{mode:?} must put the directory first");
            assert_eq!(sorted.len(), 3, "{mode:?} must keep every entry");
        }
    }

    #[test]
    fn reverse_name_mode_keeps_directories_first() {
        let (entries, keys) = split(vec![
            (file("a.txt"), EntryKey::unknown()),
            (directory("b-dir"), EntryKey::unknown()),
            (file("c.txt"), EntryKey::unknown()),
        ]);

        let sorted = sorted_names(entries, &keys, SortMode::ReverseName);

        assert_eq!(sorted, ["b-dir", "c.txt", "a.txt"]);
        assert_ne!(
            sorted,
            ["c.txt", "b-dir", "a.txt"],
            "the whole listing must not be reversed"
        );
    }

    #[test]
    fn size_mode_orders_by_known_size_and_puts_unknown_sizes_last() {
        let (entries, keys) = split(vec![
            sized("big", 3_000),
            sized("small", 1),
            (file("unknown"), EntryKey::unknown()),
            sized("middle", 42),
        ]);

        assert_eq!(
            sorted_names(entries, &keys, SortMode::Size),
            ["small", "middle", "big", "unknown"]
        );
    }

    #[test]
    fn size_mode_never_sizes_a_directory() {
        let (entries, keys) = split(vec![
            (file("big"), EntryKey::new(Some(3_000), None)),
            (directory("z-dir"), EntryKey::unknown()),
        ]);

        // The directory has no known size, and the listing still starts with it:
        // its contents are never counted.
        let sorted = sorted_names(entries, &keys, SortMode::Size);

        assert_eq!(sorted, ["z-dir", "big"]);
    }

    #[test]
    fn modified_mode_orders_by_known_time_and_puts_unknown_times_last() {
        let (entries, keys) = split(vec![
            (file("newest"), EntryKey::new(None, Some(time(300)))),
            (file("middle"), EntryKey::new(None, Some(time(20)))),
            (file("unknown"), EntryKey::unknown()),
            (file("oldest"), EntryKey::new(None, Some(time(1)))),
        ]);

        assert_eq!(
            sorted_names(entries, &keys, SortMode::Modified),
            ["oldest", "middle", "newest", "unknown"]
        );
    }

    #[test]
    fn extension_mode_orders_by_extension() {
        let entries = vec![file("b.txt"), file("a.rs"), file("c.tar.gz")];

        assert_eq!(
            sorted_names(entries, &[], SortMode::Extension),
            ["c.tar.gz", "a.rs", "b.txt"]
        );
    }

    #[test]
    fn files_without_an_extension_come_after_the_ones_with_an_extension() {
        let entries = vec![file("b"), file("a.txt"), file(".hidden"), file("c.rs")];

        assert_eq!(
            sorted_names(entries, &[], SortMode::Extension),
            ["c.rs", "a.txt", ".hidden", "b"],
            "a leading dot is not an extension, and a missing extension is not known"
        );
    }

    #[test]
    fn a_directory_name_with_a_dot_is_not_an_extension() {
        let (entries, keys) = split(vec![
            (file("a.txt"), EntryKey::unknown()),
            (directory("notes.tar"), EntryKey::unknown()),
        ]);

        assert_eq!(
            sorted_names(entries, &keys, SortMode::Extension),
            ["notes.tar", "a.txt"]
        );
    }

    #[test]
    fn equal_sizes_are_broken_by_name() {
        let (entries, keys) = split(vec![
            sized("c", 7),
            sized("a", 7),
            sized("b", 7),
            sized("d", 7),
        ]);

        assert_eq!(
            sorted_names(entries, &keys, SortMode::Size),
            ["a", "b", "c", "d"]
        );
    }

    #[test]
    fn equal_times_are_broken_by_name() {
        let (entries, keys) = split(vec![
            (file("c"), EntryKey::new(None, Some(time(5)))),
            (file("a"), EntryKey::new(None, Some(time(5)))),
            (file("b"), EntryKey::new(None, Some(time(5)))),
        ]);

        assert_eq!(
            sorted_names(entries, &keys, SortMode::Modified),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn equal_extensions_are_broken_by_name() {
        let entries = vec![file("c.txt"), file("a.txt"), file("b.txt")];

        assert_eq!(
            sorted_names(entries, &[], SortMode::Extension),
            ["a.txt", "b.txt", "c.txt"]
        );
    }

    #[test]
    fn entries_that_share_a_name_are_ordered_by_path() {
        let make = |directory: &str| {
            Entry::new(
                OsString::from("same"),
                PathBuf::from(directory).join("same"),
                EntryKind::File,
            )
        };
        let mut sorted = vec![make("second"), make("first"), make("third")];

        sort_entries(&mut sorted, &mut [], SortMode::Name);

        let paths: Vec<PathBuf> = sorted
            .iter()
            .map(|entry| entry.path().to_path_buf())
            .collect();
        assert_eq!(
            paths,
            [
                PathBuf::from("first").join("same"),
                PathBuf::from("second").join("same"),
                PathBuf::from("third").join("same"),
            ],
            "the path decides when the names are equal"
        );
    }

    #[test]
    fn sorting_is_deterministic_whatever_order_the_entries_arrive_in() {
        let entries = vec![
            directory("dir"),
            file("zeta.txt"),
            file("alpha"),
            file("beta.rs"),
        ];
        let keys = vec![
            EntryKey::new(None, Some(time(3))),
            EntryKey::new(Some(30), Some(time(2))),
            EntryKey::new(Some(10), None),
            EntryKey::unknown(),
        ];

        for mode in SortMode::ALL {
            let expected = sorted_names(entries.clone(), &keys, mode);

            for rotation in 0..entries.len() {
                let mut rotated = entries.clone();
                rotated.rotate_left(rotation);
                let mut rotated_keys = keys.clone();
                rotated_keys.rotate_left(rotation);

                let sorted = sorted_names(rotated, &rotated_keys, mode);
                assert_eq!(sorted, expected, "{mode:?} after rotating by {rotation}");
            }
        }
    }

    #[test]
    fn sorting_is_the_same_when_it_is_repeated() {
        let (entries, keys) = split(vec![
            (file("b.txt"), EntryKey::new(Some(2), Some(time(2)))),
            (directory("a-dir"), EntryKey::unknown()),
            (file("a.txt"), EntryKey::new(Some(1), Some(time(1)))),
        ]);

        for mode in SortMode::ALL {
            let (mut once_entries, mut once_keys) = (entries.clone(), keys.clone());
            sort_entries(&mut once_entries, &mut once_keys, mode);
            let once = names(&once_entries);

            let (mut again_entries, mut again_keys) = (entries.clone(), keys.clone());
            sort_entries(&mut again_entries, &mut again_keys, mode);
            sort_entries(&mut again_entries, &mut again_keys, mode);

            assert_eq!(
                names(&again_entries),
                once,
                "{mode:?} must sort a sorted listing to itself"
            );
            assert_eq!(
                again_keys, once_keys,
                "{mode:?} must leave the keys with their entries"
            );
        }
    }

    #[test]
    fn an_empty_list_sorts_to_an_empty_list() {
        let mut entries: Vec<Entry> = Vec::new();

        sort_entries(&mut entries, &mut [], SortMode::Size);

        assert!(entries.is_empty());
    }

    #[test]
    fn a_single_entry_keeps_its_place() {
        let mut entries = vec![file("only.txt")];

        for mode in SortMode::ALL {
            sort_entries(&mut entries, &mut [], mode);
            assert_eq!(names(&entries), ["only.txt"], "{mode:?}");
        }
    }

    #[test]
    fn sorting_a_mixed_list_keeps_every_entry_exactly_once() {
        let (entries, keys) = split(vec![
            (file("b.txt"), EntryKey::new(Some(1), Some(time(1)))),
            (directory("c-dir"), EntryKey::unknown()),
            (file("a.txt"), EntryKey::new(Some(1), Some(time(1)))),
            (file("d.rs"), EntryKey::new(None, None)),
            (directory("z-dir"), EntryKey::unknown()),
        ]);

        for mode in SortMode::ALL {
            let sorted = sorted_names(entries.clone(), &keys, mode);
            let mut expected = names(&entries);
            expected.sort();
            let mut received = sorted.clone();
            received.sort();

            assert_eq!(received, expected, "{mode:?} must not lose or duplicate");
        }
    }

    #[test]
    fn entries_without_a_key_are_all_unknown() {
        let entries = vec![file("b"), file("a")];

        assert_eq!(
            sorted_names(entries.clone(), &[], SortMode::Size),
            ["a", "b"],
            "without keys, sizes are unknown and the name decides"
        );
        assert_eq!(
            sorted_names(entries.clone(), &[], SortMode::Modified),
            ["a", "b"]
        );
        assert_eq!(sorted_names(entries, &[], SortMode::Extension), ["a", "b"]);
    }

    #[test]
    fn keys_shorter_than_the_entries_leave_the_rest_unknown() {
        let (entries, mut keys) = split(vec![sized("known", 1), sized("missing", 2)]);
        keys.truncate(1);

        assert_eq!(
            sorted_names(entries, &keys, SortMode::Size),
            ["known", "missing"]
        );
    }

    #[test]
    fn keys_are_reordered_with_the_entries_they_describe() {
        let (mut entries, mut keys) = split(vec![sized("a", 2), sized("b", 1)]);

        sort_entries(&mut entries, &mut keys, SortMode::Size);

        assert_eq!(names(&entries), ["b", "a"]);
        assert_eq!(
            keys,
            [EntryKey::new(Some(1), None), EntryKey::new(Some(2), None)],
            "the key of `b` must still follow `b`"
        );
    }

    #[test]
    fn sorting_a_large_listing_repeatedly_is_deterministic_and_stays_linear() {
        let thousand = 1_000;
        let ten_thousand = 10_000;

        // Every entry is a file whose size and time are the reverse of its
        // name, so a mode that uses a key cannot be satisfied by the names, and
        // two entries never compare equal.
        let fixture = |count: usize| {
            let entries: Vec<Entry> = (0..count)
                .map(|index| file(&format!("entry-{index:05}.txt")))
                .collect();
            let keys: Vec<EntryKey> = (0..count)
                .map(|index| {
                    let rank = (count - index) as u64;

                    EntryKey::new(Some(rank), Some(time(rank)))
                })
                .collect();

            (entries, keys)
        };

        let (mut small_entries, mut small_keys) = fixture(thousand);
        let (mut large_entries, mut large_keys) = fixture(ten_thousand);

        // Every mode, three times over: a listing that is sorted again and
        // again must not become slower each time.
        let cycle = |entries: &mut Vec<Entry>, keys: &mut Vec<EntryKey>| {
            for _ in 0..3 {
                for mode in SortMode::ALL {
                    sort_entries(entries, keys, mode);
                }
            }
        };

        let (_, small_time) = measure("sort 1 000 entries in every mode three times", || {
            cycle(&mut small_entries, &mut small_keys);
        });
        let (_, large_time) = measure("sort 10 000 entries in every mode three times", || {
            cycle(&mut large_entries, &mut large_keys);
        });

        assert_not_quadratic(small_time, large_time, "sorting ten times as many entries");

        // Fifteen sorts leave the mode back where it started, so the listing is
        // in name order again: the same answer, however often it is asked for.
        let listed = names(&large_entries);
        let mut by_name = listed.clone();
        by_name.sort();
        assert_eq!(listed, by_name, "the listing is back in name order");

        // A mode that uses a key puts the entries in the order those keys give
        // — smallest first — which is the reverse of the names here, and every
        // key stays with the entry it describes.
        let reversed: Vec<Entry> = fixture(ten_thousand).0.into_iter().rev().collect();
        for mode in [SortMode::Size, SortMode::Modified, SortMode::ReverseName] {
            sort_entries(&mut large_entries, &mut large_keys, mode);

            assert_eq!(
                names(&large_entries),
                names(&reversed),
                "{mode:?} must put the entries in the order the keys give"
            );
            assert_eq!(
                large_keys[0],
                EntryKey::new(Some(1), Some(time(1))),
                "{mode:?} must leave every key with its own entry"
            );
            assert_eq!(
                large_keys[ten_thousand - 1],
                EntryKey::new(Some(ten_thousand as u64), Some(time(ten_thousand as u64))),
                "{mode:?} must end with the entry whose key is the largest"
            );
            assert_eq!(large_keys.len(), ten_thousand, "no entry lost its key");
        }

        // The same question asked twice gets the same answer, entry for entry.
        sort_entries(&mut large_entries, &mut large_keys, SortMode::Extension);
        let once = names(&large_entries);
        sort_entries(&mut large_entries, &mut large_keys, SortMode::Extension);

        assert_eq!(
            names(&large_entries),
            once,
            "the same question, the same answer"
        );
    }

    #[test]
    fn an_ordering_is_applied_in_place_without_copying_the_items() {
        let mut names = vec![
            OsString::from("a"),
            OsString::from("b"),
            OsString::from("c"),
            OsString::from("d"),
        ];
        // "c" first, then "a", then "d", then "b".
        let order = [2, 0, 3, 1];

        apply_ordering(&mut names, &order);

        assert_eq!(
            names,
            [
                OsString::from("c"),
                OsString::from("a"),
                OsString::from("d"),
                OsString::from("b"),
            ]
        );
    }

    #[test]
    fn an_ordering_that_is_already_the_identity_changes_nothing() {
        let mut numbers = vec![0, 1, 2, 3, 4];

        apply_ordering(&mut numbers, &[0, 1, 2, 3, 4]);

        assert_eq!(numbers, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn every_kind_of_cycle_in_an_ordering_is_followed_to_the_end() {
        // One two-cycle, one three-cycle, and no position that stays put:
        // `order[i]` says which of the original values belongs at `i`.
        let mut values = vec![10, 11, 12, 13, 14];
        let order = [1, 0, 4, 2, 3];

        apply_ordering(&mut values, &order);

        assert_eq!(
            values,
            [11, 10, 14, 12, 13],
            "the value at `order[i]` ends up at `i`, for every cycle"
        );
    }

    #[test]
    fn sorting_a_large_listing_is_deterministic_and_stays_in_step_with_its_keys() {
        let count = 5_000;
        let entries: Vec<Entry> = (0..count)
            .map(|index| file(&format!("entry-{index:05}.txt")))
            .collect();
        let keys: Vec<EntryKey> = (0..count)
            .map(|index| EntryKey::new(Some(index as u64), Some(time(index as u64))))
            .collect();

        let mut sorted = entries.clone();
        let mut sorted_keys = keys.clone();
        sort_entries(&mut sorted, &mut sorted_keys, SortMode::Name);

        assert_eq!(
            names(&sorted),
            names(&entries),
            "the names were already in order, so the listing is unchanged"
        );
        assert_eq!(sorted_keys, keys, "and every key is still with its entry");

        // A shuffled copy sorts back to exactly the same listing, and the key
        // that follows each entry is the one that described it.
        let mut shuffled = entries.clone();
        let mut shuffled_keys = keys.clone();
        shuffled.rotate_left(2_501);
        shuffled_keys.rotate_left(2_501);

        sort_entries(&mut shuffled, &mut shuffled_keys, SortMode::Name);

        assert_eq!(names(&shuffled), names(&entries));
        assert_eq!(shuffled_keys, keys);
    }

    #[test]
    fn a_key_can_be_built_from_metadata_that_was_already_read() {
        let directory = TempDir::new("entry-key-metadata");
        let path = directory.path().join("file.txt");
        fs::write(&path, b"content").expect("file should be written");

        let metadata = crate::filesystem::metadata::read_metadata(&path).expect("metadata");
        let key = EntryKey::from_metadata(&metadata);

        assert_eq!(key, EntryKey::new(metadata.size(), metadata.modified()));

        let directory_metadata =
            crate::filesystem::metadata::read_metadata(directory.path()).expect("metadata");
        assert_eq!(
            EntryKey::from_metadata(&directory_metadata),
            EntryKey::new(None, directory_metadata.modified()),
            "a directory has no known size"
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_is_ordered_as_a_link_and_never_as_its_target() {
        use std::os::unix::fs::symlink;

        let directory = TempDir::new("entry-sort-symlink");
        let target = directory.path().join("z-target");
        fs::create_dir(&target).expect("directory should be created");
        let link = directory.path().join("a-link");
        symlink(&target, &link).expect("symlink should be created");

        let link_metadata = fs::symlink_metadata(&link).expect("metadata should be read");
        let link_kind = EntryKind::from_file_type(link_metadata.file_type());
        let entries = vec![
            Entry::new(OsString::from("a-link"), link, link_kind),
            Entry::new(OsString::from("z-target"), target, EntryKind::Directory),
        ];

        for mode in SortMode::ALL {
            let sorted = sorted_names(entries.clone(), &[], mode);
            assert_eq!(
                sorted,
                ["z-target", "a-link"],
                "{mode:?}: the link points at a directory but is not one"
            );
        }
        assert_eq!(link_kind, EntryKind::Symlink);
    }
}
