//! Changing the filesystem: creating entries, copying them, moving them,
//! renaming them and deleting them.
//!
//! These are the only functions in the project that change anything on disk.
//! Every one of them is built from standard-library calls, and a failure is
//! never reported as a success.
//!
//! Nothing here overwrites: an entry that is already there is reported as an
//! error rather than replaced or truncated. Nothing here undoes what it has
//! already done when it fails part way through. Creating a file or a directory
//! stays one level deep, and a file is created only where its directory already
//! exists; copying and deleting a directory are the two things that go further,
//! because they have to reach everything under it.

use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use super::error::ErrorCategory;

/// What a change to the filesystem was trying to do.
///
/// The operation is kept with the failure so that a caller can tell which one
/// went wrong, and so that a rename can name both of its paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// Creating a file.
    CreateFile,
    /// Creating a directory.
    CreateDirectory,
    /// Renaming the entry at this path.
    Rename(PathBuf),
    /// Copying the entry at this path.
    Copy(PathBuf),
    /// Moving the entry at this path.
    Move(PathBuf),
    /// Deleting the entry at this path.
    Delete(PathBuf),
}

/// How an operation on the filesystem ended.
///
/// A whole operation either happens or it does not, and this is the small,
/// closed set of reasons why it might not. It exists so that a caller can
/// decide what to tell the user, and what a later layer should show for it,
/// without reading a message and without matching on the operating system's own
/// error kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationOutcome {
    /// The operation happened.
    Done,
    /// The destination is already taken, so nothing was replaced.
    DestinationExists,
    /// The entry the operation started from is not there.
    SourceMissing,
    /// The directory the destination would be created in is not there.
    ParentMissing,
    /// The operating system refused the operation for this user.
    PermissionDenied,
    /// The path cannot be used for this operation.
    InvalidPath,
    /// What was asked for clashes with what is already there, or with its own
    /// paths, such as a directory that was to be copied into itself.
    Conflict,
    /// The filesystem cannot be written to.
    ReadOnly,
    /// The work was stopped rather than finishing.
    Cancelled,
    /// The platform does not offer what was asked for.
    Unsupported,
    /// Anything else the operating system reported.
    Failed,
}

/// A failed change to the filesystem.
///
/// The operation, the path it concerned and the operating system's own error
/// are all kept, so a caller can tell a name that is already taken from a
/// directory that is not writable without reading a message.
#[derive(Debug)]
pub struct OperationError {
    operation: Operation,
    path: PathBuf,
    about_the_source: bool,
    category: ErrorCategory,
    source: io::Error,
}

impl OperationError {
    /// Records that `operation` failed on `path`.
    fn new(operation: Operation, path: &Path, source: io::Error) -> Self {
        let category = ErrorCategory::of(source.kind());

        Self {
            operation,
            path: path.to_path_buf(),
            about_the_source: false,
            category,
            source,
        }
    }

    /// Records that `operation` failed because the entry it started from could
    /// not be used, such as one that is not there.
    ///
    /// `path` means what it means in [`Self::new`]: the path the operation was
    /// asked to produce. When an operation has two paths, an entry that is
    /// missing could be either the one being worked on or the directory a
    /// destination was to be created in, and which of the two it was is only
    /// known where the call was made, so it is recorded there instead of being
    /// worked out from the operating system's error afterwards.
    fn about_source(operation: Operation, path: &Path, source: io::Error) -> Self {
        let category = ErrorCategory::of(source.kind());

        Self {
            operation,
            path: path.to_path_buf(),
            about_the_source: true,
            category,
            source,
        }
    }

    /// Records a request that was refused before the filesystem was asked,
    /// because the path cannot be used for what was asked of it.
    pub(crate) fn rejected(operation: Operation, path: PathBuf, reason: &str) -> Self {
        Self::refused(operation, path, ErrorCategory::InvalidPath, reason)
    }

    /// Records a request that was refused before the filesystem was asked,
    /// because what it asked for clashes with something that is already there
    /// or with its own paths.
    pub(crate) fn conflicting(operation: Operation, path: PathBuf, reason: &str) -> Self {
        Self::refused(operation, path, ErrorCategory::Conflict, reason)
    }

    /// Records a refusal, with the category it belongs to rather than the one
    /// the placeholder operating system error would fall into.
    fn refused(operation: Operation, path: PathBuf, category: ErrorCategory, reason: &str) -> Self {
        let mut error = Self::new(
            operation,
            &path,
            io::Error::new(io::ErrorKind::InvalidInput, reason),
        );
        error.category = category;

        error
    }

    /// What the failed operation was trying to do.
    pub fn operation(&self) -> &Operation {
        &self.operation
    }

    /// The path the operation was asked to produce: the entry being created, or
    /// the new name a renamed entry was to have.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The kind of failure, as the operating system reported it.
    ///
    /// A name that is already taken is [`io::ErrorKind::AlreadyExists`]; a
    /// request that was refused before the filesystem was asked is
    /// [`io::ErrorKind::InvalidInput`].
    pub fn kind(&self) -> io::ErrorKind {
        self.source.kind()
    }

    /// Why the operation failed, as a kind a caller can act on.
    ///
    /// This is the finer answer: a path that is not a directory and a path that
    /// cannot be used at all are told apart here, while [`Self::outcome`]
    /// groups them because a user is shown the same thing for both.
    pub fn category(&self) -> ErrorCategory {
        self.category
    }

    /// How the operation ended, as a kind a caller can act on.
    ///
    /// The kinds are the ones a user is shown differently: a destination that
    /// is already taken, an entry that is not there, a directory that is not
    /// there, a refusal by the operating system, a path that cannot be used,
    /// something the platform does not offer, and everything else.
    pub fn outcome(&self) -> OperationOutcome {
        match self.category {
            ErrorCategory::AlreadyExists => OperationOutcome::DestinationExists,
            ErrorCategory::NotFound if self.about_the_source => OperationOutcome::SourceMissing,
            ErrorCategory::NotFound => OperationOutcome::ParentMissing,
            ErrorCategory::PermissionDenied => OperationOutcome::PermissionDenied,
            ErrorCategory::Conflict => OperationOutcome::Conflict,
            ErrorCategory::ReadOnly => OperationOutcome::ReadOnly,
            ErrorCategory::Cancelled => OperationOutcome::Cancelled,
            ErrorCategory::InvalidPath
            | ErrorCategory::NotDirectory
            | ErrorCategory::IsDirectory => OperationOutcome::InvalidPath,
            ErrorCategory::Unsupported => OperationOutcome::Unsupported,
            ErrorCategory::Io | ErrorCategory::Other => OperationOutcome::Failed,
        }
    }
}

impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.operation {
            Operation::CreateFile => write!(
                f,
                "cannot create the file `{}`: {}",
                self.path.display(),
                self.source
            ),
            Operation::CreateDirectory => write!(
                f,
                "cannot create the directory `{}`: {}",
                self.path.display(),
                self.source
            ),
            Operation::Rename(from) => write!(
                f,
                "cannot rename `{}` to `{}`: {}",
                from.display(),
                self.path.display(),
                self.source
            ),
            Operation::Copy(from) => write!(
                f,
                "cannot copy `{}` to `{}`: {}",
                from.display(),
                self.path.display(),
                self.source
            ),
            Operation::Move(from) => write!(
                f,
                "cannot move `{}` to `{}`: {}",
                from.display(),
                self.path.display(),
                self.source
            ),
            Operation::Delete(_) => write!(
                f,
                "cannot delete `{}`: {}",
                self.path.display(),
                self.source
            ),
        }
    }
}

impl Error for OperationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

/// Creates an empty file at `path`.
///
/// A file that is already there is left untouched and reported as an error, and
/// missing parent directories are not created.
pub fn create_file(path: &Path) -> Result<(), OperationError> {
    // `create_new` is what makes this safe: `File::create` would truncate an
    // existing file, which is exactly what must not happen here. The file is
    // closed as soon as it exists, because nothing is written to it.
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(drop)
        .map_err(|source| OperationError::new(Operation::CreateFile, path, source))
}

/// Creates a single directory at `path`.
///
/// Only one level is created: a missing parent is an error rather than
/// something to create along the way. A file or directory that is already there
/// is reported instead of replaced.
pub fn create_directory(path: &Path) -> Result<(), OperationError> {
    fs::create_dir(path)
        .map_err(|source| OperationError::new(Operation::CreateDirectory, path, source))
}

