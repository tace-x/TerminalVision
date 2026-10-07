//! Contextual Action Menu data model, target resolution, and item construction.
//!
//! Provides a structured, reusable context menu architecture that resolves
//! contextual targets (file, directory, multi-selection, empty pane) directly to
//! actions in the centralized [`ActionRegistry`].
//!
//! This module contains no filesystem I/O or state mutations directly; it operates
//! purely on metadata and existing application actions.

use std::path::PathBuf;

use ratatui::layout::Rect;

use crate::app::actions::Action;
use crate::input::platform::Platform;
use crate::input::shortcut::ShortcutRegistry;

/// The target of a contextual action menu invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuTarget {
    /// Right-clicked or summoned on empty pane space.
    Empty { directory: PathBuf },
    /// Right-clicked or summoned on a single file.
    File { path: PathBuf, is_executable: bool },
    /// Right-clicked or summoned on a single directory.
    Directory {
        path: PathBuf,
        is_project_root: bool,
        in_git: bool,
    },
    /// Right-clicked or summoned when multiple items are selected.
    MultiSelection { count: usize, paths: Vec<PathBuf> },
}

impl ContextMenuTarget {
    /// Returns the primary path associated with this target, if any.
    pub fn primary_path(&self) -> Option<&PathBuf> {
        match self {
            Self::Empty { directory } => Some(directory),
            Self::File { path, .. } => Some(path),
            Self::Directory { path, .. } => Some(path),
            Self::MultiSelection { paths, .. } => paths.first(),
        }
    }

    /// Whether this target represents multiple selected items.
    pub fn is_multi_selection(&self) -> bool {
        matches!(self, Self::MultiSelection { .. })
    }

    /// Returns the number of targeted items.
    pub fn item_count(&self) -> usize {
        match self {
            Self::Empty { .. } => 0,
            Self::File { .. } | Self::Directory { .. } => 1,
            Self::MultiSelection { count, .. } => *count,
        }
    }
}

/// A screen coordinate position where a context menu was requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContextMenuPosition {
    pub column: u16,
    pub row: u16,
}

impl From<(u16, u16)> for ContextMenuPosition {
    fn from(pos: (u16, u16)) -> Self {
        Self {
            column: pos.0,
            row: pos.1,
        }
    }
}

impl From<ContextMenuPosition> for (u16, u16) {
    fn from(pos: ContextMenuPosition) -> Self {
        (pos.column, pos.row)
    }
}

/// Logical grouping of related context menu items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuGroup {
    /// Core target actions (e.g. Open, Preview).
    Primary,
    /// Clipboard and transfer actions (Copy, Cut, Paste).
    Clipboard,
    /// Mutating filesystem operations (Rename, Delete, New File/Folder).
    Operations,
    /// Inspection and metadata actions (Get Info, Storage, Copy Path/Name).
    Properties,
    /// Secondary nested actions (Select All, Hidden Files, Git).
    More,
}

/// An item in a contextual action popup menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuItem {
    /// Action that can be executed directly.
    Action {
        action: Action,
        label: String,
        shortcut: Option<String>,
    },
    /// Submenu containing nested items (More ›).
    More {
        label: String,
        items: Vec<ContextMenuItem>,
    },
    /// Disabled action with an optional explanation tooltip/reason.
    Disabled {
        label: String,
        reason: Option<String>,
    },
    /// Visual separator dividing logical groups.
    Separator,
}

impl ContextMenuItem {
    /// Creates an actionable menu item.
    pub fn action(action: Action, label: impl Into<String>, shortcut: Option<String>) -> Self {
        Self::Action {
            action,
            label: label.into(),
            shortcut,
        }
    }

    /// Creates a submenu item.
    pub fn more(label: impl Into<String>, items: Vec<ContextMenuItem>) -> Self {
        Self::More {
            label: label.into(),
            items,
        }
    }

    /// Creates a disabled menu item.
    pub fn disabled(label: impl Into<String>, reason: Option<String>) -> Self {
        Self::Disabled {
            label: label.into(),
            reason,
        }
    }

    /// Creates a visual separator.
    pub fn separator() -> Self {
        Self::Separator
    }

