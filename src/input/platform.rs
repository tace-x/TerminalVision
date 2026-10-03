//! Host platform detection and platform-specific keyboard/modifier semantics.
//!
//! TerminalVision uses platform-native conventions:
//! - macOS: Command (⌘) is the primary modifier for application shortcuts.
//! - Windows & Linux: Control (Ctrl) is the primary modifier.

/// The operating system platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Apple macOS.
    Mac,
    /// Microsoft Windows.
    Windows,
    /// Linux and other Unix-like platforms.
    Linux,
}

impl Platform {
    /// Detects the current host platform at runtime based on compile target.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    /// Whether this platform is macOS.
    pub const fn is_mac(self) -> bool {
        matches!(self, Self::Mac)
    }

    /// Whether this platform is Windows.
    pub const fn is_windows(self) -> bool {
        matches!(self, Self::Windows)
    }

    /// Whether this platform is Linux or Unix-like.
    pub const fn is_linux(self) -> bool {
        matches!(self, Self::Linux)
    }

    /// Returns the human-readable display label for the primary modifier on this platform.
    ///
    /// - macOS: `"⌘"`
    /// - Windows / Linux: `"Ctrl"`
    pub const fn primary_modifier_label(self) -> &'static str {
        match self {
            Self::Mac => "⌘",
            Self::Windows | Self::Linux => "Ctrl",
        }
    }

    /// Returns the human-readable display label for the Alt / Option modifier.
    ///
    /// - macOS: `"⌥"`
    /// - Windows / Linux: `"Alt"`
    pub const fn alt_modifier_label(self) -> &'static str {
        match self {
            Self::Mac => "⌥",
            Self::Windows | Self::Linux => "Alt",
        }
    }

    /// Returns the human-readable display label for the Shift modifier.
    ///
    /// - macOS: `"⇧"`
    /// - Windows / Linux: `"Shift"`
    pub const fn shift_modifier_label(self) -> &'static str {
        match self {
            Self::Mac => "⇧",
            Self::Windows | Self::Linux => "Shift",
        }
    }

    /// Returns the human-readable display label for the Control modifier.
    ///
    /// - macOS: `"⌃"`
    /// - Windows / Linux: `"Ctrl"`
    pub const fn ctrl_modifier_label(self) -> &'static str {
        match self {
            Self::Mac => "⌃",
            Self::Windows | Self::Linux => "Ctrl",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let current = Platform::current();
        if cfg!(target_os = "macos") {
            assert_eq!(current, Platform::Mac);
            assert!(current.is_mac());
            assert!(!current.is_windows());
            assert!(!current.is_linux());
            assert_eq!(current.primary_modifier_label(), "⌘");
            assert_eq!(current.alt_modifier_label(), "⌥");
            assert_eq!(current.shift_modifier_label(), "⇧");
            assert_eq!(current.ctrl_modifier_label(), "⌃");
        } else if cfg!(target_os = "windows") {
            assert_eq!(current, Platform::Windows);
            assert!(current.is_windows());
            assert_eq!(current.primary_modifier_label(), "Ctrl");
        } else {
            assert_eq!(current, Platform::Linux);
            assert!(current.is_linux());
            assert_eq!(current.primary_modifier_label(), "Ctrl");
        }
    }

    #[test]
    fn test_platform_labels_for_all_variants() {
        assert_eq!(Platform::Mac.primary_modifier_label(), "⌘");
        assert_eq!(Platform::Mac.alt_modifier_label(), "⌥");
        assert_eq!(Platform::Mac.shift_modifier_label(), "⇧");
        assert_eq!(Platform::Mac.ctrl_modifier_label(), "⌃");

        assert_eq!(Platform::Windows.primary_modifier_label(), "Ctrl");
        assert_eq!(Platform::Windows.alt_modifier_label(), "Alt");
        assert_eq!(Platform::Windows.shift_modifier_label(), "Shift");
        assert_eq!(Platform::Windows.ctrl_modifier_label(), "Ctrl");

        assert_eq!(Platform::Linux.primary_modifier_label(), "Ctrl");
        assert_eq!(Platform::Linux.alt_modifier_label(), "Alt");
        assert_eq!(Platform::Linux.shift_modifier_label(), "Shift");
        assert_eq!(Platform::Linux.ctrl_modifier_label(), "Ctrl");
    }
}