/// Renames `source` to `destination`.
///
/// The entry keeps what it is: a directory stays a directory together with its
/// contents, a file keeps its contents, and a symbolic link is moved as the
/// link it is. The filesystem's own rename is used, so nothing is copied and
/// nothing is deleted on the way.
///
/// Nothing is overwritten. A destination that is already taken is reported as
/// an error, even though a rename on some platforms would replace it, and
/// renaming an entry to the name it already has asks for nothing, so it
/// succeeds without touching anything.
pub fn rename(source: &Path, destination: &Path) -> Result<(), OperationError> {
    if source == destination {
        return Ok(());
    }

    relocate(
        &Operation::Rename(source.to_path_buf()),
        source,
        destination,
    )
}

/// Moves `source` to `destination`.
///
/// This is the filesystem's own move: nothing is copied and nothing is deleted
/// on the way, so a directory arrives together with its contents, a file keeps
/// its contents and a symbolic link is moved as the link it is.
///
/// A destination that is already taken is reported as an error, which includes
/// putting an entry where it already is. See [`rename`] for the one difference
/// between the two: there, asking for the name an entry already has is not an
/// error.
pub fn move_item(source: &Path, destination: &Path) -> Result<(), OperationError> {
    relocate(&Operation::Move(source.to_path_buf()), source, destination)
}

/// Moves an entry to another name or another directory, replacing nothing.
///
/// The source must be there and the destination must be free; the destination's
/// directory must already exist, because nothing here creates parents.
fn relocate(
    operation: &Operation,
    source: &Path,
    destination: &Path,
) -> Result<(), OperationError> {
    let about_destination =
        |error: io::Error| OperationError::new(operation.clone(), destination, error);
    let about_source =
        |error: io::Error| OperationError::about_source(operation.clone(), destination, error);

    // The source is looked up as an entry in its own right, so a symbolic link
    // is seen even when it points nowhere. Looking at the source first also
    // means an entry that is gone is reported as gone, whatever the destination
    // looks like.
    fs::symlink_metadata(source).map_err(about_source)?;

    // The destination is checked before the move, because a move would
    // otherwise replace what is already there on some platforms. The standard
    // library offers no way to check and move in one step.
    if fs::symlink_metadata(destination).is_ok() {
        return Err(about_destination(destination_is_taken()));
    }

    fs::rename(source, destination).map_err(about_destination)
}

/// The error for a destination that is not free.
fn destination_is_taken() -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        "the destination already exists",
    )
}

/// Copies `source` to `destination`.
///
/// A directory is copied together with everything under it, keeping the shape
/// of the tree; a symbolic link is copied as a link, never followed; and a
/// regular file is copied by the operating system itself, so its contents are
/// never held in memory here.
///
/// The source stays where it is, and nothing is replaced: a destination that is
/// already taken is reported as an error. A copy that fails part way through
/// stops there and leaves what it had already copied in place, because removing
/// it is not something this service does.
pub fn copy(source: &Path, destination: &Path) -> Result<(), OperationError> {
    let about_destination = |error: io::Error| {
        OperationError::new(Operation::Copy(source.to_path_buf()), destination, error)
    };
    let about_source = |error: io::Error| {
        OperationError::about_source(Operation::Copy(source.to_path_buf()), destination, error)
    };

    // The source comes first, so an entry that is gone is reported as gone even
    // when the destination is its own path.
    let metadata = fs::symlink_metadata(source).map_err(about_source)?;

    if fs::symlink_metadata(destination).is_ok() {
        return Err(about_destination(destination_is_taken()));
    }

    // A directory that is copied into itself would be copied forever, so that
    // is refused before anything is created. A file cannot contain anything, so
    // only directories are checked.
    if metadata.is_dir() && is_inside(source, destination).map_err(about_destination)? {
        return Err(OperationError::conflicting(
            Operation::Copy(source.to_path_buf()),
            destination.to_path_buf(),
            "the destination is inside the source",
        ));
    }

    copy_entry(source, destination, &metadata, source)
}

/// Copies one entry, which may be a directory holding more entries.
///
/// `origin` is the entry the whole copy started from, so a failure deep inside
/// a tree can still say what the user asked for.
fn copy_entry(
    source: &Path,
    destination: &Path,
    metadata: &fs::Metadata,
    origin: &Path,
) -> Result<(), OperationError> {
    let failure = |error: io::Error| {
        OperationError::new(Operation::Copy(origin.to_path_buf()), destination, error)
    };

    // A link is copied as the link it is. Following it would copy whatever it
    // points at, and a link to a directory would be walked as though it were
    // part of the tree.
    if metadata.file_type().is_symlink() {
        return copy_link(source, destination).map_err(failure);
    }

    if metadata.is_dir() {
        fs::create_dir(destination).map_err(failure)?;

        // Entries are taken one at a time, so a large directory is never held
        // in memory as a whole.
        for item in fs::read_dir(source).map_err(failure)? {
            let item = item.map_err(failure)?;
            let from = item.path();
            let to = destination.join(item.file_name());
            let metadata = fs::symlink_metadata(&from).map_err(failure)?;

            copy_entry(&from, &to, &metadata, origin)?;
        }

        return Ok(());
    }

    if metadata.is_file() {
        return fs::copy(source, destination).map(drop).map_err(failure);
    }

    Err(failure(io::Error::new(
        io::ErrorKind::Unsupported,
        "only files, directories and symbolic links can be copied",
    )))
}

/// Copies a symbolic link as a link, without looking at what it points at.
#[cfg(unix)]
fn copy_link(source: &Path, destination: &Path) -> Result<(), io::Error> {
    std::os::unix::fs::symlink(fs::read_link(source)?, destination)
}

/// Copies a symbolic link on Windows as either a file or directory link.
#[cfg(windows)]
fn copy_link(source: &Path, destination: &Path) -> Result<(), io::Error> {
    let target = fs::read_link(source)?;
    let is_dir = if target.is_absolute() {
        target.is_dir()
    } else if let Some(parent) = source.parent() {
        parent.join(&target).is_dir()
    } else {
        target.is_dir()
    };

    if is_dir {
        std::os::windows::fs::symlink_dir(&target, destination)
    } else {
        std::os::windows::fs::symlink_file(&target, destination)
    }
}

/// Copying a symbolic link is only offered where the platform makes it plain.
#[cfg(not(any(unix, windows)))]
fn copy_link(_source: &Path, _destination: &Path) -> Result<(), io::Error> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "copying a symbolic link is not supported on this platform",
    ))
}

/// Whether `destination` is `source` or lies inside it.
///
/// Both paths are resolved as far as they exist, so a destination that reaches
/// into the source through `.` or `..` is recognised as well. The source must
/// exist and the destination's directory must exist, which is true of every
/// copy that is allowed to go ahead.
fn is_inside(source: &Path, destination: &Path) -> Result<bool, io::Error> {
    let name = destination.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the destination has no file name",
        )
    })?;
    let parent = destination.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the destination has no directory",
        )
    })?;

    let source = source.canonicalize()?;
    let parent_canon = if parent.as_os_str().is_empty() {
        working_directory()?
    } else {
        parent.canonicalize()?
    };
    let candidate = parent_canon.join(name);

    Ok(candidate == source || candidate.starts_with(&source))
}

/// Deletes an entry.
///
/// A regular file and a symbolic link are removed as themselves: a link is
/// never followed, so what it points at is left exactly where it is. An empty
/// directory is removed, and a directory that still holds entries is emptied
/// from the bottom up, one entry at a time, and then removed, so a directory is
/// deleted together with everything under it. Links found inside such a
/// directory are removed as links, so a tree is never entered through one.
///
/// A few paths cannot be deleted at all, because removing them would take away
/// far more than could have been meant. See [`check_removable`] for which ones
/// and why. Deleting is the one operation here that cannot be undone, so what
/// it refuses matters as much as what it does.
///
/// A deletion that fails part way through a directory stops there and reports
/// the failure: what has already been removed stays removed, because putting it
/// back is not something this service does.
pub fn delete(path: &Path) -> Result<(), OperationError> {
    let failure = |error: io::Error| {
        OperationError::about_source(Operation::Delete(path.to_path_buf()), path, error)
    };

    check_removable(path).map_err(failure)?;

    // The entry is looked up as an entry in its own right, so a symbolic link
    // is seen even when it points nowhere.
    let metadata = fs::symlink_metadata(path).map_err(failure)?;

    remove_entry(path, &metadata, path)
}

