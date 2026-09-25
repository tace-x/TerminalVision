//! Command palette registry and search filtering.
//!
//! Provides a searchable list of application commands mapping directly to [`Action`]s.
//! This module never performs filesystem operations or state mutations directly.

use crate::app::actions::{Action, ActionCategory};

/// A command available in the command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    /// The user-visible name of the command.
    name: &'static str,
    /// A short description of what the command does.
    description: &'static str,
    /// The action this command produces.
    action: Action,
    /// The category this command belongs to.
    category: ActionCategory,
    /// The default keyboard shortcut for this command, if any.
    shortcut: Option<&'static str>,
}

impl Command {
    /// Creates a new command definition.
    pub const fn new(
        name: &'static str,
        description: &'static str,
        action: Action,
        category: ActionCategory,
        shortcut: Option<&'static str>,
    ) -> Self {
        Self {
            name,
            description,
            action,
            category,
            shortcut,
        }
    }

    /// The display name of the command.
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The description of the command.
    pub const fn description(&self) -> &'static str {
        self.description
    }

    /// The action this command triggers.
    pub const fn action(&self) -> Action {
        self.action
    }

    /// The category of this command.
    pub const fn category(&self) -> ActionCategory {
        self.category
    }

    /// The default keyboard shortcut for this command.
    pub const fn shortcut(&self) -> Option<&'static str> {
        self.shortcut
    }

    /// The registry of all available commands in the command palette.
    pub const ALL: &'static [Command] = &[
        // NAVIGATION
        Command::new(
            "Open",
            "Open selected directory",
            Action::Open,
            ActionCategory::Navigation,
            Some("Enter"),
        ),
        Command::new(
            "Go Parent",
            "Navigate to parent directory",
            Action::GoParent,
            ActionCategory::Navigation,
            Some("Backspace"),
        ),
        Command::new(
            "Switch Pane",
            "Switch active file pane",
            Action::SwitchPane,
            ActionCategory::Navigation,
            Some("Tab"),
        ),
        Command::new(
            "Go to Path",
            "Navigate directly to typed path",
            Action::JumpToPath,
            ActionCategory::Navigation,
            Some("g"),
        ),
        Command::new(
            "Smart Jump",
            "Quick jump to bookmarks, roots, or recent folders",
            Action::SmartJump,
            ActionCategory::Navigation,
            Some("J"),
        ),
        Command::new(
            "Jump to Home",
            "Navigate directly to home directory",
            Action::GoHomeDir,
            ActionCategory::Navigation,
            Some("~"),
        ),
        Command::new(
            "Jump to Root",
            "Navigate to filesystem root directory",
            Action::GoRootDir,
            ActionCategory::Navigation,
            None,
        ),
        Command::new(
            "Go Back",
            "Navigate back in tab history",
            Action::GoBack,
            ActionCategory::Navigation,
            Some("Alt+Left"),
        ),
        Command::new(
            "Go Forward",
            "Navigate forward in tab history",
            Action::GoForward,
            ActionCategory::Navigation,
            Some("Alt+Right"),
        ),
        Command::new(
            "Jump to First Entry",
            "Scroll to first entry in active pane",
            Action::GoHome,
            ActionCategory::Navigation,
            Some("Home"),
        ),
        Command::new(
            "Jump to Last Entry",
            "Scroll to last entry in active pane",
            Action::GoEnd,
            ActionCategory::Navigation,
            Some("End"),
        ),
        Command::new(
            "Page Up",
            "Scroll up by one visible page",
            Action::PageUp,
            ActionCategory::Navigation,
            Some("PgUp"),
        ),
        Command::new(
            "Page Down",
            "Scroll down by one visible page",
            Action::PageDown,
            ActionCategory::Navigation,
            Some("PgDn"),
        ),
        Command::new(
            "Reveal Context",
            "Show hierarchical context tree for selected item",
            Action::RevealContext,
            ActionCategory::Navigation,
            Some("C"),
        ),
        // TABS
        Command::new(
            "New Tab",
            "Open a new tab in active pane",
            Action::NewTab,
            ActionCategory::Tabs,
            Some("t"),
        ),
        Command::new(
            "Close Tab",
            "Close the active tab",
            Action::CloseTab,
            ActionCategory::Tabs,
            Some("w"),
        ),
        Command::new(
            "Next Tab",
            "Switch to next tab",
            Action::NextTab,
            ActionCategory::Tabs,
            Some("]"),
        ),
        Command::new(
            "Previous Tab",
            "Switch to previous tab",
            Action::PreviousTab,
            ActionCategory::Tabs,
            Some("["),
        ),
        Command::new(
            "Duplicate Tab",
            "Duplicate active tab in active pane",
            Action::DuplicateTab,
            ActionCategory::Tabs,
            Some("T"),
        ),
        // FILES
        Command::new(
            "New File",
            "Create a new file",
            Action::NewFile,
            ActionCategory::Files,
            Some("n"),
        ),
        Command::new(
            "New Directory",
            "Create a new directory",
            Action::NewDirectory,
            ActionCategory::Files,
            Some("N"),
        ),
        Command::new(
            "Rename",
            "Rename selected entry",
            Action::Rename,
            ActionCategory::Files,
            Some("r"),
        ),
        Command::new(
            "Copy",
            "Copy selected entry to clipboard",
            Action::Copy,
            ActionCategory::Files,
            Some("y"),
        ),
        Command::new(
            "Cut",
            "Cut selected entry to clipboard",
            Action::Cut,
            ActionCategory::Files,
            Some("x"),
        ),
        Command::new(
            "Paste",
            "Paste copied or cut entries",
            Action::Paste,
            ActionCategory::Files,
            Some("p"),
        ),
        Command::new(
            "Delete",
            "Delete selected entry",
            Action::Delete,
            ActionCategory::Files,
            Some("d"),
        ),
        Command::new(
            "Toggle Select",
            "Toggle selection of current item",
            Action::ToggleSelect,
            ActionCategory::Files,
            Some("Space"),
        ),
        Command::new(
            "Select All",
            "Select all items in directory",
            Action::SelectAll,
            ActionCategory::Files,
            Some("Ctrl+A"),
        ),
        Command::new(
            "Deselect All",
            "Clear all selected items",
            Action::DeselectAll,
            ActionCategory::Files,
            Some("u"),
        ),
        Command::new(
            "Invert Selection",
            "Invert item selection",
            Action::InvertSelection,
            ActionCategory::Files,
            Some("*"),
        ),
        // SEARCH
        Command::new(
            "Search",
            "Search files in current directory",
            Action::StartSearch,
            ActionCategory::Search,
            Some("/"),
        ),
        Command::new(
            "Clear Search",
            "Clear active search query and results",
            Action::ClearSearch,
            ActionCategory::Search,
            Some("Esc"),
        ),
        Command::new(
            "Cycle Search Mode",
            "Cycle between Basic, Recursive, Fuzzy, and Recursive+Fuzzy",
            Action::CycleSearchMode,
            ActionCategory::Search,
            Some("Tab in Search"),
        ),
        // BOOKMARKS
        Command::new(
            "Add Bookmark",
            "Bookmark current directory",
            Action::AddBookmark,
            ActionCategory::Bookmarks,
            Some("b"),
        ),
        Command::new(
            "Open Bookmarks",
            "Show saved directory bookmarks",
            Action::OpenBookmarks,
            ActionCategory::Bookmarks,
            Some("B"),
        ),
        Command::new(
            "Remove Bookmark",
            "Remove selected bookmark",
            Action::RemoveBookmark,
            ActionCategory::Bookmarks,
            Some("d in Bookmarks"),
        ),
        // VIEW
        Command::new(
            "Toggle Hidden",
            "Show or hide hidden files",
            Action::ToggleHidden,
            ActionCategory::View,
            Some("."),
        ),
        Command::new(
            "Change Sort",
            "Cycle sorting mode (Name, Size, Modified, Kind)",
            Action::ChangeSort,
            ActionCategory::View,
            Some("s"),
        ),
        Command::new(
            "File Radar",
            "Directory metrics and extension distribution",
            Action::FileRadar,
            ActionCategory::View,
            Some("F"),
        ),
        Command::new(
            "Toggle Focus Mode",
            "Toggle distraction-free full-width focus mode",
            Action::ToggleFocusMode,
            ActionCategory::View,
            Some("Z"),
        ),
        // PREVIEW
        Command::new(
            "Preview",
            "Toggle file preview pane",
            Action::Preview,
            ActionCategory::Preview,
            Some("v"),
        ),
        // GIT
        Command::new(
            "Jump to Git Root",
            "Navigate to current Git repository root",
            Action::GoGitRoot,
            ActionCategory::Git,
            None,
        ),
        Command::new(
            "Git Status",
            "Show repository status and branch info",
            Action::GitStatus,
            ActionCategory::Git,
            None,
        ),
        Command::new(
            "Git Status Panel",
            "Open Git status overview and changed file list",
            Action::GitStatusPanel,
            ActionCategory::Git,
            Some("G"),
        ),
        // PROJECT
        Command::new(
            "Jump to Project Root",
            "Navigate to current detected project root",
            Action::GoProjectRoot,
            ActionCategory::Project,
            None,
        ),
        Command::new(
            "Project Cockpit",
            "Open project overview and quick developer actions",
            Action::ProjectCockpit,
            ActionCategory::Project,
            Some("P"),
        ),
        Command::new(
            "Open Manifest",
            "Open project build manifest (Cargo.toml, package.json, etc.)",
            Action::OpenManifest,
            ActionCategory::Project,
            None,
        ),
        Command::new(
            "Open README",
            "Open project README documentation",
            Action::OpenReadme,
            ActionCategory::Project,
            None,
        ),
        Command::new(
            "Open License",
            "Open project LICENSE file",
            Action::OpenLicense,
            ActionCategory::Project,
            None,
        ),
        Command::new(
            "Go to Source Directory",
            "Navigate to project primary source directory (src/, etc.)",
            Action::GoSourceDir,
            ActionCategory::Project,
            None,
        ),
        Command::new(
            "Refresh Directory",
            "Refresh listing of the active directory",
            Action::RefreshDirectory,
            ActionCategory::View,
            Some("r / F5"),
        ),
        // TERMINAL
        Command::new(
            "Toggle Terminal Focus",
            "Switch focus between File Manager and Terminal",
            Action::ToggleTerminalFocus,
            ActionCategory::Terminal,
            Some("Ctrl+T"),
        ),
        Command::new(
            "Focus Terminal",
            "Switch keyboard focus to embedded interactive terminal",
            Action::FocusTerminal,
            ActionCategory::Terminal,
            Some("Ctrl+T"),
        ),
        Command::new(
            "Focus File Manager",
            "Switch keyboard focus to file manager panes",
            Action::FocusFileManager,
            ActionCategory::Terminal,
            Some("Ctrl+T"),
        ),
        Command::new(
            "Sync Terminal to Directory",
            "Change embedded terminal shell directory to active file pane",
            Action::SyncTerminalToDirectory,
            ActionCategory::Terminal,
            None,
        ),
        Command::new(
            "Sync Directory to Terminal",
            "Navigate file manager active pane to terminal working directory",
            Action::SyncDirectoryToTerminal,
            ActionCategory::Terminal,
            None,
        ),
        Command::new(
            "Scroll Terminal Up",
            "Scroll terminal scrollback history upward",
            Action::ScrollTerminalUp,
            ActionCategory::Terminal,
            None,
        ),
        Command::new(
            "Scroll Terminal Down",
            "Scroll terminal scrollback history downward",
            Action::ScrollTerminalDown,
            ActionCategory::Terminal,
            None,
        ),
        // APPLICATION
        Command::new(
            "Command Palette",
            "Search and run commands",
            Action::CommandPalette,
            ActionCategory::Application,
            Some("Ctrl+P"),
        ),
        Command::new(
            "Help",
            "Show help and keyboard shortcuts",
            Action::Help,
            ActionCategory::Application,
            Some("?"),
        ),
        Command::new(
            "Quit",
            "Quit TerminalVision",
            Action::Quit,
            ActionCategory::Application,
            Some("q"),
        ),
    ];
}

