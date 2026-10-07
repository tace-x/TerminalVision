//! Lightweight, thread-safe, and bounded in-memory cache for Project Intelligence and Workspace Graph.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use super::detector::ProjectDetector;
use super::fingerprint::ProjectFingerprint;
use super::graph::WorkspaceAnalyzer;
use super::workspace::WorkspaceContext;

/// Default maximum number of cached project entries.
const DEFAULT_MAX_ENTRIES: usize = 256;

/// Default time-to-live for a cached project fingerprint or workspace graph (5 seconds).
const DEFAULT_TTL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct CacheEntry<T> {
    data: T,
    inserted_at: Instant,
}

/// In-memory bounded cache for [`ProjectFingerprint`] and [`WorkspaceContext`] entries.
#[derive(Debug, Clone)]
pub struct ProjectCache {
    fingerprints: HashMap<PathBuf, CacheEntry<ProjectFingerprint>>,
    workspaces: HashMap<PathBuf, CacheEntry<WorkspaceContext>>,
    ttl: Duration,
    max_entries: usize,
}

impl Default for ProjectCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectCache {
    /// Creates a new `ProjectCache` with default capacity and TTL.
    pub fn new() -> Self {
        Self {
            fingerprints: HashMap::new(),
            workspaces: HashMap::new(),
            ttl: DEFAULT_TTL,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }

    /// Creates a new `ProjectCache` with custom TTL.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            fingerprints: HashMap::new(),
            workspaces: HashMap::new(),
            ttl,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }

    /// Creates a new `ProjectCache` with custom TTL and max entries.
    pub fn with_ttl_and_capacity(ttl: Duration, max_entries: usize) -> Self {
        Self {
            fingerprints: HashMap::new(),
            workspaces: HashMap::new(),
            ttl,
            max_entries,
        }
    }

    /// Normalizes path for canonical map keying.
    fn normalize_path(path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }

    // --- Fingerprint Operations ---

    /// Retrieves a cached [`ProjectFingerprint`] if present and not expired.
    pub fn get(&self, path: &Path) -> Option<ProjectFingerprint> {
        let key = Self::normalize_path(path);
        if let Some(entry) = self.fingerprints.get(&key)
            && entry.inserted_at.elapsed() < self.ttl
        {
            return Some(entry.data.clone());
        }
        None
    }

    /// Stores a fingerprint in the cache for the given path.
    pub fn insert(&mut self, path: PathBuf, fingerprint: ProjectFingerprint) {
        let key = Self::normalize_path(&path);

        if self.fingerprints.len() >= self.max_entries {
            self.prune_expired();
            if self.fingerprints.len() >= self.max_entries {
                self.fingerprints.clear();
            }
        }

        self.fingerprints.insert(
            key,
            CacheEntry {
                data: fingerprint,
                inserted_at: Instant::now(),
            },
        );
    }

    /// Fetches the cached fingerprint or performs detection and caches the result.
    pub fn get_or_detect(&mut self, path: &Path) -> ProjectFingerprint {
        if let Some(cached) = self.get(path) {
            return cached;
        }

        let detected = ProjectDetector::detect(path);
        self.insert(path.to_path_buf(), detected.clone());
        detected
    }

    // --- Workspace Graph Operations ---

    /// Retrieves a cached [`WorkspaceContext`] if present and not expired.
    pub fn get_workspace(&self, path: &Path) -> Option<WorkspaceContext> {
        let key = Self::normalize_path(path);
        if let Some(entry) = self.workspaces.get(&key)
            && entry.inserted_at.elapsed() < self.ttl
        {
            return Some(entry.data.clone());
        }
        None
    }

    /// Stores a workspace context in the cache for the given path.
    pub fn insert_workspace(&mut self, path: PathBuf, workspace: WorkspaceContext) {
        let key = Self::normalize_path(&path);

        if self.workspaces.len() >= self.max_entries {
            self.prune_expired();
            if self.workspaces.len() >= self.max_entries {
                self.workspaces.clear();
            }
        }

        self.workspaces.insert(
            key,
            CacheEntry {
                data: workspace,
                inserted_at: Instant::now(),
            },
        );
    }

    /// Fetches the cached workspace context or performs analysis and caches the result.
    pub fn get_or_analyze_workspace(&mut self, path: &Path) -> WorkspaceContext {
        if let Some(cached) = self.get_workspace(path) {
            return cached;
        }

        let analyzed = WorkspaceAnalyzer::analyze(path);
        self.insert_workspace(path.to_path_buf(), analyzed.clone());
        analyzed
    }

