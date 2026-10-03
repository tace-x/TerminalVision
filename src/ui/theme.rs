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

/// Identifiers for all built-in TerminalVision themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ThemeId {
    /// Signature TerminalVision theme (cyan/blue/purple accents).
    #[default]
    TerminalVision,
    /// Deep dark theme with restrained cool accents.
    Midnight,
    /// Neon yellow and magenta cyberpunk aesthetic.
    Cyberpunk,
    /// Deep blue and oceanic cyan palette.
    Ocean,
    /// Purple and pink gothic dark palette.
    Dracula,
    /// Muted cool arctic palette.
    Nord,
    /// Phosphor green terminal matrix palette.
    Matrix,
    /// Precision dark Solarized palette.
    SolarizedDark,
    /// Pure grayscale focused palette.
    Monochrome,
    /// Maximum distinction and accessible high-contrast palette.
    HighContrast,
}

impl ThemeId {
    /// All 10 built-in theme identifiers in canonical display order.
    pub const ALL: [ThemeId; 10] = [
        ThemeId::TerminalVision,
        ThemeId::Midnight,
        ThemeId::Cyberpunk,
        ThemeId::Ocean,
        ThemeId::Dracula,
        ThemeId::Nord,
        ThemeId::Matrix,
        ThemeId::SolarizedDark,
        ThemeId::Monochrome,
        ThemeId::HighContrast,
    ];

    /// Returns a slice of all built-in theme IDs.
    pub const fn all() -> &'static [ThemeId; 10] {
        &Self::ALL
    }

    /// Canonical string identifier for persistence and command parsing.
    pub const fn id_str(self) -> &'static str {
        match self {
            Self::TerminalVision => "terminalvision",
            Self::Midnight => "midnight",
            Self::Cyberpunk => "cyberpunk",
            Self::Ocean => "ocean",
            Self::Dracula => "dracula",
            Self::Nord => "nord",
            Self::Matrix => "matrix",
            Self::SolarizedDark => "solarized-dark",
            Self::Monochrome => "monochrome",
            Self::HighContrast => "high-contrast",
        }
    }

    /// Human-friendly display title for UI headers and theme picker.
    pub const fn name(self) -> &'static str {
        match self {
            Self::TerminalVision => "TerminalVision",
            Self::Midnight => "Midnight",
            Self::Cyberpunk => "Cyberpunk",
            Self::Ocean => "Ocean",
            Self::Dracula => "Dracula",
            Self::Nord => "Nord",
            Self::Matrix => "Matrix",
            Self::SolarizedDark => "Solarized Dark",
            Self::Monochrome => "Monochrome",
            Self::HighContrast => "High Contrast",
        }
    }

    /// Short thematic description.
    pub const fn description(self) -> &'static str {
        match self {
            Self::TerminalVision => "Signature technical identity with cyan, purple & blue accents",
            Self::Midnight => "Deep obsidian dark background with restrained cool accents",
            Self::Cyberpunk => "High-energy neon yellow & hot magenta cyberpunk palette",
            Self::Ocean => "Submerged deep oceanic blues and crisp teal highlights",
            Self::Dracula => "Classic vampire gothic dark with purple and pink accents",
            Self::Nord => "Arctic cool blue, frost and aurora borealis palette",
            Self::Matrix => "Retro CRT phosphor green and deep pitch black matrix",
            Self::SolarizedDark => "Precision engineered Solarized dark color structure",
            Self::Monochrome => "Minimalist pure grayscale with maximum clarity",
            Self::HighContrast => "WCAG AAA compliant high-distinction accessible palette",
        }
    }

    /// Resolves a string identifier to a ThemeId, case-insensitively.
    pub fn from_id(id: &str) -> Option<Self> {
        match id.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "terminalvision" | "signature" | "default" => Some(Self::TerminalVision),
            "midnight" => Some(Self::Midnight),
            "cyberpunk" => Some(Self::Cyberpunk),
            "ocean" => Some(Self::Ocean),
            "dracula" => Some(Self::Dracula),
            "nord" => Some(Self::Nord),
            "matrix" => Some(Self::Matrix),
            "solarized-dark" | "solarized" => Some(Self::SolarizedDark),
            "monochrome" | "mono" | "grayscale" => Some(Self::Monochrome),
            "high-contrast" | "highcontrast" | "contrast" => Some(Self::HighContrast),
            _ => None,
        }
    }
}

/// Semantic color palette for a theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePalette {
    pub background: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub foreground: Color,
    pub foreground_muted: Color,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub selection: Color,
    pub selection_foreground: Color,
    pub border: Color,
    pub border_active: Color,
    pub focus: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    pub directory: Color,
    pub symlink: Color,
    pub executable: Color,
    pub git_clean: Color,
    pub git_modified: Color,
    pub git_added: Color,
    pub git_deleted: Color,
    pub git_renamed: Color,
    pub git_untracked: Color,
    pub preview: Color,
    pub overlay: Color,
    pub disabled: Color,

    // Embedded terminal ANSI 16-color palette
    pub ansi_black: Color,
    pub ansi_red: Color,
    pub ansi_green: Color,
    pub ansi_yellow: Color,
    pub ansi_blue: Color,
    pub ansi_magenta: Color,
    pub ansi_cyan: Color,
    pub ansi_white: Color,
    pub ansi_bright_black: Color,
    pub ansi_bright_red: Color,
    pub ansi_bright_green: Color,
    pub ansi_bright_yellow: Color,
    pub ansi_bright_blue: Color,
    pub ansi_bright_magenta: Color,
    pub ansi_bright_cyan: Color,
    pub ansi_bright_white: Color,
    pub cursor: Color,
}

impl ThemePalette {
    /// Returns 4 prominent accent colors for visual swatch previewing.
    pub const fn swatches(&self) -> [Color; 4] {
        [self.primary, self.secondary, self.accent, self.success]
    }

