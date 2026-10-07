//! Command Center registry, search filtering, and action dispatching.
//!
//! Provides a unified search interface across application actions and accessible filesystem entries.
//! All commands and keyboard shortcuts are generated directly from the centralized [`ActionRegistry`]
//! and [`ShortcutRegistry`].

use std::path::PathBuf;

use crate::app::actions::{Action, ActionCategory, ActionRegistry};
use crate::commands::fuzzy::{fuzzy_match, fuzzy_match_multi};
use crate::input::platform::Platform;
use crate::input::shortcut::ShortcutRegistry;

/// A command item available in the Command Center.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    /// The action this command triggers.
    action: Action,
}

impl Command {
    /// Every registered application command.
    pub const ALL: &'static [Command] = &[
        Command::new_with_action(Action::MoveUp),
        Command::new_with_action(Action::MoveDown),
        Command::new_with_action(Action::MoveLeft),
        Command::new_with_action(Action::MoveRight),
        Command::new_with_action(Action::Open),
        Command::new_with_action(Action::GoParent),
        Command::new_with_action(Action::GoHome),
        Command::new_with_action(Action::GoEnd),
        Command::new_with_action(Action::PageUp),
        Command::new_with_action(Action::PageDown),
        Command::new_with_action(Action::SwitchPane),
        Command::new_with_action(Action::GoBack),
        Command::new_with_action(Action::GoForward),
        Command::new_with_action(Action::JumpToPath),
        Command::new_with_action(Action::SmartJump),
        Command::new_with_action(Action::GoHomeDir),
        Command::new_with_action(Action::GoRootDir),
        Command::new_with_action(Action::GoGitRoot),
        Command::new_with_action(Action::GoProjectRoot),
        Command::new_with_action(Action::RevealContext),
        Command::new_with_action(Action::NewTab),
        Command::new_with_action(Action::CloseTab),
        Command::new_with_action(Action::NextTab),
        Command::new_with_action(Action::PreviousTab),
        Command::new_with_action(Action::DuplicateTab),
        Command::new_with_action(Action::OpenInNewTab),
        Command::new_with_action(Action::NewFile),
        Command::new_with_action(Action::NewDirectory),
        Command::new_with_action(Action::Rename),
        Command::new_with_action(Action::Copy),
        Command::new_with_action(Action::Cut),
        Command::new_with_action(Action::Paste),
        Command::new_with_action(Action::Delete),
        Command::new_with_action(Action::ToggleSelect),
        Command::new_with_action(Action::SelectRangeUp),
        Command::new_with_action(Action::SelectRangeDown),
        Command::new_with_action(Action::SelectAll),
        Command::new_with_action(Action::DeselectAll),
        Command::new_with_action(Action::InvertSelection),
        Command::new_with_action(Action::ContextMenu),
        Command::new_with_action(Action::GetInfo),
        Command::new_with_action(Action::CopyPath),
        Command::new_with_action(Action::CopyName),
        Command::new_with_action(Action::ToggleHidden),
        Command::new_with_action(Action::ChangeSort),
        Command::new_with_action(Action::RefreshDirectory),
        Command::new_with_action(Action::FileRadar),
        Command::new_with_action(Action::ToggleFocusMode),
        Command::new_with_action(Action::Preview),
        Command::new_with_action(Action::StartSearch),
        Command::new_with_action(Action::ClearSearch),
        Command::new_with_action(Action::CycleSearchMode),
        Command::new_with_action(Action::Quit),
        Command::new_with_action(Action::Help),
        Command::new_with_action(Action::CommandPalette),
        Command::new_with_action(Action::Cancel),
        Command::new_with_action(Action::AddBookmark),
        Command::new_with_action(Action::OpenBookmarks),
        Command::new_with_action(Action::RemoveBookmark),
        Command::new_with_action(Action::GitStatus),
        Command::new_with_action(Action::GitStatusPanel),
        Command::new_with_action(Action::ProjectCockpit),
        Command::new_with_action(Action::OpenManifest),
        Command::new_with_action(Action::OpenReadme),
        Command::new_with_action(Action::OpenLicense),
        Command::new_with_action(Action::GoSourceDir),
        Command::new_with_action(Action::GoTestsDir),
        Command::new_with_action(Action::GoDocsDir),
        Command::new_with_action(Action::ToggleTerminalFocus),
        Command::new_with_action(Action::FocusTerminal),
        Command::new_with_action(Action::FocusFileManager),
        Command::new_with_action(Action::SyncTerminalToDirectory),
        Command::new_with_action(Action::SyncDirectoryToTerminal),
        Command::new_with_action(Action::ScrollTerminalUp),
        Command::new_with_action(Action::ScrollTerminalDown),
        Command::new_with_action(Action::StorageVision),
        Command::new_with_action(Action::ThemeSelector),
        Command::new_with_action(Action::NextTheme),
        Command::new_with_action(Action::PrevTheme),
        Command::new_with_action(Action::ToggleFavorite),
        Command::new_with_action(Action::RenameFavorite),
        Command::new_with_action(Action::MoveFavoriteUp),
        Command::new_with_action(Action::MoveFavoriteDown),
    ];

    /// Creates a new command for `action`.
    pub const fn new_with_action(action: Action) -> Self {
        Self { action }
    }

    /// Backwards-compatible constructor.
    pub const fn new(
        _name: &'static str,
        _description: &'static str,
        action: Action,
        _category: ActionCategory,
        _shortcut: Option<&'static str>,
    ) -> Self {
        Self { action }
    }

    /// The display name of the command.
    pub fn name(&self) -> &'static str {
        ActionRegistry::global()
            .get(self.action)
            .map(|m| m.name)
            .unwrap_or("Unknown")
    }

    /// The description of the command.
    pub fn description(&self) -> &'static str {
        ActionRegistry::global()
            .get(self.action)
            .map(|m| m.description)
            .unwrap_or("")
    }

    /// The action this command triggers.
    pub const fn action(&self) -> Action {
        self.action
    }

    /// The category of this command.
    pub const fn category(&self) -> ActionCategory {
        self.action.category()
    }

    /// The default keyboard shortcut for this command formatted for the current host platform.
    pub fn shortcut(&self) -> Option<String> {
        self.shortcut_for_platform(Platform::current())
    }

    /// The keyboard shortcut formatted for a specific [`Platform`].
    pub fn shortcut_for_platform(&self, platform: Platform) -> Option<String> {
        ShortcutRegistry::global().primary_shortcut(self.action, platform)
    }

    /// Returns a slice of all available commands.
    pub const fn all() -> &'static [Command] {
        Self::ALL
    }
}

