pub mod classification;
pub mod entry;
pub mod error;
pub mod metadata;
pub mod navigation;
pub mod operations;
pub mod watcher;

pub use self::classification::{
    FileCategory, FileIntelligence, classify_file_by_extension_and_magic,
};
pub use self::watcher::{FilesystemChange, FilesystemWatcher};

use std::io;
use std::path::{Path, PathBuf};

use crate::search::{CancelToken, SearchMode};

use self::entry::Entry;
use self::metadata::{Metadata, MetadataError, read_metadata};
use self::navigation::{DiscoveryError, TreeSearch, read_directory, search_tree};
use self::operations::{
    OperationError, copy, create_directory, create_file, delete, move_item, rename,
};

/// Read-only access to the local filesystem.
///
/// The service is the boundary between the application layer and the operating
/// system: application code calls these methods instead of reaching for
/// `std::fs`, so the standard library details stay inside this module.
///
/// Reading is offered, together with the changes the application needs:
/// creating an entry, renaming one, copying one, moving one and deleting one.
/// Nothing here overwrites what is already on disk, and nothing follows a
/// symbolic link as though it were the thing it points at.
///
/// The service holds no state and depends on nothing but the standard library,
/// so it can be used and tested without a terminal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FilesystemService;

impl FilesystemService {
    /// Creates the service.
    pub fn new() -> Self {
        Self
    }

    /// Lists the immediate children of `directory`, sorted by name.
    ///
    /// The listing is not recursive, hidden entries are included, symbolic
    /// links are reported as links rather than as their targets, and nothing is
    /// opened or read. See [`navigation::read_directory`] for the full
    /// behaviour, including which failures are reported.
    pub fn list_directory(&self, directory: &Path) -> Result<Vec<Entry>, DiscoveryError> {
        read_directory(directory)
    }

    /// Searches `root` and everything below it for names matching `query`.
    ///
    /// The walk never follows a symbolic link, so a link's target is not
    /// searched and a loop of links cannot form; it compares names only, so no
    /// file is opened and no metadata beyond the kind a directory entry already
    /// reports is read; and it stops when `cancel` is cancelled, reporting
    /// [`navigation::SearchOutcome::Cancelled`] rather than a finished search.
    /// Directories that cannot be read are counted and skipped, while a root
    /// that cannot be read is reported. See [`navigation::search_tree`] for the
    /// full behaviour.
    pub fn search(
        &self,
        root: &Path,
        query: &str,
        mode: SearchMode,
        cancel: &CancelToken,
    ) -> TreeSearch {
        search_tree(root, query, mode, cancel)
    }

    /// Reads the metadata of `path`.
    ///
    /// The path itself is described, so a symbolic link is never reported as
    /// its target. See [`metadata::read_metadata`] for the full behaviour,
    /// including how a link's target is represented.
    pub fn metadata(&self, path: &Path) -> Result<Metadata, MetadataError> {
        read_metadata(path)
    }

    /// Creates an empty file at `path`.
    ///
    /// An entry that is already there is reported as an error rather than
    /// replaced, and missing parent directories are not created. See
    /// [`operations::create_file`] for the full behaviour.
    pub fn create_file(&self, path: &Path) -> Result<(), OperationError> {
        create_file(path)
    }

    /// Creates a single directory at `path`.
    ///
    /// Only one level is created, and an entry that is already there is
    /// reported as an error. See [`operations::create_directory`].
    pub fn create_directory(&self, path: &Path) -> Result<(), OperationError> {
        create_directory(path)
    }

    /// Renames `source` to `destination`.
    ///
    /// The entry keeps its type and contents, and a destination that is already
    /// taken is reported as an error rather than replaced. See
    /// [`operations::rename`].
    pub fn rename(&self, source: &Path, destination: &Path) -> Result<(), OperationError> {
        rename(source, destination)
    }

    /// Copies `source` to `destination`.
    ///
    /// A directory is copied with everything under it, a symbolic link is
    /// copied as a link, and a destination that is already taken is reported as
    /// an error rather than replaced. See [`operations::copy`].
    pub fn copy(&self, source: &Path, destination: &Path) -> Result<(), OperationError> {
        copy(source, destination)
    }