    /// Whether this item can be selected/navigated to by keyboard or mouse.
    pub fn is_selectable(&self) -> bool {
        matches!(self, Self::Action { .. } | Self::More { .. })
    }

    /// Returns the display label of this item.
    pub fn label(&self) -> &str {
        match self {
            Self::Action { label, .. }
            | Self::More { label, .. }
            | Self::Disabled { label, .. } => label,
            Self::Separator => "",
        }
    }

    /// Returns the associated [`Action`], if actionable.
    pub fn action_target(&self) -> Option<Action> {
        match self {
            Self::Action { action, .. } => Some(*action),
            _ => None,
        }
    }
}

/// The state of an open contextual action menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextMenuState {
    /// The target of this menu invocation.
    pub target: Option<ContextMenuTarget>,
    /// Top-level menu items.
    pub items: Vec<ContextMenuItem>,
    /// Selected index in top-level menu.
    pub selected: usize,
    /// Origin screen position (x, y) where the menu was summoned.
    pub position: (u16, u16),
    /// Whether the "More ›" submenu is currently expanded.
    pub is_more_open: bool,
    /// Selected index in the submenu when `is_more_open` is true.
    pub more_selected: usize,
    /// Top scroll offset for long main menus in constrained terminals.
    pub scroll_offset: usize,
    /// Scroll offset for long submenus in constrained terminals.
    pub more_scroll_offset: usize,
    /// The screen Rect where the main context menu was rendered.
    pub main_area: Option<Rect>,
    /// The screen Rect where the submenu was rendered.
    pub more_area: Option<Rect>,
    /// Internal buffer for type-to-select prefix matching.
    pub type_buffer: String,
    /// Timestamp when the last type-to-select character was received.
    pub last_type_time: Option<std::time::Instant>,
}

impl ContextMenuState {
    /// Creates a new context menu state with a target, items, and screen coordinates.
    pub fn new(
        target: Option<ContextMenuTarget>,
        items: Vec<ContextMenuItem>,
        position: (u16, u16),
    ) -> Self {
        let mut state = Self {
            target,
            items,
            selected: 0,
            position,
            is_more_open: false,
            more_selected: 0,
            scroll_offset: 0,
            more_scroll_offset: 0,
            main_area: None,
            more_area: None,
            type_buffer: String::new(),
            last_type_time: None,
        };
        state.ensure_valid_selection();
        state
    }

    /// Legacy constructor for backward compatibility.
    pub fn from_items(items: Vec<ContextMenuItem>, position: (u16, u16)) -> Self {
        Self::new(None, items, position)
    }

    /// Accesses the target context of this menu.
    pub fn target(&self) -> Option<&ContextMenuTarget> {
        self.target.as_ref()
    }

    /// Ensures `selected` points to a selectable item.
    pub fn ensure_valid_selection(&mut self) {
        if self.items.is_empty() {
            self.selected = 0;
            return;
        }
        if self.selected >= self.items.len() || !self.items[self.selected].is_selectable() {
            if let Some(first) = self.items.iter().position(ContextMenuItem::is_selectable) {
                self.selected = first;
            } else {
                self.selected = 0;
            }
        }
    }

    /// Moves selection up to the previous selectable item.
    pub fn move_up(&mut self) {
        if self.is_more_open {
            if let Some(ContextMenuItem::More { items, .. }) = self.items.get(self.selected)
                && !items.is_empty()
            {
                let mut prev = self.more_selected;
                loop {
                    if prev == 0 {
                        prev = items.len().saturating_sub(1);
                    } else {
                        prev -= 1;
                    }
                    if items.get(prev).is_some_and(ContextMenuItem::is_selectable)
                        || prev == self.more_selected
                    {
                        break;
                    }
                }
                self.more_selected = prev;
            }
            return;
        }

        if self.items.is_empty() {
            return;
        }
        let mut prev = self.selected;
        loop {
            if prev == 0 {
                prev = self.items.len().saturating_sub(1);
            } else {
                prev -= 1;
            }
            if self
                .items
                .get(prev)
                .is_some_and(ContextMenuItem::is_selectable)
                || prev == self.selected
            {
                break;
            }
        }
        self.selected = prev;
    }