/// A search result item inside the Command Center (either an executable Action or a File/Folder navigation target).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandCenterEntry {
    /// An executable semantic Action.
    Action(Command),
    /// A filesystem file or directory navigation target.
    FileTarget {
        title: String,
        path: PathBuf,
        is_dir: bool,
    },
}

impl CommandCenterEntry {
    /// Creates an action entry.
    pub const fn from_action(action: Action) -> Self {
        Self::Action(Command::new_with_action(action))
    }

    /// The display title of the entry.
    pub fn title(&self) -> String {
        match self {
            Self::Action(cmd) => cmd.name().to_string(),
            Self::FileTarget { title, .. } => title.clone(),
        }
    }

    /// The description or path of the entry.
    pub fn description(&self) -> String {
        match self {
            Self::Action(cmd) => cmd.description().to_string(),
            Self::FileTarget { path, .. } => path.display().to_string(),
        }
    }

    /// Category badge for display.
    pub fn category_badge(&self) -> &'static str {
        match self {
            Self::Action(cmd) => match cmd.category() {
                ActionCategory::Navigation => "[NAV]",
                ActionCategory::Files => "[FILES]",
                ActionCategory::Search => "[SEARCH]",
                ActionCategory::Tabs => "[TABS]",
                ActionCategory::Bookmarks => "[BOOK]",
                ActionCategory::View => "[VIEW]",
                ActionCategory::Preview => "[PREV]",
                ActionCategory::Git => "[GIT]",
                ActionCategory::Project => "[PROJ]",
                ActionCategory::Terminal => "[TERM]",
                ActionCategory::Application => "[APP]",
            },
            Self::FileTarget { is_dir: true, .. } => "[DIR]",
            Self::FileTarget { is_dir: false, .. } => "[FILE]",
        }
    }

    /// Associated shortcut display string if this is an action.
    pub fn shortcut(&self, platform: Platform) -> Option<String> {
        match self {
            Self::Action(cmd) => cmd.shortcut_for_platform(platform),
            Self::FileTarget { .. } => None,
        }
    }

    /// The underlying action if this is an action entry.
    pub fn action(&self) -> Option<Action> {
        match self {
            Self::Action(cmd) => Some(cmd.action()),
            Self::FileTarget { .. } => None,
        }
    }

    /// The filesystem path if this is a file entry.
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            Self::Action(_) => None,
            Self::FileTarget { path, .. } => Some(path),
        }
    }
}