    /// Maps standard terminal ANSI colors to this theme's synchronized palette.
    pub fn map_ansi(&self, color: Color) -> Color {
        match color {
            Color::Black => self.ansi_black,
            Color::Red => self.ansi_red,
            Color::Green => self.ansi_green,
            Color::Yellow => self.ansi_yellow,
            Color::Blue => self.ansi_blue,
            Color::Magenta => self.ansi_magenta,
            Color::Cyan => self.ansi_cyan,
            Color::White | Color::Gray => self.ansi_white,
            Color::DarkGray => self.ansi_bright_black,
            Color::LightRed => self.ansi_bright_red,
            Color::LightGreen => self.ansi_bright_green,
            Color::LightYellow => self.ansi_bright_yellow,
            Color::LightBlue => self.ansi_bright_blue,
            Color::LightMagenta => self.ansi_bright_magenta,
            Color::LightCyan => self.ansi_bright_cyan,
            Color::Reset => self.foreground,
            _ => color,
        }
    }
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
    /// Header/breadcrumb hierarchical chevron delimiter.
    pub chevron: &'static str,
    /// Status prefix for errors / alerts.
    pub error_prefix: &'static str,
    /// Status prefix for warnings.
    pub warning_prefix: &'static str,
    /// Status prefix for success.
    pub success_prefix: &'static str,
    /// Status prefix for info.
    pub info_prefix: &'static str,
    /// Universal success check mark.
    pub check_mark: &'static str,
    /// Universal failure cross mark.
    pub cross_mark: &'static str,
    /// Universal warning alert mark.
    pub warning_mark: &'static str,
    /// Universal focused bullet mark.
    pub focus_bullet: &'static str,
    /// Universal unfocused bullet mark.
    pub unfocused_bullet: &'static str,
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
            selection_marker: "▸ ",
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
            chevron: "›",
            error_prefix: "✕ ",
            warning_prefix: "! ",
            success_prefix: "✓ ",
            info_prefix: "ℹ ",
            check_mark: "✓",
            cross_mark: "✕",
            warning_mark: "!",
            focus_bullet: "●",
            unfocused_bullet: "○",
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
            chevron: ">",
            error_prefix: "[x]",
            warning_prefix: "[!]",
            success_prefix: "[+]",
            info_prefix: "[i]",
            check_mark: "+",
            cross_mark: "x",
            warning_mark: "!",
            focus_bullet: "*",
            unfocused_bullet: " ",
        }
    }
}

/// Centralized UI theme containing all semantic styles and palette definitions.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Active theme identifier.
    pub id: ThemeId,
    /// Display name.
    pub name: &'static str,
    /// Theme description.
    pub description: &'static str,
    /// Underlying color palette.
    pub palette: ThemePalette,
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
        Self::terminalvision()
    }
}

impl Theme {
    /// Builds a cohesive `Theme` instance from a semantic `ThemePalette`.
    pub const fn from_palette(
        id: ThemeId,
        name: &'static str,
        description: &'static str,
        p: ThemePalette,
        symbols: Symbols,
    ) -> Self {
        Self {
            id,
            name,
            description,
            palette: p,
            symbols,

            // Header & Branding
            app_title: Style::new().fg(p.primary).add_modifier(Modifier::BOLD),
            header_path: Style::new().fg(p.accent).add_modifier(Modifier::BOLD),
            header_separator: Style::new().fg(p.border),
            project_info_clean: Style::new().fg(p.git_clean).add_modifier(Modifier::BOLD),
            project_info_dirty: Style::new().fg(p.git_modified).add_modifier(Modifier::BOLD),

            // Panes & Layout
            active_pane_border_type: BorderType::Double,
            active_pane_border: Style::new().fg(p.border_active),
            active_pane_title: Style::new().fg(p.focus).add_modifier(Modifier::BOLD),
            inactive_pane_border_type: BorderType::Plain,
            inactive_pane_border: Style::new().fg(p.border),
            inactive_pane_title: Style::new().fg(p.foreground_muted),

            // Tabs
            tab_active_focused: Style::new().fg(p.primary).add_modifier(Modifier::BOLD),
            tab_active_unfocused: Style::new().fg(p.foreground).add_modifier(Modifier::BOLD),
            tab_inactive: Style::new().fg(p.foreground_muted),
            tab_separator: Style::new().fg(p.border),

            // File Rows & Selections
            row_selected_active: Style::new()
                .bg(p.selection)
                .fg(p.selection_foreground)
                .add_modifier(Modifier::BOLD),
            row_selected_inactive: Style::new().bg(p.surface_alt).fg(p.foreground),
            row_multi_selected: Style::new().fg(p.warning).add_modifier(Modifier::BOLD),
            row_unselected: Style::new(),

            // Entry Kinds
            entry_directory: Style::new().fg(p.directory),
            entry_file: Style::new().fg(p.foreground),
            entry_symlink: Style::new().fg(p.symlink),
            entry_broken_symlink: Style::new().fg(p.error),
            entry_hidden: Style::new().fg(p.disabled),
            entry_executable: Style::new().fg(p.executable),
            entry_code: Style::new().fg(p.primary),
            entry_image: Style::new().fg(p.secondary),
            entry_archive: Style::new().fg(p.accent),
            entry_doc: Style::new().fg(p.info),
            entry_config: Style::new().fg(p.warning),
            entry_other: Style::new().fg(p.foreground_muted),

            // Git Statuses
            git_modified: Style::new().fg(p.git_modified),
            git_added: Style::new().fg(p.git_added),
            git_deleted: Style::new().fg(p.git_deleted),
            git_renamed: Style::new().fg(p.git_renamed),
            git_untracked: Style::new().fg(p.git_untracked),
            git_clean: Style::new().fg(p.git_clean),

            // File Preview
            preview_border: Style::new().fg(p.border),
            preview_title: Style::new().fg(p.foreground).add_modifier(Modifier::BOLD),
            preview_line_number: Style::new().fg(p.foreground_muted),
            preview_metadata_label: Style::new().fg(p.primary).add_modifier(Modifier::BOLD),
            preview_metadata_value: Style::new().fg(p.foreground),
            preview_muted: Style::new().fg(p.foreground_muted),

            // Footer & Status Bar
            footer_mode: Style::new().fg(p.primary).add_modifier(Modifier::BOLD),
            footer_pane: Style::new().fg(p.foreground).add_modifier(Modifier::BOLD),
            footer_selection: Style::new().fg(p.foreground),
            footer_clipboard: Style::new().fg(p.accent).add_modifier(Modifier::BOLD),
            footer_hint: Style::new().fg(p.foreground_muted),
            footer_separator: Style::new().fg(p.border),

            // Notifications & Alerts
            notify_info: Style::new().fg(p.info),
            notify_success: Style::new().fg(p.success).add_modifier(Modifier::BOLD),
            notify_warning: Style::new().fg(p.warning).add_modifier(Modifier::BOLD),
            notify_error_badge: Style::new()
                .fg(p.selection_foreground)
                .bg(p.error)
                .add_modifier(Modifier::BOLD),
            notify_error_text: Style::new().fg(p.error).add_modifier(Modifier::BOLD),

            // Dialogs & Modals
            dialog_border: Style::new().fg(p.border_active),
            dialog_border_confirm: Style::new().fg(p.warning).add_modifier(Modifier::BOLD),
            dialog_title: Style::new().fg(p.foreground).add_modifier(Modifier::BOLD),
            dialog_message: Style::new().fg(p.foreground),
            dialog_input: Style::new().fg(p.primary).add_modifier(Modifier::BOLD),
            dialog_button_active: Style::new()
                .fg(p.selection_foreground)
                .bg(p.primary)
                .add_modifier(Modifier::BOLD),
            dialog_button_inactive: Style::new().fg(p.foreground_muted),
            dialog_hint: Style::new().fg(p.foreground_muted),

            // Command Palette
            palette_input: Style::new().fg(p.primary).add_modifier(Modifier::BOLD),
            palette_selected: Style::new()
                .bg(p.selection)
                .fg(p.selection_foreground)
                .add_modifier(Modifier::BOLD),
            palette_unselected: Style::new().fg(p.foreground),
            palette_description: Style::new().fg(p.foreground_muted),
            palette_shortcut: Style::new().fg(p.accent).add_modifier(Modifier::BOLD),

            // Help Dialog
            help_category: Style::new().fg(p.primary).add_modifier(Modifier::BOLD),
            help_key: Style::new().fg(p.accent).add_modifier(Modifier::BOLD),
            help_desc: Style::new().fg(p.foreground),

            // Empty States
            empty_state_text: Style::new().fg(p.foreground_muted),

            // Integrated Terminal Panel
            active_terminal_border_type: BorderType::Thick,
            active_terminal_border: Style::new().fg(p.primary),
            inactive_terminal_border_type: BorderType::Plain,
            inactive_terminal_border: Style::new().fg(p.border),
            terminal_title_focused: Style::new().fg(p.primary).add_modifier(Modifier::BOLD),
            terminal_title_unfocused: Style::new()
                .fg(p.foreground_muted)
                .add_modifier(Modifier::BOLD),
            terminal_meta_focused: Style::new().fg(p.primary),
            terminal_meta_unfocused: Style::new().fg(p.foreground_muted),
        }
    }

