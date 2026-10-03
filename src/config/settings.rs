//! Persistent application configuration and user settings.
//!
//! Handles saving and loading lightweight state (tabs, bookmarks, and active pane)
//! across application launches without external dependencies.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// The pane that was active when state was saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePaneConfig {
    #[default]
    Left,
    Right,
}

impl ActivePaneConfig {
    /// Returns the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

impl FromStr for ActivePaneConfig {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            _ => Err(()),
        }
    }
}

/// A persisted bookmark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarkConfig {
    /// The display name.
    pub name: String,
    /// The target directory path.
    pub path: PathBuf,
}

impl BookmarkConfig {
    /// Creates a new bookmark configuration.
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
        }
    }
}

/// Persisted tab configuration for a single pane.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PaneTabsConfig {
    /// Which tab was active (0-indexed).
    pub active_tab_index: usize,
    /// Directory paths for all open tabs in this pane.
    pub tab_paths: Vec<PathBuf>,
}

impl PaneTabsConfig {
    /// Creates a new pane tabs configuration.
    pub fn new(active_tab_index: usize, tab_paths: Vec<PathBuf>) -> Self {
        Self {
            active_tab_index,
            tab_paths,
        }
    }
}

/// The complete persistent configuration for TerminalVision.
/// The complete persistent configuration for TerminalVision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Active pane selection.
    pub active_pane: ActivePaneConfig,
    /// Left pane tabs.
    pub left_tabs: PaneTabsConfig,
    /// Right pane tabs.
    pub right_tabs: PaneTabsConfig,
    /// Saved directory bookmarks.
    pub bookmarks: Vec<BookmarkConfig>,
    /// Saved recent directory locations.
    pub recent_locations: Vec<PathBuf>,
    /// Saved recent file paths.
    pub recent_files: Vec<PathBuf>,
    /// Whether reduced motion is enabled (skips UI transition frames).
    pub reduced_motion: bool,
    /// Active visual theme identifier (e.g. "terminalvision", "cyberpunk", "nord").
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            active_pane: ActivePaneConfig::default(),
            left_tabs: PaneTabsConfig::default(),
            right_tabs: PaneTabsConfig::default(),
            bookmarks: Vec::new(),
            recent_locations: Vec::new(),
            recent_files: Vec::new(),
            reduced_motion: false,
            theme: "terminalvision".to_string(),
        }
    }
}

impl Settings {
    /// Creates an empty default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Serializes configuration into a deterministic, human-readable string.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str("# TerminalVision Configuration\n");
        out.push_str("version = 1\n\n");

        out.push_str("[settings]\n");
        out.push_str(&format!("active_pane = {}\n", self.active_pane.as_str()));
        out.push_str(&format!("reduced_motion = {}\n", self.reduced_motion));
        out.push_str(&format!("theme = {}\n\n", self.theme));

        out.push_str("[tabs.left]\n");
        out.push_str(&format!("active = {}\n", self.left_tabs.active_tab_index));
        for path in &self.left_tabs.tab_paths {
            out.push_str(&format!("path = {}\n", path.display()));
        }
        out.push('\n');

        out.push_str("[tabs.right]\n");
        out.push_str(&format!("active = {}\n", self.right_tabs.active_tab_index));
        for path in &self.right_tabs.tab_paths {
            out.push_str(&format!("path = {}\n", path.display()));
        }
        out.push('\n');

        out.push_str("[bookmarks]\n");
        for b in &self.bookmarks {
            out.push_str(&format!("bookmark = {} | {}\n", b.name, b.path.display()));
        }
        out.push('\n');

        if !self.recent_locations.is_empty() || !self.recent_files.is_empty() {
            out.push_str("[history]\n");
            for path in &self.recent_locations {
                out.push_str(&format!("location = {}\n", path.display()));
            }
            for path in &self.recent_files {
                out.push_str(&format!("file = {}\n", path.display()));
            }
            out.push('\n');
        }