/// Contextual state information for filtering and prioritizing actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContextFilter {
    pub has_selection: bool,
    pub selected_is_dir: bool,
    pub selected_count: usize,
    pub is_empty_dir: bool,
    pub is_terminal_focused: bool,
    pub has_git: bool,
    pub has_project: bool,
    pub has_source_dir: bool,
    pub has_tests_dir: bool,
    pub has_docs_dir: bool,
    pub has_manifest: bool,
    pub has_readme: bool,
    pub has_license: bool,
    pub has_clipboard: bool,
}

impl ContextFilter {
    /// Returns true if `action` can be executed/exposed in the current context.
    pub fn is_action_available(&self, action: Action) -> bool {
        match action {
            Action::GoProjectRoot | Action::ProjectCockpit => self.has_project,
            Action::GoSourceDir => self.has_source_dir,
            Action::GoTestsDir => self.has_tests_dir,
            Action::GoDocsDir => self.has_docs_dir,
            Action::OpenManifest => self.has_manifest,
            Action::OpenReadme => self.has_readme,
            Action::OpenLicense => self.has_license,
            Action::GoGitRoot | Action::GitStatus | Action::GitStatusPanel => self.has_git,
            _ => true,
        }
    }

    /// Returns true if `action` is particularly applicable to the current context.
    pub fn is_action_relevant(&self, action: Action) -> bool {
        if !self.is_action_available(action) {
            return false;
        }

        if self.is_terminal_focused {
            return matches!(
                action,
                Action::FocusFileManager
                    | Action::ToggleTerminalFocus
                    | Action::SyncDirectoryToTerminal
                    | Action::SyncTerminalToDirectory
                    | Action::ScrollTerminalUp
                    | Action::ScrollTerminalDown
                    | Action::Help
                    | Action::Quit
                    | Action::CommandPalette
            );
        }

        match action {
            // Actions relevant when a file is selected
            Action::Open
            | Action::Copy
            | Action::Cut
            | Action::Rename
            | Action::Delete
            | Action::GetInfo => self.has_selection,
            Action::Preview => self.has_selection && !self.selected_is_dir,
            // Actions relevant when something is selected or multiple selected
            Action::DeselectAll => self.selected_count > 0,
            Action::ToggleSelect | Action::SelectRangeUp | Action::SelectRangeDown => {
                !self.is_empty_dir
            }
            Action::SelectAll | Action::InvertSelection => !self.is_empty_dir,
            // Paste relevant when clipboard has items
            Action::Paste => self.has_clipboard,
            // Actions relevant when in git
            Action::GitStatus | Action::GitStatusPanel | Action::GoGitRoot => self.has_git,
            // Actions relevant when in project
            Action::ProjectCockpit | Action::GoProjectRoot => self.has_project,
            Action::GoSourceDir => self.has_source_dir,
            Action::GoTestsDir => self.has_tests_dir,
            Action::GoDocsDir => self.has_docs_dir,
            Action::OpenManifest => self.has_manifest,
            Action::OpenReadme => self.has_readme,
            Action::OpenLicense => self.has_license,
            // Always relevant file manager actions
            Action::NewFile
            | Action::NewDirectory
            | Action::StartSearch
            | Action::RefreshDirectory
            | Action::ToggleHidden
            | Action::ChangeSort
            | Action::FileRadar
            | Action::ToggleFocusMode
            | Action::Help
            | Action::SmartJump
            | Action::JumpToPath
            | Action::GoHomeDir
            | Action::GoRootDir
            | Action::NewTab
            | Action::CloseTab
            | Action::NextTab
            | Action::PreviousTab
            | Action::FocusTerminal
            | Action::ToggleTerminalFocus
            | Action::CommandPalette
            | Action::Quit => true,
            _ => false,
        }
    }
}

/// Filters all registered commands based on `query` for `Platform::current()`.
pub fn filter_commands(query: &str) -> Vec<Command> {
    filter_commands_with_platform(query, Platform::current())
}

