use std::error::Error;
use std::fmt;
use std::fs::{self, Metadata as PlatformMetadata};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::entry::{Entry, EntryKind};
use super::error::ErrorCategory;

/// The timestamps the platform records for one entry.
///
/// A platform or filesystem that does not record a timestamp reports `None`
/// instead of failing the whole read.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Timestamps {
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    accessed: Option<SystemTime>,
}

impl Timestamps {
    /// Collects the timestamps of platform metadata, ignoring those the
    /// platform does not record.
    fn of(metadata: &PlatformMetadata) -> Self {
        Self {
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            accessed: metadata.accessed().ok(),
        }
    }
}

/// What a symbolic link points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// The entry is not a symbolic link, so it has no target.
    NotApplicable,
    /// The entry is a symbolic link whose target could not be read, such as a
    /// broken link, a link loop or a denied permission.
    Unavailable {
        /// Why the target could not be read.
        reason: io::ErrorKind,
    },
    /// The entry is a symbolic link whose target was read.
    ///
    /// The operating system follows the link, so a link to a link resolves to
    /// the final target and the target never carries a target of its own.
    Resolved(Box<Metadata>),
}

/// What the filesystem reports about one entry.
///
/// The values describe the entry itself. Nothing here is formatted for
/// display: timestamps stay `SystemTime` and sizes stay bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    kind: EntryKind,
    size: Option<u64>,
    times: Timestamps,
    target: LinkTarget,
}

impl Metadata {
    /// The kind of the entry itself. A symbolic link is never reported as the
    /// kind of its target.
    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    /// The size in bytes as the filesystem reports it.
    ///
    /// A directory has no size here, because a meaningful one would require
    /// visiting its contents, which is never done.
    pub fn size(&self) -> Option<u64> {
        self.size
    }

    /// The time of the last modification, when the platform records it.
    pub fn modified(&self) -> Option<SystemTime> {
        self.times.modified
    }

    /// The time of creation, when the platform and filesystem record it.
    pub fn created(&self) -> Option<SystemTime> {
        self.times.created
    }

    /// The time of the last access, when the platform records it.
    pub fn accessed(&self) -> Option<SystemTime> {
        self.times.accessed
    }

    /// What the entry points at, if it is a symbolic link.
    pub fn target(&self) -> &LinkTarget {
        &self.target
    }
}

/// A failure to read the metadata of a path.
#[derive(Debug)]
pub struct MetadataError {
    path: PathBuf,
    source: io::Error,
}

impl MetadataError {
    fn new(path: &Path, source: io::Error) -> Self {
        Self {
            path: path.to_path_buf(),
            source,
        }
    }

    /// The path whose metadata could not be read.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Why the metadata could not be read, as a kind a caller can act on.
    pub fn category(&self) -> ErrorCategory {
        ErrorCategory::of(self.source.kind())
    }
}

impl fmt::Display for MetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "failed to read metadata of `{}`: {}",
            self.path.display(),
            self.source
        )
    }
}

impl Error for MetadataError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

/// Reads the metadata of `path`.
///
/// The entry is described by its own metadata, so a symbolic link is never
/// silently reported as its target. For a symbolic link the target is read
/// separately: a resolvable target is described by [`LinkTarget::Resolved`],
/// and a target that cannot be read, such as a broken link, is reported as
/// [`LinkTarget::Unavailable`] while the link's own metadata is still returned.
///
/// Nothing is opened or read, directory contents are not visited, and no sizes
/// are computed. Failures are returned instead of panicking, including a
/// missing path, a deleted entry, a denied permission and an invalid path.
pub fn read_metadata(path: &Path) -> Result<Metadata, MetadataError> {
    let own = fs::symlink_metadata(path).map_err(|source| MetadataError::new(path, source))?;
    let mut metadata = Metadata::of(&own);

    if metadata.kind == EntryKind::Symlink {
        metadata.target = match fs::metadata(path) {
            Ok(target) => LinkTarget::Resolved(Box::new(Metadata::of(&target))),
            Err(error) => LinkTarget::Unavailable {
                reason: error.kind(),
            },
        };
    }

    Ok(metadata)
}