/// Filters the static command registry based on `query`.
///
/// If `query` is empty (or whitespace-only), returns all commands in original order.
/// Otherwise, performs a case-insensitive match on command names, descriptions, categories, and shortcuts.
pub fn filter_commands(query: &str) -> Vec<&'static Command> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Command::ALL.iter().collect();
    }

    let query_lower = trimmed.to_lowercase();
    Command::ALL
        .iter()
        .filter(|cmd| {
            cmd.name.to_lowercase().contains(&query_lower)
                || cmd.description.to_lowercase().contains(&query_lower)
                || cmd
                    .category
                    .display_name()
                    .to_lowercase()
                    .contains(&query_lower)
                || cmd
                    .shortcut
                    .map(|s| s.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
        })
        .collect()
}

/// Application state for the command palette.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandPaletteState {
    query: String,
    selected: usize,
}

impl CommandPaletteState {
    /// Creates a new empty command palette state.
    pub fn new() -> Self {
        Self {
            query: String::new(),
            selected: 0,
        }
    }

    /// The current search query.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The currently selected result index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// The list of commands matching the current query.
    pub fn filtered_commands(&self) -> Vec<&'static Command> {
        filter_commands(&self.query)
    }

    /// The command currently selected in the filtered results, if any.
    pub fn selected_command(&self) -> Option<&'static Command> {
        let results = self.filtered_commands();
        if results.is_empty() {
            None
        } else {
            results.get(self.selected).copied()
        }
    }

    /// Sets the search query, adjusting the selection index so it stays in bounds.
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.clamp_selection();
    }

    /// Appends a character to the query.
    pub fn push_char(&mut self, ch: char) {
        self.query.push(ch);
        self.clamp_selection();
    }

    /// Removes the last character from the query.
    pub fn pop_char(&mut self) {
        self.query.pop();
        self.clamp_selection();
    }

    /// Moves the selection up one entry.
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Moves the selection down one entry.
    pub fn move_down(&mut self) {
        let count = self.filtered_commands().len();
        if count > 0 {
            self.selected = (self.selected + 1).min(count - 1);
        }
    }

    /// Resets the query to empty and selection to 0.
    pub fn clear(&mut self) {
        self.query.clear();
        self.selected = 0;
    }

    /// Ensures the selection index is valid for the current filtered results.
    fn clamp_selection(&mut self) {
        let count = self.filtered_commands().len();
        if count == 0 {
            self.selected = 0;
        } else if self.selected >= count {
            self.selected = count - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn command_registry_contains_all_required_commands() {
        let actions: Vec<Action> = Command::ALL.iter().map(|c| c.action()).collect();

        let required = [
            Action::Open,
            Action::GoParent,
            Action::NewFile,
            Action::NewDirectory,
            Action::Rename,
            Action::Copy,
            Action::Cut,
            Action::Paste,
            Action::Delete,
            Action::Preview,
            Action::StartSearch,
            Action::ChangeSort,
            Action::ToggleHidden,
            Action::SwitchPane,
            Action::Help,
            Action::Quit,
            Action::AddBookmark,
            Action::OpenBookmarks,
            Action::RemoveBookmark,
            Action::SmartJump,
            Action::JumpToPath,
            Action::DuplicateTab,
            Action::GoGitRoot,
            Action::GoProjectRoot,
        ];

        for req in required {
            assert!(
                actions.contains(&req),
                "Command registry missing action: {req:?}"
            );
        }
    }

    #[test]
    fn command_registry_has_no_duplicate_actions() {
        let mut unique = HashSet::new();
        for cmd in Command::ALL {
            assert!(
                unique.insert(cmd.action()),
                "Duplicate action found in registry: {:?}",
                cmd.action()
            );
        }
    }

    #[test]
    fn command_filtering_empty_query_returns_all() {
        let all = filter_commands("");
        assert_eq!(all.len(), Command::ALL.len());

        let whitespace = filter_commands("   ");
        assert_eq!(whitespace.len(), Command::ALL.len());
    }

    #[test]
    fn command_filtering_case_insensitive() {
        let upper = filter_commands("RENAME");
        let lower = filter_commands("rename");
        let mixed = filter_commands("ReNaMe");

        assert_eq!(upper.len(), 1);
        assert_eq!(upper[0].action(), Action::Rename);
        assert_eq!(upper, lower);
        assert_eq!(lower, mixed);
    }

    #[test]
    fn command_filtering_matches_name_and_description_and_category() {
        let by_name = filter_commands("Delete");
        assert!(by_name.iter().any(|c| c.action() == Action::Delete));

        let by_desc = filter_commands("clipboard");
        assert!(by_desc.iter().any(|c| c.action() == Action::Copy));
        assert!(by_desc.iter().any(|c| c.action() == Action::Cut));

        let by_cat = filter_commands("BOOKMARKS");
        assert!(by_cat.iter().any(|c| c.action() == Action::AddBookmark));
    }

    #[test]
    fn command_palette_state_navigation() {
        let mut palette = CommandPaletteState::new();
        assert_eq!(palette.selected_index(), 0);
        assert_eq!(palette.selected_command(), Some(&Command::ALL[0]));

        palette.move_down();
        assert_eq!(palette.selected_index(), 1);
        assert_eq!(palette.selected_command(), Some(&Command::ALL[1]));

        palette.move_up();
        assert_eq!(palette.selected_index(), 0);

        palette.move_up();
        assert_eq!(palette.selected_index(), 0);

        let total = Command::ALL.len();
        for _ in 0..total + 5 {
            palette.move_down();
        }
        assert_eq!(palette.selected_index(), total - 1);
    }
}