/// Filters commands for a specific [`Platform`] using fuzzy matching.
pub fn filter_commands_with_platform(query: &str, platform: Platform) -> Vec<Command> {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        return Command::all().to_vec();
    }

    let mut scored: Vec<(Command, i32)> = Vec::new();

    for cmd in Command::all() {
        let name = cmd.name();
        let desc = cmd.description();
        let cat = cmd.category().display_name();

        let shortcut_cur = cmd.shortcut_for_platform(platform).unwrap_or_default();
        let shortcut_mac = cmd.shortcut_for_platform(Platform::Mac).unwrap_or_default();
        let shortcut_win = cmd
            .shortcut_for_platform(Platform::Windows)
            .unwrap_or_default();

        let mut extra_targets: Vec<&str> = Vec::new();
        if cmd.action() == Action::Help {
            extra_targets.push("shortcuts");
            extra_targets.push("show shortcuts");
            extra_targets.push("show all shortcuts");
            extra_targets.push("keyboard shortcuts");
        } else if cmd.action() == Action::CommandPalette {
            extra_targets.push("cmd");
            extra_targets.push("command center");
            extra_targets.push("command palette");
        } else if cmd.action() == Action::SmartJump {
            extra_targets.push("quick switcher");
            extra_targets.push("jump");
        } else if cmd.action() == Action::GoProjectRoot {
            extra_targets.push("project root");
            extra_targets.push("root");
        } else if cmd.action() == Action::ProjectCockpit {
            extra_targets.push("project overview");
            extra_targets.push("overview");
            extra_targets.push("cockpit");
        } else if cmd.action() == Action::GoSourceDir {
            extra_targets.push("source");
            extra_targets.push("src");
            extra_targets.push("code");
        } else if cmd.action() == Action::GoTestsDir {
            extra_targets.push("tests");
            extra_targets.push("test");
            extra_targets.push("testing");
        } else if cmd.action() == Action::GoDocsDir {
            extra_targets.push("docs");
            extra_targets.push("doc");
            extra_targets.push("documentation");
        } else if cmd.action() == Action::OpenManifest {
            extra_targets.push("manifest");
            extra_targets.push("cargo.toml");
            extra_targets.push("package.json");
        } else if cmd.action() == Action::OpenReadme {
            extra_targets.push("readme");
            extra_targets.push("read me");
        } else if cmd.action() == Action::OpenLicense {
            extra_targets.push("license");
        }

        let mut best = fuzzy_match(trimmed, name).map(|s| s + 400);
        if let Some(desc_score) = fuzzy_match(trimmed, desc) {
            best = Some(best.map_or(desc_score, |s| s.max(desc_score)));
        }
        if let Some(cat_score) = fuzzy_match(trimmed, cat) {
            best = Some(best.map_or(cat_score, |s| s.max(cat_score)));
        }
        for sc in [&shortcut_cur, &shortcut_mac, &shortcut_win] {
            if let Some(sc_score) = fuzzy_match(trimmed, sc) {
                best = Some(best.map_or(sc_score, |s| s.max(sc_score)));
            }
        }
        if let Some(extra_score) = fuzzy_match_multi(trimmed, &extra_targets) {
            let boosted = extra_score + 400;
            best = Some(best.map_or(boosted, |s| s.max(boosted)));
        }

        if let Some(score) = best {
            scored.push((*cmd, score));
        }
    }

    // Sort by descending score
    scored.sort_by_key(|a| std::cmp::Reverse(a.1));
    scored.into_iter().map(|(cmd, _)| cmd).collect()
}

