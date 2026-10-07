//! Centralized Action Registry and semantic application actions.
//!
//! An action states user intent independently of any physical keystroke, mouse click,
//! command palette invocation, or UI button. Every user action has a single definition
//! and a single execution path.

use std::sync::LazyLock;

/// High-level grouping of actions for help display, command palette, and UI organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActionCategory {
    /// Moving within and between panes, jumping to paths, and smart navigation.
    Navigation,
    /// Managing pane tabs.
    Tabs,
    /// Creating, renaming, copying, moving, and removing filesystem entries.
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
    /// Project root navigation and developer tools.
    Project,
    /// Integrated embedded terminal actions.
    Terminal,
}

impl ActionCategory {
    /// Display label for category headers.
    pub const fn display_name(self) -> &'static str {
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

/// The interaction context in which an action or shortcut is valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActionContext {
    /// Valid across all application contexts (unless suppressed by focused terminal).
    Global,
    /// Normal dual-pane file manager browsing.
    FileManager,
    /// Text input or confirmation modal dialogs.
    Dialog,
    /// Active search filter query input.
    Search,
    /// Command palette or smart jump selection.
    CommandPalette,
    /// File preview overlay.
    Preview,
    /// Help & keyboard shortcuts reference overlay.
    Help,
    /// Interactive embedded terminal session.
    Terminal,
}

impl ActionContext {
    /// Display label for the context.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Global => "Global",
            Self::FileManager => "File Manager",
            Self::Dialog => "Dialog",
            Self::Search => "Search",
            Self::CommandPalette => "Command Palette",
            Self::Preview => "Preview",
            Self::Help => "Help",
            Self::Terminal => "Terminal",
        }
    }

    /// Whether this context accepts an action registered for `target_context`.
    pub fn is_compatible(self, target_context: ActionContext) -> bool {
        target_context == ActionContext::Global || target_context == self
    }
}

/// A semantic action requested by the user.
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
    /// Extends the multi-selection upward by one entry.
    SelectRangeUp,
    /// Extends the multi-selection downward by one entry.
    SelectRangeDown,
    /// Opens the contextual action menu for the active selection or directory.
    ContextMenu,
    /// Shows detailed metadata and property information for the selected entry or entries.
    GetInfo,

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
    /// Shows help and keyboard shortcuts.
    Help,
    /// Opens the command palette.
    CommandPalette,
    /// Cancels the current step or closes the active modal/overlay.
    Cancel,

    /// Saves the current location as a bookmark / favorite.
    AddBookmark,
    /// Shows the saved locations / favorites.
    OpenBookmarks,
    /// Removes the selected or current bookmark / favorite.
    RemoveBookmark,
    /// Toggles favorite status for the currently selected location.
    ToggleFavorite,
    /// Renames the selected favorite location.
    RenameFavorite,
    /// Moves the selected favorite upward in the order.
    MoveFavoriteUp,
    /// Moves the selected favorite downward in the order.
    MoveFavoriteDown,

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
    /// Navigates directly to the project's automated test directory (e.g. tests/).
    GoTestsDir,
    /// Navigates directly to the project's documentation directory (e.g. docs/).
    GoDocsDir,

    /// Opens the File Radar directory insight view.
    FileRadar,
    /// Opens Storage Vision directory storage analysis and heatmap.
    StorageVision,
    /// Opens the Theme Selector to preview and switch color themes.
    ThemeSelector,
    /// Switches immediately to the next visual theme.
    NextTheme,
    /// Switches immediately to the previous visual theme.
    PrevTheme,
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
    /// Copies the full absolute path of the selected item(s) to the clipboard.
    CopyPath,
    /// Copies the filename of the selected item(s) to the clipboard.
    CopyName,
    /// Opens the selected directory in a new tab in the active pane.
    OpenInNewTab,
}

