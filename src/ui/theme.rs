//! Centralized theme, styling, and visual identity system for TerminalVision.
//!
//! Provides semantic styling tokens, typographic hierarchy, responsive spacing constants,
//! and terminal capability fallback symbols.
//!
//! This module belongs strictly to the UI layer and never mutates application state
//! or performs filesystem operations.

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::BorderType;

use crate::filesystem::entry::EntryKind;
use crate::git::FileStatus;

/// Semantic notification severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    /// Informational message.
    Info,
    /// Success confirmation.
    Success,
    /// Warning or non-fatal alert.
    Warning,
    /// Error condition.
    Error,
}

/// Visual spacing constants to enforce clean, predictable terminal density.
pub struct Spacing;

impl Spacing {
    /// Inner padding for modal dialogs (horizontal cells).
    pub const MODAL_PADDING_X: u16 = 2;
    /// Inner padding for modal dialogs (vertical rows).
    pub const MODAL_PADDING_Y: u16 = 1;
    /// Minimum column width required for multi-pane split layout.
    pub const DUAL_PANE_MIN_WIDTH: u16 = 80;
    /// Minimum column width required for 3-column preview layout.
    pub const PREVIEW_PANE_MIN_WIDTH: u16 = 160;
    /// Maximum width for centered modal dialogs.
    pub const MAX_MODAL_WIDTH: u16 = 60;
    /// Maximum height for centered modal dialogs.
    pub const MAX_MODAL_HEIGHT: u16 = 20;
}

/// Glyphs and indicator symbols used across the user interface.
#[derive(Debug, Clone, Copy)]
pub struct Symbols {
    /// Prefix indicator for selected entries.
    pub selection_marker: &'static str,
    /// Prefix indicator for unselected entries.
    pub unselected_marker: &'static str,
    /// Active pane / tab focus bullet.
    pub active_indicator: &'static str,
    /// Inactive pane / tab focus bullet.
    pub inactive_indicator: &'static str,
    /// Vertical delimiter for headers, tabs, and status bars.
    pub vertical_separator: &'static str,
    /// Directory entry icon/glyph.
    pub dir_icon: &'static str,
    /// Regular file entry icon/glyph.
    pub file_icon: &'static str,
    /// Symlink entry icon/glyph.
    pub symlink_icon: &'static str,
    /// Broken symlink entry icon/glyph.
    pub broken_symlink_icon: &'static str,
    /// Code source file entry icon/glyph.
    pub code_icon: &'static str,
    /// Image file entry icon/glyph.
    pub image_icon: &'static str,
    /// Archive file entry icon/glyph.
    pub archive_icon: &'static str,
    /// Document file entry icon/glyph.
    pub doc_icon: &'static str,
    /// Configuration file entry icon/glyph.
    pub config_icon: &'static str,
    /// Executable binary/script entry icon/glyph.
    pub executable_icon: &'static str,
    /// Other/unrecognized entry icon/glyph.
    pub other_icon: &'static str,
    /// Scroll indicator pointing upward.
    pub scroll_up: &'static str,
    /// Scroll indicator pointing downward.
    pub scroll_down: &'static str,
    /// Status prefix for errors / alerts.
    pub error_prefix: &'static str,
    /// Status prefix for warnings.
    pub warning_prefix: &'static str,
    /// Status prefix for success.
    pub success_prefix: &'static str,
    /// Status prefix for info.
    pub info_prefix: &'static str,
}

impl Default for Symbols {
    fn default() -> Self {
        Self::standard()
    }
}

impl Symbols {
    /// Standard Unicode symbol set (rich terminal aesthetics).
    pub const fn standard() -> Self {
        Self {
            selection_marker: "> ",
            unselected_marker: "  ",
            active_indicator: "●",
            inactive_indicator: "○",
            vertical_separator: "│",
            dir_icon: "📁",
            file_icon: "- ",
            symlink_icon: "@ ",
            broken_symlink_icon: "! ",
            code_icon: "📜",
            image_icon: "🖼 ",
            archive_icon: "📦",
            doc_icon: "📄",
            config_icon: "⚙ ",
            executable_icon: "⚡",
            other_icon: "? ",
            scroll_up: "↑",
            scroll_down: "↓",
            error_prefix: "[!]",
            warning_prefix: "[!]",
            success_prefix: "[✓]",
            info_prefix: "[i]",
        }
    }