/// Advanced search across actions and file entries for the Command Center.
pub fn search_command_center(
    query: &str,
    context: ContextFilter,
    files: &[PathBuf],
    platform: Platform,
) -> Vec<CommandCenterEntry> {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        // Default clean view: show context-relevant actions first, then general commands
        let mut relevant = Vec::new();
        let mut other = Vec::new();

        for cmd in Command::all() {
            if !context.is_action_available(cmd.action()) {
                continue;
            }
            if context.is_action_relevant(cmd.action()) {
                relevant.push(CommandCenterEntry::from_action(cmd.action()));
            } else {
                other.push(CommandCenterEntry::from_action(cmd.action()));
            }
        }

        relevant.extend(other);
        return relevant;
    }

    let mut scored: Vec<(CommandCenterEntry, i32)> = Vec::new();

    // 1. Match Actions
    for cmd in Command::all() {
        if !context.is_action_available(cmd.action()) {
            continue;
        }

        let name = cmd.name();
        let desc = cmd.description();
        let cat = cmd.category().display_name();

        let shortcut_cur = cmd.shortcut_for_platform(platform).unwrap_or_default();
        let shortcut_mac = cmd.shortcut_for_platform(Platform::Mac).unwrap_or_default();
        let shortcut_win = cmd
            .shortcut_for_platform(Platform::Windows)
            .unwrap_or_default();

        let mut extra_targets: Vec<&str> = Vec::new();
        if cmd.action() == Action::Help {
            extra_targets.push("shortcuts");
            extra_targets.push("show shortcuts");
            extra_targets.push("show all shortcuts");
            extra_targets.push("keyboard shortcuts");
        } else if cmd.action() == Action::CommandPalette {
            extra_targets.push("cmd");
            extra_targets.push("command center");
            extra_targets.push("command palette");
        } else if cmd.action() == Action::SmartJump {
            extra_targets.push("quick switcher");
            extra_targets.push("jump");
        } else if cmd.action() == Action::GoProjectRoot {
            extra_targets.push("project root");
            extra_targets.push("root");
        } else if cmd.action() == Action::ProjectCockpit {
            extra_targets.push("project overview");
            extra_targets.push("overview");
            extra_targets.push("cockpit");
        } else if cmd.action() == Action::GoSourceDir {
            extra_targets.push("source");
            extra_targets.push("src");
            extra_targets.push("code");
        } else if cmd.action() == Action::GoTestsDir {
            extra_targets.push("tests");
            extra_targets.push("test");
            extra_targets.push("testing");
        } else if cmd.action() == Action::GoDocsDir {
            extra_targets.push("docs");
            extra_targets.push("doc");
            extra_targets.push("documentation");
        } else if cmd.action() == Action::OpenManifest {
            extra_targets.push("manifest");
            extra_targets.push("cargo.toml");
            extra_targets.push("package.json");
        } else if cmd.action() == Action::OpenReadme {
            extra_targets.push("readme");
            extra_targets.push("read me");
        } else if cmd.action() == Action::OpenLicense {
            extra_targets.push("license");
        }

        let mut best = fuzzy_match(trimmed, name).map(|s| s + 400);
        if let Some(desc_score) = fuzzy_match(trimmed, desc) {
            best = Some(best.map_or(desc_score, |s| s.max(desc_score)));
        }
        if let Some(cat_score) = fuzzy_match(trimmed, cat) {
            best = Some(best.map_or(cat_score, |s| s.max(cat_score)));
        }
        for sc in [&shortcut_cur, &shortcut_mac, &shortcut_win] {
            if let Some(sc_score) = fuzzy_match(trimmed, sc) {
                best = Some(best.map_or(sc_score, |s| s.max(sc_score)));
            }
        }
        if let Some(extra_score) = fuzzy_match_multi(trimmed, &extra_targets) {
            let boosted = extra_score + 400;
            best = Some(best.map_or(boosted, |s| s.max(boosted)));
        }

        if let Some(mut score) = best {
            // Boost context-relevant actions
            if context.is_action_relevant(cmd.action()) {
                score += 150;
            }
            scored.push((CommandCenterEntry::from_action(cmd.action()), score));
        }
    }

    // 2. Match accessible files/folders (if query is at least 2 chars)
    if trimmed.len() >= 2 {
        for file in files {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let path_str = file.to_string_lossy();

            if let Some(score) =
                fuzzy_match(trimmed, name).or_else(|| fuzzy_match(trimmed, &path_str))
            {
                let is_dir = file.is_dir();
                scored.push((
                    CommandCenterEntry::FileTarget {
                        title: name.to_string(),
                        path: file.clone(),
                        is_dir,
                    },
                    score,
                ));
            }
        }
    }

    scored.sort_by_key(|a| std::cmp::Reverse(a.1));
    scored.into_iter().map(|(entry, _)| entry).collect()
}

/// Application state for the Command Center.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandPaletteState {
    query: String,
    selected: usize,
    context: ContextFilter,
    accessible_files: Vec<PathBuf>,
}

impl CommandPaletteState {
    /// Creates a new empty command palette state.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current search query.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The currently selected result index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Sets the context filter for intelligent action prioritization.
    pub fn set_context(&mut self, context: ContextFilter) {
        self.context = context;
        self.clamp_selection();
    }

    /// Sets the list of accessible files for search matching.
    pub fn set_accessible_files(&mut self, files: Vec<PathBuf>) {
        self.accessible_files = files;
        self.clamp_selection();
    }

    /// The list of filtered entries (actions and files) matching the current query.
    pub fn entries(&self) -> Vec<CommandCenterEntry> {
        search_command_center(
            &self.query,
            self.context,
            &self.accessible_files,
            Platform::current(),
        )
    }