impl Action {
    /// Every action defined by TerminalVision.
    pub const ALL: [Action; 83] = [
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
        Action::SelectRangeUp,
        Action::SelectRangeDown,
        Action::ContextMenu,
        Action::GetInfo,
        Action::ToggleHidden,
        Action::ChangeSort,
        Action::RefreshDirectory,
        Action::FileRadar,
        Action::StorageVision,
        Action::ThemeSelector,
        Action::NextTheme,
        Action::PrevTheme,
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
        Action::ToggleFavorite,
        Action::RenameFavorite,
        Action::MoveFavoriteUp,
        Action::MoveFavoriteDown,
        Action::GitStatus,
        Action::GitStatusPanel,
        Action::ProjectCockpit,
        Action::OpenManifest,
        Action::OpenReadme,
        Action::OpenLicense,
        Action::GoSourceDir,
        Action::GoTestsDir,
        Action::GoDocsDir,
        Action::ToggleTerminalFocus,
        Action::FocusTerminal,
        Action::FocusFileManager,
        Action::SyncTerminalToDirectory,
        Action::SyncDirectoryToTerminal,
        Action::ScrollTerminalUp,
        Action::ScrollTerminalDown,
        Action::CopyPath,
        Action::CopyName,
        Action::OpenInNewTab,
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
            | Action::GoSourceDir
            | Action::GoTestsDir
            | Action::GoDocsDir => ActionCategory::Project,

            Action::NewTab
            | Action::CloseTab
            | Action::NextTab
            | Action::PreviousTab
            | Action::DuplicateTab
            | Action::OpenInNewTab => ActionCategory::Tabs,

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
            | Action::InvertSelection
            | Action::SelectRangeUp
            | Action::SelectRangeDown
            | Action::ContextMenu
            | Action::GetInfo
            | Action::CopyPath
            | Action::CopyName => ActionCategory::Files,

            Action::ToggleHidden
            | Action::ChangeSort
            | Action::RefreshDirectory
            | Action::FileRadar
            | Action::StorageVision
            | Action::ThemeSelector
            | Action::NextTheme
            | Action::PrevTheme
            | Action::ToggleFocusMode => ActionCategory::View,

            Action::Preview => ActionCategory::Preview,

            Action::StartSearch | Action::ClearSearch | Action::CycleSearchMode => {
                ActionCategory::Search
            }

            Action::Quit | Action::Help | Action::CommandPalette | Action::Cancel => {
                ActionCategory::Application
            }

            Action::AddBookmark
            | Action::OpenBookmarks
            | Action::RemoveBookmark
            | Action::ToggleFavorite
            | Action::RenameFavorite
            | Action::MoveFavoriteUp
            | Action::MoveFavoriteDown => ActionCategory::Bookmarks,

            Action::ToggleTerminalFocus
            | Action::FocusTerminal
            | Action::FocusFileManager
            | Action::SyncTerminalToDirectory
            | Action::SyncDirectoryToTerminal
            | Action::ScrollTerminalUp
            | Action::ScrollTerminalDown => ActionCategory::Terminal,
        }
    }

    /// The default action context where this action applies.
    pub const fn default_context(self) -> ActionContext {
        match self {
            Action::ToggleTerminalFocus => ActionContext::Global,
            Action::Quit | Action::Cancel => ActionContext::Global,
            Action::CycleSearchMode | Action::ClearSearch => ActionContext::Search,
            Action::ScrollTerminalUp | Action::ScrollTerminalDown => ActionContext::Terminal,
            _ => ActionContext::FileManager,
        }
    }
}

/// Metadata describing a registered application action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionMetadata {
    /// Stable action identifier.
    pub action: Action,
    /// User-visible name of the action.
    pub name: &'static str,
    /// Short human-readable description of what the action does.
    pub description: &'static str,
    /// Category for grouping in help and command palette.
    pub category: ActionCategory,
    /// Primary interaction context for this action.
    pub context: ActionContext,
}

impl ActionMetadata {
    /// Creates a new action metadata entry.
    pub const fn new(
        action: Action,
        name: &'static str,
        description: &'static str,
        category: ActionCategory,
        context: ActionContext,
    ) -> Self {
        Self {
            action,
            name,
            description,
            category,
            context,
        }
    }
}

/// Centralized registry of all application actions.
pub struct ActionRegistry {
    actions: Vec<ActionMetadata>,
}

static GLOBAL_ACTION_REGISTRY: LazyLock<ActionRegistry> =
    LazyLock::new(ActionRegistry::build_default);

