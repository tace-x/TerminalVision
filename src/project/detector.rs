//! Safe, read-only, and deterministic project root discovery and fingerprint extraction.

use std::fs;
use std::path::{Path, PathBuf};

use super::fingerprint::ProjectFingerprint;
use super::signals::ProjectSignals;
use super::types::{BuildSystem, DetectionConfidence, Language, ProjectType};
use crate::git::detector::detect_repository;

/// Maximum upward directory traversal hops before aborting to avoid potential loops.
const MAX_UPWARD_HOPS: usize = 32;

/// Maximum number of directory entries inspected at root level to guarantee bounded latency.
const MAX_ROOT_SCAN_ENTRIES: usize = 128;

/// Detector engine for discovering project boundaries and extracting fingerprints.
pub struct ProjectDetector;

impl ProjectDetector {
    /// Detects the project root directory starting from any path (file or directory).
    ///
    /// The algorithm traverses safely upwards from `start_path` looking for:
    /// 1. Closest primary manifest (`Cargo.toml`, `package.json`, `go.mod`, `pom.xml`, etc.)
    /// 2. Closest secondary manifest (`Makefile`, `requirements.txt`, etc.)
    /// 3. Git repository root (`.git` directory or worktree file)
    /// 4. Generic project markers (`README.md`, `LICENSE`)
    pub fn find_project_root(start_path: &Path) -> Option<PathBuf> {
        if start_path.as_os_str().is_empty() {
            return None;
        }

        // Clean and prepare initial directory
        let initial_dir = if start_path.is_file() {
            start_path.parent()?
        } else if start_path.is_dir() {
            start_path
        } else {
            // Path does not exist or cannot be accessed
            return None;
        };

        let mut current = initial_dir.to_path_buf();
        let mut closest_primary_manifest_root: Option<PathBuf> = None;
        let mut closest_secondary_manifest_root: Option<PathBuf> = None;
        let mut git_root: Option<PathBuf> = None;
        let mut generic_root: Option<PathBuf> = None;

        let mut hops = 0;
        loop {
            // 1. Check strong / primary manifests
            if has_primary_manifest(&current) && closest_primary_manifest_root.is_none() {
                closest_primary_manifest_root = Some(current.clone());
            }

            // 2. Check secondary manifests
            if has_secondary_manifest(&current) && closest_secondary_manifest_root.is_none() {
                closest_secondary_manifest_root = Some(current.clone());
            }

            // 3. Check git repository marker at this level
            if git_root.is_none() && has_git_marker(&current) {
                git_root = Some(current.clone());
            }

            // 4. Check generic markers
            if generic_root.is_none() && has_generic_markers(&current) {
                generic_root = Some(current.clone());
            }

            // If we found a primary manifest and git root, we can stop
            if closest_primary_manifest_root.is_some() && git_root.is_some() {
                break;
            }

            if hops >= MAX_UPWARD_HOPS || !current.pop() {
                break;
            }
            hops += 1;
        }

        // Resolution precedence:
        // 1. Primary manifest root (closest to start path)
        if let Some(pmr) = closest_primary_manifest_root {
            return Some(pmr);
        }

        // 2. Secondary manifest root
        if let Some(smr) = closest_secondary_manifest_root {
            return Some(smr);
        }

        // 3. Git repository root
        if let Some(gr) = git_root {
            return Some(gr);
        }

        // Fall back to git detection via detector if above loop stopped early
        let repo = detect_repository(initial_dir);
        if let Some(repo_root) = repo.root() {
            return Some(repo_root.to_path_buf());
        }

        // 4. Generic documentation / license root
        if let Some(gen_root) = generic_root {
            return Some(gen_root);
        }

        None
    }

    /// Full project intelligence detection for `path`.
    pub fn detect(path: &Path) -> ProjectFingerprint {
        let root = match Self::find_project_root(path) {
            Some(r) => r,
            None => return ProjectFingerprint::inactive(),
        };

        Self::inspect_root(&root)
    }

    /// Inspects a confirmed project root directory and builds its `ProjectFingerprint`.
    pub fn inspect_root(root: &Path) -> ProjectFingerprint {
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .map(ToString::to_string);

        let mut project_types = Vec::new();
        let mut languages = Vec::new();
        let mut build_systems = Vec::new();
        let mut manifests = Vec::new();
        let mut signals = ProjectSignals::default();

        // 1. Git Inspection
        let git_repo = detect_repository(root);
        let is_git = git_repo.is_repo();
        let git_root = git_repo.root().map(Path::to_path_buf);
        let is_git_worktree = git_repo.is_worktree();
        signals.git_root = git_root.clone();
        signals.is_git_worktree = is_git_worktree;

        // 2. Bounded scan of root entries
        let mut root_files = Vec::new();
        let mut root_dirs = Vec::new();

        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten().take(MAX_ROOT_SCAN_ENTRIES) {
                if let Ok(ft) = entry.file_type() {
                    let file_name = entry.file_name();
                    if let Some(name_str) = file_name.to_str() {
                        if ft.is_file() {
                            root_files.push(name_str.to_string());
                        } else if ft.is_dir() {
                            root_dirs.push(name_str.to_string());
                        }
                    }
                }
            }
        }

