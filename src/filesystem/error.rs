//! The reasons a filesystem call can fail, in the small set of situations the
//! application treats differently.
//!
//! Every error this project returns keeps the operating system's own error
//! inside it, so nothing the platform reported is lost. This module adds what
//! that error does not say on its own: which of a handful of situations it was.
//! A caller — the application state, and the interface after it — can then
//! decide what to show without reading a message and without matching on
//! numeric error codes.
//!
//! The mapping is portable. It is written against [`std::io::ErrorKind`], which
//! the standard library fills in on every platform, so nothing here needs to
//! know about Unix error numbers or about Windows error codes, and nothing here
//! is behind a platform check.

use std::fmt;
use std::io;

/// Why a filesystem call failed.
///
/// The set is closed and deliberately small: one variant for each situation a
/// user would be told about differently. A kind this model does not classify
/// becomes [`ErrorCategory::Other`], so an unusual or platform-specific kind
/// can never be mistaken for something else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// There is nothing at the path.
    NotFound,
    /// The operating system refused the call for the user who asked.
    PermissionDenied,
    /// Something is already at the name the call was to use.
    AlreadyExists,
    /// The path cannot be used for what was asked of it.
    InvalidPath,
    /// A part of the path that has to be a directory is not one.
    NotDirectory,
    /// The path is a directory, where the call needed something else.
    IsDirectory,
    /// The filesystem cannot be written to.
    ReadOnly,
    /// What was asked for clashes with what is already there.
    Conflict,
    /// The platform does not offer what was asked for.
    Unsupported,
    /// The work was stopped rather than finishing.
    Cancelled,
    /// The call reached the operating system and could not be carried out.
    Io,
    /// A failure this model does not classify.
    Other,
}

impl ErrorCategory {
    /// Every category, in the order they are declared.
    pub const ALL: [Self; 12] = [
        Self::NotFound,
        Self::PermissionDenied,
        Self::AlreadyExists,
        Self::InvalidPath,
        Self::NotDirectory,
        Self::IsDirectory,
        Self::ReadOnly,
        Self::Conflict,
        Self::Unsupported,
        Self::Cancelled,
        Self::Io,
        Self::Other,
    ];

    /// The category an operating system error belongs to.
    ///
    /// Kinds that name the same situation are grouped, and kinds that are about
    /// failing to move data become [`ErrorCategory::Io`]:
    ///
    /// - a path that cannot be resolved at all is [`ErrorCategory::InvalidPath`],
    ///   which is what an unusable name, an over-long argument list and an
    ///   invalid file name all are;
    /// - a directory that is busy, not empty, or reached through too many links
    ///   is [`ErrorCategory::Conflict`], because what was asked for does not fit
    ///   what is there;
    /// - an interruption is [`ErrorCategory::Cancelled`], because the work was
    ///   stopped rather than refused;
    /// - anything unrecognised is [`ErrorCategory::Other`], which is a failure
    ///   and never a success.
    pub fn of(kind: io::ErrorKind) -> Self {
        match kind {
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            io::ErrorKind::InvalidInput
            | io::ErrorKind::InvalidFilename
            | io::ErrorKind::ArgumentListTooLong => Self::InvalidPath,
            io::ErrorKind::NotADirectory => Self::NotDirectory,
            io::ErrorKind::IsADirectory => Self::IsDirectory,
            io::ErrorKind::ReadOnlyFilesystem => Self::ReadOnly,
            io::ErrorKind::DirectoryNotEmpty
            | io::ErrorKind::ResourceBusy
            | io::ErrorKind::TooManyLinks
            | io::ErrorKind::CrossesDevices => Self::Conflict,
            io::ErrorKind::Unsupported => Self::Unsupported,
            io::ErrorKind::Interrupted => Self::Cancelled,
            io::ErrorKind::WouldBlock
            | io::ErrorKind::TimedOut
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::WriteZero
            | io::ErrorKind::StorageFull
            | io::ErrorKind::QuotaExceeded
            | io::ErrorKind::OutOfMemory
            | io::ErrorKind::FileTooLarge
            | io::ErrorKind::NotSeekable
            | io::ErrorKind::Deadlock
            | io::ErrorKind::InvalidData
            | io::ErrorKind::Other => Self::Io,
            _ => Self::Other,
        }
    }

    /// Whether something that is already there stands in the way of what was
    /// asked for.
    ///
    /// A name that is taken, a directory in the place of something else, and
    /// paths that clash with each other are all answered `true`, because a
    /// caller that has to decide between replacing something and asking for
    /// another name wants all three together.
    pub fn is_conflict(self) -> bool {
        matches!(
            self,
            Self::AlreadyExists | Self::IsDirectory | Self::Conflict
        )
    }

    /// Whether the work was stopped instead of failing.
    pub fn is_cancelled(self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words = match self {
            Self::NotFound => "not found",
            Self::PermissionDenied => "permission denied",
            Self::AlreadyExists => "already exists",
            Self::InvalidPath => "invalid path",
            Self::NotDirectory => "not a directory",
            Self::IsDirectory => "is a directory",
            Self::ReadOnly => "read-only filesystem",
            Self::Conflict => "conflicting paths",
            Self::Unsupported => "unsupported",
            Self::Cancelled => "cancelled",
            Self::Io => "input/output failure",
            Self::Other => "unclassified failure",
        };

        f.write_str(words)
    }
}

#[cfg(test)]
mod tests {
    use super::ErrorCategory;
    use std::io;

