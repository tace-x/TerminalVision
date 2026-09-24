//! Read-only Git repository status detection and metadata parsing.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::detector::{GitRepository, detect_repository};

/// The status of a file in a Git repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FileStatus {
    /// File is tracked and unchanged.
    #[default]
    Clean,
    /// File is tracked and has modifications.
    Modified,
    /// File is newly added to the index.
    Added,
    /// File is tracked in index but removed from working directory.
    Deleted,
    /// File is present in working directory but not tracked in index.
    Untracked,
    /// File was renamed.
    Renamed,
}

impl FileStatus {
    /// Short single-letter status code for UI display.
    pub fn code(self) -> &'static str {
        match self {
            Self::Clean => "",
            Self::Modified => "M",
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Untracked => "?",
            Self::Renamed => "R",
        }
    }
}

/// The current branch or HEAD state of a Git repository.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GitBranch {
    /// Branch name is unknown or missing.
    #[default]
    Unknown,
    /// On a named branch (e.g., `main`, `feature/foo`).
    Branch(String),
    /// In detached HEAD state (e.g., commit SHA or tag name).
    Detached(String),
}

impl GitBranch {
    /// Returns the branch name if on a named branch.
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Branch(name) => Some(name.as_str()),
            _ => None,
        }
    }

    /// User-facing display representation.
    pub fn display(&self) -> String {
        match self {
            Self::Branch(name) => name.clone(),
            Self::Detached(ref_or_sha) => format!("detached at {ref_or_sha}"),
            Self::Unknown => "HEAD".to_string(),
        }
    }
}

/// Consolidated Git status for a repository/directory view.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitStatus {
    /// Repository detection info.
    pub repository: GitRepository,
    /// Current branch or detached state.
    pub branch: GitBranch,
    /// Map of relative path from repository root to its status.
    pub file_statuses: HashMap<PathBuf, FileStatus>,
    /// Whether the repository working tree is completely clean.
    pub is_clean: bool,
    /// Count of modified files.
    pub modified_count: usize,
    /// Count of added files.
    pub added_count: usize,
    /// Count of deleted files.
    pub deleted_count: usize,
    /// Count of untracked files.
    pub untracked_count: usize,
    /// Count of renamed files.
    pub renamed_count: usize,
}

impl GitStatus {
    /// Creates an empty/no-repository status.
    pub fn none() -> Self {
        Self {
            repository: GitRepository::None,
            branch: GitBranch::Unknown,
            file_statuses: HashMap::new(),
            is_clean: true,
            modified_count: 0,
            added_count: 0,
            deleted_count: 0,
            untracked_count: 0,
            renamed_count: 0,
        }
    }

    /// Whether this status represents an active Git repository.
    pub fn is_repo(&self) -> bool {
        self.repository.is_repo()
    }

    /// The repository root directory path if detected.
    pub fn root(&self) -> Option<&Path> {
        self.repository.root()
    }

    /// Returns the status for a path (relative or absolute).
    pub fn file_status_for(&self, path: &Path) -> Option<FileStatus> {
        if !self.is_repo() {
            return None;
        }

        let rel_path = if let Some(root) = self.repository.root() {
            if path.is_absolute() {
                path.strip_prefix(root).ok()?.to_path_buf()
            } else {
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        };

        self.file_statuses.get(&rel_path).copied()
    }

    /// Formats a compact summary string for UI header (e.g. `[git: main | +1 ~2 -0 ?1]`).
    pub fn summary_string(&self) -> Option<String> {
        let repo_name = self.repository.repo_name()?;
        let branch_disp = self.branch.display();

        if self.is_clean {
            Some(format!("[git: {repo_name} ({branch_disp}) | clean]"))
        } else {
            let mut parts = Vec::new();
            if self.added_count > 0 {
                parts.push(format!("+{}", self.added_count));
            }
            if self.modified_count > 0 {
                parts.push(format!("~{}", self.modified_count));
            }
            if self.deleted_count > 0 {
                parts.push(format!("-{}", self.deleted_count));
            }
            if self.renamed_count > 0 {
                parts.push(format!("R{}", self.renamed_count));
            }
            if self.untracked_count > 0 {
                parts.push(format!("?{}", self.untracked_count));
            }
            let diff_str = parts.join(" ");
            Some(format!("[git: {repo_name} ({branch_disp}) | {diff_str}]"))
        }
    }
}

/// Parsed entry from Git `.git/index`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub rel_path: PathBuf,
    pub mtime_sec: u32,
    pub mtime_nsec: u32,
    pub file_size: u32,
    pub sha1: [u8; 20],
    pub stage: u8,
    pub intent_to_add: bool,
}

