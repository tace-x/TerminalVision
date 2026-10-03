//! Centralized Shortcut Registry and cross-platform key mapping.
//!
//! Provides a single source of truth for all keyboard shortcuts, platform-specific
//! modifier translations (Command on macOS vs Control on Windows/Linux), context-aware
//! shortcut resolution, conflict detection, and human-readable shortcut formatting.

use std::sync::LazyLock;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::actions::{Action, ActionCategory, ActionContext, ActionRegistry};
use crate::input::platform::Platform;

/// Modifiers associated with a key chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    /// Platform primary modifier: Command (⌘) on macOS, Control (Ctrl) on Windows/Linux.
    pub primary: bool,
    /// Explicit Control modifier (⌃ on macOS, Ctrl on Windows/Linux).
    pub ctrl: bool,
    /// Alt / Option modifier (⌥ on macOS, Alt on Windows/Linux).
    pub alt: bool,
    /// Shift modifier (⇧ on macOS, Shift on Windows/Linux).
    pub shift: bool,
    /// Explicit Super / Command modifier.
    pub super_key: bool,
}

impl Modifiers {
    /// No modifiers.
    pub const NONE: Self = Self {
        primary: false,
        ctrl: false,
        alt: false,
        shift: false,
        super_key: false,
    };

    /// Platform primary modifier only (⌘ on macOS, Ctrl on Windows/Linux).
    pub const PRIMARY: Self = Self {
        primary: true,
        ctrl: false,
        alt: false,
        shift: false,
        super_key: false,
    };

    /// Primary modifier + Shift (⌘⇧ on macOS, Ctrl+Shift on Windows/Linux).
    pub const PRIMARY_SHIFT: Self = Self {
        primary: true,
        ctrl: false,
        alt: false,
        shift: true,
        super_key: false,
    };

    /// Alt / Option modifier only.
    pub const ALT: Self = Self {
        primary: false,
        ctrl: false,
        alt: true,
        shift: false,
        super_key: false,
    };

    /// Control modifier only.
    pub const CTRL: Self = Self {
        primary: false,
        ctrl: true,
        alt: false,
        shift: false,
        super_key: false,
    };

    /// Shift modifier only.
    pub const SHIFT: Self = Self {
        primary: false,
        ctrl: false,
        alt: false,
        shift: true,
        super_key: false,
    };

    /// Converts this abstract modifier specification into crossterm [`KeyModifiers`]
    /// for a target [`Platform`].
    pub fn to_crossterm_modifiers(self, platform: Platform) -> KeyModifiers {
        let mut mods = KeyModifiers::NONE;
        if self.shift {
            mods |= KeyModifiers::SHIFT;
        }
        if self.alt {
            mods |= KeyModifiers::ALT;
        }
        if self.ctrl {
            mods |= KeyModifiers::CONTROL;
        }
        if self.super_key {
            mods |= KeyModifiers::SUPER;
        }
        if self.primary {
            if platform.is_mac() {
                mods |= KeyModifiers::SUPER;
            } else {
                mods |= KeyModifiers::CONTROL;
            }
        }
        mods
    }
}

/// A physical key chord representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    /// Key code (e.g. `Char('c')`, `Enter`, `F(2)`, `Tab`).
    pub code: KeyCode,
    /// Required modifier flags.
    pub modifiers: Modifiers,
}

