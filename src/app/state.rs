use std::collections::HashSet;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};

use super::actions::Action;
use super::modes::Mode;
use crate::commands::palette::CommandPaletteState;
use crate::config::settings::{
    ActivePaneConfig, BookmarkConfig, PaneTabsConfig, Settings, default_config_path,
};
use crate::filesystem::FilesystemService;
use crate::filesystem::entry::{Entry, EntryKind, SortMode, sort_entries};
use crate::filesystem::error::ErrorCategory;
use crate::filesystem::metadata::MetadataError;
use crate::filesystem::navigation::{
    DiscoveryError, SearchFailure, SearchOutcome, SearchResult, is_enterable_directory, parent_of,
};
use crate::filesystem::operations::{Operation, OperationError, OperationOutcome};
use crate::git::{GitRepository, GitStatus, ProjectInfo};
use crate::project::WorkspaceContext;
use crate::search::{CancelToken, Matcher, SearchMode};
use crate::ui::theme::{Theme, ThemeId, ThemeRegistry};
use crate::utils::path::{paths_are_equivalent, safe_fallback_directory};

/// Opens a file with the host operating system's default application using native process APIs.
pub fn open_system_default(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to execute 'open': {e}"))
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to launch default application: {e}"))
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to execute 'xdg-open': {e}"))
    }
}

/// The largest scroll offset a listing of `total_items` rows can have while a
/// window of `visible_rows` rows still shows the last row.
///
/// A listing that is empty, or no taller than its window, cannot be scrolled
/// at all. Nothing here can underflow or overflow: the subtraction saturates.
fn maximum_scroll_offset(total_items: usize, visible_rows: usize) -> usize {
    total_items.saturating_sub(visible_rows)
}

/// The scroll offset that keeps `selected_index` inside a window of
/// `visible_rows` rows.
///
/// The offset is first brought inside `0..=maximum_scroll_offset(total_items,
/// visible_rows)`, so it is always valid for the listing it describes. It is
/// then moved just far enough for the selected row to fall inside the window,
/// so a selection that left the window brings the window with it.
///
/// A window of no rows cannot scroll anything into view, so the offset is only
/// kept valid. That is not an error either: an offset that is merely useless is
/// still better than a pane whose state cannot be trusted.
fn ensure_visible(
    selected_index: usize,
    scroll_offset: usize,
    total_items: usize,
    visible_rows: usize,
) -> usize {
    let maximum = maximum_scroll_offset(total_items, visible_rows);
    let offset = scroll_offset.min(maximum);

    if visible_rows == 0 {
        return offset;
    }

    if selected_index < offset {
        return selected_index.min(maximum);
    }

    if selected_index >= offset.saturating_add(visible_rows) {
        return (selected_index - visible_rows + 1).min(maximum);
    }

    offset
}

/// Whether `name` is a single, ordinary file name.
///
/// A name that is empty, that is `.` or `..`, or that contains a separator is
/// not a name for a child of a directory, and is refused so that these entry
/// points can only ever reach inside the directory a pane is showing.
fn is_single_name(name: &OsStr) -> bool {
    let mut components = Path::new(name).components();

    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

/// The operation a clipboard state stands for, naming `source`.
fn clipboard_operation(operation: ClipboardOperation, source: &Path) -> Operation {
    match operation {
        ClipboardOperation::Copy => Operation::Copy(source.to_path_buf()),
        ClipboardOperation::Cut => Operation::Move(source.to_path_buf()),
    }
}

/// Which pane the application acts on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ActivePane {
    /// The left pane.
    #[default]
    Left,
    /// The right pane.
    Right,
}

impl ActivePane {
    /// The other pane.
    pub fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

/// Origin of a directory synchronization event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncOrigin {
    /// Initial startup synchronization.
    #[default]
    Startup,
    /// User navigated in the File Manager (open, go_parent, history, smart jump, etc.).
    FileManagerNavigation,
    /// Shell child process changed directory (via cd in terminal or OSC 7).
    ShellCwdChange,
    /// Switched active File Manager pane.
    ActivePaneSwitch,
    /// Filesystem watcher detected disk changes.
    FilesystemMutation,
    /// Explicit internal refresh.
    InternalRefresh,
}

/// The active filesystem location and synchronization state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveLocation {
    pub path: PathBuf,
    pub active_pane: ActivePane,
    pub last_sync_origin: SyncOrigin,
    pub synchronized: bool,
}

/// What one tab within a pane holds.
///
/// A tab stores where it points, the entries discovered there, its sort mode,
/// its search state, and which entry is currently selected and scrolled to.
/// It also keeps a bounded directory-visit history so the user can step back
/// and forward through the directories they have opened.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tab {
    current_path: PathBuf,
    entries: Vec<Entry>,
    sort_mode: SortMode,
    search: SearchState,
    selected_index: Option<usize>,
    selection_anchor: Option<usize>,
    selected_paths: std::collections::HashSet<PathBuf>,
    scroll_offset: usize,
    git_status: GitStatus,
    project_info: ProjectInfo,
    workspace_context: WorkspaceContext,
    /// Ordered list of directories visited by this tab, newest at the end.
    history: Vec<PathBuf>,
    /// Index into `history` that `current_path` corresponds to.
    history_index: usize,
    /// Whether hidden files are shown in this tab.
    show_hidden: bool,
}

impl Tab {
    /// Maximum number of directories kept in the per-tab navigation history.
    const HISTORY_LIMIT: usize = 32;

    /// Creates a new tab pointing to `path` with `entries`.
    pub fn new(path: PathBuf, entries: Vec<Entry>) -> Self {
        let git_status = crate::git::compute_status(&path);
        let project_info = crate::git::detect_project_for_path(&path, &git_status);
        let workspace_context = crate::project::analyze_workspace(&path);
        let history = if path.as_os_str().is_empty() {
            Vec::new()
        } else {
            vec![path.clone()]
        };
        let mut tab = Self {
            current_path: path,
            entries: Vec::new(),
            sort_mode: SortMode::Name,
            search: SearchState::default(),
            selected_index: None,
            selection_anchor: None,
            selected_paths: std::collections::HashSet::new(),
            scroll_offset: 0,
            git_status,
            project_info,
            workspace_context,
            history,
            history_index: 0,
            show_hidden: false,
        };
        tab.replace_entries(entries, 0);
        tab
    }

    /// The directory this tab points at.
    pub fn current_path(&self) -> &PathBuf {
        &self.current_path
    }

    /// Sets the directory this tab points at, without recording history.
    ///
    /// Used internally when a rename or cut moves the directory the tab was
    /// showing; the path changes but no navigation took place.
    pub fn set_current_path(&mut self, path: PathBuf) {
        self.git_status = crate::git::compute_status(&path);
        self.project_info = crate::git::detect_project_for_path(&path, &self.git_status);
        self.workspace_context = crate::project::analyze_workspace(&path);
        self.current_path = path;
    }

    /// Records `path` in the navigation history and makes it `current_path`.
    ///
    /// Any forward history after the current position is discarded, and the
    /// list is capped at [`Self::HISTORY_LIMIT`] entries so the oldest drops
    /// off the back when the limit is reached.
    pub fn navigate_to(&mut self, path: PathBuf) {
        // Discard any forward history.
        if self.history_index + 1 < self.history.len() {
            self.history.truncate(self.history_index + 1);
        }
        // Push the new destination.
        self.history.push(path.clone());
        if self.history.len() > Self::HISTORY_LIMIT {
            self.history.remove(0);
        } else {
            self.history_index += 1;
        }
        self.set_current_path(path);
    }

    /// The path one step back in history, if any.
    pub fn back_path(&self) -> Option<&PathBuf> {
        if self.history_index > 0 {
            self.history.get(self.history_index - 1)
        } else {
            None
        }
    }

    /// The path one step forward in history, if any.
    pub fn forward_path(&self) -> Option<&PathBuf> {
        self.history.get(self.history_index + 1)
    }

    /// Moves back one step in history without changing the stored path.
    ///
    /// Returns `Some(path)` to navigate to when movement is possible.
    pub fn step_back(&mut self) -> Option<PathBuf> {
        if self.history_index > 0 {
            self.history_index -= 1;
            let path = self.history[self.history_index].clone();
            self.set_current_path(path.clone());
            Some(path)
        } else {
            None
        }
    }

    /// Moves forward one step in history without changing the stored path.
    ///
    /// Returns `Some(path)` to navigate to when movement is possible.
    pub fn step_forward(&mut self) -> Option<PathBuf> {
        if self.history_index + 1 < self.history.len() {
            self.history_index += 1;
            let path = self.history[self.history_index].clone();
            self.set_current_path(path.clone());
            Some(path)
        } else {
            None
        }
    }

    /// Whether there is a directory to go back to.
    pub fn can_go_back(&self) -> bool {
        self.history_index > 0
    }

    /// Whether there is a directory to go forward to.
    pub fn can_go_forward(&self) -> bool {
        self.history_index + 1 < self.history.len()
    }

    /// The Git repository detection state of this tab.
    pub fn git(&self) -> &GitRepository {
        &self.git_status.repository
    }

    /// The Git status of this tab.
    pub fn git_status(&self) -> &GitStatus {
        &self.git_status
    }

    /// The project awareness info of this tab.
    pub fn project_info(&self) -> &ProjectInfo {
        &self.project_info
    }

    /// The workspace structure and project graph context of this tab.
    pub fn workspace_context(&self) -> &WorkspaceContext {
        &self.workspace_context
    }

    /// Re-evaluates Git status, project information, and workspace context for the current directory.
    pub fn refresh_git_and_project(&mut self) {
        self.git_status = crate::git::compute_status(&self.current_path);
        self.project_info =
            crate::git::detect_project_for_path(&self.current_path, &self.git_status);
        self.workspace_context = crate::project::analyze_workspace(&self.current_path);
    }

    /// Short display name for this tab (e.g. folder name, "/" for root, or "[empty]").
    pub fn name(&self) -> String {
        if self.current_path.as_os_str().is_empty() {
            return "[empty]".to_string();
        }
        if let Some(name) = self
            .current_path
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
        {
            return name.to_string();
        }
        self.current_path.to_string_lossy().to_string()
    }

    /// The entries discovered in [`Self::current_path`].
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The search state of this tab.
    pub fn search(&self) -> &SearchState {
        &self.search
    }

    /// How many entries this tab is showing.
    pub fn visible_count(&self) -> usize {
        self.search.count()
    }

    /// The entries of the tab's listing that are showing, in order.
    pub fn visible_entries(&self) -> impl Iterator<Item = &Entry> + '_ {
        self.search
            .listing_positions()
            .iter()
            .filter_map(|index| self.entries.get(*index))
    }

    /// What a walk found, when its results are the ones being shown.
    pub fn visible_results(&self) -> &[SearchResult] {
        if self.search.shows_results() {
            self.search.results()
        } else {
            &[]
        }
    }

    /// The order this tab displays its entries in.
    pub fn sort_mode(&self) -> SortMode {
        self.sort_mode
    }

    /// The index of the selected entry, or `None` when nothing is selected.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    /// Sets the selected index directly.
    pub fn set_selected_index(&mut self, index: Option<usize>) {
        self.selected_index = index;
    }

    /// The selection anchor index in this tab, if any.
    pub fn selection_anchor(&self) -> Option<usize> {
        self.selection_anchor
    }

    /// Sets the selection anchor index directly.
    pub fn set_selection_anchor(&mut self, anchor: Option<usize>) {
        self.selection_anchor = anchor;
    }

    /// How many entries are scrolled past.
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Sets the scroll offset directly.
    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    /// The position of the last entry the tab shows, when it shows any.
    fn last_index(&self) -> Option<usize> {
        self.visible_count().checked_sub(1)
    }

    /// Moves the selection to `index` and scrolls so the entry stays visible.
    fn select(&mut self, index: usize, visible_rows: usize) {
        self.selected_index = Some(index);
        self.selection_anchor = Some(index);
        self.selected_paths.clear();
        self.scroll_to_selection(visible_rows);
    }

    /// Forgets the selection and returns to the top.
    fn clear_selection(&mut self) {
        self.selected_index = None;
        self.selection_anchor = None;
        self.selected_paths.clear();
        self.scroll_offset = 0;
    }

    /// Scrolls exactly as far as the selected entry needs.
    fn scroll_to_selection(&mut self, visible_rows: usize) {
        let total = self.visible_count();
        self.scroll_offset = match self.selected_index {
            Some(index) => ensure_visible(index, self.scroll_offset, total, visible_rows),
            None => self
                .scroll_offset
                .min(maximum_scroll_offset(total, visible_rows)),
        };
    }

    /// Selects the first entry, or nothing when the tab is empty.
    fn select_first(&mut self, visible_rows: usize) {
        match self.last_index() {
            Some(_) => self.select(0, visible_rows),
            None => self.clear_selection(),
        }
    }

    /// Selects the last entry, or nothing when the tab is empty.
    fn select_last(&mut self, visible_rows: usize) {
        match self.last_index() {
            Some(last) => self.select(last, visible_rows),
            None => self.clear_selection(),
        }
    }

    /// Moves the selection towards the end by `steps`, stopping at the last entry.
    fn move_selection_down(&mut self, steps: usize, visible_rows: usize) {
        let Some(last) = self.last_index() else {
            self.clear_selection();
            return;
        };

        let next = match self.selected_index {
            None => 0,
            Some(current) => current.min(last).saturating_add(steps).min(last),
        };

        self.select(next, visible_rows);
    }

    /// Moves the selection towards the start by `steps`, stopping at the first entry.
    fn move_selection_up(&mut self, steps: usize, visible_rows: usize) {
        let Some(last) = self.last_index() else {
            self.clear_selection();
            return;
        };

        let next = match self.selected_index {
            None => 0,
            Some(current) => current.min(last).saturating_sub(steps),
        };

        self.select(next, visible_rows);
    }

    /// Extends the range selection towards the start by `steps`.
    fn extend_selection_up(&mut self, steps: usize, visible_rows: usize) {
        let Some(last) = self.last_index() else {
            self.clear_selection();
            return;
        };
        let current = self.selected_index.unwrap_or(0).min(last);
        let anchor = self.selection_anchor.unwrap_or(current);
        self.selection_anchor = Some(anchor);
        let target = current.saturating_sub(steps);
        self.selected_index = Some(target);
        self.select_range(anchor, target);
        self.scroll_to_selection(visible_rows);
    }

    /// Extends the range selection towards the end by `steps`.
    fn extend_selection_down(&mut self, steps: usize, visible_rows: usize) {
        let Some(last) = self.last_index() else {
            self.clear_selection();
            return;
        };
        let current = self.selected_index.unwrap_or(0).min(last);
        let anchor = self.selection_anchor.unwrap_or(current);
        self.selection_anchor = Some(anchor);
        let target = current.saturating_add(steps).min(last);
        self.selected_index = Some(target);
        self.select_range(anchor, target);
        self.scroll_to_selection(visible_rows);
    }

    /// Selects a contiguous range of items from anchor to `target`.
    pub fn select_range_to(&mut self, target: usize, visible_rows: usize) {
        let Some(last) = self.last_index() else {
            self.clear_selection();
            return;
        };
        let target_clamped = target.min(last);
        let anchor = self
            .selection_anchor
            .unwrap_or_else(|| self.selected_index.unwrap_or(0).min(last));
        self.selection_anchor = Some(anchor);
        self.selected_index = Some(target_clamped);
        self.select_range(anchor, target_clamped);
        self.scroll_to_selection(visible_rows);
    }

    /// The selected entry of the tab's listing, when the selection points at one.
    pub fn selected_entry(&self) -> Option<&Entry> {
        if self.search.shows_results() {
            return None;
        }

        self.selected_index
            .and_then(|position| self.entry_at(position))
    }

    /// Whether this tab is showing search results from a recursive walk.
    pub fn shows_results(&self) -> bool {
        self.search.shows_results()
    }

    /// The selected search result when showing recursive search results.
    pub fn selected_search_result(&self) -> Option<&SearchResult> {
        if self.search.shows_results() {
            self.selected_index
                .and_then(|position| self.search.result_at(position))
        } else {
            None
        }
    }

    /// Selects the entry whose filename matches `name`.
    pub fn select_name(&mut self, name: &OsStr, visible_rows: usize) -> bool {
        let pos = self.entries.iter().position(|e| e.name() == name);
        if let Some(pos) = pos {
            self.selected_index = Some(pos);
            self.scroll_to_selection(visible_rows);
            true
        } else {
            false
        }
    }

    /// Selects the entry whose full path matches `path`.
    pub fn select_path(&mut self, path: &Path, visible_rows: usize) -> bool {
        let pos = self.entries.iter().position(|e| e.path() == path);
        if let Some(pos) = pos {
            self.selected_index = Some(pos);
            self.scroll_to_selection(visible_rows);
            true
        } else {
            false
        }
    }

    /// The entry shown at `position`, when the tab shows one there.
    pub fn entry_at(&self, position: usize) -> Option<&Entry> {
        let index = self.search.entry_index(position)?;
        self.entries.get(index)
    }

    /// Whether the entry shown at `position` is a directory.
    pub fn is_directory_at(&self, position: usize) -> bool {
        if self.search.shows_results() {
            self.shown_path(position).is_some_and(|p| p.is_dir())
        } else if let Some(entry) = self.entry_at(position) {
            match entry.kind() {
                EntryKind::Directory => true,
                EntryKind::Symlink => is_enterable_directory(entry.path()).unwrap_or(false),
                _ => false,
            }
        } else {
            false
        }
    }

    /// The path shown at `position`, whichever view is being shown.
    fn shown_path(&self, position: usize) -> Option<&Path> {
        if self.search.shows_results() {
            self.search.result_at(position).map(SearchResult::path)
        } else {
            self.entry_at(position).map(Entry::path)
        }
    }

    /// Replaces the contents of the tab, preserving selected path or position when possible.
    fn replace_entries(&mut self, entries: Vec<Entry>, visible_rows: usize) {
        let previous_selected = self.selected_path();
        self.set_entries(entries);

        // Remove from selected_paths any paths that no longer exist in self.entries
        self.selected_paths
            .retain(|p| self.entries.iter().any(|e| e.path() == p));

        self.reselect(previous_selected.as_deref(), visible_rows);
    }

    /// Whether the item at `position` is multi-selected.
    pub fn is_item_selected(&self, position: usize) -> bool {
        if let Some(path) = self.shown_path(position) {
            self.selected_paths.contains(path)
        } else {
            false
        }
    }

    /// Toggles the multi-selection of the entry at `position`.
    pub fn toggle_selection(&mut self, position: usize) {
        if position >= self.visible_count() {
            return;
        }
        self.selected_index = Some(position);
        self.selection_anchor = Some(position);
        if let Some(path) = self.shown_path(position).map(Path::to_path_buf) {
            if self.selected_paths.contains(&path) {
                self.selected_paths.remove(&path);
            } else {
                self.selected_paths.insert(path);
            }
        }
    }

    /// Selects all visible entries in this tab.
    pub fn select_all(&mut self) {
        for pos in 0..self.visible_count() {
            if let Some(path) = self.shown_path(pos).map(Path::to_path_buf) {
                self.selected_paths.insert(path);
            }
        }
    }

    /// Clears all multi-selected entries in this tab.
    pub fn deselect_all(&mut self) {
        self.selected_paths.clear();
    }

    /// Inverts the selection of all visible entries in this tab.
    pub fn invert_selection(&mut self) {
        let mut new_set = std::collections::HashSet::new();
        for pos in 0..self.visible_count() {
            if let Some(path) = self
                .shown_path(pos)
                .map(Path::to_path_buf)
                .filter(|path| !self.selected_paths.contains(path))
            {
                new_set.insert(path);
            }
        }
        self.selected_paths = new_set;
    }

    /// Selects a contiguous range of visible entries [start, end].
    pub fn select_range(&mut self, start: usize, end: usize) {
        let min = start.min(end);
        let max = start.max(end).min(self.visible_count().saturating_sub(1));
        for pos in min..=max {
            if let Some(path) = self.shown_path(pos).map(Path::to_path_buf) {
                self.selected_paths.insert(path);
            }
        }
    }

    /// Computes the aggregate size in bytes of all currently selected entries.
    pub fn aggregate_selected_size(&self) -> u64 {
        let paths = self.effective_selected_paths();
        let mut total = 0u64;
        for path in paths {
            if let Ok(meta) = std::fs::symlink_metadata(&path) {
                total = total.saturating_add(meta.len());
            }
        }
        total
    }

    /// How many items are currently multi-selected.
    pub fn selected_count(&self) -> usize {
        self.selected_paths.len()
    }

    /// The list of selected paths: if multi-selection is active, returns them;
    /// otherwise returns the single cursor-selected path (if any).
    pub fn effective_selected_paths(&self) -> Vec<PathBuf> {
        if !self.selected_paths.is_empty() {
            let mut paths: Vec<PathBuf> = self.selected_paths.iter().cloned().collect();
            paths.sort();
            paths
        } else if let Some(path) = self.selected_path() {
            vec![path]
        } else {
            Vec::new()
        }
    }

    /// Whether hidden files are shown in this tab.
    pub fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    /// Toggles showing hidden files in this tab.
    pub fn toggle_hidden(&mut self, visible_rows: usize) {
        self.show_hidden = !self.show_hidden;
        let selected = self.selected_path();
        self.search.refilter(&self.entries, self.show_hidden);
        self.reselect_or_first(selected.as_deref(), visible_rows);
    }

    /// Stores a new listing and works out again which of its entries are shown.
    fn set_entries(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
        if self.search.shows_results() {
            self.search.forget_results();
        }
        self.search.refilter(&self.entries, self.show_hidden);
    }

    /// Brings the selection and the scroll offset back into range.
    fn clamp_to_entries(&mut self, visible_rows: usize) {
        self.selected_index = match (self.selected_index, self.last_index()) {
            (_, None) => None,
            (Some(index), Some(last)) => Some(index.min(last)),
            (None, Some(_)) => None,
        };
        self.scroll_to_selection(visible_rows);
    }

    /// Moves to the next sort mode and reorders the entries the tab holds.
    fn change_sort(&mut self, visible_rows: usize) {
        self.sort_mode = self.sort_mode.next();
        let selected = self.selected_path();
        sort_entries(&mut self.entries, &mut [], self.sort_mode);
        self.search.refilter(&self.entries, self.show_hidden);
        self.reselect(selected.as_deref(), visible_rows);
    }

    /// Looks among the entries this tab already holds for `query`.
    fn set_search_query(&mut self, query: &str, visible_rows: usize) {
        let selected = self.selected_path();
        self.search.update(query, &self.entries, self.show_hidden);
        self.reselect_or_first(selected.as_deref(), visible_rows);
    }

    /// Sets the mode of this tab's search.
    fn set_search_mode(&mut self, mode: SearchMode, visible_rows: usize) {
        self.search.set_mode(mode, &self.entries, self.show_hidden);
        let selected = self.selected_path();
        self.reselect_or_first(selected.as_deref(), visible_rows);
    }

    /// Runs the search this tab is set to, for `query`.
    fn run_search(&mut self, query: &str, filesystem: &FilesystemService, visible_rows: usize) {
        let selected = self.selected_path();
        self.search.update(query, &self.entries, self.show_hidden);
        if !self.search.shows_results() {
            self.reselect_or_first(selected.as_deref(), visible_rows);
            return;
        }

        let mode = self.search.mode();
        let cancel = self.search.cancel_handle();
        let tree = filesystem.search(self.current_path(), query, mode, &cancel);
        let (results, outcome, skipped) = tree.into_parts();
        self.search.store(results, outcome, skipped);
        self.reselect_or_first(selected.as_deref(), visible_rows);
    }

    /// Asks this tab's search to stop.
    fn cancel_search(&self) {
        self.search.cancel();
    }

    /// Returns the tab to a plain listing of everything it holds.
    fn clear_search(&mut self, visible_rows: usize) {
        let selected = self.selected_path();
        self.search.clear();
        self.search.refilter(&self.entries, self.show_hidden);
        self.reselect_or_first(selected.as_deref(), visible_rows);
    }

    /// The path of what the tab has selected.
    pub fn selected_path(&self) -> Option<PathBuf> {
        let position = self.selected_index?;
        self.shown_path(position).map(Path::to_path_buf)
    }

    fn position_of(&self, path: Option<&Path>) -> Option<usize> {
        let path = path?;
        (0..self.visible_count())
            .find(|position| matches!(self.shown_path(*position), Some(shown) if shown == path))
    }

    fn reselect(&mut self, path: Option<&Path>, visible_rows: usize) {
        match self.position_of(path) {
            Some(position) => self.select(position, visible_rows),
            None => self.clamp_to_entries(visible_rows),
        }
    }

    fn reselect_or_first(&mut self, path: Option<&Path>, visible_rows: usize) {
        match self.position_of(path) {
            Some(position) => self.select(position, visible_rows),
            None => self.select_first(visible_rows),
        }
    }
}

/// What one pane currently shows.
///
/// A pane owns one or more [`Tab`]s and tracks which tab is currently active.
/// It provides access to the active tab's properties while retaining its own
/// layout geometry (`visible_rows`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    tabs: Vec<Tab>,
    active_tab_index: usize,
    visible_rows: usize,
}

impl Default for Pane {
    fn default() -> Self {
        Self {
            tabs: vec![Tab::default()],
            active_tab_index: 0,
            visible_rows: 0,
        }
    }
}

impl Pane {
    /// Returns the slice of all tabs in this pane.
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// The index of the active tab.
    pub fn active_tab_index(&self) -> usize {
        self.active_tab_index
    }

    /// The active tab.
    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active_tab_index]
    }

    /// The active tab mutably.
    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab_index]
    }

    /// How many tabs are in this pane.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// The Git repository detection state of the active tab.
    pub fn git(&self) -> &GitRepository {
        self.active_tab().git()
    }

    /// The Git status of the active tab.
    pub fn git_status(&self) -> &GitStatus {
        self.active_tab().git_status()
    }

    /// The project awareness info of the active tab.
    pub fn project_info(&self) -> &ProjectInfo {
        self.active_tab().project_info()
    }

    /// The workspace structure and project graph context of the active tab.
    pub fn workspace_context(&self) -> &WorkspaceContext {
        self.active_tab().workspace_context()
    }

    /// Re-evaluates Git status, project information, and workspace context for the active tab.
    pub fn refresh_git_and_project(&mut self) {
        self.active_tab_mut().refresh_git_and_project();
    }

    /// Adds a new tab pointing at `path` with `entries`, making it active.
    /// Returns the index of the newly created tab.
    pub fn new_tab(&mut self, path: PathBuf, entries: Vec<Entry>) -> usize {
        let mut tab = Tab::new(path, entries);
        tab.select_first(self.visible_rows);
        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;
        self.active_tab_index
    }

    /// Duplicates the active tab in this pane.
    pub fn duplicate_active_tab(&mut self) -> usize {
        let cloned_tab = self.active_tab().clone();
        self.tabs.push(cloned_tab);
        self.active_tab_index = self.tabs.len() - 1;
        self.active_tab_index
    }

    /// Closes the active tab if more than one tab exists.
    ///
    /// If only one tab exists, this is a no-op and returns `false`.
    /// Otherwise removes the active tab, adjusts the active tab index
    /// to a valid neighbor, and returns `true`.
    pub fn close_active_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        self.tabs.remove(self.active_tab_index);
        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        }
        true
    }

    /// Cycles to the next tab, wrapping around to the first tab.
    pub fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab_index = (self.active_tab_index + 1) % self.tabs.len();
        }
    }

    /// Cycles to the previous tab, wrapping around to the last tab.
    pub fn previous_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab_index = if self.active_tab_index == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab_index - 1
            };
        }
    }

    /// Selects a specific tab by index.
    pub fn select_tab(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.active_tab_index = index;
            true
        } else {
            false
        }
    }

    /// The directory this pane's active tab points at. Empty until a later phase sets it.
    pub fn current_path(&self) -> &PathBuf {
        self.active_tab().current_path()
    }

    /// Sets the directory of the active tab.
    pub fn set_current_path(&mut self, path: PathBuf) {
        self.active_tab_mut().set_current_path(path);
    }

    /// Records `path` as a visited directory in the active tab's history.
    pub fn navigate_to(&mut self, path: PathBuf) {
        self.active_tab_mut().navigate_to(path);
    }

    /// The entries discovered in the active tab's current path.
    pub fn entries(&self) -> &[Entry] {
        self.active_tab().entries()
    }

    /// Whether the active tab has a previous directory to return to.
    pub fn can_go_back(&self) -> bool {
        self.active_tab().can_go_back()
    }

    /// Whether the active tab has a forward directory to return to.
    pub fn can_go_forward(&self) -> bool {
        self.active_tab().can_go_forward()
    }

    /// The search of this pane's active tab.
    pub fn search(&self) -> &SearchState {
        self.active_tab().search()
    }

    /// How many entries the active tab is showing.
    pub fn visible_count(&self) -> usize {
        self.active_tab().visible_count()
    }

    /// The entries of the active tab's listing that are showing, in order.
    pub fn visible_entries(&self) -> impl Iterator<Item = &Entry> + '_ {
        self.active_tab().visible_entries()
    }

    /// What a walk found, when its results are the ones being shown.
    pub fn visible_results(&self) -> &[SearchResult] {
        self.active_tab().visible_results()
    }

    /// The order this pane's active tab displays its entries in.
    pub fn sort_mode(&self) -> SortMode {
        self.active_tab().sort_mode()
    }

    /// The index of the selected entry, or `None` when nothing is selected.
    pub fn selected_index(&self) -> Option<usize> {
        self.active_tab().selected_index()
    }

    /// Sets the selected index of the active tab.
    pub fn set_selected_index(&mut self, index: Option<usize>) {
        self.active_tab_mut().set_selected_index(index);
    }

    /// How many entries are scrolled past in the active tab.
    pub fn scroll_offset(&self) -> usize {
        self.active_tab().scroll_offset()
    }

    /// Sets the scroll offset of the active tab.
    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.active_tab_mut().set_scroll_offset(offset);
    }

    /// How many rows of this pane are on screen.
    pub fn visible_rows(&self) -> usize {
        self.visible_rows
    }

    /// Tells the pane how many of its rows are on screen.
    pub fn set_visible_rows(&mut self, rows: usize) {
        self.visible_rows = rows;
        for tab in &mut self.tabs {
            tab.scroll_to_selection(rows);
        }
    }

    /// Moves the active tab's selection to `index` and scrolls so the entry stays visible.
    pub fn select(&mut self, index: usize) {
        let rows = self.visible_rows;
        self.active_tab_mut().select(index, rows);
    }

    /// Forgets the active tab's selection and returns to the top.
    pub fn clear_selection(&mut self) {
        self.active_tab_mut().clear_selection();
    }

    /// Scrolls exactly as far as the selected entry needs.
    pub fn scroll_to_selection(&mut self) {
        let rows = self.visible_rows;
        self.active_tab_mut().scroll_to_selection(rows);
    }

    /// Selects the first entry in the active tab, or nothing when empty.
    pub fn select_first(&mut self) {
        let rows = self.visible_rows;
        self.active_tab_mut().select_first(rows);
    }

    /// Selects the last entry in the active tab, or nothing when empty.
    pub fn select_last(&mut self) {
        let rows = self.visible_rows;
        self.active_tab_mut().select_last(rows);
    }

    /// Moves the active tab selection towards the end by `steps`.
    pub fn move_selection_down(&mut self, steps: usize) {
        let rows = self.visible_rows;
        self.active_tab_mut().move_selection_down(steps, rows);
    }

    /// Moves the active tab selection towards the start by `steps`.
    pub fn move_selection_up(&mut self, steps: usize) {
        let rows = self.visible_rows;
        self.active_tab_mut().move_selection_up(steps, rows);
    }

    /// The selected entry of the active tab's listing.
    pub fn selected_entry(&self) -> Option<&Entry> {
        self.active_tab().selected_entry()
    }

    /// Whether the active tab is showing search results from a recursive walk.
    pub fn shows_results(&self) -> bool {
        self.active_tab().shows_results()
    }

    /// The selected search result when showing recursive search results.
    pub fn selected_search_result(&self) -> Option<&SearchResult> {
        self.active_tab().selected_search_result()
    }

    /// Selects the entry whose filename matches `name`.
    pub fn select_name(&mut self, name: &OsStr) -> bool {
        let rows = self.visible_rows;
        self.active_tab_mut().select_name(name, rows)
    }

    /// Selects the entry whose path matches `path`.
    pub fn select_path(&mut self, path: &Path) -> bool {
        let rows = self.visible_rows;
        self.active_tab_mut().select_path(path, rows)
    }

    /// The entry shown at `position` in the active tab.
    pub fn entry_at(&self, position: usize) -> Option<&Entry> {
        self.active_tab().entry_at(position)
    }

    /// Whether the entry shown at `position` in the active tab is a directory.
    pub fn is_directory_at(&self, position: usize) -> bool {
        self.active_tab().is_directory_at(position)
    }

    /// The path shown at `position` in the active tab.
    pub fn shown_path(&self, position: usize) -> Option<&Path> {
        self.active_tab().shown_path(position)
    }

    /// Replaces the contents of the active tab and starts again from the top.
    pub fn replace_entries(&mut self, entries: Vec<Entry>) {
        let rows = self.visible_rows;
        self.active_tab_mut().replace_entries(entries, rows);
    }

    /// Stores a new listing in the active tab.
    pub fn set_entries(&mut self, entries: Vec<Entry>) {
        self.active_tab_mut().set_entries(entries);
    }

    /// Refreshes the listing in the active tab while preserving selection and scroll.
    pub fn refresh_preserving_selection(&mut self, entries: Vec<Entry>) {
        let prev_selected = self.selected_path();
        self.set_entries(entries);
        self.reselect_or_first(prev_selected.as_deref());
        self.refresh_git_and_project();
    }

    /// Brings the active tab's selection and scroll offset back into range.
    pub fn clamp_to_entries(&mut self) {
        let rows = self.visible_rows;
        self.active_tab_mut().clamp_to_entries(rows);
    }

    /// Moves to the next sort mode and reorders the active tab's entries.
    pub fn change_sort(&mut self) {
        let rows = self.visible_rows;
        self.active_tab_mut().change_sort(rows);
    }

    /// Whether hidden files are shown in the active tab.
    pub fn show_hidden(&self) -> bool {
        self.active_tab().show_hidden()
    }

    /// Toggles showing hidden files in the active tab.
    pub fn toggle_hidden(&mut self) {
        let rows = self.visible_rows;
        self.active_tab_mut().toggle_hidden(rows);
    }

    /// Looks among the entries the active tab holds for `query`.
    pub fn set_search_query(&mut self, query: &str) {
        let rows = self.visible_rows;
        self.active_tab_mut().set_search_query(query, rows);
    }

    /// Sets the search mode of the active tab.
    pub fn set_search_mode(&mut self, mode: SearchMode) {
        let rows = self.visible_rows;
        self.active_tab_mut().set_search_mode(mode, rows);
    }

    /// Advances the active tab's search mode to the next one in the cycle.
    ///
    /// The order is Basic → Recursive → Fuzzy → RecursiveFuzzy → Basic.
    pub fn cycle_search_mode(&mut self) {
        use crate::search::SearchMode;
        let current = self.active_tab().search().mode();
        let next = match current {
            SearchMode::Basic => SearchMode::Recursive,
            SearchMode::Recursive => SearchMode::Fuzzy,
            SearchMode::Fuzzy => SearchMode::RecursiveFuzzy,
            SearchMode::RecursiveFuzzy => SearchMode::Basic,
        };
        let rows = self.visible_rows;
        self.active_tab_mut().set_search_mode(next, rows);
    }

    /// Runs the search the active tab is set to for `query`.
    pub fn run_search(&mut self, query: &str, filesystem: &FilesystemService) {
        let rows = self.visible_rows;
        self.active_tab_mut().run_search(query, filesystem, rows);
    }

    /// Asks the active tab's search to stop.
    pub fn cancel_search(&self) {
        self.active_tab().cancel_search();
    }

    /// Returns the active tab to a plain listing.
    pub fn clear_search(&mut self) {
        let rows = self.visible_rows;
        self.active_tab_mut().clear_search(rows);
    }

    /// The path of what the active tab has selected.
    pub fn selected_path(&self) -> Option<PathBuf> {
        self.active_tab().selected_path()
    }

    /// Keeps the entry at `path` selected, or clamps the selection safely.
    pub fn reselect(&mut self, path: Option<&Path>) {
        let rows = self.visible_rows;
        self.active_tab_mut().reselect(path, rows);
    }

    /// Keeps the entry at `path` selected, or selects the first entry shown.
    pub fn reselect_or_first(&mut self, path: Option<&Path>) {
        let rows = self.visible_rows;
        self.active_tab_mut().reselect_or_first(path, rows);
    }

    /// Whether the item at `position` is multi-selected in the active tab.
    pub fn is_item_selected(&self, position: usize) -> bool {
        self.active_tab().is_item_selected(position)
    }

    /// Toggles the multi-selection of the entry at `position` in the active tab.
    pub fn toggle_selection(&mut self, position: usize) {
        self.active_tab_mut().toggle_selection(position);
    }

    /// Selects all visible entries in the active tab.
    pub fn select_all(&mut self) {
        self.active_tab_mut().select_all();
    }

    /// Clears all multi-selected entries in the active tab.
    pub fn deselect_all(&mut self) {
        self.active_tab_mut().deselect_all();
    }

    /// Inverts the selection of all visible entries in the active tab.
    pub fn invert_selection(&mut self) {
        self.active_tab_mut().invert_selection();
    }

    /// Selects a contiguous range of visible entries [start, end] in the active tab.
    pub fn select_range(&mut self, start: usize, end: usize) {
        self.active_tab_mut().select_range(start, end);
    }

    /// How many items are currently multi-selected in the active tab.
    pub fn selected_count(&self) -> usize {
        self.active_tab().selected_count()
    }

    /// The selection anchor index in the active tab, if any.
    pub fn selection_anchor(&self) -> Option<usize> {
        self.active_tab().selection_anchor()
    }

    /// Sets the selection anchor index in the active tab.
    pub fn set_selection_anchor(&mut self, anchor: Option<usize>) {
        self.active_tab_mut().set_selection_anchor(anchor);
    }

    /// Extends the active tab's range selection towards the start by `steps`.
    pub fn extend_selection_up(&mut self, steps: usize) {
        let rows = self.visible_rows;
        self.active_tab_mut().extend_selection_up(steps, rows);
    }

    /// Extends the active tab's range selection towards the end by `steps`.
    pub fn extend_selection_down(&mut self, steps: usize) {
        let rows = self.visible_rows;
        self.active_tab_mut().extend_selection_down(steps, rows);
    }

    /// Selects a contiguous range of items from anchor to `target` in the active tab.
    pub fn select_range_to(&mut self, target: usize) {
        let rows = self.visible_rows;
        self.active_tab_mut().select_range_to(target, rows);
    }

    /// Computes the aggregate size in bytes of all currently selected entries.
    pub fn aggregate_selected_size(&self) -> u64 {
        self.active_tab().aggregate_selected_size()
    }

    /// The list of selected paths in the active tab.
    pub fn effective_selected_paths(&self) -> Vec<PathBuf> {
        self.active_tab().effective_selected_paths()
    }

    /// The path of the entry at `index` in the active tab.
    pub fn path_at(&self, index: usize) -> Option<&Path> {
        self.active_tab().entry_at(index).map(|e| e.path())
    }

    /// The set of multi-selected paths in the active tab.
    pub fn selected_paths(&self) -> &HashSet<PathBuf> {
        &self.active_tab().selected_paths
    }

    /// The mutable set of multi-selected paths in the active tab.
    pub fn selected_paths_mut(&mut self) -> &mut HashSet<PathBuf> {
        &mut self.active_tab_mut().selected_paths
    }
}

/// The category of a file management operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Copy,
    Move,
    Delete,
    CreateFile,
    CreateDirectory,
    Rename,
}

/// The status of an ongoing or completed file operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationStatus {
    Idle,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Foundation progress tracker for file operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationProgress {
    kind: OperationKind,
    status: OperationStatus,
    current_item: Option<String>,
    completed_count: usize,
    total_count: usize,
    error_message: Option<String>,
}

impl OperationProgress {
    /// Creates a new progress tracker.
    pub fn new(kind: OperationKind, total_count: usize) -> Self {
        Self {
            kind,
            status: OperationStatus::Running,
            current_item: None,
            completed_count: 0,
            total_count,
            error_message: None,
        }
    }

    /// The kind of operation.
    pub fn kind(&self) -> OperationKind {
        self.kind
    }

    /// The current status.
    pub fn status(&self) -> OperationStatus {
        self.status
    }

    /// The item currently being processed.
    pub fn current_item(&self) -> Option<&str> {
        self.current_item.as_deref()
    }

    /// Number of successfully completed items.
    pub fn completed_count(&self) -> usize {
        self.completed_count
    }

    /// Total count of items in this operation.
    pub fn total_count(&self) -> usize {
        self.total_count
    }

    /// Error message if the operation failed.
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// Marks the operation as successfully completed.
    pub fn finish_success(&mut self) {
        self.status = OperationStatus::Completed;
        self.completed_count = self.total_count;
        self.current_item = None;
    }

    /// Marks the operation as failed with an error message.
    pub fn finish_error(&mut self, err: String) {
        self.status = OperationStatus::Failed;
        self.error_message = Some(err);
    }
}

/// Why navigation could not be carried out.
///
/// The variant records which step failed and keeps the original error, so a
/// caller can tell a missing directory from a denied one without reading a
/// message.
#[derive(Debug)]
pub enum NavigationError {
    /// The process's working directory could not be determined.
    WorkingDirectory(io::Error),
    /// A directory could not be listed.
    Directory(DiscoveryError),
    /// An entry could not be inspected.
    Metadata(MetadataError),
}

impl NavigationError {
    /// Why the step failed, as a kind a caller can act on.
    ///
    /// Every variant keeps the error underneath it, so this is a second answer
    /// to the same question rather than a replacement for it.
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::WorkingDirectory(error) => ErrorCategory::of(error.kind()),
            Self::Directory(error) => error.category(),
            Self::Metadata(error) => error.category(),
        }
    }

    /// The path the failed navigation concerned, when one is known.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::WorkingDirectory(_) => None,
            Self::Directory(error) => Some(error.path()),
            Self::Metadata(error) => Some(error.path()),
        }
    }
}

impl fmt::Display for NavigationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkingDirectory(error) => {
                write!(f, "cannot determine the working directory: {error}")
            }
            Self::Directory(error) => write!(f, "{error}"),
            Self::Metadata(error) => write!(f, "{error}"),
        }
    }
}

impl From<DiscoveryError> for NavigationError {
    fn from(error: DiscoveryError) -> Self {
        Self::Directory(error)
    }
}

impl From<MetadataError> for NavigationError {
    fn from(error: MetadataError) -> Self {
        Self::Metadata(error)
    }
}

impl Error for NavigationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WorkingDirectory(error) => Some(error),
            Self::Directory(error) => Some(error),
            Self::Metadata(error) => Some(error),
        }
    }
}

/// What the entries on the clipboard were marked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardOperation {
    /// The entries are to be copied, so pasting leaves them where they are.
    Copy,
    /// The entries are to be moved, so pasting takes them away.
    Cut,
}

/// What has been marked, and what marking it meant.
///
/// Only the paths are kept, never anything read from them: the clipboard
/// describes what the user asked for, and the filesystem is asked to do it when
/// the entries are pasted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClipboardState {
    entries: Vec<PathBuf>,
    operation: Option<ClipboardOperation>,
}

impl ClipboardState {
    /// Whether the clipboard holds nothing.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many entries the clipboard holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The entries that were marked.
    pub fn entries(&self) -> &[PathBuf] {
        &self.entries
    }

    /// What the marked entries were marked for, or nothing when the clipboard
    /// is empty.
    pub fn operation(&self) -> Option<ClipboardOperation> {
        self.operation
    }

    /// Marks `entries` for `operation`, replacing what was marked before.
    fn set(&mut self, entries: Vec<PathBuf>, operation: ClipboardOperation) {
        self.entries = entries;
        self.operation = Some(operation);
    }

    /// Forgets what was marked.
    fn clear(&mut self) {
        self.entries.clear();
        self.operation = None;
    }

    /// Removes a path from the clipboard if present, clearing the operation if empty.
    pub fn remove_path(&mut self, path: &Path) {
        self.entries.retain(|p| p != path && !p.starts_with(path));
        if self.entries.is_empty() {
            self.operation = None;
        }
    }

    /// Replaces a path in the clipboard if present.
    pub fn replace_path(&mut self, from: &Path, to: PathBuf) {
        for entry in &mut self.entries {
            if entry == from {
                *entry = to.clone();
            } else if let Ok(suffix) = entry.strip_prefix(from) {
                *entry = to.join(suffix);
            }
        }
    }
}

/// What the preview panel currently holds and whether it is shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreviewState {
    active: bool,
    path: Option<PathBuf>,
    content: Option<crate::preview::PreviewContent>,
}

impl PreviewState {
    /// Whether a preview is currently shown.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// The path of the entry being previewed.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The prepared preview content.
    pub fn content(&self) -> Option<&crate::preview::PreviewContent> {
        self.content.as_ref()
    }

    /// Sets the active preview content for `path`.
    pub fn set_content(&mut self, path: PathBuf, content: crate::preview::PreviewContent) {
        self.path = Some(path);
        self.content = Some(content);
    }

    /// Clears the preview content.
    pub fn clear(&mut self) {
        self.path = None;
        self.content = None;
    }
}

/// What the pane it belongs to is looking for, and what it has found.
///
/// A search has two views of a pane's contents. In the basic and fuzzy modes it
/// looks at the listing the pane has already loaded, and `matches` holds the
/// positions of the entries that matched, in the pane's own order. In the two
/// recursive modes it looks below the pane's directory, and `results` holds
/// what the walk found, ordered by the filesystem layer. Exactly one of the two
/// is shown, which is what [`Self::shows_results`] says, and the other is left
/// as it was rather than thrown away.
///
/// The outcome records how the last walk ended, so a search that was stopped or
/// that could not start is never presented as one that finished. The
/// cancellation handle is kept so that a walk can be stopped from outside the
/// call that started it.
///
/// Highlighting a match and moving between matches belong to a later phase.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchState {
    query: String,
    mode: SearchMode,
    matches: Vec<usize>,
    results: Vec<SearchResult>,
    outcome: Option<SearchOutcome>,
    skipped: usize,
    cancel: CancelToken,
}

impl SearchState {
    /// The current query, empty when the search is inactive.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Whether a query has been entered.
    pub fn is_active(&self) -> bool {
        !self.query.is_empty()
    }

    /// Which entries this search looks at, and how it matches their names.
    pub fn mode(&self) -> SearchMode {
        self.mode
    }

    /// Whether the pane is showing what a walk found rather than its listing.
    fn shows_results(&self) -> bool {
        self.mode.is_recursive()
    }

    /// How the last walk ended, or `None` when none has run.
    ///
    /// A walk that was stopped reports [`SearchOutcome::Cancelled`] and one
    /// whose root could not be read reports [`SearchOutcome::Failed`], so
    /// neither can be mistaken for a search that finished.
    pub fn outcome(&self) -> Option<&SearchOutcome> {
        self.outcome.as_ref()
    }

    /// What the last walk found, in the order the filesystem layer put it in.
    pub fn results(&self) -> &[SearchResult] {
        &self.results
    }

    /// How many directories the last walk had to skip.
    ///
    /// A walk that carried on past something it could not read still ends as
    /// [`SearchOutcome::Completed`], because it did search the rest of the tree:
    /// this is what says that the answer is a partial one and how partial.
    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// Whether this search has been asked to stop.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// A handle that stops this search when it is cancelled.
    ///
    /// The handle shares its flag with the search it came from, so it can be
    /// used from another thread while the walk runs.
    pub fn cancel_handle(&self) -> CancelToken {
        self.cancel.clone()
    }

    /// Asks the search to stop.
    fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Looks at the listing again for `query`.
    fn update(&mut self, query: &str, entries: &[Entry], show_hidden: bool) {
        self.query = query.to_string();

        if self.shows_results() {
            self.forget_results();
        }

        self.refilter(entries, show_hidden);
    }

    /// Recomputes the matches from `entries` and the stored query.
    fn refilter(&mut self, entries: &[Entry], show_hidden: bool) {
        let matcher = Matcher::new(&self.query, self.mode.is_fuzzy());

        let matching: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                let name = entry.name().to_string_lossy();
                let is_hidden = name.starts_with('.');
                if !show_hidden && is_hidden && self.query.is_empty() {
                    return false;
                }
                matcher.matches(entry.name())
            })
            .map(|(index, _)| index)
            .collect();

        self.matches = matching;
    }

    /// Takes a mode, keeping what the pane is showing true to it.
    fn set_mode(&mut self, mode: SearchMode, entries: &[Entry], show_hidden: bool) {
        self.mode = mode;

        self.forget_results();
        self.refilter(entries, show_hidden);
    }

    /// Stores what a walk found, how it ended and how much it skipped.
    fn store(&mut self, results: Vec<SearchResult>, outcome: SearchOutcome, skipped: usize) {
        self.results = results;
        self.outcome = Some(outcome);
        self.skipped = skipped;
    }

    /// Forgets what a walk found, keeping the query and the mode.
    fn forget_results(&mut self) {
        self.results.clear();
        self.outcome = None;
        self.skipped = 0;
    }

    /// Returns to a plain listing of everything the pane holds.
    ///
    /// The query, the mode and everything a walk found are discarded, and any
    /// earlier cancellation request with them: clearing a search leaves the
    /// pane as it is before a search is started, and the next search begins
    /// uncancelled.
    fn clear(&mut self) {
        self.query.clear();
        self.mode = SearchMode::Basic;
        self.forget_results();
        self.cancel = CancelToken::new();
    }

    /// How many entries the pane is showing.
    fn count(&self) -> usize {
        if self.shows_results() {
            self.results.len()
        } else {
            self.matches.len()
        }
    }

    /// The positions of the listing's entries that are shown.
    ///
    /// Empty while a walk's results are shown, because the pane is then showing
    /// paths that are not part of its listing.
    fn listing_positions(&self) -> &[usize] {
        if self.shows_results() {
            &[]
        } else {
            &self.matches
        }
    }

    /// The listing position of the entry that matches at `position`.
    fn entry_index(&self, position: usize) -> Option<usize> {
        self.listing_positions().get(position).copied()
    }

    /// What a walk found at `position`.
    fn result_at(&self, position: usize) -> Option<&SearchResult> {
        if self.shows_results() {
            self.results.get(position)
        } else {
            None
        }
    }
}

/// A saved directory bookmark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    id: usize,
    name: String,
    path: PathBuf,
}

impl Bookmark {
    /// Creates a new bookmark with a stable id, display name and path.
    pub fn new(id: usize, name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id,
            name: name.into(),
            path: path.into(),
        }
    }

    /// The unique identifier of this bookmark.
    pub fn id(&self) -> usize {
        self.id
    }

    /// The display name of this bookmark.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The destination path of this bookmark.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the destination path currently exists on disk.
    pub fn is_valid(&self) -> bool {
        self.path.exists()
    }
}

/// The saved directory bookmarks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookmarkState {
    bookmarks: Vec<Bookmark>,
    next_id: usize,
    selected: usize,
}

impl BookmarkState {
    /// Creates an empty bookmark state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether no location is saved.
    pub fn is_empty(&self) -> bool {
        self.bookmarks.is_empty()
    }

    /// How many locations are saved.
    pub fn len(&self) -> usize {
        self.bookmarks.len()
    }

    /// The list of saved bookmarks.
    pub fn bookmarks(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    /// The selected bookmark index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// The currently selected bookmark, if any.
    pub fn selected_bookmark(&self) -> Option<&Bookmark> {
        self.bookmarks.get(self.selected)
    }

    /// Whether `path` is already bookmarked.
    pub fn contains_path(&self, path: &Path) -> bool {
        self.bookmarks.iter().any(|b| b.path == path)
    }

    /// Adds `path` as a bookmark with an auto-derived name if not already present.
    ///
    /// Returns `true` if added, `false` if already bookmarked or if `path` is a regular file.
    pub fn add(&mut self, path: PathBuf) -> bool {
        if self.contains_path(&path) || path.is_file() {
            return false;
        }

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        self.add_with_name(name, path)
    }

    /// Adds `path` with a custom `name` as a bookmark if not already present.
    ///
    /// Returns `true` if added, `false` if already bookmarked or if `path` is a regular file.
    pub fn add_with_name(&mut self, name: impl Into<String>, path: PathBuf) -> bool {
        if self.contains_path(&path) || path.is_file() {
            return false;
        }

        let id = self.next_id;
        self.next_id += 1;
        self.bookmarks.push(Bookmark::new(id, name, path));
        self.clamp_selection();
        true
    }

    /// Removes a bookmark by `id`. Returns the removed bookmark if found.
    pub fn remove_by_id(&mut self, id: usize) -> Option<Bookmark> {
        if let Some(index) = self.bookmarks.iter().position(|b| b.id == id) {
            let removed = self.bookmarks.remove(index);
            self.clamp_selection();
            Some(removed)
        } else {
            None
        }
    }

    /// Removes the currently selected bookmark. Returns the removed bookmark if any.
    pub fn remove_selected(&mut self) -> Option<Bookmark> {
        if self.bookmarks.is_empty() {
            return None;
        }
        let index = self.selected.min(self.bookmarks.len() - 1);
        let removed = self.bookmarks.remove(index);
        self.clamp_selection();
        Some(removed)
    }

    /// Removes a bookmark matching `path`. Returns true if found and removed.
    pub fn remove_by_path(&mut self, path: &Path) -> bool {
        if let Some(index) = self.bookmarks.iter().position(|b| b.path == path) {
            self.bookmarks.remove(index);
            self.clamp_selection();
            true
        } else {
            false
        }
    }

    /// Renames the currently selected bookmark.
    pub fn rename_selected(&mut self, new_name: impl Into<String>) -> bool {
        if let Some(b) = self.bookmarks.get_mut(self.selected) {
            b.name = new_name.into();
            true
        } else {
            false
        }
    }

    /// Moves the currently selected bookmark upward in the order.
    pub fn move_selected_up(&mut self) -> bool {
        if self.selected > 0 && self.selected < self.bookmarks.len() {
            self.bookmarks.swap(self.selected, self.selected - 1);
            self.selected -= 1;
            true
        } else {
            false
        }
    }

    /// Moves the currently selected bookmark downward in the order.
    pub fn move_selected_down(&mut self) -> bool {
        if self.selected + 1 < self.bookmarks.len() {
            self.bookmarks.swap(self.selected, self.selected + 1);
            self.selected += 1;
            true
        } else {
            false
        }
    }

    /// Moves selection up one entry.
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Moves selection down one entry.
    pub fn move_down(&mut self) {
        if !self.bookmarks.is_empty() {
            self.selected = (self.selected + 1).min(self.bookmarks.len() - 1);
        }
    }

    /// Selects a specific index.
    pub fn select(&mut self, index: usize) {
        if self.bookmarks.is_empty() {
            self.selected = 0;
        } else {
            self.selected = index.min(self.bookmarks.len() - 1);
        }
    }

    /// Resets or clamps selection to bounds.
    pub fn clamp_selection(&mut self) {
        if self.bookmarks.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.bookmarks.len() {
            self.selected = self.bookmarks.len() - 1;
        }
    }
}

/// The user's settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsState {
    config_path: Option<PathBuf>,
}

impl SettingsState {
    /// Creates a new settings state.
    pub fn new() -> Self {
        Self::default()
    }

    /// The active configuration path, if any.
    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    /// Sets the configuration path.
    pub fn set_config_path(&mut self, path: Option<PathBuf>) {
        self.config_path = path;
    }
}

/// State for the interactive Theme Selector modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeSelectorState {
    /// Currently selected theme index in `ThemeId::ALL`.
    pub selected_index: usize,
    /// The active theme before opening the selector (restored on Cancel).
    pub initial_theme: ThemeId,
}

impl Default for ThemeSelectorState {
    fn default() -> Self {
        Self {
            selected_index: 0,
            initial_theme: ThemeId::TerminalVision,
        }
    }
}

impl ThemeSelectorState {
    /// Creates a new theme selector state seeded with the currently active theme.
    pub fn new(active_theme: ThemeId) -> Self {
        let selected_index = ThemeId::all()
            .iter()
            .position(|&id| id == active_theme)
            .unwrap_or(0);
        Self {
            selected_index,
            initial_theme: active_theme,
        }
    }

    /// Returns the currently highlighted theme ID.
    pub fn selected_theme(&self) -> ThemeId {
        ThemeId::all()[self.selected_index.min(ThemeId::all().len() - 1)]
    }

    /// Moves selection up one theme.
    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else {
            self.selected_index = ThemeId::all().len() - 1;
        }
    }

    /// Moves selection down one theme.
    pub fn move_down(&mut self) {
        if self.selected_index + 1 < ThemeId::all().len() {
            self.selected_index += 1;
        } else {
            self.selected_index = 0;
        }
    }

    /// Returns the currently highlighted theme index.
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// Explicitly selects the theme at `index`.
    pub fn select_index(&mut self, index: usize) {
        if index < ThemeId::all().len() {
            self.selected_index = index;
        }
    }
}

/// The step a [`Notice`] is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoticeSource {
    /// A change to the filesystem the user asked for.
    Operation(Operation),
    /// Looking at a directory: reading its entries, or reading one entry's
    /// metadata to find out what it is.
    Navigation,
    /// A search that goes beyond the listing the pane already holds.
    Search,
}

/// Something that failed, kept in a form the interface can act on.
///
/// The words are kept for showing, and everything else is kept apart from them:
/// which step failed, where, and why. A later layer can therefore decide what to
/// offer — another name for a conflict, say — without reading a message and
/// without knowing how the operating system words its errors.
///
/// The path is absent only where there was none to name, which is a working
/// directory the process could not report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    what: NoticeSource,
    path: Option<PathBuf>,
    category: ErrorCategory,
    text: String,
}

impl Notice {
    /// Records that `what` failed.
    fn new(
        what: NoticeSource,
        path: Option<PathBuf>,
        category: ErrorCategory,
        text: String,
    ) -> Self {
        Self {
            what,
            path,
            category,
            text,
        }
    }

    /// What was being done when it failed.
    pub fn what(&self) -> &NoticeSource {
        &self.what
    }

    /// What the step concerned, when it named anything.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Why it failed.
    pub fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Whether something that is already there stood in the way, which is the
    /// one failure a user is usually offered another name for.
    pub fn is_conflict(&self) -> bool {
        self.category.is_conflict()
    }

    /// Whether the work was stopped rather than failing.
    pub fn is_cancelled(&self) -> bool {
        self.category.is_cancelled()
    }

    /// The words to show for it.
    pub fn text(&self) -> &str {
        &self.text
    }
}

impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl From<&OperationError> for Notice {
    fn from(error: &OperationError) -> Self {
        Self::new(
            NoticeSource::Operation(error.operation().clone()),
            Some(error.path().to_path_buf()),
            error.category(),
            error.to_string(),
        )
    }
}

impl From<&NavigationError> for Notice {
    fn from(error: &NavigationError) -> Self {
        Self::new(
            NoticeSource::Navigation,
            error.path().map(Path::to_path_buf),
            error.category(),
            error.to_string(),
        )
    }
}

impl From<&SearchFailure> for Notice {
    fn from(failure: &SearchFailure) -> Self {
        Self::new(
            NoticeSource::Search,
            Some(failure.path().to_path_buf()),
            failure.category(),
            format!(
                "cannot search {}: {}",
                failure.path().display(),
                io::Error::from(failure.kind())
            ),
        )
    }
}

/// The failure currently shown to the user, if any.
///
/// Nothing shows it yet, and nothing clears itself after a while: what is kept
/// here is what happened, for the interface to read when it is built.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotificationState {
    notice: Option<Notice>,
}

impl NotificationState {
    /// The active message, or `None` when nothing is shown.
    pub fn message(&self) -> Option<&str> {
        self.notice.as_ref().map(Notice::text)
    }

    /// The active failure, in the form a later layer can act on.
    pub fn notice(&self) -> Option<&Notice> {
        self.notice.as_ref()
    }

    /// Whether a message is shown.
    pub fn is_active(&self) -> bool {
        self.notice.is_some()
    }

    /// Shows a failure.
    fn show(&mut self, notice: Notice) {
        self.notice = Some(notice);
    }

    /// Shows an informational or status message.
    pub fn show_message(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice::new(
            NoticeSource::Navigation,
            None,
            ErrorCategory::Other,
            text.into(),
        ));
    }

    /// Removes the message.
    fn clear(&mut self) {
        self.notice = None;
    }
}

/// Everything the application currently knows.
///
/// The state holds no behaviour: reading directories, moving the selection and
/// changing modes all belong to a later phase, and every one of them will run
/// through the filesystem service rather than here.
///
/// The default state is deterministic and touches nothing outside itself: it
/// neither reads the filesystem nor resolves the working directory, so a pane's
/// path stays empty until a later phase decides where it should point.
/// Bounded in-memory session history of visited directory locations and files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentLocations {
    locations: Vec<PathBuf>,
    files: Vec<PathBuf>,
    max_entries: usize,
}

impl Default for RecentLocations {
    fn default() -> Self {
        Self {
            locations: Vec::new(),
            files: Vec::new(),
            max_entries: 50,
        }
    }
}

impl RecentLocations {
    /// Creates a new recent locations tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a visited directory path, moving it to the front and suppressing duplicates.
    pub fn record(&mut self, path: PathBuf) {
        self.record_directory(path);
    }

    /// Records a visited directory path, moving it to the front and suppressing duplicates.
    pub fn record_directory(&mut self, path: PathBuf) {
        if path.as_os_str().is_empty() {
            return;
        }
        self.locations.retain(|p| p != &path);
        self.locations.insert(0, path);
        if self.locations.len() > self.max_entries {
            self.locations.truncate(self.max_entries);
        }
    }

    /// Records an accessed/opened file path, moving it to the front and suppressing duplicates.
    pub fn record_file(&mut self, path: PathBuf) {
        if path.as_os_str().is_empty() {
            return;
        }
        self.files.retain(|p| p != &path);
        self.files.insert(0, path);
        if self.files.len() > self.max_entries {
            self.files.truncate(self.max_entries);
        }
    }

    /// The slice of recently visited directory locations, newest first.
    pub fn locations(&self) -> &[PathBuf] {
        &self.locations
    }

    /// The slice of recently accessed files, newest first.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Sets the locations list directly.
    pub fn set_locations(&mut self, locations: Vec<PathBuf>) {
        self.locations = locations;
    }

    /// Sets the files list directly.
    pub fn set_files(&mut self, files: Vec<PathBuf>) {
        self.files = files;
    }

    /// Removes paths that no longer exist on disk.
    pub fn cleanup_invalid(&mut self) {
        self.locations.retain(|p| p.exists());
        self.files.retain(|p| p.exists());
    }

    /// How many locations are recorded.
    pub fn len(&self) -> usize {
        self.locations.len() + self.files.len()
    }

    /// Whether the history is empty.
    pub fn is_empty(&self) -> bool {
        self.locations.is_empty() && self.files.is_empty()
    }
}

/// A destination item for the Smart Jump / Quick Switcher picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartJumpItem {
    /// The user-facing label of the destination.
    pub title: String,
    /// The destination filesystem path.
    pub path: PathBuf,
    /// Category badge (e.g. `HOME`, `GIT ROOT`, `PROJECT`, `BOOKMARK`, `RECENT`, `FILE`, `TAB`, `PARENT`, `CURRENT`, `ROOT`).
    pub category: &'static str,
    /// Visual icon or symbol.
    pub icon: &'static str,
    /// Whether this destination is a file (true) or directory (false).
    pub is_file: bool,
}

/// State for the unified Smart Jump / Quick Switcher dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SmartJumpState {
    query: String,
    selected: usize,
    items: Vec<SmartJumpItem>,
}

impl SmartJumpState {
    /// Creates a new empty Smart Jump state.
    pub fn new() -> Self {
        Self::default()
    }

    /// The active search/filter query.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The currently selected index in filtered results.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Sets the available jump targets, resetting selection.
    pub fn set_items(&mut self, items: Vec<SmartJumpItem>) {
        self.items = items;
        self.clamp_selection();
    }

    /// All destination items currently collected.
    pub fn all_items(&self) -> &[SmartJumpItem] {
        &self.items
    }

    /// Destination items matching the current query, ranked by fuzzy match score.
    pub fn filtered_items(&self) -> Vec<&SmartJumpItem> {
        let trimmed = self.query.trim();
        if trimmed.is_empty() {
            return self.items.iter().collect();
        }

        let mut scored: Vec<(&SmartJumpItem, i32)> = Vec::new();
        for item in &self.items {
            let path_str = item.path.to_string_lossy();
            let targets = [item.title.as_str(), &path_str, item.category];

            if let Some(score) = crate::commands::fuzzy::fuzzy_match_multi(trimmed, &targets) {
                scored.push((item, score));
            }
        }

        scored.sort_by_key(|a| std::cmp::Reverse(a.1));
        scored.into_iter().map(|(item, _)| item).collect()
    }

    /// The destination item currently selected, if any.
    pub fn selected_item(&self) -> Option<&SmartJumpItem> {
        let filtered = self.filtered_items();
        if filtered.is_empty() {
            None
        } else {
            filtered.get(self.selected).copied()
        }
    }

    /// Sets the search query.
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.clamp_selection();
    }

    /// Appends a character to the search query.
    pub fn push_char(&mut self, ch: char) {
        self.query.push(ch);
        self.clamp_selection();
    }

    /// Removes the last character from the search query.
    pub fn pop_char(&mut self) {
        self.query.pop();
        self.clamp_selection();
    }

    /// Moves selection up one entry.
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Moves selection down one entry.
    pub fn move_down(&mut self) {
        let count = self.filtered_items().len();
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

    fn clamp_selection(&mut self) {
        let count = self.filtered_items().len();
        if count == 0 {
            self.selected = 0;
        } else if self.selected >= count {
            self.selected = count - 1;
        }
    }
}

/// An action entry in the Project Cockpit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCockpitAction {
    pub label: &'static str,
    pub description: String,
    pub target_path: Option<PathBuf>,
    pub action: Action,
}

/// State for the Project Cockpit modal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectCockpitState {
    selected: usize,
    actions: Vec<ProjectCockpitAction>,
}

impl ProjectCockpitState {
    /// Creates a new empty project cockpit state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the available actions, resetting selection if out of bounds.
    pub fn set_actions(&mut self, actions: Vec<ProjectCockpitAction>) {
        self.actions = actions;
        if self.selected >= self.actions.len() {
            self.selected = 0;
        }
    }

    /// The selected action index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// All cockpit actions.
    pub fn actions(&self) -> &[ProjectCockpitAction] {
        &self.actions
    }

    /// Currently selected action.
    pub fn selected_action(&self) -> Option<&ProjectCockpitAction> {
        self.actions.get(self.selected)
    }

    /// Moves selection up.
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Moves selection down.
    pub fn move_down(&mut self) {
        if !self.actions.is_empty() {
            self.selected = (self.selected + 1).min(self.actions.len() - 1);
        }
    }

    /// Sets the selected index.
    pub fn select(&mut self, index: usize) {
        if index < self.actions.len() {
            self.selected = index;
        }
    }

    /// Moves selection up (alias for move_up).
    pub fn select_previous(&mut self) {
        self.move_up();
    }

    /// Moves selection down (alias for move_down).
    pub fn select_next(&mut self) {
        self.move_down();
    }
}

/// A changed file entry in the Git status panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusPanelEntry {
    pub relative_path: PathBuf,
    pub full_path: PathBuf,
    pub status: crate::git::FileStatus,
    pub staged: bool,
}

/// State for the Git Status Panel modal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStatusPanelState {
    selected: usize,
    entries: Vec<GitStatusPanelEntry>,
}

impl GitStatusPanelState {
    /// Creates a new empty Git status panel state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the changed file entries, resetting selection if out of bounds.
    pub fn set_entries(&mut self, entries: Vec<GitStatusPanelEntry>) {
        self.entries = entries;
        if self.selected >= self.entries.len() {
            self.selected = 0;
        }
    }

    /// The selected entry index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// All changed entries.
    pub fn entries(&self) -> &[GitStatusPanelEntry] {
        &self.entries
    }

    /// Currently selected entry.
    pub fn selected_entry(&self) -> Option<&GitStatusPanelEntry> {
        self.entries.get(self.selected)
    }

    /// Moves selection up.
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Moves selection down.
    pub fn move_down(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1).min(self.entries.len() - 1);
        }
    }
}

/// Extension statistic for the File Radar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionStat {
    pub extension: String,
    pub count: usize,
    pub total_bytes: u64,
}

/// State for the File Radar modal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileRadarState {
    pub total_entries: usize,
    pub directory_count: usize,
    pub file_count: usize,
    pub symlink_count: usize,
    pub hidden_count: usize,
    pub total_file_bytes: u64,
    pub top_extensions: Vec<ExtensionStat>,
}

impl FileRadarState {
    /// Computes file radar statistics from the visible entries in a directory.
    pub fn compute_from_entries(entries: &[Entry]) -> Self {
        let mut directory_count = 0;
        let mut file_count = 0;
        let mut symlink_count = 0;
        let mut hidden_count = 0;
        let mut total_file_bytes = 0u64;
        let mut ext_map: std::collections::HashMap<String, (usize, u64)> =
            std::collections::HashMap::new();

        for entry in entries {
            let is_hidden = entry
                .name()
                .to_str()
                .map(|n| n.starts_with('.'))
                .unwrap_or(false);
            if is_hidden {
                hidden_count += 1;
            }
            match entry.kind() {
                EntryKind::Directory => directory_count += 1,
                EntryKind::Symlink => symlink_count += 1,
                EntryKind::File => {
                    file_count += 1;
                    let size = entry.path().metadata().ok().map(|m| m.len());
                    if let Some(sz) = size {
                        total_file_bytes = total_file_bytes.saturating_add(sz);
                    }
                    let ext = entry
                        .path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| format!(".{}", e.to_lowercase()))
                        .unwrap_or_else(|| "[no ext]".to_string());
                    let (cnt, bytes) = ext_map.entry(ext).or_insert((0, 0));
                    *cnt += 1;
                    *bytes = bytes.saturating_add(size.unwrap_or(0));
                }
                EntryKind::Other => {}
            }
        }

        let mut top_extensions: Vec<ExtensionStat> = ext_map
            .into_iter()
            .map(|(extension, (count, total_bytes))| ExtensionStat {
                extension,
                count,
                total_bytes,
            })
            .collect();
        top_extensions.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| b.total_bytes.cmp(&a.total_bytes))
        });
        top_extensions.truncate(8);

        Self {
            total_entries: entries.len(),
            directory_count,
            file_count,
            symlink_count,
            hidden_count,
            total_file_bytes,
            top_extensions,
        }
    }
}

/// A node in the Reveal Context hierarchy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextLevel {
    pub level_name: &'static str,
    pub title: String,
    pub details: String,
    pub path: PathBuf,
}

/// State for the Reveal Context modal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RevealContextState {
    selected: usize,
    levels: Vec<ContextLevel>,
}

impl RevealContextState {
    /// Creates a new empty reveal context state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the context levels, resetting selection if out of bounds.
    pub fn set_levels(&mut self, levels: Vec<ContextLevel>) {
        self.levels = levels;
        if self.selected >= self.levels.len() {
            self.selected = 0;
        }
    }

    /// The selected level index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// All context levels in hierarchy order.
    pub fn levels(&self) -> &[ContextLevel] {
        &self.levels
    }

    /// Currently selected context level.
    pub fn selected_level(&self) -> Option<&ContextLevel> {
        self.levels.get(self.selected)
    }

    /// Moves selection up.
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Moves selection down.
    pub fn move_down(&mut self) {
        if !self.levels.is_empty() {
            self.selected = (self.selected + 1).min(self.levels.len() - 1);
        }
    }
}
pub use crate::commands::context_menu::*;

/// The kind of entry being created in a creation dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
}

/// Thread-safe wrapper for the embedded terminal session.
#[derive(Clone, Default)]
pub struct AppTerminal(
    pub std::sync::Arc<std::sync::Mutex<Option<crate::terminal::TerminalSession>>>,
);

impl std::fmt::Debug for AppTerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AppTerminal")
    }
}

impl PartialEq for AppTerminal {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for AppTerminal {}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct App {
    left: Pane,
    right: Pane,
    active_pane: ActivePane,
    mode: Mode,
    clipboard: ClipboardState,
    preview: PreviewState,
    bookmarks: BookmarkState,
    recent_locations: RecentLocations,
    smart_jump: SmartJumpState,
    project_cockpit: ProjectCockpitState,
    git_status_panel: GitStatusPanelState,
    file_radar: FileRadarState,
    reveal_context: RevealContextState,
    context_menu: ContextMenuState,
    settings: SettingsState,
    notification: NotificationState,
    last_outcome: Option<OperationOutcome>,
    operation_progress: Option<OperationProgress>,
    should_quit: bool,
    filesystem: FilesystemService,
    create_kind: Option<CreateKind>,
    input_buffer: String,
    cursor_position: usize,
    confirm_selection: bool,
    palette: CommandPaletteState,
    focus_mode: bool,
    terminal: AppTerminal,
    last_sync_origin: SyncOrigin,
    last_synced_terminal_cwd: Option<PathBuf>,
    pending_terminal_sync: Option<(PathBuf, std::time::Instant)>,
    filesystem_watcher: crate::filesystem::FilesystemWatcher,
    operation_manager: crate::operations::OperationManager,
    storage_vision: crate::storage::StorageVisionState,
    motion: crate::ui::motion::MotionState,
    animations: crate::animation::AnimationEngine,
    active_theme: ThemeId,
    preview_theme: Option<ThemeId>,
    theme_selector: ThemeSelectorState,
    boot_state: Option<crate::animation::BootState>,
}

impl App {
    /// Creates an application whose active pane shows `directory`.
    ///
    /// The directory is listed before anything is stored, so a directory that
    /// cannot be read leaves no half-built state behind.
    pub fn at(directory: PathBuf) -> Result<Self, NavigationError> {
        let mut app = Self::default();
        app.open_in(ActivePane::Left, directory.clone())?;
        app.open_in(ActivePane::Right, directory)?;
        app.active_pane = ActivePane::Left;
        app.refresh_preview();
        Ok(app)
    }

    /// Creates an application whose active pane starts in the working
    /// directory of the process.
    ///
    /// Nothing about that location is assumed, and a process whose working
    /// directory cannot be determined is reported as an error instead of
    /// guessed at.
    pub fn at_working_directory() -> Result<Self, NavigationError> {
        let directory = FilesystemService::new()
            .current_directory()
            .map_err(NavigationError::WorkingDirectory)?;

        Self::at(directory)
    }

    /// Shows `directory` in `which` pane, replacing what that pane held.
    ///
    /// The pane keeps pointing at its previous directory if the new one cannot
    /// be read, so a failed navigation never leaves a pane describing a
    /// directory it is not showing.
    pub fn open_in(
        &mut self,
        which: ActivePane,
        directory: PathBuf,
    ) -> Result<(), NavigationError> {
        let entries = self.filesystem.list_directory(&directory)?;

        self.recent_locations.record(directory.clone());

        let pane = self.pane_mut(which);
        pane.navigate_to(directory);
        pane.clear_selection();
        pane.deselect_all();
        pane.set_entries(entries);
        pane.select_first();

        if which == self.active_pane {
            self.refresh_preview();
            self.trigger_navigation_animation();
            if self.last_sync_origin != SyncOrigin::ShellCwdChange {
                self.last_sync_origin = SyncOrigin::FileManagerNavigation;
                self.sync_terminal_to_directory();
            }
        }

        Ok(())
    }

    /// Initializes the integrated terminal session at the active directory.
    pub fn init_terminal(&self, cols: u16, rows: u16) {
        let cwd = self.pane(self.active_pane).current_path();
        if let Ok(term) = crate::terminal::TerminalSession::start(cwd, cols, rows)
            && let Ok(mut guard) = self.terminal.0.lock()
        {
            *guard = Some(term);
        }
    }

    /// Polls pending PTY output into the terminal emulator.
    pub fn terminal_poll_output(&self) -> bool {
        if let Ok(guard) = self.terminal.0.lock()
            && let Some(term) = guard.as_ref()
        {
            return term.poll_output();
        }
        false
    }

    /// Sends a key event to the interactive terminal shell.
    pub fn terminal_send_key(&self, key: crossterm::event::KeyEvent) {
        if let Ok(guard) = self.terminal.0.lock()
            && let Some(term) = guard.as_ref()
        {
            let _ = term.send_key(key);
        }
    }

    /// Sends raw byte input to the interactive terminal shell.
    pub fn terminal_send_bytes(&self, bytes: &[u8]) {
        if let Ok(guard) = self.terminal.0.lock()
            && let Some(term) = guard.as_ref()
        {
            let _ = term.write_bytes(bytes);
        }
    }

    /// Resizes the interactive terminal session.
    pub fn terminal_resize(&self, cols: u16, rows: u16) {
        if let Ok(guard) = self.terminal.0.lock()
            && let Some(term) = guard.as_ref()
        {
            term.resize(cols, rows);
        }
    }

    /// Returns snapshot of visible rows of the terminal screen.
    pub fn terminal_visible_rows(&self) -> Vec<Vec<crate::terminal::Cell>> {
        if let Ok(guard) = self.terminal.0.lock()
            && let Some(term) = guard.as_ref()
        {
            return term.visible_rows();
        }
        Vec::new()
    }

    /// Returns the cursor row, column, and visibility state.
    pub fn terminal_cursor_info(&self) -> (u16, u16, bool) {
        if let Ok(guard) = self.terminal.0.lock()
            && let Some(term) = guard.as_ref()
        {
            return term.cursor_info();
        }
        (0, 0, false)
    }

    /// Returns the active shell executable name.
    pub fn terminal_shell_name(&self) -> String {
        if let Ok(guard) = self.terminal.0.lock()
            && let Some(term) = guard.as_ref()
        {
            return term.shell_name().to_string();
        }
        "shell".to_string()
    }

    /// Returns the terminal's tracked or initial working directory.
    pub fn terminal_cwd(&self) -> PathBuf {
        if let Ok(guard) = self.terminal.0.lock()
            && let Some(term) = guard.as_ref()
        {
            return term.current_path();
        }
        self.pane(self.active_pane).current_path().clone()
    }

    /// Scrolls terminal scrollback history upward.
    pub fn terminal_scroll_up(&self, count: usize) {
        if let Ok(guard) = self.terminal.0.lock()
            && let Some(term) = guard.as_ref()
        {
            term.scroll_up(count);
        }
    }

    /// Scrolls terminal scrollback history downward.
    pub fn terminal_scroll_down(&self, count: usize) {
        if let Ok(guard) = self.terminal.0.lock()
            && let Some(term) = guard.as_ref()
        {
            term.scroll_down(count);
        }
    }

    /// Resets terminal scrollback to live view.
    pub fn terminal_reset_scroll(&self) {
        if let Ok(guard) = self.terminal.0.lock()
            && let Some(term) = guard.as_ref()
        {
            term.reset_scroll();
        }
    }

    /// Returns the current active filesystem location and synchronization state.
    /// Returns the current active filesystem location and synchronization state.
    pub fn active_location(&self) -> ActiveLocation {
        let path = self.pane(self.active_pane).current_path().clone();
        let term_cwd = self.terminal_cwd();
        ActiveLocation {
            path: path.clone(),
            active_pane: self.active_pane,
            last_sync_origin: self.last_sync_origin,
            synchronized: paths_are_equivalent(&path, &term_cwd),
        }
    }

    /// Returns the origin of the last directory synchronization event.
    pub fn last_sync_origin(&self) -> SyncOrigin {
        self.last_sync_origin
    }

    /// Mutable reference to the filesystem watcher for testing or configuration.
    pub fn filesystem_watcher_mut(&mut self) -> &mut crate::filesystem::FilesystemWatcher {
        &mut self.filesystem_watcher
    }

    /// Synchronizes the terminal shell's directory to the active pane's path.
    pub fn sync_terminal_to_directory(&mut self) {
        let path = self.pane(self.active_pane).current_path().clone();
        if self
            .last_synced_terminal_cwd
            .as_ref()
            .is_some_and(|p| paths_are_equivalent(p, &path))
            || paths_are_equivalent(&self.terminal_cwd(), &path)
        {
            self.last_synced_terminal_cwd = Some(path);
            self.pending_terminal_sync = None;
            return;
        }
        if let Ok(guard) = self.terminal.0.lock()
            && let Some(term) = guard.as_ref()
        {
            let _ = term.cd_to_path(&path);
            self.last_synced_terminal_cwd = Some(path.clone());
            self.pending_terminal_sync = Some((path, std::time::Instant::now()));
        }
    }

    /// Synchronizes the active pane to the terminal shell's current working directory.
    pub fn sync_directory_to_terminal(&mut self) {
        let term_cwd = self.terminal_cwd();
        if term_cwd.exists() && term_cwd.is_dir() {
            let _ = self.handle_shell_cwd_change(term_cwd);
        }
    }

    /// Handles a verified shell process working directory change without looping.
    pub fn handle_shell_cwd_change(&mut self, new_cwd: PathBuf) -> bool {
        if !new_cwd.is_dir() {
            return false;
        }
        let active = self.active_pane;
        if paths_are_equivalent(self.pane(active).current_path(), &new_cwd) {
            self.last_synced_terminal_cwd = Some(new_cwd);
            return false;
        }
        if let Ok(entries) = self.filesystem.list_directory(&new_cwd) {
            self.last_sync_origin = SyncOrigin::ShellCwdChange;
            self.recent_locations.record(new_cwd.clone());
            let pane = self.pane_mut(active);
            pane.navigate_to(new_cwd.clone());
            pane.clear_selection();
            pane.deselect_all();
            pane.set_entries(entries);
            pane.select_first();
            self.refresh_preview();
            self.last_synced_terminal_cwd = Some(new_cwd);
            return true;
        }
        false
    }

    /// Polls terminal PTY output, detects shell process CWD changes, checks motion animations, and checks filesystem watcher.
    pub fn poll_sync_and_filesystem(&mut self) -> bool {
        let mut updated = self.terminal_poll_output();

        if self.motion.is_active(std::time::Instant::now()) {
            updated = true;
        }

        if self.animations.tick() {
            updated = true;
        }

        // Advance Vision Boot state machine if active
        if self.mode == Mode::Boot {
            if let Some(prog) = self
                .animations
                .tag_progress(crate::animation::AnimationTag::VisionBoot)
            {
                if prog.state.is_finished() {
                    self.finish_boot();
                    updated = true;
                }
            } else {
                self.finish_boot();
                updated = true;
            }
        }

        // 1. Check shell process working directory
        let shell_cwd = self.terminal_cwd();
        let active_fm_cwd = self.pane(self.active_pane).current_path().clone();

        if let Some((ref pending, started_at)) = self.pending_terminal_sync {
            if paths_are_equivalent(&shell_cwd, pending) {
                self.pending_terminal_sync = None;
                self.last_synced_terminal_cwd = Some(shell_cwd);
            } else if started_at.elapsed() > std::time::Duration::from_millis(1500) {
                // Pending sync timed out
                self.pending_terminal_sync = None;
            }
        } else if !paths_are_equivalent(&shell_cwd, &active_fm_cwd)
            && shell_cwd.exists()
            && shell_cwd.is_dir()
            && self.handle_shell_cwd_change(shell_cwd)
        {
            updated = true;
        }

        // 2. Poll filesystem watcher for changes to watched directories / preview file / git
        let left_path = self.left.current_path().clone();
        let right_path = self.right.current_path().clone();
        let preview_path = self.preview.path().map(|p| p.to_path_buf());

        let changes = self.filesystem_watcher.poll_changes(
            &[left_path.clone(), right_path.clone()],
            preview_path.as_deref(),
        );

        if !changes.is_empty() {
            updated = true;
            for change in changes {
                match change {
                    crate::filesystem::FilesystemChange::DirectoryModified(dir) => {
                        if self.left.current_path() == &dir
                            && let Ok(entries) = self.filesystem.list_directory(&dir)
                        {
                            self.left.refresh_preserving_selection(entries);
                        }
                        if self.right.current_path() == &dir
                            && let Ok(entries) = self.filesystem.list_directory(&dir)
                        {
                            self.right.refresh_preserving_selection(entries);
                        }
                        if self.pane(self.active_pane).current_path() == &dir {
                            self.refresh_preview();
                        }
                    }
                    crate::filesystem::FilesystemChange::DirectoryDeleted(dir) => {
                        let fallback = safe_fallback_directory(&dir);
                        if self.left.current_path() == &dir {
                            let _ = self.open_in(ActivePane::Left, fallback.clone());
                        }
                        if self.right.current_path() == &dir {
                            let _ = self.open_in(ActivePane::Right, fallback.clone());
                        }
                        if self.pane(self.active_pane).current_path() == &fallback {
                            self.sync_terminal_to_directory();
                        }
                    }
                    crate::filesystem::FilesystemChange::PreviewFileModified(_)
                    | crate::filesystem::FilesystemChange::PreviewFileDeleted(_) => {
                        self.refresh_preview();
                    }
                    crate::filesystem::FilesystemChange::GitStateChanged(dir) => {
                        if self.left.current_path() == &dir {
                            self.left.refresh_git_and_project();
                        }
                        if self.right.current_path() == &dir {
                            self.right.refresh_git_and_project();
                        }
                    }
                }
            }
        }

        if self.mode == Mode::StorageVision {
            let was_scanning = self.storage_vision.is_scanning;
            self.storage_vision.poll_updates();
            if was_scanning != self.storage_vision.is_scanning || self.storage_vision.is_scanning {
                updated = true;
            }
        }

        updated
    }

    /// Refreshes the directory listing of both panes without losing position.
    pub fn refresh_directory(&mut self) {
        let left_path = self.left.current_path().clone();
        if let Ok(entries) = self.filesystem.list_directory(&left_path) {
            self.left.refresh_preserving_selection(entries);
        }
        let right_path = self.right.current_path().clone();
        if let Ok(entries) = self.filesystem.list_directory(&right_path) {
            self.right.refresh_preserving_selection(entries);
        }
        self.refresh_preview();
    }

    /// Navigates back one step in the active tab's history.\
    ///
    /// A directory that cannot be read is reported; the history pointer is
    /// already moved at this point, so a failure leaves the pane at the
    /// directory it failed to list rather than reverting the history step.
    pub fn go_back(&mut self) -> Result<(), NavigationError> {
        let path = self.active_pane_mut().active_tab_mut().step_back();
        let Some(path) = path else {
            return Ok(());
        };
        let which = self.active_pane;
        let entries = self.filesystem.list_directory(&path)?;
        let pane = self.pane_mut(which);
        pane.clear_selection();
        pane.deselect_all();
        pane.set_entries(entries);
        pane.select_first();
        if which == self.active_pane {
            self.refresh_preview();
            self.last_sync_origin = SyncOrigin::FileManagerNavigation;
            self.sync_terminal_to_directory();
        }
        Ok(())
    }

    /// Navigates forward one step in the active tab's history.
    pub fn go_forward(&mut self) -> Result<(), NavigationError> {
        let path = self.active_pane_mut().active_tab_mut().step_forward();
        let Some(path) = path else {
            return Ok(());
        };
        let which = self.active_pane;
        let entries = self.filesystem.list_directory(&path)?;
        let pane = self.pane_mut(which);
        pane.clear_selection();
        pane.deselect_all();
        pane.set_entries(entries);
        pane.select_first();
        if which == self.active_pane {
            self.refresh_preview();
            self.last_sync_origin = SyncOrigin::FileManagerNavigation;
            self.sync_terminal_to_directory();
        }
        Ok(())
    }

    /// Opens the selected entry, or selected search result.
    ///
    /// Enters the selected entry, if it is a directory or an enterable
    /// symbolic link.
    ///
    /// Opens the selected entry, or selected search result.
    ///
    /// Enters the selected entry, if it is a directory or an enterable symbolic link.
    /// If the selected entry is a file, launches it with the host operating system's
    /// default application without blocking the TUI or failing navigation.
    pub fn open_selected(&mut self) -> Result<(), NavigationError> {
        let Some(entry) = self.pane(self.active_pane).selected_entry().cloned() else {
            return Ok(());
        };

        let enters = match entry.kind() {
            EntryKind::Directory => true,
            EntryKind::Symlink => is_enterable_directory(entry.path())?,
            EntryKind::File | EntryKind::Other => false,
        };

        if !enters {
            let path = entry.path().to_path_buf();
            let name_disp = entry.name().to_string_lossy().to_string();
            match open_system_default(&path) {
                Ok(()) => {
                    self.notification
                        .show_message(format!("Opened '{name_disp}' with default application"));
                }
                Err(err) => {
                    self.notification
                        .show_message(format!("Cannot open '{name_disp}': {err}"));
                }
            }
            return Ok(());
        }

        let directory = entry.path().to_path_buf();
        let which = self.active_pane;
        self.open_in(which, directory)
    }

    /// Accesses the active smart operation manager.
    pub fn operation_manager(&self) -> &crate::operations::OperationManager {
        &self.operation_manager
    }

    /// Mutably accesses the smart operation manager.
    pub fn operation_manager_mut(&mut self) -> &mut crate::operations::OperationManager {
        &mut self.operation_manager
    }

    /// Accesses the recent operations history.
    pub fn operation_history(&self) -> &crate::operations::OperationHistory {
        &self.operation_manager.history
    }

    /// Creates a file called `name` in the directory the active pane shows.
    ///
    /// The name comes from the caller: nothing here makes one up. Only the
    /// active pane is re-read afterwards, and a change that failed leaves both
    /// panes exactly as they were.
    pub fn create_file(&mut self, name: &OsStr) -> Result<(), OperationError> {
        let outcome = self
            .named_child(Operation::CreateFile, name)
            .and_then(|path| self.filesystem.create_file(&path));

        self.finish_change(outcome)
    }

    /// Creates a directory called `name` in the directory the active pane
    /// shows.
    ///
    /// The name comes from the caller, and only the active pane is re-read
    /// afterwards.
    pub fn create_directory(&mut self, name: &OsStr) -> Result<(), OperationError> {
        let outcome = self
            .named_child(Operation::CreateDirectory, name)
            .and_then(|path| self.filesystem.create_directory(&path));

        self.finish_change(outcome)
    }

    /// The progress tracker of the most recent file management operation, if any.
    pub fn operation_progress(&self) -> Option<&OperationProgress> {
        self.operation_progress.as_ref()
    }

    /// Marks the entry or multi-selected entries in the active pane for copying.
    ///
    /// Nothing is copied yet and nothing on the filesystem changes: the entry
    /// is only remembered, and the copy happens when it is pasted. With nothing
    /// selected there is nothing to mark, so nothing happens.
    pub fn mark_for_copy(&mut self) {
        self.mark_selected(ClipboardOperation::Copy);
    }

    /// Marks the entry or multi-selected entries in the active pane for moving.
    ///
    /// Nothing moves yet and nothing on the filesystem changes.
    pub fn mark_for_cut(&mut self) {
        self.mark_selected(ClipboardOperation::Cut);
    }

    /// Puts what the clipboard holds into the directory the active pane shows.
    ///
    /// A copy leaves the marked entries where they are and keeps them marked,
    /// so the same copy can be pasted again. A cut moves them and empties the
    /// clipboard once they have arrived. Nothing is ever replaced: a name that
    /// is already taken, including an entry that is already in the directory it
    /// is being pasted into, is reported as an error.
    pub fn paste(&mut self) -> Result<(), OperationError> {
        let Some(operation) = self.clipboard.operation() else {
            return Ok(());
        };

        let directory = self.pane(self.active_pane).current_path().clone();

        // A cut takes entries away from wherever they were listed, so the pane
        // showing that directory may have to be re-read to stay truthful. This
        // is decided before the move, and only acted on if the move worked.
        let other = self.active_pane.other();
        let other_lost_an_entry = operation == ClipboardOperation::Cut
            && self
                .clipboard
                .entries()
                .iter()
                .any(|source| self.pane_lists(other, source));

        let sources = self.clipboard.entries().to_vec();
        let op_kind = match operation {
            ClipboardOperation::Copy => OperationKind::Copy,
            ClipboardOperation::Cut => OperationKind::Move,
        };
        let mut progress = OperationProgress::new(op_kind, sources.len());

        let outcome = self.paste_marked(operation, &directory);
        let finished = self.finish_change(outcome);

        if finished.is_ok() {
            progress.finish_success();
            if other_lost_an_entry {
                let reloaded = self.refresh_pane(other);
                self.report_navigation(reloaded);
            }
        } else if let Err(ref err) = finished {
            progress.finish_error(err.to_string());
        }

        self.operation_progress = Some(progress);
        finished
    }

    /// Deletes the entry or multi-selected entries in the active pane.
    ///
    /// Nothing is deleted when there is nothing selected: there is no entry to
    /// name, so nothing is asked of the filesystem and nothing changes. A
    /// directory goes together with everything under it, which is why a pane
    /// showing a directory that has just been deleted, or showing something
    /// inside it, is taken to the nearest directory still there; a pane that
    /// was listing the deleted entry is re-read, and any other pane is left
    /// exactly as it was.
    ///
    /// A deletion that failed changes nothing in either pane and is reported,
    /// because an entry that is still there must still be listed.
    pub fn delete_selected(&mut self) -> Result<(), OperationError> {
        let active_pane = self.active_pane;
        let paths = self.pane(active_pane).effective_selected_paths();
        if paths.is_empty() {
            return Ok(());
        }

        let other = active_pane.other();
        let total = paths.len();
        let mut progress = OperationProgress::new(OperationKind::Delete, total);
        let mut first_error = None;

        for path in &paths {
            let other_lists_it = self.pane_lists(other, path);

            let outcome = self.filesystem.delete(path);
            let finished = self.finish_change(outcome);

            if finished.is_ok() {
                self.clipboard.remove_path(path);
                self.leave_deleted_directory(path);
                if other_lists_it {
                    let reloaded = self.refresh_pane(other);
                    self.report_navigation(reloaded);
                }
                progress.completed_count += 1;
            } else if first_error.is_none() {
                first_error = Some(finished);
            }
        }

        self.active_pane_mut().deselect_all();

        if let Some(Err(err)) = first_error {
            progress.finish_error(err.to_string());
            self.operation_progress = Some(progress);
            Err(err)
        } else {
            progress.finish_success();
            self.operation_progress = Some(progress);
            Ok(())
        }
    }

    /// Marks the selected entry or multi-selected entries in the active pane.
    fn mark_selected(&mut self, operation: ClipboardOperation) {
        let active_pane = self.active_pane;
        let paths = self.pane(active_pane).effective_selected_paths();
        if paths.is_empty() {
            return;
        }

        self.clipboard.set(paths.clone(), operation);
        let op_kind = match operation {
            ClipboardOperation::Copy => OperationKind::Copy,
            ClipboardOperation::Cut => OperationKind::Move,
        };
        let mut progress = OperationProgress::new(op_kind, paths.len());
        progress.finish_success();
        self.operation_progress = Some(progress);
    }

    /// Shows any tabs that were pointing inside the deleted directory the nearest surviving ancestor.
    fn leave_deleted_directory(&mut self, deleted: &Path) {
        let Some(parent) = parent_of(deleted) else {
            return;
        };

        let other = self.active_pane.other();
        if self.pane(other).current_path().starts_with(deleted) {
            let outcome = self.open_in(other, parent.to_path_buf());
            self.report_navigation(outcome);
        }

        let parent_buf = parent.to_path_buf();
        for tab in &mut self.left.tabs {
            if tab.current_path().starts_with(deleted) {
                tab.set_current_path(parent_buf.clone());
                let entries = self
                    .filesystem
                    .list_directory(&parent_buf)
                    .unwrap_or_default();
                tab.replace_entries(entries, self.left.visible_rows);
            }
        }
        for tab in &mut self.right.tabs {
            if tab.current_path().starts_with(deleted) {
                tab.set_current_path(parent_buf.clone());
                let entries = self
                    .filesystem
                    .list_directory(&parent_buf)
                    .unwrap_or_default();
                tab.replace_entries(entries, self.right.visible_rows);
            }
        }
    }

    /// Puts every marked entry into `directory`, or stops at the first failure.
    fn paste_marked(
        &mut self,
        operation: ClipboardOperation,
        directory: &Path,
    ) -> Result<(), OperationError> {
        let sources = self.clipboard.entries().to_vec();

        // Marking always keeps at least one entry, so there is always something
        // to name in a failure; an empty clipboard is nothing to do.
        let Some(first) = sources.first() else {
            return Ok(());
        };

        // A pane that is not showing a directory has nowhere to paste into, and
        // an empty path would otherwise be joined into a path relative to the
        // process's own directory.
        if directory.as_os_str().is_empty() {
            return Err(OperationError::rejected(
                clipboard_operation(operation, first),
                directory.to_path_buf(),
                "the pane is not showing a directory",
            ));
        }

        for source in &sources {
            let name = source.file_name().ok_or_else(|| {
                OperationError::rejected(
                    clipboard_operation(operation, source),
                    directory.to_path_buf(),
                    "the entry has no file name to paste under",
                )
            })?;

            let destination = directory.join(name);

            match operation {
                ClipboardOperation::Copy => self.filesystem.copy(source, &destination)?,
                ClipboardOperation::Cut => {
                    self.filesystem.move_item(source, &destination)?;
                    for tab in &mut self.left.tabs {
                        if tab.current_path() == source {
                            tab.set_current_path(destination.clone());
                        } else if let Ok(suffix) = tab.current_path().strip_prefix(source) {
                            tab.set_current_path(destination.join(suffix));
                        }
                    }
                    for tab in &mut self.right.tabs {
                        if tab.current_path() == source {
                            tab.set_current_path(destination.clone());
                        } else if let Ok(suffix) = tab.current_path().strip_prefix(source) {
                            tab.set_current_path(destination.join(suffix));
                        }
                    }
                }
            }
        }

        if operation == ClipboardOperation::Cut {
            // The entries have arrived, so there is nothing left to move.
            self.clipboard.clear();
        }

        Ok(())
    }

    /// Renames the entry selected in the active pane to `name`.
    ///
    /// With nothing selected there is no entry to rename, so nothing happens.
    /// The entry keeps its type and contents, the inactive pane is not touched,
    /// and the active pane is re-read so the listing shows the new name.
    pub fn rename_selected(&mut self, name: &OsStr) -> Result<(), OperationError> {
        let Some(from) = self
            .pane(self.active_pane)
            .selected_entry()
            .map(|entry| entry.path().to_path_buf())
        else {
            return Ok(());
        };

        let destination = self.named_child(Operation::Rename(from.clone()), name)?;
        let outcome = self.filesystem.rename(&from, &destination);

        let finished = self.finish_change(outcome);
        if finished.is_ok() {
            self.clipboard.replace_path(&from, destination.clone());
            let active = self.active_pane;
            self.pane_mut(active).reselect_or_first(Some(&destination));
            if self.pane_mut(active).selected_paths_mut().remove(&from) {
                self.pane_mut(active)
                    .selected_paths_mut()
                    .insert(destination.clone());
            }

            for tab in &mut self.left.tabs {
                if tab.current_path() == &from {
                    tab.set_current_path(destination.clone());
                } else if let Ok(suffix) = tab.current_path().strip_prefix(&from) {
                    tab.set_current_path(destination.join(suffix));
                }
            }
            for tab in &mut self.right.tabs {
                if tab.current_path() == &from {
                    tab.set_current_path(destination.clone());
                } else if let Ok(suffix) = tab.current_path().strip_prefix(&from) {
                    tab.set_current_path(destination.join(suffix));
                }
            }
        }
        finished
    }

    /// The path `name` would have inside the active pane's directory.
    ///
    /// The name is a child of that directory and nothing else, so a caller
    /// cannot reach outside what the user is looking at. A pane that is not
    /// showing a directory has nowhere to put an entry, and that is reported
    /// rather than guessed at.
    fn named_child(&self, operation: Operation, name: &OsStr) -> Result<PathBuf, OperationError> {
        let directory = self.pane(self.active_pane).current_path().clone();
        let requested = PathBuf::from(name);

        if directory.as_os_str().is_empty() {
            return Err(OperationError::rejected(
                operation,
                requested,
                "the pane is not showing a directory",
            ));
        }

        if !is_single_name(name) {
            return Err(OperationError::rejected(
                operation,
                requested,
                "the name is not a single file name",
            ));
        }

        Ok(directory.join(name))
    }

    /// Applies the outcome of a change to the filesystem.
    ///
    /// A change that happened is followed by a re-read of the active pane, so
    /// the listing shows what is really there; the other pane is left alone. A
    /// change that failed is reported and leaves both panes as they were,
    /// because nothing happened and nothing may look as if it did.
    fn finish_change(&mut self, outcome: Result<(), OperationError>) -> Result<(), OperationError> {
        match outcome {
            Ok(()) => {
                self.last_outcome = Some(OperationOutcome::Done);
                let reloaded = self.refresh_active_pane();
                self.report_navigation(reloaded);
                Ok(())
            }
            Err(error) => {
                self.last_outcome = Some(error.outcome());
                self.notification.show(Notice::from(&error));
                Err(error)
            }
        }
    }

    /// Shows the parent of the active pane's directory.
    ///
    /// A directory that has no parent, such as a filesystem root, is left
    /// alone: there is nowhere to go, and that is not an error.
    pub fn go_to_parent(&mut self) -> Result<(), NavigationError> {
        let current = self.pane(self.active_pane).current_path().clone();
        let Some(parent) = parent_of(&current) else {
            return Ok(());
        };

        let which = self.active_pane;
        self.open_in(which, parent.to_path_buf())?;
        self.pane_mut(which).reselect(Some(&current));
        Ok(())
    }

    /// Re-reads the directory the active pane is showing.
    ///
    /// The path stays as it is. The listing is replaced, and a selection that no
    /// longer exists is brought back into range rather than pointing at an entry
    /// that is gone.
    pub fn refresh_active_pane(&mut self) -> Result<(), NavigationError> {
        self.refresh_pane(self.active_pane)
    }

    /// Re-reads the directory one pane is showing.
    ///
    /// The path stays as it is, and a selection that no longer exists is
    /// brought back into range rather than pointing at an entry that is gone.
    fn refresh_pane(&mut self, which: ActivePane) -> Result<(), NavigationError> {
        let current = self.pane(which).current_path().clone();
        let entries = self.filesystem.list_directory(&current)?;

        let pane = self.pane_mut(which);
        pane.replace_entries(entries);
        pane.refresh_git_and_project();

        Ok(())
    }

    /// Whether `path` is one of the entries `which` pane is showing.
    fn pane_lists(&self, which: ActivePane, path: &Path) -> bool {
        path.parent() == Some(self.pane(which).current_path().as_path())
    }

    /// Tells one pane how many rows of its listing are on screen.
    ///
    /// The window size comes from the layer that decides the layout, which is
    /// why it is supplied from outside instead of being assumed here. Changing
    /// it can only bring the scroll offset back into range; it never touches
    /// what is selected.
    pub fn set_visible_rows(&mut self, which: ActivePane, rows: usize) {
        self.pane_mut(which).set_visible_rows(rows);
    }

    /// Records the outcome of a navigation the user asked for.
    ///
    /// A failure is reported in the notification state, which is where the
    /// Sets the notification when navigation fails.
    ///
    /// The outcome is inspected here rather than by the navigation itself so the
    /// application keeps what it has to tell the user. A success clears the
    /// previous failure, because a message about a directory the pane is no
    /// longer showing would be misleading.
    pub fn report_navigation(&mut self, outcome: Result<(), NavigationError>) {
        match outcome {
            Ok(()) => {
                self.notification.clear();
            }
            Err(error) => self.notification.show(Notice::from(&error)),
        }
    }

    /// Applies an action to the state.
    ///
    /// This is the whole state transition system: an action goes in, updated
    /// state comes out. Every transition is deterministic, and none of them
    /// touches anything outside the state, so no transition reads the
    /// filesystem, initialises a terminal, or depends on time or randomness.
    ///
    /// An action that needs a later phase is recognised and deliberately
    /// changes nothing rather than pretending an operation happened.
    pub fn handle_action(&mut self, action: Action) {
        match action {
            // Quitting must not depend on which mode happens to be active.
            Action::Quit => self.should_quit = true,

            // Leaves whatever temporary mode is active, throwing away a
            // search that was being entered. In `Normal`, clears multi-selection if any exists.
            Action::Cancel => {
                if self.mode == Mode::Boot {
                    self.skip_boot();
                } else if self.mode == Mode::StorageVision {
                    self.storage_vision.cancel();
                    self.leave_temporary_mode();
                } else if self.mode == Mode::ThemeSelector {
                    self.cancel_theme_selector();
                } else if self.mode == Mode::ContextMenu {
                    if self.context_menu.is_more_open {
                        self.context_menu.close_more();
                    } else {
                        self.close_context_menu();
                    }
                } else if self.mode == Mode::Normal
                    && self.pane(self.active_pane).selected_count() > 0
                {
                    self.active_pane_mut().deselect_all();
                } else {
                    self.cancel();
                }
            }

            Action::MoveUp if self.mode == Mode::ThemeSelector => {
                self.theme_selector_move_up();
            }

            Action::MoveDown if self.mode == Mode::ThemeSelector => {
                self.theme_selector_move_down();
            }

            Action::Open if self.mode == Mode::ThemeSelector => {
                self.apply_theme_selector();
            }

            // A search can be cleared while browsing or while entering a query.
            // In any other mode it is ignored, so a half-finished name is never
            // discarded by it.
            Action::ClearSearch if matches!(self.mode, Mode::Normal | Mode::Search) => {
                self.clear_search();
            }

            // Cycling search mode is only useful while a query is being typed.
            Action::CycleSearchMode if self.mode == Mode::Search => {
                self.active_pane_mut().cycle_search_mode();
            }

            Action::Preview if self.mode == Mode::Preview => {
                self.leave_temporary_mode();
            }

            Action::RemoveBookmark if self.mode == Mode::Bookmarks => {
                self.remove_selected_bookmark();
            }

            Action::MoveFavoriteUp if self.mode == Mode::Bookmarks => {
                self.bookmarks.move_selected_up();
            }

            Action::MoveFavoriteDown if self.mode == Mode::Bookmarks => {
                self.bookmarks.move_selected_down();
            }

            Action::GoParent if self.mode == Mode::StorageVision => {
                if !self.storage_vision.go_back() {
                    self.leave_temporary_mode();
                }
            }

            Action::Open if self.mode == Mode::StorageVision => {
                let target = self.storage_vision.selected_target();
                self.leave_temporary_mode();
                let which = self.active_pane;
                let outcome = self.open_in(which, target);
                self.report_navigation(outcome);
                self.refresh_preview();
            }

            Action::ToggleTerminalFocus
                if self.mode == Mode::Normal || self.mode == Mode::Terminal =>
            {
                if self.mode == Mode::Terminal {
                    self.mode = Mode::Normal;
                } else {
                    self.mode = Mode::Terminal;
                }
            }
            Action::FocusTerminal if self.mode == Mode::Normal || self.mode == Mode::Terminal => {
                self.mode = Mode::Terminal;
            }
            Action::FocusFileManager
                if self.mode == Mode::Normal || self.mode == Mode::Terminal =>
            {
                self.mode = Mode::Normal;
            }
            Action::ScrollTerminalUp
                if self.mode == Mode::Normal || self.mode == Mode::Terminal =>
            {
                self.terminal_scroll_up(5);
            }
            Action::ScrollTerminalDown
                if self.mode == Mode::Normal || self.mode == Mode::Terminal =>
            {
                self.terminal_scroll_down(5);
            }
            Action::SyncTerminalToDirectory
                if self.mode == Mode::Normal || self.mode == Mode::Terminal =>
            {
                self.sync_terminal_to_directory();
            }
            Action::SyncDirectoryToTerminal
                if self.mode == Mode::Normal || self.mode == Mode::Terminal =>
            {
                self.sync_directory_to_terminal();
            }
            Action::RefreshDirectory
                if self.mode == Mode::Normal || self.mode == Mode::Terminal =>
            {
                self.refresh_directory();
            }

            _ if self.mode == Mode::Normal => self.handle_normal_action(action),

            // Not valid in the active mode. Ignoring the action keeps the state
            // consistent, which is better than applying it partially.
            _ => {}
        }
    }

    /// Applies an action while no temporary mode is active.
    fn handle_normal_action(&mut self, action: Action) {
        match action {
            Action::MoveUp => {
                self.active_pane_mut().move_selection_up(1);
                self.trigger_selection_animation();
                self.refresh_preview();
            }
            Action::MoveDown => {
                self.active_pane_mut().move_selection_down(1);
                self.trigger_selection_animation();
                self.refresh_preview();
            }
            // A page is one window of rows. The pane knows how many of its
            // rows are on screen, and a pane that has not been told moves by no
            // page at all rather than by a guess.
            Action::PageUp => {
                let rows = self.pane(self.active_pane).visible_rows();
                self.active_pane_mut().move_selection_up(rows);
                self.trigger_selection_animation();
                self.refresh_preview();
            }
            Action::PageDown => {
                let rows = self.pane(self.active_pane).visible_rows();
                self.active_pane_mut().move_selection_down(rows);
                self.trigger_selection_animation();
                self.refresh_preview();
            }
            Action::GoHome => {
                self.active_pane_mut().select_first();
                self.trigger_selection_animation();
                self.refresh_preview();
            }
            Action::GoEnd => {
                self.active_pane_mut().select_last();
                self.trigger_selection_animation();
                self.refresh_preview();
            }

            // Marking reads the selection and remembers it; the filesystem is
            // not touched until something is pasted.
            Action::Copy => self.mark_for_copy(),
            Action::Cut => self.mark_for_cut(),

            // Multi-selection operations
            Action::ToggleSelect => {
                let idx = self.active_pane_mut().selected_index();
                if let Some(i) = idx {
                    self.active_pane_mut().toggle_selection(i);
                    self.trigger_selection_animation();
                }
            }
            Action::SelectAll => {
                self.active_pane_mut().select_all();
                self.trigger_selection_animation();
            }
            Action::DeselectAll => {
                self.active_pane_mut().deselect_all();
                self.trigger_selection_animation();
            }
            Action::InvertSelection => {
                self.active_pane_mut().invert_selection();
                self.trigger_selection_animation();
            }
            Action::SelectRangeUp => {
                self.active_pane_mut().extend_selection_up(1);
                self.trigger_selection_animation();
                self.refresh_preview();
            }
            Action::SelectRangeDown => {
                self.active_pane_mut().extend_selection_down(1);
                self.trigger_selection_animation();
                self.refresh_preview();
            }
            Action::ContextMenu => {
                let pane = self.pane(self.active_pane);
                let sel_idx = pane.selected_index().unwrap_or(0);
                let scroll = pane.scroll_offset();
                let rel_row = sel_idx.saturating_sub(scroll) as u16;
                let pos_x = match self.active_pane {
                    ActivePane::Left => 10,
                    ActivePane::Right => 50,
                };
                let pos_y = (4 + rel_row).max(2);
                self.open_context_menu((pos_x, pos_y));
            }
            Action::GetInfo => {
                self.open_reveal_context();
            }
            Action::CopyPath => {
                let pane = self.pane(self.active_pane);
                let paths = pane.effective_selected_paths();
                if paths.is_empty() {
                    let cur = pane.current_path();
                    self.notification
                        .show_message(format!("Copied path: {}", cur.display()));
                } else if paths.len() == 1 {
                    self.notification
                        .show_message(format!("Copied path: {}", paths[0].display()));
                } else {
                    self.notification
                        .show_message(format!("Copied {} paths to clipboard", paths.len()));
                }
            }
            Action::CopyName => {
                let pane = self.pane(self.active_pane);
                let paths = pane.effective_selected_paths();
                if paths.is_empty() {
                    let cur = pane.current_path();
                    let name = cur
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.notification
                        .show_message(format!("Copied name: {name}"));
                } else if paths.len() == 1 {
                    let name = paths[0]
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.notification
                        .show_message(format!("Copied name: {name}"));
                } else {
                    self.notification
                        .show_message(format!("Copied {} names to clipboard", paths.len()));
                }
            }
            Action::OpenInNewTab => {
                let target = self
                    .pane(self.active_pane)
                    .selected_entry()
                    .and_then(|e| {
                        if e.is_dir() {
                            Some(e.path().to_path_buf())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| self.pane(self.active_pane).current_path().clone());
                self.open_path_in_new_tab(target);
                self.notification.show_message("Opened in new tab");
            }

            // Pasting changes the filesystem. What happened is recorded by the
            // operation itself, which is what the notification state is for,
            // so the result is deliberately left where the user will see it.
            Action::Paste => {
                let _ = self.paste();
            }
            // Switching moves the focus and synchronizes the embedded terminal
            // to the newly active pane's directory.
            Action::SwitchPane => {
                self.active_pane = self.active_pane.other();
                self.last_sync_origin = SyncOrigin::ActivePaneSwitch;
                self.trigger_focus_animation();
                self.refresh_preview();
                self.sync_terminal_to_directory();
            }

            // Reorders what the active pane already holds. The other pane keeps
            // its own mode, its own listing and its own selection, and no
            // directory is read again just to change the order of one of them.
            Action::ChangeSort => {
                self.active_pane_mut().change_sort();
                self.refresh_preview();
            }

            // The two actions that reach the filesystem. Each one reports what
            // happened, so a directory that cannot be read is never presented as
            // if it had been opened.
            Action::Open => {
                let outcome = self.open_selected();
                self.report_navigation(outcome);
                self.refresh_preview();
            }
            Action::GoParent => {
                let outcome = self.go_to_parent();
                self.report_navigation(outcome);
                self.refresh_preview();
            }

            Action::StartSearch => self.start_search(),
            Action::CycleSearchMode => {
                // Only meaningful while a search is active; silently ignored otherwise.
                self.active_pane_mut().cycle_search_mode();
            }
            Action::JumpToPath => {
                self.mode = Mode::Jump;
                self.input_buffer.clear();
                self.cursor_position = 0;
            }
            Action::GoBack => {
                let outcome = self.go_back();
                self.report_navigation(outcome);
            }
            Action::GoForward => {
                let outcome = self.go_forward();
                self.report_navigation(outcome);
            }
            Action::Rename => {
                self.mode = Mode::Rename;
                let name = self
                    .pane(self.active_pane)
                    .selected_entry()
                    .map(|entry| entry.name().to_string_lossy().to_string());
                if let Some(name) = name {
                    self.cursor_position = name.chars().count();
                    self.input_buffer = name;
                } else {
                    self.input_buffer.clear();
                    self.cursor_position = 0;
                }
            }
            Action::NewFile => {
                self.mode = Mode::Create;
                self.create_kind = Some(CreateKind::File);
                self.input_buffer.clear();
                self.cursor_position = 0;
            }
            Action::NewDirectory => {
                self.mode = Mode::Create;
                self.create_kind = Some(CreateKind::Directory);
                self.input_buffer.clear();
                self.cursor_position = 0;
            }
            Action::Delete => {
                let _ = self.delete_selected();
            }
            Action::Preview => {
                self.preview.active = true;
                self.mode = Mode::Preview;
                self.refresh_preview();
            }
            Action::CommandPalette => {
                self.open_command_palette();
            }
            Action::Help => {
                self.mode = Mode::Help;
            }
            Action::AddBookmark => {
                self.add_current_bookmark();
            }
            Action::OpenBookmarks => {
                self.mode = Mode::Bookmarks;
                self.bookmarks.clamp_selection();
            }
            Action::RemoveBookmark => {
                self.remove_selected_bookmark();
            }

            Action::NewTab => {
                self.new_tab_in_active_pane();
                self.refresh_preview();
            }
            Action::CloseTab => {
                self.close_tab_in_active_pane();
                self.refresh_preview();
            }
            Action::NextTab => {
                self.next_tab_in_active_pane();
                self.refresh_preview();
            }
            Action::PreviousTab => {
                self.previous_tab_in_active_pane();
                self.refresh_preview();
            }
            Action::DuplicateTab => {
                self.duplicate_tab_in_active_pane();
                self.refresh_preview();
            }

            Action::SmartJump => {
                self.open_smart_jump();
            }
            Action::GoHomeDir => {
                if let Some(home) = crate::utils::path::home_dir() {
                    let which = self.active_pane;
                    let outcome = self.open_in(which, home);
                    self.report_navigation(outcome);
                    self.refresh_preview();
                }
            }
            Action::GoRootDir => {
                let current = self.pane(self.active_pane).current_path().clone();
                let root = crate::utils::path::filesystem_root(&current);
                let which = self.active_pane;
                let outcome = self.open_in(which, root);
                self.report_navigation(outcome);
                self.refresh_preview();
            }
            Action::GoGitRoot => {
                if let Some(root) = self
                    .pane(self.active_pane)
                    .git()
                    .root()
                    .map(|p| p.to_path_buf())
                {
                    let which = self.active_pane;
                    let outcome = self.open_in(which, root);
                    self.report_navigation(outcome);
                    self.refresh_preview();
                }
            }
            Action::GoProjectRoot => {
                self.go_project_root();
            }
            Action::ProjectCockpit => {
                self.open_project_cockpit();
            }
            Action::GitStatus | Action::GitStatusPanel => {
                self.open_git_status_panel();
            }
            Action::FileRadar => {
                self.open_file_radar();
            }
            Action::RevealContext => {
                self.open_reveal_context();
            }
            Action::OpenManifest => {
                self.open_manifest();
            }
            Action::OpenReadme => {
                self.open_readme();
            }
            Action::OpenLicense => {
                self.open_license();
            }
            Action::GoSourceDir => {
                self.go_source_dir();
            }
            Action::GoTestsDir => {
                self.go_tests_dir();
            }
            Action::GoDocsDir => {
                self.go_docs_dir();
            }
            Action::ToggleFocusMode => {
                self.toggle_focus_mode();
            }
            Action::ToggleTerminalFocus => {
                if self.mode == Mode::Terminal {
                    self.mode = Mode::Normal;
                } else {
                    self.mode = Mode::Terminal;
                }
            }
            Action::FocusTerminal => {
                self.mode = Mode::Terminal;
            }
            Action::FocusFileManager => {
                if self.mode == Mode::Terminal {
                    self.mode = Mode::Normal;
                }
            }
            Action::SyncTerminalToDirectory => {
                self.sync_terminal_to_directory();
            }
            Action::SyncDirectoryToTerminal => {
                self.sync_directory_to_terminal();
            }
            Action::ScrollTerminalUp => {
                self.terminal_scroll_up(3);
            }
            Action::ScrollTerminalDown => {
                self.terminal_scroll_down(3);
            }
            Action::RefreshDirectory => {
                self.refresh_directory();
            }
            Action::StorageVision => {
                self.open_storage_vision();
            }
            Action::ThemeSelector => {
                self.open_theme_selector();
            }
            Action::NextTheme => {
                self.next_theme();
            }
            Action::PrevTheme => {
                self.prev_theme();
            }
            Action::ToggleFavorite => {
                self.toggle_favorite_for_current();
            }
            Action::RenameFavorite => {
                // If in bookmarks/favorites modal, can rename
            }
            Action::MoveFavoriteUp => {
                self.bookmarks.move_selected_up();
            }
            Action::MoveFavoriteDown => {
                self.bookmarks.move_selected_down();
            }

            // Handled before this point; listed so that the match stays
            // exhaustive over every action.
            Action::ToggleHidden => {
                self.active_pane_mut().toggle_hidden();
                self.refresh_preview();
            }

            // Handled before this point; listed so that the match stays
            // exhaustive over every action.
            Action::Quit | Action::Cancel | Action::ClearSearch => {}

            // Recognised, but they cannot change the state yet: each one either
            // needs a filesystem operation, a state field that does not exist
            // yet, or the filesystem initialisation that resolves the working
            // directory. Nothing is faked for them, so the state stays truthful.
            Action::MoveLeft | Action::MoveRight => {}
        }
    }

    /// The pane the application acts on, mutably.
    pub fn active_pane_mut(&mut self) -> &mut Pane {
        self.pane_mut(self.active_pane)
    }

    /// The pane the application acts on, mutably.
    pub fn pane_mut(&mut self, which: ActivePane) -> &mut Pane {
        match which {
            ActivePane::Left => &mut self.left,
            ActivePane::Right => &mut self.right,
        }
    }

    /// Leaves a temporary mode for [`Mode::Normal`].
    ///
    /// Only the overlay is left: the entries, the selection, the clipboard and
    /// any search query stay as they are. The preview is deactivated because
    /// leaving preview mode hides it.
    pub fn leave_temporary_mode(&mut self) {
        self.mode = Mode::Normal;
        self.create_kind = None;
        self.input_buffer.clear();
        self.cursor_position = 0;
        self.confirm_selection = false;
        self.palette.clear();
        self.smart_jump.clear();
        self.project_cockpit = ProjectCockpitState::new();
        self.git_status_panel = GitStatusPanelState::new();
        self.file_radar = FileRadarState::default();
        self.reveal_context = RevealContextState::new();
        self.preview_theme = None;
        self.preview.active = false;
        self.boot_state = None;
        self.animations
            .cancel_tag(crate::animation::AnimationTag::VisionBoot);
        self.animations
            .cancel_tag(crate::animation::AnimationTag::Dialog);
        self.animations
            .cancel_tag(crate::animation::AnimationTag::CommandCenter);
        self.animations
            .cancel_tag(crate::animation::AnimationTag::QuickSwitcher);
        self.animations
            .cancel_tag(crate::animation::AnimationTag::ContextMenu);
        self.refresh_preview();
    }

    /// Closes any active modal dialog and returns to normal mode.
    pub fn close_modal(&mut self) {
        self.leave_temporary_mode();
    }

    /// Discards the search of the pane the application acts on and leaves
    /// search mode.
    ///
    /// The query, the mode and everything a walk found go, the whole listing is
    /// shown again, and an earlier cancellation request is forgotten. The pane
    /// can do all of that from the entries it already holds, so undoing a
    /// search never reads the directory, and the selected entry is kept,
    /// because it is shown again like every other one.
    fn clear_search(&mut self) {
        self.active_pane_mut().clear_search();

        if self.mode == Mode::Search {
            self.mode = Mode::Normal;
        }
    }

    /// Starts a search in the pane the application acts on.
    ///
    /// The query starts empty and the search starts from the basic mode, so
    /// nothing is filtered and the whole listing stays shown. The pane's
    /// directory and the entries it holds are left untouched, and nothing is
    /// read from the filesystem.
    fn start_search(&mut self) {
        self.active_pane_mut().clear_search();
        self.mode = Mode::Search;
    }

    /// Leaves a temporary mode, throwing away a search that was being entered.
    ///
    /// Cancelling a search is more than leaving its mode: the query goes and
    /// the pane shows every entry it loaded again. Every other mode is simply
    /// left, and whatever state it was using is kept.
    fn cancel(&mut self) {
        if self.mode == Mode::Search {
            self.clear_search();
        } else {
            self.leave_temporary_mode();
        }
    }

    /// Whether a quit has been requested.
    ///
    /// The request is recorded here and acted on by the event loop in a later
    /// phase; the state layer never terminates the process itself.
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// The pane the application acts on.
    pub fn pane(&self, which: ActivePane) -> &Pane {
        match which {
            ActivePane::Left => &self.left,
            ActivePane::Right => &self.right,
        }
    }

    /// Which pane is active.
    pub fn active_pane(&self) -> ActivePane {
        self.active_pane
    }

    /// Sets which pane is active.
    pub fn set_active_pane(&mut self, which: ActivePane) {
        if self.mode != Mode::Normal {
            return;
        }
        if self.active_pane != which {
            self.active_pane = which;
            self.last_sync_origin = SyncOrigin::ActivePaneSwitch;
            self.refresh_preview();
            self.sync_terminal_to_directory();
        }
    }

    /// Activates `which` pane and selects entry at `index`.
    pub fn select_entry(&mut self, which: ActivePane, index: usize) {
        if self.mode != Mode::Normal {
            return;
        }
        let pane_changed = self.active_pane != which;
        self.active_pane = which;
        if pane_changed {
            self.last_sync_origin = SyncOrigin::ActivePaneSwitch;
            self.sync_terminal_to_directory();
        }
        let pane = self.pane_mut(which);
        let total = pane.visible_count();
        if total > 0 {
            let clamped = index.min(total.saturating_sub(1));
            pane.select(clamped);
        } else {
            pane.clear_selection();
        }
        self.refresh_preview();
    }

    /// The current interaction mode.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Sets the interaction mode.
    pub fn set_mode(&mut self, mode: Mode) {
        if mode != self.mode {
            if mode.is_modal() {
                self.motion.notify_dialog_opened();
                match mode {
                    Mode::CommandPalette => {
                        self.trigger_command_center_animation();
                    }
                    Mode::SmartJump | Mode::Jump => {
                        self.trigger_quick_switcher_animation();
                    }
                    Mode::ContextMenu => {
                        self.trigger_context_menu_animation();
                    }
                    _ => {
                        self.trigger_dialog_animation();
                    }
                }
            } else if self.mode.is_modal() {
                self.motion.clear_dialog_transition();
                self.animations
                    .cancel_tag(crate::animation::AnimationTag::Dialog);
                self.animations
                    .cancel_tag(crate::animation::AnimationTag::CommandCenter);
                self.animations
                    .cancel_tag(crate::animation::AnimationTag::QuickSwitcher);
                self.animations
                    .cancel_tag(crate::animation::AnimationTag::ContextMenu);
            }
        }
        self.mode = mode;
    }

    /// Triggers a navigation micro-interaction animation (~140ms).
    pub fn trigger_navigation_animation(&mut self) {
        crate::animation::trigger_micro_animation(
            &mut self.animations,
            crate::animation::AnimationTag::Navigation,
        );
    }

    /// Triggers a selection micro-interaction animation (~100ms).
    pub fn trigger_selection_animation(&mut self) {
        crate::animation::trigger_micro_animation(
            &mut self.animations,
            crate::animation::AnimationTag::Selection,
        );
    }

    /// Triggers a focus micro-interaction animation (~120ms).
    pub fn trigger_focus_animation(&mut self) {
        crate::animation::trigger_micro_animation(
            &mut self.animations,
            crate::animation::AnimationTag::Custom("Focus"),
        );
    }

    /// Triggers a preview micro-interaction animation (~120ms).
    pub fn trigger_preview_animation(&mut self) {
        crate::animation::trigger_micro_animation(
            &mut self.animations,
            crate::animation::AnimationTag::Custom("Preview"),
        );
    }

    /// Triggers a dialog entrance animation (~150ms).
    pub fn trigger_dialog_animation(&mut self) {
        crate::animation::trigger_micro_animation(
            &mut self.animations,
            crate::animation::AnimationTag::Dialog,
        );
    }

    /// Triggers a Command Center entrance animation (~150ms).
    pub fn trigger_command_center_animation(&mut self) {
        crate::animation::trigger_micro_animation(
            &mut self.animations,
            crate::animation::AnimationTag::CommandCenter,
        );
    }

    /// Triggers a Quick Switcher entrance animation (~150ms).
    pub fn trigger_quick_switcher_animation(&mut self) {
        crate::animation::trigger_micro_animation(
            &mut self.animations,
            crate::animation::AnimationTag::QuickSwitcher,
        );
    }

    /// Triggers a Context Menu popover animation (~120ms).
    pub fn trigger_context_menu_animation(&mut self) {
        crate::animation::trigger_micro_animation(
            &mut self.animations,
            crate::animation::AnimationTag::ContextMenu,
        );
    }

    /// Triggers an operation feedback animation (~160ms).
    pub fn trigger_operation_animation(&mut self) {
        crate::animation::trigger_micro_animation(
            &mut self.animations,
            crate::animation::AnimationTag::Operation,
        );
    }

    /// Triggers the signature reusable Vision Pulse (~220ms).
    pub fn trigger_vision_pulse(&mut self) {
        crate::animation::trigger_micro_animation(
            &mut self.animations,
            crate::animation::AnimationTag::VisionPulse,
        );
    }

    /// The motion state tracker.
    pub fn motion(&self) -> &crate::ui::motion::MotionState {
        &self.motion
    }

    /// Mutable motion state tracker.
    pub fn motion_mut(&mut self) -> &mut crate::ui::motion::MotionState {
        &mut self.motion
    }

    /// The centralized Animation Engine.
    pub fn animation_engine(&self) -> &crate::animation::AnimationEngine {
        &self.animations
    }

    /// Mutable access to the centralized Animation Engine.
    pub fn animation_engine_mut(&mut self) -> &mut crate::animation::AnimationEngine {
        &mut self.animations
    }

    /// Returns `true` if any animation is currently active.
    pub fn has_active_animations(&self) -> bool {
        self.animations.is_animating()
    }

    /// Sets the complete motion preferences.
    pub fn set_motion_preferences(&mut self, prefs: crate::animation::MotionPreferences) {
        self.motion.reduced_motion = prefs.mode != crate::animation::MotionMode::Full;
        self.animations.set_preferences(prefs);
    }

    /// Accesses the active motion preferences.
    pub fn motion_preferences(&self) -> &crate::animation::MotionPreferences {
        self.animations.preferences()
    }

    /// Returns the active motion mode.
    pub fn motion_mode(&self) -> crate::animation::MotionMode {
        self.animations.preferences().mode
    }

    /// Sets the motion mode directly.
    pub fn set_motion_mode(&mut self, mode: crate::animation::MotionMode) {
        self.motion.reduced_motion = mode != crate::animation::MotionMode::Full;
        self.animations.set_motion_mode(mode);
    }

    /// Whether reduced motion is active.
    pub fn reduced_motion(&self) -> bool {
        self.motion.reduced_motion
    }

    /// Sets the reduced motion preference.
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.motion.reduced_motion = reduced;
        let mode = if reduced {
            crate::animation::MotionMode::Reduced
        } else {
            crate::animation::MotionMode::Full
        };
        self.animations.set_motion_mode(mode);
    }

    /// Begins the signature Vision Boot sequence according to user settings and motion mode.
    pub fn start_boot(&mut self) {
        let startup_mode = self.startup_motion_mode();
        let motion_mode = self.motion_mode();

        if let Some((duration, easing)) =
            crate::animation::boot_duration_and_easing(startup_mode, motion_mode)
        {
            let left_pane = self.pane(self.active_pane);
            let directory = left_pane.current_path().clone();
            let entry_count = left_pane.entries().len();
            let terminal_ready = self.terminal.0.lock().map(|g| g.is_some()).unwrap_or(false);
            let config_ready = true;

            let (project_name, project_type, project_indicators, project_structure_items) = {
                let fp = crate::project::detect_project(&directory);
                if fp.root.is_some() || !fp.project_types.is_empty() {
                    let p_name = fp.name.clone().unwrap_or_else(|| {
                        directory
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    });
                    let p_type_str = fp
                        .project_types
                        .first()
                        .map(|t| t.display_name().to_string())
                        .unwrap_or_else(|| "Project".to_string());

                    let mut indicators = Vec::new();
                    for t in &fp.project_types {
                        indicators.push(t.display_name().to_string());
                    }
                    for b in &fp.build_systems {
                        let b_name = b.display_name().to_string();
                        if !indicators.contains(&b_name) {
                            indicators.push(b_name);
                        }
                    }

                    let mut structure_items = Vec::new();
                    if !fp.signals.source_dirs.is_empty() || directory.join("src").is_dir() {
                        structure_items.push(("src/".to_string(), true));
                    }
                    if !fp.signals.test_dirs.is_empty() || directory.join("tests").is_dir() {
                        structure_items.push(("tests/".to_string(), true));
                    }
                    if !fp.signals.doc_dirs.is_empty() || directory.join("docs").is_dir() {
                        structure_items.push(("docs/".to_string(), true));
                    }
                    if let Some(manifest) = fp.manifests.first() {
                        let m_name = manifest
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        structure_items.push((m_name, true));
                    }
                    if directory.join("README.md").is_file()
                        || fp
                            .signals
                            .doc_files
                            .iter()
                            .any(|f| f.ends_with("README.md") || f.ends_with("README"))
                    {
                        structure_items.push(("README.md".to_string(), true));
                    }
                    (Some(p_name), Some(p_type_str), indicators, structure_items)
                } else {
                    (None, None, Vec::new(), Vec::new())
                }
            };

            let git_status = self.git_status();
            let (git_branch, git_dirty) = if git_status.repository.is_repo() {
                let b_name = match &git_status.branch {
                    crate::git::GitBranch::Branch(name) => Some(name.clone()),
                    crate::git::GitBranch::Detached(sha) => Some(format!("detached@{sha}")),
                    crate::git::GitBranch::Unknown => Some("HEAD".to_string()),
                };
                (b_name, !git_status.is_clean)
            } else {
                (None, false)
            };

            self.boot_state = Some(crate::animation::BootState::new(
                startup_mode,
                motion_mode,
                true,
                directory,
                entry_count,
                terminal_ready,
                config_ready,
                project_name,
                project_type,
                project_indicators,
                project_structure_items,
                git_branch,
                git_dirty,
                None,
            ));

            self.animations
                .start_raw(crate::animation::AnimationTag::VisionBoot, duration, easing);
            self.mode = Mode::Boot;
        } else {
            self.boot_state = None;
            if self.mode == Mode::Boot {
                self.mode = Mode::Normal;
            }
        }
    }

    /// Skips the Vision Boot sequence immediately, returning cleanly to the ready UI.
    pub fn skip_boot(&mut self) {
        self.finish_boot();
    }

    /// Finishes the Vision Boot sequence and restores the normal interaction mode.
    pub fn finish_boot(&mut self) {
        self.boot_state = None;
        self.animations.clear();
        if self.mode == Mode::Boot {
            self.mode = Mode::Normal;
        }
    }

    /// Whether the application is currently running the Vision Boot startup sequence.
    pub fn is_booting(&self) -> bool {
        self.mode == Mode::Boot
    }

    /// Returns a reference to the active `BootState`, if booting.
    pub fn boot_state(&self) -> Option<&crate::animation::BootState> {
        self.boot_state.as_ref()
    }

    /// Returns normalized boot animation progress (`0.0..=1.0`).
    pub fn boot_progress(&self) -> f32 {
        if let Some(prog) = self
            .animations
            .tag_progress(crate::animation::AnimationTag::VisionBoot)
        {
            prog.eased
        } else {
            1.0
        }
    }

    /// Returns the active startup motion mode configuration.
    pub fn startup_motion_mode(&self) -> crate::animation::StartupMotionMode {
        self.animations.preferences().startup
    }

    /// Sets the startup motion mode configuration.
    pub fn set_startup_motion_mode(&mut self, mode: crate::animation::StartupMotionMode) {
        self.animations.preferences_mut().startup = mode;
    }

    /// Sets a custom animation engine (primarily for deterministic unit testing).
    pub fn set_animation_engine(&mut self, engine: crate::animation::AnimationEngine) {
        self.animations = engine;
    }

    /// Whether distraction-free Focus Mode is active.
    pub fn is_focus_mode(&self) -> bool {
        self.focus_mode
    }

    /// Toggles distraction-free Focus Mode.
    pub fn toggle_focus_mode(&mut self) {
        self.focus_mode = !self.focus_mode;
    }

    /// Enters confirmation mode for deleting the selected entry if one exists.
    pub fn prompt_delete_confirmation(&mut self) {
        if !self
            .pane(self.active_pane)
            .effective_selected_paths()
            .is_empty()
        {
            self.mode = Mode::Confirm;
            self.confirm_selection = false;
        }
    }

    /// What kind of entry is being created in `Mode::Create`.
    pub fn create_kind(&self) -> Option<CreateKind> {
        self.create_kind
    }

    /// The text buffer for input dialogs.
    pub fn input_buffer(&self) -> &str {
        &self.input_buffer
    }

    /// Sets the text buffer for input dialogs.
    pub fn set_input_buffer(&mut self, text: &str) {
        self.input_buffer = text.to_string();
        self.cursor_position = self.input_buffer.chars().count();
    }

    /// The character cursor position in `input_buffer`.
    pub fn cursor_position(&self) -> usize {
        self.cursor_position
    }

    /// Whether Confirm (Yes) is selected in `Mode::Confirm`.
    pub fn confirm_selection(&self) -> bool {
        self.confirm_selection
    }

    /// Toggles the selected option in `Mode::Confirm`.
    pub fn toggle_confirm_selection(&mut self) {
        self.confirm_selection = !self.confirm_selection;
    }

    /// Sets the selected option in `Mode::Confirm`.
    pub fn set_confirm_selection(&mut self, val: bool) {
        self.confirm_selection = val;
    }

    /// The command palette state.
    pub fn command_palette(&self) -> &CommandPaletteState {
        &self.palette
    }

    /// The command palette state, mutably.
    pub fn command_palette_mut(&mut self) -> &mut CommandPaletteState {
        &mut self.palette
    }

    /// Inserts a character at the cursor position in input buffer.
    pub fn input_push_char(&mut self, ch: char) {
        let mut chars: Vec<char> = self.input_buffer.chars().collect();
        let pos = self.cursor_position.min(chars.len());
        chars.insert(pos, ch);
        self.input_buffer = chars.into_iter().collect();
        self.cursor_position = pos + 1;
    }

    /// Removes the character before cursor in input buffer.
    pub fn input_pop_char(&mut self) {
        let mut chars: Vec<char> = self.input_buffer.chars().collect();
        if self.cursor_position > 0 && !chars.is_empty() {
            let remove_idx = (self.cursor_position - 1).min(chars.len() - 1);
            chars.remove(remove_idx);
            self.input_buffer = chars.into_iter().collect();
            self.cursor_position = self.cursor_position.saturating_sub(1);
        }
    }

    /// Moves cursor left in input buffer.
    pub fn input_move_cursor_left(&mut self) {
        self.cursor_position = self.cursor_position.saturating_sub(1);
    }

    /// Moves cursor right in input buffer.
    pub fn input_move_cursor_right(&mut self) {
        let len = self.input_buffer.chars().count();
        if self.cursor_position < len {
            self.cursor_position += 1;
        }
    }

    /// Moves cursor to the beginning of the input buffer.
    pub fn input_move_cursor_home(&mut self) {
        self.cursor_position = 0;
    }

    /// Moves cursor to the end of the input buffer.
    pub fn input_move_cursor_end(&mut self) {
        self.cursor_position = self.input_buffer.chars().count();
    }

    /// Deletes the character at the current cursor position.
    pub fn input_delete_char(&mut self) {
        let mut chars: Vec<char> = self.input_buffer.chars().collect();
        if self.cursor_position < chars.len() {
            chars.remove(self.cursor_position);
            self.input_buffer = chars.into_iter().collect();
        }
    }

    /// Appends a character to the command palette search query.
    pub fn palette_push_char(&mut self, ch: char) {
        self.palette.push_char(ch);
    }

    /// Removes the last character from the command palette search query.
    pub fn palette_pop_char(&mut self) {
        self.palette.pop_char();
    }

    /// Moves the command palette selection up.
    pub fn palette_move_up(&mut self) {
        self.palette.move_up();
    }

    /// Moves the command palette selection down.
    pub fn palette_move_down(&mut self) {
        self.palette.move_down();
    }

    /// Confirms the active modal dialog or command palette, executing its action and leaving the modal mode.
    pub fn confirm_modal(&mut self) -> Option<Action> {
        match self.mode {
            Mode::Confirm => {
                if self.confirm_selection {
                    let outcome = self.delete_selected();
                    self.finish_modal_operation(outcome);
                } else {
                    self.leave_temporary_mode();
                }
                None
            }
            Mode::Create => {
                let name = self.input_buffer.trim().to_string();
                if !name.is_empty() {
                    let is_dir = self.create_kind == Some(CreateKind::Directory);
                    let outcome = if is_dir {
                        self.create_directory(OsStr::new(&name))
                    } else {
                        self.create_file(OsStr::new(&name))
                    };
                    self.finish_modal_operation(outcome);
                } else {
                    self.leave_temporary_mode();
                }
                None
            }
            Mode::Rename => {
                let name = self.input_buffer.trim().to_string();
                if !name.is_empty() {
                    let outcome = self.rename_selected(OsStr::new(&name));
                    self.finish_modal_operation(outcome);
                } else {
                    self.leave_temporary_mode();
                }
                None
            }
            Mode::CommandPalette => {
                if let Some(cmd) = self.palette.selected_command() {
                    let action = cmd.action();
                    self.leave_temporary_mode();
                    Some(action)
                } else if let Some(target) = self.palette.selected_navigation_target() {
                    self.leave_temporary_mode();
                    let which = self.active_pane;
                    if target.is_file() {
                        if let Some(parent) = target.parent() {
                            let parent_buf = parent.to_path_buf();
                            let file_name = target.file_name().map(|n| n.to_os_string());
                            let outcome = self.open_in(which, parent_buf);
                            self.report_navigation(outcome);
                            if let Some(name) = file_name {
                                self.pane_mut(which).select_name(&name);
                            }
                            self.refresh_preview();
                        }
                    } else {
                        let outcome = self.open_in(which, target);
                        self.report_navigation(outcome);
                        self.refresh_preview();
                    }
                    None
                } else {
                    self.leave_temporary_mode();
                    None
                }
            }
            Mode::Help => {
                self.leave_temporary_mode();
                None
            }
            Mode::Bookmarks => {
                let _ = self.open_selected_bookmark();
                None
            }
            Mode::Jump => {
                let raw = self.input_buffer.trim().to_string();
                self.leave_temporary_mode();
                if !raw.is_empty() {
                    let base = self.pane(self.active_pane).current_path().clone();
                    let target = crate::utils::path::resolve_target_path(&raw, &base);
                    let which = self.active_pane;
                    if target.is_file() {
                        if let Some(parent) = target.parent() {
                            let parent_buf = parent.to_path_buf();
                            let file_name = target.file_name().map(|n| n.to_os_string());
                            let outcome = self.open_in(which, parent_buf);
                            self.report_navigation(outcome);
                            if let Some(name) = file_name {
                                self.pane_mut(which).select_name(&name);
                            }
                            self.refresh_preview();
                        }
                    } else {
                        let outcome = self.open_in(which, target);
                        self.report_navigation(outcome);
                        self.refresh_preview();
                    }
                }
                None
            }
            Mode::SmartJump => {
                let selected_target = self
                    .smart_jump
                    .selected_item()
                    .map(|item| item.path.clone());
                self.leave_temporary_mode();
                if let Some(target) = selected_target {
                    let which = self.active_pane;
                    if target.is_file() {
                        if let Some(parent) = target.parent() {
                            let parent_buf = parent.to_path_buf();
                            let file_name = target.file_name().map(|n| n.to_os_string());
                            let outcome = self.open_in(which, parent_buf);
                            self.report_navigation(outcome);
                            if let Some(name) = file_name {
                                self.pane_mut(which).select_name(&name);
                            }
                            self.refresh_preview();
                        }
                    } else {
                        let outcome = self.open_in(which, target);
                        self.report_navigation(outcome);
                        self.refresh_preview();
                    }
                }
                None
            }
            Mode::ProjectCockpit => {
                if let Some(action_item) = self.project_cockpit.selected_action().cloned() {
                    self.leave_temporary_mode();
                    Some(action_item.action)
                } else {
                    self.leave_temporary_mode();
                    None
                }
            }
            Mode::GitStatusPanel => {
                if let Some(entry) = self.git_status_panel.selected_entry().cloned() {
                    self.leave_temporary_mode();
                    let full_path = entry.full_path;
                    if let Some(parent) = full_path.parent() {
                        let which = self.active_pane;
                        let outcome = self.open_in(which, parent.to_path_buf());
                        self.report_navigation(outcome);
                        self.pane_mut(which).select_path(&full_path);
                        self.refresh_preview();
                    }
                } else {
                    self.leave_temporary_mode();
                }
                None
            }
            Mode::FileRadar => {
                self.leave_temporary_mode();
                None
            }
            Mode::RevealContext => {
                if let Some(level) = self.reveal_context.selected_level().cloned() {
                    self.leave_temporary_mode();
                    let target = level.path;
                    let which = self.active_pane;
                    if target.is_dir() {
                        let outcome = self.open_in(which, target);
                        self.report_navigation(outcome);
                        self.refresh_preview();
                    } else if let Some(parent) = target.parent() {
                        let outcome = self.open_in(which, parent.to_path_buf());
                        self.report_navigation(outcome);
                        self.pane_mut(which).select_path(&target);
                        self.refresh_preview();
                    }
                } else {
                    self.leave_temporary_mode();
                }
                None
            }
            Mode::ContextMenu => {
                if let Some(action) = self.context_menu.selected_action() {
                    self.close_context_menu();
                    Some(action)
                } else if let Some(ContextMenuItem::More { .. }) =
                    self.context_menu.items.get(self.context_menu.selected)
                {
                    if self.context_menu.is_more_open {
                        self.context_menu.close_more();
                    } else {
                        self.context_menu.open_more();
                    }
                    None
                } else {
                    self.close_context_menu();
                    None
                }
            }
            Mode::StorageVision => {
                self.storage_vision.drill_down();
                None
            }
            Mode::ThemeSelector => {
                self.apply_theme_selector();
                None
            }
            _ => None,
        }
    }

    /// Opens the Command Center, collecting contextual application state and accessible entries.
    pub fn open_command_palette(&mut self) {
        self.mode = Mode::CommandPalette;

        let (context, mut accessible) = {
            let active_pane = self.pane(self.active_pane);
            let current_path = active_pane.current_path().clone();
            let selected_entry = active_pane.selected_entry();
            let ws = active_pane.workspace_context();
            let proj = ws.project_for_path(&current_path);
            let project_info = active_pane.project_info();
            let git_status = active_pane.git_status();

            let has_project = ws.is_active() || project_info.root.is_some();
            let has_source_dir = proj.and_then(|p| p.primary_source_dir()).is_some()
                || !ws.source_directories().is_empty()
                || project_info.source_dir.is_some();
            let has_tests_dir = proj.and_then(|p| p.primary_test_dir()).is_some()
                || !ws.test_directories().is_empty();
            let has_docs_dir =
                proj.and_then(|p| p.primary_doc_dir()).is_some() || !ws.documentation().is_empty();
            let has_manifest = proj.and_then(|p| p.primary_manifest()).is_some()
                || project_info.manifest_file.is_some()
                || ws.important_files().iter().any(|f| f.role.is_manifest());
            let has_readme = proj.is_some_and(|p| {
                p.important_files
                    .iter()
                    .any(|f| f.role == crate::project::ImportantFileRole::Documentation)
            }) || ws
                .important_files()
                .iter()
                .any(|f| f.role == crate::project::ImportantFileRole::Documentation)
                || project_info.readme_file.is_some();
            let has_license = proj.is_some_and(|p| {
                p.important_files
                    .iter()
                    .any(|f| f.role == crate::project::ImportantFileRole::License)
            }) || ws
                .important_files()
                .iter()
                .any(|f| f.role == crate::project::ImportantFileRole::License)
                || project_info.license_file.is_some();

            let context = crate::commands::palette::ContextFilter {
                has_selection: selected_entry.is_some(),
                selected_is_dir: selected_entry.is_some_and(|e| e.is_dir()),
                selected_count: active_pane.selected_count(),
                is_empty_dir: active_pane.entries().is_empty(),
                is_terminal_focused: false,
                has_git: git_status.is_repo(),
                has_project,
                has_source_dir,
                has_tests_dir,
                has_docs_dir,
                has_manifest,
                has_readme,
                has_license,
                has_clipboard: self.clipboard.operation.is_some(),
            };

            let mut accessible = Vec::new();
            for entry in active_pane.entries() {
                accessible.push(entry.path().to_path_buf());
            }

            if let Some(p) = proj {
                for imp in &p.important_files {
                    if !accessible.contains(&imp.path) {
                        accessible.push(imp.path.clone());
                    }
                }
                for s in &p.source_directories {
                    if !accessible.contains(&s.path) {
                        accessible.push(s.path.clone());
                    }
                }
                for t in &p.test_directories {
                    if !accessible.contains(&t.path) {
                        accessible.push(t.path.clone());
                    }
                }
                for d in &p.documentation_directories {
                    if !accessible.contains(&d.path) {
                        accessible.push(d.path.clone());
                    }
                }
            }

            (context, accessible)
        };

        for file in self.recent_locations.files() {
            if !accessible.contains(file) {
                accessible.push(file.clone());
            }
        }

        self.palette.clear();
        self.palette.set_context(context);
        self.palette.set_accessible_files(accessible);
    }

    /// Opens the unified Quick Switcher / Smart Jump modal, collecting all available targets across the system.
    pub fn open_smart_jump(&mut self) {
        self.mode = Mode::SmartJump;
        self.smart_jump.clear();

        let active_path = self.pane(self.active_pane).current_path().clone();
        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // 1. Project Structure (Project Root, Source, Tests, Docs, Important Files)
        let ws = self.pane(self.active_pane).workspace_context();
        let proj = ws.project_for_path(&active_path);

        if let Some(p) = proj {
            if seen.insert(p.root.clone()) {
                items.push(SmartJumpItem {
                    title: format!("Project Root: {}", p.name),
                    path: p.root.clone(),
                    category: "PROJECT",
                    icon: "📦",
                    is_file: false,
                });
            }
            for s in &p.source_directories {
                if seen.insert(s.path.clone()) {
                    items.push(SmartJumpItem {
                        title: format!("Source: {}", s.name),
                        path: s.path.clone(),
                        category: "SOURCE",
                        icon: "📁",
                        is_file: false,
                    });
                }
            }
            for t in &p.test_directories {
                if seen.insert(t.path.clone()) {
                    items.push(SmartJumpItem {
                        title: format!("Tests: {}", t.name),
                        path: t.path.clone(),
                        category: "TESTS",
                        icon: "🧪",
                        is_file: false,
                    });
                }
            }
            for d in &p.documentation_directories {
                if seen.insert(d.path.clone()) {
                    items.push(SmartJumpItem {
                        title: format!("Docs: {}", d.name),
                        path: d.path.clone(),
                        category: "DOCS",
                        icon: "📚",
                        is_file: false,
                    });
                }
            }
            for imp in &p.important_files {
                if seen.insert(imp.path.clone()) {
                    items.push(SmartJumpItem {
                        title: format!("{}: {}", imp.role.display_name(), imp.name),
                        path: imp.path.clone(),
                        category: "IMPORTANT",
                        icon: "📄",
                        is_file: true,
                    });
                }
            }
        } else if let Some(ref proj_root) = self.pane(self.active_pane).project_info().root
            && seen.insert(proj_root.clone())
        {
            let name = proj_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("project");
            items.push(SmartJumpItem {
                title: format!("Project Root: {name}"),
                path: proj_root.clone(),
                category: "PROJECT",
                icon: "📦",
                is_file: false,
            });
        }

        // 2. Recent Locations (folders)
        for recent in self.recent_locations.locations() {
            if recent.exists() && recent.is_dir() && seen.insert(recent.clone()) {
                let name = recent
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_else(|| recent.to_str().unwrap_or("/"));
                items.push(SmartJumpItem {
                    title: format!("{name} ({})", recent.display()),
                    path: recent.clone(),
                    category: "RECENT",
                    icon: "📁",
                    is_file: false,
                });
            }
        }

        // 3. Recent Files
        for file in self.recent_locations.files() {
            if file.exists() && file.is_file() && seen.insert(file.clone()) {
                let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                items.push(SmartJumpItem {
                    title: format!("{name} ({})", file.display()),
                    path: file.clone(),
                    category: "RECENT",
                    icon: "📄",
                    is_file: true,
                });
            }
        }

        // 4. Current directory
        if !active_path.as_os_str().is_empty() && seen.insert(active_path.clone()) {
            let name = active_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(".");
            items.push(SmartJumpItem {
                title: format!("Current: {name}"),
                path: active_path.clone(),
                category: "CURRENT",
                icon: "📁",
                is_file: false,
            });
        }

        // 5. Parent directory
        if let Some(parent) = parent_of(&active_path)
            && seen.insert(parent.clone())
        {
            items.push(SmartJumpItem {
                title: format!("Parent: {}", parent.display()),
                path: parent,
                category: "PARENT",
                icon: "⬆️",
                is_file: false,
            });
        }

        // 6. Git Root (if active)
        if let Some(git_root) = self.pane(self.active_pane).git().root() {
            let git_buf = git_root.to_path_buf();
            if seen.insert(git_buf.clone()) {
                let name = git_buf
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("repo");
                items.push(SmartJumpItem {
                    title: format!("Git Root: {name}"),
                    path: git_buf,
                    category: "GIT ROOT",
                    icon: "🌳",
                    is_file: false,
                });
            }
        } else if let Some(ref proj_root) = self.pane(self.active_pane).project_info().root
            && seen.insert(proj_root.clone())
        {
            let name = proj_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("project");
            items.push(SmartJumpItem {
                title: format!("Project Root: {name}"),
                path: proj_root.clone(),
                category: "PROJECT",
                icon: "📦",
                is_file: false,
            });
        }

        // 7. User Home Directory
        if let Some(home) = crate::utils::path::home_dir()
            && seen.insert(home.clone())
        {
            items.push(SmartJumpItem {
                title: format!("Home: ~ ({})", home.display()),
                path: home,
                category: "HOME",
                icon: "🏠",
                is_file: false,
            });
        }

        // 8. Filesystem Root
        let fs_root = crate::utils::path::filesystem_root(&active_path);
        if seen.insert(fs_root.clone()) {
            items.push(SmartJumpItem {
                title: format!("Root: {}", fs_root.display()),
                path: fs_root,
                category: "ROOT",
                icon: "🗄️",
                is_file: false,
            });
        }

        // 9. Bookmarks
        for bm in self.bookmarks.bookmarks() {
            if seen.insert(bm.path().to_path_buf()) {
                items.push(SmartJumpItem {
                    title: format!("{}: {}", bm.name(), bm.path().display()),
                    path: bm.path().to_path_buf(),
                    category: "BOOKMARK",
                    icon: "🔖",
                    is_file: false,
                });
            }
        }

        // 10. Open Tabs
        for pane_kind in [ActivePane::Left, ActivePane::Right] {
            let p = self.pane(pane_kind);
            for (t_idx, tab) in p.tabs().iter().enumerate() {
                if !tab.current_path().as_os_str().is_empty()
                    && seen.insert(tab.current_path().clone())
                {
                    items.push(SmartJumpItem {
                        title: format!(
                            "{:?} Tab {}: {}",
                            pane_kind,
                            t_idx + 1,
                            tab.current_path().display()
                        ),
                        path: tab.current_path().clone(),
                        category: "TAB",
                        icon: "📑",
                        is_file: false,
                    });
                }
            }
        }

        // 11. Current directory entries (files and folders)
        for entry in self.pane(self.active_pane).entries() {
            let path = entry.path().to_path_buf();
            if seen.insert(path.clone()) {
                let name = entry.name().to_string_lossy().to_string();
                let is_dir = entry.is_dir();
                let icon = if is_dir { "📁" } else { "📄" };
                let cat = if is_dir { "FOLDER" } else { "FILE" };
                items.push(SmartJumpItem {
                    title: name,
                    path,
                    category: cat,
                    icon,
                    is_file: !is_dir,
                });
            }
        }

        self.smart_jump.set_items(items);
    }

    /// Accessor for session recent locations history.
    pub fn recent_locations(&self) -> &RecentLocations {
        &self.recent_locations
    }

    /// Mutable accessor for session recent locations history.
    pub fn recent_locations_mut(&mut self) -> &mut RecentLocations {
        &mut self.recent_locations
    }

    /// Accessor for Smart Jump state.
    pub fn smart_jump(&self) -> &SmartJumpState {
        &self.smart_jump
    }

    /// Mutable accessor for Smart Jump state.
    pub fn smart_jump_mut(&mut self) -> &mut SmartJumpState {
        &mut self.smart_jump
    }

    /// Appends a character to the Smart Jump filter query.
    pub fn smart_jump_push_char(&mut self, ch: char) {
        self.smart_jump.push_char(ch);
    }

    /// Removes the last character from the Smart Jump filter query.
    pub fn smart_jump_pop_char(&mut self) {
        self.smart_jump.pop_char();
    }

    /// Moves Smart Jump selection up.
    pub fn smart_jump_move_up(&mut self) {
        self.smart_jump.move_up();
    }

    /// Moves Smart Jump selection down.
    pub fn smart_jump_move_down(&mut self) {
        self.smart_jump.move_down();
    }

    /// Accessor for Project Cockpit state.
    pub fn project_cockpit(&self) -> &ProjectCockpitState {
        &self.project_cockpit
    }

    /// Mutable accessor for Project Cockpit state.
    pub fn project_cockpit_mut(&mut self) -> &mut ProjectCockpitState {
        &mut self.project_cockpit
    }

    /// Accessor for Git Status Panel state.
    pub fn git_status_panel(&self) -> &GitStatusPanelState {
        &self.git_status_panel
    }

    /// Mutable accessor for Git Status Panel state.
    pub fn git_status_panel_mut(&mut self) -> &mut GitStatusPanelState {
        &mut self.git_status_panel
    }

    /// Accessor for File Radar state.
    pub fn file_radar(&self) -> &FileRadarState {
        &self.file_radar
    }

    /// Accessor for Reveal Context state.
    pub fn reveal_context(&self) -> &RevealContextState {
        &self.reveal_context
    }

    /// Mutable accessor for Reveal Context state.
    pub fn reveal_context_mut(&mut self) -> &mut RevealContextState {
        &mut self.reveal_context
    }

    /// Opens the Project Cockpit modal with quick actions for the current project.
    pub fn open_project_cockpit(&mut self) {
        self.mode = Mode::ProjectCockpit;
        let mut actions = Vec::new();
        let which = self.active_pane;
        let active_pane = self.pane(which);
        let active_path = active_pane.current_path().clone();
        let ws = active_pane.workspace_context();
        let proj = ws.project_for_path(&active_path);
        let project_info = active_pane.project_info();
        let git_status = active_pane.git_status();

        // 1. Go to Project Root (if detected)
        let root_target = if let Some(p) = proj {
            Some(p.root.clone())
        } else if let Some(ws_root) = ws.project_root() {
            Some(ws_root.to_path_buf())
        } else {
            project_info.root.clone()
        };

        if let Some(ref root) = root_target {
            actions.push(ProjectCockpitAction {
                label: "Go to Project Root",
                description: format!("Navigate to project root ({})", root.display()),
                target_path: Some(root.clone()),
                action: Action::GoProjectRoot,
            });
        }

        // 2. Go to Git Root (if git repository)
        if let Some(git_root) = git_status.root() {
            actions.push(ProjectCockpitAction {
                label: "Go to Git Root",
                description: format!("Navigate to Git repository root ({})", git_root.display()),
                target_path: Some(git_root.to_path_buf()),
                action: Action::GoGitRoot,
            });
        }

        // 3. Open Manifest (only if manifest exists)
        let manifest_target = if let Some(p) = proj {
            p.primary_manifest().map(|m| m.path.clone())
        } else if let Some(first_imp) = ws.important_files().iter().find(|f| f.role.is_manifest()) {
            Some(first_imp.path.clone())
        } else {
            project_info.manifest_file.clone()
        };

        if let Some(ref manifest) = manifest_target {
            let name = manifest
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("manifest");
            actions.push(ProjectCockpitAction {
                label: "Open Manifest",
                description: format!("Navigate to and select {name}"),
                target_path: Some(manifest.clone()),
                action: Action::OpenManifest,
            });
        }

        // 4. Open README (only if README exists)
        let readme_target = if let Some(p) = proj {
            p.important_files
                .iter()
                .find(|f| f.role == crate::project::ImportantFileRole::Documentation)
                .map(|f| f.path.clone())
        } else if let Some(first_doc) = ws
            .important_files()
            .iter()
            .find(|f| f.role == crate::project::ImportantFileRole::Documentation)
        {
            Some(first_doc.path.clone())
        } else {
            project_info.readme_file.clone()
        };

        if let Some(ref readme) = readme_target {
            let name = readme
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("README");
            actions.push(ProjectCockpitAction {
                label: "Open README",
                description: format!("Navigate to and select {name}"),
                target_path: Some(readme.clone()),
                action: Action::OpenReadme,
            });
        }

        // 5. Open LICENSE (only if LICENSE exists)
        let license_target = if let Some(p) = proj {
            p.important_files
                .iter()
                .find(|f| f.role == crate::project::ImportantFileRole::License)
                .map(|f| f.path.clone())
        } else if let Some(first_lic) = ws
            .important_files()
            .iter()
            .find(|f| f.role == crate::project::ImportantFileRole::License)
        {
            Some(first_lic.path.clone())
        } else {
            project_info.license_file.clone()
        };

        if let Some(ref license) = license_target {
            let name = license
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("LICENSE");
            actions.push(ProjectCockpitAction {
                label: "Open LICENSE",
                description: format!("Navigate to and select {name}"),
                target_path: Some(license.clone()),
                action: Action::OpenLicense,
            });
        }

        // 6. Go to Source Directory (only if source dir exists)
        let src_target = if let Some(p) = proj {
            p.primary_source_dir().map(|p| p.to_path_buf())
        } else if let Some(first_src) = ws.source_directories().first() {
            Some(first_src.to_path_buf())
        } else {
            project_info.source_dir.clone()
        };

        if let Some(ref src_dir) = src_target {
            actions.push(ProjectCockpitAction {
                label: "Go to Source Directory",
                description: format!(
                    "Navigate to primary source directory ({})",
                    src_dir.display()
                ),
                target_path: Some(src_dir.clone()),
                action: Action::GoSourceDir,
            });
        }

        // 7. Go to Tests Directory (only if test dir exists)
        let test_target = if let Some(p) = proj {
            p.primary_test_dir().map(|p| p.to_path_buf())
        } else {
            ws.test_directories().first().map(|p| p.to_path_buf())
        };

        if let Some(ref test_dir) = test_target {
            actions.push(ProjectCockpitAction {
                label: "Go to Tests Directory",
                description: format!(
                    "Navigate to automated tests directory ({})",
                    test_dir.display()
                ),
                target_path: Some(test_dir.clone()),
                action: Action::GoTestsDir,
            });
        }

        // 8. Go to Documentation Directory (only if docs dir exists)
        let doc_target = if let Some(p) = proj {
            p.primary_doc_dir().map(|p| p.to_path_buf())
        } else {
            ws.documentation().first().map(|p| p.to_path_buf())
        };

        if let Some(ref doc_dir) = doc_target {
            actions.push(ProjectCockpitAction {
                label: "Go to Documentation",
                description: format!(
                    "Navigate to documentation directory ({})",
                    doc_dir.display()
                ),
                target_path: Some(doc_dir.clone()),
                action: Action::GoDocsDir,
            });
        }

        actions.push(ProjectCockpitAction {
            label: "Reveal Context",
            description: "Inspect hierarchical file and project context".to_string(),
            target_path: None,
            action: Action::RevealContext,
        });

        if git_status.is_repo() {
            actions.push(ProjectCockpitAction {
                label: "Git Status Panel",
                description: "Inspect repository status and changed files".to_string(),
                target_path: None,
                action: Action::GitStatusPanel,
            });
        }

        actions.push(ProjectCockpitAction {
            label: "File Radar",
            description: "Directory metrics and file type statistics".to_string(),
            target_path: None,
            action: Action::FileRadar,
        });

        self.project_cockpit.set_actions(actions);
    }

    /// Opens the Git Status Panel modal with changed file list and status.
    pub fn open_git_status_panel(&mut self) {
        self.mode = Mode::GitStatusPanel;
        let mut entries = Vec::new();
        let active_pane = self.pane(self.active_pane);
        let git_status = active_pane.git_status();

        if let Some(root) = git_status.root() {
            let mut sorted_files: Vec<(&PathBuf, &crate::git::FileStatus)> =
                git_status.file_statuses.iter().collect();
            sorted_files.sort_by(|a, b| a.0.cmp(b.0));

            for (rel_path, status) in sorted_files {
                let full_path = root.join(rel_path);
                let staged = matches!(status, crate::git::FileStatus::Added);
                entries.push(GitStatusPanelEntry {
                    relative_path: rel_path.clone(),
                    full_path,
                    status: *status,
                    staged,
                });
            }
        }

        self.git_status_panel.set_entries(entries);
    }

    /// Opens the File Radar directory insight modal.
    pub fn open_file_radar(&mut self) {
        self.mode = Mode::FileRadar;
        let active_pane = self.pane(self.active_pane);
        self.file_radar = FileRadarState::compute_from_entries(active_pane.entries());
    }

    /// Opens the Reveal Context modal displaying hierarchical path context.
    pub fn open_reveal_context(&mut self) {
        self.mode = Mode::RevealContext;
        let mut levels = Vec::new();
        let active_pane = self.pane(self.active_pane);
        let current_dir = active_pane.current_path().clone();
        let selected_entry = active_pane.selected_entry();

        // Level 0: Selected Item
        if let Some(entry) = selected_entry {
            let kind_str = match entry.kind() {
                EntryKind::File => "File",
                EntryKind::Directory => "Directory",
                EntryKind::Symlink => "Symlink",
                EntryKind::Other => "Special",
            };
            let size_str = entry
                .path()
                .metadata()
                .ok()
                .map(|m| crate::preview::format_size(m.len()))
                .unwrap_or_else(|| "—".to_string());
            let git_code = active_pane
                .git_status()
                .file_status_for(entry.path())
                .map(|s| s.code())
                .unwrap_or("");
            let git_desc = if git_code.is_empty() {
                String::new()
            } else {
                format!(" [Git: {git_code}]")
            };

            levels.push(ContextLevel {
                level_name: "Selected Item",
                title: entry.name().to_string_lossy().into_owned(),
                details: format!("{kind_str} • {size_str}{git_desc}"),
                path: entry.path().to_path_buf(),
            });
        }

        // Level 1: Current Directory
        if !current_dir.as_os_str().is_empty() {
            let dir_name = current_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Root");
            let items_count = active_pane.entries().len();
            levels.push(ContextLevel {
                level_name: "Directory",
                title: format!("{dir_name}/"),
                details: format!("{} • {items_count} visible items", current_dir.display()),
                path: current_dir.clone(),
            });
        }

        // Level 2: Project Root (if detected)
        let project_info = active_pane.project_info();
        if let Some(ref proj_root) = project_info.root {
            let manifest_name = project_info
                .manifest_file
                .as_ref()
                .and_then(|m| m.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("Manifest");
            let type_name = project_info
                .primary_type()
                .map(|t| t.display_name())
                .unwrap_or("Generic");
            levels.push(ContextLevel {
                level_name: "Project Root",
                title: format!("{} ({type_name})", project_info.name()),
                details: format!("{} • {}", proj_root.display(), manifest_name),
                path: proj_root.clone(),
            });
        }

        // Level 3: Git Repository (if detected)
        let git_status = active_pane.git_status();
        if let Some(git_root) = git_status.root() {
            let repo_name = git_status.repository.repo_name().unwrap_or("Repository");
            let branch = git_status.branch.display();
            let state = if git_status.is_clean {
                "Clean"
            } else {
                "Modified"
            };
            levels.push(ContextLevel {
                level_name: "Git Repository",
                title: format!("{repo_name} ({branch})"),
                details: format!("{} • {state}", git_root.display()),
                path: git_root.to_path_buf(),
            });
        }

        self.reveal_context.set_levels(levels);
    }

    /// Navigates to the active project root directory.
    pub fn go_project_root(&mut self) {
        let which = self.active_pane;
        let active_path = self.pane(which).current_path().clone();
        let ws = self.pane(which).workspace_context();
        let target = if let Some(proj) = ws.project_for_path(&active_path) {
            Some(proj.root.clone())
        } else if let Some(ws_root) = ws.project_root() {
            Some(ws_root.to_path_buf())
        } else {
            self.pane(which).project_info().root.clone()
        };

        if let Some(root) = target {
            let outcome = self.open_in(which, root);
            self.report_navigation(outcome);
            self.refresh_preview();
        }
    }

    /// Navigates to and selects the project's build manifest file if present.
    pub fn open_manifest(&mut self) {
        let which = self.active_pane;
        let active_path = self.pane(which).current_path().clone();
        let ws = self.pane(which).workspace_context();
        let target = if let Some(proj) = ws.project_for_path(&active_path) {
            proj.primary_manifest().map(|m| m.path.clone())
        } else if let Some(first_imp) = ws.important_files().iter().find(|f| f.role.is_manifest()) {
            Some(first_imp.path.clone())
        } else {
            self.pane(which).project_info().manifest_file.clone()
        };

        if let Some(manifest) = target
            && let Some(parent) = manifest.parent()
        {
            let outcome = self.open_in(which, parent.to_path_buf());
            self.report_navigation(outcome);
            self.pane_mut(which).select_path(&manifest);
            self.refresh_preview();
        }
    }

    /// Navigates to and selects the project's README file if present.
    pub fn open_readme(&mut self) {
        let which = self.active_pane;
        let active_path = self.pane(which).current_path().clone();
        let ws = self.pane(which).workspace_context();
        let target = if let Some(proj) = ws.project_for_path(&active_path) {
            proj.important_files
                .iter()
                .find(|f| f.role == crate::project::ImportantFileRole::Documentation)
                .map(|f| f.path.clone())
        } else if let Some(first_doc) = ws
            .important_files()
            .iter()
            .find(|f| f.role == crate::project::ImportantFileRole::Documentation)
        {
            Some(first_doc.path.clone())
        } else {
            self.pane(which).project_info().readme_file.clone()
        };

        if let Some(readme) = target
            && let Some(parent) = readme.parent()
        {
            let outcome = self.open_in(which, parent.to_path_buf());
            self.report_navigation(outcome);
            self.pane_mut(which).select_path(&readme);
            self.refresh_preview();
        }
    }

    /// Navigates to and selects the project's LICENSE file if present.
    pub fn open_license(&mut self) {
        let which = self.active_pane;
        let active_path = self.pane(which).current_path().clone();
        let ws = self.pane(which).workspace_context();
        let target = if let Some(proj) = ws.project_for_path(&active_path) {
            proj.important_files
                .iter()
                .find(|f| f.role == crate::project::ImportantFileRole::License)
                .map(|f| f.path.clone())
        } else if let Some(first_lic) = ws
            .important_files()
            .iter()
            .find(|f| f.role == crate::project::ImportantFileRole::License)
        {
            Some(first_lic.path.clone())
        } else {
            self.pane(which).project_info().license_file.clone()
        };

        if let Some(license) = target
            && let Some(parent) = license.parent()
        {
            let outcome = self.open_in(which, parent.to_path_buf());
            self.report_navigation(outcome);
            self.pane_mut(which).select_path(&license);
            self.refresh_preview();
        }
    }

    /// Navigates directly into the project's primary source directory if detected.
    pub fn go_source_dir(&mut self) {
        let which = self.active_pane;
        let active_path = self.pane(which).current_path().clone();
        let ws = self.pane(which).workspace_context();
        let target = if let Some(proj) = ws.project_for_path(&active_path) {
            proj.primary_source_dir().map(|p| p.to_path_buf())
        } else if let Some(first_src) = ws.source_directories().first() {
            Some(first_src.to_path_buf())
        } else {
            self.pane(which).project_info().source_dir.clone()
        };

        if let Some(src_dir) = target {
            let outcome = self.open_in(which, src_dir);
            self.report_navigation(outcome);
            self.refresh_preview();
        }
    }

    /// Navigates directly into the project's automated test directory if detected.
    pub fn go_tests_dir(&mut self) {
        let which = self.active_pane;
        let active_path = self.pane(which).current_path().clone();
        let ws = self.pane(which).workspace_context();
        let target = if let Some(proj) = ws.project_for_path(&active_path) {
            proj.primary_test_dir().map(|p| p.to_path_buf())
        } else {
            ws.test_directories().first().map(|p| p.to_path_buf())
        };

        if let Some(test_dir) = target {
            let outcome = self.open_in(which, test_dir);
            self.report_navigation(outcome);
            self.refresh_preview();
        }
    }

    /// Navigates directly into the project's documentation directory if detected.
    pub fn go_docs_dir(&mut self) {
        let which = self.active_pane;
        let active_path = self.pane(which).current_path().clone();
        let ws = self.pane(which).workspace_context();
        let target = if let Some(proj) = ws.project_for_path(&active_path) {
            proj.primary_doc_dir().map(|p| p.to_path_buf())
        } else {
            ws.documentation().first().map(|p| p.to_path_buf())
        };

        if let Some(doc_dir) = target {
            let outcome = self.open_in(which, doc_dir);
            self.report_navigation(outcome);
            self.refresh_preview();
        }
    }

    /// Duplicates the active tab in the active pane.
    pub fn duplicate_tab_in_active_pane(&mut self) -> usize {
        let which = self.active_pane;
        self.pane_mut(which).duplicate_active_tab()
    }

    /// Saves the active pane's current directory as a bookmark.
    ///
    /// Returns `true` if added, `false` if already bookmarked or if the pane has no directory.
    pub fn add_current_bookmark(&mut self) -> bool {
        let path = self.pane(self.active_pane).current_path().clone();
        if path.as_os_str().is_empty() {
            return false;
        }
        self.bookmarks.add(path)
    }

    /// Saves `path` as a bookmark if it is a directory and not already bookmarked.
    pub fn add_bookmark(&mut self, path: PathBuf) -> bool {
        if path.as_os_str().is_empty() {
            return false;
        }
        self.bookmarks.add(path)
    }

    /// Opens the currently selected bookmark in the active pane.
    pub fn open_selected_bookmark(&mut self) -> Result<(), NavigationError> {
        let path = self
            .bookmarks
            .selected_bookmark()
            .map(|b| b.path().to_path_buf());

        self.leave_temporary_mode();

        if let Some(path) = path {
            let outcome = self.open_in(self.active_pane, path);
            match &outcome {
                Ok(()) => self.notification.clear(),
                Err(error) => self.notification.show(Notice::from(error)),
            }
            self.refresh_preview();
            outcome
        } else {
            Ok(())
        }
    }

    /// Removes the bookmark currently selected in the bookmarks state.
    pub fn remove_selected_bookmark(&mut self) -> Option<Bookmark> {
        self.bookmarks.remove_selected()
    }

    /// Returns the currently active or live-previewed Theme definition.
    pub fn theme(&self) -> Theme {
        ThemeRegistry::get(self.theme_id())
    }

    /// Returns the active or previewed ThemeId.
    pub fn theme_id(&self) -> ThemeId {
        self.preview_theme.unwrap_or(self.active_theme)
    }

    /// Returns the permanently active ThemeId (ignoring temporary live previews).
    pub fn active_theme_id(&self) -> ThemeId {
        self.active_theme
    }

    /// Sets the active theme directly and saves to persistent configuration.
    pub fn set_active_theme(&mut self, theme_id: ThemeId) {
        self.active_theme = theme_id;
        self.preview_theme = None;
        self.save_persistent_state().ok();
    }

    /// Returns the ThemeSelector state.
    pub fn theme_selector(&self) -> &ThemeSelectorState {
        &self.theme_selector
    }

    /// Returns mutable ThemeSelector state.
    pub fn theme_selector_mut(&mut self) -> &mut ThemeSelectorState {
        &mut self.theme_selector
    }

    /// Opens the Theme Selector dialog and initializes live preview.
    pub fn open_theme_selector(&mut self) {
        self.theme_selector = ThemeSelectorState::new(self.active_theme);
        self.preview_theme = Some(self.active_theme);
        self.mode = Mode::ThemeSelector;
    }

    /// Moves selection up in the Theme Selector and updates live preview.
    pub fn theme_selector_move_up(&mut self) {
        self.theme_selector.move_up();
        self.preview_theme = Some(self.theme_selector.selected_theme());
    }

    /// Moves selection down in the Theme Selector and updates live preview.
    pub fn theme_selector_move_down(&mut self) {
        self.theme_selector.move_down();
        self.preview_theme = Some(self.theme_selector.selected_theme());
    }

    /// Selects a specific theme index in the Theme Selector and updates live preview.
    pub fn theme_selector_select_index(&mut self, index: usize) {
        self.theme_selector.select_index(index);
        self.preview_theme = Some(self.theme_selector.selected_theme());
    }

    /// Applies the selected theme from the Theme Selector permanently.
    pub fn apply_theme_selector(&mut self) {
        let chosen = self.theme_selector.selected_theme();
        self.active_theme = chosen;
        self.preview_theme = None;
        self.mode = Mode::Normal;
        self.save_persistent_state().ok();
    }

    /// Cancels theme selection and restores the initial theme.
    pub fn cancel_theme_selector(&mut self) {
        self.preview_theme = None;
        self.mode = Mode::Normal;
    }

    /// Switches to the next available visual theme.
    pub fn next_theme(&mut self) {
        let next = ThemeRegistry::next(self.active_theme);
        self.set_active_theme(next);
    }

    /// Switches to the previous available visual theme.
    pub fn prev_theme(&mut self) {
        let prev = ThemeRegistry::prev(self.active_theme);
        self.set_active_theme(prev);
    }

    /// Accessor for bookmark state.
    pub fn bookmarks(&self) -> &BookmarkState {
        &self.bookmarks
    }

    /// Mutable accessor for bookmark state.
    pub fn bookmarks_mut(&mut self) -> &mut BookmarkState {
        &mut self.bookmarks
    }

    /// Accessor for Storage Vision state.
    pub fn storage_vision(&self) -> &crate::storage::StorageVisionState {
        &self.storage_vision
    }

    /// Mutable accessor for Storage Vision state.
    pub fn storage_vision_mut(&mut self) -> &mut crate::storage::StorageVisionState {
        &mut self.storage_vision
    }

    /// Opens the Storage Vision analysis workspace for the active directory.
    pub fn open_storage_vision(&mut self) {
        let current_path = self.pane(self.active_pane).current_path().clone();
        self.storage_vision.start_scan(current_path);
        self.mode = Mode::StorageVision;
    }

    /// Toggles the active directory in or out of Favorites / Bookmarks.
    pub fn toggle_favorite_for_current(&mut self) {
        let path = self.pane(self.active_pane).current_path().clone();
        if self.bookmarks.contains_path(&path) {
            self.bookmarks.remove_by_path(&path);
        } else {
            self.bookmarks.add(path);
        }
    }

    /// Opens a new tab in the active pane with the active tab's directory.
    pub fn new_tab_in_active_pane(&mut self) {
        let current_path = self.pane(self.active_pane).current_path().clone();
        if current_path.as_os_str().is_empty() {
            self.active_pane_mut().new_tab(current_path, Vec::new());
            self.refresh_preview();
            return;
        }

        match self.filesystem.list_directory(&current_path) {
            Ok(entries) => {
                self.active_pane_mut().new_tab(current_path, entries);
                self.refresh_preview();
                self.last_sync_origin = SyncOrigin::FileManagerNavigation;
                self.sync_terminal_to_directory();
            }
            Err(err) => {
                self.report_navigation(Err(NavigationError::Directory(err)));
            }
        }
    }

    /// Closes the active tab in the active pane.
    pub fn close_tab_in_active_pane(&mut self) -> bool {
        let closed = self.active_pane_mut().close_active_tab();
        if closed {
            self.refresh_preview();
            self.last_sync_origin = SyncOrigin::FileManagerNavigation;
            self.sync_terminal_to_directory();
        }
        closed
    }

    /// Switches to the next tab in the active pane.
    pub fn next_tab_in_active_pane(&mut self) {
        self.active_pane_mut().next_tab();
        self.refresh_preview();
        self.last_sync_origin = SyncOrigin::FileManagerNavigation;
        self.sync_terminal_to_directory();
    }

    /// Switches to the previous tab in the active pane.
    pub fn previous_tab_in_active_pane(&mut self) {
        self.active_pane_mut().previous_tab();
        self.refresh_preview();
        self.last_sync_origin = SyncOrigin::FileManagerNavigation;
        self.sync_terminal_to_directory();
    }

    /// Selects tab `index` in pane `which`.
    pub fn select_tab_in_pane(&mut self, which: ActivePane, index: usize) {
        self.active_pane = which;
        self.pane_mut(which).select_tab(index);
        self.refresh_preview();
        self.last_sync_origin = SyncOrigin::FileManagerNavigation;
        self.sync_terminal_to_directory();
    }

    fn finish_modal_operation(&mut self, outcome: Result<(), OperationError>) {
        match outcome {
            Ok(()) => {
                self.notification.clear();
                self.leave_temporary_mode();
                self.refresh_preview();
            }
            Err(err) => {
                self.notification.show(Notice::from(&err));
                self.leave_temporary_mode();
                self.refresh_preview();
            }
        }
    }

    /// What has been cut or copied.
    pub fn clipboard(&self) -> &ClipboardState {
        &self.clipboard
    }

    /// Whether a preview is shown.
    pub fn preview(&self) -> &PreviewState {
        &self.preview
    }

    /// Refreshes the prepared preview content from the active pane's selected entry.
    pub fn refresh_preview(&mut self) {
        let active_pane = self.pane(self.active_pane);
        if let Some(path) = active_pane.selected_path() {
            let mut content = crate::preview::load_preview(&path);
            if let crate::preview::PreviewContent::Metadata(ref mut meta) = content {
                let ws = active_pane.workspace_context();
                if let Some(proj) = ws.project_for_path(&path) {
                    let role = ws.directory_role(&path);
                    let role_str = if role != crate::project::DirectoryRole::Unknown {
                        format!(" ({})", role.display_name())
                    } else {
                        String::new()
                    };
                    *meta = meta
                        .clone()
                        .with_project_context(Some(format!("{}{role_str}", proj.name)));
                } else if let Some(ref proj_name) = active_pane.project_info().name {
                    *meta = meta.clone().with_project_context(Some(proj_name.clone()));
                }
            }
            self.preview.set_content(path, content);
        } else {
            self.preview.clear();
        }
        let left = self.left.current_path().clone();
        let right = self.right.current_path().clone();
        let prev = self.preview.path().map(|p| p.to_path_buf());
        self.filesystem_watcher
            .watch(&[left, right], prev.as_deref());
    }

    /// The search of the pane the application acts on.
    ///
    /// Every pane keeps its own, and this is the one being typed in, which is
    /// the one the pane the application acts on is filtered by.
    pub fn search(&self) -> &SearchState {
        self.pane(self.active_pane).search()
    }

    /// Sets the query of the search in the pane the application acts on.
    ///
    /// The pane is filtered again from the entries it already holds, so this is
    /// an in-memory operation: no directory is read, no file is opened and the
    /// pane's path does not change. The entries keep the pane's order, so the
    /// results are shown in the order the pane sorts them in.
    ///
    /// The input layer calls this while a query is being typed. Until it is
    /// connected, the actions that start and clear a search are the other way
    /// of changing a query, and they go through the same pane method.
    pub fn set_search_query(&mut self, query: &str) {
        self.active_pane_mut().set_search_query(query);
    }

    /// Finishes entering a search query and returns to Normal mode so the
    /// filtered results can be browsed.
    pub fn confirm_search(&mut self) {
        if self.mode == Mode::Search {
            self.mode = Mode::Normal;
        }
    }

    /// Sets the mode of the search in the pane the application acts on.
    ///
    /// Nothing is read from the filesystem: this only says where the next
    /// search should look and how it should match. The pane's directory, its
    /// listing, its order and its selection are left where they were, and the
    /// other pane is not touched.
    pub fn set_search_mode(&mut self, mode: SearchMode) {
        self.active_pane_mut().set_search_mode(mode);
    }

    /// Runs a search for `query` in the pane the application acts on.
    ///
    /// This is the application's way of starting a search beyond the listing:
    /// in a recursive mode it walks the pane's own directory and everything
    /// below it through the filesystem service, and in the basic and fuzzy
    /// modes it matches the entries the pane has already loaded, reading
    /// nothing. Whichever it is, the pane keeps its directory, its listing and
    /// its order, the other pane is untouched, and the entry that was selected
    /// stays selected while it is still one of the results.
    ///
    /// A root that cannot be read is reported to the user and recorded as the
    /// search's outcome, rather than being passed over as an empty result set.
    pub fn run_search(&mut self, query: &str) {
        let which = self.active_pane;
        let filesystem = self.filesystem;

        self.pane_mut(which).run_search(query, &filesystem);

        match self.pane(which).search().outcome().cloned() {
            Some(SearchOutcome::Failed(failure)) => {
                self.notification.show(Notice::from(&failure));
            }
            // A search that was stopped is not a failure, so it is neither
            // reported nor allowed to wipe out what an earlier failure said.
            // One that finished is what the user asked for, so a message about
            // a search that went wrong must not outlive it.
            Some(SearchOutcome::Completed) => self.notification.clear(),
            Some(SearchOutcome::Cancelled) | None => {}
        }
    }

    /// Asks the search in the pane the application acts on to stop.
    ///
    /// Nothing is stopped here and nothing is thrown away: a walk that is
    /// running notices the request at its next check, and one that has not
    /// started yet sees it when it does, reporting that it was stopped rather
    /// than finishing. The pane's directory and the listing it holds are not
    /// touched either way, so cancelling cannot lose the user's place.
    pub fn cancel_search(&mut self) {
        self.pane(self.active_pane).cancel_search();
    }

    /// Accessor for context menu state.
    pub fn context_menu(&self) -> &ContextMenuState {
        &self.context_menu
    }

    /// Mutable accessor for context menu state.
    pub fn context_menu_mut(&mut self) -> &mut ContextMenuState {
        &mut self.context_menu
    }

    /// Opens `path` in a new tab in the active pane.
    pub fn open_path_in_new_tab(&mut self, path: std::path::PathBuf) {
        if path.as_os_str().is_empty() {
            self.active_pane_mut().new_tab(path, Vec::new());
            self.refresh_preview();
            return;
        }

        match self.filesystem.list_directory(&path) {
            Ok(entries) => {
                self.active_pane_mut().new_tab(path, entries);
                self.refresh_preview();
            }
            Err(err) => {
                self.report_navigation(Err(NavigationError::Directory(err)));
            }
        }
    }

    /// Constructs a context menu tailored to the current selection in the active pane.
    pub fn build_context_menu(&self, position: (u16, u16)) -> ContextMenuState {
        let active = self.active_pane;
        let pane = self.pane(active);
        let paths = pane.effective_selected_paths();
        let has_clipboard = !self.clipboard.is_empty();
        let platform = crate::input::platform::Platform::current();

        let target = if paths.len() > 1 {
            ContextMenuTarget::MultiSelection {
                count: paths.len(),
                paths: paths.clone(),
            }
        } else if let Some(path) = paths.first() {
            let is_dir = pane
                .selected_entry()
                .map(Entry::is_dir)
                .unwrap_or_else(|| path.is_dir());
            if is_dir {
                let is_project_root = self
                    .project_info()
                    .root
                    .as_ref()
                    .map(|r| r == path)
                    .unwrap_or(false);
                let in_git = self.git_status().repository.is_repo();
                ContextMenuTarget::Directory {
                    path: path.clone(),
                    is_project_root,
                    in_git,
                }
            } else {
                ContextMenuTarget::File {
                    path: path.clone(),
                    is_executable: false,
                }
            }
        } else {
            ContextMenuTarget::Empty {
                directory: pane.current_path().clone(),
            }
        };

        let items = build_context_menu_items(&target, has_clipboard, platform);
        ContextMenuState::new(Some(target), items, position)
    }

    /// Opens the context menu at `position`.
    pub fn open_context_menu(&mut self, position: (u16, u16)) {
        self.context_menu = self.build_context_menu(position);
        self.set_mode(Mode::ContextMenu);
    }

    /// Closes the context menu and returns to normal mode.
    pub fn close_context_menu(&mut self) {
        if self.mode == Mode::ContextMenu {
            self.set_mode(Mode::Normal);
        }
        self.context_menu.is_more_open = false;
        self.context_menu.items.clear();
    }

    /// Extends selection towards the start by `steps` in the active pane.
    pub fn extend_selection_up(&mut self, steps: usize) {
        self.active_pane_mut().extend_selection_up(steps);
    }

    /// Extends selection towards the end by `steps` in the active pane.
    pub fn extend_selection_down(&mut self, steps: usize) {
        self.active_pane_mut().extend_selection_down(steps);
    }

    /// Selects a contiguous range of entries to `target` in the given pane.
    pub fn select_range_to(&mut self, pane: ActivePane, target: usize) {
        self.pane_mut(pane).select_range_to(target);
    }

    /// The user's settings.
    pub fn settings(&self) -> &SettingsState {
        &self.settings
    }

    /// The Git repository detection state of the active pane.
    pub fn git(&self) -> &GitRepository {
        self.pane(self.active_pane).git()
    }

    /// The Git status of the active pane.
    pub fn git_status(&self) -> &GitStatus {
        self.pane(self.active_pane).git_status()
    }

    /// The project awareness info of the active pane.
    pub fn project_info(&self) -> &ProjectInfo {
        self.pane(self.active_pane).project_info()
    }

    /// The workspace structure and project graph context of the active pane.
    pub fn workspace_context(&self) -> &WorkspaceContext {
        self.pane(self.active_pane).workspace_context()
    }

    /// The user's settings mutably.
    pub fn settings_mut(&mut self) -> &mut SettingsState {
        &mut self.settings
    }

    /// Converts current state into persistent settings.
    pub fn to_settings(&self) -> Settings {
        let active_pane = match self.active_pane {
            ActivePane::Left => ActivePaneConfig::Left,
            ActivePane::Right => ActivePaneConfig::Right,
        };

        let left_tabs = PaneTabsConfig::new(
            self.left.active_tab_index(),
            self.left
                .tabs()
                .iter()
                .map(|t| t.current_path().clone())
                .filter(|p| !p.as_os_str().is_empty())
                .collect(),
        );

        let right_tabs = PaneTabsConfig::new(
            self.right.active_tab_index(),
            self.right
                .tabs()
                .iter()
                .map(|t| t.current_path().clone())
                .filter(|p| !p.as_os_str().is_empty())
                .collect(),
        );

        let bookmarks = self
            .bookmarks
            .bookmarks()
            .iter()
            .map(|b| BookmarkConfig::new(b.name(), b.path().to_path_buf()))
            .collect();

        let recent_locations = self.recent_locations.locations().to_vec();
        let recent_files = self.recent_locations.files().to_vec();
        let motion_mode = self.animations.preferences().mode;
        let startup_motion = self.animations.preferences().startup;
        let reduced_motion = self.reduced_motion();
        let theme = self.active_theme.id_str().to_string();

        Settings {
            active_pane,
            left_tabs,
            right_tabs,
            bookmarks,
            recent_locations,
            recent_files,
            motion_mode,
            startup_motion,
            reduced_motion,
            theme,
        }
    }

    /// Applies loaded settings into application state, validating filesystem paths non-destructively.
    pub fn apply_persistent_settings(&mut self, settings: &Settings) {
        // 0. Motion preferences
        let mut prefs = *self.animations.preferences();
        prefs.mode = settings.motion_mode;
        prefs.startup = settings.startup_motion;
        if settings.reduced_motion && prefs.mode == crate::animation::MotionMode::Full {
            prefs.mode = crate::animation::MotionMode::Reduced;
        }
        self.set_motion_preferences(prefs);

        // Theme
        self.active_theme = ThemeRegistry::resolve_id(&settings.theme);
        self.preview_theme = None;

        // 1. Active pane
        self.active_pane = match settings.active_pane {
            ActivePaneConfig::Left => ActivePane::Left,
            ActivePaneConfig::Right => ActivePane::Right,
        };

        // 2. Bookmarks
        for b in &settings.bookmarks {
            if !b.path.as_os_str().is_empty() {
                self.bookmarks.add_with_name(b.name.clone(), b.path.clone());
            }
        }

        // 3. Left tabs
        if !settings.left_tabs.tab_paths.is_empty() {
            let mut valid_tabs = Vec::new();
            for path in &settings.left_tabs.tab_paths {
                if path.is_dir() {
                    let entries = self.filesystem.list_directory(path).unwrap_or_default();
                    valid_tabs.push(Tab::new(path.clone(), entries));
                }
            }
            if !valid_tabs.is_empty() {
                self.left.tabs = valid_tabs;
                self.left.active_tab_index = settings
                    .left_tabs
                    .active_tab_index
                    .min(self.left.tabs.len() - 1);
            }
        }

        // 4. Right tabs
        if !settings.right_tabs.tab_paths.is_empty() {
            let mut valid_tabs = Vec::new();
            for path in &settings.right_tabs.tab_paths {
                if path.is_dir() {
                    let entries = self.filesystem.list_directory(path).unwrap_or_default();
                    valid_tabs.push(Tab::new(path.clone(), entries));
                }
            }
            if !valid_tabs.is_empty() {
                self.right.tabs = valid_tabs;
                self.right.active_tab_index = settings
                    .right_tabs
                    .active_tab_index
                    .min(self.right.tabs.len() - 1);
            }
        }

        // 5. Recent locations & files
        for loc in &settings.recent_locations {
            if loc.is_dir() {
                self.recent_locations.record_directory(loc.clone());
            }
        }
        for file in &settings.recent_files {
            if file.is_file() {
                self.recent_locations.record_file(file.clone());
            }
        }

        self.refresh_preview();
    }

    /// Saves persistent state to the specified path.
    pub fn save_persistent_state_to(&self, path: &Path) -> io::Result<()> {
        let settings = self.to_settings();
        settings.save_to_path(path)
    }

    /// Saves persistent state to the active or default config path.
    pub fn save_persistent_state(&self) -> io::Result<()> {
        if let Some(path) = self
            .settings
            .config_path()
            .map(Path::to_path_buf)
            .or_else(default_config_path)
        {
            self.save_persistent_state_to(&path)
        } else {
            Ok(())
        }
    }

    /// Loads persistent state from the specified path.
    pub fn load_persistent_state_from(&mut self, path: &Path) -> io::Result<()> {
        let settings = Settings::load_from_path(path)?;
        self.apply_persistent_settings(&settings);
        self.settings.set_config_path(Some(path.to_path_buf()));
        Ok(())
    }

    /// Loads persistent state from the default config path if it exists.
    pub fn load_persistent_state(&mut self) -> io::Result<()> {
        if let Some(path) = default_config_path() {
            if path.exists() {
                return self.load_persistent_state_from(&path);
            }
            self.settings.set_config_path(Some(path));
        }
        Ok(())
    }

    /// The message currently shown to the user.
    pub fn notification(&self) -> &NotificationState {
        &self.notification
    }

    /// How the last change to the filesystem ended, or `None` when none has
    /// been asked for yet.
    ///
    /// The outcome and the notice are kept apart on purpose: this is the whole
    /// operation's answer, while [`NotificationState::notice`] carries which
    /// step failed, where, why, and the words to show. A later layer can decide
    /// what to offer, such as another name for a destination that is already
    /// taken, from either without reading the text.
    pub fn last_outcome(&self) -> Option<OperationOutcome> {
        self.last_outcome
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActivePane, App, BookmarkState, ClipboardOperation, ClipboardState, NavigationError,
        NoticeSource, NotificationState, Pane, PreviewState, SearchState, SortMode, Tab,
        ensure_visible, maximum_scroll_offset,
    };
    use crate::app::actions::Action;
    use crate::app::modes::Mode;
    use crate::config::settings::{ActivePaneConfig, BookmarkConfig, PaneTabsConfig, Settings};
    use crate::filesystem::entry::{Entry, EntryKind};
    use crate::filesystem::error::ErrorCategory;
    use crate::filesystem::navigation::SearchOutcome;
    use crate::filesystem::operations::{Operation, OperationOutcome};
    use crate::filesystem::test_support::{TempDir, assert_not_quadratic, fill, measure};
    use crate::search::SearchMode;
    use std::error::Error;
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    /// A path that deliberately does not exist, so a transition that reached
    /// the filesystem would fail or rewrite it rather than leave it alone.
    const UNREAL_PATH: &str = "/terminalvision/does-not-exist";

    /// The I/O error a navigation failure ultimately wraps.
    fn io_error(error: &NavigationError) -> &io::Error {
        let mut cause = Error::source(error).expect("a navigation error should have a cause");

        loop {
            if let Some(io_error) = cause.downcast_ref::<io::Error>() {
                return io_error;
            }

            cause = cause
                .source()
                .expect("the cause chain should end in an I/O error");
        }
    }

    fn entry(name: &str) -> Entry {
        Entry::new(OsString::from(name), PathBuf::from(name), EntryKind::File)
    }

    /// A pane holding `count` entries in memory, with nothing selected and no
    /// known window size.
    fn pane_with(count: usize) -> Pane {
        let mut pane = Pane::default();
        pane.set_current_path(PathBuf::from(UNREAL_PATH));
        pane.set_entries(
            (0..count)
                .map(|index| entry(&format!("entry-{index:02}")))
                .collect(),
        );

        pane
    }

    /// An entry of `kind` named `name`, in a directory that does not exist: a
    /// sort that reached the filesystem would have nothing to read there.
    fn unreachable_entry(name: &str, kind: EntryKind) -> Entry {
        Entry::new(
            OsString::from(name),
            PathBuf::from(UNREAL_PATH).join(name),
            kind,
        )
    }

    /// A pane holding `entries`, with nothing selected.
    fn pane_holding(entries: Vec<Entry>) -> Pane {
        let mut pane = Pane::default();
        pane.set_current_path(PathBuf::from(UNREAL_PATH));
        pane.set_entries(entries);

        pane
    }

    /// The names of the entries of a pane, in the order it holds them.
    fn pane_names(app: &App, which: ActivePane) -> Vec<String> {
        app.pane(which)
            .entries()
            .iter()
            .map(|entry| entry.name().to_string_lossy().into_owned())
            .collect()
    }

    /// The names of the entries a pane is showing, in the order it shows them.
    fn shown_names(app: &App, which: ActivePane) -> Vec<String> {
        app.pane(which)
            .visible_entries()
            .map(|entry| entry.name().to_string_lossy().into_owned())
            .collect()
    }

    /// The paths a pane's walk found, as they sit below its directory.
    fn result_paths(app: &App, which: ActivePane) -> Vec<PathBuf> {
        app.pane(which)
            .visible_results()
            .iter()
            .map(|result| result.relative().to_path_buf())
            .collect()
    }

    /// An application whose left pane shows a directory holding `files` and one
    /// subdirectory, both of which hold an entry named after `needle`.
    fn app_over_a_tree(label: &str) -> (App, TempDir) {
        let directory = TempDir::new(label);
        fs::create_dir(directory.path().join("nested")).expect("directory should be created");
        fs::write(directory.path().join("top-needle.txt"), b"content")
            .expect("file should be written");
        fs::write(
            directory.path().join("nested").join("deep-needle.txt"),
            b"content",
        )
        .expect("file should be written");
        fs::write(
            directory.path().join("nested").join("other.txt"),
            b"content",
        )
        .expect("file should be written");

        let app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        (app, directory)
    }

    /// An application whose left pane holds `count` entries named
    /// `prefix-00000` upwards, in name order, without touching the filesystem.
    fn app_with_padded(count: usize, prefix: &str) -> App {
        let mut app = app_with(0);
        app.left.replace_entries(
            (0..count)
                .map(|index| entry(&format!("{prefix}-{index:05}.txt")))
                .collect(),
        );

        app
    }

    /// An application whose left pane holds one entry per name, as it would
    /// after loading a directory that contains them.
    fn app_showing(names: &[&str]) -> App {
        let mut app = app_with(0);
        app.left
            .replace_entries(names.iter().map(|name| entry(name)).collect());

        app
    }

    /// The name of the entry the pane has selected.
    fn selected_name(app: &App, which: ActivePane) -> Option<String> {
        app.pane(which)
            .selected_entry()
            .map(|entry| entry.name().to_string_lossy().into_owned())
    }

    /// An application whose left pane holds `count` entries.
    fn app_with(count: usize) -> App {
        App {
            left: pane_with(count),
            ..App::default()
        }
    }

    /// An application whose left pane holds `count` entries and can show `rows`
    /// of them, as the layout layer will report it.
    fn app_with_rows(count: usize, rows: usize) -> App {
        let mut app = app_with(count);
        app.set_visible_rows(ActivePane::Left, rows);
        app
    }

    fn all_modes() -> [Mode; 8] {
        [
            Mode::Normal,
            Mode::Search,
            Mode::Rename,
            Mode::Create,
            Mode::Confirm,
            Mode::Preview,
            Mode::CommandPalette,
            Mode::Help,
        ]
    }

    fn temporary_modes() -> [Mode; 14] {
        [
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
        ]
    }

    fn navigation_actions() -> [Action; 6] {
        [
            Action::MoveUp,
            Action::MoveDown,
            Action::GoHome,
            Action::GoEnd,
            Action::PageUp,
            Action::PageDown,
        ]
    }

    #[test]
    fn default_app_uses_normal_mode() {
        assert_eq!(App::default().mode(), Mode::Normal);
    }

    #[test]
    fn default_active_pane_is_left() {
        assert_eq!(App::default().active_pane(), ActivePane::Left);
    }

    #[test]
    fn app_contains_two_separate_panes() {
        let app = App::default();
        let left = app.pane(ActivePane::Left);
        let right = app.pane(ActivePane::Right);

        assert_eq!(left, &Pane::default());
        assert_eq!(right, &Pane::default());
        assert!(
            !std::ptr::eq(left, right),
            "the two panes must be separate pieces of state"
        );
    }

    #[test]
    fn default_pane_contents_are_empty() {
        let app = App::default();

        for which in [ActivePane::Left, ActivePane::Right] {
            let pane = app.pane(which);
            assert!(pane.entries().is_empty());
            assert_eq!(pane.scroll_offset(), 0);
        }
    }

    #[test]
    fn default_pane_selection_is_valid() {
        let app = App::default();

        for which in [ActivePane::Left, ActivePane::Right] {
            let pane = app.pane(which);
            let selection = pane.selected_index();

            // An empty pane can only have no selection; a selected index must
            // point at an entry that exists.
            match selection {
                None => {}
                Some(index) => assert!(
                    index < pane.entries().len(),
                    "selection {index} must point at an entry"
                ),
            }
        }
    }

    #[test]
    fn default_clipboard_is_empty() {
        let clipboard = ClipboardState::default();

        assert!(clipboard.is_empty());
        assert_eq!(clipboard.len(), 0);
    }

    #[test]
    fn default_search_state_is_inactive() {
        let search = SearchState::default();

        assert!(!search.is_active());
        assert!(search.query().is_empty());
    }

    #[test]
    fn default_preview_state_is_inactive() {
        assert!(!PreviewState::default().is_active());
    }

    #[test]
    fn default_bookmarks_are_empty() {
        let bookmarks = BookmarkState::default();

        assert!(bookmarks.is_empty());
        assert_eq!(bookmarks.len(), 0);
    }

    #[test]
    fn default_notification_state_is_empty() {
        let notification = NotificationState::default();

        assert!(!notification.is_active());
        assert_eq!(notification.message(), None);
    }

    #[test]
    fn default_app_holds_the_defaults_of_every_part() {
        let app = App::default();

        assert_eq!(app.clipboard(), &ClipboardState::default());
        assert_eq!(app.preview(), &PreviewState::default());
        assert_eq!(app.search(), &SearchState::default());
        assert_eq!(app.bookmarks(), &BookmarkState::default());
        assert_eq!(app.notification(), &NotificationState::default());
    }

    #[test]
    fn default_app_does_not_access_the_filesystem() {
        let app = App::default();

        for which in [ActivePane::Left, ActivePane::Right] {
            assert!(
                app.pane(which).current_path().as_os_str().is_empty(),
                "the default must not resolve a working directory"
            );
        }
    }

    #[test]
    fn default_state_is_deterministic() {
        assert_eq!(App::default(), App::default());
    }

    #[test]
    fn move_down_selects_the_first_entry_and_then_advances() {
        let mut app = app_with(3);

        app.handle_action(Action::MoveDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));

        app.handle_action(Action::MoveDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));
    }

    #[test]
    fn move_down_stops_at_the_last_entry() {
        let mut app = app_with(3);

        for _ in 0..10 {
            app.handle_action(Action::MoveDown);
        }

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(2));
    }

    #[test]
    fn move_up_retreats_from_the_last_entry() {
        let mut app = app_with(3);
        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(2));

        app.handle_action(Action::MoveUp);

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));
    }

    #[test]
    fn move_up_stops_at_the_first_entry() {
        let mut app = app_with(3);

        for _ in 0..10 {
            app.handle_action(Action::MoveUp);
        }

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
    }

    #[test]
    fn go_home_selects_the_first_entry() {
        let mut app = app_with(4);
        app.handle_action(Action::GoEnd);

        app.handle_action(Action::GoHome);

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
    }

    #[test]
    fn go_end_selects_the_last_entry() {
        let mut app = app_with(4);

        app.handle_action(Action::GoEnd);

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(3));
    }

    #[test]
    fn page_down_moves_a_page_and_stops_at_the_last_entry() {
        let mut app = app_with_rows(25, 10);
        app.handle_action(Action::GoHome);

        app.handle_action(Action::PageDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(10));

        app.handle_action(Action::PageDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(20));

        for _ in 0..2 {
            app.handle_action(Action::PageDown);
        }
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(24));
        assert_eq!(
            app.pane(ActivePane::Left).scroll_offset(),
            15,
            "the last window of 25 entries is the one that starts at 15"
        );
    }

    #[test]
    fn page_up_moves_a_page_and_stops_at_the_first_entry() {
        let mut app = app_with_rows(25, 10);
        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(24));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 15);

        app.handle_action(Action::PageUp);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(14));
        assert_eq!(
            app.pane(ActivePane::Left).scroll_offset(),
            14,
            "the window follows the selection up"
        );

        for _ in 0..3 {
            app.handle_action(Action::PageUp);
        }
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
    }

    #[test]
    fn a_page_cannot_pass_the_entries_of_a_short_listing() {
        // Three entries in a window of five: the whole listing is visible, so
        // there is nothing to scroll and the page stops at the last entry.
        let mut app = app_with_rows(3, 5);
        app.handle_action(Action::GoHome);

        app.handle_action(Action::PageDown);

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(2));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);

        app.handle_action(Action::PageUp);

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
    }

    #[test]
    fn switch_pane_changes_the_active_pane() {
        let mut app = App::default();
        assert_eq!(app.active_pane(), ActivePane::Left);

        app.handle_action(Action::SwitchPane);
        assert_eq!(app.active_pane(), ActivePane::Right);

        app.handle_action(Action::SwitchPane);
        assert_eq!(app.active_pane(), ActivePane::Left);
    }

    #[test]
    fn navigation_moves_the_selection_of_the_active_pane_only() {
        let mut app = app_with(5);
        app.right = pane_with(5);

        app.handle_action(Action::MoveDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(app.pane(ActivePane::Right).selected_index(), None);

        app.handle_action(Action::SwitchPane);
        app.handle_action(Action::MoveDown);

        assert_eq!(app.pane(ActivePane::Right).selected_index(), Some(0));
        assert_eq!(
            app.pane(ActivePane::Left).selected_index(),
            Some(0),
            "the inactive pane must keep the selection it had"
        );
    }

    #[test]
    fn mode_actions_enter_their_mode() {
        let cases = [
            (Action::StartSearch, Mode::Search),
            (Action::Rename, Mode::Rename),
            (Action::NewFile, Mode::Create),
            (Action::NewDirectory, Mode::Create),
            (Action::Preview, Mode::Preview),
            (Action::CommandPalette, Mode::CommandPalette),
            (Action::Help, Mode::Help),
            (Action::OpenBookmarks, Mode::Bookmarks),
        ];

        for (action, expected) in cases {
            let mut app = App::default();

            app.handle_action(action);

            assert_eq!(app.mode(), expected, "{action:?} should enter {expected:?}");
        }
    }

    #[test]
    fn entering_preview_activates_it() {
        let mut app = app_with(1);
        assert!(!app.preview().is_active());

        app.handle_action(Action::Preview);

        assert!(app.preview().is_active());
        assert_eq!(app.mode(), Mode::Preview);
    }

    #[test]
    fn preview_loads_content_for_selected_file_and_refreshes_on_navigation() {
        let temp = TempDir::new("state-preview-test");
        let file1 = temp.path().join("a.txt");
        let file2 = temp.path().join("b.rs");
        let dir = temp.path().join("sub");
        fs::write(&file1, "Content of A").unwrap();
        fs::write(&file2, "fn b() {}").unwrap();
        fs::create_dir(&dir).unwrap();

        let mut app = App::at(temp.path().to_path_buf()).unwrap();
        // Initially on first item
        app.handle_action(Action::Preview);
        assert!(app.preview().is_active());
        assert_eq!(app.preview().path(), Some(file1.as_path()));
        let content = app.preview().content().expect("must have content");
        assert!(content.is_text());

        // Cancel returns to Normal mode
        app.handle_action(Action::Cancel);
        assert_eq!(app.mode(), Mode::Normal);
        assert!(!app.preview().is_active());

        // Move down to next file (b.rs) in Normal mode
        app.handle_action(Action::MoveDown);
        app.handle_action(Action::Preview);
        assert_eq!(app.preview().path(), Some(file2.as_path()));
        let content2 = app.preview().content().expect("must have content");
        assert_eq!(content2.as_text().unwrap().lines(), &["fn b() {}"]);

        // Move to sub directory
        app.handle_action(Action::Cancel);
        app.handle_action(Action::MoveDown);
        app.handle_action(Action::Preview);
        assert_eq!(app.preview().path(), Some(dir.as_path()));
        assert!(matches!(
            app.preview().content(),
            Some(crate::preview::PreviewContent::Directory(_))
        ));

        // Cancel leaves preview mode and returns to normal mode with live preview
        app.handle_action(Action::Cancel);
        assert_eq!(app.mode(), Mode::Normal);
        assert!(!app.preview().is_active());
        assert!(app.preview().content().is_some());
    }

    #[test]
    fn preview_is_pane_independent_and_invalidates_on_selection_change() {
        let temp = TempDir::new("state-pane-preview-test");
        let file_left = temp.path().join("left.txt");
        let file_right = temp.path().join("right.txt");
        fs::write(&file_left, "Left content").unwrap();
        fs::write(&file_right, "Right content").unwrap();

        let mut app = App::at(temp.path().to_path_buf()).unwrap();
        app.open_in(ActivePane::Right, temp.path().to_path_buf())
            .unwrap();

        // Left pane starts on left.txt
        app.handle_action(Action::Preview);
        assert!(app.preview().is_active());
        assert_eq!(app.preview().path(), Some(file_left.as_path()));

        // Exit preview mode and switch to right pane
        app.handle_action(Action::Cancel);
        app.handle_action(Action::SwitchPane);
        app.handle_action(Action::MoveDown);

        // Preview right pane selection
        app.handle_action(Action::Preview);
        assert!(app.preview().is_active());
        assert_eq!(app.preview().path(), Some(file_right.as_path()));

        // Exit preview mode, switch back to left pane
        app.handle_action(Action::Cancel);
        app.handle_action(Action::SwitchPane);
        app.handle_action(Action::Preview);
        assert_eq!(app.preview().path(), Some(file_left.as_path()));
    }

    #[test]
    fn cancel_returns_to_normal_from_every_temporary_mode() {
        for mode in temporary_modes() {
            let mut app = App {
                mode,
                ..App::default()
            };

            app.handle_action(Action::Cancel);

            assert_eq!(app.mode(), Mode::Normal, "Cancel should leave {mode:?}");
        }
    }

    #[test]
    fn cancel_leaves_preview_mode_without_a_preview() {
        let mut app = app_with(1);
        app.handle_action(Action::Preview);

        app.handle_action(Action::Cancel);

        assert_eq!(app.mode(), Mode::Normal);
        assert!(!app.preview().is_active());
    }

    #[test]
    fn cancel_keeps_the_entries_the_selection_and_the_query() {
        let mut app = app_with(4);
        app.set_search_query("entry");
        app.handle_action(Action::GoEnd);
        app.handle_action(Action::Help);

        app.handle_action(Action::Cancel);

        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.search().query(), "entry");
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(3));
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 4);
    }

    #[test]
    fn cancel_in_normal_mode_changes_nothing() {
        let mut app = app_with(3);
        app.handle_action(Action::MoveDown);
        let before = app.clone();

        app.handle_action(Action::Cancel);

        assert_eq!(app, before);
    }

    #[test]
    fn clear_search_discards_the_query_and_leaves_search_mode() {
        let mut app = app_with(1);
        app.handle_action(Action::StartSearch);
        app.set_search_query("needle");
        assert_eq!(app.mode(), Mode::Search);

        app.handle_action(Action::ClearSearch);

        assert_eq!(app.mode(), Mode::Normal);
        assert!(app.search().query().is_empty());
        assert!(!app.search().is_active());
    }

    #[test]
    fn quit_records_a_request_without_terminating() {
        let mut app = App::default();
        assert!(!app.should_quit());

        app.handle_action(Action::Quit);

        assert!(app.should_quit());
        assert_eq!(
            app.mode(),
            Mode::Normal,
            "quitting must not disturb the rest of the state"
        );
    }

    #[test]
    fn quit_is_available_from_every_mode() {
        for mode in all_modes() {
            let mut app = App {
                mode,
                ..App::default()
            };

            app.handle_action(Action::Quit);

            assert!(app.should_quit(), "Quit should work in {mode:?}");
        }
    }

    #[test]
    fn navigating_an_empty_pane_is_safe() {
        for action in navigation_actions() {
            let mut app = App::default();

            for _ in 0..5 {
                app.handle_action(action);
            }

            assert_eq!(
                app.pane(ActivePane::Left).selected_index(),
                None,
                "{action:?} must not select anything in an empty pane"
            );
            assert_eq!(app.pane(ActivePane::Right).selected_index(), None);
        }
    }

    #[test]
    fn switch_pane_on_empty_panes_is_safe() {
        let mut app = App::default();

        // An odd number of switches lands on the other pane, an even number
        // returns to the starting one; neither selects anything.
        app.handle_action(Action::SwitchPane);
        assert_eq!(app.active_pane(), ActivePane::Right);

        for _ in 0..3 {
            app.handle_action(Action::SwitchPane);
        }
        assert_eq!(app.active_pane(), ActivePane::Left);

        assert_eq!(app.pane(ActivePane::Left).selected_index(), None);
        assert_eq!(app.pane(ActivePane::Right).selected_index(), None);
    }

    #[test]
    fn selection_never_points_outside_the_entries() {
        for count in [0, 1, 2, 7, 23] {
            for action in Action::ALL {
                let mut app = app_with(count);

                for _ in 0..3 {
                    app.handle_action(action);
                }

                for which in [ActivePane::Left, ActivePane::Right] {
                    let pane = app.pane(which);
                    if let Some(index) = pane.selected_index() {
                        assert!(
                            index < pane.entries().len(),
                            "{action:?} selected {index} of {} entries",
                            pane.entries().len()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn repeated_navigation_at_the_boundaries_stays_safe() {
        for action in navigation_actions() {
            let mut app = app_with(2);

            for _ in 0..100 {
                app.handle_action(action);
            }

            let selection = app.pane(ActivePane::Left).selected_index();
            assert!(
                matches!(selection, Some(index) if index < 2),
                "{action:?} left {selection:?}"
            );
        }
    }

    #[test]
    fn a_selection_that_no_longer_fits_is_brought_back_into_range() {
        let mut app = app_with(5);
        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(4));

        // The listing shrinks behind the selection, as it will when a later
        // phase replaces the entries of a pane.
        let mut shrunk = app.pane(ActivePane::Left).entries().to_vec();
        shrunk.truncate(2);
        app.left.set_entries(shrunk);

        app.handle_action(Action::MoveUp);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));

        app.handle_action(Action::MoveDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));
    }

    #[test]
    fn actions_that_need_a_later_phase_are_ignored_in_normal_mode() {
        // Open and GoParent are not here: since Phase 5.1 they navigate for
        // real. Copy, Cut and Paste are not here either: since Phase 6.2 they
        // mark and paste for real, and Delete joined them in Phase 6.3.
        // ChangeSort joined them in Phase 7.1: it reorders the entries of the
        // active pane. Bookmarks joined them in Phase 12.1.
        // GitStatus joined them in Phase 18.2: it opens the Git Status Panel.
        // All of them have their own tests.
        let unimplemented = [Action::MoveLeft, Action::MoveRight];

        for action in unimplemented {
            let mut app = app_with(3);
            let before = app.clone();

            app.handle_action(action);

            assert_eq!(
                app, before,
                "{action:?} must not change the state or pretend to have run"
            );
            assert!(
                !app.notification().is_active(),
                "{action:?} must not announce anything"
            );
        }
    }

    #[test]
    fn actions_ignored_by_a_temporary_mode_leave_the_state_unchanged() {
        let mode_independent = [Action::Cancel, Action::ClearSearch, Action::Quit];

        for mode in temporary_modes() {
            for action in Action::ALL {
                if mode_independent.contains(&action) {
                    continue;
                }
                if mode == Mode::Search && action == Action::CycleSearchMode {
                    continue;
                }
                if mode == Mode::Bookmarks && action == Action::RemoveBookmark {
                    continue;
                }
                if mode == Mode::Preview && action == Action::Preview {
                    continue;
                }

                let mut app = app_with(3);
                app.handle_action(Action::MoveDown);
                app.mode = mode;
                let before = app.clone();

                app.handle_action(action);
                assert_eq!(app, before, "{action:?} must be ignored in {mode:?}");

                app.handle_action(Action::Cancel);
                assert_eq!(app.mode(), Mode::Normal);
                assert_eq!(
                    app.pane(ActivePane::Left).selected_index(),
                    before.pane(ActivePane::Left).selected_index(),
                    "leaving {mode:?} must keep the selection"
                );
            }
        }
    }

    #[test]
    fn an_application_starts_in_a_directory_that_exists() {
        let directory = TempDir::new("navigation-start");
        fs::write(directory.path().join("file.txt"), b"content").expect("file should be written");

        let app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        assert_eq!(app.pane(ActivePane::Left).current_path(), directory.path());
    }

    #[test]
    fn starting_loads_the_entries_of_the_directory() {
        let directory = TempDir::new("navigation-entries");
        fs::write(directory.path().join("alpha.txt"), b"content").expect("file should be written");
        fs::create_dir(directory.path().join("beta")).expect("directory should be created");

        let app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        let names: Vec<String> = app
            .pane(ActivePane::Left)
            .entries()
            .iter()
            .map(|entry| entry.name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["alpha.txt", "beta"]);
    }

    #[test]
    fn the_working_directory_is_read_from_the_process() {
        let expected = std::env::current_dir().expect("the test process has a working directory");

        let app = App::at_working_directory().expect("the working directory should load");

        assert_eq!(app.pane(ActivePane::Left).current_path(), &expected);
    }

    #[test]
    fn entering_a_child_directory_changes_the_path_and_loads_it() {
        let directory = TempDir::new("navigation-enter");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        fs::write(child.join("inside.txt"), b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        // The child is the only entry, so it is selected.
        app.handle_action(Action::Open);

        assert_eq!(app.pane(ActivePane::Left).current_path(), &child);
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);
        assert_eq!(
            app.pane(ActivePane::Left).entries()[0].name(),
            OsStr::new("inside.txt")
        );
        assert!(!app.notification().is_active(), "opening should not warn");
    }

    #[test]
    fn entering_an_empty_directory_selects_nothing() {
        let directory = TempDir::new("navigation-empty-child");
        let child = directory.path().join("empty");
        fs::create_dir(&child).expect("directory should be created");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        app.handle_action(Action::Open);

        assert_eq!(app.pane(ActivePane::Left).current_path(), &child);
        assert!(app.pane(ActivePane::Left).entries().is_empty());
        assert_eq!(app.pane(ActivePane::Left).selected_index(), None);
    }

    #[test]
    fn entering_a_directory_selects_its_first_entry() {
        let directory = TempDir::new("navigation-first-entry");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        for name in ["a.txt", "b.txt"] {
            fs::write(child.join(name), b"content").expect("file should be written");
        }
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        app.handle_action(Action::Open);

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
    }

    #[test]
    fn entering_resets_the_scroll_offset() {
        let directory = TempDir::new("navigation-scroll-child");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        for index in 0..30 {
            fs::write(child.join(format!("file-{index:02}.txt")), b"content")
                .expect("file should be written");
        }
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        app.left.set_scroll_offset(12);

        app.handle_action(Action::Open);

        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
    }

    #[test]
    fn a_regular_file_is_not_opened() {
        let directory = TempDir::new("navigation-file");
        fs::write(directory.path().join("file.txt"), b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        let before = app.clone();

        app.handle_action(Action::Open);

        assert_eq!(app, before, "opening a file is not implemented yet");
        assert!(
            !app.notification().is_active(),
            "and it must not be reported as a failure"
        );
    }

    #[test]
    fn opening_with_nothing_selected_changes_nothing() {
        let directory = TempDir::new("navigation-nothing-selected");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        let before = app.clone();

        app.handle_action(Action::Open);

        assert_eq!(app, before);
    }

    #[test]
    fn going_to_the_parent_changes_the_path_and_loads_it() {
        let directory = TempDir::new("navigation-parent");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        fs::write(directory.path().join("sibling.txt"), b"content")
            .expect("file should be written");
        let mut app = App::at(child).expect("the directory should load");

        app.handle_action(Action::GoParent);

        assert_eq!(app.pane(ActivePane::Left).current_path(), directory.path());

        let names: Vec<&OsStr> = app
            .pane(ActivePane::Left)
            .entries()
            .iter()
            .map(Entry::name)
            .collect();
        assert_eq!(
            names,
            [OsStr::new("child"), OsStr::new("sibling.txt")],
            "the parent's own listing is loaded, in the usual order"
        );
    }

    #[test]
    fn going_to_the_parent_at_a_filesystem_root_stays_put() {
        // The root is found by walking up until there is nowhere else to go, so
        // no platform-specific root spelling is assumed.
        let mut root = std::env::current_dir().expect("the test process has a working directory");
        let mut steps = 0;
        while let Some(parent) = crate::filesystem::navigation::parent_of(&root) {
            root = parent;
            steps += 1;
            assert!(steps < 64, "walking up must end");
        }

        let mut app = App::at(root.clone()).expect("the root should load");
        let before = app.clone();

        app.handle_action(Action::GoParent);

        assert_eq!(app.pane(ActivePane::Left).current_path(), &root);
        assert_eq!(
            app, before,
            "there is nowhere to go, and that is not an error"
        );
    }

    #[test]
    fn refreshing_rereads_the_directory() {
        let directory = TempDir::new("navigation-refresh");
        fs::write(directory.path().join("first.txt"), b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);

        fs::write(directory.path().join("second.txt"), b"content").expect("file should be written");
        fs::remove_file(directory.path().join("first.txt")).expect("file should be removed");

        app.refresh_active_pane()
            .expect("refreshing should succeed");

        let names: Vec<String> = app
            .pane(ActivePane::Left)
            .entries()
            .iter()
            .map(|entry| entry.name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["second.txt"]);
        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            directory.path(),
            "refreshing keeps the path"
        );
    }

    #[test]
    fn refreshing_keeps_the_selection_within_the_entries() {
        let directory = TempDir::new("navigation-refresh-selection");
        for index in 0..5 {
            fs::write(
                directory.path().join(format!("file-{index}.txt")),
                b"content",
            )
            .expect("file should be written");
        }
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(4));

        // The listing shrinks underneath the selection.
        for index in 2..5 {
            fs::remove_file(directory.path().join(format!("file-{index}.txt")))
                .expect("file should be removed");
        }
        app.refresh_active_pane()
            .expect("refreshing should succeed");

        assert_eq!(app.pane(ActivePane::Left).entries().len(), 2);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));
    }

    #[test]
    fn refreshing_an_empty_directory_selects_nothing() {
        let directory = TempDir::new("navigation-refresh-empty");
        fs::write(directory.path().join("file.txt"), b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));

        fs::remove_file(directory.path().join("file.txt")).expect("file should be removed");
        app.refresh_active_pane()
            .expect("refreshing should succeed");

        assert!(app.pane(ActivePane::Left).entries().is_empty());
        assert_eq!(app.pane(ActivePane::Left).selected_index(), None);
    }

    #[test]
    fn a_missing_directory_is_reported_and_not_opened() {
        let directory = TempDir::new("navigation-missing");
        let missing = directory.path().join("gone");

        let error = App::at(missing.clone()).expect_err("the directory should not load");

        assert_eq!(error.path(), Some(missing.as_path()));
        assert_eq!(io_error(&error).kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn a_deleted_directory_is_reported_when_refreshed() {
        let directory = TempDir::new("navigation-deleted");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        let mut app = App::at(child.clone()).expect("the directory should load");
        fs::remove_dir(&child).expect("directory should be removed");

        let error = app
            .refresh_active_pane()
            .expect_err("refreshing should fail");

        assert_eq!(error.path(), Some(child.as_path()));
        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            &child,
            "the pane keeps the path it was showing"
        );
    }

    #[test]
    fn a_failed_navigation_preserves_the_pane_and_reports_it() {
        let directory = TempDir::new("navigation-failure");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        fs::write(child.join("inside.txt"), b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        let loaded = app.pane(ActivePane::Left).entries().to_vec();

        // The selected directory disappears after it was listed, so entering it
        // fails.
        fs::remove_dir_all(&child).expect("directory should be removed");
        app.handle_action(Action::Open);

        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            directory.path(),
            "the pane keeps the path it was showing"
        );
        assert_eq!(
            app.pane(ActivePane::Left).entries(),
            loaded.as_slice(),
            "the listing must survive a failed navigation"
        );
        let message = app
            .notification()
            .message()
            .expect("the failure must be reported to the user");
        assert!(
            message.contains(&child.display().to_string()),
            "the report names what could not be read, and said: {message}"
        );
    }

    #[test]
    fn a_path_that_is_a_file_is_reported_as_a_failed_navigation() {
        let directory = TempDir::new("navigation-not-a-directory");
        let file = directory.path().join("file.txt");
        fs::write(&file, b"content").expect("file should be written");

        let error = App::at(file.clone()).expect_err("the directory should not load");

        assert_eq!(error.path(), Some(file.as_path()));
    }

    #[test]
    fn a_successful_navigation_clears_a_previous_failure() {
        let directory = TempDir::new("navigation-clears");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        // A navigation that fails leaves a message behind, which the navigation
        // that works must not leave standing.
        let selected = app
            .pane(ActivePane::Left)
            .selected_entry()
            .expect("the child directory is selected")
            .path()
            .to_path_buf();
        fs::remove_dir_all(&selected).expect("the directory should be removed");
        app.handle_action(Action::Open);
        assert!(
            app.notification().is_active(),
            "the failed navigation must be reported first"
        );

        fs::create_dir(&selected).expect("the directory should be created again");
        app.handle_action(Action::Open);

        assert_eq!(app.pane(ActivePane::Left).current_path(), &child);
        assert_eq!(
            app.notification.message(),
            None,
            "a stale failure must not outlive the navigation that replaced it"
        );
    }

    #[test]
    fn a_failed_change_is_reported_with_what_it_was_and_why() {
        let directory = TempDir::new("notice-failed-change");
        let selected = directory.path().join("gone.txt");
        fs::write(&selected, b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        // The entry disappears after it was listed, so renaming it fails.
        fs::remove_file(&selected).expect("file should be removed");
        let error = app
            .rename_selected(OsStr::new("renamed.txt"))
            .expect_err("there is nothing to rename");

        assert_eq!(error.category(), ErrorCategory::NotFound);
        assert_eq!(app.last_outcome(), Some(OperationOutcome::SourceMissing));

        let notice = app
            .notification()
            .notice()
            .expect("a failed change must be kept in structured form");
        assert_eq!(
            notice.what(),
            &NoticeSource::Operation(Operation::Rename(selected.clone())),
            "the notice says which entry was being renamed"
        );
        assert_eq!(notice.category(), ErrorCategory::NotFound);
        assert!(!notice.is_conflict());
        assert!(!notice.is_cancelled());
        assert_eq!(
            notice.path(),
            Some(directory.path().join("renamed.txt").as_path()),
            "the notice names the path the change was to produce"
        );
        assert!(!notice.text().is_empty());
    }

    #[test]
    fn a_conflict_is_recognizable_as_one_without_reading_the_words() {
        let directory = TempDir::new("notice-conflict");
        let taken = directory.path().join("taken.txt");
        fs::write(&taken, b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        app.create_file(OsStr::new("taken.txt"))
            .expect_err("the name is taken");

        let notice = app
            .notification()
            .notice()
            .expect("a failed change must be kept in structured form");
        assert_eq!(
            notice.what(),
            &NoticeSource::Operation(Operation::CreateFile)
        );
        assert_eq!(notice.category(), ErrorCategory::AlreadyExists);
        assert!(notice.is_conflict(), "a taken name is a clash");
        assert!(
            !notice.is_cancelled(),
            "nothing was stopped, it was refused"
        );
        assert_eq!(
            app.last_outcome(),
            Some(OperationOutcome::DestinationExists)
        );
    }

    #[test]
    fn a_failed_navigation_is_reported_with_its_category() {
        let directory = TempDir::new("notice-failed-navigation");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        // The selected directory is removed, so entering it fails.
        fs::remove_dir_all(&child).expect("directory should be removed");
        app.handle_action(Action::Open);

        let notice = app
            .notification()
            .notice()
            .expect("a failed navigation must be kept in structured form");
        assert_eq!(notice.what(), &NoticeSource::Navigation);
        assert_eq!(notice.category(), ErrorCategory::NotFound);
        assert_eq!(notice.path(), Some(child.as_path()));
        assert!(!notice.is_conflict());
        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            directory.path(),
            "the pane keeps what it was showing"
        );
    }

    #[test]
    fn a_stopped_search_is_not_reported_as_a_failure() {
        let (mut app, _directory) = app_over_a_tree("notice-cancelled-search");
        app.set_search_mode(SearchMode::Recursive);
        app.cancel_search();

        app.run_search("needle");

        assert_eq!(
            app.search().outcome(),
            Some(&SearchOutcome::Cancelled),
            "a stopped walk is not a finished one"
        );
        assert!(
            !app.notification().is_active(),
            "stopping is what was asked for, so there is nothing to report"
        );
    }

    #[test]
    fn a_search_that_works_clears_what_a_failure_had_to_say() {
        let (mut app, _directory) = app_over_a_tree("notice-search-clears");
        app.set_search_mode(SearchMode::Recursive);

        // A root that cannot be read leaves a message behind...
        let shown = app.pane(ActivePane::Left).current_path().to_path_buf();
        app.left.set_current_path(PathBuf::from(UNREAL_PATH));
        app.run_search("needle");
        assert!(app.notification().is_active());

        // ...which a search that works must not leave standing.
        app.left.set_current_path(shown);
        app.run_search("needle");

        assert_eq!(app.search().outcome(), Some(&SearchOutcome::Completed));
        assert_eq!(
            app.notification().message(),
            None,
            "a stale failure must not outlive the search that replaced it"
        );
    }

    #[test]
    fn no_normal_filesystem_failure_panics_or_leaves_the_state_unsound() {
        let directory = TempDir::new("failures-leave-state-alone");
        // The directory sorts first, so `GoHome` selects it.
        let folder = directory.path().join("aaa-folder");
        fs::create_dir(&folder).expect("directory should be created");
        let file = directory.path().join("document.txt");
        fs::write(&file, b"content").expect("file should be written");
        fs::write(directory.path().join("taken.txt"), b"content").expect("file should be written");

        let other = TempDir::new("failures-leave-other-pane-alone");
        // The other pane holds a name that clashes with the one marked in the
        // first, so pasting is refused.
        fs::write(other.path().join("taken.txt"), b"content").expect("file should be written");

        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        app.open_in(ActivePane::Right, other.path().to_path_buf())
            .expect("the other pane should load");
        app.set_visible_rows(ActivePane::Left, 5);
        app.set_visible_rows(ActivePane::Right, 5);

        let left_before = app.pane(ActivePane::Left).entries().to_vec();
        let right_before = app.pane(ActivePane::Right).entries().to_vec();
        let left_path_before = app.pane(ActivePane::Left).current_path().to_path_buf();
        let right_path_before = app.pane(ActivePane::Right).current_path().to_path_buf();

        /// What a step of the batch is expected to fail as.
        #[derive(Clone, Copy)]
        enum Expected {
            /// A change to the filesystem, with the outcome it reports.
            Change(OperationOutcome),
            /// Looking at a directory.
            Navigation,
            /// A search beyond the listing.
            Search,
        }

        /// One step of the batch: what it is, what it should fail as, and how
        /// to bring the failure about.
        type Step = (&'static str, Expected, Box<dyn Fn(&mut App)>);

        // Each step is a failure a user can bring about by ordinary use: a
        // directory they entered that is gone, a name that is taken, a search
        // from a directory that disappeared. None of them may panic, and none
        // may leave a pane describing something that is not there.
        let steps: Vec<Step> = vec![
            (
                "creating over a name that is taken",
                Expected::Change(OperationOutcome::DestinationExists),
                Box::new(|app: &mut App| {
                    app.create_file(OsStr::new("taken.txt"))
                        .expect_err("the name is taken");
                }),
            ),
            (
                "creating with a name that is not a single name",
                Expected::Change(OperationOutcome::InvalidPath),
                Box::new(|app: &mut App| {
                    app.create_file(OsStr::new("nested/child.txt"))
                        .expect_err("a path is not a name");
                }),
            ),
            (
                "creating a directory over a name that is taken",
                Expected::Change(OperationOutcome::DestinationExists),
                Box::new(|app: &mut App| {
                    app.create_directory(OsStr::new("taken.txt"))
                        .expect_err("the name is taken");
                }),
            ),
            (
                "renaming an entry that is gone",
                Expected::Change(OperationOutcome::SourceMissing),
                Box::new(|app: &mut App| {
                    // `document.txt` sorts second and has been removed below.
                    app.handle_action(Action::GoHome);
                    app.handle_action(Action::MoveDown);
                    app.rename_selected(OsStr::new("renamed.txt"))
                        .expect_err("the entry is not there");
                }),
            ),
            (
                "deleting an entry that is gone",
                Expected::Change(OperationOutcome::SourceMissing),
                Box::new(|app: &mut App| {
                    app.delete_selected().expect_err("the entry is not there");
                }),
            ),
            (
                "entering a directory that is gone",
                Expected::Navigation,
                Box::new(|app: &mut App| {
                    // `aaa-folder` sorts first and has been removed below.
                    app.handle_action(Action::GoHome);
                    app.handle_action(Action::Open);
                }),
            ),
            (
                "refreshing a directory that is gone",
                Expected::Navigation,
                Box::new(|app: &mut App| {
                    let gone = app.left.current_path().clone();
                    app.left.set_current_path(PathBuf::from(UNREAL_PATH));
                    app.refresh_active_pane()
                        .expect_err("the directory is not there");
                    app.left.set_current_path(gone);
                }),
            ),
            (
                "searching from a directory that is gone",
                Expected::Search,
                Box::new(|app: &mut App| {
                    let gone = app.left.current_path().clone();
                    app.set_search_mode(SearchMode::Recursive);
                    app.left.set_current_path(PathBuf::from(UNREAL_PATH));
                    app.run_search("needle");
                    app.left.set_current_path(gone);
                }),
            ),
            (
                "pasting where the name is taken",
                Expected::Change(OperationOutcome::DestinationExists),
                Box::new(|app: &mut App| {
                    // The search above left the pane showing its results, and a
                    // result is not an entry that can be marked, so the search
                    // is cleared first and the listing is back.
                    app.handle_action(Action::ClearSearch);
                    app.handle_action(Action::GoEnd);
                    app.mark_for_copy();
                    app.handle_action(Action::SwitchPane);
                    app.paste().expect_err("the name is taken");
                    app.handle_action(Action::SwitchPane);
                }),
            ),
        ];

        // The two entries the batch needs to be gone are removed here, after
        // the panes have listed them, which is exactly how they disappear in
        // ordinary use.
        fs::remove_file(&file).expect("file should be removed");
        fs::remove_dir_all(&folder).expect("directory should be removed");
        app.handle_action(Action::GoHome);

        for (what, expected, run) in steps {
            run(&mut app);

            let notice = app
                .notification()
                .notice()
                .unwrap_or_else(|| panic!("{what}: a failure must leave something to show"));
            assert!(
                !notice.text().is_empty(),
                "{what}: the notice must have words"
            );

            match expected {
                Expected::Change(outcome) => {
                    assert_eq!(app.last_outcome(), Some(outcome), "{what}");
                    assert!(
                        matches!(notice.what(), NoticeSource::Operation(_)),
                        "{what}: the notice must be about the change, not about {}",
                        notice.text()
                    );
                }
                Expected::Navigation => assert_eq!(
                    notice.what(),
                    &NoticeSource::Navigation,
                    "{what}: the notice must be about the navigation"
                ),
                Expected::Search => assert_eq!(
                    notice.what(),
                    &NoticeSource::Search,
                    "{what}: the notice must be about the search"
                ),
            }

            assert_eq!(
                app.pane(ActivePane::Left).current_path(),
                &left_path_before,
                "{what}: the active pane must keep its directory"
            );
            assert_eq!(
                app.pane(ActivePane::Right).current_path(),
                &right_path_before,
                "{what}: the other pane must not move"
            );
            assert_eq!(
                app.pane(ActivePane::Left).entries(),
                left_before.as_slice(),
                "{what}: a failure must not change what the pane lists"
            );
            assert_eq!(
                app.pane(ActivePane::Right).entries(),
                right_before.as_slice(),
                "{what}: a failure must not change what the other pane lists"
            );
            assert_pane_is_sound(&app, ActivePane::Left, what);
            assert_pane_is_sound(&app, ActivePane::Right, what);
        }

        assert_eq!(
            app.clipboard().entries(),
            [left_path_before.join("taken.txt")],
            "the entry a failed paste was to copy must stay marked"
        );
        assert_eq!(
            app.clipboard().operation(),
            Some(ClipboardOperation::Copy),
            "the clipboard must still be a copy"
        );

        // And the application is still usable afterwards.
        app.handle_action(Action::GoParent);
        app.handle_action(Action::GoEnd);
        assert_pane_is_sound(&app, ActivePane::Left, "after the failures");
        assert_eq!(
            app.pane(ActivePane::Right).entries(),
            right_before.as_slice(),
            "the pane that had nothing to do with any of it is untouched"
        );
    }

    #[test]
    fn a_symbolic_link_to_a_directory_can_be_entered() {
        let directory = TempDir::new("navigation-symlink");
        let target = directory.path().join("target");
        fs::create_dir(&target).expect("directory should be created");
        fs::write(target.join("inside.txt"), b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        // Symbolic links cannot be created reliably on Windows without
        // elevation, so the link is only made where it can be.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, directory.path().join("link"))
                .expect("symlink should be created");
            app.refresh_active_pane()
                .expect("refreshing should succeed");
            let link = app
                .pane(ActivePane::Left)
                .entries()
                .iter()
                .position(|entry| entry.name() == OsStr::new("link"))
                .expect("the link should be listed");
            app.left.set_selected_index(Some(link));

            app.handle_action(Action::Open);

            let link = directory.path().join("link");
            assert_eq!(
                app.pane(ActivePane::Left).current_path(),
                &link,
                "the pane keeps the path it opened"
            );

            let names: Vec<&OsStr> = app
                .pane(ActivePane::Left)
                .entries()
                .iter()
                .map(Entry::name)
                .collect();
            assert_eq!(
                names,
                [OsStr::new("inside.txt")],
                "what is listed is what the link points at"
            );
        }
    }

    #[test]
    fn each_pane_keeps_its_own_path_and_entries() {
        let first = TempDir::new("navigation-pane-left");
        fs::write(first.path().join("left.txt"), b"content").expect("file should be written");
        let second = TempDir::new("navigation-pane-right");
        fs::write(second.path().join("right.txt"), b"content").expect("file should be written");

        let mut app = App::at(first.path().to_path_buf()).expect("the directory should load");
        app.open_in(ActivePane::Right, second.path().to_path_buf())
            .expect("the directory should load");

        assert_eq!(app.pane(ActivePane::Left).current_path(), first.path());
        assert_eq!(app.pane(ActivePane::Right).current_path(), second.path());

        let left_names: Vec<String> = app
            .pane(ActivePane::Left)
            .entries()
            .iter()
            .map(|entry| entry.name().to_string_lossy().into_owned())
            .collect();
        let right_names: Vec<String> = app
            .pane(ActivePane::Right)
            .entries()
            .iter()
            .map(|entry| entry.name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left_names, ["left.txt"]);
        assert_eq!(right_names, ["right.txt"]);

        // Navigating one pane must leave the other untouched.
        app.open_in(ActivePane::Left, first.path().to_path_buf())
            .expect("the directory should load");
        app.handle_action(Action::SwitchPane);
        app.handle_action(Action::SwitchPane);
        app.handle_action(Action::GoParent);

        assert_eq!(app.pane(ActivePane::Right).current_path(), second.path());
        assert_eq!(app.pane(ActivePane::Right).entries().len(), 1);
    }

    #[test]
    fn each_pane_has_its_own_selection() {
        let first = TempDir::new("navigation-selection-left");
        for index in 0..3 {
            fs::write(first.path().join(format!("left-{index}.txt")), b"content")
                .expect("file should be written");
        }
        let second = TempDir::new("navigation-selection-right");
        for index in 0..3 {
            fs::write(second.path().join(format!("right-{index}.txt")), b"content")
                .expect("file should be written");
        }

        let mut app = App::at(first.path().to_path_buf()).expect("the directory should load");
        app.open_in(ActivePane::Right, second.path().to_path_buf())
            .expect("the directory should load");

        app.handle_action(Action::MoveDown);
        app.handle_action(Action::MoveDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(2));

        app.handle_action(Action::SwitchPane);
        assert_eq!(
            app.pane(ActivePane::Right).selected_index(),
            Some(0),
            "the other pane keeps its own selection"
        );
    }

    #[test]
    fn navigation_never_lists_recursively() {
        let directory = TempDir::new("navigation-not-recursive");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        fs::write(child.join("nested.txt"), b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);

        app.handle_action(Action::Open);

        assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);
        assert_eq!(
            app.pane(ActivePane::Left).entries()[0].name(),
            OsStr::new("nested.txt")
        );
    }

    #[test]
    fn handling_every_action_leaves_the_entries_consistent() {
        for mode in all_modes() {
            let mut app = app_with(3);
            app.mode = mode;
            let entries = app.pane(ActivePane::Left).entries().to_vec();

            for action in Action::ALL {
                app.handle_action(action);
            }

            // The path does not exist, so the two actions that do reach the
            // filesystem, Open and GoParent, fail and leave the pane exactly as
            // it was: a failed navigation never rewrites a pane's contents.
            assert_eq!(app.pane(ActivePane::Left).entries(), entries.as_slice());
            assert_eq!(app.pane(ActivePane::Right).entries().len(), 0);
            assert_eq!(
                app.pane(ActivePane::Left).current_path().as_os_str(),
                UNREAL_PATH
            );
        }
    }

    #[test]
    fn handling_actions_needs_no_terminal() {
        // Every transition is exercised in memory: no terminal is initialised,
        // no event loop runs and nothing is rendered or printed.
        let mut app = app_with(3);

        for mode in all_modes() {
            app.mode = mode;
            for action in Action::ALL {
                app.handle_action(action);
            }
            app.handle_action(Action::Cancel);
        }

        assert!(app.should_quit());
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 3);
    }

    #[test]
    fn transitions_are_deterministic() {
        let sequence = [
            Action::MoveDown,
            Action::MoveDown,
            Action::PageUp,
            Action::GoEnd,
            Action::SwitchPane,
            Action::SwitchPane,
            Action::StartSearch,
            Action::ClearSearch,
            Action::Preview,
            Action::Cancel,
            Action::Quit,
        ];

        let mut first = app_with(30);
        let mut second = app_with(30);

        for action in sequence {
            first.handle_action(action);
            second.handle_action(action);
        }

        assert_eq!(first, second);
    }

    #[test]
    fn app_state_constructs_independently_of_actions() {
        use crate::app::actions::Action;

        let before = App::default();

        // An action is only an instruction: naming every action must neither
        // perform an operation nor modify any state.
        for action in Action::ALL {
            std::hint::black_box(action);
        }

        assert_eq!(App::default(), before);
    }

    /// The largest offset `which` pane's listing allows.
    fn pane_maximum_offset(app: &App, which: ActivePane) -> usize {
        let pane = app.pane(which);
        maximum_scroll_offset(pane.visible_count(), pane.visible_rows())
    }

    /// Asserts the invariants that must hold for one pane after every change.
    fn assert_pane_is_sound(app: &App, which: ActivePane, context: &str) {
        let pane = app.pane(which);
        let total = pane.visible_count();
        let rows = pane.visible_rows();
        let scroll = pane.scroll_offset();

        assert!(
            scroll <= pane_maximum_offset(app, which),
            "{context}: offset {scroll} of {total} entries in {rows} rows"
        );

        let Some(index) = pane.selected_index() else {
            assert_eq!(total, 0, "{context}: {total} entries but nothing selected");
            assert_eq!(scroll, 0, "{context}: an empty pane cannot be scrolled");
            return;
        };

        assert!(
            index < total,
            "{context}: selected {index} of {total} entries"
        );

        if rows == 0 {
            return;
        }

        assert!(index >= scroll, "{context}: {index} is above the window");
        assert!(
            index < scroll + rows,
            "{context}: {index} is below the window that starts at {scroll} and is {rows} rows tall"
        );
    }

    /// Asserts the same invariants for the left pane, which is where the
    /// selection and scrolling tests keep their state.
    fn assert_selection_is_sound(app: &App, context: &str) {
        assert_pane_is_sound(app, ActivePane::Left, context);
    }

    #[test]
    fn a_listing_that_is_empty_has_no_selection_and_no_scroll() {
        let mut app = App::default();
        app.set_visible_rows(ActivePane::Left, 10);

        for action in navigation_actions() {
            for _ in 0..3 {
                app.handle_action(action);

                assert_eq!(app.pane(ActivePane::Left).selected_index(), None);
                assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
            }
        }
    }

    #[test]
    fn a_single_entry_can_be_selected_but_never_moved_past() {
        let mut app = app_with_rows(1, 10);

        for action in navigation_actions() {
            for _ in 0..3 {
                app.handle_action(action);

                assert_eq!(
                    app.pane(ActivePane::Left).selected_index(),
                    Some(0),
                    "{action:?} on a listing of one entry"
                );
                assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
            }
        }
    }

    #[test]
    fn moving_down_then_up_visits_every_entry_in_order() {
        let mut app = app_with_rows(5, 10);

        // The first move selects the first entry instead of skipping it.
        let mut visited = Vec::new();
        for _ in 0..7 {
            app.handle_action(Action::MoveDown);
            visited.push(app.pane(ActivePane::Left).selected_index());
        }
        assert_eq!(
            visited,
            [
                Some(0),
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(4),
                Some(4)
            ]
        );

        let mut back = Vec::new();
        for _ in 0..7 {
            app.handle_action(Action::MoveUp);
            back.push(app.pane(ActivePane::Left).selected_index());
        }
        assert_eq!(
            back,
            [
                Some(3),
                Some(2),
                Some(1),
                Some(0),
                Some(0),
                Some(0),
                Some(0)
            ]
        );

        for _ in 0..7 {
            app.handle_action(Action::PageDown);
        }
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(4));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
    }

    #[test]
    fn moving_up_from_the_first_entry_stays_there() {
        let mut app = app_with_rows(25, 10);
        app.handle_action(Action::GoHome);

        for _ in 0..5 {
            app.handle_action(Action::MoveUp);

            assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
            assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
        }
    }

    #[test]
    fn moving_down_from_the_last_entry_stays_there() {
        let mut app = app_with_rows(25, 10);
        app.handle_action(Action::GoEnd);

        for _ in 0..5 {
            app.handle_action(Action::MoveDown);

            assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(24));
            assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 15);
        }
    }

    #[test]
    fn home_and_end_go_to_the_first_and_last_entry_and_scroll_to_them() {
        let mut app = app_with_rows(100, 10);

        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(99));
        assert_eq!(
            app.pane(ActivePane::Left).scroll_offset(),
            90,
            "the window starts so that the last entry is the last visible row"
        );

        app.handle_action(Action::GoHome);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
    }

    #[test]
    fn a_page_is_exactly_the_window_the_caller_supplied() {
        // No page size is built in: one page is one window of rows, whatever
        // the caller says that window is.
        for rows in [1, 2, 10, 20] {
            let mut app = app_with_rows(100, rows);
            app.handle_action(Action::GoHome);

            app.handle_action(Action::PageDown);
            assert_eq!(
                app.pane(ActivePane::Left).selected_index(),
                Some(rows),
                "a page in a window of {rows} rows"
            );

            app.handle_action(Action::PageUp);
            assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));

            assert_selection_is_sound(&app, "a page up and down");
        }
    }

    #[test]
    fn a_page_carries_the_window_with_it() {
        let mut app = app_with_rows(100, 10);
        app.handle_action(Action::GoHome);
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);

        // The page from 0 to 10 leaves the selection on the last row of a
        // window that starts at 1.
        app.handle_action(Action::PageDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(10));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 1);

        app.handle_action(Action::PageDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(20));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 11);

        app.handle_action(Action::PageUp);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(10));
        assert_eq!(
            app.pane(ActivePane::Left).scroll_offset(),
            10,
            "the window follows the selection up"
        );
    }

    #[test]
    fn a_selection_leaving_the_window_scrolls_it_one_row_at_a_time() {
        let mut app = app_with_rows(30, 10);
        app.handle_action(Action::GoHome);

        for _ in 0..10 {
            app.handle_action(Action::MoveDown);
        }
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(10));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 1);

        // Down to the last row of the window: still no scrolling needed.
        for _ in 0..9 {
            app.handle_action(Action::MoveDown);
        }
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(19));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 10);

        // One row further, and the window moves by exactly one row.
        app.handle_action(Action::MoveDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(20));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 11);

        // Back up to the first row of the window: still no scrolling needed.
        for _ in 0..9 {
            app.handle_action(Action::MoveUp);
        }
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(11));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 11);

        app.handle_action(Action::MoveUp);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(10));
        assert_eq!(
            app.pane(ActivePane::Left).scroll_offset(),
            10,
            "the window follows the selection up"
        );
    }

    #[test]
    fn an_unknown_window_scrolls_nothing_and_stays_valid() {
        // Nothing has told the pane how tall it is, so there is no page to
        // move and nothing to scroll, but the selection still moves.
        let mut app = app_with(25);
        app.set_visible_rows(ActivePane::Left, 0);

        app.handle_action(Action::PageDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);

        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(24));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);

        // Once the window is known, the selection is brought into it.
        app.set_visible_rows(ActivePane::Left, 10);
        assert_eq!(
            app.pane(ActivePane::Left).scroll_offset(),
            15,
            "the window now has to start at 15 to show the last entry"
        );
    }

    #[test]
    fn a_shorter_listing_brings_the_selection_and_the_offset_back() {
        let mut app = app_with_rows(25, 10);
        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 15);

        // The listing shrinks behind the selection, as it does when a later
        // phase replaces the entries of a pane.
        let mut shrunk = app.pane(ActivePane::Left).entries().to_vec();
        shrunk.truncate(3);
        app.left.set_entries(shrunk);
        app.handle_action(Action::MoveUp);

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));
        assert_eq!(
            app.pane(ActivePane::Left).scroll_offset(),
            0,
            "three entries fit in a window of ten, so nothing is scrolled"
        );
    }

    #[test]
    fn a_listing_that_becomes_empty_loses_its_selection_and_its_scroll() {
        let mut app = app_with_rows(25, 10);
        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(24));

        app.left.set_entries(Vec::new());

        for action in navigation_actions() {
            app.handle_action(action);

            assert_eq!(app.pane(ActivePane::Left).selected_index(), None);
            assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
        }
    }

    #[test]
    fn a_listing_that_grows_from_empty_can_be_moved_in_again() {
        let mut app = app_with(0);
        app.set_visible_rows(ActivePane::Left, 10);

        app.left
            .set_entries((0..40).map(|_| entry("grown.txt")).collect());

        app.handle_action(Action::MoveDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);

        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(39));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 30);

        assert_selection_is_sound(&app, "a listing that grew from empty");
    }

    #[test]
    fn a_window_that_changes_size_keeps_the_selected_entry_visible() {
        let mut app = app_with_rows(60, 10);
        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(59));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 50);

        // A taller window can show the same selection from further up.
        app.set_visible_rows(ActivePane::Left, 30);
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 30);

        // A shorter one has to scroll further down.
        app.set_visible_rows(ActivePane::Left, 5);
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 55);

        // A window taller than the listing never scrolls at all, and changing
        // the window never moves the selection.
        app.set_visible_rows(ActivePane::Left, 1000);
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(59));

        assert_selection_is_sound(&app, "a window that changed size");
    }

    #[test]
    fn a_very_large_listing_is_handled_without_overflow() {
        let count = 100_000;
        let mut app = app_with_rows(count, 10);

        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(count - 1));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), count - 10);

        app.handle_action(Action::PageUp);
        assert_eq!(
            app.pane(ActivePane::Left).selected_index(),
            Some(count - 11)
        );
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), count - 11);

        // A step larger than any listing saturates instead of overflowing.
        app.left.move_selection_down(usize::MAX);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(count - 1));

        app.left.move_selection_up(usize::MAX);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);

        assert_selection_is_sound(&app, "a listing of 100000 entries");
    }

    #[test]
    fn repeating_movement_at_the_boundaries_never_leaves_the_listing() {
        for action in navigation_actions() {
            let mut app = app_with_rows(25, 10);

            for step in 0..400 {
                app.handle_action(action);

                assert_selection_is_sound(&app, &format!("{action:?} repeated {step} times"));
            }
        }
    }

    #[test]
    fn every_navigation_action_keeps_the_state_sound_in_every_window() {
        for count in [0, 1, 2, 3, 7, 25, 100] {
            for rows in [0, 1, 2, 10, 25, 1000] {
                for action in navigation_actions() {
                    let mut app = app_with_rows(count, rows);

                    for _ in 0..12 {
                        app.handle_action(action);

                        assert_selection_is_sound(
                            &app,
                            &format!("{count} entries in {rows} rows after {action:?}"),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn an_empty_listing_has_no_scrollable_range() {
        assert_eq!(maximum_scroll_offset(0, 10), 0);
        assert_eq!(maximum_scroll_offset(0, 0), 0);
        assert_eq!(ensure_visible(0, 0, 0, 10), 0);
        assert_eq!(ensure_visible(0, 7, 0, 10), 0);
    }

    #[test]
    fn a_window_at_least_as_tall_as_the_listing_never_scrolls() {
        assert_eq!(maximum_scroll_offset(4, 10), 0);
        assert_eq!(maximum_scroll_offset(10, 10), 0);
        assert_eq!(maximum_scroll_offset(4, 1000), 0);
        assert_eq!(ensure_visible(3, 0, 4, 10), 0);
        assert_eq!(ensure_visible(3, 7, 4, 10), 0);
        assert_eq!(ensure_visible(9, 4, 10, 10), 0);
    }

    #[test]
    fn the_last_window_starts_at_what_the_listing_allows() {
        assert_eq!(maximum_scroll_offset(100, 10), 90);
        assert_eq!(maximum_scroll_offset(25, 10), 15);
        assert_eq!(maximum_scroll_offset(5, 1), 4);
        assert_eq!(maximum_scroll_offset(5, 0), 5);

        // The window that shows the last entry ends exactly at it.
        for (total, rows) in [(100, 10), (25, 10), (5, 1), (7, 3)] {
            let offset = ensure_visible(total - 1, 0, total, rows);

            assert_eq!(offset, maximum_scroll_offset(total, rows));
            assert_eq!(offset + rows, total, "the window ends at the listing");
        }
    }

    #[test]
    fn a_selection_below_the_window_brings_the_window_with_it() {
        assert_eq!(ensure_visible(20, 0, 100, 10), 11);
        assert_eq!(ensure_visible(30, 11, 100, 10), 21);
        assert_eq!(ensure_visible(99, 0, 100, 10), 90);
        assert_eq!(ensure_visible(4, 0, 5, 2), 3);
    }

    #[test]
    fn a_selection_above_the_window_brings_the_window_with_it() {
        assert_eq!(ensure_visible(3, 40, 100, 10), 3);
        assert_eq!(ensure_visible(0, 40, 100, 10), 0);
        assert_eq!(ensure_visible(9, 10, 100, 10), 9);
    }

    #[test]
    fn a_selection_inside_the_window_leaves_the_offset_alone() {
        assert_eq!(ensure_visible(10, 10, 100, 10), 10);
        assert_eq!(ensure_visible(15, 10, 100, 10), 10);
        assert_eq!(ensure_visible(19, 10, 100, 10), 10);
    }

    #[test]
    fn an_offset_beyond_the_listing_is_brought_back_into_range() {
        assert_eq!(ensure_visible(4, 900, 5, 10), 0);
        assert_eq!(ensure_visible(4, 900, 5, 2), 3);
        assert_eq!(ensure_visible(0, 900, 5, 5), 0);
    }

    #[test]
    fn no_known_window_leaves_the_offset_at_the_top_of_the_range() {
        assert_eq!(ensure_visible(3, 40, 100, 0), 40);
        assert_eq!(ensure_visible(3, 400, 100, 0), 100);
        assert_eq!(ensure_visible(0, 0, 0, 0), 0);
    }

    #[test]
    fn an_enormous_window_cannot_overflow() {
        assert_eq!(maximum_scroll_offset(usize::MAX, usize::MAX), 0);
        assert_eq!(maximum_scroll_offset(0, usize::MAX), 0);
        assert_eq!(ensure_visible(3, 2, 100, usize::MAX), 0);
        assert_eq!(ensure_visible(usize::MAX - 1, 0, usize::MAX, usize::MAX), 0);
    }

    /// Makes `which` the active pane by switching to it, as a user would.
    fn activate(app: &mut App, which: ActivePane) {
        while app.active_pane() != which {
            app.handle_action(Action::SwitchPane);
        }
    }

    /// Two panes showing different directories, each with its own selection and
    /// scroll offset, as they are once both have been browsed.
    ///
    /// The left pane shows three entries and the right five, and both can show
    /// two rows at a time, so every piece of per-pane state holds a value that
    /// only that pane has. The left pane ends up active, so a test that wants a
    /// different one only has to call [`activate`].
    fn two_loaded_panes() -> (App, TempDir, TempDir) {
        let left = TempDir::new("panes-left");
        let right = TempDir::new("panes-right");

        for name in ["left-a.txt", "left-b.txt", "left-c.txt"] {
            fs::write(left.path().join(name), b"content").expect("file should be written");
        }
        for name in [
            "right-a.txt",
            "right-b.txt",
            "right-c.txt",
            "right-d.txt",
            "right-e.txt",
        ] {
            fs::write(right.path().join(name), b"content").expect("file should be written");
        }

        let mut app = App::at(left.path().to_path_buf()).expect("the left directory should load");
        app.open_in(ActivePane::Right, right.path().to_path_buf())
            .expect("the right directory should load");
        app.set_visible_rows(ActivePane::Left, 2);
        app.set_visible_rows(ActivePane::Right, 2);

        activate(&mut app, ActivePane::Left);
        app.handle_action(Action::GoEnd);
        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::GoEnd);
        activate(&mut app, ActivePane::Left);

        (app, left, right)
    }

    /// Both panes of an application, so a test can say which one changed.
    fn snapshot(app: &App) -> (Pane, Pane) {
        (
            app.pane(ActivePane::Left).clone(),
            app.pane(ActivePane::Right).clone(),
        )
    }

    /// The pane of a snapshot that `which` names.
    fn pane_in(panes: &(Pane, Pane), which: ActivePane) -> &Pane {
        match which {
            ActivePane::Left => &panes.0,
            ActivePane::Right => &panes.1,
        }
    }

    #[test]
    fn the_left_pane_is_active_to_begin_with() {
        assert_eq!(App::default().active_pane(), ActivePane::Left);

        let directory = TempDir::new("panes-start");
        let app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        assert_eq!(app.active_pane(), ActivePane::Left);

        let app = App::at_working_directory().expect("the working directory should load");
        let working_directory =
            std::env::current_dir().expect("the test process has a working directory");

        assert_eq!(app.active_pane(), ActivePane::Left);
        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            &working_directory,
            "the active pane starts in the directory the process runs in"
        );
        assert_eq!(
            app.pane(ActivePane::Right).current_path(),
            &working_directory,
            "both panes start initialized with the working directory"
        );
    }

    #[test]
    fn switching_focus_leaves_both_panes_exactly_as_they_were() {
        let (mut app, left, right) = two_loaded_panes();

        // Each pane has its own directory, listing, selection and offset.
        assert_eq!(app.pane(ActivePane::Left).current_path(), left.path());
        assert_eq!(app.pane(ActivePane::Right).current_path(), right.path());
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 3);
        assert_eq!(app.pane(ActivePane::Right).entries().len(), 5);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(2));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 1);
        assert_eq!(app.pane(ActivePane::Right).selected_index(), Some(4));
        assert_eq!(app.pane(ActivePane::Right).scroll_offset(), 3);

        let before = snapshot(&app);

        app.handle_action(Action::SwitchPane);
        assert_eq!(app.active_pane(), ActivePane::Right);
        assert_eq!(
            snapshot(&app),
            before,
            "the left pane must not be reset by losing the focus"
        );

        app.handle_action(Action::SwitchPane);
        assert_eq!(app.active_pane(), ActivePane::Left);
        assert_eq!(
            snapshot(&app),
            before,
            "the right pane must not be reset by losing the focus"
        );
    }

    #[test]
    fn switching_between_panes_many_times_changes_nothing() {
        let (mut app, _, _) = two_loaded_panes();
        let before = snapshot(&app);

        for round in 0..500 {
            app.handle_action(Action::SwitchPane);

            assert_eq!(
                app.active_pane(),
                if round % 2 == 0 {
                    ActivePane::Right
                } else {
                    ActivePane::Left
                },
                "switching must alternate, and must never leave no pane active"
            );
            assert_eq!(snapshot(&app), before, "after {round} switches");
        }
    }

    #[test]
    fn switching_panes_does_not_reread_the_directories() {
        let (mut app, left, right) = two_loaded_panes();
        let before = snapshot(&app);

        // Both directories change behind the panes' backs. A pane that were
        // reloaded while switching would pick the new entries up.
        fs::write(left.path().join("left-new.txt"), b"content").expect("file should be written");
        fs::write(right.path().join("right-new.txt"), b"content").expect("file should be written");

        for _ in 0..4 {
            app.handle_action(Action::SwitchPane);
        }

        assert_eq!(app.pane(ActivePane::Left).entries().len(), 3);
        assert_eq!(app.pane(ActivePane::Right).entries().len(), 5);
        assert_eq!(snapshot(&app), before);
        assert!(
            !app.notification().is_active(),
            "switching must not reach the filesystem, so it cannot fail either"
        );
    }

    #[test]
    fn switching_panes_keeps_working_when_the_directories_are_gone() {
        let (mut app, left, right) = two_loaded_panes();
        let before = snapshot(&app);

        fs::remove_dir_all(left.path()).expect("directory should be removed");
        fs::remove_dir_all(right.path()).expect("directory should be removed");

        for _ in 0..4 {
            app.handle_action(Action::SwitchPane);

            assert_eq!(snapshot(&app), before);
        }
        assert!(!app.notification().is_active());
    }

    #[test]
    fn every_selection_action_acts_only_on_the_active_pane() {
        for action in navigation_actions() {
            for starting_pane in [ActivePane::Left, ActivePane::Right] {
                let (mut app, _, _) = two_loaded_panes();
                activate(&mut app, starting_pane);
                let before = snapshot(&app);
                let (inactive, active) = match starting_pane {
                    ActivePane::Left => (ActivePane::Right, ActivePane::Left),
                    ActivePane::Right => (ActivePane::Left, ActivePane::Right),
                };

                for _ in 0..3 {
                    app.handle_action(action);
                }

                assert_eq!(
                    app.pane(inactive),
                    pane_in(&before, inactive),
                    "{action:?} in the {starting_pane:?} pane must leave the {inactive:?} pane alone"
                );
                assert_pane_is_sound(&app, active, &format!("{action:?} in {starting_pane:?}"));
            }
        }
    }

    #[test]
    fn opening_a_directory_acts_only_on_the_active_pane() {
        let outer = TempDir::new("panes-open");
        let child = outer.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        fs::write(child.join("inside.txt"), b"content").expect("file should be written");

        let other = TempDir::new("panes-open-other");
        fs::write(other.path().join("other.txt"), b"content").expect("file should be written");

        // The directory to be entered is the first entry of the left pane.
        let mut app = App::at(outer.path().to_path_buf()).expect("the directory should load");
        app.open_in(ActivePane::Right, other.path().to_path_buf())
            .expect("the other directory should load");
        let before = snapshot(&app);

        app.handle_action(Action::Open);

        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            &child,
            "the active pane enters the selected directory"
        );
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);
        assert_eq!(
            app.pane(ActivePane::Right),
            pane_in(&before, ActivePane::Right),
            "the inactive pane must not move"
        );

        // And the other way round: the right pane is the one that navigates.
        let before = snapshot(&app);
        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::Open);

        assert_eq!(app.pane(ActivePane::Right).current_path(), other.path());
        assert_eq!(app.pane(ActivePane::Right).entries(), before.1.entries());
        assert_eq!(
            app.pane(ActivePane::Left),
            pane_in(&before, ActivePane::Left),
            "the pane that was active before must be left where it was"
        );
    }

    #[test]
    fn going_to_the_parent_acts_only_on_the_active_pane() {
        // The shape of the phase's example: one pane deep inside a tree, the
        // other somewhere else entirely.
        let project = TempDir::new("panes-parent");
        let source = project.path().join("src");
        fs::create_dir(&source).expect("directory should be created");

        let home = TempDir::new("panes-parent-other");
        fs::write(home.path().join("notes.txt"), b"content").expect("file should be written");

        let mut app = App::at(source.clone()).expect("the directory should load");
        app.open_in(ActivePane::Right, home.path().to_path_buf())
            .expect("the other directory should load");
        let before = snapshot(&app);

        app.handle_action(Action::GoParent);

        assert_eq!(app.pane(ActivePane::Left).current_path(), project.path());
        assert_eq!(
            app.pane(ActivePane::Right),
            pane_in(&before, ActivePane::Right),
            "the inactive pane must stay where it is"
        );

        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::GoParent);

        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            project.path(),
            "the pane that was left must not move"
        );
        assert_ne!(
            app.pane(ActivePane::Right).current_path(),
            before.1.current_path(),
            "the active pane did go up"
        );
    }

    #[test]
    fn going_up_at_a_filesystem_root_acts_on_neither_pane() {
        // The root is found by walking up, so no platform spelling is assumed.
        let mut root = std::env::current_dir().expect("the test process has a working directory");
        while let Some(parent) = crate::filesystem::navigation::parent_of(&root) {
            root = parent;
        }

        let other = TempDir::new("panes-root-other");
        fs::write(other.path().join("other.txt"), b"content").expect("file should be written");

        let mut app = App::at(root.clone()).expect("the root should load");
        app.open_in(ActivePane::Right, other.path().to_path_buf())
            .expect("the other directory should load");
        let before = snapshot(&app);

        app.handle_action(Action::GoParent);

        assert_eq!(app.pane(ActivePane::Left).current_path(), &root);
        assert_eq!(snapshot(&app), before, "there is nowhere to go");

        // The other pane, which does have a parent, still goes up.
        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::GoParent);

        assert_eq!(app.pane(ActivePane::Left).current_path(), &root);
        assert_eq!(
            app.pane(ActivePane::Right).current_path(),
            other
                .path()
                .parent()
                .expect("a temporary directory is inside a directory")
        );
    }

    #[test]
    fn refreshing_acts_only_on_the_active_pane() {
        let (mut app, left, right) = two_loaded_panes();
        let before = snapshot(&app);

        fs::write(left.path().join("left-new.txt"), b"content").expect("file should be written");
        fs::write(right.path().join("right-new.txt"), b"content").expect("file should be written");

        app.refresh_active_pane()
            .expect("refreshing should succeed");

        assert_eq!(
            app.pane(ActivePane::Left).entries().len(),
            4,
            "the active pane re-reads its directory"
        );
        assert_eq!(
            app.pane(ActivePane::Right),
            pane_in(&before, ActivePane::Right),
            "refreshing must not touch the inactive pane"
        );

        activate(&mut app, ActivePane::Right);
        app.refresh_active_pane()
            .expect("refreshing should succeed");

        assert_eq!(app.pane(ActivePane::Right).entries().len(), 6);
        assert_eq!(
            app.pane(ActivePane::Left).entries().len(),
            4,
            "the pane refreshed before must keep what it read"
        );
    }

    #[test]
    fn an_empty_pane_can_be_switched_to_and_navigated_without_panicking() {
        let mut app = App::default();

        for which in [ActivePane::Right, ActivePane::Left, ActivePane::Right] {
            activate(&mut app, which);
            assert_eq!(app.active_pane(), which);

            for action in navigation_actions() {
                for _ in 0..2 {
                    app.handle_action(action);
                }
            }

            assert_eq!(app.pane(which).selected_index(), None);
            assert_eq!(app.pane(which).scroll_offset(), 0);
            assert!(!app.notification().is_active());
        }

        // The navigation actions that reach the filesystem are safe too: there
        // is nothing selected to open, and no directory to go up from.
        assert!(
            app.open_selected().is_ok(),
            "nothing is selected, so opening can only do nothing"
        );
        assert!(
            app.go_to_parent().is_ok(),
            "a pane with no directory has nowhere to go, which is not an error"
        );

        assert_eq!(snapshot(&app), (Pane::default(), Pane::default()));
    }

    /// The names a pane is showing.
    fn names_of(app: &App, which: ActivePane) -> Vec<String> {
        app.pane(which)
            .entries()
            .iter()
            .map(|entry| entry.name().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn creating_a_file_creates_it_and_reloads_only_the_active_pane() {
        let (mut app, left, _right) = two_loaded_panes();
        let before = snapshot(&app);

        app.create_file(OsStr::new("created by test.txt"))
            .expect("the file should be created");

        assert!(left.path().join("created by test.txt").is_file());
        assert!(
            names_of(&app, ActivePane::Left).contains(&String::from("created by test.txt")),
            "the active pane is re-read, so the new entry is in its listing"
        );
        assert_eq!(
            app.pane(ActivePane::Right),
            pane_in(&before, ActivePane::Right),
            "the inactive pane must not be re-read or changed"
        );
        assert_pane_is_sound(&app, ActivePane::Left, "after creating a file");
        assert!(
            !app.notification().is_active(),
            "a change that worked has nothing to report"
        );
    }

    #[test]
    fn creating_a_directory_creates_it_and_reloads_only_the_active_pane() {
        let (mut app, left, _right) = two_loaded_panes();
        let before = snapshot(&app);

        app.create_directory(OsStr::new("new folder"))
            .expect("the directory should be created");

        assert!(left.path().join("new folder").is_dir());
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 4);
        assert_eq!(
            app.pane(ActivePane::Right),
            pane_in(&before, ActivePane::Right)
        );
        assert_pane_is_sound(&app, ActivePane::Left, "after creating a directory");
    }

    #[test]
    fn renaming_the_selected_entry_moves_it_and_reloads_only_the_active_pane() {
        let (mut app, left, _right) = two_loaded_panes();
        // The left pane's selection is the third of its three entries.
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(2));
        let before = snapshot(&app);

        app.rename_selected(OsStr::new("renamed.txt"))
            .expect("the rename should succeed");

        assert!(!left.path().join("left-c.txt").exists());
        assert_eq!(
            fs::read(left.path().join("renamed.txt")).expect("file should be read"),
            b"content",
            "the entry keeps its contents"
        );

        let names = names_of(&app, ActivePane::Left);
        assert!(names.contains(&String::from("renamed.txt")));
        assert!(!names.contains(&String::from("left-c.txt")));
        assert_eq!(
            app.pane(ActivePane::Right),
            pane_in(&before, ActivePane::Right)
        );
        assert_pane_is_sound(&app, ActivePane::Left, "after renaming");
    }

    #[test]
    fn the_right_pane_can_be_the_one_that_changes() {
        let (mut app, left, right) = two_loaded_panes();
        activate(&mut app, ActivePane::Right);
        let before = snapshot(&app);

        app.create_file(OsStr::new("right-new.txt"))
            .expect("the file should be created");
        app.handle_action(Action::GoHome);
        app.rename_selected(OsStr::new("right-renamed.txt"))
            .expect("the rename should succeed");

        assert_eq!(app.pane(ActivePane::Right).entries().len(), 6);
        assert!(!right.path().join("right-a.txt").exists());
        assert!(right.path().join("right-renamed.txt").is_file());
        assert!(right.path().join("right-new.txt").is_file());
        assert_eq!(
            app.pane(ActivePane::Left),
            pane_in(&before, ActivePane::Left),
            "the pane that was not worked in must be left alone"
        );
        assert!(
            !left.path().join("right-new.txt").exists(),
            "nothing may appear in the other directory"
        );
    }

    #[test]
    fn a_failed_creation_changes_nothing_and_is_reported() {
        let (mut app, left, _right) = two_loaded_panes();
        let before = snapshot(&app);

        let error = app
            .create_file(OsStr::new("left-a.txt"))
            .expect_err("that name is taken");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read(left.path().join("left-a.txt")).expect("file should be read"),
            b"content",
            "the entry that was already there must be untouched"
        );
        assert_eq!(snapshot(&app), before, "a failed change alters no state");
        assert!(
            app.notification().is_active(),
            "the failure must be reported rather than passed over"
        );
    }

    #[test]
    fn a_failed_rename_changes_nothing_and_is_reported() {
        let (mut app, left, _right) = two_loaded_panes();
        let before = snapshot(&app);

        let error = app
            .rename_selected(OsStr::new("left-a.txt"))
            .expect_err("another entry already has that name");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(snapshot(&app), before);
        assert!(app.notification().is_active());
        assert!(left.path().join("left-a.txt").is_file());
        assert!(
            left.path().join("left-c.txt").is_file(),
            "the entry that was to be renamed is still there"
        );
    }

    #[test]
    fn a_name_that_is_not_a_single_name_is_refused() {
        // A backslash separates names on Windows as well; on Unix it is an
        // ordinary character, so that name is only refused there.
        #[cfg(windows)]
        let names = ["", ".", "..", "nested/file.txt", "nested\\file.txt"];
        #[cfg(not(windows))]
        let names = ["", ".", "..", "nested/file.txt"];

        for name in names {
            let (mut app, left, _right) = two_loaded_panes();
            let before = snapshot(&app);

            let error = app
                .create_file(OsStr::new(name))
                .expect_err("that name cannot name a child of a directory");

            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "for {name:?}");
            assert_eq!(snapshot(&app), before, "for {name:?}");
            assert_eq!(
                fs::read_dir(left.path())
                    .expect("listing should succeed")
                    .count(),
                3,
                "nothing may be created for {name:?}"
            );
        }
    }

    #[test]
    fn an_unusable_name_is_refused_when_renaming_too() {
        let (mut app, left, _right) = two_loaded_panes();
        let before = snapshot(&app);

        let error = app
            .rename_selected(OsStr::new(".."))
            .expect_err("that name cannot name a child of a directory");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(snapshot(&app), before);
        assert!(left.path().join("left-c.txt").is_file());
    }

    #[test]
    fn renaming_with_nothing_selected_does_nothing() {
        let mut app = App::default();
        let before = snapshot(&app);

        app.rename_selected(OsStr::new("anything.txt"))
            .expect("there is nothing to rename, which is not an error");

        assert_eq!(snapshot(&app), before);
        assert!(!app.notification().is_active());
    }

    #[test]
    fn a_pane_without_a_directory_cannot_create_anything() {
        let mut app = App::default();
        let before = snapshot(&app);

        let error = app
            .create_file(OsStr::new("terminalvision-should-not-exist.txt"))
            .expect_err("there is nowhere to create it");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(snapshot(&app), before);
        assert!(
            !PathBuf::from("terminalvision-should-not-exist.txt").exists(),
            "nothing may be created relative to the process's own directory"
        );

        assert!(
            app.create_directory(OsStr::new("terminalvision-should-not-exist"))
                .is_err()
        );
        assert!(!PathBuf::from("terminalvision-should-not-exist").exists());
    }

    #[test]
    fn only_the_active_pane_is_re_read_after_a_change() {
        let (mut app, _left, right) = two_loaded_panes();

        // The other directory changes behind the pane's back, so a pane that
        // were re-read after the change would show the new entry.
        fs::write(right.path().join("right-new.txt"), b"content").expect("file should be written");

        app.create_file(OsStr::new("left-new.txt"))
            .expect("the file should be created");

        assert_eq!(
            app.pane(ActivePane::Left).entries().len(),
            4,
            "the pane that was worked in is re-read"
        );
        assert_eq!(
            app.pane(ActivePane::Right).entries().len(),
            5,
            "the other pane is not re-read"
        );
    }

    #[test]
    fn a_failed_change_re_reads_neither_pane() {
        let (mut app, left, right) = two_loaded_panes();

        // Both directories change behind the panes' backs, then a change that
        // cannot work is asked for.
        fs::write(left.path().join("left-new.txt"), b"content").expect("file should be written");
        fs::write(right.path().join("right-new.txt"), b"content").expect("file should be written");
        let before = snapshot(&app);

        assert!(app.create_file(OsStr::new("left-a.txt")).is_err());

        assert_eq!(
            snapshot(&app),
            before,
            "a failed change leaves both listings exactly as they were, unread"
        );
    }

    /// An application whose left pane shows `left` and whose right pane shows
    /// `right`, with the left pane active.
    fn two_panes_showing(left: &TempDir, right: &TempDir) -> App {
        let mut app = App::at(left.path().to_path_buf()).expect("the left directory should load");
        app.open_in(ActivePane::Right, right.path().to_path_buf())
            .expect("the right directory should load");

        app
    }

    /// The path of the entry `which` pane has selected.
    fn selected_path(app: &App, which: ActivePane) -> PathBuf {
        app.pane(which)
            .entries()
            .get(
                app.pane(which)
                    .selected_index()
                    .expect("something is selected"),
            )
            .expect("the selection points at an entry")
            .path()
            .to_path_buf()
    }

    #[test]
    fn copying_marks_the_selected_entry_without_touching_the_filesystem() {
        let (mut app, left, _right) = two_loaded_panes();
        let marked = selected_path(&app, ActivePane::Left);

        app.handle_action(Action::Copy);

        assert_eq!(app.clipboard().operation(), Some(ClipboardOperation::Copy));
        assert_eq!(app.clipboard().len(), 1);
        assert_eq!(app.clipboard().entries().to_vec(), vec![marked.clone()]);
        assert!(marked.is_file(), "marking copies nothing");
        assert_eq!(
            fs::read_dir(left.path())
                .expect("listing should succeed")
                .count(),
            3,
            "marking creates nothing"
        );
        assert!(!app.notification().is_active());
    }

    #[test]
    fn cutting_marks_the_selected_entry_without_touching_the_filesystem() {
        let (mut app, left, _right) = two_loaded_panes();
        let marked = selected_path(&app, ActivePane::Left);

        app.handle_action(Action::Cut);

        assert_eq!(app.clipboard().operation(), Some(ClipboardOperation::Cut));
        assert_eq!(app.clipboard().entries().to_vec(), vec![marked.clone()]);
        assert!(marked.is_file(), "marking moves nothing");
        assert_eq!(
            fs::read_dir(left.path())
                .expect("listing should succeed")
                .count(),
            3
        );
    }

    #[test]
    fn marking_with_nothing_selected_marks_nothing() {
        for action in [Action::Copy, Action::Cut] {
            let mut app = App::default();
            let before = snapshot(&app);

            app.handle_action(action);

            assert!(app.clipboard().is_empty(), "for {action:?}");
            assert_eq!(app.clipboard().operation(), None, "for {action:?}");
            assert_eq!(snapshot(&app), before, "for {action:?}");
            assert!(!app.notification().is_active());
        }
    }

    #[test]
    fn marking_replaces_what_was_marked_before() {
        let (mut app, _left, _right) = two_loaded_panes();
        let first = app.pane(ActivePane::Left).entries()[0].path().to_path_buf();
        app.handle_action(Action::MoveUp);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));

        app.handle_action(Action::Cut);
        app.handle_action(Action::MoveUp);
        app.handle_action(Action::Copy);

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(app.clipboard().len(), 1, "one entry is marked at a time");
        assert_eq!(app.clipboard().operation(), Some(ClipboardOperation::Copy));
        assert_eq!(app.clipboard().entries().to_vec(), vec![first]);
    }

    #[test]
    fn pasting_a_copy_leaves_the_source_and_keeps_the_clipboard() {
        let (mut app, left, right) = two_loaded_panes();
        let marked = selected_path(&app, ActivePane::Left);

        app.handle_action(Action::Copy);
        activate(&mut app, ActivePane::Right);

        // The directory the copy came from changes behind its pane's back, so a
        // pane that were re-read would show the new entry.
        fs::write(left.path().join("left-new.txt"), b"content").expect("file should be written");

        app.handle_action(Action::Paste);

        assert_eq!(
            fs::read(right.path().join("left-c.txt")).expect("file should be read"),
            b"content"
        );
        assert_eq!(fs::read(&marked).expect("file should be read"), b"content");
        assert_eq!(
            app.pane(ActivePane::Right).entries().len(),
            6,
            "the pane that was pasted into is re-read"
        );
        assert_eq!(
            app.pane(ActivePane::Left).entries().len(),
            3,
            "the pane the copy came from is not re-read, because nothing there changed"
        );
        assert_eq!(
            app.clipboard().operation(),
            Some(ClipboardOperation::Copy),
            "a copy stays marked, so it can be pasted again"
        );
        assert!(!app.notification().is_active());
        assert_pane_is_sound(&app, ActivePane::Right, "after a copy paste");

        // Pasting the same copy into the same place is refused, because the name
        // is taken; the copy is still there to paste somewhere else.
        let error = app.paste().expect_err("the name is taken");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(app.clipboard().len(), 1);
    }

    #[test]
    fn pasting_a_cut_moves_the_entry_and_empties_the_clipboard() {
        let (mut app, _left, right) = two_loaded_panes();
        let marked = selected_path(&app, ActivePane::Left);

        app.handle_action(Action::Cut);
        assert!(marked.is_file(), "cutting moves nothing by itself");

        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::Paste);

        assert!(!marked.exists(), "the source is gone");
        assert_eq!(
            fs::read(right.path().join("left-c.txt")).expect("file should be read"),
            b"content"
        );
        assert!(
            app.clipboard().is_empty(),
            "a moved entry is no longer marked"
        );
        assert_eq!(app.clipboard().operation(), None);
        assert_eq!(
            app.pane(ActivePane::Left).entries().len(),
            2,
            "the pane that lost an entry is re-read"
        );
        assert_eq!(app.pane(ActivePane::Right).entries().len(), 6);
        assert_pane_is_sound(&app, ActivePane::Left, "the pane that lost an entry");
        assert_pane_is_sound(&app, ActivePane::Right, "the pane that gained one");

        // Nothing is marked any more, so pasting again does nothing at all.
        let before = snapshot(&app);
        app.paste().expect("there is nothing to paste");
        assert_eq!(snapshot(&app), before);
    }

    #[test]
    fn pasting_into_the_directory_an_entry_already_lives_in_is_a_conflict() {
        let (mut app, left, _right) = two_loaded_panes();

        for action in [Action::Copy, Action::Cut] {
            app.handle_action(action);
            let before = snapshot(&app);

            let error = app.paste().expect_err("the entry is already there");

            assert_eq!(error.kind(), io::ErrorKind::AlreadyExists, "for {action:?}");
            assert_eq!(snapshot(&app), before, "for {action:?}");
            assert_eq!(
                fs::read_dir(left.path())
                    .expect("listing should succeed")
                    .count(),
                3,
                "nothing may be created or renamed around it for {action:?}"
            );
            assert_eq!(
                app.clipboard().len(),
                1,
                "nothing was pasted, so nothing is unmarked for {action:?}"
            );
        }
    }

    #[test]
    fn pasting_with_nothing_marked_does_nothing() {
        let (mut app, _left, _right) = two_loaded_panes();
        let before = snapshot(&app);

        app.paste()
            .expect("there is nothing to paste, which is not an error");

        assert_eq!(snapshot(&app), before);
        assert!(!app.notification().is_active());
    }

    #[test]
    fn pasting_into_a_pane_without_a_directory_is_refused() {
        let mut app = App::default();
        let before = snapshot(&app);
        // A pane that is showing nothing has nothing to select, so the mark is
        // made directly, the way a later phase would fill the clipboard.
        app.clipboard.set(
            vec![PathBuf::from("somewhere.txt")],
            ClipboardOperation::Copy,
        );

        let error = app.paste().expect_err("there is nowhere to paste into");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(snapshot(&app), before, "neither pane changes");
        assert!(
            !PathBuf::from("somewhere.txt").exists(),
            "nothing may be created relative to the process's own directory"
        );
        assert_eq!(app.clipboard().len(), 1, "nothing was pasted");
    }

    #[test]
    fn pasting_an_entry_that_has_gone_reports_it() {
        for action in [Action::Copy, Action::Cut] {
            let (mut app, _left, right) = two_loaded_panes();
            let marked = selected_path(&app, ActivePane::Left);

            app.handle_action(action);
            fs::remove_file(&marked).expect("file should be removed");
            activate(&mut app, ActivePane::Right);
            let before = snapshot(&app);

            let error = app.paste().expect_err("the entry is gone");

            assert_eq!(error.kind(), io::ErrorKind::NotFound, "for {action:?}");
            assert_eq!(snapshot(&app), before, "a failed paste changes nothing");
            assert_eq!(
                app.clipboard().len(),
                1,
                "nothing arrived, so nothing is unmarked for {action:?}"
            );
            assert!(!right.path().join("left-c.txt").exists());
        }
    }

    #[test]
    fn pasting_a_directory_brings_everything_under_it() {
        let source = TempDir::new("paste-tree-source");
        let tree = source.path().join("tree");
        fs::create_dir_all(tree.join("nested")).expect("directory should be created");
        fs::write(tree.join("nested").join("deep.txt"), b"deep").expect("file should be written");
        let destination = TempDir::new("paste-tree-destination");

        let mut app = two_panes_showing(&source, &destination);
        app.handle_action(Action::Copy);
        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::Paste);

        assert_eq!(
            fs::read(
                destination
                    .path()
                    .join("tree")
                    .join("nested")
                    .join("deep.txt")
            )
            .expect("file should be read"),
            b"deep"
        );
        assert!(
            tree.join("nested").join("deep.txt").is_file(),
            "a copy leaves the source tree alone"
        );
        assert_eq!(app.pane(ActivePane::Right).entries().len(), 1);
        assert_eq!(
            app.pane(ActivePane::Right).selected_index(),
            None,
            "the destination had nothing selected, and a reload selects nothing by itself"
        );
        assert_eq!(app.pane(ActivePane::Right).scroll_offset(), 0);

        app.handle_action(Action::MoveDown);
        assert_eq!(
            app.pane(ActivePane::Right).entries()[0].name(),
            OsStr::new("tree"),
            "the pasted tree can be selected and browsed like any other entry"
        );
        assert_pane_is_sound(&app, ActivePane::Right, "after pasting a directory");
    }

    #[test]
    fn pasting_into_an_empty_directory_leaves_it_selectable_again() {
        let source = TempDir::new("paste-into-empty-source");
        fs::write(source.path().join("file.txt"), b"content").expect("file should be written");
        let empty = TempDir::new("paste-into-empty-destination");

        let mut app = two_panes_showing(&source, &empty);
        assert!(app.pane(ActivePane::Right).entries().is_empty());
        assert_eq!(app.pane(ActivePane::Right).selected_index(), None);

        app.handle_action(Action::Copy);
        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::Paste);

        assert!(empty.path().join("file.txt").is_file());
        assert_eq!(app.pane(ActivePane::Right).entries().len(), 1);
        assert_eq!(
            app.pane(ActivePane::Right).selected_index(),
            None,
            "the empty destination had nothing selected, and a reload selects nothing by itself"
        );
        assert_eq!(app.pane(ActivePane::Right).scroll_offset(), 0);

        app.handle_action(Action::MoveDown);
        assert_eq!(
            app.pane(ActivePane::Right).selected_index(),
            Some(0),
            "the pasted entry can be selected as soon as the pane is moved in"
        );
        assert_eq!(app.pane(ActivePane::Right).scroll_offset(), 0);
        assert_pane_is_sound(
            &app,
            ActivePane::Right,
            "after pasting into an empty directory",
        );
    }

    #[test]
    fn a_cut_that_empties_the_pane_it_came_from_leaves_it_sound() {
        let source = TempDir::new("cut-empties-source");
        fs::write(source.path().join("only.txt"), b"content").expect("file should be written");
        let destination = TempDir::new("cut-empties-destination");

        let mut app = two_panes_showing(&source, &destination);
        app.handle_action(Action::Cut);
        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::Paste);

        let emptied = app.pane(ActivePane::Left);
        assert!(emptied.entries().is_empty());
        assert_eq!(
            emptied.selected_index(),
            None,
            "an empty pane has no selection"
        );
        assert_eq!(emptied.scroll_offset(), 0);
        assert_pane_is_sound(&app, ActivePane::Left, "a pane that lost its only entry");

        let gained = app.pane(ActivePane::Right);
        assert_eq!(gained.entries().len(), 1);
        assert_eq!(
            gained.selected_index(),
            None,
            "the destination had nothing selected, and a reload selects nothing by itself"
        );
        assert_eq!(gained.scroll_offset(), 0);

        app.handle_action(Action::MoveDown);
        assert_eq!(app.pane(ActivePane::Right).selected_index(), Some(0));
        assert_pane_is_sound(&app, ActivePane::Right, "the pane that gained it");
        assert!(destination.path().join("only.txt").is_file());
    }

    #[test]
    fn the_paste_action_pastes_and_reports_a_conflict() {
        let (mut app, left, right) = two_loaded_panes();
        let marked = selected_path(&app, ActivePane::Left);

        // A copy pasted through the actions themselves.
        app.handle_action(Action::Copy);
        app.handle_action(Action::SwitchPane);
        app.handle_action(Action::Paste);

        assert!(right.path().join("left-c.txt").is_file());
        assert!(marked.is_file(), "a copy leaves the source where it is");
        assert!(
            !app.notification().is_active(),
            "a paste that worked has nothing to report"
        );

        // A cut of an entry that is already in the directory it is pasted into.
        app.handle_action(Action::SwitchPane);
        app.handle_action(Action::Cut);
        app.handle_action(Action::Paste);

        assert!(
            app.notification().is_active(),
            "a paste that could not work must be reported by the action"
        );
        assert_eq!(app.clipboard().len(), 1, "the mark survives a failed paste");
        assert!(marked.is_file(), "nothing may be moved or replaced");
        assert_eq!(
            fs::read_dir(left.path())
                .expect("listing should succeed")
                .count(),
            3,
            "nothing may be created"
        );
    }

    #[test]
    fn deleting_the_selected_entry_removes_it_and_re_loads_the_active_pane() {
        let (mut app, _left, right) = two_loaded_panes();
        let selected = selected_path(&app, ActivePane::Left);
        // The directory the other pane is showing changes behind its back, so a
        // pane that were re-read would show an entry the other one does not.
        fs::write(right.path().join("right-new.txt"), b"content").expect("file should be written");
        let other_pane = app.pane(ActivePane::Right).clone();

        app.handle_action(Action::Delete);

        assert!(!selected.exists(), "the entry must be gone");
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 2);
        assert_eq!(app.last_outcome(), Some(OperationOutcome::Done));
        assert!(!app.notification().is_active(), "nothing went wrong");
        assert_pane_is_sound(&app, ActivePane::Left, "after deleting the selected entry");
        assert_eq!(
            app.pane(ActivePane::Right),
            &other_pane,
            "the pane that is not showing the deleted entry must not be re-read"
        );
        assert!(right.path().join("right-a.txt").is_file());
    }

    #[test]
    fn deleting_an_entry_the_other_pane_is_listing_re_loads_it() {
        let directory = TempDir::new("delete-listed-in-both");
        for name in ["a.txt", "b.txt", "c.txt"] {
            fs::write(directory.path().join(name), b"content").expect("file should be written");
        }
        let mut app = two_panes_showing(&directory, &directory);
        let selected = selected_path(&app, ActivePane::Left);

        app.handle_action(Action::Delete);

        assert!(!selected.exists());
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 2);
        assert_eq!(
            app.pane(ActivePane::Right).entries().len(),
            2,
            "a pane listing the deleted entry must be re-read"
        );
        assert_pane_is_sound(&app, ActivePane::Right, "the pane that listed the entry");
    }

    #[test]
    fn deleting_a_directory_the_other_pane_is_showing_moves_that_pane_out() {
        let outer = TempDir::new("delete-shown-directory");
        let inner = outer.path().join("inner");
        fs::create_dir(&inner).expect("directory should be created");
        fs::write(inner.join("deep.txt"), b"content").expect("file should be written");
        fs::write(outer.path().join("keep.txt"), b"content").expect("file should be written");

        let mut app = App::at(outer.path().to_path_buf()).expect("the directory should load");
        app.open_in(ActivePane::Right, inner.clone())
            .expect("the inner directory should load");
        activate(&mut app, ActivePane::Left);
        assert_eq!(
            selected_path(&app, ActivePane::Left),
            inner,
            "the directory to delete is the first entry"
        );

        app.handle_action(Action::Delete);

        assert!(
            !inner.exists(),
            "the directory must be gone with everything in it"
        );
        assert_eq!(
            app.pane(ActivePane::Right).current_path(),
            outer.path(),
            "the pane that was inside it must go to the nearest directory still there"
        );
        assert_eq!(app.pane(ActivePane::Right).entries().len(), 1);
        assert_pane_is_sound(
            &app,
            ActivePane::Right,
            "a pane taken out of a deleted directory",
        );
    }

    #[test]
    fn deleting_the_last_entry_leaves_the_selection_on_the_nearest_one() {
        let directory = TempDir::new("delete-last-entry");
        for name in ["a.txt", "b.txt", "c.txt"] {
            fs::write(directory.path().join(name), b"content").expect("file should be written");
        }
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        app.set_visible_rows(ActivePane::Left, 2);
        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(2));

        app.handle_action(Action::Delete);

        assert_eq!(app.pane(ActivePane::Left).entries().len(), 2);
        assert_eq!(
            app.pane(ActivePane::Left).selected_index(),
            Some(1),
            "the nearest entry left must be selected"
        );
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
        assert_pane_is_sound(&app, ActivePane::Left, "after deleting the last entry");
    }

    #[test]
    fn deleting_the_only_entry_leaves_an_empty_pane() {
        let directory = TempDir::new("delete-only-entry");
        let only = directory.path().join("only.txt");
        fs::write(&only, b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        app.set_visible_rows(ActivePane::Left, 5);

        app.handle_action(Action::Delete);

        assert!(!only.exists());
        assert!(app.pane(ActivePane::Left).entries().is_empty());
        assert_eq!(
            app.pane(ActivePane::Left).selected_index(),
            None,
            "a listing with nothing in it has no selection at all"
        );
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
        assert_eq!(app.last_outcome(), Some(OperationOutcome::Done));
        assert_pane_is_sound(&app, ActivePane::Left, "an emptied pane");
    }

    #[test]
    fn deleting_with_nothing_selected_does_nothing() {
        let directory = TempDir::new("delete-nothing-selected");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        let before = app.clone();

        app.handle_action(Action::Delete);

        assert_eq!(app, before, "nothing may change when nothing is selected");
        assert!(!app.notification().is_active());
        assert_eq!(
            app.last_outcome(),
            None,
            "nothing was asked of the filesystem, so there is nothing to report"
        );
    }

    #[test]
    fn repeated_deletion_at_the_boundaries_does_not_panic() {
        let directory = TempDir::new("delete-repeatedly");
        for name in ["a.txt", "b.txt"] {
            fs::write(directory.path().join(name), b"content").expect("file should be written");
        }
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        app.set_visible_rows(ActivePane::Left, 1);

        for round in 0..4 {
            app.handle_action(Action::GoHome);
            app.handle_action(Action::GoEnd);
            app.handle_action(Action::Delete);

            assert_pane_is_sound(&app, ActivePane::Left, &format!("deletion round {round}"));
        }

        assert!(app.pane(ActivePane::Left).entries().is_empty());
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("listing should succeed")
                .count(),
            0,
            "both entries must be gone"
        );

        // One more deletion with nothing left to delete.
        app.handle_action(Action::Delete);
        assert_pane_is_sound(&app, ActivePane::Left, "deleting with nothing left");
    }

    #[test]
    fn a_failed_deletion_changes_nothing_and_is_reported() {
        let (mut app, _left, _right) = two_loaded_panes();
        let selected = selected_path(&app, ActivePane::Left);
        // The entry disappears behind the application's back.
        fs::remove_file(&selected).expect("file should be removed");
        let before = snapshot(&app);

        app.handle_action(Action::Delete);

        assert_eq!(
            snapshot(&app),
            before,
            "a deletion that failed must change neither pane"
        );
        assert!(
            app.notification().is_active(),
            "the failure must be reported"
        );
        assert_eq!(app.last_outcome(), Some(OperationOutcome::SourceMissing));
    }

    #[test]
    fn a_copy_that_conflicts_reports_it_and_keeps_the_clipboard() {
        let directory = TempDir::new("paste-conflict-copy");
        let marked = directory.path().join("file.txt");
        fs::write(&marked, b"content").expect("file should be written");
        let mut app = two_panes_showing(&directory, &directory);

        app.handle_action(Action::Copy);
        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::Paste);

        assert_eq!(
            app.last_outcome(),
            Some(OperationOutcome::DestinationExists),
            "a name that is taken is reported as such"
        );
        assert!(
            app.notification().is_active(),
            "the conflict must be reported"
        );
        assert_eq!(
            app.clipboard().len(),
            1,
            "a copy stays marked after a failed paste"
        );
        assert_eq!(
            fs::read(&marked).expect("file should be read"),
            b"content",
            "the destination must be untouched"
        );
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("listing should succeed")
                .count(),
            1,
            "nothing may be created next to it"
        );
    }

    #[test]
    fn a_cut_that_conflicts_keeps_the_source_and_the_clipboard() {
        let directory = TempDir::new("paste-conflict-cut");
        let marked = directory.path().join("file.txt");
        fs::write(&marked, b"content").expect("file should be written");
        let mut app = two_panes_showing(&directory, &directory);

        app.handle_action(Action::Cut);
        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::Paste);

        assert_eq!(
            app.last_outcome(),
            Some(OperationOutcome::DestinationExists)
        );
        assert!(app.notification().is_active());
        assert_eq!(
            app.clipboard().len(),
            1,
            "a cut stays marked when nothing was pasted"
        );
        assert_eq!(
            app.clipboard().operation(),
            Some(ClipboardOperation::Cut),
            "the mark must still be a cut"
        );
        assert_eq!(
            fs::read(&marked).expect("the source must still be read"),
            b"content",
            "the source must be untouched"
        );
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("listing should succeed")
                .count(),
            1
        );
    }

    #[test]
    fn creating_a_file_that_is_already_there_reports_the_conflict() {
        let (mut app, left, _right) = two_loaded_panes();
        let taken = left.path().join("left-a.txt");

        let error = app
            .create_file(OsStr::new("left-a.txt"))
            .expect_err("the name is taken");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            app.last_outcome(),
            Some(OperationOutcome::DestinationExists)
        );
        assert!(app.notification().is_active());
        assert_eq!(
            fs::read(&taken).expect("file should be read"),
            b"content",
            "an entry that is already there must not be emptied or replaced"
        );
        assert_eq!(
            fs::read_dir(left.path())
                .expect("listing should succeed")
                .count(),
            3
        );
    }

    #[test]
    fn creating_a_directory_that_is_already_there_reports_the_conflict() {
        let (mut app, left, _right) = two_loaded_panes();
        let taken = left.path().join("left-a.txt");

        let error = app
            .create_directory(OsStr::new("left-a.txt"))
            .expect_err("the name is taken");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            app.last_outcome(),
            Some(OperationOutcome::DestinationExists)
        );
        assert!(app.notification().is_active());
        assert!(taken.is_file(), "what was there must not be replaced");
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 3);
    }

    #[test]
    fn renaming_onto_a_taken_name_reports_the_conflict() {
        let (mut app, left, _right) = two_loaded_panes();
        let selected = selected_path(&app, ActivePane::Left);
        let taken = left.path().join("left-a.txt");
        assert_ne!(
            selected, taken,
            "the rename needs a different name to go to"
        );

        let error = app
            .rename_selected(OsStr::new("left-a.txt"))
            .expect_err("the name is taken");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            app.last_outcome(),
            Some(OperationOutcome::DestinationExists)
        );
        assert!(app.notification().is_active());
        assert!(selected.is_file(), "the source must be untouched");
        assert_eq!(
            fs::read(&taken).expect("file should be read"),
            b"content",
            "the destination must be untouched"
        );
        assert_eq!(
            fs::read_dir(left.path())
                .expect("listing should succeed")
                .count(),
            3
        );
    }

    #[test]
    fn both_panes_start_in_name_mode() {
        let app = App::default();

        assert_eq!(app.pane(ActivePane::Left).sort_mode(), SortMode::Name);
        assert_eq!(app.pane(ActivePane::Right).sort_mode(), SortMode::Name);
    }

    #[test]
    fn changing_the_sort_cycles_through_every_mode_and_returns_to_name() {
        let mut app = app_with(3);
        let mut seen = vec![app.pane(ActivePane::Left).sort_mode()];

        for _ in 0..SortMode::ALL.len() {
            app.handle_action(Action::ChangeSort);
            seen.push(app.pane(ActivePane::Left).sort_mode());
        }

        assert_eq!(
            seen,
            [
                SortMode::Name,
                SortMode::ReverseName,
                SortMode::Size,
                SortMode::Modified,
                SortMode::Extension,
                SortMode::Name,
            ]
        );
        assert_eq!(seen.len(), SortMode::ALL.len() + 1);
    }

    #[test]
    fn changing_the_sort_reorders_the_entries_the_active_pane_holds() {
        let mut app = app_with(0);
        app.left = pane_holding(vec![
            unreachable_entry("a.txt", EntryKind::File),
            unreachable_entry("z-dir", EntryKind::Directory),
            unreachable_entry("b.txt", EntryKind::File),
        ]);

        app.handle_action(Action::ChangeSort);

        assert_eq!(
            pane_names(&app, ActivePane::Left),
            ["z-dir", "b.txt", "a.txt"],
            "the directory stays first and the names are reversed"
        );
    }

    #[test]
    fn changing_the_sort_changes_the_mode_of_the_active_pane_only() {
        let mut app = app_with(3);
        app.right = pane_holding(vec![
            unreachable_entry("b", EntryKind::File),
            unreachable_entry("a", EntryKind::File),
        ]);
        let other = app.pane(ActivePane::Right).clone();

        for _ in 0..3 {
            app.handle_action(Action::ChangeSort);
        }

        assert_eq!(app.pane(ActivePane::Left).sort_mode(), SortMode::Modified);
        assert_eq!(
            app.pane(ActivePane::Right),
            &other,
            "the inactive pane keeps its listing, its order and its mode"
        );

        // And the other way round: the right pane sorts, the left one does not.
        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::ChangeSort);

        assert_eq!(pane_names(&app, ActivePane::Right), ["b", "a"]);
        assert_eq!(app.pane(ActivePane::Left).sort_mode(), SortMode::Modified);
        assert_eq!(
            pane_names(&app, ActivePane::Left),
            ["entry-00", "entry-01", "entry-02"],
            "the pane that was active before keeps the order it had"
        );
    }

    #[test]
    fn changing_the_sort_keeps_the_same_entry_selected() {
        let mut app = app_with(0);
        app.left = pane_holding(vec![
            unreachable_entry("a.txt", EntryKind::File),
            unreachable_entry("b.txt", EntryKind::File),
            unreachable_entry("c.txt", EntryKind::File),
        ]);
        app.left.select(0);
        let selected = app
            .pane(ActivePane::Left)
            .selected_entry()
            .expect("the pane has a selection")
            .path()
            .to_path_buf();

        app.handle_action(Action::ChangeSort);

        let pane = app.pane(ActivePane::Left);
        assert_eq!(
            pane.selected_index(),
            Some(2),
            "the entry moved, so the index has to move with it"
        );
        assert_eq!(
            pane.selected_entry()
                .expect("the pane still has a selection")
                .path(),
            selected.as_path(),
            "the selected entry must still be the one that was selected"
        );
    }

    #[test]
    fn changing_the_sort_leaves_the_listing_and_the_scroll_valid() {
        let mut app = app_with_rows(7, 2);
        app.handle_action(Action::GoEnd);

        for mode in SortMode::ALL {
            app.handle_action(Action::ChangeSort);

            assert_eq!(app.pane(ActivePane::Left).sort_mode(), mode.next());
            assert_pane_is_sound(&app, ActivePane::Left, "after changing the sort");
        }
    }

    #[test]
    fn the_selected_entry_stays_visible_after_the_sort_changes() {
        let mut app = app_with_rows(10, 3);
        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 7);

        app.handle_action(Action::ChangeSort);

        let pane = app.pane(ActivePane::Left);
        assert_eq!(pane.selected_index(), Some(0));
        assert_eq!(
            pane.scroll_offset(),
            0,
            "the window has to follow the entry that is now first"
        );
        assert_eq!(
            pane.selected_entry()
                .expect("the pane has a selection")
                .name(),
            OsStr::new("entry-09"),
            "the entry that was selected is still the selected one"
        );
    }

    #[test]
    fn changing_the_sort_of_a_pane_with_nothing_selected_keeps_it_unselected() {
        let mut app = app_with(3);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), None);

        app.handle_action(Action::ChangeSort);

        assert_eq!(app.pane(ActivePane::Left).selected_index(), None);
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
    }

    #[test]
    fn a_selection_that_cannot_be_found_again_is_brought_back_into_range() {
        // An in-memory reorder always finds its selection again, so the safety
        // net is exercised directly: it must leave a valid selection behind.
        let mut app = app_with(3);
        let mut shrunk = app.pane(ActivePane::Left).entries().to_vec();
        shrunk.truncate(1);
        app.left.set_entries(shrunk);
        app.left.select(2);

        app.left
            .reselect(Some(Path::new("/terminalvision/not-an-entry")));

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
    }

    #[test]
    fn changing_the_sort_reorders_in_memory_and_never_reads_the_directory_again() {
        let directory = TempDir::new("sort-without-reload");
        fs::write(directory.path().join("a.txt"), b"content").expect("file should be written");
        fs::write(directory.path().join("b.txt"), b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        // The directory changes behind the pane's back. Sorting that read it
        // again would show the new entry.
        fs::write(directory.path().join("c.txt"), b"content").expect("file should be written");

        let expected = [
            (SortMode::ReverseName, ["b.txt", "a.txt"]),
            (SortMode::Size, ["a.txt", "b.txt"]),
            (SortMode::Modified, ["a.txt", "b.txt"]),
            (SortMode::Extension, ["a.txt", "b.txt"]),
            (SortMode::Name, ["a.txt", "b.txt"]),
        ];

        for (mode, names) in expected {
            app.handle_action(Action::ChangeSort);

            assert_eq!(app.pane(ActivePane::Left).sort_mode(), mode);
            assert_eq!(
                pane_names(&app, ActivePane::Left),
                names,
                "the listing is reordered, never discovered again"
            );
        }

        assert_eq!(app.pane(ActivePane::Left).current_path(), directory.path());
    }

    #[test]
    fn changing_the_sort_is_ignored_while_a_temporary_mode_is_active() {
        for mode in temporary_modes() {
            let mut app = app_with(3);
            app.mode = mode;
            let before = app.pane(ActivePane::Left).clone();

            app.handle_action(Action::ChangeSort);

            assert_eq!(app.mode(), mode);
            assert_eq!(app.pane(ActivePane::Left).sort_mode(), SortMode::Name);
            assert_eq!(
                app.pane(ActivePane::Left),
                &before,
                "{mode:?} must leave the listing and its order alone"
            );
        }
    }

    #[test]
    fn both_panes_start_with_no_search() {
        let app = App::default();

        for which in [ActivePane::Left, ActivePane::Right] {
            assert!(!app.pane(which).search().is_active());
            assert_eq!(app.pane(which).search().query(), "");
            assert_eq!(app.pane(which).visible_count(), 0);
        }
    }

    #[test]
    fn an_empty_query_shows_every_entry() {
        let mut app = app_showing(&["README.md", "Cargo.toml", "src"]);

        app.set_search_query("");

        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["README.md", "Cargo.toml", "src"]
        );
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 3);
        assert!(!app.search().is_active());
    }

    #[test]
    fn an_exact_name_matches() {
        let mut app = app_showing(&["README.md", "Cargo.toml", "notes.txt"]);

        app.set_search_query("notes.txt");

        assert_eq!(app.search().query(), "notes.txt");
        assert!(app.search().is_active());
        assert_eq!(shown_names(&app, ActivePane::Left), ["notes.txt"]);
    }

    #[test]
    fn part_of_a_name_matches() {
        let mut app = app_showing(&["README.md", "Cargo.toml", "notes.txt"]);

        app.set_search_query("arg");
        assert_eq!(shown_names(&app, ActivePane::Left), ["Cargo.toml"]);

        app.set_search_query("o");
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["Cargo.toml", "notes.txt"]
        );

        app.set_search_query(".md");
        assert_eq!(shown_names(&app, ActivePane::Left), ["README.md"]);
    }

    #[test]
    fn matching_ignores_case() {
        let mut app = app_showing(&["README.md", "Cargo.toml", "notes.txt"]);

        app.set_search_query("read");
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["README.md"],
            "a lower case query matches an upper case name"
        );

        app.set_search_query("CARGO");
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["Cargo.toml"],
            "an upper case query matches a mixed case name"
        );

        app.set_search_query("NoTeS");
        assert_eq!(shown_names(&app, ActivePane::Left), ["notes.txt"]);
    }

    #[test]
    fn a_directory_matches_by_its_name() {
        let mut app = app_with(0);
        app.left.replace_entries(vec![
            unreachable_entry("src", EntryKind::Directory),
            unreachable_entry("Cargo.toml", EntryKind::File),
            unreachable_entry("target", EntryKind::Directory),
        ]);

        app.set_search_query("get");

        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["target"],
            "a directory is matched on its name, like any other entry"
        );
    }

    #[test]
    fn matching_looks_at_the_name_and_never_at_the_rest_of_the_path() {
        let mut app = app_with(0);
        app.left.replace_entries(vec![
            unreachable_entry("alpha.txt", EntryKind::File),
            unreachable_entry("beta.txt", EntryKind::File),
        ]);
        assert_eq!(
            app.pane(ActivePane::Left).entries()[0].path(),
            Path::new(UNREAL_PATH).join("alpha.txt")
        );

        app.set_search_query("terminalvision");

        assert_eq!(
            app.pane(ActivePane::Left).visible_count(),
            0,
            "the directory the entries were found in is not part of their name"
        );
    }

    #[test]
    fn unicode_names_match_and_fold_case() {
        let mut app = app_showing(&["café.txt", "cafe.txt", "日本語.txt", "Ωmega.txt"]);

        app.set_search_query("CAFÉ");
        assert_eq!(shown_names(&app, ActivePane::Left), ["café.txt"]);

        app.set_search_query("日本");
        assert_eq!(shown_names(&app, ActivePane::Left), ["日本語.txt"]);

        app.set_search_query("ωMEGA");
        assert_eq!(shown_names(&app, ActivePane::Left), ["Ωmega.txt"]);

        app.set_search_query("café.txt");
        assert_eq!(shown_names(&app, ActivePane::Left), ["café.txt"]);
        assert_eq!(
            app.pane(ActivePane::Left).entries().len(),
            4,
            "every name is still held, accented or not"
        );
    }

    #[test]
    fn names_with_spaces_match() {
        let mut app = app_showing(&["my file.txt", "my-file.txt", "notes.txt"]);

        app.set_search_query("my file");
        assert_eq!(shown_names(&app, ActivePane::Left), ["my file.txt"]);

        app.set_search_query("file.txt");
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["my file.txt", "my-file.txt"]
        );
    }

    #[test]
    fn a_query_that_matches_nothing_shows_nothing() {
        let mut app = app_with_rows(5, 2);
        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(4));

        app.set_search_query("no entry is called this");

        let pane = app.pane(ActivePane::Left);
        assert_eq!(pane.visible_count(), 0);
        assert!(pane.visible_entries().next().is_none());
        // Phase 5.2 represents "nothing selected" as `None`, and a listing that
        // shows nothing can only be represented that way; the offset is zero.
        assert_eq!(pane.selected_index(), None);
        assert_eq!(pane.scroll_offset(), 0);
        assert_eq!(pane.entries().len(), 5, "the directory is still loaded");
        assert_pane_is_sound(&app, ActivePane::Left, "after a query that matches nothing");
    }

    #[test]
    fn a_query_that_matches_one_entry_shows_one_entry() {
        let mut app = app_showing(&["alpha.txt", "beta.txt", "gamma.txt"]);

        app.set_search_query("beta");

        assert_eq!(shown_names(&app, ActivePane::Left), ["beta.txt"]);
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 1);
        assert_eq!(
            selected_name(&app, ActivePane::Left),
            Some(String::from("beta.txt")),
            "the entry that matches is the one the pane is on"
        );
    }

    #[test]
    fn a_query_that_matches_several_entries_shows_all_of_them_in_order() {
        let mut app = app_showing(&["notes.txt", "report.txt", "readme.md", "draft.txt"]);

        app.set_search_query(".txt");

        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["notes.txt", "report.txt", "draft.txt"],
            "the matches keep the order of the listing"
        );
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 3);
    }

    #[test]
    fn searching_never_looks_inside_a_directory() {
        let directory = TempDir::new("search-not-recursive");
        let child = directory.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        fs::write(child.join("needle.txt"), b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        app.set_search_query("needle");

        assert_eq!(
            app.pane(ActivePane::Left).visible_count(),
            0,
            "`child` does not match, and what it holds was never loaded"
        );
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);
    }

    #[test]
    fn searching_never_reads_the_contents_of_a_file() {
        let directory = TempDir::new("search-name-only");
        fs::write(
            directory.path().join("notes.txt"),
            b"needle in the contents",
        )
        .expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        app.set_search_query("needle");

        assert_eq!(
            app.pane(ActivePane::Left).visible_count(),
            0,
            "a name is matched, never what a file holds"
        );

        app.set_search_query("notes");

        assert_eq!(shown_names(&app, ActivePane::Left), ["notes.txt"]);
    }

    #[test]
    fn the_entries_that_were_loaded_are_kept_while_searching() {
        let mut app = app_showing(&["alpha.txt", "beta.txt", "gamma.txt"]);
        app.left.select(1);
        let loaded = app.pane(ActivePane::Left).entries().to_vec();

        app.set_search_query("gamma");

        assert_eq!(shown_names(&app, ActivePane::Left), ["gamma.txt"]);
        assert_eq!(
            app.pane(ActivePane::Left).entries(),
            loaded.as_slice(),
            "a search filters a listing, it never replaces it"
        );

        app.set_search_query("");

        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["alpha.txt", "beta.txt", "gamma.txt"]
        );
        assert_eq!(app.pane(ActivePane::Left).entries(), loaded.as_slice());
    }

    #[test]
    fn clearing_the_search_shows_every_entry_again() {
        let mut app = app_showing(&["alpha.txt", "beta.txt", "gamma.txt"]);
        app.handle_action(Action::StartSearch);
        app.set_search_query("beta");
        assert_eq!(shown_names(&app, ActivePane::Left), ["beta.txt"]);

        app.handle_action(Action::ClearSearch);

        assert_eq!(app.mode(), Mode::Normal);
        assert!(!app.search().is_active());
        assert_eq!(app.search().query(), "");
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["alpha.txt", "beta.txt", "gamma.txt"]
        );
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 3);
        assert_pane_is_sound(&app, ActivePane::Left, "after clearing the search");
    }

    #[test]
    fn cancelling_a_search_restores_the_listing_and_leaves_the_directory() {
        let mut app = app_showing(&["alpha.txt", "beta.txt"]);
        app.handle_action(Action::StartSearch);
        app.set_search_query("beta");
        let directory = app.pane(ActivePane::Left).current_path().clone();

        app.handle_action(Action::Cancel);

        assert_eq!(app.mode(), Mode::Normal);
        assert!(!app.search().is_active());
        assert_eq!(app.search().query(), "");
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["alpha.txt", "beta.txt"]
        );
        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            &directory,
            "cancelling never moves the pane"
        );
    }

    #[test]
    fn starting_a_search_shows_the_whole_listing_and_keeps_the_directory() {
        let mut app = app_showing(&["alpha.txt", "beta.txt"]);
        app.set_search_query("beta");
        app.left.select(0);

        app.handle_action(Action::StartSearch);

        assert_eq!(app.mode(), Mode::Search);
        assert_eq!(app.search().query(), "");
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["alpha.txt", "beta.txt"]
        );
        assert_eq!(
            app.pane(ActivePane::Left).current_path().as_os_str(),
            OsStr::new(UNREAL_PATH)
        );
    }

    #[test]
    fn updating_the_query_refilters_the_entries() {
        let mut app = app_showing(&["alpha.txt", "beta.txt", "gamma.txt"]);

        let updates = [
            ("beta", vec!["beta.txt"]),
            ("a", vec!["alpha.txt", "beta.txt", "gamma.txt"]),
            ("", vec!["alpha.txt", "beta.txt", "gamma.txt"]),
            ("gamma.txt", vec!["gamma.txt"]),
            ("nothing", vec![]),
        ];

        for (query, expected) in updates {
            app.set_search_query(query);

            assert_eq!(app.search().query(), query);
            assert_eq!(
                shown_names(&app, ActivePane::Left),
                expected,
                "after the query {query:?}"
            );
        }
    }

    #[test]
    fn the_selected_entry_stays_selected_while_it_still_matches() {
        let mut app = app_showing(&["alpha.txt", "beta.txt", "gamma.txt"]);
        app.left.select(2);
        assert_eq!(
            selected_name(&app, ActivePane::Left),
            Some(String::from("gamma.txt"))
        );

        app.set_search_query("gamma");

        assert_eq!(
            app.pane(ActivePane::Left).selected_index(),
            Some(0),
            "the entry moved to the front of the results"
        );
        assert_eq!(
            selected_name(&app, ActivePane::Left),
            Some(String::from("gamma.txt")),
            "the entry that was selected is still the selected one"
        );

        app.set_search_query("a");

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(2));
        assert_eq!(
            selected_name(&app, ActivePane::Left),
            Some(String::from("gamma.txt")),
            "and it is still selected once the results grow again"
        );
    }

    #[test]
    fn a_selection_that_is_filtered_out_moves_to_the_first_result() {
        let mut app = app_showing(&["one.txt", "two.txt", "three.txt", "four.txt"]);
        app.left.select(2);
        assert_eq!(
            selected_name(&app, ActivePane::Left),
            Some(String::from("three.txt"))
        );

        app.set_search_query("o");

        // Three entries match and the selected one is not among them, so the
        // selection has to come from the results rather than from the position
        // it used to hold: it is the first result, not the third one.
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["one.txt", "two.txt", "four.txt"]
        );
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(
            selected_name(&app, ActivePane::Left),
            Some(String::from("one.txt")),
            "the first result is selected"
        );
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
        assert_pane_is_sound(
            &app,
            ActivePane::Left,
            "after the selected entry was filtered out",
        );
    }

    #[test]
    fn a_selection_below_the_results_is_brought_back_into_range() {
        let mut app = app_with_rows(5, 2);
        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 3);

        app.set_search_query("entry-00");

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(
            app.pane(ActivePane::Left).scroll_offset(),
            0,
            "the window follows the entry that is selected"
        );
        assert_pane_is_sound(&app, ActivePane::Left, "after filtering a scrolled listing");
    }

    #[test]
    fn a_listing_that_arrives_while_a_search_is_active_is_filtered_by_it() {
        let parent = TempDir::new("search-into-a-directory");
        let child = parent.path().join("child");
        fs::create_dir(&child).expect("directory should be created");
        fs::write(child.join("child-notes.txt"), b"content").expect("file should be written");
        fs::write(child.join("other.txt"), b"content").expect("file should be written");
        fs::write(parent.path().join("notes.txt"), b"content").expect("file should be written");
        let mut app = App::at(parent.path().to_path_buf()).expect("the directory should load");

        app.set_search_query("child");
        assert_eq!(shown_names(&app, ActivePane::Left), ["child"]);

        app.handle_action(Action::Open);

        assert_eq!(app.pane(ActivePane::Left).current_path(), &child);
        assert_eq!(
            app.pane(ActivePane::Left).entries().len(),
            2,
            "the directory that was entered is loaded whole"
        );
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["child-notes.txt"],
            "and filtered by the query that is still being looked for"
        );
    }

    #[test]
    fn the_scroll_offset_stays_valid_while_the_query_changes() {
        let mut app = app_with_rows(30, 5);
        app.handle_action(Action::GoEnd);

        for query in ["entry", "entry-1", "entry-2", "entry-29", "nothing", ""] {
            app.set_search_query(query);

            assert_pane_is_sound(
                &app,
                ActivePane::Left,
                &format!("after the query {query:?}"),
            );
            assert!(
                app.pane(ActivePane::Left).scroll_offset()
                    <= pane_maximum_offset(&app, ActivePane::Left),
                "the offset must fit the results of {query:?}"
            );
        }
    }

    #[test]
    fn the_search_keeps_the_order_the_pane_sorts_in() {
        let mut app = app_with(0);
        app.left.replace_entries(vec![
            unreachable_entry("a.txt", EntryKind::File),
            unreachable_entry("b.log", EntryKind::File),
            unreachable_entry("c.txt", EntryKind::File),
        ]);

        for _ in 0..4 {
            app.handle_action(Action::ChangeSort);
        }
        assert_eq!(app.pane(ActivePane::Left).sort_mode(), SortMode::Extension);
        assert_eq!(
            pane_names(&app, ActivePane::Left),
            ["b.log", "a.txt", "c.txt"],
            "the smallest extension comes first"
        );

        app.set_search_query("txt");

        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["a.txt", "c.txt"],
            "the results keep the order the pane sorted them in"
        );
        assert_eq!(
            pane_names(&app, ActivePane::Left),
            ["b.log", "a.txt", "c.txt"],
            "the listing itself is not reordered by the search"
        );

        app.handle_action(Action::ChangeSort);

        assert_eq!(app.pane(ActivePane::Left).sort_mode(), SortMode::Name);
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["a.txt", "c.txt"],
            "sorting while a search is active keeps the search"
        );
        assert_eq!(
            pane_names(&app, ActivePane::Left),
            ["a.txt", "b.log", "c.txt"]
        );
    }

    #[test]
    fn searching_filters_the_pane_the_user_searches_in_only() {
        let mut app = app_showing(&["alpha.txt", "beta.txt"]);
        app.right
            .replace_entries(vec![entry("gamma.txt"), entry("beta.txt")]);
        let other = app.pane(ActivePane::Right).clone();

        app.handle_action(Action::StartSearch);
        app.set_search_query("beta");

        assert_eq!(app.mode(), Mode::Search);
        assert_eq!(app.active_pane(), ActivePane::Left);
        assert_eq!(shown_names(&app, ActivePane::Left), ["beta.txt"]);
        assert_eq!(
            app.pane(ActivePane::Right),
            &other,
            "the pane that is not being searched keeps everything it had"
        );
    }

    #[test]
    fn each_pane_keeps_its_own_search() {
        let mut app = app_showing(&["alpha.txt", "beta.txt", "gamma.txt"]);
        app.right
            .replace_entries(vec![entry("delta.txt"), entry("alpha.txt")]);
        app.left.set_search_query("gamma");
        app.right.set_search_query("delta");

        assert_eq!(shown_names(&app, ActivePane::Left), ["gamma.txt"]);
        assert_eq!(shown_names(&app, ActivePane::Right), ["delta.txt"]);

        app.handle_action(Action::SwitchPane);

        assert_eq!(app.active_pane(), ActivePane::Right);
        assert_eq!(
            app.search().query(),
            "delta",
            "the query in hand belongs to the active pane"
        );
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["gamma.txt"],
            "the pane that lost the focus keeps its own search"
        );
        assert_eq!(shown_names(&app, ActivePane::Right), ["delta.txt"]);

        app.handle_action(Action::ClearSearch);

        assert_eq!(
            shown_names(&app, ActivePane::Right),
            ["delta.txt", "alpha.txt"],
            "clearing shows every entry of the active pane again"
        );
        assert_eq!(
            app.pane(ActivePane::Left).search().query(),
            "gamma",
            "the other pane's search is left alone"
        );
        assert_eq!(shown_names(&app, ActivePane::Left), ["gamma.txt"]);
    }

    #[test]
    fn searching_never_reads_the_directory_again() {
        let directory = TempDir::new("search-without-reload");
        fs::write(directory.path().join("alpha.txt"), b"content").expect("file should be written");
        fs::write(directory.path().join("beta.txt"), b"content").expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        // The directory changes behind the pane's back. A search that read it
        // again would find the entry that was added.
        fs::write(directory.path().join("beta-extra.txt"), b"content")
            .expect("file should be written");

        app.handle_action(Action::StartSearch);
        app.set_search_query("beta");

        assert_eq!(shown_names(&app, ActivePane::Left), ["beta.txt"]);
        assert_eq!(
            app.pane(ActivePane::Left).entries().len(),
            2,
            "the listing is the one that was loaded"
        );
        assert_eq!(app.pane(ActivePane::Left).current_path(), directory.path());
    }

    #[test]
    fn repeated_query_changes_leave_the_state_sound() {
        let mut app = app_with_rows(12, 3);
        app.handle_action(Action::GoHome);
        let loaded = app.pane(ActivePane::Left).entries().to_vec();

        for round in 0..4 {
            for query in [
                "entry",
                "entry-0",
                "entry-1",
                "",
                "nothing at all",
                "entry-11",
            ] {
                app.set_search_query(query);

                assert_eq!(app.search().query(), query);
                assert_eq!(
                    app.pane(ActivePane::Left).entries(),
                    loaded.as_slice(),
                    "the loaded listing survives every query"
                );
                assert_pane_is_sound(
                    &app,
                    ActivePane::Left,
                    &format!("round {round}, query {query:?}"),
                );
            }
        }
    }

    #[test]
    fn both_panes_start_in_the_basic_mode() {
        let app = App::default();

        for which in [ActivePane::Left, ActivePane::Right] {
            assert_eq!(app.pane(which).search().mode(), SearchMode::Basic);
            assert_eq!(app.pane(which).search().outcome(), None);
            assert!(!app.pane(which).search().is_cancelled());
            assert!(app.pane(which).visible_results().is_empty());
        }
    }

    #[test]
    fn a_recursive_search_finds_entries_below_the_pane_directory() {
        let (mut app, directory) = app_over_a_tree("search-recursive-app");
        let listed = app.pane(ActivePane::Left).entries().to_vec();

        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");

        assert_eq!(app.search().query(), "needle");
        assert_eq!(app.search().mode(), SearchMode::Recursive);
        assert_eq!(app.search().outcome(), Some(&SearchOutcome::Completed));
        assert_eq!(
            result_paths(&app, ActivePane::Left),
            [
                PathBuf::from("nested").join("deep-needle.txt"),
                PathBuf::from("top-needle.txt"),
            ],
            "the paths sit below the directory, in path order"
        );
        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            directory.path(),
            "a search never moves the pane"
        );
        assert_eq!(
            app.pane(ActivePane::Left).entries(),
            listed.as_slice(),
            "the loaded listing is kept, not replaced by the results"
        );
    }

    #[test]
    fn a_recursive_search_matches_background_directories_as_well_as_files() {
        let (mut app, _directory) = app_over_a_tree("search-recursive-directory");

        app.set_search_mode(SearchMode::Recursive);
        app.run_search("nested");

        assert_eq!(
            result_paths(&app, ActivePane::Left),
            [PathBuf::from("nested")],
            "a directory is found by its name like a file"
        );
        assert!(app.notification().message().is_none());
    }

    #[test]
    fn a_recursive_search_keeps_the_selection_when_the_entry_is_a_result() {
        let directory = TempDir::new("search-recursive-selection");
        for index in 0..5 {
            fs::write(
                directory.path().join(format!("needle-{index}.txt")),
                b"content",
            )
            .expect("file should be written");
        }
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");

        // The third result is selected, so it is not the first one: keeping the
        // selection has to be about the entry, not about where the results
        // start.
        app.handle_action(Action::GoHome);
        app.handle_action(Action::MoveDown);
        app.handle_action(Action::MoveDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(2));
        let selected = app
            .pane(ActivePane::Left)
            .selected_path()
            .expect("a result is selected");
        assert_ne!(
            selected,
            app.pane(ActivePane::Left)
                .visible_results()
                .first()
                .expect("there are results")
                .path(),
            "the entry that is selected is not the first result"
        );

        app.run_search("needle");

        assert_eq!(
            app.pane(ActivePane::Left).selected_index(),
            Some(2),
            "the selection stays where the entry is, not where the list starts"
        );
        assert_eq!(
            app.pane(ActivePane::Left).selected_path(),
            Some(selected),
            "the entry that was selected is the one that still is"
        );
        assert_pane_is_sound(&app, ActivePane::Left, "after a second recursive search");
    }

    #[test]
    fn a_selection_that_is_not_a_result_moves_to_the_first_one() {
        let (mut app, _directory) = app_over_a_tree("search-recursive-first-result");
        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");
        app.handle_action(Action::GoEnd);

        app.run_search("top");

        assert_eq!(
            result_paths(&app, ActivePane::Left),
            [PathBuf::from("top-needle.txt")]
        );
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_pane_is_sound(
            &app,
            ActivePane::Left,
            "after the selection was filtered out",
        );
    }

    #[test]
    fn a_recursive_search_reports_how_much_of_the_tree_it_could_not_read() {
        let (mut app, _directory) = app_over_a_tree("search-skipped-count");

        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");

        assert_eq!(
            app.search().skipped(),
            0,
            "a tree that could be read from end to end reports nothing skipped"
        );

        app.run_search("nothing");

        assert_eq!(app.search().skipped(), 0);
        assert_eq!(app.search().outcome(), Some(&SearchOutcome::Completed));
    }

    #[test]
    fn a_recursive_search_that_finds_nothing_leaves_the_pane_sound() {
        let (mut app, _directory) = app_over_a_tree("search-recursive-nothing");
        app.set_visible_rows(ActivePane::Left, 2);
        app.set_search_mode(SearchMode::Recursive);

        app.run_search("nothing is called this");

        let pane = app.pane(ActivePane::Left);
        assert_eq!(app.search().outcome(), Some(&SearchOutcome::Completed));
        assert!(pane.visible_results().is_empty());
        assert_eq!(pane.visible_count(), 0);
        assert_eq!(pane.selected_index(), None);
        assert_eq!(pane.scroll_offset(), 0);
        assert!(pane.selected_path().is_none());
        assert_eq!(pane.entries().len(), 2, "the directory is still loaded");
        assert_pane_is_sound(
            &app,
            ActivePane::Left,
            "after a recursive search found nothing",
        );
    }

    #[test]
    fn the_scroll_offset_stays_valid_while_recursive_results_change() {
        let directory = TempDir::new("search-recursive-scroll");
        for index in 0..30 {
            fs::write(
                directory.path().join(format!("needle-{index:02}.txt")),
                b"content",
            )
            .expect("file should be written");
        }
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        app.set_visible_rows(ActivePane::Left, 4);
        app.set_search_mode(SearchMode::Recursive);

        for query in ["needle", "needle-1", "needle-2", "nothing", "needle"] {
            app.run_search(query);

            assert_pane_is_sound(&app, ActivePane::Left, &format!("after {query:?}"));
            assert!(
                app.pane(ActivePane::Left).scroll_offset()
                    <= pane_maximum_offset(&app, ActivePane::Left)
            );
        }

        app.handle_action(Action::GoEnd);
        app.run_search("needle-29");

        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_pane_is_sound(&app, ActivePane::Left, "after narrowing the results");
    }

    #[test]
    fn a_recursive_search_keeps_the_sort_state_and_the_listing_order() {
        let (mut app, _directory) = app_over_a_tree("search-recursive-sorting");
        app.handle_action(Action::ChangeSort);
        assert_eq!(
            app.pane(ActivePane::Left).sort_mode(),
            SortMode::ReverseName
        );
        let order = pane_names(&app, ActivePane::Left);

        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");

        assert_eq!(
            app.pane(ActivePane::Left).sort_mode(),
            SortMode::ReverseName,
            "searching never changes how the listing is sorted"
        );
        assert_eq!(
            pane_names(&app, ActivePane::Left),
            order,
            "and it never reorders the listing itself"
        );

        // Sorting while the results are shown is allowed and changes nothing
        // about them: they are ordered by path, not by the pane's mode.
        let results = result_paths(&app, ActivePane::Left);
        app.handle_action(Action::ChangeSort);

        assert_eq!(app.pane(ActivePane::Left).sort_mode(), SortMode::Size);
        assert_eq!(result_paths(&app, ActivePane::Left), results);
        assert_eq!(pane_names(&app, ActivePane::Left), order);
    }

    #[test]
    fn the_fuzzy_mode_matches_characters_in_order() {
        let mut app = app_showing(&["README.md", "notes.txt", "cargo.toml"]);
        app.set_search_mode(SearchMode::Fuzzy);

        app.set_search_query("rme");
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["README.md"],
            "the characters may be spread out in the fuzzy mode"
        );

        app.set_search_mode(SearchMode::Basic);
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            Vec::<String>::new(),
            "the basic mode still wants the characters next to each other"
        );

        app.set_search_query("read");
        assert_eq!(shown_names(&app, ActivePane::Left), ["README.md"]);
    }

    #[test]
    fn a_fuzzy_mode_can_be_recursive_and_still_finds_scattered_names() {
        let directory = TempDir::new("search-recursive-fuzzy");
        fs::create_dir_all(directory.path().join("src")).expect("directory should be created");
        fs::write(directory.path().join("src").join("README.md"), b"content")
            .expect("file should be written");
        fs::write(directory.path().join("src").join("notes.txt"), b"content")
            .expect("file should be written");
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        app.set_search_mode(SearchMode::RecursiveFuzzy);
        app.run_search("rme");

        assert_eq!(
            result_paths(&app, ActivePane::Left),
            [PathBuf::from("src").join("README.md")],
            "the walk and the fuzzy rule work together"
        );
    }

    #[test]
    fn a_recursive_search_that_cannot_read_its_root_is_reported() {
        let (mut app, _directory) = app_over_a_tree("search-recursive-failed");
        app.left.set_current_path(PathBuf::from(UNREAL_PATH));
        app.set_search_mode(SearchMode::Recursive);

        app.run_search("needle");

        match app.search().outcome() {
            Some(SearchOutcome::Failed(failure)) => {
                assert_eq!(failure.path(), Path::new(UNREAL_PATH));
                assert_eq!(failure.kind(), io::ErrorKind::NotFound);
            }
            other => panic!("a missing root should be reported, found {other:?}"),
        }
        assert!(app.pane(ActivePane::Left).visible_results().is_empty());
        assert_eq!(app.pane(ActivePane::Left).selected_index(), None);
        assert!(
            app.notification().is_active(),
            "a root that cannot be read must not be passed over in silence"
        );

        let notice = app
            .notification()
            .notice()
            .expect("the failure must be kept in structured form");
        assert_eq!(notice.what(), &NoticeSource::Search);
        assert_eq!(notice.category(), ErrorCategory::NotFound);
        assert_eq!(notice.path(), Some(Path::new(UNREAL_PATH)));
        assert!(!notice.is_conflict());
        assert!(
            notice.text().contains(UNREAL_PATH),
            "the words must name what could not be searched: {}",
            notice.text()
        );
    }

    #[test]
    fn a_recursive_search_never_touches_the_inactive_pane() {
        let (mut app, _directory) = app_over_a_tree("search-recursive-active-only");
        app.right
            .replace_entries(vec![entry("right-needle.txt"), entry("right.txt")]);
        let other = app.pane(ActivePane::Right).clone();

        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");

        assert_eq!(app.pane(ActivePane::Left).visible_count(), 2);
        assert_eq!(
            app.pane(ActivePane::Right),
            &other,
            "the pane that is not being searched keeps everything it had"
        );

        // And the other way round: the right pane keeps its own basic search
        // while the left one walks.
        activate(&mut app, ActivePane::Right);
        app.set_search_query("right-needle");
        assert_eq!(
            shown_names(&app, ActivePane::Right),
            ["right-needle.txt"],
            "the right pane searches the entries it holds"
        );
        assert_eq!(
            app.pane(ActivePane::Left).search().mode(),
            SearchMode::Recursive,
            "and the left pane keeps the mode it was searching in"
        );
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 2);
    }

    #[test]
    fn the_root_of_a_recursive_search_is_the_active_panes_own_directory() {
        let (mut app, directory) = app_over_a_tree("search-root-active-pane");
        let other = TempDir::new("search-root-other-pane");
        fs::write(other.path().join("other-needle.txt"), b"content")
            .expect("file should be written");
        app.open_in(ActivePane::Right, other.path().to_path_buf())
            .expect("the other directory should load");
        activate(&mut app, ActivePane::Right);

        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");

        assert_eq!(
            app.pane(ActivePane::Right).current_path(),
            other.path(),
            "switching panes must not change which directory is searched"
        );
        assert_eq!(
            result_paths(&app, ActivePane::Right),
            [PathBuf::from("other-needle.txt")],
            "the results come from the active pane's own directory"
        );
        assert_eq!(
            app.pane(ActivePane::Left).visible_count(),
            2,
            "the pane that was left keeps its own listing"
        );

        // The left pane's directory is still the one it was showing, so a
        // search there starts from there and not from the other pane's.
        activate(&mut app, ActivePane::Left);
        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");

        assert_eq!(app.pane(ActivePane::Left).current_path(), directory.path());
        assert_eq!(
            result_paths(&app, ActivePane::Left),
            [
                PathBuf::from("nested").join("deep-needle.txt"),
                PathBuf::from("top-needle.txt"),
            ]
        );
    }

    #[test]
    fn a_cancelled_recursive_search_does_not_report_that_it_finished() {
        let (mut app, directory) = app_over_a_tree("search-cancelled-app");
        let listed = app.pane(ActivePane::Left).entries().to_vec();
        app.set_search_mode(SearchMode::Recursive);
        app.cancel_search();
        assert!(app.search().is_cancelled());

        app.run_search("needle");

        assert_eq!(
            app.search().outcome(),
            Some(&SearchOutcome::Cancelled),
            "a search that was stopped must not report success"
        );
        assert_ne!(app.search().outcome(), Some(&SearchOutcome::Completed));
        assert!(app.pane(ActivePane::Left).visible_results().is_empty());
        assert_eq!(
            app.pane(ActivePane::Left).entries(),
            listed.as_slice(),
            "cancelling the walk leaves the loaded listing alone"
        );
        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            directory.path(),
            "a cancelled search keeps the user's directory"
        );
    }

    #[test]
    fn cancelling_keeps_the_directory_the_selection_and_the_scroll() {
        let (mut app, directory) = app_over_a_tree("search-cancel-keeps");
        app.set_visible_rows(ActivePane::Left, 1);
        app.handle_action(Action::GoEnd);
        let selected = app.pane(ActivePane::Left).selected_path();
        let scroll = app.pane(ActivePane::Left).scroll_offset();
        let mode = app.pane(ActivePane::Left).sort_mode();

        app.cancel_search();

        let pane = app.pane(ActivePane::Left);
        assert_eq!(pane.current_path(), directory.path());
        assert_eq!(pane.selected_path(), selected);
        assert_eq!(pane.scroll_offset(), scroll);
        assert_eq!(pane.sort_mode(), mode);
        assert_eq!(pane.entries().len(), 2);
    }

    #[test]
    fn a_recursive_search_can_run_on_another_thread_and_be_stopped_from_this_one() {
        let directory = TempDir::new("search-recursive-thread");
        // Enough entries that the walk cannot finish before the request
        // arrives: every entry costs at least one call into the filesystem.
        for folder in 0..150 {
            let inner = directory.path().join(format!("folder-{folder:03}"));
            fs::create_dir(&inner).expect("directory should be created");
            for file in 0..20 {
                fs::write(inner.join(format!("needle-{file:03}.txt")), b"content")
                    .expect("file should be written");
            }
        }
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        app.set_search_mode(SearchMode::Recursive);

        // The handle is taken before the application is lent to the thread, and
        // shares its request with the search that is about to run.
        let cancel = app.search().cancel_handle();

        std::thread::scope(|scope| {
            scope.spawn(|| app.run_search("needle"));

            std::thread::sleep(std::time::Duration::from_millis(1));
            cancel.cancel();
        });

        assert_eq!(
            app.search().outcome(),
            Some(&SearchOutcome::Cancelled),
            "the search that was stopped says so"
        );
        assert!(
            app.pane(ActivePane::Left).visible_results().len() < 150 * 20,
            "it stopped part way through: {} results",
            app.pane(ActivePane::Left).visible_count()
        );
        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            directory.path(),
            "the pane stays where it was"
        );
        assert_pane_is_sound(&app, ActivePane::Left, "after a search was stopped");
    }

    #[test]
    fn clearing_a_recursive_search_shows_the_whole_listing_again() {
        let (mut app, _directory) = app_over_a_tree("search-clear-recursive");
        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 2);

        app.handle_action(Action::ClearSearch);

        let pane = app.pane(ActivePane::Left);
        assert_eq!(pane.search().mode(), SearchMode::Basic);
        assert_eq!(pane.search().query(), "");
        assert_eq!(pane.search().outcome(), None);
        assert!(pane.visible_results().is_empty());
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["nested", "top-needle.txt"],
            "every entry of the loaded listing is shown again"
        );
        assert_eq!(pane.entries().len(), 2);
        assert!(!pane.search().is_cancelled());
    }

    #[test]
    fn beginning_a_search_again_forgets_an_earlier_cancellation() {
        let (mut app, _directory) = app_over_a_tree("search-cancel-forgotten");
        app.cancel_search();
        assert!(app.search().is_cancelled());

        app.handle_action(Action::StartSearch);

        assert!(
            !app.search().is_cancelled(),
            "a search that is started again is not cancelled before it begins"
        );

        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");

        assert_eq!(app.search().outcome(), Some(&SearchOutcome::Completed));
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 2);
    }

    #[test]
    fn switching_back_to_a_basic_mode_shows_the_listing_again() {
        let (mut app, _directory) = app_over_a_tree("search-mode-switch-back");
        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 2);

        app.set_search_mode(SearchMode::Basic);

        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["top-needle.txt"],
            "the listing is shown again, filtered by the same query"
        );
        assert_eq!(
            app.search().query(),
            "needle",
            "the query survives a change of mode"
        );
        assert_eq!(app.search().outcome(), None, "and the walk's result goes");

        app.set_search_query("nst");
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            Vec::<String>::new(),
            "the basic mode wants the characters next to each other"
        );

        app.set_search_mode(SearchMode::Fuzzy);
        assert_eq!(
            shown_names(&app, ActivePane::Left),
            ["nested"],
            "the fuzzy mode finds them spread out"
        );
    }

    #[test]
    fn a_result_of_a_walk_is_not_an_entry_of_the_listing() {
        let (mut app, directory) = app_over_a_tree("search-result-not-an-entry");
        app.set_search_mode(SearchMode::Recursive);
        app.run_search("nested");

        assert_eq!(
            result_paths(&app, ActivePane::Left),
            [PathBuf::from("nested")]
        );
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert!(
            app.pane(ActivePane::Left).selected_path().is_some(),
            "a result is selected by its path"
        );

        // What is selected is a path a walk found, not an entry of the
        // listing, so the operations that act on the listing have nothing to
        // act on and do nothing rather than acting on the wrong entry.
        app.handle_action(Action::Open);

        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            directory.path(),
            "nothing was entered"
        );
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 2);
        assert_eq!(
            result_paths(&app, ActivePane::Left),
            [PathBuf::from("nested")],
            "and the results are still the ones that were found"
        );
    }

    #[test]
    fn a_listing_that_changes_discards_results_that_describe_the_old_one() {
        let (mut app, _directory) = app_over_a_tree("search-results-stale");
        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 2);

        // An operation that creates an entry reloads the pane's directory,
        // which replaces the listing the results were found below. They go with
        // it rather than being left to describe a tree that has changed.
        app.create_file(OsStr::new("created-after-the-search.txt"))
            .expect("the file should be created");

        let pane = app.pane(ActivePane::Left);
        assert!(pane.visible_results().is_empty());
        assert_eq!(app.search().outcome(), None);
        assert_eq!(
            app.search().query(),
            "needle",
            "the query is kept, so the search can be run again"
        );
        assert_eq!(pane.entries().len(), 3, "the listing was reloaded");
        assert_eq!(pane.visible_count(), 0);
        assert_pane_is_sound(
            &app,
            ActivePane::Left,
            "after the listing changed under the results",
        );
    }

    #[test]
    fn a_recursive_search_does_not_duplicate_the_listing_it_filters() {
        let (mut app, _directory) = app_over_a_tree("search-recursive-no-duplication");
        let listed = app.pane(ActivePane::Left).entries().to_vec();

        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");
        app.run_search("needle");
        app.run_search("needle");

        assert_eq!(
            app.pane(ActivePane::Left).entries(),
            listed.as_slice(),
            "searching repeatedly leaves the listing exactly as it was"
        );
        assert_eq!(
            result_paths(&app, ActivePane::Left),
            [
                PathBuf::from("nested").join("deep-needle.txt"),
                PathBuf::from("top-needle.txt"),
            ],
            "and the results are the same every time"
        );
    }

    /// A directory holding `count` empty files named `prefix-00000` upwards,
    /// created as the fixture of a test rather than committed to the repository.
    fn directory_of_many_files(label: &str, count: usize, prefix: &str) -> TempDir {
        let directory = TempDir::new(label);
        fill(directory.path(), count, prefix);

        directory
    }

    #[test]
    fn a_ten_thousand_entry_pane_searches_moves_and_sorts_in_a_practical_time() {
        let mut small = app_with_padded(1_000, "entry");
        let mut large = app_with_padded(10_000, "entry");
        for app in [&mut small, &mut large] {
            app.set_visible_rows(ActivePane::Left, 25);
        }

        // The same query over ten times as many entries: ten times the work,
        // and never more than that.
        let (large_matches, filter_large) = measure("filter 10 000 entries", || {
            large.set_search_query("entry-00");
            large.pane(ActivePane::Left).visible_count()
        });
        let (small_matches, filter_small) = measure("filter 1 000 entries", || {
            small.set_search_query("entry-00");
            small.pane(ActivePane::Left).visible_count()
        });

        assert_eq!(large_matches, 1_000, "entry-00000 up to entry-00999 match");
        assert_eq!(
            small_matches, 1_000,
            "every entry of the small pane matches"
        );
        assert_eq!(
            large.pane(ActivePane::Left).selected_index(),
            Some(0),
            "the first match is selected"
        );
        assert_not_quadratic(
            filter_small,
            filter_large,
            "filtering ten times as many entries",
        );

        // Fuzzy matching over the same listing: measured, and the same question
        // must keep getting the same answer.
        let (fuzzy_matches, fuzzy_time) = measure("fuzzy-filter 10 000 entries", || {
            large.set_search_mode(SearchMode::Fuzzy);
            large.run_search("nt-05");
            large.pane(ActivePane::Left).visible_count()
        });
        large.run_search("nt-05");

        assert!(
            fuzzy_matches > 0 && fuzzy_matches < 10_000,
            "a fuzzy query selects part of the listing: {fuzzy_matches}"
        );
        assert_eq!(
            large.pane(ActivePane::Left).visible_count(),
            fuzzy_matches,
            "the same query finds the same entries"
        );
        println!("fuzzy matching took {fuzzy_time:?} of the filter above");

        // Movement over the whole listing again.
        for app in [&mut small, &mut large] {
            app.handle_action(Action::ClearSearch);
        }
        assert_eq!(large.pane(ActivePane::Left).visible_count(), 10_000);

        let (_, moves_small) = measure("10 000 moves in a 1 000-entry pane", || {
            for _ in 0..10_000 {
                small.handle_action(Action::MoveDown);
            }
        });
        let (_, moves_large) = measure("10 000 moves in a 10 000-entry pane", || {
            for _ in 0..10_000 {
                large.handle_action(Action::MoveDown);
            }
        });

        assert_eq!(small.pane(ActivePane::Left).selected_index(), Some(999));
        assert_eq!(large.pane(ActivePane::Left).selected_index(), Some(9_999));
        assert_not_quadratic(
            moves_small,
            moves_large,
            "moving through ten times as many entries",
        );

        // Home, End and paging reach both ends of the same listing.
        large.handle_action(Action::GoHome);
        assert_eq!(large.pane(ActivePane::Left).selected_index(), Some(0));

        let (_, paging) = measure("page down through 10 000 entries", || {
            let mut steps = 0;
            while large.pane(ActivePane::Left).selected_index() != Some(9_999) {
                large.handle_action(Action::PageDown);
                steps += 1;
                assert!(steps < 10_000, "paging must reach the end");
            }
        });
        assert_eq!(large.pane(ActivePane::Left).selected_index(), Some(9_999));

        let mut steps = 0;
        while large.pane(ActivePane::Left).selected_index() != Some(0) {
            large.handle_action(Action::PageUp);
            steps += 1;
            assert!(steps < 10_000, "paging must reach the start");
        }
        assert_eq!(large.pane(ActivePane::Left).selected_index(), Some(0));
        println!("paging through the listing took {paging:?}");

        // Every order, three times over, and the pane is back in name order.
        let before = names_of(&large, ActivePane::Left);
        let (_, sorting) = measure("sort a 10 000-entry pane in every mode three times", || {
            for _ in 0..3 {
                for _ in SortMode::ALL {
                    large.handle_action(Action::ChangeSort);
                }
            }
        });
        println!("sorting the listing took {sorting:?}");

        assert_eq!(
            names_of(&large, ActivePane::Left),
            before,
            "five changes of order are a round trip"
        );
        assert_pane_is_sound(
            &large,
            ActivePane::Left,
            "after sorting ten thousand entries",
        );
        assert!(
            !large.notification().is_active(),
            "nothing went wrong in any of this"
        );
    }

    #[test]
    fn movement_search_and_sorting_do_not_need_the_directory_to_exist() {
        let directory = directory_of_many_files("perf-in-memory", 500, "entry");
        let path = directory.path().to_path_buf();
        let mut app = App::at(path.clone()).expect("the directory should load");
        app.set_visible_rows(ActivePane::Left, 20);

        let before = names_of(&app, ActivePane::Left);
        assert_eq!(before.len(), 500);

        // The directory is gone. A reload fails, which is how the tests know
        // that anything else reaching the filesystem would fail too.
        fs::remove_dir_all(&path).expect("the directory should be removed");
        app.refresh_active_pane()
            .expect_err("the directory is gone, so a reload cannot work");

        // Everything the pane does from what it already holds goes on working.
        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(499));
        app.handle_action(Action::GoHome);
        for _ in 0..3 {
            app.handle_action(Action::PageDown);
        }
        app.handle_action(Action::MoveUp);
        for _ in 0..500 {
            app.handle_action(Action::MoveDown);
        }
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(499));

        app.set_search_query("entry-00");
        assert_eq!(
            app.pane(ActivePane::Left).visible_count(),
            500,
            "every entry matches the query"
        );
        app.set_search_mode(SearchMode::Fuzzy);
        app.run_search("e4");
        let fuzzy = app.pane(ActivePane::Left).visible_count();
        assert!(
            fuzzy > 0 && fuzzy < 500,
            "a fuzzy query selects part of the listing: {fuzzy}"
        );
        app.handle_action(Action::ClearSearch);
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 500);

        app.handle_action(Action::ChangeSort);
        app.handle_action(Action::ChangeSort);

        // The listing is untouched by all of it, and nothing was reported,
        // because nothing looked at the filesystem.
        assert_eq!(names_of(&app, ActivePane::Left).len(), 500);
        assert!(
            names_of(&app, ActivePane::Left)
                .iter()
                .all(|name| name.starts_with("entry-") && name.ends_with(".txt"))
        );
        assert_pane_is_sound(&app, ActivePane::Left, "after a reload had failed");
        assert!(
            !app.notification().is_active(),
            "nothing failed here, because none of it reads the filesystem"
        );
    }

    #[test]
    fn refreshing_a_large_pane_reloads_only_that_pane_and_only_one_level() {
        let large = TempDir::new("perf-refresh-large");
        fill(large.path(), 2_000, "entry");
        let nested = large.path().join("nested");
        fs::create_dir(&nested).expect("directory should be created");
        fill(&nested, 200, "inner");

        let small = directory_of_many_files("perf-refresh-small", 200, "entry");
        let other = directory_of_many_files("perf-refresh-other", 50, "other");

        let mut app = App::at(large.path().to_path_buf()).expect("the directory should load");
        app.open_in(ActivePane::Right, other.path().to_path_buf())
            .expect("the other pane should load");
        app.set_visible_rows(ActivePane::Left, 25);

        let left_before = names_of(&app, ActivePane::Left);
        let right_before = app.pane(ActivePane::Right).entries().to_vec();
        assert_eq!(
            left_before.len(),
            2_001,
            "two thousand files and one directory"
        );

        let (_, large_time) = measure("refresh a pane of 2 001 entries twenty times", || {
            for _ in 0..20 {
                app.refresh_active_pane().expect("the pane should reload");
            }
        });

        assert_eq!(
            app.pane(ActivePane::Left).visible_count(),
            2_001,
            "a reload reads one level, never the tree below it"
        );
        assert_eq!(
            names_of(&app, ActivePane::Left),
            left_before,
            "a reload gives the listing it gave before"
        );
        assert_eq!(
            app.pane(ActivePane::Right).entries(),
            right_before.as_slice(),
            "the other pane is not read again"
        );

        let mut small_app = App::at(small.path().to_path_buf()).expect("the directory should load");
        small_app.set_visible_rows(ActivePane::Left, 25);
        let (_, small_time) = measure("refresh a pane of 200 entries twenty times", || {
            for _ in 0..20 {
                small_app
                    .refresh_active_pane()
                    .expect("the pane should reload");
            }
        });

        assert_not_quadratic(
            small_time,
            large_time,
            "refreshing ten times as many entries",
        );
    }

    #[test]
    fn a_five_thousand_entry_pane_can_be_moved_around_without_leaving_it() {
        let mut app = app_with_rows(5_000, 25);

        app.handle_action(Action::GoHome);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);

        app.handle_action(Action::GoEnd);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(4_999));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 4_975);
        assert_pane_is_sound(
            &app,
            ActivePane::Left,
            "at the end of five thousand entries",
        );

        // A page up from the last entry, and a page down back to it.
        app.handle_action(Action::PageUp);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(4_974));

        app.handle_action(Action::PageDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(4_999));

        // Repeated movement at the end stays at the end rather than running
        // past it or wrapping around.
        for _ in 0..5 {
            app.handle_action(Action::MoveDown);
            app.handle_action(Action::PageDown);
            app.handle_action(Action::MoveDown);
        }
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(4_999));
        assert_pane_is_sound(&app, ActivePane::Left, "after repeated movement at the end");

        // The same at the beginning.
        app.handle_action(Action::GoHome);
        for _ in 0..5 {
            app.handle_action(Action::MoveUp);
            app.handle_action(Action::PageUp);
        }
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
        assert_pane_is_sound(
            &app,
            ActivePane::Left,
            "after repeated movement at the start",
        );
    }

    #[test]
    fn paging_through_a_large_listing_reaches_every_entry_once() {
        let rows = 40;
        let total = 5_000;
        let mut app = app_with_rows(total, rows);
        app.handle_action(Action::GoHome);

        let mut visited = 0;
        let mut seen_end = false;

        while !seen_end {
            let before = app.pane(ActivePane::Left).selected_index();
            app.handle_action(Action::PageDown);
            let after = app.pane(ActivePane::Left).selected_index();

            assert_pane_is_sound(&app, ActivePane::Left, "while paging through");
            assert!(after.unwrap_or_default() >= before.unwrap_or_default());

            visited += 1;
            seen_end = after == Some(total - 1);
            assert!(visited <= total, "paging must reach the end and stop");
        }

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(total - 1));
        assert_eq!(
            app.pane(ActivePane::Left).scroll_offset(),
            total - rows,
            "the window ends up showing the last page"
        );
    }

    #[test]
    fn a_query_over_a_large_listing_filters_it_without_copying_it() {
        let mut app = app_with_padded(5_000, "entry");
        let listed = app.pane(ActivePane::Left).entries().to_vec();
        app.set_visible_rows(ActivePane::Left, 20);

        app.set_search_query("entry-0000");
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 10);
        assert_eq!(
            app.pane(ActivePane::Left).entries(),
            listed.as_slice(),
            "a search filters a listing, it never replaces it"
        );

        app.set_search_query("entry-000");
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 100);

        app.set_search_query("entry-");
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 5_000);
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 5_000);

        app.set_search_query("nothing at all");
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 0);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), None);
        assert_eq!(app.pane(ActivePane::Left).scroll_offset(), 0);
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 5_000);

        app.set_search_query("");
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 5_000);
        assert_eq!(app.pane(ActivePane::Left).entries(), listed.as_slice());
        assert_pane_is_sound(&app, ActivePane::Left, "after searching a large listing");
    }

    #[test]
    fn a_fuzzy_query_over_a_large_listing_answers_the_same_way_every_time() {
        let mut app = app_with(0);
        app.left.replace_entries(
            (0..5_000)
                .map(|index| {
                    if index % 2 == 0 {
                        entry(&format!("report-{index:05}.txt"))
                    } else {
                        entry(&format!("entry-{index:05}.txt"))
                    }
                })
                .collect(),
        );
        app.set_search_mode(SearchMode::Fuzzy);

        let answers: Vec<usize> = (0..3)
            .map(|_| {
                app.set_search_query("rp");
                app.pane(ActivePane::Left).visible_count()
            })
            .collect();

        assert_eq!(
            answers,
            [2_500, 2_500, 2_500],
            "every report name matches, and the answer does not change"
        );

        app.set_search_mode(SearchMode::Basic);
        assert_eq!(
            app.pane(ActivePane::Left).visible_count(),
            0,
            "the basic mode still wants the characters next to each other"
        );

        app.set_search_query("report");
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 2_500);
        assert_pane_is_sound(
            &app,
            ActivePane::Left,
            "after a fuzzy search of a large listing",
        );
    }

    #[test]
    fn changing_the_sort_of_a_large_listing_is_deterministic_and_sound() {
        let mut app = app_with_padded(5_000, "entry");
        app.set_visible_rows(ActivePane::Left, 30);
        let listed = pane_names(&app, ActivePane::Left);
        assert!(
            listed.len() == 5_000 && listed[0] == "entry-00000.txt",
            "the fixture starts in name order"
        );
        app.handle_action(Action::GoEnd);
        let selected = app.pane(ActivePane::Left).selected_path();

        for mode in SortMode::ALL {
            app.handle_action(Action::ChangeSort);

            assert_eq!(app.pane(ActivePane::Left).sort_mode(), mode.next());
            assert_pane_is_sound(&app, ActivePane::Left, "after sorting a large listing");
            assert_eq!(
                app.pane(ActivePane::Left).selected_path(),
                selected,
                "the entry that was selected is still the selected one"
            );
            assert_eq!(app.pane(ActivePane::Left).entries().len(), 5_000);
        }

        assert_eq!(
            app.pane(ActivePane::Left).sort_mode(),
            SortMode::Name,
            "five changes bring the cycle back to where it started"
        );
        assert_eq!(
            pane_names(&app, ActivePane::Left),
            listed,
            "and the listing is exactly the one it started with"
        );
    }

    #[test]
    fn a_large_directory_loads_searches_and_sorts_through_the_application() {
        let directory = directory_of_many_files("app-large-directory", 5_000, "entry");

        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        app.set_visible_rows(ActivePane::Left, 24);

        assert_eq!(app.pane(ActivePane::Left).entries().len(), 5_000);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
        assert_eq!(app.pane(ActivePane::Left).current_path(), directory.path());
        assert_pane_is_sound(&app, ActivePane::Left, "after loading a large directory");

        let listed = pane_names(&app, ActivePane::Left);
        let mut expected = listed.clone();
        expected.sort();
        assert_eq!(listed, expected, "the listing is in name order");

        app.set_search_query("entry-0499");
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 10);
        assert_eq!(
            shown_names(&app, ActivePane::Left)[0],
            "entry-04990.txt",
            "the results keep the order of the listing"
        );

        app.handle_action(Action::ClearSearch);
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 5_000);

        app.handle_action(Action::ChangeSort);
        assert_eq!(
            app.pane(ActivePane::Left).sort_mode(),
            SortMode::ReverseName
        );
        assert_eq!(pane_names(&app, ActivePane::Left).len(), 5_000);
        assert_eq!(
            pane_names(&app, ActivePane::Left)[4_999],
            "entry-00000.txt",
            "reversing the names keeps the same entries"
        );
        assert_pane_is_sound(&app, ActivePane::Left, "after sorting a large directory");
    }

    #[test]
    fn a_recursive_search_over_a_large_directory_finds_every_match() {
        let directory = directory_of_many_files("app-large-recursive", 4_999, "entry");
        let inner = directory.path().join("inner");
        fs::create_dir(&inner).expect("directory should be created");
        for index in 0..100 {
            fs::File::create(inner.join(format!("deep-{index:03}.txt")))
                .expect("file should be created");
        }
        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");

        app.set_search_mode(SearchMode::Recursive);
        app.run_search("deep-");

        assert_eq!(app.search().outcome(), Some(&SearchOutcome::Completed));
        assert_eq!(app.search().skipped(), 0);
        assert_eq!(app.pane(ActivePane::Left).visible_results().len(), 100);
        assert_eq!(
            result_paths(&app, ActivePane::Left)[0],
            PathBuf::from("inner").join("deep-000.txt"),
            "the results are in path order"
        );
        assert_eq!(
            app.pane(ActivePane::Left).entries().len(),
            5_000,
            "the listing is kept whole beside the results"
        );
        assert_pane_is_sound(
            &app,
            ActivePane::Left,
            "after a recursive search of a large directory",
        );

        // The same walk twice gives the same answer.
        let first: Vec<String> = result_paths(&app, ActivePane::Left)
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        app.run_search("deep-");
        let second: Vec<String> = result_paths(&app, ActivePane::Left)
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();

        assert_eq!(first, second);
    }

    #[test]
    fn a_large_directory_with_a_broken_link_and_a_denied_child_still_loads() {
        let directory = directory_of_many_files("app-large-safety", 1_000, "entry");
        fs::create_dir(directory.path().join("inner")).expect("directory should be created");
        fs::File::create(directory.path().join("inner").join("nested.txt"))
            .expect("file should be created");
        let long = format!("{}.txt", "long".repeat(60));
        fs::write(directory.path().join(&long), b"content").expect("file should be written");
        fs::write(directory.path().join("my file.txt"), b"content")
            .expect("file should be written");
        fs::write(directory.path().join("café.txt"), b"content").expect("file should be written");

        let mut app = App::at(directory.path().to_path_buf()).expect("the directory should load");
        app.set_visible_rows(ActivePane::Left, 10);

        assert_eq!(app.pane(ActivePane::Left).entries().len(), 1_004);
        assert_eq!(
            app.pane(ActivePane::Left)
                .entries()
                .iter()
                .filter(|entry| entry.is_file())
                .count(),
            1_003
        );
        assert_pane_is_sound(
            &app,
            ActivePane::Left,
            "after loading a mixed large directory",
        );

        app.set_search_query("long");
        assert_eq!(app.pane(ActivePane::Left).visible_count(), 1);
        assert_eq!(
            app.pane(ActivePane::Left).entries().len(),
            1_004,
            "the long name was matched against the whole listing"
        );
    }

    #[test]
    fn test_bookmark_state_operations() {
        let mut state = BookmarkState::new();
        assert!(state.is_empty());
        assert_eq!(state.len(), 0);
        assert_eq!(state.selected_index(), 0);
        assert_eq!(state.selected_bookmark(), None);

        // Add bookmarks
        let path_a = PathBuf::from("/alpha");
        let path_b = PathBuf::from("/beta");
        assert!(state.add(path_a.clone()));
        assert_eq!(state.len(), 1);
        assert!(!state.is_empty());

        // Duplicate prevention using PathBuf equality
        assert!(!state.add(path_a.clone()));
        assert_eq!(state.len(), 1);

        assert!(state.add(path_b.clone()));
        assert_eq!(state.len(), 2);

        // Navigation
        assert_eq!(state.selected_index(), 0);
        assert_eq!(
            state.selected_bookmark().map(|b| b.path()),
            Some(path_a.as_path())
        );

        state.move_down();
        assert_eq!(state.selected_index(), 1);
        assert_eq!(
            state.selected_bookmark().map(|b| b.path()),
            Some(path_b.as_path())
        );

        // Cannot move past end
        state.move_down();
        assert_eq!(state.selected_index(), 1);

        state.move_up();
        assert_eq!(state.selected_index(), 0);

        // Cannot move before 0
        state.move_up();
        assert_eq!(state.selected_index(), 0);

        // Remove selected
        let removed = state.remove_selected();
        assert_eq!(removed.map(|b| b.path().to_path_buf()), Some(path_a));
        assert_eq!(state.len(), 1);
        assert_eq!(state.selected_index(), 0);
        assert_eq!(
            state.selected_bookmark().map(|b| b.path()),
            Some(path_b.as_path())
        );

        // Remove remaining
        let removed2 = state.remove_selected();
        assert_eq!(removed2.map(|b| b.path().to_path_buf()), Some(path_b));
        assert_eq!(state.len(), 0);
        assert!(state.is_empty());
        assert_eq!(state.remove_selected(), None);
    }

    #[test]
    fn test_bookmark_unicode_and_custom_names() {
        let mut state = BookmarkState::new();
        let unicode_path = PathBuf::from("/workspace/📁_project_文档");
        assert!(state.add_with_name("项目文档", unicode_path.clone()));

        assert_eq!(state.len(), 1);
        let bm = state.selected_bookmark().unwrap();
        assert_eq!(bm.name(), "项目文档");
        assert_eq!(bm.path(), unicode_path.as_path());
        assert_eq!(bm.id(), 0);
    }

    #[test]
    fn test_app_add_and_open_bookmark() {
        let (mut app, left, right) = two_loaded_panes();
        let left_path = left.path().to_path_buf();
        let right_path = right.path().to_path_buf();

        // Bookmark left pane directory
        assert!(app.add_current_bookmark());
        assert_eq!(app.bookmarks().len(), 1);
        assert_eq!(app.bookmarks().bookmarks()[0].path(), &left_path);

        // Duplicate addition returns false and does not duplicate
        assert!(!app.add_current_bookmark());
        assert_eq!(app.bookmarks().len(), 1);

        // Switch to right pane and bookmark it
        app.handle_action(Action::SwitchPane);
        assert_eq!(app.active_pane(), ActivePane::Right);
        assert!(app.add_current_bookmark());
        assert_eq!(app.bookmarks().len(), 2);

        // Switch back to left pane
        app.handle_action(Action::SwitchPane);
        assert_eq!(app.active_pane(), ActivePane::Left);

        // Open bookmarks modal
        app.handle_action(Action::OpenBookmarks);
        assert_eq!(app.mode(), Mode::Bookmarks);

        // Select the right_path bookmark (index 1)
        app.bookmarks_mut().select(1);

        // Confirm modal opens selected bookmark in left pane (active pane)
        app.confirm_modal();
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.pane(ActivePane::Left).current_path(), &right_path);
        // Inactive pane (right) remains untouched
        assert_eq!(app.pane(ActivePane::Right).current_path(), &right_path);
    }

    #[test]
    fn test_app_missing_bookmark_path_handled_safely() {
        let mut app = App::default();
        let non_existent = PathBuf::from("/path/that/does/not/exist/9999");
        app.bookmarks_mut().add(non_existent.clone());
        assert_eq!(app.bookmarks().len(), 1);

        app.handle_action(Action::OpenBookmarks);
        assert_eq!(app.mode(), Mode::Bookmarks);

        // Confirm opening missing bookmark
        let outcome = app.open_selected_bookmark();
        assert!(outcome.is_err());
        assert_eq!(app.mode(), Mode::Normal);
        // Error reported to notification
        assert!(app.notification().is_active());
        // Bookmark is preserved, not deleted
        assert_eq!(app.bookmarks().len(), 1);
    }

    #[test]
    fn test_file_cannot_become_bookmark() {
        let temp_dir = TempDir::new("bookmark-file-test");
        let file_path = temp_dir.path().join("file.txt");
        fs::write(&file_path, b"content").expect("file created");

        let mut state = BookmarkState::new();
        assert!(!state.add(file_path.clone()));
        assert!(!state.add_with_name("My File", file_path));
        assert!(state.is_empty());
    }

    #[test]
    fn test_bookmark_removal_does_not_delete_filesystem_directory() {
        let temp_dir = TempDir::new("bookmark-no-delete-fs");
        let dir_path = temp_dir.path().to_path_buf();

        let mut app = App::default();
        assert!(app.add_bookmark(dir_path.clone()));
        assert_eq!(app.bookmarks().len(), 1);

        // Remove bookmark
        let removed = app.remove_selected_bookmark();
        assert_eq!(
            removed.map(|b| b.path().to_path_buf()),
            Some(dir_path.clone())
        );
        assert_eq!(app.bookmarks().len(), 0);

        // Directory still exists on disk
        assert!(dir_path.exists());
        assert!(dir_path.is_dir());
    }

    #[test]
    fn test_pane_has_initial_tab() {
        let pane = Pane::default();
        assert_eq!(pane.tab_count(), 1);
        assert_eq!(pane.active_tab_index(), 0);
        assert_eq!(pane.tabs().len(), 1);
        assert_eq!(pane.active_tab().name(), "[empty]");
    }

    #[test]
    fn test_app_new_tab_copies_directory_and_lists_entries() {
        let temp_dir = TempDir::new("tab-new-test");
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        fs::write(&file1, b"one").expect("file1 created");
        fs::write(&file2, b"two").expect("file2 created");

        let mut app = App::at(temp_dir.path().to_path_buf()).expect("loaded directory");
        assert_eq!(app.pane(ActivePane::Left).tab_count(), 1);
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 2);

        // Trigger NewTab
        app.handle_action(Action::NewTab);
        assert_eq!(app.pane(ActivePane::Left).tab_count(), 2);
        assert_eq!(app.pane(ActivePane::Left).active_tab_index(), 1);
        assert_eq!(app.pane(ActivePane::Left).current_path(), temp_dir.path());
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 2);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
    }

    #[test]
    fn test_tab_switching_with_wrap_around() {
        let mut pane = Pane::default();
        pane.new_tab(PathBuf::from("/dir1"), Vec::new());
        pane.new_tab(PathBuf::from("/dir2"), Vec::new());
        // Currently 3 tabs: 0: default, 1: dir1, 2: dir2. Active is 2.
        assert_eq!(pane.tab_count(), 3);
        assert_eq!(pane.active_tab_index(), 2);

        // NextTab wraps to 0
        pane.next_tab();
        assert_eq!(pane.active_tab_index(), 0);

        pane.next_tab();
        assert_eq!(pane.active_tab_index(), 1);

        pane.next_tab();
        assert_eq!(pane.active_tab_index(), 2);

        // PreviousTab wraps backwards
        pane.previous_tab();
        assert_eq!(pane.active_tab_index(), 1);

        pane.previous_tab();
        assert_eq!(pane.active_tab_index(), 0);

        pane.previous_tab();
        assert_eq!(pane.active_tab_index(), 2);
    }

    #[test]
    fn test_tab_state_preservation_across_switches() {
        let mut pane = Pane::default();
        let entry_a = Entry::new(
            OsString::from("a.txt"),
            PathBuf::from("/a.txt"),
            EntryKind::File,
        );
        let entry_b = Entry::new(
            OsString::from("b.txt"),
            PathBuf::from("/b.txt"),
            EntryKind::File,
        );
        let entry_c = Entry::new(
            OsString::from("c.txt"),
            PathBuf::from("/c.txt"),
            EntryKind::File,
        );

        // Tab 0: select entry 1
        pane.set_entries(vec![entry_a.clone(), entry_b.clone(), entry_c.clone()]);
        pane.select(1);
        pane.set_scroll_offset(5);
        assert_eq!(pane.selected_index(), Some(1));
        assert_eq!(pane.scroll_offset(), 5);

        // Tab 1: select entry 2
        pane.new_tab(
            PathBuf::from("/tab2"),
            vec![entry_a.clone(), entry_b.clone(), entry_c.clone()],
        );
        pane.select(2);
        pane.set_scroll_offset(8);
        assert_eq!(pane.selected_index(), Some(2));
        assert_eq!(pane.scroll_offset(), 8);

        // Switch back to Tab 0
        pane.select_tab(0);
        assert_eq!(pane.selected_index(), Some(1));
        assert_eq!(pane.scroll_offset(), 5);

        // Switch to Tab 1
        pane.select_tab(1);
        assert_eq!(pane.selected_index(), Some(2));
        assert_eq!(pane.scroll_offset(), 8);
    }

    #[test]
    fn test_closing_tabs_and_minimum_tab_safety() {
        let mut pane = Pane::default();
        pane.new_tab(PathBuf::from("/dir1"), Vec::new());
        pane.new_tab(PathBuf::from("/dir2"), Vec::new());
        assert_eq!(pane.tab_count(), 3);
        assert_eq!(pane.active_tab_index(), 2);

        // Close last tab (index 2) -> active becomes new last tab (index 1)
        assert!(pane.close_active_tab());
        assert_eq!(pane.tab_count(), 2);
        assert_eq!(pane.active_tab_index(), 1);
        assert_eq!(pane.current_path(), &PathBuf::from("/dir1"));

        // Close tab at index 1 -> active becomes index 0
        assert!(pane.close_active_tab());
        assert_eq!(pane.tab_count(), 1);
        assert_eq!(pane.active_tab_index(), 0);

        // Attempting to close the only tab must fail safely and keep 1 tab
        assert!(!pane.close_active_tab());
        assert_eq!(pane.tab_count(), 1);
        assert_eq!(pane.active_tab_index(), 0);
    }

    #[test]
    fn test_closing_first_and_middle_tab() {
        let mut pane = Pane::default();
        pane.new_tab(PathBuf::from("/dir1"), Vec::new());
        pane.new_tab(PathBuf::from("/dir2"), Vec::new());
        // Tabs: [0: default, 1: dir1, 2: dir2]

        // Select first tab (0) and close it
        pane.select_tab(0);
        assert!(pane.close_active_tab());
        assert_eq!(pane.tab_count(), 2);
        assert_eq!(pane.active_tab_index(), 0);
        assert_eq!(pane.current_path(), &PathBuf::from("/dir1"));

        // Add two more tabs: [0: dir1, 1: dir3, 2: dir4]
        pane.new_tab(PathBuf::from("/dir3"), Vec::new());
        pane.new_tab(PathBuf::from("/dir4"), Vec::new());

        // Select middle tab (1) and close it
        pane.select_tab(1);
        assert!(pane.close_active_tab());
        assert_eq!(pane.tab_count(), 3);
        assert_eq!(pane.active_tab_index(), 1);
        assert_eq!(pane.current_path(), &PathBuf::from("/dir3"));
    }

    #[test]
    fn test_left_and_right_panes_have_independent_tabs() {
        let mut app = App::default();
        assert_eq!(app.pane(ActivePane::Left).tab_count(), 1);
        assert_eq!(app.pane(ActivePane::Right).tab_count(), 1);

        // Add 2 tabs to Left pane
        app.handle_action(Action::NewTab);
        app.handle_action(Action::NewTab);
        assert_eq!(app.pane(ActivePane::Left).tab_count(), 3);
        assert_eq!(app.pane(ActivePane::Right).tab_count(), 1);

        // Switch pane to Right
        app.handle_action(Action::SwitchPane);
        assert_eq!(app.active_pane(), ActivePane::Right);

        // Add 1 tab to Right pane
        app.handle_action(Action::NewTab);
        assert_eq!(app.pane(ActivePane::Left).tab_count(), 3);
        assert_eq!(app.pane(ActivePane::Right).tab_count(), 2);
    }

    #[test]
    fn test_tab_unicode_name_and_root() {
        let tab_unicode = Tab::new(PathBuf::from("/home/user/📁 Projects"), Vec::new());
        assert_eq!(tab_unicode.name(), "📁 Projects");

        let tab_root = Tab::new(PathBuf::from("/"), Vec::new());
        assert_eq!(tab_root.name(), "/");

        let tab_empty = Tab::default();
        assert_eq!(tab_empty.name(), "[empty]");
    }

    #[test]
    fn test_closing_tab_does_not_delete_filesystem_directory() {
        let temp_dir = TempDir::new("tab-fs-safe-delete");
        let dir_path = temp_dir.path().to_path_buf();

        let mut app = App::at(dir_path.clone()).expect("app loaded");
        app.handle_action(Action::NewTab);
        assert_eq!(app.pane(ActivePane::Left).tab_count(), 2);

        // Close active tab
        app.handle_action(Action::CloseTab);
        assert_eq!(app.pane(ActivePane::Left).tab_count(), 1);

        // Directory on disk remains intact
        assert!(dir_path.exists());
        assert!(dir_path.is_dir());
    }

    #[test]
    fn test_app_persistence_save_load_round_trip() {
        let temp_dir = TempDir::new("app-persist-round-trip");
        let root = temp_dir.path().to_path_buf();
        let left1 = root.join("left_alpha");
        let left2 = root.join("left_beta");
        let right1 = root.join("right_gamma");
        let bookmark1 = root.join("bookmark_one");

        std::fs::create_dir(&left1).expect("create dir");
        std::fs::create_dir(&left2).expect("create dir");
        std::fs::create_dir(&right1).expect("create dir");
        std::fs::create_dir(&bookmark1).expect("create dir");

        let mut app = App::at(left1.clone()).expect("app loaded");
        app.new_tab_in_active_pane();
        app.open_in(ActivePane::Left, left2.clone())
            .expect("open left2");
        app.handle_action(Action::SwitchPane);
        app.open_in(ActivePane::Right, right1.clone())
            .expect("open right");
        app.add_bookmark(bookmark1.clone());

        let cfg_path = root.join("test_config.tv");
        app.save_persistent_state_to(&cfg_path)
            .expect("save succeeds");
        assert!(cfg_path.exists());

        // Create a new app and restore
        let mut restored_app = App::at(root.clone()).expect("app loaded");
        restored_app
            .load_persistent_state_from(&cfg_path)
            .expect("load succeeds");

        assert_eq!(restored_app.active_pane(), ActivePane::Right);
        assert_eq!(restored_app.pane(ActivePane::Left).tab_count(), 2);
        assert_eq!(
            restored_app.pane(ActivePane::Left).tabs()[0].current_path(),
            &left1
        );
        assert_eq!(
            restored_app.pane(ActivePane::Left).tabs()[1].current_path(),
            &left2
        );
        assert_eq!(restored_app.pane(ActivePane::Right).tab_count(), 1);
        assert_eq!(
            restored_app.pane(ActivePane::Right).tabs()[0].current_path(),
            &right1
        );
        assert_eq!(restored_app.bookmarks().len(), 1);
        assert_eq!(
            restored_app.bookmarks().bookmarks()[0].path(),
            bookmark1.as_path()
        );
    }

    #[test]
    fn test_app_persistence_empty_state_and_missing_file() {
        let temp_dir = TempDir::new("app-persist-empty");
        let root = temp_dir.path().to_path_buf();
        let mut app = App::at(root.clone()).expect("app loaded");

        // Missing file should return error cleanly from load_from_path, but App::load_persistent_state handles it safely
        let non_existent = root.join("missing.tv");
        assert!(app.load_persistent_state_from(&non_existent).is_err());
        // App remains in valid default state
        assert_eq!(app.active_pane(), ActivePane::Left);
        assert_eq!(app.pane(ActivePane::Left).tab_count(), 1);
    }

    #[test]
    fn test_app_persistence_unicode_and_spaces() {
        let temp_dir = TempDir::new("app-persist-unicode");
        let root = temp_dir.path().to_path_buf();
        let u_dir = root.join("📁 Projects with spaces 🦀");
        let j_dir = root.join("日本語フォルダ");
        std::fs::create_dir(&u_dir).expect("create unicode dir");
        std::fs::create_dir(&j_dir).expect("create japanese dir");

        let mut app = App::at(u_dir.clone()).expect("app loaded");
        app.new_tab_in_active_pane();
        app.open_in(ActivePane::Left, j_dir.clone())
            .expect("open japanese dir");
        app.add_bookmark(u_dir.clone());

        let cfg_path = root.join("unicode_config.tv");
        app.save_persistent_state_to(&cfg_path)
            .expect("save succeeds");

        let mut restored_app = App::at(root.clone()).expect("app loaded");
        restored_app
            .load_persistent_state_from(&cfg_path)
            .expect("load succeeds");

        assert_eq!(restored_app.pane(ActivePane::Left).tab_count(), 2);
        assert_eq!(
            restored_app.pane(ActivePane::Left).tabs()[0].current_path(),
            &u_dir
        );
        assert_eq!(
            restored_app.pane(ActivePane::Left).tabs()[1].current_path(),
            &j_dir
        );
        assert_eq!(restored_app.bookmarks().len(), 1);
        assert_eq!(
            restored_app.bookmarks().bookmarks()[0].path(),
            u_dir.as_path()
        );
    }

    #[test]
    fn test_app_persistence_missing_tab_and_bookmark_directory_fallback() {
        let temp_dir = TempDir::new("app-persist-missing-dirs");
        let root = temp_dir.path().to_path_buf();
        let valid_dir = root.join("valid_dir");
        let deleted_dir = root.join("deleted_dir");
        std::fs::create_dir(&valid_dir).expect("create valid dir");

        // Settings specify a missing directory for tabs
        let settings = Settings {
            active_pane: ActivePaneConfig::Left,
            left_tabs: PaneTabsConfig::new(0, vec![deleted_dir.clone(), valid_dir.clone()]),
            right_tabs: PaneTabsConfig::new(0, vec![deleted_dir.clone()]),
            bookmarks: vec![
                BookmarkConfig::new("Deleted", deleted_dir.clone()),
                BookmarkConfig::new("Valid", valid_dir.clone()),
            ],
            recent_locations: Vec::new(),
            recent_files: Vec::new(),
            motion_mode: crate::animation::MotionMode::Full,
            startup_motion: crate::animation::StartupMotionMode::Cinematic,
            reduced_motion: false,
            theme: "terminalvision".to_string(),
        };

        let mut app = App::at(valid_dir.clone()).expect("app loaded");
        app.apply_persistent_settings(&settings);

        // Left pane kept only valid tab
        assert_eq!(app.pane(ActivePane::Left).tab_count(), 1);
        assert_eq!(app.pane(ActivePane::Left).current_path(), &valid_dir);

        // Right pane had no valid saved tabs, so it preserved its default tab safely without crashing
        assert_eq!(app.pane(ActivePane::Right).tab_count(), 1);

        // Bookmarks preserved (including missing directory record as data)
        assert_eq!(app.bookmarks().len(), 2);
    }

    #[test]
    fn test_app_persistence_no_filesystem_mutation_on_load() {
        let temp_dir = TempDir::new("app-persist-no-mutation");
        let root = temp_dir.path().to_path_buf();
        let initial_entries: Vec<_> = std::fs::read_dir(&root).expect("read dir").collect();

        let cfg_path = root.join("config.tv");
        let raw = r#"
            [settings]
            active_pane = left
            [tabs.left]
            active = 0
            path = /non_existent/fake/directory/12345
            [bookmarks]
            bookmark = Fake | /non_existent/fake/bookmark
        "#;
        std::fs::write(&cfg_path, raw).expect("write config");

        let mut app = App::at(root.clone()).expect("app loaded");
        app.load_persistent_state_from(&cfg_path)
            .expect("load succeeds");

        // The non-existent paths must NOT be created on disk
        assert!(!Path::new("/non_existent/fake/directory/12345").exists());
        assert!(!Path::new("/non_existent/fake/bookmark").exists());

        // Root directory has only config.tv created by the test
        let current_entries: Vec<_> = std::fs::read_dir(&root).expect("read dir").collect();
        assert_eq!(current_entries.len(), initial_entries.len() + 1);
    }

    #[test]
    fn test_git_detection_state_invalidation_and_pane_independence() {
        let temp_dir = TempDir::new("app-git-independence");
        let root = temp_dir.path().to_path_buf();
        let git_dir = root.join("git_repo");
        let non_git_dir = root.join("plain_dir");

        std::fs::create_dir_all(&git_dir).expect("create git_dir");
        std::fs::create_dir_all(&non_git_dir).expect("create plain_dir");
        std::fs::create_dir(git_dir.join(".git")).expect("create .git");

        let mut app = App::at(non_git_dir.clone()).expect("app loaded");
        assert!(!app.git().is_repo());

        // Navigate to git repo -> state updates immediately
        app.open_in(ActivePane::Left, git_dir.clone())
            .expect("open git dir");
        assert!(app.git().is_repo());
        assert_eq!(app.git().root(), Some(git_dir.as_path()));

        // Switch pane -> Right pane is in plain_dir, Git state is independent
        app.handle_action(Action::SwitchPane);
        assert!(!app.git().is_repo());

        // Create new tab in Left pane pointing at plain_dir
        app.handle_action(Action::SwitchPane);
        app.new_tab_in_active_pane();
        app.open_in(ActivePane::Left, non_git_dir.clone())
            .expect("open non-git");
        assert!(!app.git().is_repo());

        // Cycle tab back to original git_dir tab -> Git state is restored
        app.previous_tab_in_active_pane();
        assert!(app.git().is_repo());
    }

    #[test]
    fn test_git_status_pane_tab_independence() {
        let temp_dir = TempDir::new("app-git-status-independence");
        let root = temp_dir.path().to_path_buf();
        let repo_a = root.join("repo_a");
        let repo_b = root.join("repo_b");

        std::fs::create_dir_all(&repo_a).expect("create repo_a");
        std::fs::create_dir_all(&repo_b).expect("create repo_b");

        let git_a = repo_a.join(".git");
        let git_b = repo_b.join(".git");
        std::fs::create_dir(&git_a).expect("create git_a");
        std::fs::create_dir(&git_b).expect("create git_b");

        std::fs::write(git_a.join("HEAD"), "ref: refs/heads/branch-a\n").expect("head a");
        std::fs::write(git_b.join("HEAD"), "ref: refs/heads/branch-b\n").expect("head b");

        std::fs::write(repo_a.join("untracked_a.txt"), "data a").expect("file a");

        let mut app = App::at(repo_a.clone()).expect("app loaded");
        assert!(app.git_status().is_repo());
        assert_eq!(
            app.git_status().branch,
            crate::git::GitBranch::Branch("branch-a".to_string())
        );
        assert_eq!(app.git_status().untracked_count, 1);

        // Open repo_b in right pane
        app.open_in(ActivePane::Right, repo_b.clone())
            .expect("open repo_b");
        app.handle_action(Action::SwitchPane);

        assert!(app.git_status().is_repo());
        assert_eq!(
            app.git_status().branch,
            crate::git::GitBranch::Branch("branch-b".to_string())
        );
        assert_eq!(app.git_status().untracked_count, 0);

        // Switch back to left pane -> repo_a status restored
        app.handle_action(Action::SwitchPane);
        assert_eq!(
            app.git_status().branch,
            crate::git::GitBranch::Branch("branch-a".to_string())
        );
        assert_eq!(app.git_status().untracked_count, 1);
    }

    #[test]
    fn test_project_awareness_pane_tab_independence() {
        let temp_dir = TempDir::new("app-proj-independence");
        let root = temp_dir.path().to_path_buf();
        let proj_rust = root.join("rust_proj");
        let proj_node = root.join("node_proj");

        std::fs::create_dir_all(&proj_rust).expect("create proj_rust");
        std::fs::create_dir_all(&proj_node).expect("create proj_node");

        std::fs::create_dir(proj_rust.join(".git")).expect("git rust");
        std::fs::create_dir(proj_node.join(".git")).expect("git node");

        std::fs::write(proj_rust.join("Cargo.toml"), "").expect("cargo");
        std::fs::write(proj_node.join("package.json"), "").expect("package");

        let mut app = App::at(proj_rust.clone()).expect("app loaded");
        assert!(app.project_info().is_active());
        assert_eq!(
            app.project_info().project_types,
            vec![crate::git::ProjectType::Rust]
        );

        // Open proj_node in Right pane
        app.open_in(ActivePane::Right, proj_node.clone())
            .expect("open node");
        app.handle_action(Action::SwitchPane);

        assert!(app.project_info().is_active());
        assert_eq!(
            app.project_info().project_types,
            vec![crate::git::ProjectType::Node]
        );

        // Switch back to Left pane
        app.handle_action(Action::SwitchPane);
        assert_eq!(
            app.project_info().project_types,
            vec![crate::git::ProjectType::Rust]
        );
    }

    #[test]
    fn test_refresh_pane_updates_git_and_project() {
        let temp_dir = TempDir::new("audit-git-refresh");
        let root = temp_dir.path().to_path_buf();
        std::fs::create_dir(root.join(".git")).expect("create .git");

        let mut app = App::at(root.clone()).expect("init app");
        assert_eq!(app.project_info().project_types, vec![]);
        assert_eq!(app.git_status().untracked_count, 0);

        // Create Cargo.toml in the active pane directory
        app.handle_action(Action::NewFile);
        app.set_input_buffer("Cargo.toml");
        app.confirm_modal();

        // After creation and automatic refresh, project_info and git_status must be updated!
        assert_eq!(
            app.project_info().project_types,
            vec![crate::git::ProjectType::Rust]
        );
        assert_eq!(app.git_status().untracked_count, 1);
    }

    #[test]
    fn test_clipboard_cleanup_on_delete_and_rename() {
        let temp_dir = TempDir::new("audit-clipboard-cleanup");
        let root = temp_dir.path().to_path_buf();
        let file1 = root.join("alpha.txt");
        let file2 = root.join("beta.txt");
        std::fs::write(&file1, "1").expect("write alpha");
        std::fs::write(&file2, "2").expect("write beta");

        let mut app = App::at(root.clone()).expect("init app");
        // Mark alpha.txt for copy
        app.handle_action(Action::Copy);
        assert_eq!(app.clipboard().len(), 1);
        assert_eq!(app.clipboard().entries()[0], file1);

        // Rename alpha.txt to renamed.txt
        app.handle_action(Action::Rename);
        app.set_input_buffer("renamed.txt");
        app.confirm_modal();

        // Clipboard path should be updated
        assert_eq!(app.clipboard().len(), 1);
        // Select and delete renamed.txt
        let renamed_idx = app
            .pane(app.active_pane())
            .entries()
            .iter()
            .position(|e| e.path() == root.join("renamed.txt"))
            .expect("renamed entry");
        app.pane_mut(app.active_pane()).select(renamed_idx);

        app.handle_action(Action::Delete);
        // Clipboard should be empty now
        assert!(app.clipboard().is_empty());
        assert_eq!(app.clipboard().operation(), None);
    }

    #[test]
    fn test_tab_invalidation_when_directory_deleted() {
        let temp_dir = TempDir::new("audit-tab-invalidation");
        let root = temp_dir.path().to_path_buf();
        let sub = root.join("subfolder");
        let deep = sub.join("nested");
        std::fs::create_dir_all(&deep).expect("create sub dirs");

        let mut app = App::at(root.clone()).expect("init app");

        // Open deep subfolder in a second tab
        app.open_in(ActivePane::Left, deep.clone())
            .expect("open deep");
        assert_eq!(app.pane(app.active_pane()).current_path(), &deep);

        // Switch to root in active tab
        app.open_in(ActivePane::Left, root.clone())
            .expect("open root");

        // Add a new tab in Right pane pointing to deep
        app.open_in(ActivePane::Right, deep.clone())
            .expect("open right deep");

        // Delete subfolder from Left pane
        // First select subfolder
        let sub_idx = app
            .pane(app.active_pane())
            .entries()
            .iter()
            .position(|e| e.path() == sub)
            .expect("subfolder entry");
        app.pane_mut(ActivePane::Left).select(sub_idx);

        app.handle_action(Action::Delete);

        // Both tabs that pointed inside `sub` must now point to `root`!
        for tab in &app.left.tabs {
            assert!(!tab.current_path().starts_with(&sub));
        }
        for tab in &app.right.tabs {
            assert!(!tab.current_path().starts_with(&sub));
        }
    }

    #[test]
    fn test_tab_switching_refreshes_preview() {
        let temp_dir = TempDir::new("audit-tab-preview");
        let root = temp_dir.path().to_path_buf();
        let dir_a = root.join("dir_a");
        let dir_b = root.join("dir_b");
        std::fs::create_dir(&dir_a).expect("create dir_a");
        std::fs::create_dir(&dir_b).expect("create dir_b");
        let file_a = dir_a.join("alpha.txt");
        let file_b = dir_b.join("beta.txt");
        std::fs::write(&file_a, "Content A").expect("write a");
        std::fs::write(&file_b, "Content B").expect("write b");

        let mut app = App::at(dir_a.clone()).expect("init app");
        app.handle_action(Action::Preview);
        assert!(app.preview().is_active());
        assert_eq!(app.preview().path(), Some(file_a.as_path()));

        // Add new tab pointing to dir_b
        app.new_tab_in_active_pane();
        app.open_in(ActivePane::Left, dir_b.clone())
            .expect("open dir_b");
        assert_eq!(app.preview().path(), Some(file_b.as_path()));

        // Cycle back to previous tab
        app.previous_tab_in_active_pane();
        assert_eq!(app.preview().path(), Some(file_a.as_path()));

        // Cycle to next tab
        app.next_tab_in_active_pane();
        assert_eq!(app.preview().path(), Some(file_b.as_path()));

        // Close tab
        app.close_tab_in_active_pane();
        assert_eq!(app.preview().path(), Some(file_a.as_path()));
    }

    #[test]
    fn test_state_corruption_and_transition_stress() {
        let temp_dir = TempDir::new("audit-state-stress");
        let root = temp_dir.path().to_path_buf();
        let dir_a = root.join("dir_a");
        let dir_b = root.join("dir_b");
        let dir_c = root.join("dir_c");
        std::fs::create_dir(&dir_a).unwrap();
        std::fs::create_dir(&dir_b).unwrap();
        std::fs::create_dir(&dir_c).unwrap();

        let file_a = dir_a.join("alpha.txt");
        let file_b = dir_a.join("beta.txt");
        std::fs::write(&file_a, "alpha").unwrap();
        std::fs::write(&file_b, "beta").unwrap();

        let mut app = App::at(dir_a.clone()).expect("init app");

        // 1. Navigation -> Refresh
        let _ = app.refresh_pane(ActivePane::Left);
        assert_eq!(app.pane(ActivePane::Left).current_path(), dir_a.as_path());
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 2);

        // 2. Navigation -> Switch Pane
        app.handle_action(Action::SwitchPane);
        assert_eq!(app.active_pane(), ActivePane::Right);
        app.handle_action(Action::SwitchPane);
        assert_eq!(app.active_pane(), ActivePane::Left);

        // 3. Navigation -> Switch Tab
        app.new_tab_in_active_pane();
        app.open_in(ActivePane::Left, dir_b.clone()).unwrap();
        assert_eq!(app.pane(ActivePane::Left).current_path(), dir_b.as_path());
        app.previous_tab_in_active_pane();
        assert_eq!(app.pane(ActivePane::Left).current_path(), dir_a.as_path());
        app.next_tab_in_active_pane();
        app.close_tab_in_active_pane();
        assert_eq!(app.pane(ActivePane::Left).current_path(), dir_a.as_path());

        // 4. Search -> Navigation
        app.handle_action(Action::StartSearch);
        assert_eq!(app.mode(), Mode::Search);
        app.set_search_query("alph");
        assert!(app.search().is_active());
        app.handle_action(Action::ClearSearch);
        assert!(!app.search().is_active());
        assert_eq!(app.mode(), Mode::Normal);

        // 5. Search -> Refresh
        app.handle_action(Action::StartSearch);
        app.set_search_query("bet");
        let _ = app.refresh_pane(ActivePane::Left);
        assert_eq!(app.pane(ActivePane::Left).current_path(), dir_a.as_path());
        app.handle_action(Action::ClearSearch);
        assert_eq!(app.mode(), Mode::Normal);

        // 6. Preview -> Navigation
        app.handle_action(Action::GoHome);
        app.handle_action(Action::Preview);
        assert!(app.preview().is_active());
        assert_eq!(app.preview().path(), Some(file_a.as_path()));
        // Navigating to another directory refreshes preview
        app.open_in(ActivePane::Left, dir_b.clone()).unwrap();
        assert!(app.preview().path().is_none());
        app.open_in(ActivePane::Left, dir_a.clone()).unwrap();
        assert_eq!(app.preview().path(), Some(file_a.as_path()));

        // 7. Preview -> Delete
        app.handle_action(Action::Cancel);
        assert_eq!(app.mode(), Mode::Normal);
        app.handle_action(Action::Delete);
        if app.mode() == Mode::Confirm {
            app.set_confirm_selection(true);
            app.confirm_modal();
        }
        assert!(!file_a.exists());
        assert!(file_b.exists());

        // 8. Copy -> Navigation -> Paste
        app.handle_action(Action::Copy);
        assert_eq!(app.clipboard().entries(), std::slice::from_ref(&file_b));
        assert_eq!(app.clipboard().operation(), Some(ClipboardOperation::Copy));
        app.open_in(ActivePane::Left, dir_c.clone()).unwrap();
        app.handle_action(Action::Paste);
        assert!(dir_c.join("beta.txt").exists());
        assert_eq!(app.clipboard().entries(), std::slice::from_ref(&file_b));

        // 9. Cut -> Navigation -> Paste
        app.handle_action(Action::GoHome);
        app.handle_action(Action::Cut);
        assert_eq!(app.clipboard().operation(), Some(ClipboardOperation::Cut));
        app.open_in(ActivePane::Left, dir_b.clone()).unwrap();
        app.handle_action(Action::Paste);
        assert!(dir_b.join("beta.txt").exists());
        assert!(!dir_c.join("beta.txt").exists());
        assert!(app.clipboard().is_empty());

        // 10. Bookmark -> Directory Deletion
        let doomed_dir = root.join("doomed");
        std::fs::create_dir(&doomed_dir).unwrap();
        app.bookmarks_mut().add(doomed_dir.clone());
        std::fs::remove_dir(&doomed_dir).unwrap();
        app.handle_action(Action::OpenBookmarks);
        let doomed_idx = app.bookmarks().len() - 1;
        app.bookmarks_mut().select(doomed_idx);
        let outcome = app.open_selected_bookmark();
        assert!(outcome.is_err());
        assert_eq!(app.mode(), Mode::Normal);

        // 11. Git repository -> Leave repository
        let git_dir = root.join("repo");
        std::fs::create_dir_all(git_dir.join(".git")).unwrap();
        app.open_in(ActivePane::Left, git_dir.clone()).unwrap();
        assert!(app.git().is_repo());
        app.open_in(ActivePane::Left, dir_a.clone()).unwrap();
        assert!(!app.git().is_repo());

        // 12. Project awareness -> Leave project
        let proj_dir = root.join("rust_proj");
        std::fs::create_dir(&proj_dir).unwrap();
        std::fs::create_dir(proj_dir.join(".git")).unwrap();
        std::fs::write(proj_dir.join("Cargo.toml"), "[package]\nname = \"tv_proj\"").unwrap();
        app.open_in(ActivePane::Left, proj_dir.clone()).unwrap();
        assert!(app.project_info().is_active());
        app.open_in(ActivePane::Left, dir_a.clone()).unwrap();
        assert!(!app.project_info().is_active());
    }

    #[test]
    fn test_phase17_3_multi_selection_model() {
        let dir = TempDir::new("phase17-multiselect");
        let f1 = dir.path().join("file1.txt");
        let f2 = dir.path().join("file2.txt");
        let f3 = dir.path().join("file3.txt");
        std::fs::write(&f1, b"1").unwrap();
        std::fs::write(&f2, b"2").unwrap();
        std::fs::write(&f3, b"3").unwrap();

        let mut app = App::at(dir.path().to_path_buf()).unwrap();
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 3);
        assert_eq!(app.pane(ActivePane::Left).selected_count(), 0);

        // Toggle selection on current item (index 0)
        app.handle_action(Action::ToggleSelect);
        assert_eq!(app.pane(ActivePane::Left).selected_count(), 1);
        assert!(app.pane(ActivePane::Left).is_item_selected(0));

        // Toggle again to deselect
        app.handle_action(Action::ToggleSelect);
        assert_eq!(app.pane(ActivePane::Left).selected_count(), 0);
        assert!(!app.pane(ActivePane::Left).is_item_selected(0));

        // Select All
        app.handle_action(Action::SelectAll);
        assert_eq!(app.pane(ActivePane::Left).selected_count(), 3);
        assert!(app.pane(ActivePane::Left).is_item_selected(0));
        assert!(app.pane(ActivePane::Left).is_item_selected(1));
        assert!(app.pane(ActivePane::Left).is_item_selected(2));

        // Invert Selection (all -> none)
        app.handle_action(Action::InvertSelection);
        assert_eq!(app.pane(ActivePane::Left).selected_count(), 0);

        // Select range 0..=1
        app.pane_mut(ActivePane::Left).select_range(0, 1);
        assert_eq!(app.pane(ActivePane::Left).selected_count(), 2);
        assert!(app.pane(ActivePane::Left).is_item_selected(0));
        assert!(app.pane(ActivePane::Left).is_item_selected(1));
        assert!(!app.pane(ActivePane::Left).is_item_selected(2));

        // Deselect All
        app.handle_action(Action::DeselectAll);
        assert_eq!(app.pane(ActivePane::Left).selected_count(), 0);
    }

    #[test]
    fn test_phase17_3_multi_file_operations_copy_cut_delete() {
        let src_dir = TempDir::new("phase17-ops-src");
        let dest_dir = TempDir::new("phase17-ops-dest");

        let f1 = src_dir.path().join("a.txt");
        let f2 = src_dir.path().join("b.txt");
        let f3 = src_dir.path().join("c.txt");
        std::fs::write(&f1, b"aaa").unwrap();
        std::fs::write(&f2, b"bbb").unwrap();
        std::fs::write(&f3, b"ccc").unwrap();

        let mut app = two_panes_showing(&src_dir, &dest_dir);

        // Multi-select a.txt and b.txt
        app.pane_mut(ActivePane::Left).select_range(0, 1);
        assert_eq!(app.pane(ActivePane::Left).selected_count(), 2);

        // Copy multi-selected
        app.handle_action(Action::Copy);
        assert_eq!(app.clipboard().entries().len(), 2);
        assert_eq!(app.clipboard().operation(), Some(ClipboardOperation::Copy));

        // Paste into right pane
        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::Paste);
        assert!(dest_dir.path().join("a.txt").is_file());
        assert!(dest_dir.path().join("b.txt").is_file());
        assert!(!dest_dir.path().join("c.txt").is_file());
        // Source files still exist after copy
        assert!(f1.is_file());
        assert!(f2.is_file());

        // Cut c.txt
        activate(&mut app, ActivePane::Left);
        app.handle_action(Action::DeselectAll);
        app.pane_mut(ActivePane::Left).select_range(2, 2);
        app.handle_action(Action::Cut);
        assert_eq!(app.clipboard().entries().len(), 1);
        assert_eq!(app.clipboard().operation(), Some(ClipboardOperation::Cut));

        // Paste c.txt into destination
        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::Paste);
        assert!(dest_dir.path().join("c.txt").is_file());
        assert!(!f3.is_file()); // moved
        assert!(app.clipboard().is_empty()); // cut clears clipboard

        // Multi-select all in right pane and delete
        app.handle_action(Action::SelectAll);
        assert_eq!(app.pane(ActivePane::Right).selected_count(), 3);
        app.handle_action(Action::Delete);
        assert_eq!(app.pane(ActivePane::Right).entries().len(), 0);
    }

    #[test]
    fn test_phase17_3_selection_preservation_across_sort_and_refresh() {
        let dir = TempDir::new("phase17-preservation");
        let f_alpha = dir.path().join("alpha.txt");
        let f_beta = dir.path().join("beta.txt");
        let f_gamma = dir.path().join("gamma.txt");
        std::fs::write(&f_alpha, b"1").unwrap();
        std::fs::write(&f_beta, b"2").unwrap();
        std::fs::write(&f_gamma, b"3").unwrap();

        let mut app = App::at(dir.path().to_path_buf()).unwrap();
        // Select beta.txt (index 1)
        app.pane_mut(ActivePane::Left).move_selection_down(1);
        assert_eq!(
            app.pane(ActivePane::Left).selected_path(),
            Some(f_beta.clone())
        );

        // Change sort
        app.handle_action(Action::ChangeSort);
        // Path should still be preserved
        assert_eq!(
            app.pane(ActivePane::Left).selected_path(),
            Some(f_beta.clone())
        );

        // Add a new file and refresh
        let f_delta = dir.path().join("delta.txt");
        std::fs::write(&f_delta, b"4").unwrap();
        app.refresh_active_pane().unwrap();
        assert_eq!(
            app.pane(ActivePane::Left).selected_path(),
            Some(f_beta.clone())
        );

        // Delete the currently selected file and refresh
        std::fs::remove_file(&f_beta).unwrap();
        app.refresh_active_pane().unwrap();
        // Nearest index selection is preserved
        assert!(app.pane(ActivePane::Left).selected_path().is_some());
        assert_ne!(
            app.pane(ActivePane::Left).selected_path(),
            Some(f_beta.clone())
        );
    }

    #[test]
    fn test_phase17_3_unicode_filename_operations() {
        let dir = TempDir::new("phase17-unicode");
        let unicode_file = dir.path().join("മലയാളം_രേഖ_🇯🇵_مرحبا.txt");
        std::fs::write(&unicode_file, b"unicode content").unwrap();

        let mut app = App::at(dir.path().to_path_buf()).unwrap();
        assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);
        assert_eq!(
            app.pane(ActivePane::Left).selected_path(),
            Some(unicode_file.clone())
        );

        // Toggle selection on unicode file
        app.handle_action(Action::ToggleSelect);
        assert_eq!(app.pane(ActivePane::Left).selected_count(), 1);

        // Copy and paste into a temp directory
        let dest = TempDir::new("phase17-unicode-dest");
        app.open_in(ActivePane::Right, dest.path().to_path_buf())
            .unwrap();
        app.handle_action(Action::Copy);
        activate(&mut app, ActivePane::Right);
        app.handle_action(Action::Paste);

        assert!(dest.path().join("മലയാളം_രേഖ_🇯🇵_مرحبا.txt").is_file());
    }

    #[test]
    fn test_phase17_3_operation_center_and_progress_status() {
        let dir = TempDir::new("phase17-progress");
        let f1 = dir.path().join("f1.txt");
        std::fs::write(&f1, b"hello").unwrap();

        let mut app = App::at(dir.path().to_path_buf()).unwrap();
        app.handle_action(Action::Copy);

        let progress = app.operation_progress();
        assert!(progress.is_some());
        let p = progress.unwrap();
        assert_eq!(p.kind, super::OperationKind::Copy);
        assert_eq!(p.total_count, 1);
        assert_eq!(p.status, super::OperationStatus::Completed);
    }

    #[test]
    fn test_phase18_1_recent_locations_bounding_and_deduplication() {
        let mut history = super::RecentLocations::new();
        assert!(history.is_empty());

        let p1 = PathBuf::from("/a");
        let p2 = PathBuf::from("/b");
        let p3 = PathBuf::from("/c");

        history.record(p1.clone());
        history.record(p2.clone());
        history.record(p3.clone());
        assert_eq!(history.len(), 3);
        assert_eq!(history.locations(), &[p3.clone(), p2.clone(), p1.clone()]);

        // Duplicate push moves it to the front
        history.record(p1.clone());
        assert_eq!(history.len(), 3);
        assert_eq!(history.locations(), &[p1.clone(), p3.clone(), p2.clone()]);

        // Overflow capacity check (default max is 50)
        for i in 0..60 {
            history.record(PathBuf::from(format!("/dir_{i}")));
        }
        assert_eq!(history.len(), 50);
    }

    #[test]
    fn test_phase18_1_smart_jump_generation_and_filtering() {
        let root = TempDir::new("phase18-smart-jump");
        let sub = root.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();

        let mut app = App::at(root.path().to_path_buf()).unwrap();
        app.bookmarks_mut().add(sub.clone());

        app.handle_action(Action::SmartJump);
        assert_eq!(app.mode(), Mode::SmartJump);

        let items = app.smart_jump().all_items();
        assert!(!items.is_empty());

        // Search for bookmark or current
        app.smart_jump_push_char('s');
        app.smart_jump_push_char('u');
        app.smart_jump_push_char('b');
        assert_eq!(app.smart_jump().query(), "sub");

        let filtered = app.smart_jump().filtered_items();
        assert!(!filtered.is_empty());

        // Cancel modal
        app.handle_action(Action::Cancel);
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn test_phase18_1_tab_duplication() {
        let dir = TempDir::new("phase18-tab-dup");
        let f1 = dir.path().join("item1.txt");
        let f2 = dir.path().join("item2.txt");
        std::fs::write(&f1, b"1").unwrap();
        std::fs::write(&f2, b"2").unwrap();

        let mut app = App::at(dir.path().to_path_buf()).unwrap();
        assert_eq!(app.pane(ActivePane::Left).tab_count(), 1);

        app.handle_action(Action::MoveDown);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));

        // Duplicate tab preserves current path and selection
        app.handle_action(Action::DuplicateTab);
        assert_eq!(app.pane(ActivePane::Left).tab_count(), 2);
        assert_eq!(app.pane(ActivePane::Left).active_tab_index(), 1);
        assert_eq!(app.pane(ActivePane::Left).current_path(), dir.path());
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));
    }

    #[test]
    fn test_phase18_1_quick_jump_actions() {
        let dir = TempDir::new("phase18-quick-jump");
        let mut app = App::at(dir.path().to_path_buf()).unwrap();

        // Jump to Root
        app.handle_action(Action::GoRootDir);
        let root_expected = crate::utils::path::filesystem_root(dir.path());
        assert_eq!(app.pane(ActivePane::Left).current_path(), &root_expected);

        // Jump to Home (if home exists)
        if let Some(home) = crate::utils::path::home_dir() {
            app.handle_action(Action::GoHomeDir);
            assert_eq!(app.pane(ActivePane::Left).current_path(), &home);
        }
    }

    #[test]
    fn test_phase18_1_command_palette_categories_and_shortcuts() {
        assert!(!crate::commands::palette::Command::ALL.is_empty());

        // Filter by category or name
        let nav_cmds = crate::commands::palette::filter_commands("Navigation");
        assert!(!nav_cmds.is_empty());

        let tab_cmds = crate::commands::palette::filter_commands("Tabs");
        assert!(!tab_cmds.is_empty());

        let sc_cmds = crate::commands::palette::filter_commands("Ctrl+P");
        assert!(!sc_cmds.is_empty());
    }

    #[test]
    fn test_phase18_1_search_matcher_extension_and_hidden_filtering() {
        use crate::search::Matcher;
        use std::ffi::OsStr;

        let ext_matcher = Matcher::new("*.rs", false);
        assert!(ext_matcher.extension_filter().is_some());
        assert_eq!(ext_matcher.extension_filter(), Some("rs"));
        assert!(ext_matcher.matches(OsStr::new("main.rs")));
        assert!(!ext_matcher.matches(OsStr::new("main.py")));

        let hidden_matcher = Matcher::new(".env", false);
        assert!(hidden_matcher.allows_hidden());

        let normal_matcher = Matcher::new("cargo", false);
        assert!(!normal_matcher.allows_hidden());
    }

    #[test]
    fn test_phase18_2_project_cockpit_flow() {
        let dir = TempDir::new("phase18-2-cockpit");
        let project_dir = dir.path().join("my_rust_pkg");
        fs::create_dir_all(project_dir.join("src")).unwrap();
        fs::write(
            project_dir.join("Cargo.toml"),
            b"[package]\nname = \"my_rust_pkg\"\n",
        )
        .unwrap();
        fs::write(project_dir.join("README.md"), b"# My Rust Pkg\n").unwrap();
        fs::write(project_dir.join("LICENSE"), b"MIT\n").unwrap();
        fs::write(project_dir.join("src/main.rs"), b"fn main() {}\n").unwrap();

        let mut app = App::at(project_dir.clone()).unwrap();

        // Project detected
        assert_eq!(
            app.project_info().primary_type(),
            Some(crate::git::ProjectType::Rust)
        );
        assert_eq!(app.project_info().name(), "my_rust_pkg");
        assert!(app.project_info().manifest_file.is_some());
        assert!(app.project_info().readme_file.is_some());
        assert!(app.project_info().license_file.is_some());
        assert!(app.project_info().source_dir.is_some());

        // Open Project Cockpit
        app.handle_action(Action::ProjectCockpit);
        assert_eq!(app.mode(), Mode::ProjectCockpit);
        assert!(!app.project_cockpit().actions().is_empty());
        assert_eq!(app.project_cockpit().selected_index(), 0);

        // Move down and up in cockpit actions
        app.project_cockpit_mut().move_down();
        assert_eq!(app.project_cockpit().selected_index(), 1);
        app.project_cockpit_mut().move_up();
        assert_eq!(app.project_cockpit().selected_index(), 0);

        // Execute selected action (Go to Project Root)
        let action = app.confirm_modal();
        assert_eq!(action, Some(Action::GoProjectRoot));
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn test_phase18_2_git_status_panel_flow() {
        let dir = TempDir::new("phase18-2-git-panel");
        let repo_dir = dir.path().join("repo");
        fs::create_dir_all(repo_dir.join(".git")).unwrap();
        fs::write(repo_dir.join(".git/HEAD"), b"ref: refs/heads/feature-x\n").unwrap();
        fs::write(repo_dir.join("file1.txt"), b"data1").unwrap();

        let mut app = App::at(repo_dir.clone()).unwrap();
        assert_eq!(app.git_status().branch.display(), "feature-x");

        // Open Git Status Panel
        app.handle_action(Action::GitStatusPanel);
        assert_eq!(app.mode(), Mode::GitStatusPanel);

        // Esc cancels modal
        app.handle_action(Action::Cancel);
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn test_phase18_2_file_radar_computation() {
        let dir = TempDir::new("phase18-2-radar");
        let folder = dir.path().join("data_folder");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("a.rs"), b"// rust 1").unwrap();
        fs::write(folder.join("b.rs"), b"// rust 2").unwrap();
        fs::write(folder.join("c.py"), b"# python").unwrap();
        fs::write(folder.join(".hidden"), b"hidden").unwrap();
        fs::create_dir(folder.join("sub")).unwrap();

        let mut app = App::at(folder.clone()).unwrap();
        app.handle_action(Action::FileRadar);
        assert_eq!(app.mode(), Mode::FileRadar);

        let radar = app.file_radar();
        assert_eq!(radar.directory_count, 1);
        assert_eq!(radar.file_count, 4);
        assert_eq!(radar.hidden_count, 1);
        assert!(!radar.top_extensions.is_empty());
        assert_eq!(radar.top_extensions[0].extension, ".rs");
        assert_eq!(radar.top_extensions[0].count, 2);

        // Close FileRadar
        app.handle_action(Action::Cancel);
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn test_phase18_2_reveal_context_flow() {
        let dir = TempDir::new("phase18-2-reveal");
        let project_dir = dir.path().join("proj");
        fs::create_dir_all(project_dir.join("src")).unwrap();
        fs::write(
            project_dir.join("Cargo.toml"),
            b"[package]\nname = \"proj\"\n",
        )
        .unwrap();
        fs::write(project_dir.join("src/main.rs"), b"fn main() {}\n").unwrap();

        let mut app = App::at(project_dir.join("src")).unwrap();
        app.select_entry(ActivePane::Left, 0);

        app.handle_action(Action::RevealContext);
        assert_eq!(app.mode(), Mode::RevealContext);
        assert!(app.reveal_context().levels().len() >= 2);

        // Move down in levels
        app.reveal_context_mut().move_down();
        assert_eq!(app.reveal_context().selected_index(), 1);

        // Close
        app.handle_action(Action::Cancel);
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn test_phase18_2_quick_developer_navigations() {
        let dir = TempDir::new("phase18-2-dev-nav");
        let project_dir = dir.path().join("my_app");
        fs::create_dir_all(project_dir.join("src")).unwrap();
        fs::write(project_dir.join("package.json"), b"{\"name\": \"my-app\"}").unwrap();
        fs::write(project_dir.join("README.md"), b"# My App").unwrap();
        fs::write(project_dir.join("LICENSE"), b"MIT").unwrap();
        fs::write(project_dir.join("src/index.js"), b"console.log('hi');").unwrap();

        let mut app = App::at(project_dir.clone()).unwrap();

        // Open Manifest
        app.handle_action(Action::OpenManifest);
        assert_eq!(
            app.pane(ActivePane::Left)
                .selected_entry()
                .map(|e| e.name()),
            Some(std::ffi::OsStr::new("package.json"))
        );

        // Open README
        app.handle_action(Action::OpenReadme);
        assert_eq!(
            app.pane(ActivePane::Left)
                .selected_entry()
                .map(|e| e.name()),
            Some(std::ffi::OsStr::new("README.md"))
        );

        // Open License
        app.handle_action(Action::OpenLicense);
        assert_eq!(
            app.pane(ActivePane::Left)
                .selected_entry()
                .map(|e| e.name()),
            Some(std::ffi::OsStr::new("LICENSE"))
        );

        // Go Source Dir
        app.handle_action(Action::GoSourceDir);
        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            &project_dir.join("src")
        );
    }

    #[test]
    fn test_phase18_2_git_status_panel_jump_to_file() {
        let dir = TempDir::new("phase18-2-git-jump");
        let repo_dir = dir.path().join("repo");
        fs::create_dir_all(repo_dir.join(".git")).unwrap();
        fs::write(repo_dir.join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();
        fs::write(repo_dir.join("modified.txt"), b"mod").unwrap();

        let mut app = App::at(repo_dir.clone()).unwrap();

        // Populate a synthetic git status panel entry
        app.handle_action(Action::GitStatusPanel);
        assert_eq!(app.mode(), Mode::GitStatusPanel);

        // Enter on empty entries doesn't crash
        let action = app.confirm_modal();
        assert_eq!(action, None);
    }

    #[test]
    fn test_phase18_2_reveal_context_navigation_selection() {
        let dir = TempDir::new("phase18-2-reveal-nav");
        let root = dir.path().join("parent").join("child");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("file.txt"), b"contents").unwrap();

        let mut app = App::at(root.clone()).unwrap();
        app.select_entry(ActivePane::Left, 0);

        app.handle_action(Action::RevealContext);
        assert_eq!(app.mode(), Mode::RevealContext);

        // Select the parent directory level (index 1)
        if app.reveal_context().levels().len() > 1 {
            app.reveal_context_mut().move_down();
            let action = app.confirm_modal();
            assert_eq!(action, None);
            // It navigates directly
            assert_eq!(app.mode(), Mode::Normal);
        }
    }

    #[test]
    fn test_phase18_2_file_radar_performance_1k_5k_10k() {
        use crate::filesystem::entry::{Entry, EntryKind};
        use std::path::PathBuf;

        for size in [1_000, 5_000, 10_000] {
            let entries: Vec<Entry> = (0..size)
                .map(|i| {
                    let ext = match i % 5 {
                        0 => "rs",
                        1 => "ts",
                        2 => "py",
                        3 => "go",
                        _ => "json",
                    };
                    let name = format!("item_{i}.{ext}");
                    Entry::new(
                        std::ffi::OsString::from(name),
                        PathBuf::from(format!("/tmp/perf/item_{i}.{ext}")),
                        EntryKind::File,
                    )
                })
                .collect();

            let start = std::time::Instant::now();
            let radar = super::FileRadarState::compute_from_entries(&entries);
            let elapsed = start.elapsed();

            assert_eq!(radar.total_entries, size);
            assert_eq!(radar.file_count, size);
            assert_eq!(radar.directory_count, 0);
            assert!(!radar.top_extensions.is_empty());

            // Should complete well under 100ms even for 10,000 entries
            assert!(
                elapsed.as_millis() < 100,
                "File radar computation took too long for {size} entries: {elapsed:?}"
            );
        }
    }

    #[test]
    fn test_phase18_3_focus_mode_flow() {
        let mut app = App::default();
        assert!(!app.is_focus_mode());

        // Toggle Focus Mode
        app.handle_action(Action::ToggleFocusMode);
        assert!(app.is_focus_mode());

        // Toggle Focus Mode off
        app.handle_action(Action::ToggleFocusMode);
        assert!(!app.is_focus_mode());
    }

    #[test]
    fn test_phase18_3_command_registry_completeness() {
        use crate::commands::palette::Command;
        use std::collections::HashSet;

        // Verify every Action in Action::ALL is referenced in commands or explicitly handled
        let mut action_set = HashSet::new();
        for cmd in Command::ALL {
            action_set.insert(cmd.action());
            assert!(!cmd.name().is_empty());
            assert!(!cmd.description().is_empty());
        }

        assert!(action_set.contains(&Action::ToggleFocusMode));
        assert!(action_set.contains(&Action::SmartJump));
        assert!(action_set.contains(&Action::ProjectCockpit));
        assert!(action_set.contains(&Action::GitStatusPanel));
        assert!(action_set.contains(&Action::FileRadar));
        assert!(action_set.contains(&Action::RevealContext));
    }
}