    /// Moves selection down to the next selectable item.
    pub fn move_down(&mut self) {
        if self.is_more_open {
            if let Some(ContextMenuItem::More { items, .. }) = self.items.get(self.selected)
                && !items.is_empty()
            {
                let mut next = self.more_selected;
                loop {
                    next = (next + 1) % items.len();
                    if items.get(next).is_some_and(ContextMenuItem::is_selectable)
                        || next == self.more_selected
                    {
                        break;
                    }
                }
                self.more_selected = next;
            }
            return;
        }

        if self.items.is_empty() {
            return;
        }
        let mut next = self.selected;
        loop {
            next = (next + 1) % self.items.len();
            if self
                .items
                .get(next)
                .is_some_and(ContextMenuItem::is_selectable)
                || next == self.selected
            {
                break;
            }
        }
        self.selected = next;
    }

    /// Jumps selection to the first selectable item (Home key).
    pub fn jump_first(&mut self) {
        if self.is_more_open {
            if let Some(ContextMenuItem::More { items, .. }) = self.items.get(self.selected)
                && let Some(idx) = items.iter().position(ContextMenuItem::is_selectable)
            {
                self.more_selected = idx;
            }
            return;
        }
        if let Some(idx) = self.items.iter().position(ContextMenuItem::is_selectable) {
            self.selected = idx;
        }
    }

    /// Jumps selection to the last selectable item (End key).
    pub fn jump_last(&mut self) {
        if self.is_more_open {
            if let Some(ContextMenuItem::More { items, .. }) = self.items.get(self.selected)
                && let Some(idx) = items.iter().rposition(ContextMenuItem::is_selectable)
            {
                self.more_selected = idx;
            }
            return;
        }
        if let Some(idx) = self.items.iter().rposition(ContextMenuItem::is_selectable) {
            self.selected = idx;
        }
    }

    /// Moves selection up by multiple items (PageUp).
    pub fn page_up(&mut self) {
        for _ in 0..5 {
            self.move_up();
        }
    }

    /// Moves selection down by multiple items (PageDown).
    pub fn page_down(&mut self) {
        for _ in 0..5 {
            self.move_down();
        }
    }

    /// Deterministic type-to-select navigation jumping selection to matching items.
    pub fn type_to_select(&mut self, ch: char) {
        let now = std::time::Instant::now();
        let is_recent = self
            .last_type_time
            .map(|t| now.duration_since(t) < std::time::Duration::from_millis(600))
            .unwrap_or(false);

        if is_recent {
            self.type_buffer.push(ch);
        } else {
            self.type_buffer = ch.to_string();
        }
        self.last_type_time = Some(now);

        let query = self.type_buffer.to_lowercase();

        if self.is_more_open {
            if let Some(ContextMenuItem::More { items, .. }) = self.items.get(self.selected) {
                if let Some(idx) = items.iter().enumerate().position(|(_, item)| {
                    item.is_selectable() && item.label().to_lowercase().starts_with(&query)
                }) {
                    self.more_selected = idx;
                    return;
                }

                if query.chars().count() > 1 {
                    let single = ch.to_lowercase().to_string();
                    if let Some(idx) = items.iter().enumerate().position(|(_, item)| {
                        item.is_selectable() && item.label().to_lowercase().starts_with(&single)
                    }) {
                        self.type_buffer = ch.to_string();
                        self.more_selected = idx;
                    }
                }
            }
            return;
        }

        // Top level menu: If single character typed repeatedly, cycle matches
        if self.type_buffer.len() == 1 {
            let ch_lower = ch.to_ascii_lowercase();
            let matches: Vec<usize> = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    if item.is_selectable()
                        && item
                            .label()
                            .chars()
                            .next()
                            .map(|c| c.to_ascii_lowercase() == ch_lower)
                            .unwrap_or(false)
                    {
                        Some(idx)
                    } else {
                        None
                    }
                })
                .collect();