impl KeyChord {
    /// Creates a key chord with no modifiers.
    pub const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: Modifiers::NONE,
        }
    }

    /// Creates a key chord requiring the platform primary modifier (⌘ on macOS, Ctrl on Win/Linux).
    pub const fn primary(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: Modifiers::PRIMARY,
        }
    }

    /// Creates a key chord requiring Primary + Shift.
    pub const fn primary_shift(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: Modifiers::PRIMARY_SHIFT,
        }
    }

    /// Creates a key chord requiring Alt / Option.
    pub const fn alt(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: Modifiers::ALT,
        }
    }

    /// Creates a key chord requiring Control.
    pub const fn ctrl(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: Modifiers::CTRL,
        }
    }

    /// Creates a key chord requiring Shift.
    pub const fn shift(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: Modifiers::SHIFT,
        }
    }

    /// Creates a key chord with arbitrary modifiers.
    pub const fn new(code: KeyCode, modifiers: Modifiers) -> Self {
        Self { code, modifiers }
    }

    /// Checks whether this key chord matches an incoming Crossterm [`KeyEvent`] on `platform`.
    pub fn matches(&self, key: &KeyEvent, platform: Platform) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }

        let expected_mods = self.modifiers.to_crossterm_modifiers(platform);

        // Check key code match
        let (code_matches, is_shift_implied) = match (self.code, key.code) {
            (KeyCode::Char(c1), KeyCode::Char(c2)) => {
                if self.modifiers.primary || self.modifiers.ctrl || self.modifiers.alt {
                    // With modifier chords (e.g. ⌘C or Ctrl+C), case is insensitive
                    (c1.eq_ignore_ascii_case(&c2), false)
                } else if c1.is_ascii_uppercase() {
                    // Self is uppercase (e.g. 'N', 'J', 'P', 'B', 'G', 'F', 'C', 'T', 'Z')
                    // Matches if incoming char is uppercase, or lowercase with SHIFT modifier
                    let matches = c2 == c1
                        || (c2.to_ascii_uppercase() == c1
                            && key.modifiers.contains(KeyModifiers::SHIFT));
                    (matches, true)
                } else {
                    // Self is lowercase (e.g. 'n', 'r', 'y', 'x', 'p', 'd', 'u', 's', 'b', 't', 'w', 'q', 'g', 'j', 'h', 'l', 'z')
                    // Must match lowercase char and NOT have SHIFT
                    let matches = c2 == c1 && !key.modifiers.contains(KeyModifiers::SHIFT);
                    (matches, false)
                }
            }
            (KeyCode::BackTab, KeyCode::BackTab) => (true, true),
            (KeyCode::Tab, KeyCode::BackTab) if self.modifiers.shift => (true, true),
            (KeyCode::BackTab, KeyCode::Tab) if key.modifiers.contains(KeyModifiers::SHIFT) => {
                (true, true)
            }
            (c1, c2) => (c1 == c2, false),
        };

        if !code_matches {
            return false;
        }

        // Check modifiers match
        let is_punctuation = matches!(
            self.code,
            KeyCode::Char(
                '?' | '*'
                    | '/'
                    | '~'
                    | '!'
                    | '@'
                    | '#'
                    | '$'
                    | '%'
                    | '^'
                    | '&'
                    | '('
                    | ')'
                    | '_'
                    | '+'
                    | '{'
                    | '}'
                    | '|'
                    | ':'
                    | '"'
                    | '<'
                    | '>'
            )
        );

        if is_shift_implied
            || (is_punctuation
                && !self.modifiers.primary
                && !self.modifiers.ctrl
                && !self.modifiers.alt)
        {
            let clean_key = key.modifiers - KeyModifiers::SHIFT;
            let clean_expected = expected_mods - KeyModifiers::SHIFT;
            clean_key == clean_expected
        } else {
            key.modifiers == expected_mods
        }
    }

    /// Formats the key chord for human-readable display according to `platform`.
    pub fn format(&self, platform: Platform) -> String {
        ShortcutFormatter::format(self, platform)
    }
}

/// Formatter for displaying key chords cleanly on macOS or Windows/Linux.
pub struct ShortcutFormatter;

impl ShortcutFormatter {
    /// Formats a key chord into a clean, platform-idiomatic display string.
    pub fn format(chord: &KeyChord, platform: Platform) -> String {
        if platform.is_mac() {
            Self::format_mac(chord)
        } else {
            Self::format_standard(chord)
        }
    }

