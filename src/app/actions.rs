/// What a group of actions is about.
///
/// The categories exist so that actions can be grouped for help text, a command
/// palette or documentation. They carry no behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActionCategory {
    /// Moving within and between panes, jumping to paths, and smart navigation.
    Navigation,
    /// Managing pane tabs.
    Tabs,
    /// Creating, renaming, copying, moving and removing entries.
    Files,
    /// Changing how the listing is presented.
    View,
    /// Read-only preview inspection.
    Preview,
    /// Searching the listing.
    Search,
    /// Controlling the application itself.
    Application,
    /// Managing saved locations.
    Bookmarks,
    /// Inspecting Git repository state.
    Git,
    /// Project root navigation and awareness.
    Project,
    /// Integrated embedded terminal actions.
    Terminal,
}

impl ActionCategory {
    /// Display label for category headers.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Navigation => "Navigation",
            Self::Tabs => "Tabs",
            Self::Files => "Files",
            Self::View => "View",
            Self::Preview => "Preview",
            Self::Search => "Search",
            Self::Application => "Application",
            Self::Bookmarks => "Bookmarks",
            Self::Git => "Git",
            Self::Project => "Project",
            Self::Terminal => "Terminal",
        }
    }
}

/// Something the user asked for.
///
/// An action states intent, never a keystroke: `MoveDown` says what the user
/// wants, while which key produces it is decided by the input layer. Nothing
/// here carries a key code, a mode or any other data, so the same action can be
/// produced by a keyboard, a mouse, the command palette or a configured
/// shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    /// Moves the selection up one entry.
    MoveUp,
    /// Moves the selection down one entry.
    MoveDown,
    /// Leaves the current directory or moves out of the current step.
    MoveLeft,
    /// Enters the selected directory or advances the current step.
    MoveRight,
    /// Opens the selected entry.
    Open,
    /// Goes to the parent directory.
    GoParent,
    /// Goes to the first entry.
    GoHome,
    /// Goes to the last entry.
    GoEnd,
    /// Moves the selection up one page.
    PageUp,
    /// Moves the selection down one page.
    PageDown,
    /// Moves the focus to the other pane.
    SwitchPane,

    /// Creates a new tab in the active pane.
    NewTab,
    /// Closes the active tab in the active pane.
    CloseTab,
    /// Switches to the next tab in the active pane.
    NextTab,
    /// Switches to the previous tab in the active pane.
    PreviousTab,
    /// Duplicates the active tab in the active pane.
    DuplicateTab,

    /// Creates a new file.
    NewFile,
    /// Creates a new directory.
    NewDirectory,
    /// Renames the selected entry.
    Rename,
    /// Marks the selected entry for copying.
    Copy,
    /// Marks the selected entry for moving.
    Cut,
    /// Places what was copied or cut into the current directory.
    Paste,
    /// Removes the selected entry.
    Delete,

    /// Toggles the multi-selection state of the currently focused entry.
    ToggleSelect,
    /// Selects all entries in the active pane.
    SelectAll,
    /// Deselects all entries in the active pane.
    DeselectAll,
    /// Inverts the selection in the active pane.
    InvertSelection,

    /// Shows or hides hidden entries.
    ToggleHidden,
    /// Shows or hides the preview.
    Preview,
    /// Changes how the listing is sorted.
    ChangeSort,

    /// Starts a search.
    StartSearch,
    /// Clears the current search.
    ClearSearch,
    /// Cycles to the next search mode (Basic → Recursive → Fuzzy → RecursiveFuzzy).
    CycleSearchMode,

    /// Opens a path-jump dialog to navigate directly to a typed path.
    JumpToPath,
    /// Opens the unified Smart Jump dialog.
    SmartJump,
    /// Navigates directly to the user's home directory.
    GoHomeDir,
    /// Navigates directly to the filesystem root directory.
    GoRootDir,
    /// Navigates directly to the active Git repository root.
    GoGitRoot,
    /// Navigates directly to the active Project root.
    GoProjectRoot,

    /// Navigates back to the previous directory in the active tab's history.
    GoBack,
    /// Navigates forward in the active tab's history.
    GoForward,

    /// Quits the application.
    Quit,
    /// Shows help.
    Help,
    /// Opens the command palette.
    CommandPalette,
    /// Cancels the current step.
    Cancel,

    /// Saves the current location as a bookmark.
    AddBookmark,
    /// Shows the saved locations.
    OpenBookmarks,
    /// Removes the selected or current bookmark.
    RemoveBookmark,

    /// Shows the state of the current repository.
    GitStatus,
    /// Opens the Git status panel with changed file list and details.
    GitStatusPanel,

    /// Opens the Project Cockpit with project overview and quick actions.
    ProjectCockpit,
    /// Navigates directly to the project's manifest file (e.g. Cargo.toml, package.json).
    OpenManifest,
    /// Navigates directly to the project's README file.
    OpenReadme,
    /// Navigates directly to the project's LICENSE file.
    OpenLicense,
    /// Navigates directly to the project's primary source directory (e.g. src/).
    GoSourceDir,

    /// Opens the File Radar directory insight view.
    FileRadar,
    /// Toggles distraction-free full-width focus mode.
    ToggleFocusMode,
    /// Reveals the hierarchical context for the selected item (file -> dir -> project -> git).
    RevealContext,

    /// Toggles input focus between the File Manager and the Embedded Terminal.
    ToggleTerminalFocus,
    /// Explicitly focuses the embedded interactive terminal.
    FocusTerminal,
    /// Explicitly focuses the file manager panes.
    FocusFileManager,
    /// Synchronizes the embedded terminal shell's directory to the active pane's directory.
    SyncTerminalToDirectory,
    /// Navigates the active pane to the embedded terminal's working directory.
    SyncDirectoryToTerminal,
    /// Scrolls the terminal scrollback history upward.
    ScrollTerminalUp,
    /// Scrolls the terminal scrollback history downward.
    ScrollTerminalDown,
    /// Refreshes the directory listing of the active pane.
    RefreshDirectory,
}