            if !matches.is_empty() {
                if let Some(pos) = matches.iter().position(|&idx| idx == self.selected) {
                    let next_pos = (pos + 1) % matches.len();
                    self.selected = matches[next_pos];
                    return;
                } else {
                    self.selected = matches[0];
                    return;
                }
            }
        }

        // Search full query prefix
        if let Some(idx) = self.items.iter().enumerate().position(|(_, item)| {
            item.is_selectable() && item.label().to_lowercase().starts_with(&query)
        }) {
            self.selected = idx;
            return;
        }

        // Fallback to single character if multi-char failed
        if self.type_buffer.chars().count() > 1 {
            let single = ch.to_lowercase().to_string();
            if let Some(idx) = self.items.iter().enumerate().position(|(_, item)| {
                item.is_selectable() && item.label().to_lowercase().starts_with(&single)
            }) {
                self.type_buffer = ch.to_string();
                self.selected = idx;
            }
        }
    }

    /// Expands the submenu for the currently selected `More` item.
    pub fn open_more(&mut self) {
        if let Some(ContextMenuItem::More { items, .. }) = self.items.get(self.selected)
            && !items.is_empty()
        {
            self.is_more_open = true;
            self.more_selected = items
                .iter()
                .position(ContextMenuItem::is_selectable)
                .unwrap_or(0);
        }
    }

    /// Closes the expanded submenu.
    pub fn close_more(&mut self) {
        self.is_more_open = false;
        self.more_selected = 0;
    }

    /// Returns the [`Action`] for the currently selected item, taking submenu state into account.
    pub fn selected_action(&self) -> Option<Action> {
        if self.is_more_open {
            if let Some(ContextMenuItem::More { items, .. }) = self.items.get(self.selected)
                && let Some(ContextMenuItem::Action { action, .. }) = items.get(self.more_selected)
            {
                return Some(*action);
            }
            return None;
        }

        if let Some(ContextMenuItem::Action { action, .. }) = self.items.get(self.selected) {
            Some(*action)
        } else {
            None
        }
    }

    /// Selects an explicit top-level index if valid.
    pub fn select_index(&mut self, index: usize) {
        if index < self.items.len() && self.items[index].is_selectable() {
            self.selected = index;
        }
    }

    /// Selects an explicit submenu index if valid.
    pub fn select_more_index(&mut self, index: usize) {
        if let Some(ContextMenuItem::More { items, .. }) = self.items.get(self.selected)
            && index < items.len()
            && items[index].is_selectable()
        {
            self.more_selected = index;
        }
    }
}

/// Helper to create a [`ContextMenuItem::Action`] with dynamic shortcut resolution from [`ShortcutRegistry`].
fn action_item(action: Action, label: impl Into<String>, platform: Platform) -> ContextMenuItem {
    let shortcut = ShortcutRegistry::global().primary_shortcut(action, platform);
    ContextMenuItem::Action {
        action,
        label: label.into(),
        shortcut,
    }
}