/// Whether `path` may be deleted at all.
///
/// Two things are refused, because each of them would remove far more than a
/// person can have meant to ask for: a path that names no entry of its own,
/// and the directory the process is working in.
///
/// The first refusal is what keeps the root safe. The root directory itself,
/// `.`, `..`, and any path that climbs out of the tree ending in `..` all have
/// no last component to speak of, so they are refused before anything is looked
/// at, and there is no way left to name the root here that would get past it.
///
/// The directory the entry lives in is resolved, so a path that reaches its
/// target through `..` or through a link is judged by where it really points.
/// The entry itself is deliberately not resolved: a symbolic link is the thing
/// being deleted, never what it points at, which is why a link is removable
/// even when its target is not.
fn is_cur_or_parent_dir(path: &Path) -> bool {
    let bytes = path.as_os_str().as_encoded_bytes();
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b'/' || bytes[end - 1] == b'\\') {
        end -= 1;
    }
    let trimmed = &bytes[..end];
    if trimmed.is_empty() {
        return false;
    }
    let last_sep = trimmed.iter().rposition(|&b| b == b'/' || b == b'\\');
    let last_token = match last_sep {
        Some(pos) => &trimmed[pos + 1..],
        None => trimmed,
    };
    last_token == b"." || last_token == b".."
}

fn check_removable(path: &Path) -> Result<(), io::Error> {
    let refuse = |reason: &str| -> Result<(), io::Error> {
        Err(io::Error::new(io::ErrorKind::InvalidInput, reason))
    };

    if is_cur_or_parent_dir(path) {
        return refuse("the path cannot end in . or ..");
    }

    let Some(last) = path.components().next_back() else {
        return refuse("the path does not name an entry");
    };

    let name = match last {
        std::path::Component::Normal(n) => n,
        std::path::Component::CurDir | std::path::Component::ParentDir => {
            return refuse("the path cannot end in . or ..");
        }
        std::path::Component::RootDir | std::path::Component::Prefix(_) => {
            return refuse("the root directory cannot be deleted");
        }
    };

    let Some(parent) = path.parent() else {
        return refuse("the root directory cannot be deleted");
    };

    // A bare name is relative to the working directory, which is what an empty
    // directory part means.
    let parent = if parent.as_os_str().is_empty() {
        working_directory()?
    } else {
        parent.canonicalize()?
    };
    let resolved = parent.join(name);

    if resolved == working_directory()? {
        return refuse("the working directory cannot be deleted");
    }

    Ok(())
}

/// The directory the process is working in, with any symbolic links resolved.
///
/// Nothing about it is assumed: a process whose working directory cannot be
/// determined has nothing to protect, so the failure is reported rather than
/// guessed at.
fn working_directory() -> Result<PathBuf, io::Error> {
    std::env::current_dir()?.canonicalize()
}

/// Removes one entry, which may be a directory holding more entries.
///
/// `origin` is the entry the whole deletion started from, so a failure deep
/// inside a tree can still say what the user asked to delete.
fn remove_entry(path: &Path, metadata: &fs::Metadata, origin: &Path) -> Result<(), OperationError> {
    let failure = |error: io::Error| {
        OperationError::about_source(Operation::Delete(origin.to_path_buf()), path, error)
    };

    // A link goes as the link it is, whatever it points at: it is never walked
    // as a directory, and a link that points nowhere is still a link that can
    // be removed.
    if metadata.file_type().is_symlink() {
        #[cfg(windows)]
        {
            if fs::remove_file(path).is_err() {
                return fs::remove_dir(path).map_err(failure);
            }
            return Ok(());
        }
        #[cfg(not(windows))]
        {
            return fs::remove_file(path).map_err(failure);
        }
    }

    if metadata.is_file() {
        return fs::remove_file(path).map_err(failure);
    }

    if metadata.is_dir() {
        // Entries are taken one at a time, so a large directory is never held
        // in memory as a whole, and each entry is looked at as itself so that
        // its own kind decides what happens to it.
        for item in fs::read_dir(path).map_err(failure)? {
            let item = item.map_err(failure)?;
            let child = item.path();
            let metadata = fs::symlink_metadata(&child).map_err(failure)?;

            remove_entry(&child, &metadata, origin)?;
        }

        return fs::remove_dir(path).map_err(failure);
    }

    Err(failure(io::Error::new(
        io::ErrorKind::Unsupported,
        "only files, directories and symbolic links can be deleted",
    )))
}

#[cfg(test)]
mod tests {
    use super::{
        ErrorCategory, Operation, OperationError, OperationOutcome, check_removable, copy,
        create_directory, create_file, delete, move_item, rename, working_directory,
    };
    use crate::filesystem::test_support::TempDir;
    use std::ffi::OsStr;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    /// The contents the tests write into a file they expect to survive.
    const CONTENTS: &[u8] = b"content";

    /// The source of a failure, when the failure was a rename or a move.
    fn renamed_or_moved_from(error: &OperationError) -> Option<&Path> {
        match error.operation() {
            Operation::Rename(from) | Operation::Move(from) => Some(from),
            _ => None,
        }
    }