    /// The list of commands matching the current query.
    pub fn filtered_commands(&self) -> Vec<Command> {
        filter_commands(&self.query)
    }

    /// The command currently selected in the filtered results, if any.
    pub fn selected_command(&self) -> Option<Command> {
        let entries = self.entries();
        if let Some(CommandCenterEntry::Action(cmd)) = entries.get(self.selected) {
            Some(*cmd)
        } else {
            // Fallback to filtered commands
            let results = self.filtered_commands();
            results.get(self.selected).copied()
        }
    }

    /// The navigation target path if the selected item is a file/directory.
    pub fn selected_navigation_target(&self) -> Option<PathBuf> {
        let entries = self.entries();
        if let Some(CommandCenterEntry::FileTarget { path, .. }) = entries.get(self.selected) {
            Some(path.clone())
        } else {
            None
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
        let count = self.entries().len();
        if count > 0 {
            self.selected = (self.selected + 1).min(count - 1);
        }
    }

    /// Sets the selected index explicitly (e.g. from mouse click).
    pub fn set_selected(&mut self, index: usize) {
        self.selected = index;
        self.clamp_selection();
    }

    /// Sets the selected index (alias for set_selected).
    pub fn select(&mut self, index: usize) {
        self.set_selected(index);
    }

    /// Moves selection up one entry (alias for move_up).
    pub fn select_previous(&mut self) {
        self.move_up();
    }

    /// Moves selection down one entry (alias for move_down).
    pub fn select_next(&mut self) {
        self.move_down();
    }

    /// Resets the query to empty and selection to 0.
    pub fn clear(&mut self) {
        self.query.clear();
        self.selected = 0;
    }

    /// Ensures the selection index is valid for the current filtered results.
    fn clamp_selection(&mut self) {
        let count = self.entries().len();
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
    fn command_registry_contains_all_actions() {
        let commands = Command::all();
        let actions: HashSet<Action> = commands.iter().map(|c| c.action()).collect();

        for action in Action::ALL {
            assert!(
                actions.contains(&action),
                "Command list missing action: {action:?}"
            );
        }
    }

    #[test]
    fn command_filtering_empty_query_returns_all() {
        let all = filter_commands("");
        assert_eq!(all.len(), Action::ALL.len());

        let whitespace = filter_commands("   ");
        assert_eq!(whitespace.len(), Action::ALL.len());
    }

    #[test]
    fn command_filtering_case_insensitive() {
        let upper = filter_commands("RENAME");
        let lower = filter_commands("rename");
        let mixed = filter_commands("ReNaMe");

        assert_eq!(upper[0].action(), Action::Rename);
        assert_eq!(upper[0], lower[0]);
        assert_eq!(lower[0], mixed[0]);
    }

    #[test]
    fn command_filtering_fuzzy_matches() {
        // "ren" -> Rename
        let ren = filter_commands("ren");
        assert_eq!(ren[0].action(), Action::Rename);

        // "ref" -> Refresh Directory
        let ref_cmd = filter_commands("ref");
        assert_eq!(ref_cmd[0].action(), Action::RefreshDirectory);

        // "shcut" -> Help & Shortcuts
        let shcut = filter_commands("shcut");
        assert_eq!(shcut[0].action(), Action::Help);

        // "cmd" -> Command Center
        let cmd = filter_commands("cmd");
        assert_eq!(cmd[0].action(), Action::CommandPalette);
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
        assert!(palette.selected_command().is_some());

        palette.move_down();
        assert_eq!(palette.selected_index(), 1);

        palette.move_up();
        assert_eq!(palette.selected_index(), 0);

        palette.move_up();
        assert_eq!(palette.selected_index(), 0);

        let total = palette.entries().len();
        for _ in 0..total + 5 {
            palette.move_down();
        }
        assert_eq!(palette.selected_index(), total - 1);
    }

    #[test]
    fn test_command_center_search_with_files() {
        let files = vec![
            PathBuf::from("/test/src/main.rs"),
            PathBuf::from("/test/Cargo.toml"),
        ];
        let entries =
            search_command_center("main", ContextFilter::default(), &files, Platform::Mac);
        assert!(!entries.is_empty());
        let has_file = entries.iter().any(|e| match e {
            CommandCenterEntry::FileTarget { title, .. } => title == "main.rs",
            _ => false,
        });
        assert!(has_file, "Search for 'main' should find main.rs");
    }
}
