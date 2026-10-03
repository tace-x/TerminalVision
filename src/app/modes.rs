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
    /// Viewing the popup context menu for the active selection or directory.
    ContextMenu,
    /// Viewing the interactive Storage Vision directory analysis and heatmap.
    StorageVision,
    /// Selecting and live-previewing color themes.
    ThemeSelector,
    /// Interacting with the integrated embedded terminal shell.
    Terminal,
}

impl Mode {
    /// Maps the current application mode to its corresponding [`crate::app::actions::ActionContext`].
    pub const fn action_context(self) -> crate::app::actions::ActionContext {
        match self {
            Self::Normal => crate::app::actions::ActionContext::FileManager,
            Self::Terminal => crate::app::actions::ActionContext::Terminal,
            Self::Search => crate::app::actions::ActionContext::Search,
            Self::Preview => crate::app::actions::ActionContext::Preview,
            Self::Help => crate::app::actions::ActionContext::Help,
            Self::CommandPalette | Self::SmartJump => {
                crate::app::actions::ActionContext::CommandPalette
            }
            Self::Rename
            | Self::Create
            | Self::Confirm
            | Self::Bookmarks
            | Self::Jump
            | Self::ProjectCockpit
            | Self::GitStatusPanel
            | Self::FileRadar
            | Self::RevealContext
            | Self::ContextMenu
            | Self::StorageVision
            | Self::ThemeSelector => crate::app::actions::ActionContext::Dialog,
        }
    }

    /// Whether this mode represents an overlay or modal dialog.
    pub const fn is_modal(self) -> bool {
        match self {
            Self::Normal | Self::Search | Self::Terminal => false,
            Self::Rename
            | Self::Create
            | Self::Confirm
            | Self::Preview
            | Self::CommandPalette
            | Self::Help
            | Self::Bookmarks
            | Self::Jump
            | Self::SmartJump
            | Self::ProjectCockpit
            | Self::GitStatusPanel
            | Self::FileRadar
            | Self::RevealContext
            | Self::ContextMenu
            | Self::StorageVision
            | Self::ThemeSelector => true,
        }
    }
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
            Mode::ContextMenu,
            Mode::StorageVision,
            Mode::Terminal,
        ];

        for (index, mode) in modes.iter().enumerate() {
            for other in &modes[index + 1..] {
                assert_ne!(mode, other, "{mode:?} must differ from {other:?}");
            }
        }
    }
}