    #[test]
    fn creating_a_file_creates_exactly_that_file() {
        let directory = TempDir::new("operations-create-file");
        let file = directory.path().join("notes.txt");

        create_file(&file).expect("the file should be created");

        assert!(file.exists());
        let metadata = fs::metadata(&file).expect("metadata should be read");
        assert!(metadata.is_file(), "the entry must be a regular file");
        assert_eq!(metadata.len(), 0, "the new file must be empty");
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("listing should succeed")
                .count(),
            1,
            "nothing else is created"
        );
    }

    #[test]
    fn creating_a_directory_creates_exactly_that_directory() {
        let directory = TempDir::new("operations-create-directory");
        let created = directory.path().join("subdirectory");

        create_directory(&created).expect("the directory should be created");

        assert!(created.exists());
        assert!(
            fs::metadata(&created)
                .expect("metadata should be read")
                .is_dir()
        );
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("listing should succeed")
                .count(),
            1,
            "nothing else is created"
        );
    }

    #[test]
    fn creating_a_file_that_is_already_there_is_refused() {
        let directory = TempDir::new("operations-create-file-exists");
        let file = directory.path().join("notes.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        let error = create_file(&file).expect_err("the file must not be replaced");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(error.path(), file);
        assert_eq!(
            fs::read(&file).expect("file should be read"),
            CONTENTS,
            "the existing file must be left exactly as it was"
        );
    }

    #[test]
    fn creating_a_directory_that_is_already_there_is_refused() {
        let directory = TempDir::new("operations-create-directory-exists");
        let created = directory.path().join("subdirectory");
        fs::create_dir(&created).expect("directory should be created");
        fs::write(created.join("inside.txt"), CONTENTS).expect("file should be written");

        let error = create_directory(&created).expect_err("the directory must not be reused");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(
            created.join("inside.txt").is_file(),
            "nothing inside the existing directory may be touched"
        );
    }

    #[test]
    fn creating_a_directory_over_a_file_is_refused() {
        let directory = TempDir::new("operations-directory-over-file");
        let file = directory.path().join("notes.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        let error = create_directory(&file).expect_err("the file must not be replaced");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(file.is_file(), "the file must still be a file");
        assert_eq!(fs::read(&file).expect("file should be read"), CONTENTS);
    }

    #[test]
    fn creating_a_file_over_a_directory_is_refused() {
        let directory = TempDir::new("operations-file-over-directory");
        let taken = directory.path().join("subdirectory");
        fs::create_dir(&taken).expect("directory should be created");

        let error = create_file(&taken).expect_err("the directory must not be replaced");

        assert!(error.path() == taken);
        assert!(taken.is_dir(), "the directory must still be a directory");
    }

    #[test]
    fn creating_where_the_parent_does_not_exist_is_refused() {
        let directory = TempDir::new("operations-missing-parent");
        let missing = directory.path().join("missing").join("notes.txt");

        let error = create_file(&missing).expect_err("the parent must not be created");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(error.path(), missing);
        assert!(
            !directory.path().join("missing").exists(),
            "no parent directory may be created on the way"
        );

        let deeper = directory.path().join("missing").join("subdirectory");
        assert!(create_directory(&deeper).is_err());
        assert!(!directory.path().join("missing").exists());
    }

    #[test]
    fn a_file_name_may_contain_spaces_and_letters_from_any_language() {
        let directory = TempDir::new("operations-unicode-file");
        let name = OsStr::new("café notes — 日本語.txt");
        let file = directory.path().join(name);

        create_file(&file).expect("the file should be created");

        assert!(file.is_file());
        let names: Vec<PathBuf> = fs::read_dir(directory.path())
            .expect("listing should succeed")
            .map(|item| item.expect("entry should be readable").path())
            .collect();
        assert_eq!(names, [file]);
    }

    #[test]
    fn a_directory_name_may_contain_spaces_and_letters_from_any_language() {
        let directory = TempDir::new("operations-unicode-directory");
        let name = OsStr::new("my new folder — ünïcode");
        let created = directory.path().join(name);

        create_directory(&created).expect("the directory should be created");

        assert!(created.is_dir());
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("listing should succeed")
                .count(),
            1
        );
    }

    #[test]
    fn renaming_a_file_moves_it_and_keeps_its_contents() {
        let directory = TempDir::new("operations-rename-file");
        let source = directory.path().join("before.txt");
        let destination = directory.path().join("after.txt");
        fs::write(&source, CONTENTS).expect("file should be written");

        rename(&source, &destination).expect("the rename should succeed");

        assert!(!source.exists(), "the old name must be gone");
        assert!(destination.is_file(), "the new name must be there");
        assert_eq!(
            fs::read(&destination).expect("file should be read"),
            CONTENTS
        );
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("listing should succeed")
                .count(),
            1,
            "renaming must not leave anything behind"
        );
    }

    #[test]
    fn renaming_a_directory_moves_it_with_everything_inside() {
        let directory = TempDir::new("operations-rename-directory");
        let source = directory.path().join("before");
        let destination = directory.path().join("after");
        fs::create_dir(&source).expect("directory should be created");
        fs::write(source.join("inside.txt"), CONTENTS).expect("file should be written");

        rename(&source, &destination).expect("the rename should succeed");

        assert!(!source.exists());
        assert!(destination.is_dir(), "a directory must stay a directory");
        assert_eq!(
            fs::read(destination.join("inside.txt")).expect("file should be read"),
            CONTENTS,
            "the contents must move with the directory"
        );
    }

    #[test]
    fn renaming_something_that_is_not_there_is_refused() {
        let directory = TempDir::new("operations-rename-missing");
        let source = directory.path().join("before.txt");
        let destination = directory.path().join("after.txt");

        let error = rename(&source, &destination).expect_err("the rename must fail");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(renamed_or_moved_from(&error), Some(source.as_path()));
        assert_eq!(error.path(), destination);
        assert!(
            !destination.exists(),
            "nothing may be created by a failed rename"
        );
    }

    #[test]
    fn renaming_onto_an_existing_entry_is_refused() {
        let directory = TempDir::new("operations-rename-taken");
        let source = directory.path().join("before.txt");
        let destination = directory.path().join("after.txt");
        fs::write(&source, CONTENTS).expect("file should be written");
        fs::write(&destination, b"other").expect("file should be written");

        let error = rename(&source, &destination).expect_err("the destination must be kept");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read(&destination).expect("file should be read"),
            b"other",
            "the destination must not be replaced"
        );
        assert!(source.is_file(), "the source must still be there");
    }

    #[test]
    fn renaming_a_directory_onto_an_existing_directory_is_refused() {
        let directory = TempDir::new("operations-rename-taken-directory");
        let source = directory.path().join("before");
        let destination = directory.path().join("after");
        fs::create_dir(&source).expect("directory should be created");
        fs::create_dir(&destination).expect("directory should be created");
        fs::write(destination.join("inside.txt"), CONTENTS).expect("file should be written");

        let error = rename(&source, &destination).expect_err("the destination must be kept");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(destination.join("inside.txt").is_file());
        assert!(source.is_dir());
    }

    #[test]
    fn renaming_to_the_name_it_already_has_changes_nothing() {
        let directory = TempDir::new("operations-rename-same");
        let file = directory.path().join("notes.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        rename(&file, &file).expect("asking for the name it has is not an error");

        assert!(file.is_file());
        assert_eq!(fs::read(&file).expect("file should be read"), CONTENTS);
    }

    #[test]
    fn renaming_to_a_name_with_spaces_and_letters_from_any_language_works() {
        let directory = TempDir::new("operations-rename-unicode");
        let source = directory.path().join("before.txt");
        let destination = directory.path().join("kopia — コピー.txt");
        fs::write(&source, CONTENTS).expect("file should be written");

        rename(&source, &destination).expect("the rename should succeed");

        assert!(destination.is_file());
        assert!(!source.exists());
    }

    #[test]
    fn an_unusable_path_is_refused() {
        let directory = TempDir::new("operations-unusable-path");

        for path in [
            PathBuf::new(),
            directory.path().join("with\0a-nul-byte.txt"),
        ] {
            let error = create_file(&path).expect_err("the path cannot be used");

            assert_eq!(error.path(), path);
            assert!(
                matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidInput
                ),
                "unexpected failure: {error}"
            );
        }

        assert_eq!(
            fs::read_dir(directory.path())
                .expect("listing should succeed")
                .count(),
            0
        );
    }

    #[test]
    fn a_file_name_may_not_be_an_existing_directory_that_is_not_empty() {
        // Renaming a directory to a name that is taken must not remove what is
        // under that name, which is the same rule as for files.
        let directory = TempDir::new("operations-rename-nonempty");
        let source = directory.path().join("before");
        let destination = directory.path().join("after");
        fs::create_dir(&source).expect("directory should be created");
        fs::create_dir(&destination).expect("directory should be created");
        fs::write(destination.join("kept.txt"), CONTENTS).expect("file should be written");

        assert!(rename(&source, &destination).is_err());
        assert_eq!(
            fs::read(destination.join("kept.txt")).expect("file should be read"),
            CONTENTS
        );
    }

    /// Writes a small tree and returns its root.
    ///
    /// ```text
    /// root/
    ///   file.txt
    ///   nested/
    ///     nested.txt
    ///     deeper/
    ///       deep.txt
    /// ```
    fn write_tree(root: &Path) {
        fs::create_dir_all(root.join("nested").join("deeper"))
            .expect("directory should be created");
        fs::write(root.join("file.txt"), b"top").expect("file should be written");
        fs::write(root.join("nested").join("nested.txt"), b"middle")
            .expect("file should be written");
        fs::write(
            root.join("nested").join("deeper").join("deep.txt"),
            b"bottom",
        )
        .expect("file should be written");
    }

    /// Asserts that `root` holds the tree [`write_tree`] wrote.
    fn assert_tree(root: &Path, expectation: &str) {
        assert_eq!(
            fs::read(root.join("file.txt")).expect("file should be read"),
            b"top",
            "{expectation}: the top file"
        );
        assert_eq!(
            fs::read(root.join("nested").join("nested.txt")).expect("file should be read"),
            b"middle",
            "{expectation}: the nested file"
        );
        assert_eq!(
            fs::read(root.join("nested").join("deeper").join("deep.txt"))
                .expect("file should be read"),
            b"bottom",
            "{expectation}: the deepest file"
        );
    }

    #[test]
    fn copying_a_file_copies_it_and_leaves_the_original() {
        let directory = TempDir::new("operations-copy-file");
        let source = directory.path().join("notes.txt");
        let destination = directory.path().join("notes-copy.txt");
        fs::write(&source, CONTENTS).expect("file should be written");

        copy(&source, &destination).expect("the copy should succeed");

        assert!(destination.is_file(), "the copy must be a regular file");
        assert_eq!(
            fs::read(&destination).expect("file should be read"),
            CONTENTS
        );
        assert_eq!(
            fs::read(&source).expect("file should be read"),
            CONTENTS,
            "the original must stay where it is"
        );
    }

    #[test]
    fn copying_a_directory_copies_everything_under_it() {
        let directory = TempDir::new("operations-copy-directory");
        let source = directory.path().join("tree");
        let destination = directory.path().join("tree-copy");
        write_tree(&source);

        copy(&source, &destination).expect("the copy should succeed");

        assert!(destination.is_dir());
        assert_tree(&destination, "the copy");
        assert_tree(&source, "the original");
    }

    #[test]
    fn copying_an_empty_directory_creates_an_empty_directory() {
        let directory = TempDir::new("operations-copy-empty-directory");
        let source = directory.path().join("empty");
        let destination = directory.path().join("empty-copy");
        fs::create_dir(&source).expect("directory should be created");

        copy(&source, &destination).expect("the copy should succeed");

        assert!(destination.is_dir());
        assert_eq!(
            fs::read_dir(&destination)
                .expect("listing should succeed")
                .count(),
            0
        );
    }

    #[test]
    fn copying_onto_a_taken_name_is_refused() {
        let directory = TempDir::new("operations-copy-taken");
        let source = directory.path().join("notes.txt");
        fs::write(&source, CONTENTS).expect("file should be written");

        // Onto a file, and onto a directory: neither may be replaced.
        let taken_file = directory.path().join("taken.txt");
        fs::write(&taken_file, b"other").expect("file should be written");
        let taken_directory = directory.path().join("taken");
        fs::create_dir(&taken_directory).expect("directory should be created");
        fs::write(taken_directory.join("inside.txt"), CONTENTS).expect("file should be written");

        for taken in [&taken_file, &taken_directory] {
            let error = copy(&source, taken).expect_err("nothing may be replaced");

            assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
            assert_eq!(error.path(), taken.as_path());
        }

        assert_eq!(
            fs::read(&taken_file).expect("file should be read"),
            b"other"
        );
        assert!(taken_directory.join("inside.txt").is_file());
    }

    #[test]
    fn copying_something_that_is_not_there_is_refused() {
        let directory = TempDir::new("operations-copy-missing");
        let source = directory.path().join("missing.txt");
        let destination = directory.path().join("copy.txt");

        let error = copy(&source, &destination).expect_err("the copy must fail");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(!destination.exists(), "a failed copy creates nothing");
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("listing should succeed")
                .count(),
            0
        );
    }

    #[test]
    fn copying_where_the_destination_directory_does_not_exist_is_refused() {
        let directory = TempDir::new("operations-copy-missing-parent");
        let source = directory.path().join("notes.txt");
        fs::write(&source, CONTENTS).expect("file should be written");
        let destination = directory.path().join("missing").join("notes.txt");

        let error = copy(&source, &destination).expect_err("the copy must fail");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(
            !directory.path().join("missing").exists(),
            "no parent directory may be created on the way"
        );
    }

    #[test]
    fn copying_onto_itself_is_refused() {
        let directory = TempDir::new("operations-copy-itself");
        let file = directory.path().join("notes.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        let error = copy(&file, &file).expect_err("an entry must not be copied onto itself");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&file).expect("file should be read"), CONTENTS);
    }

    #[test]
    fn copying_a_directory_into_itself_is_refused() {
        let directory = TempDir::new("operations-copy-into-itself");
        let source = directory.path().join("tree");
        write_tree(&source);

        for destination in [
            source.join("copy"),
            source.join("nested").join("copy"),
            source.join("nested").join("deeper").join("tree"),
        ] {
            let error = copy(&source, &destination)
                .expect_err("a directory must not be copied into itself");

            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert!(!destination.exists(), "nothing may be created");
        }

        assert_tree(&source, "the source");
    }

    #[test]
    fn copying_into_itself_through_a_resolved_path_is_refused() {
        let directory = TempDir::new("operations-copy-into-itself-resolved");
        let source = directory.path().join("tree");
        write_tree(&source);

        // The path does not look like it is inside the source, but resolving it
        // shows that it is.
        let destination = source.join("nested").join("..").join("copy");

        let error = copy(&source, &destination).expect_err("the copy must be refused");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(!source.join("copy").exists());
    }

    #[test]
    fn names_with_spaces_and_letters_from_any_language_can_be_copied() {
        let directory = TempDir::new("operations-copy-unicode");
        let source = directory.path().join("my notes — メモ.txt");
        fs::write(&source, CONTENTS).expect("file should be written");
        let destination = directory.path().join("my notes — コピー.txt");

        copy(&source, &destination).expect("the copy should succeed");

        assert_eq!(
            fs::read(&destination).expect("file should be read"),
            CONTENTS
        );

        let directory_source = directory.path().join("folder — フォルダ");
        let directory_destination = directory.path().join("folder — コピー");
        fs::create_dir(&directory_source).expect("directory should be created");
        fs::write(directory_source.join("inside.txt"), CONTENTS).expect("file should be written");

        copy(&directory_source, &directory_destination).expect("the copy should succeed");

        assert_eq!(
            fs::read(directory_destination.join("inside.txt")).expect("file should be read"),
            CONTENTS
        );
    }

    #[test]
    fn moving_a_file_moves_it() {
        let directory = TempDir::new("operations-move-file");
        let source = directory.path().join("before.txt");
        let destination = directory.path().join("after.txt");
        fs::write(&source, CONTENTS).expect("file should be written");

        move_item(&source, &destination).expect("the move should succeed");

        assert!(!source.exists(), "the source must be gone");
        assert_eq!(
            fs::read(&destination).expect("file should be read"),
            CONTENTS
        );
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("listing should succeed")
                .count(),
            1,
            "a move must not leave anything behind"
        );
    }

    #[test]
    fn moving_a_directory_moves_everything_under_it() {
        let directory = TempDir::new("operations-move-directory");
        let source = directory.path().join("tree");
        let destination = directory.path().join("moved");
        write_tree(&source);

        move_item(&source, &destination).expect("the move should succeed");

        assert!(!source.exists());
        assert_tree(&destination, "the moved tree");
    }

    #[test]
    fn moving_onto_a_taken_name_is_refused() {
        let directory = TempDir::new("operations-move-taken");
        let source = directory.path().join("before.txt");
        let taken = directory.path().join("after.txt");
        fs::write(&source, CONTENTS).expect("file should be written");
        fs::write(&taken, b"other").expect("file should be written");

        for destination in [taken.clone(), source.clone()] {
            let error = move_item(&source, &destination).expect_err("nothing may be replaced");

            assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
            assert_eq!(
                renamed_or_moved_from(&error),
                Some(source.as_path()),
                "the failure must name what was being moved"
            );
        }

        assert_eq!(fs::read(&taken).expect("file should be read"), b"other");
        assert_eq!(fs::read(&source).expect("file should be read"), CONTENTS);
    }

    #[test]
    fn moving_something_that_is_not_there_is_refused() {
        let directory = TempDir::new("operations-move-missing");
        let source = directory.path().join("missing.txt");
        let destination = directory.path().join("moved.txt");

        let error = move_item(&source, &destination).expect_err("the move must fail");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(!destination.exists());
    }

    #[test]
    fn moving_where_the_destination_directory_does_not_exist_is_refused() {
        let directory = TempDir::new("operations-move-missing-parent");
        let source = directory.path().join("notes.txt");
        fs::write(&source, CONTENTS).expect("file should be written");
        let destination = directory.path().join("missing").join("notes.txt");

        let error = move_item(&source, &destination).expect_err("the move must fail");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(source.is_file(), "the source must stay where it is");
        assert!(!directory.path().join("missing").exists());
    }

    #[test]
    fn deleting_a_file_removes_it() {
        let directory = TempDir::new("operations-delete-file");
        let file = directory.path().join("notes.txt");
        fs::write(&file, CONTENTS).expect("file should be written");

        delete(&file).expect("the file should be deleted");

        assert!(!file.exists(), "the file must be gone");
        assert!(
            directory.path().is_dir(),
            "the directory it was in must stay"
        );
    }

    #[test]
    fn deleting_an_empty_directory_removes_it() {
        let directory = TempDir::new("operations-delete-empty-directory");
        let empty = directory.path().join("empty");
        fs::create_dir(&empty).expect("directory should be created");

        delete(&empty).expect("the directory should be deleted");

        assert!(!empty.exists(), "the directory must be gone");
        assert!(
            directory.path().is_dir(),
            "the directory it was in must stay"
        );
    }

    #[test]
    fn deleting_a_directory_removes_everything_under_it() {
        let directory = TempDir::new("operations-delete-tree");
        let tree = directory.path().join("tree");
        write_tree(&tree);

        delete(&tree).expect("the tree should be deleted");

        assert!(!tree.exists(), "the tree must be gone");
        assert!(
            !tree.join("nested").exists(),
            "a directory under it must be gone"
        );
        assert!(
            !tree.join("nested").join("deeper").exists(),
            "a directory two levels down must be gone"
        );
        assert!(
            !tree.join("nested").join("deeper").join("deep.txt").exists(),
            "a file deep inside must be gone"
        );
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("listing should succeed")
                .count(),
            0,
            "nothing outside the tree may be removed"
        );
        assert!(
            directory.path().is_dir(),
            "the directory the tree was in must stay"
        );
    }

    #[test]
    fn deleting_something_that_is_not_there_is_refused() {
        let directory = TempDir::new("operations-delete-missing");
        let missing = directory.path().join("missing.txt");

        let error = delete(&missing).expect_err("there is nothing to delete");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(error.outcome(), OperationOutcome::SourceMissing);
        assert!(
            error.to_string().contains(&missing.display().to_string()),
            "the report must name the entry: {error}"
        );
    }

    #[test]
    fn deleting_an_empty_path_is_refused() {
        let error = delete(Path::new("")).expect_err("an empty path names no entry");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(error.outcome(), OperationOutcome::InvalidPath);
    }

    #[test]
    fn deleting_a_path_that_names_no_entry_is_refused() {
        // `.` is the directory the process is in and `..` is the one above it:
        // neither names an entry that could be removed on its own. These are
        // asked of the check rather than of the deletion itself, so that this
        // test cannot remove anything even if the check ever stopped working.
        for path in [Path::new("."), Path::new(".."), Path::new("entry/..")] {
            let error = check_removable(path).expect_err("such a path must be refused");

            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{path:?}");
        }

        assert!(Path::new(".").is_dir(), "nothing may be removed");
    }

    #[test]
    fn deleting_the_root_directory_is_refused() {
        // Asked of the check, which only looks, and never of the deletion: a
        // test that really asked for the root to be deleted would be a hazard
        // of its own.
        let root = Path::new(std::path::MAIN_SEPARATOR_STR);

        let error = check_removable(root).expect_err("the root directory must never be deleted");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(root.is_dir(), "the root directory must still be there");
    }

    #[test]
    fn deleting_the_working_directory_is_refused() {
        // The directory the process is working in is the one path that must
        // never be removed, so the check is asked about it directly. The check
        // only looks: nothing here removes anything, and the working directory
        // is left as it was for every other test that runs here.
        let working = working_directory().expect("the working directory should be known");

        let error = check_removable(&working).expect_err("the working directory must be refused");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(
            working.is_dir(),
            "the working directory must still be there"
        );
    }

    #[test]
    fn deleting_a_path_that_climbs_out_of_the_tree_is_refused() {
        let directory = TempDir::new("operations-delete-climb");
        let above = directory.path().join("one");
        let nested = above.join("two").join("three");
        fs::create_dir_all(&nested).expect("directories should be created");
        fs::write(nested.join("file.txt"), CONTENTS).expect("file should be written");

        // Such a path ends in `..`, which names the directory above rather than
        // an entry of its own, so it is refused even though it points at a
        // directory that exists.
        let climbing = nested.join("..").join("..");

        let error = delete(&climbing).expect_err("climbing out must be refused");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(error.outcome(), OperationOutcome::InvalidPath);
        assert!(nested.join("file.txt").is_file(), "nothing may be removed");
        assert!(
            above.is_dir(),
            "the directory it pointed at must still be there"
        );
    }

    #[test]
    fn a_taken_destination_is_reported_as_a_taken_destination() {
        let directory = TempDir::new("operations-outcome-taken");
        let source = directory.path().join("source.txt");
        let taken = directory.path().join("taken.txt");
        fs::write(&source, CONTENTS).expect("file should be written");
        fs::write(&taken, b"other").expect("file should be written");

        let failures = [
            copy(&source, &taken).expect_err("the name is taken"),
            move_item(&source, &taken).expect_err("the name is taken"),
            rename(&source, &taken).expect_err("the name is taken"),
            create_file(&taken).expect_err("the name is taken"),
            create_directory(&taken).expect_err("the name is taken"),
        ];

        for failure in failures {
            assert_eq!(failure.kind(), io::ErrorKind::AlreadyExists, "{failure}");
            assert_eq!(
                failure.outcome(),
                OperationOutcome::DestinationExists,
                "{failure}"
            );
        }

        assert_eq!(
            fs::read(&source).expect("file should be read"),
            CONTENTS,
            "a conflict must leave the source alone"
        );
        assert_eq!(
            fs::read(&taken).expect("file should be read"),
            b"other",
            "a conflict must leave the destination alone"
        );
    }

    #[test]
    fn a_missing_source_is_reported_as_a_missing_source() {
        let directory = TempDir::new("operations-outcome-missing-source");
        let missing = directory.path().join("missing.txt");
        let destination = directory.path().join("destination.txt");

        let failures = [
            copy(&missing, &destination).expect_err("there is nothing to copy"),
            move_item(&missing, &destination).expect_err("there is nothing to move"),
            rename(&missing, &destination).expect_err("there is nothing to rename"),
            delete(&missing).expect_err("there is nothing to delete"),
        ];

        for failure in failures {
            assert_eq!(failure.kind(), io::ErrorKind::NotFound, "{failure}");
            assert_eq!(
                failure.outcome(),
                OperationOutcome::SourceMissing,
                "{failure}"
            );
        }

        assert!(!destination.exists(), "nothing may be created");
    }

    #[test]
    fn a_missing_destination_directory_is_reported_as_a_missing_parent() {
        let directory = TempDir::new("operations-outcome-missing-parent");
        let source = directory.path().join("source.txt");
        fs::write(&source, CONTENTS).expect("file should be written");
        let destination = directory.path().join("gone").join("destination.txt");

        let failures = [
            copy(&source, &destination).expect_err("the directory is not there"),
            move_item(&source, &destination).expect_err("the directory is not there"),
            rename(&source, &destination).expect_err("the directory is not there"),
            create_file(&destination).expect_err("the directory is not there"),
            create_directory(&destination).expect_err("the directory is not there"),
        ];

        for failure in failures {
            assert_eq!(failure.kind(), io::ErrorKind::NotFound, "{failure}");
            assert_eq!(
                failure.outcome(),
                OperationOutcome::ParentMissing,
                "{failure}"
            );
        }

        assert!(source.is_file(), "the source must be untouched");
    }

    #[test]
    fn a_copy_whose_paths_clash_is_reported_as_a_conflict() {
        let directory = TempDir::new("operations-outcome-conflict");
        let tree = directory.path().join("tree");
        write_tree(&tree);

        // A directory copied into itself would be copied forever, so the two
        // paths cannot both be honoured and the copy is refused before anything
        // is created.
        let failure =
            copy(&tree, &tree.join("nested").join("copy")).expect_err("the copy must be refused");

        assert_eq!(failure.category(), ErrorCategory::Conflict);
        assert_eq!(failure.outcome(), OperationOutcome::Conflict);
        assert!(failure.category().is_conflict());
        assert_tree(&tree, "the source");
        assert!(!tree.join("nested").join("copy").exists());
    }

    #[test]
    fn a_copy_into_a_resolved_descendant_is_a_conflict_too() {
        let directory = TempDir::new("operations-outcome-conflict-resolved");
        let tree = directory.path().join("tree");
        write_tree(&tree);

        let failure = copy(&tree, &tree.join("nested").join("..").join("copy"))
            .expect_err("the copy must be refused");

        assert_eq!(failure.category(), ErrorCategory::Conflict);
        assert_eq!(failure.outcome(), OperationOutcome::Conflict);
    }

    #[test]
    fn a_missing_source_is_categorized_as_not_found() {
        let directory = TempDir::new("operations-category-missing");
        let missing = directory.path().join("missing.txt");

        let failure = delete(&missing).expect_err("there is nothing to delete");

        assert_eq!(failure.category(), ErrorCategory::NotFound);
        assert_eq!(failure.outcome(), OperationOutcome::SourceMissing);
    }

    #[test]
    fn a_taken_destination_is_categorized_as_already_there() {
        let directory = TempDir::new("operations-category-taken");
        let taken = directory.path().join("taken.txt");
        fs::write(&taken, CONTENTS).expect("file should be written");

        let failure = create_file(&taken).expect_err("the name is taken");

        assert_eq!(failure.category(), ErrorCategory::AlreadyExists);
        assert!(failure.category().is_conflict());
        assert_eq!(failure.outcome(), OperationOutcome::DestinationExists);
        assert_eq!(
            fs::read(&taken).expect("the file should still be readable"),
            CONTENTS,
            "nothing may be replaced"
        );
    }

    #[test]
    fn the_categories_that_cannot_be_arranged_here_are_still_classified() {
        // A read-only filesystem, a directory that will not come away and an
        // interrupted call cannot be arranged on a machine running the tests
        // reliably, so the mapping from what the operating system would report
        // is checked directly instead of being left untested.
        let cases = [
            (
                io::ErrorKind::ReadOnlyFilesystem,
                ErrorCategory::ReadOnly,
                OperationOutcome::ReadOnly,
            ),
            (
                io::ErrorKind::DirectoryNotEmpty,
                ErrorCategory::Conflict,
                OperationOutcome::Conflict,
            ),
            (
                io::ErrorKind::Interrupted,
                ErrorCategory::Cancelled,
                OperationOutcome::Cancelled,
            ),
            (
                io::ErrorKind::NotADirectory,
                ErrorCategory::NotDirectory,
                OperationOutcome::InvalidPath,
            ),
            (
                io::ErrorKind::IsADirectory,
                ErrorCategory::IsDirectory,
                OperationOutcome::InvalidPath,
            ),
            (
                io::ErrorKind::Unsupported,
                ErrorCategory::Unsupported,
                OperationOutcome::Unsupported,
            ),
            (
                io::ErrorKind::PermissionDenied,
                ErrorCategory::PermissionDenied,
                OperationOutcome::PermissionDenied,
            ),
        ];

        for (kind, category, outcome) in cases {
            let failure = OperationError::new(
                Operation::CreateFile,
                Path::new("somewhere"),
                io::Error::new(kind, "arranged for this test"),
            );

            assert_eq!(failure.category(), category, "{kind:?}");
            assert_eq!(failure.outcome(), outcome, "{kind:?}");
            assert_eq!(
                failure.kind(),
                kind,
                "the operating system's own kind is kept"
            );
        }
    }

    // Symbolic links can be created without elevation on Unix, but not reliably
    // on Windows, so this test runs on Unix only.
    #[cfg(unix)]
    mod links {
        use super::{
            CONTENTS, PathBuf, TempDir, copy, create_file, delete, fs, rename, write_tree,
        };
        use std::os::unix::fs::symlink;

        #[test]
        fn renaming_a_link_moves_the_link_and_leaves_its_target_alone() {
            let directory = TempDir::new("operations-rename-link");
            let target = directory.path().join("target.txt");
            fs::write(&target, super::CONTENTS).expect("file should be written");
            let link = directory.path().join("link.txt");
            symlink(&target, &link).expect("link should be created");

            let moved = directory.path().join("moved-link.txt");
            rename(&link, &moved).expect("the rename should succeed");

            assert!(
                fs::symlink_metadata(&moved)
                    .expect("metadata should be read")
                    .file_type()
                    .is_symlink(),
                "the new name must still be a link"
            );
            assert!(target.is_file(), "the target must be untouched");
            assert!(!link.exists(), "the old link name must be gone");
            assert_eq!(
                fs::read(&moved).expect("file should be read"),
                super::CONTENTS
            );
        }

        #[test]
        fn a_broken_link_can_still_be_renamed() {
            let directory = TempDir::new("operations-rename-broken-link");
            let missing_target: PathBuf = directory.path().join("missing.txt");
            let link = directory.path().join("link.txt");
            symlink(&missing_target, &link).expect("link should be created");

            let moved = directory.path().join("moved-link.txt");
            rename(&link, &moved).expect("a link that points nowhere can still be moved");

            assert!(fs::symlink_metadata(&moved).is_ok());
            assert!(!link.exists());
            assert!(
                create_file(&moved).is_err(),
                "the moved name is taken by the link"
            );
        }
        #[test]
        fn copying_a_link_copies_the_link_and_not_what_it_points_at() {
            let directory = TempDir::new("operations-copy-link");
            let target = directory.path().join("target.txt");
            fs::write(&target, CONTENTS).expect("file should be written");
            let link = directory.path().join("link.txt");
            symlink(&target, &link).expect("link should be created");

            let destination = directory.path().join("link-copy.txt");
            copy(&link, &destination).expect("the copy should succeed");

            assert!(
                fs::symlink_metadata(&destination)
                    .expect("metadata should be read")
                    .file_type()
                    .is_symlink(),
                "the copy must be a link, not the file it points at"
            );
            assert_eq!(
                fs::read_link(&destination).expect("the link should be read"),
                target
            );
            assert_eq!(
                fs::read(&destination).expect("file should be read"),
                CONTENTS
            );
        }

        #[test]
        fn copying_a_directory_holding_a_link_does_not_walk_through_it() {
            let directory = TempDir::new("operations-copy-link-tree");
            let tree = directory.path().join("tree");
            fs::create_dir(&tree).expect("directory should be created");
            fs::write(tree.join("file.txt"), CONTENTS).expect("file should be written");
            let outside = directory.path().join("outside");
            fs::create_dir(&outside).expect("directory should be created");
            fs::write(outside.join("outside.txt"), CONTENTS).expect("file should be written");
            // A link back to the directory that holds the tree: following it
            // would copy for ever.
            symlink(directory.path(), tree.join("link")).expect("link should be created");

            let destination = directory.path().join("tree-copy");
            copy(&tree, &destination).expect("the copy should succeed");

            assert_eq!(
                fs::read(destination.join("file.txt")).expect("file should be read"),
                CONTENTS
            );
            assert!(
                fs::symlink_metadata(destination.join("link"))
                    .expect("metadata should be read")
                    .file_type()
                    .is_symlink(),
                "the link must be copied as a link"
            );
            assert_eq!(
                fs::read_link(destination.join("link")).expect("the link should be read"),
                directory.path(),
                "the copy must point where the original pointed"
            );
            assert_eq!(
                fs::read_dir(&destination)
                    .expect("listing should succeed")
                    .count(),
                2,
                "the copy holds the file and the link, and nothing else"
            );
            assert_eq!(
                fs::read_dir(&tree).expect("listing should succeed").count(),
                2,
                "walking through the link would have created entries in the source"
            );
        }

        #[test]
        fn a_broken_link_can_be_copied_without_panicking() {
            let directory = TempDir::new("operations-copy-broken-link");
            let link = directory.path().join("link.txt");
            symlink(directory.path().join("nowhere.txt"), &link).expect("link should be created");

            let destination = directory.path().join("link-copy.txt");
            copy(&link, &destination).expect("a link that points nowhere can still be copied");

            assert!(
                fs::symlink_metadata(&destination)
                    .expect("metadata should be read")
                    .file_type()
                    .is_symlink()
            );
            assert!(
                fs::symlink_metadata(&destination)
                    .expect("metadata should be read")
                    .len()
                    > 0
            );
        }
        #[test]
        fn deleting_a_link_removes_the_link_and_leaves_its_target() {
            let directory = TempDir::new("operations-delete-link");
            let target = directory.path().join("target");
            fs::create_dir(&target).expect("directory should be created");
            fs::write(target.join("file.txt"), CONTENTS).expect("file should be written");
            let link = directory.path().join("link");
            symlink(&target, &link).expect("link should be created");

            delete(&link).expect("the link should be deleted");

            assert!(
                fs::symlink_metadata(&link).is_err(),
                "the link itself must be gone"
            );
            assert!(target.is_dir(), "the directory it pointed at must stay");
            assert_eq!(
                fs::read(target.join("file.txt")).expect("file should be read"),
                CONTENTS,
                "what the link pointed at must be untouched"
            );
        }

        #[test]
        fn deleting_a_link_to_a_file_leaves_the_file() {
            let directory = TempDir::new("operations-delete-file-link");
            let target = directory.path().join("target.txt");
            fs::write(&target, CONTENTS).expect("file should be written");
            let link = directory.path().join("link.txt");
            symlink(&target, &link).expect("link should be created");

            delete(&link).expect("the link should be deleted");

            assert!(
                fs::symlink_metadata(&link).is_err(),
                "the link itself must be gone"
            );
            assert_eq!(
                fs::read(&target).expect("file should be read"),
                CONTENTS,
                "the file it pointed at must be untouched"
            );
        }

        #[test]
        fn a_broken_link_can_be_deleted() {
            let directory = TempDir::new("operations-delete-broken-link");
            let link = directory.path().join("link");
            symlink(directory.path().join("gone"), &link).expect("link should be created");

            delete(&link).expect("a link that points at nothing is still a link");

            assert!(
                fs::symlink_metadata(&link).is_err(),
                "the link must be gone"
            );
        }

        #[test]
        fn deleting_a_directory_holding_a_link_does_not_follow_it() {
            let directory = TempDir::new("operations-delete-tree-link");
            let outside = directory.path().join("outside");
            fs::create_dir(&outside).expect("directory should be created");
            fs::write(outside.join("outside.txt"), CONTENTS).expect("file should be written");

            let tree = directory.path().join("tree");
            write_tree(&tree);
            symlink(&outside, tree.join("link")).expect("link should be created");

            delete(&tree).expect("the tree should be deleted");

            assert!(!tree.exists(), "the tree must be gone");
            assert!(
                outside.join("outside.txt").is_file(),
                "a link inside the tree must not have been walked, so what it pointed at must still be there"
            );
        }
    }

    // Permission handling needs a non-root user, so it is Unix-only, and it is
    // skipped when the process can write anyway.
    #[cfg(unix)]
    mod permissions {
        use super::{
            CONTENTS, OperationOutcome, TempDir, copy, create_directory, create_file, delete, fs,
            io, rename,
        };
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

        /// Makes `directory` read-only and undoes that when it goes away.
        fn lock(directory: &PathBuf) -> RestorePermissions {
            let mode = fs::metadata(directory)
                .expect("metadata should be read")
                .permissions()
                .mode();
            let restore = RestorePermissions {
                path: directory.clone(),
                mode,
            };
            fs::set_permissions(directory, fs::Permissions::from_mode(0o500))
                .expect("permissions should be set");

            restore
        }

        #[test]
        fn creating_in_a_directory_that_may_not_be_written_is_refused() {
            let directory = TempDir::new("operations-permission-create");
            let locked = directory.path().join("locked");
            fs::create_dir(&locked).expect("directory should be created");
            let _restore = lock(&locked);

            let file = locked.join("notes.txt");

            // Elevated privileges, such as running as root, ignore the mode, so
            // nothing can be observed and nothing is claimed.
            match create_file(&file) {
                Ok(()) => {}
                Err(error) => {
                    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
                    assert!(!file.exists(), "a refused creation creates nothing");
                    assert!(
                        create_directory(&locked.join("subdirectory"))
                            .expect_err("writing must be refused")
                            .kind()
                            == io::ErrorKind::PermissionDenied
                    );
                    assert_eq!(
                        fs::read_dir(&locked)
                            .expect("the directory itself can be read")
                            .count(),
                        0
                    );
                }
            }
        }

        #[test]
        fn renaming_out_of_a_directory_that_may_not_be_written_is_refused() {
            let directory = TempDir::new("operations-permission-rename");
            let locked = directory.path().join("locked");
            fs::create_dir(&locked).expect("directory should be created");
            let source = locked.join("notes.txt");
            fs::write(&source, b"content").expect("file should be written");
            let _restore = lock(&locked);

            let destination = directory.path().join("moved.txt");

            // Elevated privileges, such as running as root, ignore the mode.
            match rename(&source, &destination) {
                Ok(()) => {}
                Err(error) => {
                    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
                    assert!(source.is_file(), "the source must still be there");
                    assert!(!destination.exists(), "a refused rename moves nothing");
                }
            }
        }

        #[test]
        fn a_copy_that_cannot_read_a_directory_stops_and_reports_it() {
            let directory = TempDir::new("operations-copy-permission");
            let source = directory.path().join("tree");
            let locked = source.join("locked");
            fs::create_dir_all(&locked).expect("directory should be created");
            fs::write(source.join("kept.txt"), CONTENTS).expect("file should be written");
            fs::write(locked.join("hidden.txt"), CONTENTS).expect("file should be written");

            let mode = fs::metadata(&locked)
                .expect("metadata should be read")
                .permissions()
                .mode();
            let _restore = RestorePermissions {
                path: locked.clone(),
                mode,
            };
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o000))
                .expect("permissions should be set");

            let destination = directory.path().join("copy");
            match copy(&source, &destination) {
                // Elevated privileges, such as running as root, ignore the mode.
                Ok(()) => {}
                Err(error) => {
                    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
                    assert_eq!(
                        fs::read(source.join("kept.txt")).expect("file should be read"),
                        CONTENTS,
                        "the source must be untouched by a failed copy"
                    );
                }
            }
        }
        #[test]
        fn deleting_from_a_directory_that_may_not_be_written_is_refused() {
            let directory = TempDir::new("operations-delete-permission");
            let locked = directory.path().join("locked");
            fs::create_dir(&locked).expect("directory should be created");
            let file = locked.join("notes.txt");
            fs::write(&file, CONTENTS).expect("file should be written");
            let _restore = lock(&locked);

            // Elevated privileges, such as running as root, ignore the mode.
            match delete(&file) {
                Ok(()) => {}
                Err(error) => {
                    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
                    assert_eq!(error.outcome(), OperationOutcome::PermissionDenied);
                    assert!(file.is_file(), "the file must still be there");
                }
            }
        }
    }

    #[test]
    fn test_is_inside_with_bare_component_path() {
        use super::is_inside;
        use std::path::Path;

        let directory = TempDir::new("operations-is-inside");
        let source = directory.path().join("source_dir");
        fs::create_dir(&source).expect("create source");

        // A destination specified as a bare name (whose parent is empty OsStr)
        let bare_dest = Path::new("some_file_in_cwd.txt");
        let inside = is_inside(&source, bare_dest);
        assert!(inside.is_ok());
        assert!(!inside.unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_edge_cases_and_loops() {
        use std::os::unix::fs::symlink;

        let directory = TempDir::new("operations-symlinks");
        let root = directory.path().to_path_buf();

        // 1. Symlink loop: a -> b, b -> a
        let link_a = root.join("link_a");
        let link_b = root.join("link_b");
        symlink("link_b", &link_a).expect("create link a");
        symlink("link_a", &link_b).expect("create link b");

        // Deleting link_a must remove link_a directly and never loop infinitely or follow
        let outcome = delete(&link_a);
        assert!(outcome.is_ok());
        assert!(!link_a.exists());
        assert!(fs::symlink_metadata(&link_a).is_err());
        // link_b still exists as a broken symlink
        assert!(fs::symlink_metadata(&link_b).is_ok());

        // 2. Broken symlink operations
        let broken = root.join("broken_link");
        symlink("non_existent_target.txt", &broken).expect("create broken symlink");
        assert!(fs::symlink_metadata(&broken).is_ok());
        assert!(!broken.exists()); // target doesn't exist

        // Copy broken symlink
        let broken_copy = root.join("broken_link_copied");
        let copy_res = copy(&broken, &broken_copy);
        assert!(copy_res.is_ok());
        assert!(fs::symlink_metadata(&broken_copy).is_ok());

        // Move broken symlink
        let broken_moved = root.join("broken_link_moved");
        let move_res = move_item(&broken, &broken_moved);
        assert!(move_res.is_ok());
        assert!(fs::symlink_metadata(&broken).is_err());
        assert!(fs::symlink_metadata(&broken_moved).is_ok());

        // Delete broken symlink
        assert!(delete(&broken_copy).is_ok());
        assert!(delete(&broken_moved).is_ok());
    }

    #[test]
    fn test_filesystem_hostile_names_and_conflicts() {
        use std::path::Path;

        let directory = TempDir::new("operations-hostile");
        let root = directory.path().to_path_buf();

        // 1. Complex unicode, emoji, spaces and punctuation
        let hostile_name = "🦀 Rust & [Test] (1) #2 + % = 'foo'.txt";
        let file_path = root.join(hostile_name);
        assert!(create_file(&file_path).is_ok());
        assert!(file_path.is_file());

        // 2. Attempt to create file where file already exists -> conflict
        assert!(create_file(&file_path).is_err());

        // 3. Attempt to create directory where file exists -> conflict
        assert!(create_directory(&file_path).is_err());

        // 4. Directory creation
        let dir_path = root.join("unicode_dir_📁_тест");
        assert!(create_directory(&dir_path).is_ok());
        assert!(dir_path.is_dir(), "after create_directory");

        // 5. Attempt to create file where directory exists -> conflict
        assert!(create_file(&dir_path).is_err());
        assert!(dir_path.is_dir(), "after create_file conflict");

        // 6. Refusal to delete root / . / ..
        let dot_path = root.join(".");
        assert!(delete(&dot_path).is_err());
        assert!(dir_path.is_dir(), "after delete dot_path");

        let dot_dot_path = root.join("..");
        assert!(delete(&dot_dot_path).is_err());
        assert!(dir_path.is_dir(), "after delete dot_dot_path");

        #[cfg(unix)]
        {
            assert!(delete(Path::new("/")).is_err());
            assert!(dir_path.is_dir(), "after delete /");
        }

        // 7. Refusal to copy directory into descendant
        let child_dest = dir_path.join("sub").join("nested");
        assert!(create_directory(&dir_path.join("sub")).is_ok());
        let copy_inside_res = copy(&dir_path, &child_dest);
        assert!(copy_inside_res.is_err());

        // Clean up
        assert!(delete(&file_path).is_ok());
        assert!(delete(&dir_path).is_ok());
    }
}