    /// Pure ASCII fallback symbol set for restricted terminals.
    pub const fn ascii() -> Self {
        Self {
            selection_marker: "> ",
            unselected_marker: "  ",
            active_indicator: "*",
            inactive_indicator: " ",
            vertical_separator: "|",
            dir_icon: "D ",
            file_icon: "- ",
            symlink_icon: "@ ",
            broken_symlink_icon: "! ",
            code_icon: "c ",
            image_icon: "i ",
            archive_icon: "z ",
            doc_icon: "d ",
            config_icon: ". ",
            executable_icon: "* ",
            other_icon: "? ",
            scroll_up: "^",
            scroll_down: "v",
            error_prefix: "[!]",
            warning_prefix: "[!]",
            success_prefix: "[+]",
            info_prefix: "[i]",
        }
    }
}

/// Centralized UI theme containing all semantic styles.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Active symbol set.
    pub symbols: Symbols,

    // --- Header & Branding ---
    /// Application brand title style (`TerminalVision`).
    pub app_title: Style,
    /// Header active directory path style.
    pub header_path: Style,
    /// Header delimiter style.
    pub header_separator: Style,
    /// Project awareness text style (clean repo / general).
    pub project_info_clean: Style,
    /// Project awareness text style (uncommitted modifications).
    pub project_info_dirty: Style,

    // --- Panes & Layout ---
    /// Active pane border type.
    pub active_pane_border_type: BorderType,
    /// Active pane border style.
    pub active_pane_border: Style,
    /// Active pane title style.
    pub active_pane_title: Style,
    /// Inactive pane border type.
    pub inactive_pane_border_type: BorderType,
    /// Inactive pane border style.
    pub inactive_pane_border: Style,
    /// Inactive pane title style.
    pub inactive_pane_title: Style,

    // --- Tabs ---
    /// Active tab in active pane.
    pub tab_active_focused: Style,
    /// Active tab in inactive pane.
    pub tab_active_unfocused: Style,
    /// Inactive tab.
    pub tab_inactive: Style,
    /// Tab separator delimiter.
    pub tab_separator: Style,

    // --- File Rows & Selections ---
    /// Row style when selected in active pane.
    pub row_selected_active: Style,
    /// Row style when selected in inactive pane.
    pub row_selected_inactive: Style,
    /// Row style when multi-selected.
    pub row_multi_selected: Style,
    /// Row style when unselected.
    pub row_unselected: Style,

    // --- Entry Kinds ---
    /// Directory icon & name style.
    pub entry_directory: Style,
    /// Regular file icon & name style.
    pub entry_file: Style,
    /// Symlink icon & name style.
    pub entry_symlink: Style,
    /// Broken symlink style.
    pub entry_broken_symlink: Style,
    /// Hidden (dot) file style.
    pub entry_hidden: Style,
    /// Executable file style.
    pub entry_executable: Style,
    /// Code source file style.
    pub entry_code: Style,
    /// Image file style.
    pub entry_image: Style,
    /// Archive file style.
    pub entry_archive: Style,
    /// Document file style.
    pub entry_doc: Style,
    /// Configuration file style.
    pub entry_config: Style,
    /// Other / special entry style.
    pub entry_other: Style,

    // --- Git Statuses ---
    /// Git Modified status (`M`).
    pub git_modified: Style,
    /// Git Added / Staged status (`A`).
    pub git_added: Style,
    /// Git Deleted status (`D`).
    pub git_deleted: Style,
    /// Git Renamed status (`R`).
    pub git_renamed: Style,
    /// Git Untracked status (`?`).
    pub git_untracked: Style,
    /// Git Clean status (`✓`).
    pub git_clean: Style,

    // --- File Preview ---
    /// Preview border style.
    pub preview_border: Style,
    /// Preview header title style.
    pub preview_title: Style,
    /// Line number column style.
    pub preview_line_number: Style,
    /// Preview metadata label style.
    pub preview_metadata_label: Style,
    /// Preview metadata value style.
    pub preview_metadata_value: Style,
    /// Preview empty / fallback text style.
    pub preview_muted: Style,

    // --- Footer & Status Bar ---
    /// Current interaction mode badge.
    pub footer_mode: Style,
    /// Active pane indicator (`LEFT` / `RIGHT`).
    pub footer_pane: Style,
    /// Selection counter (`X / Y`).
    pub footer_selection: Style,
    /// Active clipboard operation badge (`COPY` / `CUT`).
    pub footer_clipboard: Style,
    /// Keyboard shortcut hint text.
    pub footer_hint: Style,
    /// Footer separator delimiter.
    pub footer_separator: Style,

    // --- Notifications & Alerts ---
    /// Info notification prefix & message.
    pub notify_info: Style,
    /// Success notification prefix & message.
    pub notify_success: Style,
    /// Warning notification prefix & message.
    pub notify_warning: Style,
    /// Error notification badge & message.
    pub notify_error_badge: Style,
    /// Error notification text.
    pub notify_error_text: Style,

    // --- Dialogs & Modals ---
    /// Dialog overlay border (standard / input / help).
    pub dialog_border: Style,
    /// Dialog overlay border (confirm / warning).
    pub dialog_border_confirm: Style,
    /// Dialog title style.
    pub dialog_title: Style,
    /// Dialog prompt/message text style.
    pub dialog_message: Style,
    /// Dialog input box style.
    pub dialog_input: Style,
    /// Dialog active/selected button.
    pub dialog_button_active: Style,
    /// Dialog inactive/unselected button.
    pub dialog_button_inactive: Style,
    /// Dialog footer navigation hint.
    pub dialog_hint: Style,

    // --- Command Palette ---
    /// Command palette search input text.
    pub palette_input: Style,
    /// Command palette selected command item.
    pub palette_selected: Style,
    /// Command palette unselected command item.
    pub palette_unselected: Style,
    /// Command palette description text.
    pub palette_description: Style,
    /// Command palette shortcut badge.
    pub palette_shortcut: Style,

    // --- Help Dialog ---
    /// Help section category header.
    pub help_category: Style,
    /// Help keybinding badge.
    pub help_key: Style,
    /// Help action description.
    pub help_desc: Style,

    // --- Empty States ---
    /// Empty directory / no matches placeholder text.
    pub empty_state_text: Style,

    // --- Integrated Terminal Panel ---
    /// Active / focused terminal border type.
    pub active_terminal_border_type: BorderType,
    /// Active / focused terminal border style.
    pub active_terminal_border: Style,
    /// Inactive / unfocused terminal border type.
    pub inactive_terminal_border_type: BorderType,
    /// Inactive / unfocused terminal border style.
    pub inactive_terminal_border: Style,
    /// Terminal title style when focused.
    pub terminal_title_focused: Style,
    /// Terminal title style when unfocused.
    pub terminal_title_unfocused: Style,
    /// Terminal metadata style when focused.
    pub terminal_meta_focused: Style,
    /// Terminal metadata style when unfocused.
    pub terminal_meta_unfocused: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self::signature()
    }
}