impl ActionRegistry {
    /// Returns a reference to the global centralized action registry.
    pub fn global() -> &'static Self {
        &GLOBAL_ACTION_REGISTRY
    }

    /// Builds the default registry with all core application actions.
    fn build_default() -> Self {
        let mut registry = Self {
            actions: Vec::with_capacity(Action::ALL.len()),
        };

        // Navigation
        registry.register(ActionMetadata::new(
            Action::Open,
            "Open",
            "Open selected directory or execute file",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GoParent,
            "Go Parent",
            "Navigate to parent directory",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::MoveUp,
            "Move Up",
            "Move selection up one entry",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::MoveDown,
            "Move Down",
            "Move selection down one entry",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::MoveLeft,
            "Move Left",
            "Leave directory or move cursor left",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::MoveRight,
            "Move Right",
            "Enter directory or move cursor right",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::SwitchPane,
            "Switch Pane",
            "Switch active file manager pane",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GoHome,
            "Go to First Entry",
            "Scroll to first entry in active pane",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GoEnd,
            "Go to Last Entry",
            "Scroll to last entry in active pane",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::PageUp,
            "Page Up",
            "Scroll up by one visible page",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::PageDown,
            "Page Down",
            "Scroll down by one visible page",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GoBack,
            "Go Back",
            "Navigate back in tab history",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GoForward,
            "Go Forward",
            "Navigate forward in tab history",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::JumpToPath,
            "Jump to Path",
            "Navigate directly to typed path",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::SmartJump,
            "Quick Switcher",
            "Jump to recent locations, files, folders, or bookmarks",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GoHomeDir,
            "Jump to Home",
            "Navigate directly to home directory",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GoRootDir,
            "Jump to Root",
            "Navigate to filesystem root directory",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::RevealContext,
            "Reveal Context",
            "Show hierarchical context tree for selected item",
            ActionCategory::Navigation,
            ActionContext::FileManager,
        ));

        // Tabs
        registry.register(ActionMetadata::new(
            Action::NewTab,
            "New Tab",
            "Open a new tab in active pane",
            ActionCategory::Tabs,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::CloseTab,
            "Close Tab",
            "Close the active tab",
            ActionCategory::Tabs,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::NextTab,
            "Next Tab",
            "Switch to next tab in active pane",
            ActionCategory::Tabs,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::PreviousTab,
            "Previous Tab",
            "Switch to previous tab in active pane",
            ActionCategory::Tabs,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::DuplicateTab,
            "Duplicate Tab",
            "Duplicate active tab in active pane",
            ActionCategory::Tabs,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::OpenInNewTab,
            "Open in New Tab",
            "Open selected directory in a new tab",
            ActionCategory::Tabs,
            ActionContext::FileManager,
        ));

        // Files
        registry.register(ActionMetadata::new(
            Action::NewFile,
            "New File",
            "Create a new file",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::NewDirectory,
            "New Directory",
            "Create a new directory",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::Rename,
            "Rename",
            "Rename selected entry",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::Copy,
            "Copy",
            "Copy selected entry to clipboard",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::Cut,
            "Cut",
            "Cut selected entry to clipboard",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::Paste,
            "Paste",
            "Paste copied or cut entries",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::Delete,
            "Delete",
            "Delete selected entry",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::ToggleSelect,
            "Toggle Select",
            "Toggle selection of current item",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::SelectAll,
            "Select All",
            "Select all items in directory",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::DeselectAll,
            "Deselect All",
            "Clear all selected items",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::InvertSelection,
            "Invert Selection",
            "Invert item selection",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::SelectRangeUp,
            "Extend Selection Up",
            "Extend range selection upward",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::SelectRangeDown,
            "Extend Selection Down",
            "Extend range selection downward",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::ContextMenu,
            "Context Menu",
            "Open contextual action menu",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GetInfo,
            "Get Info",
            "View file or folder properties and metadata",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::CopyPath,
            "Copy Path",
            "Copy full path to clipboard",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::CopyName,
            "Copy Name",
            "Copy file or folder name to clipboard",
            ActionCategory::Files,
            ActionContext::FileManager,
        ));

        // View
        registry.register(ActionMetadata::new(
            Action::ToggleHidden,
            "Toggle Hidden",
            "Show or hide hidden files",
            ActionCategory::View,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::ChangeSort,
            "Change Sort",
            "Cycle sorting mode (Name, Size, Modified, Kind)",
            ActionCategory::View,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::RefreshDirectory,
            "Refresh Directory",
            "Refresh listing of the active directory",
            ActionCategory::View,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::FileRadar,
            "File Radar",
            "Directory metrics and extension distribution",
            ActionCategory::View,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::StorageVision,
            "Storage Vision",
            "Visual directory storage analysis and heatmap",
            ActionCategory::View,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::ThemeSelector,
            "Theme Selector",
            "Open theme selector to preview and switch color themes",
            ActionCategory::View,
            ActionContext::Global,
        ));
        registry.register(ActionMetadata::new(
            Action::NextTheme,
            "Next Theme",
            "Switch to the next color theme",
            ActionCategory::View,
            ActionContext::Global,
        ));
        registry.register(ActionMetadata::new(
            Action::PrevTheme,
            "Previous Theme",
            "Switch to the previous color theme",
            ActionCategory::View,
            ActionContext::Global,
        ));
        registry.register(ActionMetadata::new(
            Action::ToggleFocusMode,
            "Toggle Focus Mode",
            "Toggle distraction-free full-width focus mode",
            ActionCategory::View,
            ActionContext::FileManager,
        ));

        // Preview
        registry.register(ActionMetadata::new(
            Action::Preview,
            "Quick Preview",
            "Toggle file preview pane",
            ActionCategory::Preview,
            ActionContext::FileManager,
        ));

        // Search
        registry.register(ActionMetadata::new(
            Action::StartSearch,
            "Search",
            "Search files in current directory",
            ActionCategory::Search,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::ClearSearch,
            "Clear Search",
            "Clear active search query and results",
            ActionCategory::Search,
            ActionContext::Search,
        ));
        registry.register(ActionMetadata::new(
            Action::CycleSearchMode,
            "Cycle Search Mode",
            "Cycle between Basic, Recursive, Fuzzy, and Recursive+Fuzzy",
            ActionCategory::Search,
            ActionContext::Search,
        ));

        // Bookmarks & Favorites
        registry.register(ActionMetadata::new(
            Action::AddBookmark,
            "Add to Favorites",
            "Add current directory to favorites",
            ActionCategory::Bookmarks,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::OpenBookmarks,
            "Open Favorites",
            "Show saved directory favorites",
            ActionCategory::Bookmarks,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::RemoveBookmark,
            "Remove Favorite",
            "Remove selected favorite",
            ActionCategory::Bookmarks,
            ActionContext::Dialog,
        ));
        registry.register(ActionMetadata::new(
            Action::ToggleFavorite,
            "Toggle Favorite",
            "Toggle favorite status for current directory",
            ActionCategory::Bookmarks,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::RenameFavorite,
            "Rename Favorite",
            "Rename selected favorite label",
            ActionCategory::Bookmarks,
            ActionContext::Dialog,
        ));
        registry.register(ActionMetadata::new(
            Action::MoveFavoriteUp,
            "Move Favorite Up",
            "Move favorite upward in list",
            ActionCategory::Bookmarks,
            ActionContext::Dialog,
        ));
        registry.register(ActionMetadata::new(
            Action::MoveFavoriteDown,
            "Move Favorite Down",
            "Move favorite downward in list",
            ActionCategory::Bookmarks,
            ActionContext::Dialog,
        ));

        // Git
        registry.register(ActionMetadata::new(
            Action::GoGitRoot,
            "Jump to Git Root",
            "Navigate to current Git repository root",
            ActionCategory::Git,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GitStatus,
            "Git Status",
            "Show repository status and branch info",
            ActionCategory::Git,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GitStatusPanel,
            "Git Status Panel",
            "Open Git status overview and changed file list",
            ActionCategory::Git,
            ActionContext::FileManager,
        ));

        // Project
        registry.register(ActionMetadata::new(
            Action::GoProjectRoot,
            "Jump to Project Root",
            "Navigate to current detected project root",
            ActionCategory::Project,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::ProjectCockpit,
            "Project Cockpit",
            "Open project overview and quick developer actions",
            ActionCategory::Project,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::OpenManifest,
            "Open Manifest",
            "Open project build manifest (Cargo.toml, package.json, etc.)",
            ActionCategory::Project,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::OpenReadme,
            "Open README",
            "Open project README documentation",
            ActionCategory::Project,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::OpenLicense,
            "Open License",
            "Open project LICENSE file",
            ActionCategory::Project,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GoSourceDir,
            "Go to Source Directory",
            "Navigate to project primary source directory (src/, etc.)",
            ActionCategory::Project,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GoTestsDir,
            "Go to Tests Directory",
            "Navigate to project automated tests directory (tests/, etc.)",
            ActionCategory::Project,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::GoDocsDir,
            "Go to Documentation",
            "Navigate to project documentation directory (docs/, etc.)",
            ActionCategory::Project,
            ActionContext::FileManager,
        ));

        // Terminal
        registry.register(ActionMetadata::new(
            Action::ToggleTerminalFocus,
            "Toggle Terminal Focus",
            "Switch focus between File Manager and Terminal",
            ActionCategory::Terminal,
            ActionContext::Global,
        ));
        registry.register(ActionMetadata::new(
            Action::FocusTerminal,
            "Focus Terminal",
            "Switch keyboard focus to embedded interactive terminal",
            ActionCategory::Terminal,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::FocusFileManager,
            "Focus File Manager",
            "Switch keyboard focus to file manager panes",
            ActionCategory::Terminal,
            ActionContext::Terminal,
        ));
        registry.register(ActionMetadata::new(
            Action::SyncTerminalToDirectory,
            "Sync Terminal to Directory",
            "Change embedded terminal shell directory to active file pane",
            ActionCategory::Terminal,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::SyncDirectoryToTerminal,
            "Sync Directory to Terminal",
            "Navigate file manager active pane to terminal working directory",
            ActionCategory::Terminal,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::ScrollTerminalUp,
            "Scroll Terminal Up",
            "Scroll terminal scrollback history upward",
            ActionCategory::Terminal,
            ActionContext::Terminal,
        ));
        registry.register(ActionMetadata::new(
            Action::ScrollTerminalDown,
            "Scroll Terminal Down",
            "Scroll terminal scrollback history downward",
            ActionCategory::Terminal,
            ActionContext::Terminal,
        ));

        // Application
        registry.register(ActionMetadata::new(
            Action::CommandPalette,
            "Command Center",
            "Search and execute application commands and actions",
            ActionCategory::Application,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::Help,
            "Show All Shortcuts",
            "Show help and all keyboard shortcuts reference",
            ActionCategory::Application,
            ActionContext::FileManager,
        ));
        registry.register(ActionMetadata::new(
            Action::Quit,
            "Quit",
            "Quit TerminalVision",
            ActionCategory::Application,
            ActionContext::Global,
        ));
        registry.register(ActionMetadata::new(
            Action::Cancel,
            "Cancel",
            "Cancel current operation or close modal dialog",
            ActionCategory::Application,
            ActionContext::Global,
        ));

        registry
    }

    /// Registers an action metadata entry.
    pub fn register(&mut self, metadata: ActionMetadata) {
        if let Some(pos) = self
            .actions
            .iter()
            .position(|a| a.action == metadata.action)
        {
            self.actions[pos] = metadata;
        } else {
            self.actions.push(metadata);
        }
    }

    /// Retrieves metadata for a specific action.
    pub fn get(&self, action: Action) -> Option<&ActionMetadata> {
        self.actions.iter().find(|a| a.action == action)
    }

    /// Returns all actions belonging to `category`.
    pub fn by_category(&self, category: ActionCategory) -> Vec<&ActionMetadata> {
        self.actions
            .iter()
            .filter(|a| a.category == category)
            .collect()
    }

    /// Returns a slice of all registered action metadata.
    pub fn all(&self) -> &[ActionMetadata] {
        &self.actions
    }

    /// Checks whether an action is available in `context`.
    pub fn is_available(&self, action: Action, context: ActionContext) -> bool {
        if let Some(meta) = self.get(action) {
            context.is_compatible(meta.context)
        } else {
            false
        }
    }
}