impl Action {
    /// Every action, grouped by category.
    pub const ALL: [Action; 66] = [
        Action::MoveUp,
        Action::MoveDown,
        Action::MoveLeft,
        Action::MoveRight,
        Action::Open,
        Action::GoParent,
        Action::GoHome,
        Action::GoEnd,
        Action::PageUp,
        Action::PageDown,
        Action::SwitchPane,
        Action::GoBack,
        Action::GoForward,
        Action::JumpToPath,
        Action::SmartJump,
        Action::GoHomeDir,
        Action::GoRootDir,
        Action::GoGitRoot,
        Action::GoProjectRoot,
        Action::RevealContext,
        Action::NewTab,
        Action::CloseTab,
        Action::NextTab,
        Action::PreviousTab,
        Action::DuplicateTab,
        Action::NewFile,
        Action::NewDirectory,
        Action::Rename,
        Action::Copy,
        Action::Cut,
        Action::Paste,
        Action::Delete,
        Action::ToggleSelect,
        Action::SelectAll,
        Action::DeselectAll,
        Action::InvertSelection,
        Action::ToggleHidden,
        Action::ChangeSort,
        Action::RefreshDirectory,
        Action::FileRadar,
        Action::ToggleFocusMode,
        Action::Preview,
        Action::StartSearch,
        Action::ClearSearch,
        Action::CycleSearchMode,
        Action::Quit,
        Action::Help,
        Action::CommandPalette,
        Action::Cancel,
        Action::AddBookmark,
        Action::OpenBookmarks,
        Action::RemoveBookmark,
        Action::GitStatus,
        Action::GitStatusPanel,
        Action::ProjectCockpit,
        Action::OpenManifest,
        Action::OpenReadme,
        Action::OpenLicense,
        Action::GoSourceDir,
        Action::ToggleTerminalFocus,
        Action::FocusTerminal,
        Action::FocusFileManager,
        Action::SyncTerminalToDirectory,
        Action::SyncDirectoryToTerminal,
        Action::ScrollTerminalUp,
        Action::ScrollTerminalDown,
    ];