impl Theme {
    /// TerminalVision Signature Edition theme.
    ///
    /// Crisp, high-contrast, technical terminal palette designed for maximum legibility
    /// and information density on standard dark terminal emulators.
    pub const fn signature() -> Self {
        Self {
            symbols: Symbols::standard(),

            // Header & Branding
            app_title: Style::new().add_modifier(Modifier::BOLD),
            header_path: Style::new().fg(Color::Yellow),
            header_separator: Style::new().fg(Color::DarkGray),
            project_info_clean: Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
            project_info_dirty: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),

            // Panes & Layout
            active_pane_border_type: BorderType::Double,
            active_pane_border: Style::new().fg(Color::Cyan),
            active_pane_title: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            inactive_pane_border_type: BorderType::Plain,
            inactive_pane_border: Style::new().fg(Color::DarkGray),
            inactive_pane_title: Style::new().fg(Color::DarkGray),

            // Tabs
            tab_active_focused: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            tab_active_unfocused: Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            tab_inactive: Style::new().fg(Color::DarkGray),
            tab_separator: Style::new().fg(Color::DarkGray),

            // File Rows & Selections
            row_selected_active: Style::new()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            row_selected_inactive: Style::new().bg(Color::Rgb(40, 40, 40)).fg(Color::Gray),
            row_multi_selected: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            row_unselected: Style::new(),

            // Entry Kinds
            entry_directory: Style::new().fg(Color::Blue),
            entry_file: Style::new().fg(Color::White),
            entry_symlink: Style::new().fg(Color::Magenta),
            entry_broken_symlink: Style::new().fg(Color::Red),
            entry_hidden: Style::new().fg(Color::DarkGray),
            entry_executable: Style::new().fg(Color::Green),
            entry_code: Style::new().fg(Color::LightGreen),
            entry_image: Style::new().fg(Color::LightMagenta),
            entry_archive: Style::new().fg(Color::LightRed),
            entry_doc: Style::new().fg(Color::LightCyan),
            entry_config: Style::new().fg(Color::Yellow),
            entry_other: Style::new().fg(Color::DarkGray),

            // Git Statuses
            git_modified: Style::new().fg(Color::Yellow),
            git_added: Style::new().fg(Color::Green),
            git_deleted: Style::new().fg(Color::Red),
            git_renamed: Style::new().fg(Color::Magenta),
            git_untracked: Style::new().fg(Color::DarkGray),
            git_clean: Style::new().fg(Color::Green),

            // File Preview
            preview_border: Style::new().fg(Color::DarkGray),
            preview_title: Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            preview_line_number: Style::new().fg(Color::DarkGray),
            preview_metadata_label: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            preview_metadata_value: Style::new().fg(Color::White),
            preview_muted: Style::new().fg(Color::DarkGray),

            // Footer & Status Bar
            footer_mode: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            footer_pane: Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            footer_selection: Style::new().fg(Color::White),
            footer_clipboard: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            footer_hint: Style::new().fg(Color::DarkGray),
            footer_separator: Style::new().fg(Color::DarkGray),

            // Notifications & Alerts
            notify_info: Style::new().fg(Color::Cyan),
            notify_success: Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
            notify_warning: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            notify_error_badge: Style::new()
                .fg(Color::White)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
            notify_error_text: Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),

