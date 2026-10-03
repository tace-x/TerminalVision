//! Lightweight Favorites and Quick Access system.
//!
//! Allows users to pin frequently visited locations, reorder them, rename labels,
//! and validate availability gracefully without crashing on deleted/renamed paths.

use std::path::{Path, PathBuf};

/// A pinned favorite location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteItem {
    /// Unique internal identifier.
    pub id: usize,
    /// User-visible display label (e.g. "Projects", "Downloads").
    pub name: String,
    /// Target filesystem path.
    pub path: PathBuf,
}

impl FavoriteItem {
    /// Creates a new favorite item.
    pub fn new(id: usize, name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            id,
            name: name.into(),
            path,
        }
    }

    /// Checks if the target path currently exists on the local filesystem.
    pub fn is_valid(&self) -> bool {
        self.path.exists()
    }

    /// Formatted display string with star badge or warning badge if missing.
    pub fn display_label(&self) -> String {
        if self.is_valid() {
            format!("★ {}", self.name)
        } else {
            format!("⚠ {} (missing)", self.name)
        }
    }
}

/// Collection of user favorites with selection and ordering controls.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FavoriteList {
    items: Vec<FavoriteItem>,
    next_id: usize,
    selected: usize,
}

impl FavoriteList {
    /// Creates an empty favorites list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the slice of favorite items.
    pub fn items(&self) -> &[FavoriteItem] {
        &self.items
    }

    /// Number of favorites.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether there are no favorites.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Selected favorite index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Currently selected favorite item.
    pub fn selected_item(&self) -> Option<&FavoriteItem> {
        self.items.get(self.selected)
    }

    /// Checks if `path` is already in favorites.
    pub fn contains_path(&self, path: &Path) -> bool {
        self.items.iter().any(|f| f.path == path)
    }

    /// Adds `path` as a favorite with derived name. Returns true if added.
    pub fn add(&mut self, path: PathBuf) -> bool {
        if self.contains_path(&path) {
            return false;
        }

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| path.to_str().unwrap_or("Directory"))
            .to_string();

        self.add_with_name(name, path)
    }

    /// Adds `path` with custom name. Returns true if added.
    pub fn add_with_name(&mut self, name: impl Into<String>, path: PathBuf) -> bool {
        if self.contains_path(&path) {
            return false;
        }

        let id = self.next_id;
        self.next_id += 1;
        self.items.push(FavoriteItem::new(id, name, path));
        self.clamp_selection();
        true
    }

    /// Removes the favorite item at `index`.
    pub fn remove_at(&mut self, index: usize) -> Option<FavoriteItem> {
        if index < self.items.len() {
            let item = self.items.remove(index);
            self.clamp_selection();
            Some(item)
        } else {
            None
        }
    }

    /// Removes favorite matching `path`.
    pub fn remove_by_path(&mut self, path: &Path) -> bool {
        if let Some(pos) = self.items.iter().position(|f| f.path == path) {
            self.items.remove(pos);
            self.clamp_selection();
            true
        } else {
            false
        }
    }

    /// Toggles favorite status for `path`. Returns true if added, false if removed.
    pub fn toggle(&mut self, path: PathBuf) -> bool {
        if self.contains_path(&path) {
            self.remove_by_path(&path);
            false
        } else {
            self.add(path);
            true
        }
    }

    /// Renames the favorite at `index`.
    pub fn rename(&mut self, index: usize, new_name: String) -> bool {
        if let Some(item) = self.items.get_mut(index) {
            item.name = new_name;
            true
        } else {
            false
        }
    }

    /// Moves the favorite at `index` up one position.
    pub fn move_up(&mut self, index: usize) -> bool {
        if index > 0 && index < self.items.len() {
            self.items.swap(index, index - 1);
            if self.selected == index {
                self.selected -= 1;
            }
            true
        } else {
            false
        }
    }

    /// Moves the favorite at `index` down one position.
    pub fn move_down(&mut self, index: usize) -> bool {
        if index + 1 < self.items.len() {
            self.items.swap(index, index + 1);
            if self.selected == index {
                self.selected += 1;
            }
            true
        } else {
            false
        }
    }

    /// Selects the previous favorite.
    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Selects the next favorite.
    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1).min(self.items.len() - 1);
        }
    }

    /// Clamps selection within valid bounds.
    pub fn clamp_selection(&mut self) {
        if self.items.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.items.len() {
            self.selected = self.items.len() - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_favorites_crud_and_ordering() {
        let mut list = FavoriteList::new();
        assert!(list.is_empty());

        assert!(list.add(PathBuf::from("/projects/a")));
        assert!(list.add(PathBuf::from("/projects/b")));
        assert!(
            !list.add(PathBuf::from("/projects/a")),
            "Duplicate must be rejected"
        );

        assert_eq!(list.len(), 2);
        assert_eq!(list.items()[0].name, "a");
        assert_eq!(list.items()[1].name, "b");

        list.move_down(0);
        assert_eq!(list.items()[0].name, "b");
        assert_eq!(list.items()[1].name, "a");

        list.rename(0, "Bravo".to_string());
        assert_eq!(list.items()[0].name, "Bravo");

        assert!(list.remove_by_path(Path::new("/projects/a")));
        assert_eq!(list.len(), 1);
    }
}