    /// Resolves a theme definition by ThemeId.
    pub const fn for_id(id: ThemeId) -> Self {
        match id {
            ThemeId::TerminalVision => Self::terminalvision(),
            ThemeId::Midnight => Self::midnight(),
            ThemeId::Cyberpunk => Self::cyberpunk(),
            ThemeId::Ocean => Self::ocean(),
            ThemeId::Dracula => Self::dracula(),
            ThemeId::Nord => Self::nord(),
            ThemeId::Matrix => Self::matrix(),
            ThemeId::SolarizedDark => Self::solarized_dark(),
            ThemeId::Monochrome => Self::monochrome(),
            ThemeId::HighContrast => Self::high_contrast(),
        }
    }

    /// Alias for signature TerminalVision theme.
    pub const fn signature() -> Self {
        Self::terminalvision()
    }

    /// 1. TerminalVision signature theme.
    pub const fn terminalvision() -> Self {
        let palette = ThemePalette {
            background: Color::Rgb(15, 17, 23),
            surface: Color::Rgb(24, 27, 36),
            surface_alt: Color::Rgb(34, 38, 50),
            foreground: Color::Rgb(225, 230, 240),
            foreground_muted: Color::Rgb(115, 120, 140),
            primary: Color::Cyan,
            secondary: Color::Rgb(140, 120, 255),
            accent: Color::Yellow,
            selection: Color::Rgb(45, 55, 80),
            selection_foreground: Color::White,
            border: Color::DarkGray,
            border_active: Color::Cyan,
            focus: Color::Cyan,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            info: Color::Cyan,
            directory: Color::Blue,
            symlink: Color::Magenta,
            executable: Color::Green,
            git_clean: Color::Green,
            git_modified: Color::Yellow,
            git_added: Color::Green,
            git_deleted: Color::Red,
            git_renamed: Color::Magenta,
            git_untracked: Color::DarkGray,
            preview: Color::DarkGray,
            overlay: Color::Rgb(20, 24, 32),
            disabled: Color::DarkGray,
            ansi_black: Color::Black,
            ansi_red: Color::Red,
            ansi_green: Color::Green,
            ansi_yellow: Color::Yellow,
            ansi_blue: Color::Blue,
            ansi_magenta: Color::Magenta,
            ansi_cyan: Color::Cyan,
            ansi_white: Color::White,
            ansi_bright_black: Color::DarkGray,
            ansi_bright_red: Color::LightRed,
            ansi_bright_green: Color::LightGreen,
            ansi_bright_yellow: Color::LightYellow,
            ansi_bright_blue: Color::LightBlue,
            ansi_bright_magenta: Color::LightMagenta,
            ansi_bright_cyan: Color::LightCyan,
            ansi_bright_white: Color::White,
            cursor: Color::Cyan,
        };
        Self::from_palette(
            ThemeId::TerminalVision,
            ThemeId::TerminalVision.name(),
            ThemeId::TerminalVision.description(),
            palette,
            Symbols::standard(),
        )
    }

    /// 2. Midnight theme (obsidian darkness with restrained cool tones).
    pub const fn midnight() -> Self {
        let palette = ThemePalette {
            background: Color::Rgb(10, 12, 16),
            surface: Color::Rgb(18, 20, 26),
            surface_alt: Color::Rgb(28, 32, 42),
            foreground: Color::Rgb(210, 215, 225),
            foreground_muted: Color::Rgb(95, 100, 115),
            primary: Color::Rgb(100, 150, 220),
            secondary: Color::Rgb(130, 120, 190),
            accent: Color::Rgb(120, 190, 210),
            selection: Color::Rgb(35, 45, 65),
            selection_foreground: Color::Rgb(240, 245, 255),
            border: Color::Rgb(45, 52, 65),
            border_active: Color::Rgb(100, 150, 220),
            focus: Color::Rgb(100, 150, 220),
            success: Color::Rgb(80, 180, 120),
            warning: Color::Rgb(215, 165, 75),
            error: Color::Rgb(225, 85, 85),
            info: Color::Rgb(100, 150, 220),
            directory: Color::Rgb(110, 165, 235),
            symlink: Color::Rgb(175, 130, 220),
            executable: Color::Rgb(90, 190, 130),
            git_clean: Color::Rgb(80, 180, 120),
            git_modified: Color::Rgb(215, 165, 75),
            git_added: Color::Rgb(80, 180, 120),
            git_deleted: Color::Rgb(225, 85, 85),
            git_renamed: Color::Rgb(175, 130, 220),
            git_untracked: Color::Rgb(95, 100, 115),
            preview: Color::Rgb(45, 52, 65),
            overlay: Color::Rgb(15, 18, 24),
            disabled: Color::Rgb(70, 75, 90),
            ansi_black: Color::Rgb(10, 12, 16),
            ansi_red: Color::Rgb(225, 85, 85),
            ansi_green: Color::Rgb(80, 180, 120),
            ansi_yellow: Color::Rgb(215, 165, 75),
            ansi_blue: Color::Rgb(100, 150, 220),
            ansi_magenta: Color::Rgb(175, 130, 220),
            ansi_cyan: Color::Rgb(120, 190, 210),
            ansi_white: Color::Rgb(210, 215, 225),
            ansi_bright_black: Color::Rgb(70, 75, 90),
            ansi_bright_red: Color::Rgb(240, 105, 105),
            ansi_bright_green: Color::Rgb(105, 205, 145),
            ansi_bright_yellow: Color::Rgb(235, 185, 95),
            ansi_bright_blue: Color::Rgb(125, 175, 245),
            ansi_bright_magenta: Color::Rgb(195, 150, 240),
            ansi_bright_cyan: Color::Rgb(145, 215, 235),
            ansi_bright_white: Color::Rgb(240, 245, 255),
            cursor: Color::Rgb(100, 150, 220),
        };
        Self::from_palette(
            ThemeId::Midnight,
            ThemeId::Midnight.name(),
            ThemeId::Midnight.description(),
            palette,
            Symbols::standard(),
        )
    }