    // --- Maintenance & Invalidation ---

    /// Evicts expired entries.
    pub fn prune_expired(&mut self) {
        let ttl = self.ttl;
        self.fingerprints
            .retain(|_, entry| entry.inserted_at.elapsed() < ttl);
        self.workspaces
            .retain(|_, entry| entry.inserted_at.elapsed() < ttl);
    }

    /// Invalidates a specific cached path.
    pub fn invalidate(&mut self, path: &Path) {
        let key = Self::normalize_path(path);
        self.fingerprints.remove(&key);
        self.workspaces.remove(&key);
    }

    /// Invalidates all cached entries whose root or path starts with `root`.
    pub fn invalidate_root(&mut self, root: &Path) {
        let norm_root = Self::normalize_path(root);

        self.fingerprints.retain(|key, entry| {
            if key.starts_with(&norm_root) {
                return false;
            }
            if let Some(ref r) = entry.data.root {
                let norm_entry_root = Self::normalize_path(r);
                if norm_entry_root == norm_root || norm_entry_root.starts_with(&norm_root) {
                    return false;
                }
            }
            true
        });

        self.workspaces.retain(|key, entry| {
            if key.starts_with(&norm_root) {
                return false;
            }
            if let Some(ref r) = entry.data.root {
                let norm_entry_root = Self::normalize_path(r);
                if norm_entry_root == norm_root || norm_entry_root.starts_with(&norm_root) {
                    return false;
                }
            }
            true
        });
    }

    /// Clears the entire cache.
    pub fn clear(&mut self) {
        self.fingerprints.clear();
        self.workspaces.clear();
    }

    /// Number of fingerprint entries currently stored in cache.
    pub fn len(&self) -> usize {
        self.fingerprints.len()
    }

    /// Whether the cache is currently empty.
    pub fn is_empty(&self) -> bool {
        self.fingerprints.is_empty() && self.workspaces.is_empty()
    }
}

/// Thread-safe wrapper around [`ProjectCache`].
#[derive(Debug, Clone, Default)]
pub struct SharedProjectCache {
    inner: Arc<RwLock<ProjectCache>>,
}

impl SharedProjectCache {
    /// Creates a new `SharedProjectCache`.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(ProjectCache::new())),
        }
    }

    /// Retrieves a cached [`ProjectFingerprint`] if present and not expired.
    pub fn get(&self, path: &Path) -> Option<ProjectFingerprint> {
        self.inner.read().ok()?.get(path)
    }

    /// Stores a fingerprint in the cache for the given path.
    pub fn insert(&self, path: PathBuf, fingerprint: ProjectFingerprint) {
        if let Ok(mut lock) = self.inner.write() {
            lock.insert(path, fingerprint);
        }
    }

    /// Retrieves a cached [`WorkspaceContext`] if present and not expired.
    pub fn get_workspace(&self, path: &Path) -> Option<WorkspaceContext> {
        self.inner.read().ok()?.get_workspace(path)
    }

    /// Stores a workspace context in the cache for the given path.
    pub fn insert_workspace(&self, path: PathBuf, workspace: WorkspaceContext) {
        if let Ok(mut lock) = self.inner.write() {
            lock.insert_workspace(path, workspace);
        }
    }

    /// Invalidates a specific cached path.
    pub fn invalidate(&self, path: &Path) {
        if let Ok(mut lock) = self.inner.write() {
            lock.invalidate(path);
        }
    }

    /// Invalidates all cached entries for a root.
    pub fn invalidate_root(&self, root: &Path) {
        if let Ok(mut lock) = self.inner.write() {
            lock.invalidate_root(root);
        }
    }

    /// Clears the entire cache.
    pub fn clear(&self) {
        if let Ok(mut lock) = self.inner.write() {
            lock.clear();
        }
    }

    /// Fetches the cached fingerprint or performs detection and caches the result.
    pub fn get_or_detect(&self, path: &Path) -> ProjectFingerprint {
        if let Some(cached) = self.get(path) {
            return cached;
        }

        let detected = ProjectDetector::detect(path);
        self.insert(path.to_path_buf(), detected.clone());
        detected
    }

    /// Fetches the cached workspace context or performs analysis and caches the result.
    pub fn get_or_analyze_workspace(&self, path: &Path) -> WorkspaceContext {
        if let Some(cached) = self.get_workspace(path) {
            return cached;
        }

        let analyzed = WorkspaceAnalyzer::analyze(path);
        self.insert_workspace(path.to_path_buf(), analyzed.clone());
        analyzed
    }
}