    fn format_mac(chord: &KeyChord) -> String {
        let mut s = String::new();
        if chord.modifiers.primary || chord.modifiers.super_key {
            s.push('⌘');
        }
        if chord.modifiers.ctrl {
            s.push('⌃');
        }
        if chord.modifiers.alt {
            s.push('⌥');
        }
        if chord.modifiers.shift {
            s.push('⇧');
        }

        let key_str = match chord.code {
            KeyCode::Enter => "Enter",
            KeyCode::Backspace => "Backspace",
            KeyCode::Tab => "Tab",
            KeyCode::BackTab => {
                if !chord.modifiers.shift {
                    "⇧Tab"
                } else {
                    "Tab"
                }
            }
            KeyCode::Delete => "Delete",
            KeyCode::Esc => "Esc",
            KeyCode::Left => "Left",
            KeyCode::Right => "Right",
            KeyCode::Up => "Up",
            KeyCode::Down => "Down",
            KeyCode::PageUp => "PgUp",
            KeyCode::PageDown => "PgDn",
            KeyCode::Home => "Home",
            KeyCode::End => "End",
            KeyCode::F(n) => return format!("{s}F{n}"),
            KeyCode::Char(' ') => "Space",
            KeyCode::Char(c) => return format!("{s}{}", c.to_ascii_uppercase()),
            _ => "?",
        };

        if s.is_empty() {
            key_str.to_string()
        } else {
            format!("{s}{key_str}")
        }
    }

    fn format_standard(chord: &KeyChord) -> String {
        let mut parts = Vec::new();
        if chord.modifiers.primary || chord.modifiers.ctrl {
            parts.push("Ctrl");
        }
        if chord.modifiers.alt {
            parts.push("Alt");
        }
        if chord.modifiers.shift {
            parts.push("Shift");
        }
        if chord.modifiers.super_key {
            parts.push("Win");
        }

        let key_str = match chord.code {
            KeyCode::Enter => "Enter",
            KeyCode::Backspace => "Backspace",
            KeyCode::Tab => "Tab",
            KeyCode::BackTab => "Shift+Tab",
            KeyCode::Delete => "Delete",
            KeyCode::Esc => "Esc",
            KeyCode::Left => "Left",
            KeyCode::Right => "Right",
            KeyCode::Up => "Up",
            KeyCode::Down => "Down",
            KeyCode::PageUp => "PgUp",
            KeyCode::PageDown => "PgDn",
            KeyCode::Home => "Home",
            KeyCode::End => "End",
            KeyCode::F(n) => {
                let f_str = format!("F{n}");
                if parts.is_empty() {
                    return f_str;
                }
                parts.push(&f_str);
                return parts.join("+");
            }
            KeyCode::Char(' ') => "Space",
            KeyCode::Char(c) => {
                let c_str = c.to_ascii_uppercase().to_string();
                if parts.is_empty() {
                    return c_str;
                }
                let mut p = parts;
                p.push(&c_str);
                return p.join("+");
            }
            _ => "?",
        };

        if parts.is_empty() {
            key_str.to_string()
        } else {
            let mut p = parts;
            p.push(key_str);
            p.join("+")
        }
    }
}

/// A binding between an [`Action`], a [`KeyChord`], and an [`ActionContext`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutBinding {
    /// The action this shortcut triggers.
    pub action: Action,
    /// The physical key chord.
    pub chord: KeyChord,
    /// The context in which this shortcut is valid.
    pub context: ActionContext,
    /// Whether this is the primary shortcut for display in help and command palette.
    pub is_primary: bool,
}

impl ShortcutBinding {
    /// Creates a new primary shortcut binding.
    pub const fn primary(action: Action, chord: KeyChord, context: ActionContext) -> Self {
        Self {
            action,
            chord,
            context,
            is_primary: true,
        }
    }

    /// Creates a new secondary (alias) shortcut binding.
    pub const fn secondary(action: Action, chord: KeyChord, context: ActionContext) -> Self {
        Self {
            action,
            chord,
            context,
            is_primary: false,
        }
    }
}

/// Represents a detected shortcut conflict between two actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutConflict {
    /// The conflicting key chord.
    pub chord: KeyChord,
    /// The context where collision occurs.
    pub context: ActionContext,
    /// The platform where collision occurs.
    pub platform: Platform,
    /// The first action bound.
    pub action1: Action,
    /// The second action bound.
    pub action2: Action,
}

/// Centralized registry of all keyboard shortcuts.
pub struct ShortcutRegistry {
    bindings: Vec<ShortcutBinding>,
}

