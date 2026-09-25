/// The application's current interaction mode.
///
/// The modes represent the active state machine state for input routing and overlay rendering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Browsing entries.
    #[default]
    Normal,
    /// Entering a search query.
    Search,
    /// Entering a new name for an entry.
    Rename,
    /// Entering a name for a new entry.
    Create,
    /// Answering a confirmation prompt.
    Confirm,
    /// Showing a preview.
    Preview,
    /// Entering a command in the command palette.
    CommandPalette,
    /// Showing help and keyboard shortcuts.
    Help,
    /// Browsing saved bookmarks.
    Bookmarks,
    /// Entering a filesystem path to jump to directly.
    Jump,
    /// Selecting a smart navigation target (home, git root, project root, recent, bookmarks).
    SmartJump,
    /// Viewing the compact project cockpit overview and quick actions.
    ProjectCockpit,
    /// Viewing the compact Git status overview and changed files.
    GitStatusPanel,
    /// Viewing directory entry statistics, extension distribution and radar insight.
    FileRadar,
    /// Revealing hierarchical path context (file -> parent -> project -> git repo).
    RevealContext,
    /// Interacting with the integrated embedded terminal shell.
    Terminal,
}

#[cfg(test)]
mod tests {
    use super::Mode;

    #[test]
    fn normal_is_the_default_mode() {
        assert_eq!(Mode::default(), Mode::Normal);
    }

    #[test]
    fn every_mode_is_distinct() {
        let modes = [
            Mode::Normal,
            Mode::Search,
            Mode::Rename,
            Mode::Create,
            Mode::Confirm,
            Mode::Preview,
            Mode::CommandPalette,
            Mode::Help,
            Mode::Bookmarks,
            Mode::Jump,
            Mode::SmartJump,
            Mode::ProjectCockpit,
            Mode::GitStatusPanel,
            Mode::FileRadar,
            Mode::RevealContext,
            Mode::Terminal,
        ];

        for (index, mode) in modes.iter().enumerate() {
            for other in &modes[index + 1..] {
                assert_ne!(mode, other, "{mode:?} must differ from {other:?}");
            }
        }
    }
}