        // Helper closures for checking existence in shallow root scan
        let has_file = |name: &str| root_files.iter().any(|f| f.eq_ignore_ascii_case(name));
        let has_dir = |name: &str| root_dirs.iter().any(|d| d.eq_ignore_ascii_case(name));

        // Detect Source directories
        for s in &[
            "src", "app", "lib", "packages", "crates", "cmd", "pkg", "internal", "include",
            "sources",
        ] {
            if has_dir(s) {
                let p = root.join(s);
                signals.source_dirs.push(p);
            }
        }

        // Detect Test directories
        for t in &["tests", "test", "__tests__", "spec", "testing"] {
            if has_dir(t) {
                let p = root.join(t);
                signals.test_dirs.push(p);
            }
        }

        // Detect Documentation files
        for d in &[
            "README.md",
            "README",
            "README.txt",
            "README.rst",
            "LICENSE",
            "LICENSE.md",
            "LICENSE.txt",
            "COPYING",
            "CHANGELOG.md",
            "CONTRIBUTING.md",
        ] {
            if has_file(d) {
                let p = root.join(d);
                signals.doc_files.push(p);
            }
        }

        // Detect Documentation directories
        for dd in &["docs", "doc", "documentation"] {
            if has_dir(dd) {
                let p = root.join(dd);
                signals.doc_dirs.push(p);
            }
        }

        // Detect Configuration files
        for c in &[
            ".env",
            ".env.example",
            ".editorconfig",
            ".gitignore",
            "tsconfig.json",
            "jsconfig.json",
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
            "pnpm-workspace.yaml",
            "lerna.json",
            "turbo.json",
            "nx.json",
            "go.work",
        ] {
            if has_file(c) {
                let p = root.join(c);
                signals.config_files.push(p);
            }
        }

        // Detect Configuration directories
        for cd in &["config", ".config", ".settings"] {
            if has_dir(cd) {
                let p = root.join(cd);
                signals.config_dirs.push(p);
            }
        }

        // Detect CI/CD Signals
        if has_dir(".github") || root.join(".github").is_dir() {
            signals.ci_cd_signals.push(root.join(".github"));
        }
        for ci in &[
            ".gitlab-ci.yml",
            "Jenkinsfile",
            ".circleci",
            ".travis.yml",
            "azure-pipelines.yml",
            ".buildkite",
        ] {
            if has_file(ci) || has_dir(ci) {
                signals.ci_cd_signals.push(root.join(ci));
            }
        }

        // Detect Container Signals
        for cont in &[
            "Dockerfile",
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
            "Containerfile",
            ".dockerignore",
        ] {
            if has_file(cont) {
                signals.container_signals.push(root.join(cont));
            }
        }

        // Detect Build Output / Cache Indicators
        for b in &[
            "target",
            "dist",
            "build",
            "out",
            "node_modules",
            ".next",
            ".nuxt",
            ".turbo",
            "bin",
            "obj",
            "vendor",
        ] {
            if has_dir(b) {
                signals.build_artifact_indicators.push(root.join(b));
            }
        }

        // 3. Project Type, Language, and Build System Analysis

        // A. Rust
        if has_file("Cargo.toml") {
            let cargo_path = root.join("Cargo.toml");
            project_types.push(ProjectType::Rust);
            languages.push(Language::Rust);
            build_systems.push(BuildSystem::Cargo);
            manifests.push(cargo_path.clone());
            signals.manifest_files.push(cargo_path);
        }

        // B. Node / JavaScript / TypeScript
        if has_file("package.json") {
            let pkg_path = root.join("package.json");
            project_types.push(ProjectType::Node);
            manifests.push(pkg_path.clone());
            signals.manifest_files.push(pkg_path);

            // Package manager determination
            if has_file("pnpm-lock.yaml") || has_file("pnpm-workspace.yaml") {
                build_systems.push(BuildSystem::Pnpm);
            } else if has_file("yarn.lock") {
                build_systems.push(BuildSystem::Yarn);
            } else if has_file("bun.lockb") || has_file("bun.lock") {
                build_systems.push(BuildSystem::Bun);
            } else {
                build_systems.push(BuildSystem::Npm);
            }

            // TypeScript vs JavaScript
            if has_file("tsconfig.json") {
                project_types.push(ProjectType::TypeScript);
                languages.push(Language::TypeScript);
            } else {
                project_types.push(ProjectType::JavaScript);
                languages.push(Language::JavaScript);
            }
        } else if has_file("tsconfig.json") {
            let ts_path = root.join("tsconfig.json");
            project_types.push(ProjectType::TypeScript);
            languages.push(Language::TypeScript);
            manifests.push(ts_path.clone());
            signals.manifest_files.push(ts_path);
        }