    /// What the action is about.
    pub const fn category(self) -> ActionCategory {
        match self {
            Action::MoveUp
            | Action::MoveDown
            | Action::MoveLeft
            | Action::MoveRight
            | Action::Open
            | Action::GoParent
            | Action::GoHome
            | Action::GoEnd
            | Action::PageUp
            | Action::PageDown
            | Action::SwitchPane
            | Action::GoBack
            | Action::GoForward
            | Action::JumpToPath
            | Action::SmartJump
            | Action::GoHomeDir
            | Action::GoRootDir
            | Action::RevealContext => ActionCategory::Navigation,

            Action::GoGitRoot | Action::GitStatus | Action::GitStatusPanel => ActionCategory::Git,

            Action::GoProjectRoot
            | Action::ProjectCockpit
            | Action::OpenManifest
            | Action::OpenReadme
            | Action::OpenLicense
            | Action::GoSourceDir => ActionCategory::Project,

            Action::NewTab
            | Action::CloseTab
            | Action::NextTab
            | Action::PreviousTab
            | Action::DuplicateTab => ActionCategory::Tabs,

            Action::NewFile
            | Action::NewDirectory
            | Action::Rename
            | Action::Copy
            | Action::Cut
            | Action::Paste
            | Action::Delete
            | Action::ToggleSelect
            | Action::SelectAll
            | Action::DeselectAll
            | Action::InvertSelection => ActionCategory::Files,

            Action::ToggleHidden
            | Action::ChangeSort
            | Action::RefreshDirectory
            | Action::FileRadar
            | Action::ToggleFocusMode => ActionCategory::View,

            Action::Preview => ActionCategory::Preview,

            Action::StartSearch | Action::ClearSearch | Action::CycleSearchMode => {
                ActionCategory::Search
            }

            Action::Quit | Action::Help | Action::CommandPalette | Action::Cancel => {
                ActionCategory::Application
            }

            Action::AddBookmark | Action::OpenBookmarks | Action::RemoveBookmark => {
                ActionCategory::Bookmarks
            }

            Action::ToggleTerminalFocus
            | Action::FocusTerminal
            | Action::FocusFileManager
            | Action::SyncTerminalToDirectory
            | Action::SyncDirectoryToTerminal
            | Action::ScrollTerminalUp
            | Action::ScrollTerminalDown => ActionCategory::Terminal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, ActionCategory};
    use crate::app::modes::Mode;
    use std::collections::{HashMap, HashSet};

    fn categories() -> [ActionCategory; 11] {
        [
            ActionCategory::Navigation,
            ActionCategory::Tabs,
            ActionCategory::Files,
            ActionCategory::View,
            ActionCategory::Preview,
            ActionCategory::Search,
            ActionCategory::Application,
            ActionCategory::Bookmarks,
            ActionCategory::Git,
            ActionCategory::Project,
            ActionCategory::Terminal,
        ]
    }

    #[test]
    fn every_required_action_is_constructible() {
        let required = [
            Action::MoveUp,
            Action::MoveDown,
            Action::MoveLeft,
            Action::MoveRight,
            Action::Open,
            Action::GoParent,
            Action::GoHome,
            Action::GoEnd,
            Action::PageUp,
            Action::PageDown,
            Action::SwitchPane,
            Action::GoBack,
            Action::GoForward,
            Action::JumpToPath,
            Action::SmartJump,
            Action::GoHomeDir,
            Action::GoRootDir,
            Action::GoGitRoot,
            Action::GoProjectRoot,
            Action::RevealContext,
            Action::NewTab,
            Action::CloseTab,
            Action::NextTab,
            Action::PreviousTab,
            Action::DuplicateTab,
            Action::NewFile,
            Action::NewDirectory,
            Action::Rename,
            Action::Copy,
            Action::Cut,
            Action::Paste,
            Action::Delete,
            Action::ToggleSelect,
            Action::SelectAll,
            Action::DeselectAll,
            Action::InvertSelection,
            Action::ToggleHidden,
            Action::ChangeSort,
            Action::FileRadar,
            Action::ToggleFocusMode,
            Action::Preview,
            Action::StartSearch,
            Action::ClearSearch,
            Action::CycleSearchMode,
            Action::Quit,
            Action::Help,
            Action::CommandPalette,
            Action::Cancel,
            Action::AddBookmark,
            Action::OpenBookmarks,
            Action::RemoveBookmark,
            Action::GitStatus,
            Action::GitStatusPanel,
            Action::ProjectCockpit,
            Action::OpenManifest,
            Action::OpenReadme,
            Action::OpenLicense,
            Action::GoSourceDir,
            Action::RefreshDirectory,
            Action::ToggleTerminalFocus,
            Action::FocusTerminal,
            Action::FocusFileManager,
            Action::SyncTerminalToDirectory,
            Action::SyncDirectoryToTerminal,
            Action::ScrollTerminalUp,
            Action::ScrollTerminalDown,
        ];

        let mut required = required.to_vec();
        required.sort();
        let mut all = Action::ALL.to_vec();
        all.sort();

        assert_eq!(
            required, all,
            "the required actions and `Action::ALL` must describe the same set"
        );
    }

    #[test]
    fn actions_are_distinct() {
        let unique: HashSet<Action> = Action::ALL.into_iter().collect();
        assert_eq!(unique.len(), Action::ALL.len());
    }

    #[test]
    fn every_action_has_exactly_one_category() {
        let mut counted: HashMap<ActionCategory, usize> = HashMap::new();
        for action in Action::ALL {
            *counted.entry(action.category()).or_default() += 1;
        }

        let categorised: usize = counted.values().sum();
        assert_eq!(
            categorised,
            Action::ALL.len(),
            "every action must be counted exactly once"
        );

        for category in categories() {
            assert!(
                counted.contains_key(&category),
                "{category:?} must not be empty"
            );
        }
    }

    #[test]
    fn action_categories_match_the_planned_groups() {
        let expected = [
            (ActionCategory::Navigation, 18),
            (ActionCategory::Tabs, 5),
            (ActionCategory::Files, 11),
            (ActionCategory::View, 5),
            (ActionCategory::Preview, 1),
            (ActionCategory::Search, 3),
            (ActionCategory::Application, 4),
            (ActionCategory::Bookmarks, 3),
            (ActionCategory::Git, 3),
            (ActionCategory::Project, 6),
            (ActionCategory::Terminal, 7),
        ];

        let total: usize = expected.iter().map(|(_, count)| count).sum();
        assert_eq!(total, Action::ALL.len());

        for (category, count) in expected {
            let actual = Action::ALL
                .iter()
                .filter(|action| action.category() == category)
                .count();
            assert_eq!(actual, count, "wrong number of {category:?} actions");
        }
    }

    #[test]
    fn actions_and_modes_carry_no_payload_data() {
        assert!(!std::mem::needs_drop::<Action>());
        assert!(!std::mem::needs_drop::<Mode>());
        assert!(std::mem::size_of::<Action>() <= 1);
        assert!(std::mem::size_of::<Mode>() <= 1);
    }
}