    /// 3. Cyberpunk theme (neon yellow and hot magenta).
    pub const fn cyberpunk() -> Self {
        let palette = ThemePalette {
            background: Color::Rgb(13, 10, 25),
            surface: Color::Rgb(26, 18, 45),
            surface_alt: Color::Rgb(42, 28, 70),
            foreground: Color::Rgb(245, 245, 255),
            foreground_muted: Color::Rgb(135, 115, 165),
            primary: Color::Rgb(255, 230, 0),
            secondary: Color::Rgb(255, 0, 128),
            accent: Color::Rgb(0, 245, 255),
            selection: Color::Rgb(75, 20, 95),
            selection_foreground: Color::Rgb(255, 255, 255),
            border: Color::Rgb(80, 50, 120),
            border_active: Color::Rgb(255, 0, 128),
            focus: Color::Rgb(255, 230, 0),
            success: Color::Rgb(0, 255, 150),
            warning: Color::Rgb(255, 200, 0),
            error: Color::Rgb(255, 50, 80),
            info: Color::Rgb(0, 230, 255),
            directory: Color::Rgb(0, 240, 255),
            symlink: Color::Rgb(255, 0, 128),
            executable: Color::Rgb(0, 255, 150),
            git_clean: Color::Rgb(0, 255, 150),
            git_modified: Color::Rgb(255, 230, 0),
            git_added: Color::Rgb(0, 255, 150),
            git_deleted: Color::Rgb(255, 50, 80),
            git_renamed: Color::Rgb(255, 0, 128),
            git_untracked: Color::Rgb(135, 115, 165),
            preview: Color::Rgb(80, 50, 120),
            overlay: Color::Rgb(20, 15, 38),
            disabled: Color::Rgb(95, 75, 130),
            ansi_black: Color::Rgb(13, 10, 25),
            ansi_red: Color::Rgb(255, 50, 80),
            ansi_green: Color::Rgb(0, 255, 150),
            ansi_yellow: Color::Rgb(255, 230, 0),
            ansi_blue: Color::Rgb(0, 180, 255),
            ansi_magenta: Color::Rgb(255, 0, 128),
            ansi_cyan: Color::Rgb(0, 245, 255),
            ansi_white: Color::Rgb(245, 245, 255),
            ansi_bright_black: Color::Rgb(95, 75, 130),
            ansi_bright_red: Color::Rgb(255, 80, 110),
            ansi_bright_green: Color::Rgb(50, 255, 180),
            ansi_bright_yellow: Color::Rgb(255, 240, 50),
            ansi_bright_blue: Color::Rgb(50, 200, 255),
            ansi_bright_magenta: Color::Rgb(255, 50, 160),
            ansi_bright_cyan: Color::Rgb(80, 255, 255),
            ansi_bright_white: Color::White,
            cursor: Color::Rgb(255, 230, 0),
        };
        Self::from_palette(
            ThemeId::Cyberpunk,
            ThemeId::Cyberpunk.name(),
            ThemeId::Cyberpunk.description(),
            palette,
            Symbols::standard(),
        )
    }

    /// 4. Ocean theme (deep blues, teal & coral).
    pub const fn ocean() -> Self {
        let palette = ThemePalette {
            background: Color::Rgb(12, 22, 34),
            surface: Color::Rgb(18, 34, 52),
            surface_alt: Color::Rgb(28, 50, 75),
            foreground: Color::Rgb(220, 235, 245),
            foreground_muted: Color::Rgb(100, 130, 150),
            primary: Color::Rgb(65, 190, 235),
            secondary: Color::Rgb(75, 215, 185),
            accent: Color::Rgb(255, 165, 95),
            selection: Color::Rgb(30, 65, 95),
            selection_foreground: Color::White,
            border: Color::Rgb(45, 75, 100),
            border_active: Color::Rgb(75, 215, 185),
            focus: Color::Rgb(65, 190, 235),
            success: Color::Rgb(85, 215, 155),
            warning: Color::Rgb(240, 195, 85),
            error: Color::Rgb(240, 95, 95),
            info: Color::Rgb(75, 195, 245),
            directory: Color::Rgb(95, 205, 250),
            symlink: Color::Rgb(180, 140, 230),
            executable: Color::Rgb(90, 220, 160),
            git_clean: Color::Rgb(85, 215, 155),
            git_modified: Color::Rgb(240, 195, 85),
            git_added: Color::Rgb(85, 215, 155),
            git_deleted: Color::Rgb(240, 95, 95),
            git_renamed: Color::Rgb(180, 140, 230),
            git_untracked: Color::Rgb(100, 130, 150),
            preview: Color::Rgb(45, 75, 100),
            overlay: Color::Rgb(16, 28, 44),
            disabled: Color::Rgb(70, 95, 115),
            ansi_black: Color::Rgb(12, 22, 34),
            ansi_red: Color::Rgb(240, 95, 95),
            ansi_green: Color::Rgb(85, 215, 155),
            ansi_yellow: Color::Rgb(240, 195, 85),
            ansi_blue: Color::Rgb(65, 190, 235),
            ansi_magenta: Color::Rgb(180, 140, 230),
            ansi_cyan: Color::Rgb(75, 215, 185),
            ansi_white: Color::Rgb(220, 235, 245),
            ansi_bright_black: Color::Rgb(70, 95, 115),
            ansi_bright_red: Color::Rgb(255, 115, 115),
            ansi_bright_green: Color::Rgb(110, 235, 175),
            ansi_bright_yellow: Color::Rgb(255, 210, 105),
            ansi_bright_blue: Color::Rgb(95, 210, 255),
            ansi_bright_magenta: Color::Rgb(200, 160, 250),
            ansi_bright_cyan: Color::Rgb(105, 235, 205),
            ansi_bright_white: Color::White,
            cursor: Color::Rgb(65, 190, 235),
        };
        Self::from_palette(
            ThemeId::Ocean,
            ThemeId::Ocean.name(),
            ThemeId::Ocean.description(),
            palette,
            Symbols::standard(),
        )
    }

