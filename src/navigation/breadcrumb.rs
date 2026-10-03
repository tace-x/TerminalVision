//! Smart Breadcrumb path hierarchy and navigation.
//!
//! Deconstructs absolute filesystem paths into navigable hierarchical segments,
//! provides responsive width-aware string formatting, and computes mouse hitboxes.

use std::path::{Path, PathBuf};

use crate::ui::display_width;

/// A single navigable segment in a breadcrumb path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreadcrumbSegment {
    /// Display label (e.g. "Home", "Projects", "src", "app").
    pub name: String,
    /// The full absolute path this segment represents.
    pub path: PathBuf,
    /// Whether this is the deepest / currently active directory.
    pub is_current: bool,
}

/// Structured hierarchy representation of a filesystem path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartBreadcrumb {
    /// Ordered segments from root/home down to active directory.
    pub segments: Vec<BreadcrumbSegment>,
    /// The full path.
    pub full_path: PathBuf,
}

impl SmartBreadcrumb {
    /// Builds a smart breadcrumb from `path`.
    pub fn from_path(path: &Path) -> Self {
        let home_dir = dirs_home();
        let mut segments = Vec::new();

        // Check if path is within home directory
        if let Some(ref home) = home_dir {
            if path == home {
                segments.push(BreadcrumbSegment {
                    name: "Home".to_string(),
                    path: home.clone(),
                    is_current: true,
                });
                return Self {
                    segments,
                    full_path: path.to_path_buf(),
                };
            } else if path.starts_with(home) {
                // Add Home as root segment
                segments.push(BreadcrumbSegment {
                    name: "Home".to_string(),
                    path: home.clone(),
                    is_current: false,
                });

                if let Ok(rel) = path.strip_prefix(home) {
                    let mut current_accum = home.clone();
                    let comps: Vec<_> = rel.components().collect();
                    let count = comps.len();
                    for (i, comp) in comps.into_iter().enumerate() {
                        let name_str = comp.as_os_str().to_string_lossy().to_string();
                        current_accum.push(&name_str);
                        segments.push(BreadcrumbSegment {
                            name: name_str,
                            path: current_accum.clone(),
                            is_current: i + 1 == count,
                        });
                    }
                }

                return Self {
                    segments,
                    full_path: path.to_path_buf(),
                };
            }
        }

        // Generic path (e.g. /etc/nginx or C:\Windows)
        let mut current_accum = PathBuf::new();
        let comps: Vec<_> = path.components().collect();
        let count = comps.len();

        for (i, comp) in comps.into_iter().enumerate() {
            let is_last = i + 1 == count;
            match comp {
                std::path::Component::RootDir => {
                    current_accum.push(std::path::MAIN_SEPARATOR.to_string());
                    segments.push(BreadcrumbSegment {
                        name: std::path::MAIN_SEPARATOR.to_string(),
                        path: current_accum.clone(),
                        is_current: is_last,
                    });
                }
                std::path::Component::Prefix(p) => {
                    let prefix_str = p.as_os_str().to_string_lossy().to_string();
                    current_accum.push(&prefix_str);
                    segments.push(BreadcrumbSegment {
                        name: prefix_str,
                        path: current_accum.clone(),
                        is_current: is_last,
                    });
                }
                std::path::Component::Normal(os_str) => {
                    let name_str = os_str.to_string_lossy().to_string();
                    current_accum.push(&name_str);
                    segments.push(BreadcrumbSegment {
                        name: name_str,
                        path: current_accum.clone(),
                        is_current: is_last,
                    });
                }
                _ => {}
            }
        }

        if segments.is_empty() {
            segments.push(BreadcrumbSegment {
                name: "/".to_string(),
                path: PathBuf::from("/"),
                is_current: true,
            });
        }

        Self {
            segments,
            full_path: path.to_path_buf(),
        }
    }

    /// Formats the breadcrumb string tailored to available terminal `max_width`.
    ///
    /// Wide: `"Home  /  Projects  /  TerminalVision  /  src  /  app"`
    /// Medium: `"Home / Projects / TerminalVision / src / app"`
    /// Narrow: `"~/Projects/TerminalVision/src/app"`
    pub fn format_for_width(&self, max_width: usize) -> String {
        if max_width == 0 {
            return String::new();
        }

        // Try wide spaced hierarchy
        let wide = self
            .segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join("  /  ");
        if display_width(&wide) <= max_width {
            return wide;
        }

        // Try standard hierarchy
        let medium = self
            .segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(" / ");
        if display_width(&medium) <= max_width {
            return medium;
        }

        // Try compact home-tilded representation
        let compact = self.compact_path_string();
        if display_width(&compact) <= max_width {
            return compact;
        }

        // Truncate from left with ellipsis
        crate::ui::truncate_path_to_width(&self.full_path, max_width)
    }

    /// Returns the compact path string (e.g. `~/Projects/app`).
    pub fn compact_path_string(&self) -> String {
        if let Some(home) = dirs_home() {
            if self.full_path == home {
                return "~".to_string();
            } else if let Ok(rel) = self.full_path.strip_prefix(&home) {
                return format!("~/{rel}", rel = rel.display());
            }
        }
        self.full_path.display().to_string()
    }
}

/// Helper to get user home directory cross-platform.
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breadcrumb_root() {
        let breadcrumb = SmartBreadcrumb::from_path(Path::new("/"));
        assert!(!breadcrumb.segments.is_empty());
    }

    #[test]
    fn test_breadcrumb_nested_path() {
        let path = Path::new("/var/log/nginx");
        let breadcrumb = SmartBreadcrumb::from_path(path);
        assert!(breadcrumb.segments.len() >= 3);
        assert_eq!(breadcrumb.segments.last().unwrap().name, "nginx");
        assert!(breadcrumb.segments.last().unwrap().is_current);
    }

    #[test]
    fn test_breadcrumb_format_widths() {
        let path = Path::new("/a/b/c/d/e");
        let breadcrumb = SmartBreadcrumb::from_path(path);
        let wide = breadcrumb.format_for_width(200);
        assert!(wide.contains('/'));

        let narrow = breadcrumb.format_for_width(10);
        assert!(display_width(&narrow) <= 10);
    }
}