/// Resolves the actual `.git` directory path for a repository root.
pub fn resolve_git_dir(root: &Path, is_worktree: bool) -> Option<PathBuf> {
    let git_path = root.join(".git");
    if !is_worktree {
        if git_path.is_dir() {
            return Some(git_path);
        }
        return None;
    }

    // Worktree / submodule: .git is a text file containing "gitdir: <path>"
    let content = fs::read_to_string(&git_path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(target) = trimmed.strip_prefix("gitdir:") {
            let target_trimmed = target.trim();
            let target_path = Path::new(target_trimmed);
            let resolved = if target_path.is_relative() {
                root.join(target_path)
            } else {
                target_path.to_path_buf()
            };
            if resolved.exists() {
                return Some(resolved);
            }
        }
    }
    None
}

/// Reads the Git branch or HEAD state from `<git_dir>/HEAD`.
pub fn read_branch(git_dir: &Path) -> GitBranch {
    let head_path = git_dir.join("HEAD");
    let content = match fs::read_to_string(&head_path) {
        Ok(c) => c,
        Err(_) => return GitBranch::Unknown,
    };

    let line = content.lines().next().unwrap_or("").trim();
    if let Some(ref_path) = line.strip_prefix("ref:") {
        let ref_trimmed = ref_path.trim();
        if let Some(branch_name) = ref_trimmed.strip_prefix("refs/heads/") {
            GitBranch::Branch(branch_name.to_string())
        } else {
            GitBranch::Detached(ref_trimmed.to_string())
        }
    } else if line.len() >= 7 && line.chars().all(|c| c.is_ascii_hexdigit()) {
        let sha_short = if line.len() >= 7 { &line[..7] } else { line };
        GitBranch::Detached(sha_short.to_string())
    } else {
        GitBranch::Unknown
    }
}

/// Parses a `.git/index` binary file into a list of [`IndexEntry`] structs.
/// Supports Git index formats v2, v3, and v4. Returns empty Vec on malformed input.
pub fn parse_index(git_dir: &Path) -> Vec<IndexEntry> {
    let index_path = git_dir.join("index");
    let bytes = match fs::read(&index_path) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };

    if bytes.len() < 12 {
        return Vec::new();
    }

    // Header: 4b "DIRC", 4b version, 4b entry count
    if &bytes[0..4] != b"DIRC" {
        return Vec::new();
    }

    let version = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
    if !(2..=4).contains(&version) {
        return Vec::new();
    }

    let count = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let mut offset = 12;
    let mut entries = Vec::with_capacity(count);
    let mut prev_path_bytes = Vec::<u8>::new();

    for _ in 0..count {
        if offset + 62 > bytes.len() {
            break;
        }

        let mtime_sec = u32::from_be_bytes(bytes[offset + 8..offset + 12].try_into().unwrap());
        let mtime_nsec = u32::from_be_bytes(bytes[offset + 12..offset + 16].try_into().unwrap());
        let file_size = u32::from_be_bytes(bytes[offset + 36..offset + 40].try_into().unwrap());
        let mut sha1 = [0u8; 20];
        sha1.copy_from_slice(&bytes[offset + 40..offset + 60]);

        let flags = u16::from_be_bytes(bytes[offset + 60..offset + 62].try_into().unwrap());
        let extended_flag = (flags & 0x4000) != 0;
        let stage = ((flags >> 12) & 0x03) as u8;

        let mut header_len = 62;
        let mut intent_to_add = false;

        if extended_flag && version >= 3 {
            if offset + 64 > bytes.len() {
                break;
            }
            let extra_flags =
                u16::from_be_bytes(bytes[offset + 62..offset + 64].try_into().unwrap());
            intent_to_add = (extra_flags & 0x2000) != 0;
            header_len = 64;
        }

        offset += header_len;

        let path_bytes = if version == 4 {
            // Varint prefix_len
            let mut prefix_len: usize = 0;
            loop {
                if offset >= bytes.len() {
                    break;
                }
                let b = bytes[offset];
                offset += 1;
                prefix_len = (prefix_len << 7) | ((b & 0x7F) as usize);
                if (b & 0x80) == 0 {
                    break;
                }
            }

            let start = offset;
            while offset < bytes.len() && bytes[offset] != 0 {
                offset += 1;
            }
            let suffix = &bytes[start..offset];
            if offset < bytes.len() {
                offset += 1; // skip NUL
            }

            let prefix = if prefix_len <= prev_path_bytes.len() {
                &prev_path_bytes[..prefix_len]
            } else {
                &[]
            };
            let mut full = Vec::with_capacity(prefix.len() + suffix.len());
            full.extend_from_slice(prefix);
            full.extend_from_slice(suffix);
            full
        } else {
            // v2 / v3 format: path terminated by NUL, padded to 8-byte alignment
            let start = offset;
            while offset < bytes.len() && bytes[offset] != 0 {
                offset += 1;
            }
            let path_slice = &bytes[start..offset];

            // Total entry length (header_len + path_len + NUL) padded to multiple of 8
            let name_len = offset - start;
            let unpadded_len = header_len + name_len + 1; // +1 for NUL byte
            let pad = (8 - (unpadded_len % 8)) % 8;
            offset += 1 + pad;

            path_slice.to_vec()
        };

        prev_path_bytes = path_bytes.clone();
        let path_str = String::from_utf8_lossy(&path_bytes).to_string();
        let rel_path: PathBuf = path_str.split('/').collect();
        entries.push(IndexEntry {
            rel_path,
            mtime_sec,
            mtime_nsec,
            file_size,
            sha1,
            stage,
            intent_to_add,
        });
    }

    entries
}