    /// 5. Dracula theme (gothic purple, pink & cyan).
    pub const fn dracula() -> Self {
        let palette = ThemePalette {
            background: Color::Rgb(40, 42, 54),
            surface: Color::Rgb(50, 52, 68),
            surface_alt: Color::Rgb(68, 71, 90),
            foreground: Color::Rgb(248, 248, 242),
            foreground_muted: Color::Rgb(140, 145, 165),
            primary: Color::Rgb(189, 147, 249),
            secondary: Color::Rgb(255, 121, 198),
            accent: Color::Rgb(139, 233, 253),
            selection: Color::Rgb(68, 71, 90),
            selection_foreground: Color::Rgb(255, 255, 255),
            border: Color::Rgb(75, 78, 98),
            border_active: Color::Rgb(189, 147, 249),
            focus: Color::Rgb(255, 121, 198),
            success: Color::Rgb(80, 250, 123),
            warning: Color::Rgb(241, 250, 140),
            error: Color::Rgb(255, 85, 85),
            info: Color::Rgb(139, 233, 253),
            directory: Color::Rgb(139, 233, 253),
            symlink: Color::Rgb(255, 121, 198),
            executable: Color::Rgb(80, 250, 123),
            git_clean: Color::Rgb(80, 250, 123),
            git_modified: Color::Rgb(241, 250, 140),
            git_added: Color::Rgb(80, 250, 123),
            git_deleted: Color::Rgb(255, 85, 85),
            git_renamed: Color::Rgb(255, 121, 198),
            git_untracked: Color::Rgb(140, 145, 165),
            preview: Color::Rgb(75, 78, 98),
            overlay: Color::Rgb(33, 34, 44),
            disabled: Color::Rgb(98, 114, 164),
            ansi_black: Color::Rgb(40, 42, 54),
            ansi_red: Color::Rgb(255, 85, 85),
            ansi_green: Color::Rgb(80, 250, 123),
            ansi_yellow: Color::Rgb(241, 250, 140),
            ansi_blue: Color::Rgb(189, 147, 249),
            ansi_magenta: Color::Rgb(255, 121, 198),
            ansi_cyan: Color::Rgb(139, 233, 253),
            ansi_white: Color::Rgb(248, 248, 242),
            ansi_bright_black: Color::Rgb(98, 114, 164),
            ansi_bright_red: Color::Rgb(255, 110, 110),
            ansi_bright_green: Color::Rgb(110, 255, 145),
            ansi_bright_yellow: Color::Rgb(255, 255, 165),
            ansi_bright_blue: Color::Rgb(210, 175, 255),
            ansi_bright_magenta: Color::Rgb(255, 150, 215),
            ansi_bright_cyan: Color::Rgb(165, 240, 255),
            ansi_bright_white: Color::White,
            cursor: Color::Rgb(255, 121, 198),
        };
        Self::from_palette(
            ThemeId::Dracula,
            ThemeId::Dracula.name(),
            ThemeId::Dracula.description(),
            palette,
            Symbols::standard(),
        )
    }

    /// 6. Nord theme (arctic cool palette).
    pub const fn nord() -> Self {
        let palette = ThemePalette {
            background: Color::Rgb(46, 52, 64),
            surface: Color::Rgb(59, 66, 82),
            surface_alt: Color::Rgb(67, 76, 94),
            foreground: Color::Rgb(236, 239, 244),
            foreground_muted: Color::Rgb(140, 150, 168),
            primary: Color::Rgb(136, 192, 208),
            secondary: Color::Rgb(129, 161, 193),
            accent: Color::Rgb(235, 203, 139),
            selection: Color::Rgb(76, 86, 106),
            selection_foreground: Color::Rgb(245, 248, 252),
            border: Color::Rgb(76, 86, 106),
            border_active: Color::Rgb(136, 192, 208),
            focus: Color::Rgb(136, 192, 208),
            success: Color::Rgb(163, 190, 140),
            warning: Color::Rgb(235, 203, 139),
            error: Color::Rgb(191, 97, 106),
            info: Color::Rgb(129, 161, 193),
            directory: Color::Rgb(143, 188, 187),
            symlink: Color::Rgb(180, 142, 173),
            executable: Color::Rgb(163, 190, 140),
            git_clean: Color::Rgb(163, 190, 140),
            git_modified: Color::Rgb(235, 203, 139),
            git_added: Color::Rgb(163, 190, 140),
            git_deleted: Color::Rgb(191, 97, 106),
            git_renamed: Color::Rgb(180, 142, 173),
            git_untracked: Color::Rgb(140, 150, 168),
            preview: Color::Rgb(76, 86, 106),
            overlay: Color::Rgb(40, 45, 56),
            disabled: Color::Rgb(90, 100, 120),
            ansi_black: Color::Rgb(46, 52, 64),
            ansi_red: Color::Rgb(191, 97, 106),
            ansi_green: Color::Rgb(163, 190, 140),
            ansi_yellow: Color::Rgb(235, 203, 139),
            ansi_blue: Color::Rgb(129, 161, 193),
            ansi_magenta: Color::Rgb(180, 142, 173),
            ansi_cyan: Color::Rgb(136, 192, 208),
            ansi_white: Color::Rgb(236, 239, 244),
            ansi_bright_black: Color::Rgb(76, 86, 106),
            ansi_bright_red: Color::Rgb(210, 115, 125),
            ansi_bright_green: Color::Rgb(180, 210, 160),
            ansi_bright_yellow: Color::Rgb(245, 215, 155),
            ansi_bright_blue: Color::Rgb(145, 180, 215),
            ansi_bright_magenta: Color::Rgb(195, 160, 190),
            ansi_bright_cyan: Color::Rgb(155, 210, 225),
            ansi_bright_white: Color::White,
            cursor: Color::Rgb(136, 192, 208),
        };
        Self::from_palette(
            ThemeId::Nord,
            ThemeId::Nord.name(),
            ThemeId::Nord.description(),
            palette,
            Symbols::standard(),
        )
    }