        // C. Python
        let has_pyproject = has_file("pyproject.toml");
        let has_setup_py = has_file("setup.py");
        let has_reqs = has_file("requirements.txt");
        let has_pipfile = has_file("Pipfile");

        if has_pyproject || has_setup_py || has_reqs || has_pipfile {
            project_types.push(ProjectType::Python);
            languages.push(Language::Python);

            if has_pyproject {
                let p = root.join("pyproject.toml");
                manifests.push(p.clone());
                signals.manifest_files.push(p);
                if has_file("poetry.lock") {
                    build_systems.push(BuildSystem::Poetry);
                } else {
                    build_systems.push(BuildSystem::Pip);
                }
            } else if has_setup_py {
                let p = root.join("setup.py");
                manifests.push(p.clone());
                signals.manifest_files.push(p);
                build_systems.push(BuildSystem::Pip);
            } else if has_reqs {
                let p = root.join("requirements.txt");
                manifests.push(p.clone());
                signals.manifest_files.push(p);
                build_systems.push(BuildSystem::Pip);
            } else if has_pipfile {
                let p = root.join("Pipfile");
                manifests.push(p.clone());
                signals.manifest_files.push(p);
                build_systems.push(BuildSystem::Pipenv);
            }
        }

        // D. Java / Kotlin
        let has_pom = has_file("pom.xml");
        let has_gradle = has_file("build.gradle");
        let has_gradle_kts = has_file("build.gradle.kts");

        if has_pom || has_gradle || has_gradle_kts {
            project_types.push(ProjectType::Java);
            if has_gradle_kts {
                languages.push(Language::Kotlin);
            } else {
                languages.push(Language::Java);
            }

            if has_pom {
                let p = root.join("pom.xml");
                manifests.push(p.clone());
                signals.manifest_files.push(p);
                build_systems.push(BuildSystem::Maven);
            }
            if has_gradle {
                let p = root.join("build.gradle");
                manifests.push(p.clone());
                signals.manifest_files.push(p);
                build_systems.push(BuildSystem::Gradle);
            }
            if has_gradle_kts {
                let p = root.join("build.gradle.kts");
                manifests.push(p.clone());
                signals.manifest_files.push(p);
                build_systems.push(BuildSystem::Gradle);
            }
        }

        // E. Go
        if has_file("go.mod") {
            let p = root.join("go.mod");
            project_types.push(ProjectType::Go);
            languages.push(Language::Go);
            build_systems.push(BuildSystem::GoModules);
            manifests.push(p.clone());
            signals.manifest_files.push(p);
        }

        // F. C / C++
        let has_cmake = has_file("CMakeLists.txt");
        let has_makefile = has_file("Makefile");

        if has_cmake || has_makefile {
            project_types.push(ProjectType::Cpp);
            languages.push(Language::Cpp);

            if has_cmake {
                let p = root.join("CMakeLists.txt");
                manifests.push(p.clone());
                signals.manifest_files.push(p);
                build_systems.push(BuildSystem::CMake);
            }
            if has_makefile {
                let p = root.join("Makefile");
                manifests.push(p.clone());
                signals.manifest_files.push(p);
                build_systems.push(BuildSystem::Make);
            }
        }

        // G. PHP
        if has_file("composer.json") {
            let p = root.join("composer.json");
            project_types.push(ProjectType::Php);
            languages.push(Language::Php);
            build_systems.push(BuildSystem::Composer);
            manifests.push(p.clone());
            signals.manifest_files.push(p);
        }

        // H. Ruby
        if has_file("Gemfile") {
            let p = root.join("Gemfile");
            project_types.push(ProjectType::Ruby);
            languages.push(Language::Ruby);
            build_systems.push(BuildSystem::Bundler);
            manifests.push(p.clone());
            signals.manifest_files.push(p);
        }

        // I. .NET / C#
        for f in &root_files {
            let lower = f.to_ascii_lowercase();
            if lower.ends_with(".csproj") || lower.ends_with(".sln") || lower.ends_with(".fsproj") {
                let p = root.join(f);
                project_types.push(ProjectType::DotNet);
                languages.push(Language::CSharp);
                build_systems.push(BuildSystem::DotNetCli);
                manifests.push(p.clone());
                signals.manifest_files.push(p);
                break;
            }
        }

        // J. Generic fallback
        if project_types.is_empty() {
            if is_git {
                project_types.push(ProjectType::GenericGit);
            } else if signals.has_docs() || signals.has_source() {
                project_types.push(ProjectType::Generic);
            }
        }