/// Simple `.gitignore` line patterns matcher.
#[derive(Debug, Clone)]
pub struct IgnoreMatcher {
    patterns: Vec<String>,
}

impl IgnoreMatcher {
    /// Loads ignore rules from repository root `.gitignore` and `.git/info/exclude`.
    pub fn load(root: &Path, git_dir: Option<&Path>) -> Self {
        let mut patterns = vec![".git".to_string()];

        let root_ignore = root.join(".gitignore");
        if let Ok(content) = fs::read_to_string(root_ignore) {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    patterns.push(trimmed.to_string());
                }
            }
        }

        if let Some(gd) = git_dir {
            let exclude = gd.join("info").join("exclude");
            if let Ok(content) = fs::read_to_string(exclude) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        patterns.push(trimmed.to_string());
                    }
                }
            }
        }

        Self { patterns }
    }

    /// Checks if a relative path or filename matches ignore rules.
    pub fn is_ignored(&self, rel_path: &Path, is_dir: bool) -> bool {
        let file_name = rel_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        for pat in &self.patterns {
            let clean_pat = pat.trim_end_matches('/');

            if let Some(suffix) = clean_pat.strip_prefix('*') {
                if file_name.ends_with(suffix) {
                    return true;
                }
            } else {
                let pat_path: PathBuf = clean_pat.split('/').collect();
                if clean_pat == file_name
                    || rel_path == pat_path
                    || (is_dir && rel_path.starts_with(&pat_path))
                {
                    return true;
                }
            }
        }
        false
    }
}