    /// 7. Matrix theme (phosphor green hacker terminal).
    pub const fn matrix() -> Self {
        let palette = ThemePalette {
            background: Color::Rgb(5, 10, 5),
            surface: Color::Rgb(10, 24, 10),
            surface_alt: Color::Rgb(18, 38, 18),
            foreground: Color::Rgb(160, 255, 160),
            foreground_muted: Color::Rgb(65, 145, 65),
            primary: Color::Rgb(0, 255, 65),
            secondary: Color::Rgb(90, 225, 110),
            accent: Color::Rgb(210, 255, 110),
            selection: Color::Rgb(20, 65, 25),
            selection_foreground: Color::Rgb(235, 255, 235),
            border: Color::Rgb(35, 85, 40),
            border_active: Color::Rgb(0, 255, 65),
            focus: Color::Rgb(0, 255, 65),
            success: Color::Rgb(0, 255, 100),
            warning: Color::Rgb(225, 245, 85),
            error: Color::Rgb(255, 95, 95),
            info: Color::Rgb(130, 255, 190),
            directory: Color::Rgb(110, 255, 130),
            symlink: Color::Rgb(180, 255, 120),
            executable: Color::Rgb(0, 255, 65),
            git_clean: Color::Rgb(0, 255, 65),
            git_modified: Color::Rgb(225, 245, 85),
            git_added: Color::Rgb(0, 255, 65),
            git_deleted: Color::Rgb(255, 95, 95),
            git_renamed: Color::Rgb(180, 255, 120),
            git_untracked: Color::Rgb(65, 145, 65),
            preview: Color::Rgb(35, 85, 40),
            overlay: Color::Rgb(8, 18, 8),
            disabled: Color::Rgb(45, 100, 50),
            ansi_black: Color::Rgb(5, 10, 5),
            ansi_red: Color::Rgb(255, 95, 95),
            ansi_green: Color::Rgb(0, 255, 65),
            ansi_yellow: Color::Rgb(225, 245, 85),
            ansi_blue: Color::Rgb(60, 200, 100),
            ansi_magenta: Color::Rgb(180, 255, 120),
            ansi_cyan: Color::Rgb(130, 255, 190),
            ansi_white: Color::Rgb(160, 255, 160),
            ansi_bright_black: Color::Rgb(45, 100, 50),
            ansi_bright_red: Color::Rgb(255, 120, 120),
            ansi_bright_green: Color::Rgb(50, 255, 100),
            ansi_bright_yellow: Color::Rgb(240, 255, 120),
            ansi_bright_blue: Color::Rgb(90, 220, 130),
            ansi_bright_magenta: Color::Rgb(200, 255, 150),
            ansi_bright_cyan: Color::Rgb(160, 255, 210),
            ansi_bright_white: Color::Rgb(220, 255, 220),
            cursor: Color::Rgb(0, 255, 65),
        };
        Self::from_palette(
            ThemeId::Matrix,
            ThemeId::Matrix.name(),
            ThemeId::Matrix.description(),
            palette,
            Symbols::standard(),
        )
    }

    /// 8. Solarized Dark theme.
    pub const fn solarized_dark() -> Self {
        let palette = ThemePalette {
            background: Color::Rgb(0, 43, 54),
            surface: Color::Rgb(7, 54, 66),
            surface_alt: Color::Rgb(14, 70, 85),
            foreground: Color::Rgb(147, 161, 161),
            foreground_muted: Color::Rgb(88, 110, 117),
            primary: Color::Rgb(38, 139, 210),
            secondary: Color::Rgb(42, 161, 152),
            accent: Color::Rgb(181, 137, 0),
            selection: Color::Rgb(18, 75, 92),
            selection_foreground: Color::Rgb(253, 246, 227),
            border: Color::Rgb(32, 88, 102),
            border_active: Color::Rgb(38, 139, 210),
            focus: Color::Rgb(42, 161, 152),
            success: Color::Rgb(133, 153, 0),
            warning: Color::Rgb(181, 137, 0),
            error: Color::Rgb(220, 50, 47),
            info: Color::Rgb(38, 139, 210),
            directory: Color::Rgb(38, 139, 210),
            symlink: Color::Rgb(211, 54, 130),
            executable: Color::Rgb(133, 153, 0),
            git_clean: Color::Rgb(133, 153, 0),
            git_modified: Color::Rgb(181, 137, 0),
            git_added: Color::Rgb(133, 153, 0),
            git_deleted: Color::Rgb(220, 50, 47),
            git_renamed: Color::Rgb(211, 54, 130),
            git_untracked: Color::Rgb(88, 110, 117),
            preview: Color::Rgb(32, 88, 102),
            overlay: Color::Rgb(4, 34, 44),
            disabled: Color::Rgb(70, 90, 96),
            ansi_black: Color::Rgb(7, 54, 66),
            ansi_red: Color::Rgb(220, 50, 47),
            ansi_green: Color::Rgb(133, 153, 0),
            ansi_yellow: Color::Rgb(181, 137, 0),
            ansi_blue: Color::Rgb(38, 139, 210),
            ansi_magenta: Color::Rgb(211, 54, 130),
            ansi_cyan: Color::Rgb(42, 161, 152),
            ansi_white: Color::Rgb(238, 232, 213),
            ansi_bright_black: Color::Rgb(0, 43, 54),
            ansi_bright_red: Color::Rgb(203, 75, 22),
            ansi_bright_green: Color::Rgb(88, 110, 117),
            ansi_bright_yellow: Color::Rgb(101, 123, 131),
            ansi_bright_blue: Color::Rgb(131, 148, 150),
            ansi_bright_magenta: Color::Rgb(108, 113, 196),
            ansi_bright_cyan: Color::Rgb(147, 161, 161),
            ansi_bright_white: Color::Rgb(253, 246, 227),
            cursor: Color::Rgb(42, 161, 152),
        };
        Self::from_palette(
            ThemeId::SolarizedDark,
            ThemeId::SolarizedDark.name(),
            ThemeId::SolarizedDark.description(),
            palette,
            Symbols::standard(),
        )
    }