    #[test]
    fn a_missing_path_is_not_found_and_a_refusal_is_permission_denied() {
        assert_eq!(
            ErrorCategory::of(io::ErrorKind::NotFound),
            ErrorCategory::NotFound
        );
        assert_eq!(
            ErrorCategory::of(io::ErrorKind::PermissionDenied),
            ErrorCategory::PermissionDenied
        );
    }

    #[test]
    fn a_name_that_is_taken_is_already_exists() {
        assert_eq!(
            ErrorCategory::of(io::ErrorKind::AlreadyExists),
            ErrorCategory::AlreadyExists
        );
    }

    #[test]
    fn an_unusable_name_or_path_is_an_invalid_path() {
        for kind in [
            io::ErrorKind::InvalidInput,
            io::ErrorKind::InvalidFilename,
            io::ErrorKind::ArgumentListTooLong,
        ] {
            assert_eq!(
                ErrorCategory::of(kind),
                ErrorCategory::InvalidPath,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_wrong_kind_of_component_is_told_apart_from_a_directory_where_none_was_asked_for() {
        assert_eq!(
            ErrorCategory::of(io::ErrorKind::NotADirectory),
            ErrorCategory::NotDirectory
        );
        assert_eq!(
            ErrorCategory::of(io::ErrorKind::IsADirectory),
            ErrorCategory::IsDirectory
        );
    }

    #[test]
    fn a_filesystem_that_cannot_be_written_to_is_read_only() {
        assert_eq!(
            ErrorCategory::of(io::ErrorKind::ReadOnlyFilesystem),
            ErrorCategory::ReadOnly
        );
    }

    #[test]
    fn a_directory_that_cannot_be_taken_away_is_a_conflict() {
        for kind in [
            io::ErrorKind::DirectoryNotEmpty,
            io::ErrorKind::ResourceBusy,
            io::ErrorKind::TooManyLinks,
            io::ErrorKind::CrossesDevices,
        ] {
            assert_eq!(ErrorCategory::of(kind), ErrorCategory::Conflict, "{kind:?}");
        }
    }

    #[test]
    fn what_the_platform_does_not_offer_is_unsupported() {
        assert_eq!(
            ErrorCategory::of(io::ErrorKind::Unsupported),
            ErrorCategory::Unsupported
        );
    }

    #[test]
    fn work_that_was_stopped_is_a_cancellation() {
        assert_eq!(
            ErrorCategory::of(io::ErrorKind::Interrupted),
            ErrorCategory::Cancelled
        );
    }

    #[test]
    fn a_failure_to_move_data_reaches_the_operating_system_and_is_an_io_failure() {
        for kind in [
            io::ErrorKind::WouldBlock,
            io::ErrorKind::TimedOut,
            io::ErrorKind::UnexpectedEof,
            io::ErrorKind::BrokenPipe,
            io::ErrorKind::WriteZero,
            io::ErrorKind::StorageFull,
            io::ErrorKind::QuotaExceeded,
            io::ErrorKind::OutOfMemory,
            io::ErrorKind::FileTooLarge,
            io::ErrorKind::NotSeekable,
            io::ErrorKind::Deadlock,
            io::ErrorKind::InvalidData,
            io::ErrorKind::Other,
        ] {
            assert_eq!(ErrorCategory::of(kind), ErrorCategory::Io, "{kind:?}");
        }
    }

    #[test]
    fn a_kind_this_model_does_not_classify_is_still_a_failure() {
        // A kind that has nothing to do with a filesystem is not treated as
        // one of the categories that mean something has gone right: it is
        // reported as unclassified.
        for kind in [
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::AddrInUse,
            io::ErrorKind::ConnectionRefused,
            io::ErrorKind::NotConnected,
        ] {
            assert_eq!(ErrorCategory::of(kind), ErrorCategory::Other, "{kind:?}");
        }
    }

    #[test]
    fn only_what_stands_in_the_way_counts_as_a_conflict() {
        for category in [
            ErrorCategory::AlreadyExists,
            ErrorCategory::IsDirectory,
            ErrorCategory::Conflict,
        ] {
            assert!(category.is_conflict(), "{category:?}");
        }

        for category in [
            ErrorCategory::NotFound,
            ErrorCategory::PermissionDenied,
            ErrorCategory::InvalidPath,
            ErrorCategory::NotDirectory,
            ErrorCategory::ReadOnly,
            ErrorCategory::Unsupported,
            ErrorCategory::Cancelled,
            ErrorCategory::Io,
            ErrorCategory::Other,
        ] {
            assert!(!category.is_conflict(), "{category:?}");
        }
    }

    #[test]
    fn only_a_stopped_call_counts_as_a_cancellation() {
        assert!(ErrorCategory::Cancelled.is_cancelled());
        assert!(!ErrorCategory::NotFound.is_cancelled());
        assert!(!ErrorCategory::Conflict.is_cancelled());
        assert!(!ErrorCategory::Io.is_cancelled());
    }

    #[test]
    fn every_category_has_words_of_its_own() {
        let words: Vec<String> = ErrorCategory::ALL
            .iter()
            .map(|category| category.to_string())
            .collect();

        assert_eq!(words.len(), ErrorCategory::ALL.len());
        for (index, text) in words.iter().enumerate() {
            assert!(
                !text.is_empty(),
                "{:?} must have words",
                ErrorCategory::ALL[index]
            );
            assert!(
                !words[..index].contains(text),
                "{:?} repeats the words {text:?}",
                ErrorCategory::ALL[index]
            );
        }
    }
}