    /// Moves `source` to `destination`, using the filesystem's own move.
    ///
    /// The entry keeps its type and contents, and a destination that is already
    /// taken — including the source's own path — is reported as an error. See
    /// [`operations::move_item`].
    pub fn move_item(&self, source: &Path, destination: &Path) -> Result<(), OperationError> {
        move_item(source, destination)
    }

    /// Deletes the entry at `path`.
    ///
    /// A file and a symbolic link are removed as themselves, a symbolic link is
    /// never followed, and a directory is emptied from the bottom up and then
    /// removed, so a directory goes together with everything under it. The root
    /// directory and the working directory of the process are refused. See
    /// [`operations::delete`] for the full behaviour.
    pub fn delete(&self, path: &Path) -> Result<(), OperationError> {
        delete(path)
    }

    /// The directory the process is running in.
    ///
    /// Nothing is assumed about where that is: the answer comes from the
    /// operating system, and a directory that has since been removed is
    /// reported as an error rather than guessed at.
    pub fn current_directory(&self) -> io::Result<PathBuf> {
        std::env::current_dir()
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    /// Creates `count` empty files called `prefix-00000.txt` and upwards.
    ///
    /// The names are padded so that the order the filesystem happens to
    /// enumerate them in is visibly not the order they sort in.
    pub(crate) fn fill(directory: &Path, count: usize, prefix: &str) {
        for index in 0..count {
            let path = directory.join(format!("{prefix}-{index:05}.txt"));
            fs::File::create(&path).expect("file should be created");
        }
    }

    /// Runs `operation` and reports how long it took, with the value it
    /// produced.
    ///
    /// The report is for information: a test that measures something says what
    /// it measured, and is not made to fail by a machine that is busy or slow.
    pub(crate) fn measure<T>(what: &str, operation: impl FnOnce() -> T) -> (T, Duration) {
        let started = Instant::now();
        let value = operation();
        let elapsed = started.elapsed();

        println!("{what}: {elapsed:?}");

        (value, elapsed)
    }

    /// Checks that work which grew tenfold did not grow quadratically.
    ///
    /// A quadratic implementation needs about a hundred times as long for ten
    /// times the input, so this allowance — forty times the smaller measurement
    /// plus a fixed slack for measurements too small to time well — catches that
    /// while leaving a slower machine alone, because both measurements come
    /// from the same machine in the same test.
    pub(crate) fn assert_not_quadratic(small: Duration, large: Duration, context: &str) {
        let budget = small * 40 + Duration::from_millis(50);

        assert!(
            large <= budget,
            "{context}: ten times the input took {large:?} against {small:?}, which is more than the {budget:?} allowed"
        );
    }

    /// A temporary directory that is removed when the test ends.
    pub(crate) struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        pub(crate) fn new(label: &str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);

            let count = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default();
            let path = env::temp_dir().join(format!(
                "terminalvision-test-{}-{nanos}-{count}-{label}",
                std::process::id()
            ));

            fs::create_dir_all(&path).expect("temporary directory should be created");

            Self { path }
        }

        pub(crate) fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FilesystemService;
    use crate::filesystem::entry::{Entry, EntryKind};
    use crate::filesystem::test_support::TempDir;
    use std::error::Error;
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::io;
    use std::path::Path;

    const CONTENTS: &[u8] = b"service content";

    fn names(entries: &[Entry]) -> Vec<OsString> {
        entries
            .iter()
            .map(|entry| entry.name().to_os_string())
            .collect()
    }

    /// The kind of the I/O error a filesystem error wraps.
    fn io_kind(error: &dyn Error) -> io::ErrorKind {
        error
            .source()
            .and_then(|source| source.downcast_ref::<io::Error>())
            .expect("a filesystem error should wrap an I/O error")
            .kind()
    }

    #[test]
    fn service_lists_an_empty_directory() {
        let directory = TempDir::new("service-empty");
        let service = FilesystemService::new();

        let entries = service
            .list_directory(directory.path())
            .expect("listing should succeed");

        assert!(entries.is_empty());
    }