    /// 9. Monochrome theme (pure grayscale).
    pub const fn monochrome() -> Self {
        let palette = ThemePalette {
            background: Color::Rgb(16, 16, 16),
            surface: Color::Rgb(28, 28, 28),
            surface_alt: Color::Rgb(42, 42, 42),
            foreground: Color::Rgb(235, 235, 235),
            foreground_muted: Color::Rgb(125, 125, 125),
            primary: Color::Rgb(255, 255, 255),
            secondary: Color::Rgb(185, 185, 185),
            accent: Color::Rgb(215, 215, 215),
            selection: Color::Rgb(58, 58, 58),
            selection_foreground: Color::Rgb(255, 255, 255),
            border: Color::Rgb(68, 68, 68),
            border_active: Color::Rgb(225, 225, 225),
            focus: Color::Rgb(255, 255, 255),
            success: Color::Rgb(225, 225, 225),
            warning: Color::Rgb(195, 195, 195),
            error: Color::Rgb(245, 245, 245),
            info: Color::Rgb(205, 205, 205),
            directory: Color::Rgb(245, 245, 245),
            symlink: Color::Rgb(185, 185, 185),
            executable: Color::Rgb(255, 255, 255),
            git_clean: Color::Rgb(225, 225, 225),
            git_modified: Color::Rgb(195, 195, 195),
            git_added: Color::Rgb(255, 255, 255),
            git_deleted: Color::Rgb(145, 145, 145),
            git_renamed: Color::Rgb(185, 185, 185),
            git_untracked: Color::Rgb(105, 105, 105),
            preview: Color::Rgb(68, 68, 68),
            overlay: Color::Rgb(22, 22, 22),
            disabled: Color::Rgb(85, 85, 85),
            ansi_black: Color::Rgb(16, 16, 16),
            ansi_red: Color::Rgb(200, 200, 200),
            ansi_green: Color::Rgb(230, 230, 230),
            ansi_yellow: Color::Rgb(190, 190, 190),
            ansi_blue: Color::Rgb(220, 220, 220),
            ansi_magenta: Color::Rgb(180, 180, 180),
            ansi_cyan: Color::Rgb(210, 210, 210),
            ansi_white: Color::Rgb(235, 235, 235),
            ansi_bright_black: Color::Rgb(85, 85, 85),
            ansi_bright_red: Color::Rgb(215, 215, 215),
            ansi_bright_green: Color::Rgb(240, 240, 240),
            ansi_bright_yellow: Color::Rgb(205, 205, 205),
            ansi_bright_blue: Color::Rgb(235, 235, 235),
            ansi_bright_magenta: Color::Rgb(195, 195, 195),
            ansi_bright_cyan: Color::Rgb(225, 225, 225),
            ansi_bright_white: Color::White,
            cursor: Color::White,
        };
        Self::from_palette(
            ThemeId::Monochrome,
            ThemeId::Monochrome.name(),
            ThemeId::Monochrome.description(),
            palette,
            Symbols::standard(),
        )
    }

    /// 10. High Contrast theme (maximum distinction and WCAG compliance).
    pub const fn high_contrast() -> Self {
        let palette = ThemePalette {
            background: Color::Black,
            surface: Color::Rgb(20, 20, 20),
            surface_alt: Color::Rgb(40, 40, 40),
            foreground: Color::White,
            foreground_muted: Color::Gray,
            primary: Color::Yellow,
            secondary: Color::Cyan,
            accent: Color::White,
            selection: Color::Rgb(65, 65, 65),
            selection_foreground: Color::Yellow,
            border: Color::White,
            border_active: Color::Yellow,
            focus: Color::Yellow,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            info: Color::Cyan,
            directory: Color::Cyan,
            symlink: Color::Magenta,
            executable: Color::Green,
            git_clean: Color::Green,
            git_modified: Color::Yellow,
            git_added: Color::Green,
            git_deleted: Color::Red,
            git_renamed: Color::Magenta,
            git_untracked: Color::Gray,
            preview: Color::White,
            overlay: Color::Black,
            disabled: Color::DarkGray,
            ansi_black: Color::Black,
            ansi_red: Color::Red,
            ansi_green: Color::Green,
            ansi_yellow: Color::Yellow,
            ansi_blue: Color::Blue,
            ansi_magenta: Color::Magenta,
            ansi_cyan: Color::Cyan,
            ansi_white: Color::White,
            ansi_bright_black: Color::DarkGray,
            ansi_bright_red: Color::LightRed,
            ansi_bright_green: Color::LightGreen,
            ansi_bright_yellow: Color::LightYellow,
            ansi_bright_blue: Color::LightBlue,
            ansi_bright_magenta: Color::LightMagenta,
            ansi_bright_cyan: Color::LightCyan,
            ansi_bright_white: Color::White,
            cursor: Color::Yellow,
        };
        Self::from_palette(
            ThemeId::HighContrast,
            ThemeId::HighContrast.name(),
            ThemeId::HighContrast.description(),
            palette,
            Symbols::standard(),
        )
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
            (true, true, true) => {
                if self.symbols.selection_marker.starts_with('▸') {
                    ("▸✓", self.row_selected_active)
                } else {
                    (">✓", self.row_selected_active)
                }
            }
            (true, true, false) => {
                if self.symbols.selection_marker.starts_with('▸') {
                    ("▸✓", self.row_selected_inactive)
                } else {
                    (">✓", self.row_selected_inactive)
                }
            }
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

    /// Maps standard ANSI color to active theme's synchronized palette.
    pub fn map_terminal_color(&self, color: Color) -> Color {
        self.palette.map_ansi(color)
    }

    /// Converts an RGB color to standard 16-color ANSI fallback if terminal capabilities are limited.
    pub fn to_16_color(color: Color) -> Color {
        match color {
            Color::Rgb(r, g, b) => {
                if r < 40 && g < 40 && b < 40 {
                    Color::Black
                } else if r > 200 && g > 200 && b > 200 {
                    Color::White
                } else if r > g && r > b {
                    if r > 180 { Color::LightRed } else { Color::Red }
                } else if g > r && g > b {
                    if g > 180 {
                        Color::LightGreen
                    } else {
                        Color::Green
                    }
                } else if b > r && b > g {
                    if b > 180 {
                        Color::LightBlue
                    } else {
                        Color::Blue
                    }
                } else if r > 150 && g > 150 && b < 100 {
                    Color::Yellow
                } else if r > 150 && b > 150 && g < 100 {
                    Color::Magenta
                } else if g > 150 && b > 150 && r < 100 {
                    Color::Cyan
                } else {
                    Color::Gray
                }
            }
            c => c,
        }
    }

    /// Primary brand style.
    pub fn primary(&self) -> Style {
        self.app_title
    }

    /// Secondary contextual style.
    pub fn secondary(&self) -> Style {
        self.footer_mode
    }

    /// Selection highlight style.
    pub fn selection(&self) -> Style {
        self.row_selected_active
    }

    /// Operation/status success style.
    pub fn success(&self) -> Style {
        self.notify_success
    }

    /// Warning/alert style.
    pub fn warning(&self) -> Style {
        self.notify_warning
    }

    /// Error/failure style.
    pub fn error(&self) -> Style {
        self.notify_error_text
    }

    /// Subtle muted secondary text style.
    pub fn muted(&self) -> Style {
        self.footer_hint
    }

    /// Inactive structural border style.
    pub fn border(&self) -> Style {
        self.inactive_pane_border
    }

    /// Active surface focus style.
    pub fn focus(&self) -> Style {
        self.active_pane_border
    }

    /// Inactive surface focus style.
    pub fn focus_inactive(&self) -> Style {
        self.inactive_pane_border
    }
}

/// Central registry for resolving and iterating over all TerminalVision themes.
pub struct ThemeRegistry;

impl ThemeRegistry {
    /// Returns the theme corresponding to the given `ThemeId`.
    pub fn get(id: ThemeId) -> Theme {
        Theme::for_id(id)
    }