        // Deduplicate types, languages, build systems
        project_types.dedup();
        languages.dedup();
        build_systems.dedup();

        // 4. Calculate Confidence Rating
        let confidence = if !manifests.is_empty() && signals.has_source() {
            DetectionConfidence::Definitive
        } else if !manifests.is_empty() {
            DetectionConfidence::High
        } else if is_git && signals.has_source() {
            DetectionConfidence::Medium
        } else if is_git || signals.has_docs() {
            DetectionConfidence::Low
        } else {
            DetectionConfidence::None
        };

        // 5. Workspace / Monorepo Check (Shallow)
        let is_workspace = has_file("pnpm-workspace.yaml")
            || has_file("lerna.json")
            || has_file("turbo.json")
            || has_file("go.work")
            || signals.source_dirs.iter().any(|s| {
                s.file_name()
                    .and_then(|n| n.to_str())
                    .map(|name| name == "packages" || name == "crates")
                    .unwrap_or(false)
            });

        // 6. Check for Parent Workspace above this root
        let parent_workspace = if let Some(parent) = root.parent() {
            let parent_git = git_root
                .as_ref()
                .filter(|gr| gr.as_path() != root && root.starts_with(gr));
            if let Some(pg) = parent_git {
                Some(pg.clone())
            } else if has_primary_manifest(parent) {
                Some(parent.to_path_buf())
            } else {
                None
            }
        } else {
            None
        };

        // 7. Find immediate subproject candidates if this is a workspace
        let mut subprojects = Vec::new();
        if is_workspace {
            for dir_name in &["packages", "crates", "apps", "services"] {
                let sub_parent = root.join(dir_name);
                if let Ok(entries) = fs::read_dir(&sub_parent) {
                    for entry in entries.flatten().take(16) {
                        let path = entry.path();
                        if path.is_dir()
                            && (has_primary_manifest(&path) || has_secondary_manifest(&path))
                        {
                            subprojects.push(path);
                        }
                    }
                }
            }
        }

        ProjectFingerprint {
            root: Some(root.to_path_buf()),
            name,
            project_types,
            languages,
            build_systems,
            manifests,
            signals,
            is_git,
            git_root,
            is_git_worktree,
            confidence,
            parent_workspace,
            subprojects,
            is_workspace,
        }
    }
}

/// Convenience function to detect project root for a path.
pub fn detect_project_root(path: &Path) -> Option<PathBuf> {
    ProjectDetector::find_project_root(path)
}

/// Convenience function to detect project fingerprint for a path.
pub fn detect_project(path: &Path) -> ProjectFingerprint {
    ProjectDetector::detect(path)
}

/// Checks if `dir` contains any primary language manifest.
fn has_primary_manifest(dir: &Path) -> bool {
    dir.join("Cargo.toml").is_file()
        || dir.join("package.json").is_file()
        || dir.join("pyproject.toml").is_file()
        || dir.join("setup.py").is_file()
        || dir.join("pom.xml").is_file()
        || dir.join("build.gradle").is_file()
        || dir.join("build.gradle.kts").is_file()
        || dir.join("go.mod").is_file()
        || dir.join("CMakeLists.txt").is_file()
        || dir.join("composer.json").is_file()
        || dir.join("Gemfile").is_file()
        || has_dotnet_manifest(dir)
}

/// Checks if `dir` contains secondary build manifests.
fn has_secondary_manifest(dir: &Path) -> bool {
    dir.join("Makefile").is_file()
        || dir.join("requirements.txt").is_file()
        || dir.join("Pipfile").is_file()
}

/// Checks if `dir` contains a `.git` directory or worktree file.
fn has_git_marker(dir: &Path) -> bool {
    let git = dir.join(".git");
    if let Ok(meta) = fs::metadata(&git) {
        meta.is_dir() || meta.is_file()
    } else {
        false
    }
}

/// Checks if `dir` contains generic documentation/license markers.
fn has_generic_markers(dir: &Path) -> bool {
    dir.join("README.md").is_file()
        || dir.join("README").is_file()
        || dir.join("README.txt").is_file()
        || dir.join("LICENSE").is_file()
        || dir.join("LICENSE.md").is_file()
        || dir.join("COPYING").is_file()
}

/// Checks if `dir` contains `.csproj`, `.fsproj`, or `.sln` marker files.
fn has_dotnet_manifest(dir: &Path) -> bool {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten().take(32) {
            if let Some(name) = entry.file_name().to_str() {
                let lower = name.to_ascii_lowercase();
                if (lower.ends_with(".csproj")
                    || lower.ends_with(".sln")
                    || lower.ends_with(".fsproj"))
                    && entry.file_type().map(|t| t.is_file()).unwrap_or(false)
                {
                    return true;
                }
            }
        }
    }
    false
}