        out
    }

    /// Deserializes configuration from a string, safely ignoring unknown or malformed lines.
    pub fn deserialize(input: &str) -> Self {
        #[derive(Copy, Clone, PartialEq, Eq)]
        enum Section {
            None,
            Settings,
            TabsLeft,
            TabsRight,
            Bookmarks,
            History,
        }

        let mut settings = Self::default();
        let mut current_section = Section::None;

        for raw_line in input.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                let section_name = line[1..line.len() - 1].trim().to_lowercase();
                current_section = match section_name.as_str() {
                    "settings" | "appearance" => Section::Settings,
                    "tabs.left" | "tabs_left" => Section::TabsLeft,
                    "tabs.right" | "tabs_right" => Section::TabsRight,
                    "bookmarks" => Section::Bookmarks,
                    "history" | "recent" => Section::History,
                    _ => Section::None,
                };
                continue;
            }

            let Some((key, val)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim().to_lowercase();
            let val = val.trim();

            match current_section {
                Section::Settings => {
                    if key == "active_pane" {
                        settings.active_pane = val.parse().unwrap_or(settings.active_pane);
                    } else if key == "reduced_motion" {
                        settings.reduced_motion = val.parse().unwrap_or(false);
                    } else if key == "theme" && !val.is_empty() {
                        settings.theme = val.trim_matches('"').trim_matches('\'').to_lowercase();
                    }
                }
                Section::TabsLeft => {
                    if key == "active" {
                        if let Ok(idx) = val.parse::<usize>() {
                            settings.left_tabs.active_tab_index = idx;
                        }
                    } else if key == "path" && !val.is_empty() {
                        settings.left_tabs.tab_paths.push(PathBuf::from(val));
                    }
                }
                Section::TabsRight => {
                    if key == "active" {
                        if let Ok(idx) = val.parse::<usize>() {
                            settings.right_tabs.active_tab_index = idx;
                        }
                    } else if key == "path" && !val.is_empty() {
                        settings.right_tabs.tab_paths.push(PathBuf::from(val));
                    }
                }
                Section::Bookmarks => {
                    if key == "bookmark" && !val.is_empty() {
                        let (name, path_str) = if let Some((n, p)) = val.split_once('|') {
                            (n.trim().to_string(), p.trim())
                        } else {
                            let p = val.trim();
                            let path = PathBuf::from(p);
                            let derived_name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or(p)
                                .to_string();
                            (derived_name, p)
                        };

                        if !path_str.is_empty() {
                            let path = PathBuf::from(path_str);
                            // Avoid duplicate bookmark paths
                            if !settings.bookmarks.iter().any(|b| b.path == path) {
                                settings.bookmarks.push(BookmarkConfig::new(name, path));
                            }
                        }
                    }
                }
                Section::History => {
                    if (key == "location" || key == "dir" || key == "path") && !val.is_empty() {
                        let path = PathBuf::from(val);
                        if !settings.recent_locations.iter().any(|p| p == &path) {
                            settings.recent_locations.push(path);
                        }
                    } else if key == "file" && !val.is_empty() {
                        let path = PathBuf::from(val);
                        if !settings.recent_files.iter().any(|p| p == &path) {
                            settings.recent_files.push(path);
                        }
                    }
                }
                Section::None => {}
            }
        }

        settings
    }

    /// Loads settings from `path`.
    pub fn load_from_path(path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(path)?;
        Ok(Self::deserialize(&content))
    }

    /// Saves settings to `path`, ensuring parent directories exist and skipping redundant writes.
    pub fn save_to_path(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty() && !p.exists())
        {
            fs::create_dir_all(parent)?;
        }

        let content = self.serialize();

        // Avoid unnecessary disk write if file content has not changed
        if fs::read_to_string(path).is_ok_and(|existing| existing == content) {
            return Ok(());
        }

        fs::write(path, content)?;
        Ok(())
    }
}

/// Derives the Linux/XDG config path given optional XDG_CONFIG_HOME and HOME strings.
pub fn resolve_linux_config_path(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    if let Some(x) = xdg.filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(x).join("terminalvision").join("config.tv"));
    }
    if let Some(h) = home.filter(|s| !s.is_empty()) {
        return Some(
            PathBuf::from(h)
                .join(".config")
                .join("terminalvision")
                .join("config.tv"),
        );
    }
    None
}

/// Derives the Windows config path given optional APPDATA and USERPROFILE strings.
pub fn resolve_windows_config_path(
    appdata: Option<&str>,
    userprofile: Option<&str>,
) -> Option<PathBuf> {
    if let Some(a) = appdata.filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(a).join("terminalvision").join("config.tv"));
    }
    if let Some(u) = userprofile.filter(|s| !s.is_empty()) {
        return Some(
            PathBuf::from(u)
                .join(".config")
                .join("terminalvision")
                .join("config.tv"),
        );
    }
    None
}