            // Dialogs & Modals
            dialog_border: Style::new().fg(Color::Cyan),
            dialog_border_confirm: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            dialog_title: Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            dialog_message: Style::new().fg(Color::White),
            dialog_input: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            dialog_button_active: Style::new()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            dialog_button_inactive: Style::new().fg(Color::DarkGray),
            dialog_hint: Style::new().fg(Color::DarkGray),

            // Command Palette
            palette_input: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            palette_selected: Style::new()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            palette_unselected: Style::new().fg(Color::White),
            palette_description: Style::new().fg(Color::Gray),
            palette_shortcut: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),

            // Help Dialog
            help_category: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            help_key: Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            help_desc: Style::new().fg(Color::White),

            // Empty States
            empty_state_text: Style::new().fg(Color::DarkGray),

            // Integrated Terminal Panel
            active_terminal_border_type: BorderType::Thick,
            active_terminal_border: Style::new().fg(Color::LightCyan),
            inactive_terminal_border_type: BorderType::Plain,
            inactive_terminal_border: Style::new().fg(Color::DarkGray),
            terminal_title_focused: Style::new()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
            terminal_title_unfocused: Style::new()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
            terminal_meta_focused: Style::new().fg(Color::LightCyan),
            terminal_meta_unfocused: Style::new().fg(Color::DarkGray),
        }
    }

    /// Resolves the border type and style for the integrated terminal panel.
    pub fn terminal_border(&self, is_focused: bool) -> (BorderType, Style) {
        if is_focused {
            (
                self.active_terminal_border_type,
                self.active_terminal_border,
            )
        } else {
            (
                self.inactive_terminal_border_type,
                self.inactive_terminal_border,
            )
        }
    }

    /// Resolves the semantic style for a file list row given selection and active pane state.
    pub fn row_style(&self, is_selected: bool, is_active: bool) -> Style {
        if !is_selected {
            self.row_unselected
        } else if is_active {
            self.row_selected_active
        } else {
            self.row_selected_inactive
        }
    }

    /// Resolves the icon and style for an entry kind.
    pub fn entry_kind_indicator(&self, kind: EntryKind) -> (&'static str, Style) {
        match kind {
            EntryKind::Directory => (self.symbols.dir_icon, self.entry_directory),
            EntryKind::File => (self.symbols.file_icon, self.entry_file),
            EntryKind::Symlink => (self.symbols.symlink_icon, self.entry_symlink),
            EntryKind::Other => (self.symbols.other_icon, self.entry_other),
        }
    }

    /// Resolves the semantic file icon and style based on entry name, kind, and extension.
    pub fn file_icon_and_style(&self, name: &str, kind: EntryKind) -> (&'static str, Style) {
        if name.starts_with('.') && name != "." && name != ".." && kind != EntryKind::Directory {
            return (self.symbols.config_icon, self.entry_hidden);
        }

        match kind {
            EntryKind::Directory => (self.symbols.dir_icon, self.entry_directory),
            EntryKind::Symlink => (self.symbols.symlink_icon, self.entry_symlink),
            EntryKind::Other => (self.symbols.other_icon, self.entry_other),
            EntryKind::File => {
                let ext = std::path::Path::new(name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();

                match ext.as_str() {
                    "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "cpp" | "cc" | "h"
                    | "hpp" | "go" | "java" | "html" | "css" | "scss" | "php" | "rb" | "lua"
                    | "swift" | "kt" | "scala" | "zig" | "nim" | "hs" | "ml" | "sql" => {
                        (self.symbols.code_icon, self.entry_code)
                    }
                    "sh" | "bash" | "zsh" | "exe" | "bin" | "bat" | "cmd" => {
                        (self.symbols.executable_icon, self.entry_executable)
                    }
                    "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp" | "ico" => {
                        (self.symbols.image_icon, self.entry_image)
                    }
                    "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "tgz" | "zst" => {
                        (self.symbols.archive_icon, self.entry_archive)
                    }
                    "md" | "txt" | "pdf" | "doc" | "docx" | "rtf" | "epub" | "csv" => {
                        (self.symbols.doc_icon, self.entry_doc)
                    }
                    "toml" | "json" | "yaml" | "yml" | "ini" | "conf" | "env" | "xml" | "lock" => {
                        (self.symbols.config_icon, self.entry_config)
                    }
                    _ => (self.symbols.file_icon, self.entry_file),
                }
            }
        }
    }

    /// Resolves the selection prefix marker and style.
    pub fn row_marker(
        &self,
        is_cursor: bool,
        is_multi_selected: bool,
        is_active: bool,
    ) -> (&'static str, Style) {
        match (is_cursor, is_multi_selected, is_active) {
            (true, true, true) => (">✓", self.row_selected_active),
            (true, true, false) => (">✓", self.row_selected_inactive),
            (true, false, true) => (self.symbols.selection_marker, self.row_selected_active),
            (true, false, false) => (self.symbols.selection_marker, self.row_selected_inactive),
            (false, true, _) => (" ✓", self.row_multi_selected),
            (false, false, _) => (self.symbols.unselected_marker, self.row_unselected),
        }
    }

    /// Resolves the Git status character code and style.
    pub fn git_file_status(&self, status: FileStatus) -> (&'static str, Style) {
        match status {
            FileStatus::Modified => ("M ", self.git_modified),
            FileStatus::Added => ("A ", self.git_added),
            FileStatus::Deleted => ("D ", self.git_deleted),
            FileStatus::Renamed => ("R ", self.git_renamed),
            FileStatus::Untracked => ("? ", self.git_untracked),
            FileStatus::Clean => ("✓ ", self.git_clean),
        }
    }

    /// Resolves the border type and border style for a file pane.
    pub fn pane_border(&self, is_active: bool) -> (BorderType, Style) {
        if is_active {
            (self.active_pane_border_type, self.active_pane_border)
        } else {
            (self.inactive_pane_border_type, self.inactive_pane_border)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_row_style_invariants() {
        let theme = Theme::signature();
        assert_eq!(theme.row_style(false, true), Style::default());
        assert_eq!(theme.row_style(false, false), Style::default());

        let active_sel = theme.row_style(true, true);
        assert!(active_sel.add_modifier.contains(Modifier::BOLD));
        assert_eq!(active_sel.bg, Some(Color::DarkGray));

        let inactive_sel = theme.row_style(true, false);
        assert!(!inactive_sel.add_modifier.contains(Modifier::BOLD));
        assert_eq!(inactive_sel.bg, Some(Color::Rgb(40, 40, 40)));
    }

    #[test]
    fn theme_entry_indicators_are_distinct() {
        let theme = Theme::signature();
        let (dir_sym, dir_style) = theme.entry_kind_indicator(EntryKind::Directory);
        let (file_sym, file_style) = theme.entry_kind_indicator(EntryKind::File);
        let (sym_sym, sym_style) = theme.entry_kind_indicator(EntryKind::Symlink);
        let (other_sym, other_style) = theme.entry_kind_indicator(EntryKind::Other);

        assert_ne!(dir_sym, file_sym);
        assert_ne!(dir_style, file_style);
        assert_ne!(sym_sym, other_sym);
        assert_ne!(sym_style, other_style);
    }

    #[test]
    fn theme_pane_border_distinction() {
        let theme = Theme::signature();
        let (active_bt, active_st) = theme.pane_border(true);
        let (inactive_bt, inactive_st) = theme.pane_border(false);

        assert_eq!(active_bt, BorderType::Double);
        assert_eq!(inactive_bt, BorderType::Plain);
        assert_ne!(active_st, inactive_st);
    }

    #[test]
    fn symbols_standard_vs_ascii_completeness() {
        let std_sym = Symbols::standard();
        let asc_sym = Symbols::ascii();

        assert_eq!(std_sym.selection_marker, "> ");
        assert_eq!(asc_sym.selection_marker, "> ");
        assert_eq!(std_sym.active_indicator, "●");
        assert_eq!(asc_sym.active_indicator, "*");
    }

    #[test]
    fn git_file_status_mappings() {
        let theme = Theme::signature();
        assert_eq!(theme.git_file_status(FileStatus::Modified).0, "M ");
        assert_eq!(theme.git_file_status(FileStatus::Added).0, "A ");
        assert_eq!(theme.git_file_status(FileStatus::Deleted).0, "D ");
        assert_eq!(theme.git_file_status(FileStatus::Renamed).0, "R ");
        assert_eq!(theme.git_file_status(FileStatus::Untracked).0, "? ");
        assert_eq!(theme.git_file_status(FileStatus::Clean).0, "✓ ");
    }

    #[test]
    fn file_icon_and_style_classification() {
        let theme = Theme::signature();
        assert_eq!(
            theme.file_icon_and_style("main.rs", EntryKind::File).0,
            theme.symbols.code_icon
        );
        assert_eq!(
            theme.file_icon_and_style("image.png", EntryKind::File).0,
            theme.symbols.image_icon
        );
        assert_eq!(
            theme.file_icon_and_style("archive.zip", EntryKind::File).0,
            theme.symbols.archive_icon
        );
        assert_eq!(
            theme.file_icon_and_style("README.md", EntryKind::File).0,
            theme.symbols.doc_icon
        );
        assert_eq!(
            theme.file_icon_and_style("Cargo.toml", EntryKind::File).0,
            theme.symbols.config_icon
        );
        assert_eq!(
            theme.file_icon_and_style("build.sh", EntryKind::File).0,
            theme.symbols.executable_icon
        );
        assert_eq!(
            theme.file_icon_and_style(".gitignore", EntryKind::File).0,
            theme.symbols.config_icon
        );
        assert_eq!(
            theme.file_icon_and_style("src", EntryKind::Directory).0,
            theme.symbols.dir_icon
        );
        assert_eq!(
            theme.file_icon_and_style("link", EntryKind::Symlink).0,
            theme.symbols.symlink_icon
        );
    }

    #[test]
    fn row_marker_combinations() {
        let theme = Theme::signature();
        assert_eq!(theme.row_marker(true, false, true).0, "> ");
        assert_eq!(theme.row_marker(false, false, true).0, "  ");
        assert_eq!(theme.row_marker(true, true, true).0, ">✓");
        assert_eq!(theme.row_marker(false, true, true).0, " ✓");
    }
}