impl Metadata {
    /// Builds metadata from platform metadata.
    ///
    /// One constructor serves both an entry and a link target, so their
    /// attributes are always collected in the same way.
    fn of(metadata: &PlatformMetadata) -> Self {
        let kind = EntryKind::from_file_type(metadata.file_type());

        Self {
            kind,
            size: size_of(kind, metadata),
            times: Timestamps::of(metadata),
            target: LinkTarget::NotApplicable,
        }
    }
}

/// Reports the size of an entry, or `None` where no meaningful size exists.
///
/// Only directories are exempt: their size would require visiting their
/// contents. Every other kind reports the value the filesystem records, which
/// for a symbolic link is the size of the link itself and not of its target.
fn size_of(kind: EntryKind, metadata: &PlatformMetadata) -> Option<u64> {
    match kind {
        EntryKind::Directory => None,
        _ => Some(metadata.len()),
    }
}

// Defined here rather than in `entry`, so that the entry model stays free of
// metadata retrieval and the modules keep a single direction of dependency.
impl Entry {
    /// Reads the metadata of this entry, without following it if it is a
    /// symbolic link.
    pub fn metadata(&self) -> Result<Metadata, MetadataError> {
        read_metadata(self.path())
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorCategory, LinkTarget, MetadataError, read_metadata};
    use crate::filesystem::entry::EntryKind;
    use crate::filesystem::test_support::TempDir;
    use std::fs;
    use std::io;
    use std::path::Path;
    use std::time::UNIX_EPOCH;

    const CONTENTS: &[u8] = b"hello world";

    fn io_error(error: &MetadataError) -> &io::Error {
        std::error::Error::source(error)
            .and_then(|source| source.downcast_ref::<io::Error>())
            .expect("metadata error should wrap an I/O error")
    }