/// Returns the OS-appropriate default configuration file path for TerminalVision.
pub fn default_config_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var("HOME").ok().filter(|h| !h.is_empty()) {
            return Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("terminalvision")
                    .join("config.tv"),
            );
        }
        if let Some(xdg) = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|x| !x.is_empty())
        {
            return Some(PathBuf::from(xdg).join("terminalvision").join("config.tv"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").ok();
        let userprofile = std::env::var("USERPROFILE").ok();
        if let Some(path) = resolve_windows_config_path(appdata.as_deref(), userprofile.as_deref())
        {
            return Some(path);
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let xdg = std::env::var("XDG_CONFIG_HOME").ok();
        let home = std::env::var("HOME").ok();
        if let Some(path) = resolve_linux_config_path(xdg.as_deref(), home.as_deref()) {
            return Some(path);
        }
    }

    // Generic fallback if primary platform variable was missing
    if let Some(home) = std::env::var("HOME").ok().filter(|h| !h.is_empty()) {
        return Some(
            PathBuf::from(home)
                .join(".config")
                .join("terminalvision")
                .join("config.tv"),
        );
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_settings_serialization_round_trip() {
        let settings = Settings::default();
        let serialized = settings.serialize();
        let loaded = Settings::deserialize(&serialized);
        assert_eq!(settings, loaded);
    }

    #[test]
    fn test_full_settings_serialization_round_trip() {
        let settings = Settings {
            active_pane: ActivePaneConfig::Right,
            left_tabs: PaneTabsConfig::new(
                1,
                vec![
                    PathBuf::from("/Users/alice/projects"),
                    PathBuf::from("/Users/alice/documents"),
                ],
            ),
            right_tabs: PaneTabsConfig::new(0, vec![PathBuf::from("/Users/alice/downloads")]),
            bookmarks: vec![
                BookmarkConfig::new("Projects", PathBuf::from("/Users/alice/projects")),
                BookmarkConfig::new("Downloads", PathBuf::from("/Users/alice/downloads")),
            ],
            recent_locations: vec![
                PathBuf::from("/Users/alice/projects"),
                PathBuf::from("/Users/alice/downloads"),
            ],
            recent_files: vec![PathBuf::from("/Users/alice/projects/main.rs")],
            reduced_motion: true,
            theme: "terminalvision".to_string(),
        };

        let serialized = settings.serialize();
        let loaded = Settings::deserialize(&serialized);
        assert_eq!(settings, loaded);
    }

    #[test]
    fn test_unicode_and_spaces_in_paths() {
        let settings = Settings {
            active_pane: ActivePaneConfig::Left,
            left_tabs: PaneTabsConfig::new(
                0,
                vec![
                    PathBuf::from("/home/user/📁 My Projects/🦀 Rust"),
                    PathBuf::from("/home/user/日本語/フォルダ"),
                ],
            ),
            right_tabs: PaneTabsConfig::new(
                0,
                vec![PathBuf::from("/Volumes/External Drive/Media Files")],
            ),
            bookmarks: vec![
                BookmarkConfig::new(
                    "📁 Projects",
                    PathBuf::from("/home/user/📁 My Projects/🦀 Rust"),
                ),
                BookmarkConfig::new("日本語", PathBuf::from("/home/user/日本語/フォルダ")),
            ],
            recent_locations: vec![PathBuf::from("/home/user/📁 My Projects/🦀 Rust")],
            recent_files: Vec::new(),
            reduced_motion: false,
            theme: "terminalvision".to_string(),
        };

        let serialized = settings.serialize();
        let loaded = Settings::deserialize(&serialized);
        assert_eq!(settings, loaded);
    }

    #[test]
    fn test_windows_style_paths() {
        let settings = Settings {
            active_pane: ActivePaneConfig::Right,
            left_tabs: PaneTabsConfig::new(
                0,
                vec![PathBuf::from(r"C:\Users\Admin\Documents\Project A")],
            ),
            right_tabs: PaneTabsConfig::new(0, vec![PathBuf::from(r"D:\Backups\2026\System Data")]),
            bookmarks: vec![BookmarkConfig::new(
                "Documents",
                PathBuf::from(r"C:\Users\Admin\Documents\Project A"),
            )],
            recent_locations: vec![PathBuf::from(r"C:\Users\Admin\Documents\Project A")],
            recent_files: vec![PathBuf::from(
                r"C:\Users\Admin\Documents\Project A\file.txt",
            )],
            reduced_motion: false,
            theme: "terminalvision".to_string(),
        };

        let serialized = settings.serialize();
        let loaded = Settings::deserialize(&serialized);
        assert_eq!(settings, loaded);
    }

    #[test]
    fn test_history_deduplication() {
        let raw = r#"
            [history]
            location = /home/user/proj1
            location = /home/user/proj1
            location = /home/user/proj2
            file = /home/user/proj1/src/main.rs
            file = /home/user/proj1/src/main.rs
        "#;
        let settings = Settings::deserialize(raw);
        assert_eq!(settings.recent_locations.len(), 2);
        assert_eq!(
            settings.recent_locations[0],
            PathBuf::from("/home/user/proj1")
        );
        assert_eq!(
            settings.recent_locations[1],
            PathBuf::from("/home/user/proj2")
        );
        assert_eq!(settings.recent_files.len(), 1);
        assert_eq!(
            settings.recent_files[0],
            PathBuf::from("/home/user/proj1/src/main.rs")
        );
    }

    #[test]
    fn test_malformed_configuration_resilience() {
        let garbage = r#"
            # Random comment
            [unknown_section]
            foo = bar
            [settings]
            active_pane = invalid_pane
            random_key = 123
            [tabs.left]
            active = not_a_number
            path = /valid/left/path
            invalid_line_without_equals
            = no_key
            path = 
            [bookmarks]
            bookmark = 
            bookmark = SinglePathOnlyWithoutPipe
            bookmark = Custom | /custom/path
        "#;

        let settings = Settings::deserialize(garbage);
        assert_eq!(settings.active_pane, ActivePaneConfig::Left); // fallback default
        assert_eq!(settings.left_tabs.active_tab_index, 0); // fallback default
        assert_eq!(
            settings.left_tabs.tab_paths,
            vec![PathBuf::from("/valid/left/path")]
        );
        assert_eq!(settings.bookmarks.len(), 2);
        assert_eq!(settings.bookmarks[0].name, "SinglePathOnlyWithoutPipe");
        assert_eq!(
            settings.bookmarks[0].path,
            PathBuf::from("SinglePathOnlyWithoutPipe")
        );
        assert_eq!(settings.bookmarks[1].name, "Custom");
        assert_eq!(settings.bookmarks[1].path, PathBuf::from("/custom/path"));
    }

    #[test]
    fn test_duplicate_bookmarks_deduplicated() {
        let raw = r#"
            [bookmarks]
            bookmark = First | /same/path
            bookmark = Second | /same/path
            bookmark = Third | /different/path
        "#;

        let settings = Settings::deserialize(raw);
        assert_eq!(settings.bookmarks.len(), 2);
        assert_eq!(settings.bookmarks[0].name, "First");
        assert_eq!(settings.bookmarks[0].path, PathBuf::from("/same/path"));
        assert_eq!(settings.bookmarks[1].name, "Third");
        assert_eq!(settings.bookmarks[1].path, PathBuf::from("/different/path"));
    }

    #[test]
    fn test_file_io_save_and_load_round_trip() {
        let temp_dir = std::env::temp_dir().join(format!("tv_test_cfg_{}", std::process::id()));
        let config_file = temp_dir.join("sub_dir").join("config.tv");

        let settings = Settings {
            active_pane: ActivePaneConfig::Right,
            left_tabs: PaneTabsConfig::new(0, vec![PathBuf::from("/tmp/test_left")]),
            right_tabs: PaneTabsConfig::new(0, vec![PathBuf::from("/tmp/test_right")]),
            bookmarks: vec![BookmarkConfig::new("Test", PathBuf::from("/tmp/test_left"))],
            recent_locations: vec![PathBuf::from("/tmp/test_left")],
            recent_files: Vec::new(),
            reduced_motion: false,
            theme: "terminalvision".to_string(),
        };

        // Save creates parent dirs
        settings.save_to_path(&config_file).expect("save succeeds");
        assert!(config_file.exists());

        // Load restores exactly
        let loaded = Settings::load_from_path(&config_file).expect("load succeeds");
        assert_eq!(settings, loaded);

        // Re-saving identical content is no-op
        settings
            .save_to_path(&config_file)
            .expect("redundant save succeeds");

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_default_config_path_resolution() {
        let path = default_config_path();
        if std::env::var("HOME").is_ok() || std::env::var("XDG_CONFIG_HOME").is_ok() {
            assert!(path.is_some(), "default config path should be resolved");
            let p = path.unwrap();
            assert!(p.ends_with(Path::new("terminalvision").join("config.tv")));

            #[cfg(target_os = "macos")]
            if std::env::var("HOME").is_ok() {
                assert!(
                    p.to_string_lossy().contains("Library/Application Support"),
                    "macOS config path should resolve in Library/Application Support"
                );
            }
        }
    }

    #[test]
    fn test_resolve_linux_config_path_variants() {
        // 1. Prioritize XDG_CONFIG_HOME when present
        let xdg_res = resolve_linux_config_path(Some("/custom/xdg"), Some("/home/user"));
        assert_eq!(
            xdg_res,
            Some(PathBuf::from("/custom/xdg/terminalvision/config.tv"))
        );

        // 2. Fall back to HOME/.config when XDG is None or empty
        let home_res = resolve_linux_config_path(None, Some("/home/user"));
        assert_eq!(
            home_res,
            Some(PathBuf::from("/home/user/.config/terminalvision/config.tv"))
        );

        let empty_xdg_res = resolve_linux_config_path(Some(""), Some("/home/user"));
        assert_eq!(
            empty_xdg_res,
            Some(PathBuf::from("/home/user/.config/terminalvision/config.tv"))
        );

        // 3. None when both are missing or empty
        assert_eq!(resolve_linux_config_path(None, None), None);
        assert_eq!(resolve_linux_config_path(Some(""), Some("")), None);
    }

    #[test]
    fn test_resolve_windows_config_path_variants() {
        // 1. Prioritize APPDATA when present
        let appdata_res = resolve_windows_config_path(
            Some("C:\\Users\\User\\AppData\\Roaming"),
            Some("C:\\Users\\User"),
        );
        assert_eq!(
            appdata_res,
            Some(
                PathBuf::from("C:\\Users\\User\\AppData\\Roaming")
                    .join("terminalvision")
                    .join("config.tv")
            )
        );

        // 2. Fall back to USERPROFILE/.config when APPDATA is None or empty
        let userprofile_res = resolve_windows_config_path(None, Some("C:\\Users\\User"));
        assert_eq!(
            userprofile_res,
            Some(
                PathBuf::from("C:\\Users\\User")
                    .join(".config")
                    .join("terminalvision")
                    .join("config.tv")
            )
        );

        let empty_appdata_res = resolve_windows_config_path(Some(""), Some("C:\\Users\\User"));
        assert_eq!(
            empty_appdata_res,
            Some(
                PathBuf::from("C:\\Users\\User")
                    .join(".config")
                    .join("terminalvision")
                    .join("config.tv")
            )
        );

        // 3. None when both are missing or empty
        assert_eq!(resolve_windows_config_path(None, None), None);
        assert_eq!(resolve_windows_config_path(Some(""), Some("")), None);
    }

    #[test]
    fn test_persistence_corruption_and_stress() {
        // 1. Empty string
        let empty = Settings::deserialize("");
        assert_eq!(empty.active_pane, ActivePaneConfig::Left);
        assert!(empty.bookmarks.is_empty());

        // 2. Truncated section headers and key-value lines
        let truncated = r#"
            [sett
            active_pane = 
            [tabs.le
            active = 1
            path = /incomplete/pa
            [bookmarks
            bookmark = Truncated
        "#;
        let s_trunc = Settings::deserialize(truncated);
        assert_eq!(s_trunc.active_pane, ActivePaneConfig::Left);

        // 3. Unexpected / out-of-range values
        let unexpected = r#"
            [settings]
            active_pane = 9999999999999999999999999999999999999999999999
            [tabs.left]
            active = 9999999999999999999999999999999999999999999999
            path = /nonexistent/path/that/does/not/exist
            [tabs.right]
            active = -42
            path = ""
            [bookmarks]
            bookmark = Invalid | 
        "#;
        let s_unexp = Settings::deserialize(unexpected);
        assert_eq!(s_unexp.active_pane, ActivePaneConfig::Left);
        assert_eq!(s_unexp.left_tabs.active_tab_index, 0); // overflow ignored
        assert_eq!(s_unexp.right_tabs.active_tab_index, 0); // negative ignored

        // 4. Unicode paths and names
        let unicode_cfg = r#"
            [bookmarks]
            bookmark = 🦀 Rustacean 文件夹 | /path/to/🦀_dir/тест
            bookmark = 日本語 | /home/ユーザー/ドキュメント
        "#;
        let s_uni = Settings::deserialize(unicode_cfg);
        assert_eq!(s_uni.bookmarks.len(), 2);
        assert_eq!(s_uni.bookmarks[0].name, "🦀 Rustacean 文件夹");
        assert_eq!(
            s_uni.bookmarks[0].path,
            PathBuf::from("/path/to/🦀_dir/тест")
        );

        // 5. Extremely long values (50,000 chars)
        let huge_val = "x".repeat(50_000);
        let huge_cfg = format!(
            "[settings]\nactive_pane = {}\n[bookmarks]\nbookmark = {} | /tmp/path\n",
            huge_val, huge_val
        );
        let s_huge = Settings::deserialize(&huge_cfg);
        assert_eq!(s_huge.active_pane, ActivePaneConfig::Left);
        assert_eq!(s_huge.bookmarks.len(), 1);
        assert_eq!(s_huge.bookmarks[0].name.len(), 50_000);
    }
}