    #[test]
    fn service_lists_files_and_directories_in_name_order() {
        let directory = TempDir::new("service-contents");
        fs::create_dir(directory.path().join("zeta")).expect("directory should be created");
        for name in ["café-日本語.txt", "hello world.txt", "alpha.txt"] {
            fs::write(directory.path().join(name), CONTENTS).expect("file should be written");
        }
        let service = FilesystemService::new();

        let entries = service
            .list_directory(directory.path())
            .expect("listing should succeed");

        assert_eq!(
            names(&entries),
            ["alpha.txt", "café-日本語.txt", "hello world.txt", "zeta"].map(OsString::from)
        );
        assert_eq!(entries[3].kind(), EntryKind::Directory);
        assert!(entries[0].is_file());
    }

    #[test]
    fn service_reads_regular_file_metadata() {
        let directory = TempDir::new("service-file-metadata");
        let file = directory.path().join("file.txt");
        fs::write(&file, CONTENTS).expect("file should be written");
        let service = FilesystemService::new();

        let metadata = service.metadata(&file).expect("metadata should be read");

        assert_eq!(metadata.kind(), EntryKind::File);
        assert_eq!(metadata.size(), Some(CONTENTS.len() as u64));
        assert!(metadata.modified().is_some());
    }

    #[test]
    fn service_reads_directory_metadata() {
        let directory = TempDir::new("service-directory-metadata");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        fs::write(child.join("inside.txt"), CONTENTS).expect("file should be written");
        let service = FilesystemService::new();

        let metadata = service.metadata(&child).expect("metadata should be read");

        assert_eq!(metadata.kind(), EntryKind::Directory);
        assert_eq!(
            metadata.size(),
            None,
            "a directory must not report a recursive size"
        );
    }

    #[test]
    fn service_reports_a_missing_path_when_listing() {
        let path = TempDir::new("service-missing-listing")
            .path()
            .join("missing");
        let service = FilesystemService::new();

        let error = service
            .list_directory(&path)
            .expect_err("listing should fail");

        assert_eq!(error.path(), path);
        assert_eq!(io_kind(&error), io::ErrorKind::NotFound);
    }

    #[test]
    fn service_reports_a_missing_path_when_reading_metadata() {
        let path = TempDir::new("service-missing-metadata")
            .path()
            .join("missing");
        let service = FilesystemService::new();

        let error = service.metadata(&path).expect_err("metadata should fail");

        assert_eq!(error.path(), path);
        assert_eq!(io_kind(&error), io::ErrorKind::NotFound);
    }

    #[test]
    fn service_reports_listing_a_path_that_is_not_a_directory() {
        let directory = TempDir::new("service-not-a-directory");
        let file = directory.path().join("file.txt");
        fs::write(&file, CONTENTS).expect("file should be written");
        let service = FilesystemService::new();

        let error = service
            .list_directory(&file)
            .expect_err("listing should fail");

        assert_eq!(error.path(), file);
    }