    #[test]
    fn regular_file_reports_kind_file() {
        let directory = TempDir::new("metadata-file");
        let file = directory.path().join("file.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        let metadata = read_metadata(&file).expect("metadata should be read");

        assert_eq!(metadata.kind(), EntryKind::File);
    }

    #[test]
    fn regular_file_reports_its_size() {
        let directory = TempDir::new("metadata-size");
        let file = directory.path().join("file.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        let metadata = read_metadata(&file).expect("metadata should be read");

        assert_eq!(metadata.size(), Some(CONTENTS.len() as u64));
    }

    #[test]
    fn directory_reports_kind_directory() {
        let directory = TempDir::new("metadata-directory");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");

        let metadata = read_metadata(&child).expect("metadata should be read");

        assert_eq!(metadata.kind(), EntryKind::Directory);
    }

    #[test]
    fn directory_size_is_not_calculated_recursively() {
        let directory = TempDir::new("metadata-directory-size");
        let child = directory.path().join("child");
        let nested = child.join("nested");
        fs::create_dir_all(&nested).expect("directories should be created");
        fs::write(nested.join("file.txt"), CONTENTS).expect("file should be written");

        let metadata = read_metadata(&child).expect("metadata should be read");

        assert_eq!(
            metadata.size(),
            None,
            "a directory must not report a recursive size"
        );
    }

    #[test]
    fn modification_time_is_reported_for_a_regular_file() {
        let directory = TempDir::new("metadata-modified");
        let file = directory.path().join("file.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        let metadata = read_metadata(&file).expect("metadata should be read");

        let modified = metadata
            .modified()
            .expect("a modification time should be recorded");
        assert!(modified.duration_since(UNIX_EPOCH).is_ok());
    }

    #[test]
    fn created_time_is_represented_or_absent() {
        let directory = TempDir::new("metadata-created");
        let file = directory.path().join("file.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        let metadata = read_metadata(&file).expect("metadata should be read");

        // Creation time is not recorded by every platform or filesystem, so an
        // absent value is acceptable and must not fail the read.
        if let Some(created) = metadata.created() {
            assert!(created.duration_since(UNIX_EPOCH).is_ok());
        }
    }

    #[test]
    fn accessed_time_is_represented_or_absent() {
        let directory = TempDir::new("metadata-accessed");
        let file = directory.path().join("file.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        let metadata = read_metadata(&file).expect("metadata should be read");

        // Access time may be unavailable or unmounted without being an error.
        if let Some(accessed) = metadata.accessed() {
            assert!(accessed.duration_since(UNIX_EPOCH).is_ok());
        }
    }

    #[test]
    fn unicode_file_name_metadata_is_read() {
        let directory = TempDir::new("metadata-unicode");
        let file = directory.path().join("café-日本語.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        let metadata = read_metadata(&file).expect("metadata should be read");

        assert_eq!(metadata.kind(), EntryKind::File);
        assert_eq!(metadata.size(), Some(CONTENTS.len() as u64));
    }

    #[test]
    fn file_name_with_spaces_metadata_is_read() {
        let directory = TempDir::new("metadata-spaces");
        let file = directory.path().join("hello world.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        let metadata = read_metadata(&file).expect("metadata should be read");

        assert_eq!(metadata.kind(), EntryKind::File);
        assert_eq!(metadata.size(), Some(CONTENTS.len() as u64));
    }

    #[test]
    fn missing_path_is_an_error() {
        let path = TempDir::new("metadata-missing").path().join("missing.txt");

        let error = read_metadata(&path).expect_err("metadata should fail");

        assert_eq!(error.path(), path);
        assert_eq!(io_error(&error).kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn invalid_path_is_an_error() {
        let invalid = Path::new("invalid\0path");

        let error = read_metadata(invalid).expect_err("metadata should fail");

        assert_eq!(error.path(), invalid);
    }

    #[test]
    fn a_missing_path_is_categorized_as_not_found() {
        let directory = TempDir::new("metadata-missing-category");
        let missing = directory.path().join("nowhere.txt");

        let error = read_metadata(&missing).expect_err("there is nothing to describe");

        assert_eq!(error.category(), ErrorCategory::NotFound);
        assert!(!error.category().is_conflict());
        assert_eq!(error.path(), missing.as_path());
    }

    #[test]
    fn a_regular_file_has_no_link_target() {
        let directory = TempDir::new("metadata-target-not-applicable");
        let file = directory.path().join("file.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        let metadata = read_metadata(&file).expect("metadata should be read");

        assert_eq!(metadata.target(), &LinkTarget::NotApplicable);
    }

    #[test]
    fn a_discovered_entry_reads_the_same_kind_as_its_metadata() {
        use crate::filesystem::navigation::read_directory;

        let directory = TempDir::new("metadata-entry-integration");
        fs::write(directory.path().join("file.txt"), CONTENTS).expect("file should be written");
        fs::create_dir(directory.path().join("child")).expect("directory should be created");

        let entries = read_directory(directory.path()).expect("discovery should succeed");
        assert_eq!(entries.len(), 2);

        for entry in &entries {
            let metadata = entry.metadata().expect("metadata should be read");
            assert_eq!(entry.kind(), metadata.kind());
        }
    }

    // Permission handling needs a non-root user, so it is Unix-only, and it is
    // skipped when the process can read the directory anyway.
    #[cfg(unix)]
    mod permissions {
        use super::io_error;
        use crate::filesystem::test_support::TempDir;
        use std::fs;
        use std::io;
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

        #[test]
        fn permission_denied_is_reported() {
            let directory = TempDir::new("metadata-permission");
            let denied = directory.path().join("denied");
            fs::create_dir(&denied).expect("directory should be created");
            let file = denied.join("file.txt");
            fs::write(&file, b"content").expect("file should be written");

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

            match super::read_metadata(&file) {
                // Elevated privileges, such as running as root, ignore the mode.
                Ok(_) => {}
                Err(error) => {
                    assert_eq!(error.path(), file);
                    assert_eq!(io_error(&error).kind(), io::ErrorKind::PermissionDenied);
                }
            }
        }
    }

    // Symbolic links can be created without elevation on Unix, but not reliably
    // on Windows, so the symlink tests run on Unix only.
    #[cfg(unix)]
    mod symlinks {
        use super::{CONTENTS, LinkTarget, read_metadata};
        use crate::filesystem::entry::EntryKind;
        use crate::filesystem::test_support::TempDir;
        use std::fs;
        use std::io;
        use std::os::unix::fs::symlink;

        /// Unwraps the metadata of a resolvable link target.
        fn resolved<'a>(target: &'a LinkTarget, expectation: &str) -> &'a super::super::Metadata {
            match target {
                LinkTarget::Resolved(metadata) => metadata,
                other => panic!("{expectation}, found {other:?}"),
            }
        }

        #[test]
        fn symlink_is_identified_as_a_symlink_and_not_as_its_target() {
            let directory = TempDir::new("metadata-symlink-file");
            let target = directory.path().join("target.txt");
            fs::write(&target, CONTENTS).expect("file should be written");
            let link = directory.path().join("link");
            symlink("target.txt", &link).expect("symlink should be created");

            let metadata = read_metadata(&link).expect("metadata should be read");

            assert_eq!(metadata.kind(), EntryKind::Symlink);
            // The link's own size is the length of its target path, which is
            // not the size of the file it points at.
            assert_eq!(metadata.size(), Some("target.txt".len() as u64));
        }

        #[test]
        fn symlink_to_a_file_reports_the_target_metadata() {
            let directory = TempDir::new("metadata-symlink-target-file");
            let target = directory.path().join("target.txt");
            fs::write(&target, CONTENTS).expect("file should be written");
            let link = directory.path().join("link");
            symlink(&target, &link).expect("symlink should be created");

            let metadata = read_metadata(&link).expect("metadata should be read");
            let target_metadata = resolved(metadata.target(), "a file target should resolve");

            assert_eq!(target_metadata.kind(), EntryKind::File);
            assert_eq!(target_metadata.size(), Some(CONTENTS.len() as u64));
            assert!(target_metadata.modified().is_some());
        }

        #[test]
        fn symlink_to_a_directory_reports_the_target_kind_without_a_size() {
            let directory = TempDir::new("metadata-symlink-target-directory");
            let target = directory.path().join("target");
            fs::create_dir(&target).expect("directory should be created");
            fs::write(target.join("inside.txt"), CONTENTS).expect("file should be written");
            let link = directory.path().join("link");
            symlink(&target, &link).expect("symlink should be created");

            let metadata = read_metadata(&link).expect("metadata should be read");
            let target_metadata = resolved(metadata.target(), "a directory target should resolve");

            assert_eq!(metadata.kind(), EntryKind::Symlink);
            assert_eq!(target_metadata.kind(), EntryKind::Directory);
            assert_eq!(
                target_metadata.size(),
                None,
                "a directory target must not report a recursive size"
            );
        }

        #[test]
        fn broken_symlink_reports_an_unavailable_target() {
            let directory = TempDir::new("metadata-symlink-broken");
            let link = directory.path().join("dangling");
            symlink("missing", &link).expect("symlink should be created");

            let metadata = read_metadata(&link).expect("metadata should be read");

            assert_eq!(metadata.kind(), EntryKind::Symlink);
            assert_eq!(metadata.size(), Some("missing".len() as u64));
            assert!(
                matches!(
                    metadata.target(),
                    LinkTarget::Unavailable {
                        reason: io::ErrorKind::NotFound
                    }
                ),
                "a broken link should report an unavailable target, found {:?}",
                metadata.target()
            );
        }

        #[test]
        fn a_link_loop_reports_an_unavailable_target() {
            let directory = TempDir::new("metadata-symlink-loop");
            let first = directory.path().join("first");
            let second = directory.path().join("second");
            symlink(&second, &first).expect("symlink should be created");
            symlink(&first, &second).expect("symlink should be created");

            let metadata = read_metadata(&first).expect("metadata of the link should be read");

            assert_eq!(metadata.kind(), EntryKind::Symlink);
            assert!(
                matches!(metadata.target(), LinkTarget::Unavailable { .. }),
                "a link loop should report an unavailable target, found {:?}",
                metadata.target()
            );
        }
    }
}