    /// Resolves a theme from a string identifier, falling back safely to `TerminalVision`.
    pub fn get_by_str(id_str: &str) -> Theme {
        let id = Self::resolve_id(id_str);
        Self::get(id)
    }

    /// Resolves a `ThemeId` from a string, falling back safely to `ThemeId::TerminalVision`.
    pub fn resolve_id(id_str: &str) -> ThemeId {
        ThemeId::from_id(id_str).unwrap_or(ThemeId::TerminalVision)
    }

    /// Returns a slice of all built-in `ThemeId`s in display order.
    pub const fn all_ids() -> &'static [ThemeId; 10] {
        ThemeId::all()
    }

    /// Returns a `Vec` of all built-in `Theme` instances.
    pub fn all_themes() -> Vec<Theme> {
        ThemeId::all().iter().copied().map(Theme::for_id).collect()
    }

    /// Cycles to the next theme.
    pub fn next(current: ThemeId) -> ThemeId {
        let all = ThemeId::all();
        let idx = all.iter().position(|&id| id == current).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    /// Cycles to the previous theme.
    pub fn prev(current: ThemeId) -> ThemeId {
        let all = ThemeId::all();
        let idx = all.iter().position(|&id| id == current).unwrap_or(0);
        if idx == 0 {
            all[all.len() - 1]
        } else {
            all[idx - 1]
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
        assert_eq!(active_sel.bg, Some(theme.palette.selection));

        let inactive_sel = theme.row_style(true, false);
        assert!(!inactive_sel.add_modifier.contains(Modifier::BOLD));
        assert_eq!(inactive_sel.bg, Some(theme.palette.surface_alt));
    }

    #[test]
    fn all_ten_themes_are_valid_and_distinct() {
        let ids = ThemeRegistry::all_ids();
        assert_eq!(ids.len(), 10);

        for &id in ids {
            let theme = ThemeRegistry::get(id);
            assert_eq!(theme.id, id);
            assert!(!theme.name.is_empty());
            assert!(!theme.description.is_empty());
            assert_ne!(theme.palette.background, theme.palette.foreground);
            assert_ne!(theme.palette.border, theme.palette.border_active);
            assert_ne!(theme.palette.selection, theme.palette.background);
        }
    }

    #[test]
    fn theme_registry_lookup_and_fallback() {
        assert_eq!(ThemeRegistry::resolve_id("cyberpunk"), ThemeId::Cyberpunk);
        assert_eq!(ThemeRegistry::resolve_id("dracula"), ThemeId::Dracula);
        assert_eq!(ThemeRegistry::resolve_id("nord"), ThemeId::Nord);
        assert_eq!(ThemeRegistry::resolve_id("matrix"), ThemeId::Matrix);
        assert_eq!(ThemeRegistry::resolve_id("ocean"), ThemeId::Ocean);
        assert_eq!(ThemeRegistry::resolve_id("midnight"), ThemeId::Midnight);
        assert_eq!(
            ThemeRegistry::resolve_id("solarized-dark"),
            ThemeId::SolarizedDark
        );
        assert_eq!(ThemeRegistry::resolve_id("monochrome"), ThemeId::Monochrome);
        assert_eq!(
            ThemeRegistry::resolve_id("high-contrast"),
            ThemeId::HighContrast
        );

        // Safe fallback on unknown strings
        assert_eq!(
            ThemeRegistry::resolve_id("unknown_invalid_theme"),
            ThemeId::TerminalVision
        );
        assert_eq!(
            ThemeRegistry::get_by_str("nonexistent").id,
            ThemeId::TerminalVision
        );
    }

    #[test]
    fn theme_cycling() {
        let start = ThemeId::TerminalVision;
        let next = ThemeRegistry::next(start);
        assert_eq!(next, ThemeId::Midnight);
        let prev = ThemeRegistry::prev(start);
        assert_eq!(prev, ThemeId::HighContrast);
        assert_eq!(ThemeRegistry::next(prev), start);
    }

    #[test]
    fn ansi_mapping_synchronizes_palette() {
        let dracula = Theme::dracula();
        assert_eq!(
            dracula.map_terminal_color(Color::Red),
            dracula.palette.ansi_red
        );
        assert_eq!(
            dracula.map_terminal_color(Color::Green),
            dracula.palette.ansi_green
        );
    }

    #[test]
    fn limited_color_conversion_fallback() {
        assert_eq!(Theme::to_16_color(Color::Rgb(255, 0, 0)), Color::LightRed);
        assert_eq!(Theme::to_16_color(Color::Rgb(0, 255, 0)), Color::LightGreen);
        assert_eq!(Theme::to_16_color(Color::Rgb(0, 0, 255)), Color::LightBlue);
        assert_eq!(Theme::to_16_color(Color::Rgb(0, 0, 0)), Color::Black);
        assert_eq!(Theme::to_16_color(Color::Rgb(255, 255, 255)), Color::White);
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

        assert_eq!(std_sym.selection_marker, "▸ ");
        assert_eq!(asc_sym.selection_marker, "> ");
        assert_eq!(std_sym.active_indicator, "●");
        assert_eq!(asc_sym.active_indicator, "*");
        assert_eq!(std_sym.chevron, "›");
        assert_eq!(asc_sym.chevron, ">");
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
    fn semantic_helpers_return_styles() {
        let theme = Theme::signature();
        assert_eq!(theme.primary(), theme.app_title);
        assert_eq!(theme.secondary(), theme.footer_mode);
        assert_eq!(theme.selection(), theme.row_selected_active);
        assert_eq!(theme.success(), theme.notify_success);
        assert_eq!(theme.warning(), theme.notify_warning);
        assert_eq!(theme.error(), theme.notify_error_text);
        assert_eq!(theme.muted(), theme.footer_hint);
        assert_eq!(theme.border(), theme.inactive_pane_border);
        assert_eq!(theme.focus(), theme.active_pane_border);
        assert_eq!(theme.focus_inactive(), theme.inactive_pane_border);
    }
}