/// Builds contextual action menu items strictly mapping to the [`ActionRegistry`].
pub fn build_context_menu_items(
    target: &ContextMenuTarget,
    has_clipboard: bool,
    platform: Platform,
) -> Vec<ContextMenuItem> {
    let mut items = Vec::new();

    match target {
        ContextMenuTarget::MultiSelection { count, .. } => {
            let n = *count;
            items.push(action_item(
                Action::Copy,
                format!("Copy {n} Items"),
                platform,
            ));
            items.push(action_item(Action::Cut, format!("Cut {n} Items"), platform));
            items.push(action_item(
                Action::Delete,
                format!("Delete {n} Items"),
                platform,
            ));
            items.push(ContextMenuItem::Separator);
            items.push(action_item(Action::CopyPath, "Copy Paths", platform));
            items.push(action_item(Action::GetInfo, "Get Info", platform));
            items.push(ContextMenuItem::Separator);
            items.push(ContextMenuItem::More {
                label: "More".to_string(),
                items: vec![
                    action_item(Action::SelectAll, "Select All", platform),
                    action_item(Action::DeselectAll, "Deselect All", platform),
                    action_item(Action::InvertSelection, "Invert Selection", platform),
                ],
            });
        }

        ContextMenuTarget::Directory {
            is_project_root,
            in_git,
            ..
        } => {
            items.push(action_item(Action::Open, "Open Folder", platform));
            items.push(action_item(
                Action::OpenInNewTab,
                "Open in New Tab",
                platform,
            ));
            items.push(ContextMenuItem::Separator);
            items.push(action_item(Action::Copy, "Copy", platform));
            items.push(action_item(Action::Cut, "Cut", platform));
            if has_clipboard {
                items.push(action_item(Action::Paste, "Paste", platform));
            } else {
                items.push(ContextMenuItem::Disabled {
                    label: "Paste".to_string(),
                    reason: Some("Clipboard empty".to_string()),
                });
            }
            items.push(action_item(Action::Rename, "Rename", platform));
            items.push(action_item(Action::Delete, "Delete", platform));
            items.push(ContextMenuItem::Separator);
            items.push(action_item(
                Action::ToggleFavorite,
                "Toggle Favorite",
                platform,
            ));
            items.push(action_item(
                Action::StorageVision,
                "Storage Information",
                platform,
            ));
            items.push(action_item(Action::CopyPath, "Copy Path", platform));
            items.push(action_item(Action::CopyName, "Copy Name", platform));
            items.push(action_item(Action::GetInfo, "Get Info", platform));
            items.push(ContextMenuItem::Separator);

            let mut more_items = vec![
                action_item(Action::NewFile, "New File", platform),
                action_item(Action::NewDirectory, "New Folder", platform),
                action_item(Action::SelectAll, "Select All", platform),
                action_item(Action::ToggleHidden, "Toggle Hidden Files", platform),
            ];

            if *is_project_root {
                more_items.push(action_item(
                    Action::ProjectCockpit,
                    "Project Overview",
                    platform,
                ));
            }
            if *in_git {
                more_items.push(action_item(
                    Action::GitStatusPanel,
                    "Git Status Panel",
                    platform,
                ));
            }

            items.push(ContextMenuItem::More {
                label: "More".to_string(),
                items: more_items,
            });
        }

        ContextMenuTarget::File { .. } => {
            items.push(action_item(Action::Open, "Open", platform));
            items.push(action_item(Action::Preview, "Quick Preview", platform));
            items.push(ContextMenuItem::Separator);
            items.push(action_item(Action::Copy, "Copy", platform));
            items.push(action_item(Action::Cut, "Cut", platform));
            if has_clipboard {
                items.push(action_item(Action::Paste, "Paste", platform));
            } else {
                items.push(ContextMenuItem::Disabled {
                    label: "Paste".to_string(),
                    reason: Some("Clipboard empty".to_string()),
                });
            }
            items.push(action_item(Action::Rename, "Rename", platform));
            items.push(action_item(Action::Delete, "Delete", platform));
            items.push(ContextMenuItem::Separator);
            items.push(action_item(Action::CopyPath, "Copy Path", platform));
            items.push(action_item(Action::CopyName, "Copy Name", platform));
            items.push(action_item(Action::GetInfo, "Get Info", platform));
            items.push(ContextMenuItem::Separator);
            items.push(ContextMenuItem::More {
                label: "More".to_string(),
                items: vec![
                    action_item(Action::SelectAll, "Select All", platform),
                    action_item(Action::ToggleHidden, "Toggle Hidden Files", platform),
                    action_item(Action::RevealContext, "Reveal in Context", platform),
                ],
            });
        }

        ContextMenuTarget::Empty { .. } => {
            items.push(action_item(Action::NewDirectory, "New Folder", platform));
            items.push(action_item(Action::NewFile, "New File", platform));
            if has_clipboard {
                items.push(action_item(Action::Paste, "Paste", platform));
            } else {
                items.push(ContextMenuItem::Disabled {
                    label: "Paste".to_string(),
                    reason: Some("Clipboard empty".to_string()),
                });
            }
            items.push(action_item(Action::RefreshDirectory, "Refresh", platform));
            items.push(ContextMenuItem::Separator);
            items.push(ContextMenuItem::More {
                label: "More".to_string(),
                items: vec![
                    action_item(Action::SelectAll, "Select All", platform),
                    action_item(Action::ToggleHidden, "Toggle Hidden Files", platform),
                ],
            });
        }
    }

    items
}