    #[test]
    fn service_metadata_describes_the_entry_itself() {
        let directory = TempDir::new("service-symlink");
        let target = directory.path().join("target.txt");
        fs::write(&target, CONTENTS).expect("file should be written");
        let link = directory.path().join("link");
        let service = FilesystemService::new();

        // Symbolic links cannot be created reliably on Windows without
        // elevation, so this test only checks a link where one can be made.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &link).expect("symlink should be created");

            let entries = service
                .list_directory(directory.path())
                .expect("listing should succeed");
            let entry = entries
                .iter()
                .find(|entry| entry.name() == OsStr::new("link"))
                .expect("the symlink should be discovered");
            let metadata = service.metadata(&link).expect("metadata should be read");

            assert_eq!(entry.kind(), EntryKind::Symlink);
            assert_eq!(metadata.kind(), EntryKind::Symlink);
            assert_eq!(metadata.kind(), entry.kind());
        }
    }

    #[test]
    fn test_broken_symlink_metadata_handling() {
        #[cfg(unix)]
        {
            let directory = TempDir::new("broken-symlink");
            let target = directory.path().join("does_not_exist.txt");
            let link = directory.path().join("broken_link");
            let service = FilesystemService::new();

            std::os::unix::fs::symlink(&target, &link).expect("broken symlink should be created");

            let metadata = service
                .metadata(&link)
                .expect("metadata should read broken link itself");
            assert_eq!(metadata.kind(), EntryKind::Symlink);
            assert!(matches!(
                metadata.target(),
                crate::filesystem::metadata::LinkTarget::Unavailable { .. }
            ));
        }
    }

    #[test]
    fn test_unicode_and_spaces_directory_listing_and_metadata() {
        let directory = TempDir::new("unicode-spaces-test");
        let unicode_dir = directory.path().join("📁 Folder with Spaces 🦀");
        fs::create_dir_all(&unicode_dir).expect("create unicode dir");

        let unicode_file = unicode_dir.join("日本語 ファイル.txt");
        fs::write(&unicode_file, b"unicode content").expect("write unicode file");

        let service = FilesystemService::new();
        let entries = service
            .list_directory(&unicode_dir)
            .expect("list unicode dir");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name(), OsStr::new("日本語 ファイル.txt"));

        let meta = service
            .metadata(&unicode_file)
            .expect("read unicode metadata");
        assert_eq!(meta.kind(), EntryKind::File);
        assert_eq!(meta.size(), Some(15));
    }

    #[test]
    fn test_root_path_navigation_and_parent_handling() {
        use crate::filesystem::navigation::parent_of;
        use std::path::Path;

        assert_eq!(parent_of(Path::new("/")), None);
        assert_eq!(parent_of(Path::new("")), None);

        let service = FilesystemService::new();
        let root_listing = service.list_directory(Path::new("/"));
        assert!(root_listing.is_ok(), "root directory should be readable");
    }

    #[test]
    fn test_linux_style_deep_paths_and_traversal() {
        let temp = TempDir::new("linux-deep-paths");
        let deep = temp
            .path()
            .join("usr")
            .join("local")
            .join("share")
            .join("app");
        fs::create_dir_all(&deep).expect("create deep dirs");

        let conf = deep.join("app.conf");
        fs::write(&conf, b"linux configuration").expect("write conf");

        let service = FilesystemService::new();
        let meta = service.metadata(&conf).expect("read metadata");
        assert_eq!(meta.kind(), EntryKind::File);
        assert_eq!(meta.size(), Some(19));

        let parent = crate::filesystem::navigation::parent_of(&conf);
        assert_eq!(parent, Some(deep));
    }

    #[test]
    fn test_inaccessible_or_missing_path_error_category() {
        let service = FilesystemService::new();
        let missing = Path::new("/path/that/does/not/exist/99999");
        let err = service.list_directory(missing).expect_err("should fail");
        assert_eq!(
            err.category(),
            crate::filesystem::error::ErrorCategory::NotFound
        );
    }

    #[test]
    fn test_windows_style_paths_with_spaces_and_unicode() {
        let temp = TempDir::new("win-paths-test");
        let sub = temp
            .path()
            .join("Program Files")
            .join("App Data 🚀")
            .join("Config");
        fs::create_dir_all(&sub).expect("create nested directory with spaces and unicode");

        let conf = sub.join("settings.tv");
        fs::write(&conf, b"windows config content").expect("write settings");

        let service = FilesystemService::new();
        let meta = service.metadata(&conf).expect("read metadata");
        assert_eq!(meta.kind(), EntryKind::File);
        assert_eq!(meta.size(), Some(22));

        let parent = crate::filesystem::navigation::parent_of(&conf);
        assert_eq!(parent, Some(sub));
    }

    #[test]
    fn test_windows_file_operations_lifecycle() {
        let temp = TempDir::new("win-ops-lifecycle");
        let base = temp.path();

        // 1. Create directory
        let docs = base.join("My Documents");
        fs::create_dir(&docs).expect("create dir");

        // 2. Create file
        let doc_file = docs.join("Notes 📝.txt");
        fs::write(&doc_file, b"sample notes").expect("create file");

        // 3. Rename file
        let renamed_file = docs.join("Renamed Notes 📝.txt");
        fs::rename(&doc_file, &renamed_file).expect("rename file");
        assert!(renamed_file.exists());
        assert!(!doc_file.exists());

        // 4. Delete file
        fs::remove_file(&renamed_file).expect("remove file");
        assert!(!renamed_file.exists());

        // 5. Delete directory
        fs::remove_dir(&docs).expect("remove directory");
        assert!(!docs.exists());
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_drive_root_parent() {
        let c_drive = Path::new("C:\\");
        assert_eq!(crate::filesystem::navigation::parent_of(c_drive), None);

        let c_users = Path::new("C:\\Users");
        assert_eq!(
            crate::filesystem::navigation::parent_of(c_users),
            Some(PathBuf::from("C:\\"))
        );
    }
}