/// Computes the [`GitStatus`] for a given target path (file or directory).
pub fn compute_status(target_path: &Path) -> GitStatus {
    let repository = detect_repository(target_path);
    let root = match repository.root() {
        Some(r) => r.to_path_buf(),
        None => return GitStatus::none(),
    };

    let git_dir = resolve_git_dir(&root, repository.is_worktree());
    let branch = git_dir
        .as_ref()
        .map(|gd| read_branch(gd))
        .unwrap_or(GitBranch::Unknown);

    let mut file_statuses = HashMap::new();
    let index_entries = git_dir
        .as_ref()
        .map(|gd| parse_index(gd))
        .unwrap_or_default();

    let ignore_matcher = IgnoreMatcher::load(&root, git_dir.as_deref());

    let mut tracked_paths = HashSet::new();

    // 1. Check index entries vs filesystem
    for entry in &index_entries {
        tracked_paths.insert(entry.rel_path.clone());
        let full_path = root.join(&entry.rel_path);

        match fs::symlink_metadata(&full_path) {
            Err(_) => {
                file_statuses.insert(entry.rel_path.clone(), FileStatus::Deleted);
            }
            Ok(meta) => {
                if entry.intent_to_add {
                    file_statuses.insert(entry.rel_path.clone(), FileStatus::Added);
                } else if entry.stage > 0 {
                    file_statuses.insert(entry.rel_path.clone(), FileStatus::Modified);
                } else {
                    let disk_size = meta.len() as u32;
                    let mtime_sec = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as u32)
                        .unwrap_or(0);

                    // Check if file size or mtime differs from index
                    let is_modified = (entry.file_size > 0 && disk_size != entry.file_size)
                        || (entry.mtime_sec > 0 && mtime_sec != 0 && mtime_sec != entry.file_size);

                    if is_modified {
                        file_statuses.insert(entry.rel_path.clone(), FileStatus::Modified);
                    } else {
                        file_statuses.insert(entry.rel_path.clone(), FileStatus::Clean);
                    }
                }
            }
        }
    }

    // 2. Discover untracked files in the target_path directory
    let scan_dir = if target_path.is_dir() {
        target_path
    } else {
        target_path.parent().unwrap_or(&root)
    };

    if let Ok(dir_entries) = fs::read_dir(scan_dir) {
        for item in dir_entries.flatten() {
            let child_path = item.path();
            if let Ok(rel) = child_path.strip_prefix(&root) {
                if rel.components().next().is_none() {
                    continue;
                }

                let is_dir = child_path.is_dir();
                if !tracked_paths.contains(rel) && !ignore_matcher.is_ignored(rel, is_dir) {
                    file_statuses.insert(rel.to_path_buf(), FileStatus::Untracked);
                }
            }
        }
    }

    // 3. Simple Rename Detection heuristic (Matching Deleted + Untracked/Added with identical file_name or size)
    let deleted_paths: Vec<PathBuf> = file_statuses
        .iter()
        .filter(|(_, status)| **status == FileStatus::Deleted)
        .map(|(path, _)| path.clone())
        .collect();

    let candidate_new_paths: Vec<PathBuf> = file_statuses
        .iter()
        .filter(|(_, status)| **status == FileStatus::Untracked || **status == FileStatus::Added)
        .map(|(path, _)| path.clone())
        .collect();

    for del in deleted_paths {
        if let Some(del_name) = del.file_name() {
            let renamed_match = candidate_new_paths
                .iter()
                .find(|cand| cand.file_name() == Some(del_name));
            if let Some(cand) = renamed_match {
                file_statuses.insert(cand.clone(), FileStatus::Renamed);
            }
        }
    }

    // Calculate aggregate counts
    let mut modified_count = 0;
    let mut added_count = 0;
    let mut deleted_count = 0;
    let mut untracked_count = 0;
    let mut renamed_count = 0;

    for status in file_statuses.values() {
        match status {
            FileStatus::Modified => modified_count += 1,
            FileStatus::Added => added_count += 1,
            FileStatus::Deleted => deleted_count += 1,
            FileStatus::Untracked => untracked_count += 1,
            FileStatus::Renamed => renamed_count += 1,
            FileStatus::Clean => {}
        }
    }

    let is_clean = modified_count == 0
        && added_count == 0
        && deleted_count == 0
        && untracked_count == 0
        && renamed_count == 0;

    GitStatus {
        repository,
        branch,
        file_statuses,
        is_clean,
        modified_count,
        added_count,
        deleted_count,
        untracked_count,
        renamed_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;

    /// Helper to craft binary index bytes for index format v2.
    fn create_v2_index_bytes(entries: &[(&str, u32, u32)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"DIRC");
        bytes.extend_from_slice(&2u32.to_be_bytes()); // version 2
        bytes.extend_from_slice(&(entries.len() as u32).to_be_bytes()); // count

        for (path, file_size, mtime_sec) in entries {
            bytes.extend_from_slice(&0u32.to_be_bytes()); // ctime sec
            bytes.extend_from_slice(&0u32.to_be_bytes()); // ctime nsec
            bytes.extend_from_slice(&mtime_sec.to_be_bytes()); // mtime sec
            bytes.extend_from_slice(&0u32.to_be_bytes()); // mtime nsec
            bytes.extend_from_slice(&0u32.to_be_bytes()); // dev
            bytes.extend_from_slice(&0u32.to_be_bytes()); // ino
            bytes.extend_from_slice(&0o100644u32.to_be_bytes()); // mode
            bytes.extend_from_slice(&0u32.to_be_bytes()); // uid
            bytes.extend_from_slice(&0u32.to_be_bytes()); // gid
            bytes.extend_from_slice(&file_size.to_be_bytes()); // file size
            bytes.extend_from_slice(&[0u8; 20]); // sha1

            let name_len = (path.len() & 0x0FFF) as u16;
            bytes.extend_from_slice(&name_len.to_be_bytes()); // flags

            bytes.extend_from_slice(path.as_bytes());

            // Padding to 8-byte alignment (fixed header = 62)
            let unpadded_len = 62 + path.len() + 1;
            let pad = (8 - (unpadded_len % 8)) % 8;
            bytes.resize(bytes.len() + 1 + pad, 0);
        }

        bytes
    }

    #[test]
    fn test_clean_repository() {
        let temp = TempDir::new("git-clean-test");
        let root = temp.path();
        let git_dir = root.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        let file1 = root.join("hello.txt");
        fs::write(&file1, "hello world").unwrap();

        let index_bytes = create_v2_index_bytes(&[("hello.txt", 11, 0)]);
        fs::write(git_dir.join("index"), index_bytes).unwrap();

        let status = compute_status(root);
        assert!(status.is_repo());
        assert_eq!(status.branch, GitBranch::Branch("main".to_string()));
        assert!(status.is_clean);
        assert_eq!(
            status.file_status_for(Path::new("hello.txt")),
            Some(FileStatus::Clean)
        );
    }

    #[test]
    fn test_modified_file() {
        let temp = TempDir::new("git-mod-test");
        let root = temp.path();
        let git_dir = root.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        let file1 = root.join("hello.txt");
        fs::write(&file1, "modified content extended").unwrap(); // size 24

        let index_bytes = create_v2_index_bytes(&[("hello.txt", 11, 0)]); // index recorded size 11
        fs::write(git_dir.join("index"), index_bytes).unwrap();

        let status = compute_status(root);
        assert!(!status.is_clean);
        assert_eq!(status.modified_count, 1);
        assert_eq!(
            status.file_status_for(Path::new("hello.txt")),
            Some(FileStatus::Modified)
        );
    }

    #[test]
    fn test_untracked_file() {
        let temp = TempDir::new("git-untracked-test");
        let root = temp.path();
        let git_dir = root.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        let untracked = root.join("newfile.txt");
        fs::write(&untracked, "new").unwrap();

        let status = compute_status(root);
        assert!(!status.is_clean);
        assert_eq!(status.untracked_count, 1);
        assert_eq!(
            status.file_status_for(Path::new("newfile.txt")),
            Some(FileStatus::Untracked)
        );
    }

    #[test]
    fn test_deleted_file() {
        let temp = TempDir::new("git-deleted-test");
        let root = temp.path();
        let git_dir = root.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        // Index lists deleted.txt, but it doesn't exist on disk
        let index_bytes = create_v2_index_bytes(&[("deleted.txt", 10, 0)]);
        fs::write(git_dir.join("index"), index_bytes).unwrap();

        let status = compute_status(root);
        assert!(!status.is_clean);
        assert_eq!(status.deleted_count, 1);
        assert_eq!(
            status.file_status_for(Path::new("deleted.txt")),
            Some(FileStatus::Deleted)
        );
    }

    #[test]
    fn test_renamed_file() {
        let temp = TempDir::new("git-rename-test");
        let root = temp.path();
        let git_dir = root.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        // Index lists old/foo.txt (deleted on disk), sub/foo.txt created on disk
        let index_bytes = create_v2_index_bytes(&[("old/foo.txt", 10, 0)]);
        fs::write(git_dir.join("index"), index_bytes).unwrap();

        let sub_dir = root.join("sub");
        fs::create_dir_all(&sub_dir).unwrap();
        fs::write(sub_dir.join("foo.txt"), "data").unwrap();

        let status = compute_status(&sub_dir);
        assert_eq!(
            status.file_status_for(Path::new("sub/foo.txt")),
            Some(FileStatus::Renamed)
        );
    }

    #[test]
    fn test_detached_head() {
        let temp = TempDir::new("git-detached-test");
        let root = temp.path();
        let git_dir = root.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(
            git_dir.join("HEAD"),
            "a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4\n",
        )
        .unwrap();

        let status = compute_status(root);
        assert_eq!(status.branch, GitBranch::Detached("a1b2c3d".to_string()));
    }

    #[test]
    fn test_no_repository() {
        let temp = TempDir::new("no-git-status");
        let status = compute_status(temp.path());
        assert!(!status.is_repo());
        assert_eq!(status, GitStatus::none());
    }

    #[test]
    fn test_malformed_metadata() {
        let temp = TempDir::new("git-malformed-test");
        let root = temp.path();
        let git_dir = root.join(".git");
        fs::create_dir_all(&git_dir).unwrap();

        // Malformed index file with junk bytes
        fs::write(git_dir.join("index"), b"CORRUPT_HEADER_BYTES").unwrap();
        fs::write(git_dir.join("HEAD"), "").unwrap();

        let status = compute_status(root);
        assert!(status.is_repo());
        assert_eq!(status.branch, GitBranch::Unknown);
    }

    #[test]
    fn test_worktree_git_file() {
        let temp = TempDir::new("git-worktree-status");
        let root = temp.path();
        let target_git = temp.path().join("real_git_dir");
        fs::create_dir_all(&target_git).unwrap();
        fs::write(
            target_git.join("HEAD"),
            "ref: refs/heads/feature/worktree\n",
        )
        .unwrap();

        fs::write(
            root.join(".git"),
            format!("gitdir: {}\n", target_git.display()),
        )
        .unwrap();

        let status = compute_status(root);
        assert!(status.is_repo());
        assert_eq!(
            status.branch,
            GitBranch::Branch("feature/worktree".to_string())
        );
    }

    #[test]
    fn test_unicode_and_spaces_paths() {
        let temp = TempDir::new("git-unicode-spaces");
        let root = temp.path().join("📁 Repo with spaces");
        let git_dir = root.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        let unicode_file = root.join("日本語 file 🦀.txt");
        fs::write(&unicode_file, "content").unwrap();

        let status = compute_status(&root);
        assert!(status.is_repo());
        assert_eq!(
            status.file_status_for(Path::new("日本語 file 🦀.txt")),
            Some(FileStatus::Untracked)
        );
    }

    #[test]
    fn test_nested_directory_status() {
        let temp = TempDir::new("git-nested-dir");
        let root = temp.path();
        let git_dir = root.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        let nested = root.join("src").join("deep").join("folder");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("deep_file.txt"), "hello").unwrap();

        let status = compute_status(&nested);
        assert!(status.is_repo());
        assert_eq!(status.root(), Some(root));
        assert_eq!(
            status.file_status_for(Path::new("src/deep/folder/deep_file.txt")),
            Some(FileStatus::Untracked)
        );
    }

    #[test]
    fn test_multiple_status_types_simultaneously() {
        let temp = TempDir::new("git-multiple-status");
        let root = temp.path();
        let git_dir = root.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        // 1. Clean file: tracked, exists on disk, size matches
        fs::write(root.join("clean.txt"), "clean data").unwrap();

        // 2. Modified file: recorded size 10 in index, disk size 15
        fs::write(root.join("modified.txt"), "modified 15 byte").unwrap();

        // Index records clean.txt (10 bytes) and modified.txt (10 bytes) and deleted.txt (10 bytes)
        let index_bytes = create_v2_index_bytes(&[
            ("clean.txt", 10, 0),
            ("modified.txt", 10, 0),
            ("deleted.txt", 10, 0),
        ]);
        fs::write(git_dir.join("index"), index_bytes).unwrap();

        // 3. Untracked file
        fs::write(root.join("untracked.txt"), "untracked").unwrap();

        let status = compute_status(root);
        assert!(!status.is_clean);
        assert_eq!(status.modified_count, 1);
        assert_eq!(status.deleted_count, 1);
        assert_eq!(status.untracked_count, 1);
        assert!(status.summary_string().unwrap().contains("~1"));
        assert!(status.summary_string().unwrap().contains("-1"));
        assert!(status.summary_string().unwrap().contains("?1"));
    }
}