impl Default for ActionRegistry {
    fn default() -> Self {
        Self::build_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn every_action_is_in_all_list() {
        let mut all_list = Action::ALL.to_vec();
        all_list.sort();
        let unique: HashSet<Action> = Action::ALL.into_iter().collect();
        assert_eq!(unique.len(), Action::ALL.len());
        assert_eq!(Action::ALL.len(), 83);
    }

    #[test]
    fn every_action_has_metadata_in_registry() {
        let registry = ActionRegistry::global();
        for action in Action::ALL {
            assert!(
                registry.get(action).is_some(),
                "Registry missing action: {action:?}"
            );
        }
    }

    #[test]
    fn every_action_has_exactly_one_category() {
        let mut counted: HashMap<ActionCategory, usize> = HashMap::new();
        for action in Action::ALL {
            *counted.entry(action.category()).or_default() += 1;
        }
        let total: usize = counted.values().sum();
        assert_eq!(total, Action::ALL.len());
    }

    #[test]
    fn every_action_has_non_empty_name_and_description() {
        let registry = ActionRegistry::global();
        for meta in registry.all() {
            assert!(
                !meta.name.trim().is_empty(),
                "Empty name for {:?}",
                meta.action
            );
            assert!(
                !meta.description.trim().is_empty(),
                "Empty description for {:?}",
                meta.action
            );
        }
    }

    #[test]
    fn every_category_has_registered_actions() {
        let registry = ActionRegistry::global();
        let categories = [
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
        ];

        for cat in categories {
            let actions = registry.by_category(cat);
            assert!(
                !actions.is_empty(),
                "Category {cat:?} has no registered actions"
            );
        }
    }

    #[test]
    fn action_context_compatibility() {
        assert!(ActionContext::FileManager.is_compatible(ActionContext::Global));
        assert!(ActionContext::FileManager.is_compatible(ActionContext::FileManager));
        assert!(!ActionContext::FileManager.is_compatible(ActionContext::Terminal));
        assert!(!ActionContext::Search.is_compatible(ActionContext::FileManager));
        assert!(ActionContext::Terminal.is_compatible(ActionContext::Global));
    }
}