static GLOBAL_SHORTCUT_REGISTRY: LazyLock<ShortcutRegistry> =
    LazyLock::new(ShortcutRegistry::build_default);

impl ShortcutRegistry {
    /// Returns a reference to the global centralized shortcut registry.
    pub fn global() -> &'static Self {
        &GLOBAL_SHORTCUT_REGISTRY
    }

    /// Builds the default shortcut map conforming to TerminalVision signature specifications.
    fn build_default() -> Self {
        let mut reg = Self {
            bindings: Vec::new(),
        };

        // NAVIGATION (Context: FileManager)
        reg.register(ShortcutBinding::primary(
            Action::Open,
            KeyChord::plain(KeyCode::Enter),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::GoParent,
            KeyChord::plain(KeyCode::Backspace),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::SwitchPane,
            KeyChord::plain(KeyCode::Tab),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::SwitchPane,
            KeyChord::shift(KeyCode::BackTab),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::GoBack,
            KeyChord::alt(KeyCode::Left),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::GoForward,
            KeyChord::alt(KeyCode::Right),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::GoHome,
            KeyChord::plain(KeyCode::Home),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::GoEnd,
            KeyChord::plain(KeyCode::End),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::PageUp,
            KeyChord::plain(KeyCode::PageUp),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::PageDown,
            KeyChord::plain(KeyCode::PageDown),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::MoveUp,
            KeyChord::plain(KeyCode::Up),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::MoveDown,
            KeyChord::plain(KeyCode::Down),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::MoveLeft,
            KeyChord::plain(KeyCode::Left),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::MoveRight,
            KeyChord::plain(KeyCode::Right),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::MoveUp,
            KeyChord::plain(KeyCode::Char('k')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::MoveDown,
            KeyChord::plain(KeyCode::Char('j')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::MoveLeft,
            KeyChord::plain(KeyCode::Char('h')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::MoveRight,
            KeyChord::plain(KeyCode::Char('l')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::JumpToPath,
            KeyChord::primary(KeyCode::Char('l')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::JumpToPath,
            KeyChord::plain(KeyCode::Char('g')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::SmartJump,
            KeyChord::primary(KeyCode::Char('p')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::SmartJump,
            KeyChord::shift(KeyCode::Char('J')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::GoHomeDir,
            KeyChord::plain(KeyCode::Char('~')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::RevealContext,
            KeyChord::shift(KeyCode::Char('C')),
            ActionContext::FileManager,
        ));

        // TABS (Context: FileManager)
        reg.register(ShortcutBinding::primary(
            Action::NewTab,
            KeyChord::plain(KeyCode::Char('t')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::CloseTab,
            KeyChord::plain(KeyCode::Char('w')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::NextTab,
            KeyChord::plain(KeyCode::Char(']')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::PreviousTab,
            KeyChord::plain(KeyCode::Char('[')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::DuplicateTab,
            KeyChord::shift(KeyCode::Char('T')),
            ActionContext::FileManager,
        ));

        // FILE OPERATIONS (Context: FileManager)
        reg.register(ShortcutBinding::primary(
            Action::Copy,
            KeyChord::primary(KeyCode::Char('c')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::Copy,
            KeyChord::plain(KeyCode::Char('y')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::Cut,
            KeyChord::primary(KeyCode::Char('x')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::Cut,
            KeyChord::plain(KeyCode::Char('x')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::Paste,
            KeyChord::primary(KeyCode::Char('v')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::Paste,
            KeyChord::plain(KeyCode::Char('p')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::SelectAll,
            KeyChord::primary(KeyCode::Char('a')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::Rename,
            KeyChord::plain(KeyCode::F(2)),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::Rename,
            KeyChord::plain(KeyCode::Char('r')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::Delete,
            KeyChord::plain(KeyCode::Delete),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::Delete,
            KeyChord::plain(KeyCode::Char('d')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::NewDirectory,
            KeyChord::primary_shift(KeyCode::Char('n')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::NewDirectory,
            KeyChord::shift(KeyCode::Char('N')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::NewFile,
            KeyChord::plain(KeyCode::Char('n')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::DeselectAll,
            KeyChord::plain(KeyCode::Char('u')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::InvertSelection,
            KeyChord::plain(KeyCode::Char('*')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::SelectRangeUp,
            KeyChord::shift(KeyCode::Up),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::SelectRangeDown,
            KeyChord::shift(KeyCode::Down),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::ContextMenu,
            KeyChord::shift(KeyCode::F(10)),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::GetInfo,
            KeyChord::primary(KeyCode::Char('i')),
            ActionContext::FileManager,
        ));

        // VIEW & PREVIEW (Context: FileManager)
        reg.register(ShortcutBinding::primary(
            Action::RefreshDirectory,
            KeyChord::primary(KeyCode::Char('r')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::RefreshDirectory,
            KeyChord::plain(KeyCode::F(5)),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::Preview,
            KeyChord::plain(KeyCode::Char(' ')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::Preview,
            KeyChord::plain(KeyCode::Char('v')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::ToggleHidden,
            KeyChord::plain(KeyCode::Char('.')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::ChangeSort,
            KeyChord::plain(KeyCode::Char('s')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::ToggleFocusMode,
            KeyChord::plain(KeyCode::Char('z')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::ToggleFocusMode,
            KeyChord::shift(KeyCode::Char('Z')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::FileRadar,
            KeyChord::shift(KeyCode::Char('F')),
            ActionContext::FileManager,
        ));

        // SEARCH (Context: FileManager & Search)
        reg.register(ShortcutBinding::primary(
            Action::StartSearch,
            KeyChord::primary(KeyCode::Char('f')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::StartSearch,
            KeyChord::plain(KeyCode::Char('/')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::Cancel,
            KeyChord::plain(KeyCode::Esc),
            ActionContext::Search,
        ));
        reg.register(ShortcutBinding::primary(
            Action::CycleSearchMode,
            KeyChord::plain(KeyCode::Tab),
            ActionContext::Search,
        ));

        // BOOKMARKS (Context: FileManager)
        reg.register(ShortcutBinding::primary(
            Action::AddBookmark,
            KeyChord::plain(KeyCode::Char('b')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::OpenBookmarks,
            KeyChord::shift(KeyCode::Char('B')),
            ActionContext::FileManager,
        ));

        // GIT & PROJECT (Context: FileManager)
        reg.register(ShortcutBinding::primary(
            Action::GitStatusPanel,
            KeyChord::shift(KeyCode::Char('G')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::ProjectCockpit,
            KeyChord::shift(KeyCode::Char('P')),
            ActionContext::FileManager,
        ));

        // TERMINAL & GLOBAL SHORTCUTS
        reg.register(ShortcutBinding::primary(
            Action::ToggleTerminalFocus,
            KeyChord::ctrl(KeyCode::Char('t')),
            ActionContext::Global,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::ToggleTerminalFocus,
            KeyChord::plain(KeyCode::F(12)),
            ActionContext::Global,
        ));

        // COMMAND PALETTE / COMMAND CENTER
        reg.register(ShortcutBinding::primary(
            Action::CommandPalette,
            KeyChord::primary(KeyCode::Char('k')),
            ActionContext::FileManager,
        ));

        // HELP & SYSTEM
        reg.register(ShortcutBinding::primary(
            Action::Help,
            KeyChord::plain(KeyCode::Char('?')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::Quit,
            KeyChord::plain(KeyCode::Char('q')),
            ActionContext::FileManager,
        ));
        reg.register(ShortcutBinding::primary(
            Action::Cancel,
            KeyChord::plain(KeyCode::Esc),
            ActionContext::FileManager,
        ));

        // OVERLAYS (Preview, Help, Dialogs)
        reg.register(ShortcutBinding::primary(
            Action::Cancel,
            KeyChord::plain(KeyCode::Esc),
            ActionContext::Preview,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::Cancel,
            KeyChord::plain(KeyCode::Char('q')),
            ActionContext::Preview,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::Cancel,
            KeyChord::plain(KeyCode::Enter),
            ActionContext::Preview,
        ));

        reg.register(ShortcutBinding::primary(
            Action::Cancel,
            KeyChord::plain(KeyCode::Esc),
            ActionContext::Help,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::Cancel,
            KeyChord::plain(KeyCode::Char('q')),
            ActionContext::Help,
        ));
        reg.register(ShortcutBinding::secondary(
            Action::Cancel,
            KeyChord::plain(KeyCode::Enter),
            ActionContext::Help,
        ));

        reg.register(ShortcutBinding::primary(
            Action::Cancel,
            KeyChord::plain(KeyCode::Esc),
            ActionContext::Dialog,
        ));

        reg
    }

    /// Registers a new shortcut binding.
    pub fn register(&mut self, binding: ShortcutBinding) {
        self.bindings.push(binding);
    }

    /// Looks up an action matching `key` in `context` for `platform`.
    ///
    /// Context-specific bindings take precedence over Global bindings.
    pub fn lookup(
        &self,
        key: &KeyEvent,
        context: ActionContext,
        platform: Platform,
    ) -> Option<Action> {
        // 1. First search for an exact match in the active context
        for binding in &self.bindings {
            if binding.context == context && binding.chord.matches(key, platform) {
                return Some(binding.action);
            }
        }

        // 2. Fall back to Global context bindings if no context-specific match
        if context != ActionContext::Global {
            for binding in &self.bindings {
                if binding.context == ActionContext::Global && binding.chord.matches(key, platform)
                {
                    return Some(binding.action);
                }
            }
        }

        None
    }

    /// Returns the primary formatted shortcut string for `action` on `platform`.
    pub fn primary_shortcut(&self, action: Action, platform: Platform) -> Option<String> {
        self.bindings
            .iter()
            .find(|b| b.action == action && b.is_primary)
            .map(|b| b.chord.format(platform))
            .or_else(|| {
                self.bindings
                    .iter()
                    .find(|b| b.action == action)
                    .map(|b| b.chord.format(platform))
            })
    }

    /// Returns all bindings registered for `action`.
    pub fn shortcuts_for_action(&self, action: Action) -> Vec<&ShortcutBinding> {
        self.bindings
            .iter()
            .filter(|b| b.action == action)
            .collect()
    }

    /// Returns all primary bindings in a given category, formatted for `platform`.
    /// Returns tuples of `(Action, FormattedShortcut, Description)`.
    pub fn shortcuts_by_category(
        &self,
        category: ActionCategory,
        platform: Platform,
    ) -> Vec<(Action, String, &'static str)> {
        let action_reg = ActionRegistry::global();
        let mut results = Vec::new();

        for meta in action_reg.by_category(category) {
            if let Some(sc) = self.primary_shortcut(meta.action, platform) {
                results.push((meta.action, sc, meta.description));
            }
        }

        results
    }

    /// Detects all conflicting shortcut bindings across all platforms.
    ///
    /// A conflict exists when two distinct actions claim the same physical key event
    /// in the same context (or across a context and Global).
    pub fn detect_conflicts(&self) -> Vec<ShortcutConflict> {
        let mut conflicts = Vec::new();
        let platforms = [Platform::Mac, Platform::Windows, Platform::Linux];

        for &platform in &platforms {
            for i in 0..self.bindings.len() {
                for j in (i + 1)..self.bindings.len() {
                    let b1 = &self.bindings[i];
                    let b2 = &self.bindings[j];

                    if b1.action == b2.action {
                        continue;
                    }

                    let contexts_overlap = b1.context == b2.context
                        || b1.context == ActionContext::Global
                        || b2.context == ActionContext::Global;

                    if !contexts_overlap {
                        continue;
                    }

                    // Check if both chords produce the same key event
                    let mods1 = b1.chord.modifiers.to_crossterm_modifiers(platform);
                    let mods2 = b2.chord.modifiers.to_crossterm_modifiers(platform);

                    let code1_norm = match b1.chord.code {
                        KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                        c => c,
                    };
                    let code2_norm = match b2.chord.code {
                        KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                        c => c,
                    };

                    if code1_norm == code2_norm && mods1 == mods2 {
                        conflicts.push(ShortcutConflict {
                            chord: b1.chord,
                            context: b1.context,
                            platform,
                            action1: b1.action,
                            action2: b2.action,
                        });
                    }
                }
            }
        }

        conflicts
    }
}

impl Default for ShortcutRegistry {
    fn default() -> Self {
        Self::build_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn plain_press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_zero_conflicts_in_default_registry() {
        let registry = ShortcutRegistry::global();
        let conflicts = registry.detect_conflicts();
        assert!(
            conflicts.is_empty(),
            "Found unexpected shortcut conflicts: {conflicts:#?}"
        );
    }

    #[test]
    fn test_mac_modifier_mapping() {
        let registry = ShortcutRegistry::global();
        let mac = Platform::Mac;

        // ⌘C -> Copy
        let cmd_c = press(KeyCode::Char('c'), KeyModifiers::SUPER);
        assert_eq!(
            registry.lookup(&cmd_c, ActionContext::FileManager, mac),
            Some(Action::Copy)
        );

        // ⌘X -> Cut
        let cmd_x = press(KeyCode::Char('x'), KeyModifiers::SUPER);
        assert_eq!(
            registry.lookup(&cmd_x, ActionContext::FileManager, mac),
            Some(Action::Cut)
        );

        // ⌘V -> Paste
        let cmd_v = press(KeyCode::Char('v'), KeyModifiers::SUPER);
        assert_eq!(
            registry.lookup(&cmd_v, ActionContext::FileManager, mac),
            Some(Action::Paste)
        );

        // ⌘A -> SelectAll
        let cmd_a = press(KeyCode::Char('a'), KeyModifiers::SUPER);
        assert_eq!(
            registry.lookup(&cmd_a, ActionContext::FileManager, mac),
            Some(Action::SelectAll)
        );

        // ⌘F -> StartSearch
        let cmd_f = press(KeyCode::Char('f'), KeyModifiers::SUPER);
        assert_eq!(
            registry.lookup(&cmd_f, ActionContext::FileManager, mac),
            Some(Action::StartSearch)
        );

        // ⌘R -> RefreshDirectory
        let cmd_r = press(KeyCode::Char('r'), KeyModifiers::SUPER);
        assert_eq!(
            registry.lookup(&cmd_r, ActionContext::FileManager, mac),
            Some(Action::RefreshDirectory)
        );

        // ⌘K -> CommandPalette
        let cmd_k = press(KeyCode::Char('k'), KeyModifiers::SUPER);
        assert_eq!(
            registry.lookup(&cmd_k, ActionContext::FileManager, mac),
            Some(Action::CommandPalette)
        );

        // ⌘L -> JumpToPath
        let cmd_l = press(KeyCode::Char('l'), KeyModifiers::SUPER);
        assert_eq!(
            registry.lookup(&cmd_l, ActionContext::FileManager, mac),
            Some(Action::JumpToPath)
        );

        // ⌘⇧N -> NewDirectory
        let cmd_shift_n = press(
            KeyCode::Char('n'),
            KeyModifiers::SUPER | KeyModifiers::SHIFT,
        );
        assert_eq!(
            registry.lookup(&cmd_shift_n, ActionContext::FileManager, mac),
            Some(Action::NewDirectory)
        );
    }

    #[test]
    fn test_windows_linux_modifier_mapping() {
        let registry = ShortcutRegistry::global();
        let win = Platform::Windows;
        let linux = Platform::Linux;

        for platform in [win, linux] {
            // Ctrl+C -> Copy
            let ctrl_c = press(KeyCode::Char('c'), KeyModifiers::CONTROL);
            assert_eq!(
                registry.lookup(&ctrl_c, ActionContext::FileManager, platform),
                Some(Action::Copy)
            );

            // Ctrl+X -> Cut
            let ctrl_x = press(KeyCode::Char('x'), KeyModifiers::CONTROL);
            assert_eq!(
                registry.lookup(&ctrl_x, ActionContext::FileManager, platform),
                Some(Action::Cut)
            );

            // Ctrl+V -> Paste
            let ctrl_v = press(KeyCode::Char('v'), KeyModifiers::CONTROL);
            assert_eq!(
                registry.lookup(&ctrl_v, ActionContext::FileManager, platform),
                Some(Action::Paste)
            );

            // Ctrl+A -> SelectAll
            let ctrl_a = press(KeyCode::Char('a'), KeyModifiers::CONTROL);
            assert_eq!(
                registry.lookup(&ctrl_a, ActionContext::FileManager, platform),
                Some(Action::SelectAll)
            );

            // Ctrl+F -> StartSearch
            let ctrl_f = press(KeyCode::Char('f'), KeyModifiers::CONTROL);
            assert_eq!(
                registry.lookup(&ctrl_f, ActionContext::FileManager, platform),
                Some(Action::StartSearch)
            );

            // Ctrl+R -> RefreshDirectory
            let ctrl_r = press(KeyCode::Char('r'), KeyModifiers::CONTROL);
            assert_eq!(
                registry.lookup(&ctrl_r, ActionContext::FileManager, platform),
                Some(Action::RefreshDirectory)
            );

            // Ctrl+K -> CommandPalette
            let ctrl_k = press(KeyCode::Char('k'), KeyModifiers::CONTROL);
            assert_eq!(
                registry.lookup(&ctrl_k, ActionContext::FileManager, platform),
                Some(Action::CommandPalette)
            );

            // Ctrl+L -> JumpToPath
            let ctrl_l = press(KeyCode::Char('l'), KeyModifiers::CONTROL);
            assert_eq!(
                registry.lookup(&ctrl_l, ActionContext::FileManager, platform),
                Some(Action::JumpToPath)
            );

            // Ctrl+Shift+N -> NewDirectory
            let ctrl_shift_n = press(
                KeyCode::Char('n'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            );
            assert_eq!(
                registry.lookup(&ctrl_shift_n, ActionContext::FileManager, platform),
                Some(Action::NewDirectory)
            );
        }
    }

    #[test]
    fn test_context_filtering() {
        let registry = ShortcutRegistry::global();
        let mac = Platform::Mac;

        // F2 in FileManager is Rename
        let f2 = plain_press(KeyCode::F(2));
        assert_eq!(
            registry.lookup(&f2, ActionContext::FileManager, mac),
            Some(Action::Rename)
        );

        // F2 in Search or Terminal is None
        assert_eq!(registry.lookup(&f2, ActionContext::Search, mac), None);
        assert_eq!(registry.lookup(&f2, ActionContext::Terminal, mac), None);

        // Ctrl+T in Terminal is ToggleTerminalFocus (Global)
        let ctrl_t = press(KeyCode::Char('t'), KeyModifiers::CONTROL);
        assert_eq!(
            registry.lookup(&ctrl_t, ActionContext::Terminal, mac),
            Some(Action::ToggleTerminalFocus)
        );
        assert_eq!(
            registry.lookup(&ctrl_t, ActionContext::FileManager, mac),
            Some(Action::ToggleTerminalFocus)
        );
    }

    #[test]
    fn test_shortcut_formatting_mac_vs_windows() {
        let registry = ShortcutRegistry::global();

        // Copy
        assert_eq!(
            registry.primary_shortcut(Action::Copy, Platform::Mac),
            Some("⌘C".to_string())
        );
        assert_eq!(
            registry.primary_shortcut(Action::Copy, Platform::Windows),
            Some("Ctrl+C".to_string())
        );

        // Refresh
        assert_eq!(
            registry.primary_shortcut(Action::RefreshDirectory, Platform::Mac),
            Some("⌘R".to_string())
        );
        assert_eq!(
            registry.primary_shortcut(Action::RefreshDirectory, Platform::Windows),
            Some("Ctrl+R".to_string())
        );

        // Preview
        assert_eq!(
            registry.primary_shortcut(Action::Preview, Platform::Mac),
            Some("Space".to_string())
        );

        // NewDirectory
        assert_eq!(
            registry.primary_shortcut(Action::NewDirectory, Platform::Mac),
            Some("⌘⇧N".to_string())
        );
        assert_eq!(
            registry.primary_shortcut(Action::NewDirectory, Platform::Windows),
            Some("Ctrl+Shift+N".to_string())
        );
    }
}
