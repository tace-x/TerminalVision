//! Workspace structure builder and project graph analyzer.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::detector::ProjectDetector;
use super::types::{DetectionConfidence, Language, ProjectType};
use super::workspace::{
    ClassifiedDirectory, DirectoryRole, ImportantFile, ImportantFileRole, ProjectNode,
    WorkspaceContext,
};

/// Maximum subproject count discovered to ensure bounded execution in large monorepos.
const MAX_DISCOVERED_PROJECTS: usize = 32;

/// Maximum entries inspected per directory level.
const MAX_DIRECTORY_ENTRIES: usize = 128;

/// Input bundle for constructing a [`ProjectNode`].
struct ProjectNodeInput<'a> {
    root: &'a Path,
    id: &'a str,
    name: &'a str,
    project_types: &'a [ProjectType],
    languages: &'a [Language],
    directories: &'a [ClassifiedDirectory],
    files: &'a [ImportantFile],
    parent_workspace: Option<PathBuf>,
    is_root_project: bool,
}

/// Workspace analyzer for constructing [`WorkspaceContext`] and [`ProjectNode`] structural graphs.
pub struct WorkspaceAnalyzer;

impl WorkspaceAnalyzer {
    /// Analyzes the workspace structure for any starting path.
    pub fn analyze(start_path: &Path) -> WorkspaceContext {
        let root = match ProjectDetector::find_project_root(start_path) {
            Some(r) => r,
            None => return WorkspaceContext::inactive(),
        };

        Self::analyze_root(&root)
    }

    /// Analyzes a confirmed workspace/project root directory.
    pub fn analyze_root(root: &Path) -> WorkspaceContext {
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .map(ToString::to_string);

        let mut visited_paths = HashSet::new();
        if let Ok(canonical) = root.canonicalize() {
            visited_paths.insert(canonical);
        } else {
            visited_paths.insert(root.to_path_buf());
        }

        // 1. Inspect root level items
        let (root_dirs, root_files) = Self::inspect_shallow_directory(root);
        let root_classified_dirs = Self::classify_directories(root, &root_dirs);
        let root_important_files = Self::classify_files(root, &root_files);

        // 2. Determine root fingerprint and primary project node
        let root_fingerprint = ProjectDetector::inspect_root(root);
        let root_is_workspace = root_fingerprint.is_workspace;
        let root_has_explicit_manifest = !root_fingerprint.manifests.is_empty();

        let root_project = if !root_fingerprint.project_types.is_empty() {
            Some(Self::build_project_node(ProjectNodeInput {
                root,
                id: "root",
                name: root.file_name().and_then(|n| n.to_str()).unwrap_or("root"),
                project_types: &root_fingerprint.project_types,
                languages: &root_fingerprint.languages,
                directories: &root_classified_dirs,
                files: &root_important_files,
                parent_workspace: None,
                is_root_project: true,
            }))
        } else {
            None
        };

        // 3. Discover nested subprojects
        let mut subprojects = Vec::new();

        // Candidate subproject locations:
        // A. Direct subdirectories (e.g. `frontend/`, `backend/`, `shared/`)
        // B. Nested subdirectories (e.g. `packages/*`, `crates/*`, `tools/*`, `apps/*`, `services/*`, `libs/*`, `modules/*`)
        for dir_name in &root_dirs {
            if subprojects.len() >= MAX_DISCOVERED_PROJECTS {
                break;
            }

            let sub_path = root.join(dir_name);
            let canonical = sub_path.canonicalize().unwrap_or_else(|_| sub_path.clone());
            if visited_paths.contains(&canonical) {
                continue;
            }
            visited_paths.insert(canonical);

            let lower = dir_name.to_ascii_lowercase();
            if matches!(
                lower.as_str(),
                ".git"
                    | "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | ".cache"
                    | ".next"
                    | ".nuxt"
            ) {
                continue;
            }

            // Check if this directory itself is a project
            if Self::has_project_manifest(&sub_path) {
                let sub_fp = ProjectDetector::inspect_root(&sub_path);
                if !sub_fp.project_types.is_empty() {
                    let (s_dirs, s_files) = Self::inspect_shallow_directory(&sub_path);
                    let s_classified = Self::classify_directories(&sub_path, &s_dirs);
                    let s_important = Self::classify_files(&sub_path, &s_files);

                    let node = Self::build_project_node(ProjectNodeInput {
                        root: &sub_path,
                        id: dir_name,
                        name: dir_name,
                        project_types: &sub_fp.project_types,
                        languages: &sub_fp.languages,
                        directories: &s_classified,
                        files: &s_important,
                        parent_workspace: Some(root.to_path_buf()),
                        is_root_project: false,
                    });
                    subprojects.push(node);
                }
            } else {
                // Inspect children of intermediate container folder (e.g. tools/*, packages/*, crates/*)
                if let Ok(entries) = fs::read_dir(&sub_path) {
                    for entry in entries.flatten().take(16) {
                        if subprojects.len() >= MAX_DISCOVERED_PROJECTS {
                            break;
                        }
                        let child_path = entry.path();
                        if child_path.is_dir() && Self::has_project_manifest(&child_path) {
                            let child_canon = child_path
                                .canonicalize()
                                .unwrap_or_else(|_| child_path.clone());
                            if visited_paths.contains(&child_canon) {
                                continue;
                            }
                            visited_paths.insert(child_canon);

                            let child_name = child_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("subproject");
                            let child_id = format!("{dir_name}/{child_name}");

                            let child_fp = ProjectDetector::inspect_root(&child_path);
                            let (c_dirs, c_files) = Self::inspect_shallow_directory(&child_path);
                            let c_classified = Self::classify_directories(&child_path, &c_dirs);
                            let c_important = Self::classify_files(&child_path, &c_files);

                            let node = Self::build_project_node(ProjectNodeInput {
                                root: &child_path,
                                id: &child_id,
                                name: child_name,
                                project_types: &child_fp.project_types,
                                languages: &child_fp.languages,
                                directories: &c_classified,
                                files: &c_important,
                                parent_workspace: Some(root.to_path_buf()),
                                is_root_project: false,
                            });
                            subprojects.push(node);
                        }
                    }
                }
            }
        }

        // 4. Assemble projects list and monorepo flag
        let is_monorepo = root_is_workspace || !subprojects.is_empty();

        let mut discovered_projects = Vec::new();
        if (root_has_explicit_manifest || subprojects.is_empty())
            && let Some(ref rp) = root_project
        {
            discovered_projects.push(rp.clone());
        }
        discovered_projects.extend(subprojects);

        // Deduplicate projects by root path
        discovered_projects.dedup_by(|a, b| a.root == b.root);

        // Consolidate signals
        let mut signals = root_fingerprint.signals;
        for p in &discovered_projects {
            for s in &p.source_directories {
                if !signals.source_dirs.contains(&s.path) {
                    signals.source_dirs.push(s.path.clone());
                }
            }
            for t in &p.test_directories {
                if !signals.test_dirs.contains(&t.path) {
                    signals.test_dirs.push(t.path.clone());
                }
            }
        }

        WorkspaceContext {
            root: Some(root.to_path_buf()),
            name,
            is_monorepo,
            primary_project: root_project,
            projects: discovered_projects,
            directories: root_classified_dirs,
            important_files: root_important_files,
            signals,
        }
    }

    /// Shallow inspection of a directory returning `(subdirectories, files)`.
    fn inspect_shallow_directory(dir: &Path) -> (Vec<String>, Vec<String>) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten().take(MAX_DIRECTORY_ENTRIES) {
                if let Ok(ft) = entry.file_type()
                    && let Some(name) = entry.file_name().to_str()
                {
                    if ft.is_dir() {
                        dirs.push(name.to_string());
                    } else if ft.is_file() {
                        files.push(name.to_string());
                    }
                }
            }
        }

        (dirs, files)
    }

    /// Classifies subdirectories based on their name, path, and project-aware signals.
    fn classify_directories(base_dir: &Path, dir_names: &[String]) -> Vec<ClassifiedDirectory> {
        let mut classified = Vec::new();

        for name in dir_names {
            let path = base_dir.join(name);
            let lower = name.to_ascii_lowercase();

            let (role, confidence, desc) = match lower.as_str() {
                // Source
                "src" | "sources" => (
                    DirectoryRole::Source,
                    DetectionConfidence::Definitive,
                    "Primary source code directory",
                ),
                "app" | "lib" | "packages" | "components" | "crates" | "cmd" | "pkg"
                | "internal" | "include" => (
                    DirectoryRole::Source,
                    DetectionConfidence::High,
                    "Application or library source directory",
                ),

                // Tests
                "tests" | "__tests__" => (
                    DirectoryRole::Tests,
                    DetectionConfidence::Definitive,
                    "Automated test suite directory",
                ),
                "test" | "spec" | "testing" => (
                    DirectoryRole::Tests,
                    DetectionConfidence::High,
                    "Project tests or specifications",
                ),

                // Documentation
                "docs" | "documentation" => (
                    DirectoryRole::Documentation,
                    DetectionConfidence::Definitive,
                    "Project documentation directory",
                ),
                "doc" => (
                    DirectoryRole::Documentation,
                    DetectionConfidence::High,
                    "Documentation files",
                ),

                // Configuration / IDE
                "config" | ".config" => (
                    DirectoryRole::Configuration,
                    DetectionConfidence::Definitive,
                    "Project configuration directory",
                ),
                ".vscode" | ".idea" | ".settings" => (
                    DirectoryRole::Configuration,
                    DetectionConfidence::High,
                    "IDE and editor workspace settings",
                ),

                // Build Output
                "target" => (
                    DirectoryRole::BuildOutput,
                    DetectionConfidence::Definitive,
                    "Rust / Cargo target build output",
                ),
                "dist" | "out" => (
                    DirectoryRole::BuildOutput,
                    DetectionConfidence::Definitive,
                    "Distribution / compiler output directory",
                ),
                "build" | "bin" | "obj" => (
                    DirectoryRole::BuildOutput,
                    DetectionConfidence::High,
                    "Compiled binaries and intermediate build artifacts",
                ),

                // Generated
                ".next" | ".nuxt" | ".turbo" | "generated" => (
                    DirectoryRole::Generated,
                    DetectionConfidence::Definitive,
                    "Framework-generated code and build caches",
                ),

                // Cache
                ".cache" | ".pytest_cache" | ".mypy_cache" | ".cargo-cache" => (
                    DirectoryRole::Cache,
                    DetectionConfidence::Definitive,
                    "Compiler / toolchain cache directory",
                ),

                // Dependencies
                "node_modules" => (
                    DirectoryRole::Dependencies,
                    DetectionConfidence::Definitive,
                    "Node.js package dependencies",
                ),
                "vendor" | "third_party" => (
                    DirectoryRole::Dependencies,
                    DetectionConfidence::High,
                    "Vendored third-party dependencies",
                ),

                // CI/CD
                ".github" | ".gitlab" | ".circleci" | ".buildkite" => (
                    DirectoryRole::CI,
                    DetectionConfidence::Definitive,
                    "Continuous integration and automation workflows",
                ),

                // Tooling
                "scripts" | "tools" | ".husky" => (
                    DirectoryRole::Tooling,
                    DetectionConfidence::High,
                    "Development tooling, build scripts, and git hooks",
                ),

                // Assets
                "assets" | "static" | "public" | "media" | "templates" => (
                    DirectoryRole::Assets,
                    DetectionConfidence::High,
                    "Static assets, media files, and public web resources",
                ),

                _ => (
                    DirectoryRole::Unknown,
                    DetectionConfidence::Low,
                    "Directory",
                ),
            };

            classified.push(ClassifiedDirectory::new(path, role, confidence, desc));
        }

        classified
    }

    /// Classifies important project files based on their names.
    fn classify_files(base_dir: &Path, file_names: &[String]) -> Vec<ImportantFile> {
        let mut classified = Vec::new();

        for name in file_names {
            let path = base_dir.join(name);
            let lower = name.to_ascii_lowercase();

            let role_info: Option<(ImportantFileRole, DetectionConfidence, &'static str)> = if lower
                == "cargo.toml"
            {
                Some((
                    ImportantFileRole::Manifest,
                    DetectionConfidence::Definitive,
                    "Rust / Cargo project manifest",
                ))
            } else if lower == "package.json" {
                Some((
                    ImportantFileRole::Manifest,
                    DetectionConfidence::Definitive,
                    "Node.js / JavaScript package manifest",
                ))
            } else if lower == "pyproject.toml" {
                Some((
                    ImportantFileRole::Manifest,
                    DetectionConfidence::Definitive,
                    "Python project configuration and dependencies",
                ))
            } else if lower == "requirements.txt" || lower == "setup.py" || lower == "pipfile" {
                Some((
                    ImportantFileRole::Manifest,
                    DetectionConfidence::High,
                    "Python build / dependency manifest",
                ))
            } else if lower == "go.mod" {
                Some((
                    ImportantFileRole::Manifest,
                    DetectionConfidence::Definitive,
                    "Go module definition",
                ))
            } else if lower == "pom.xml" {
                Some((
                    ImportantFileRole::Manifest,
                    DetectionConfidence::Definitive,
                    "Java / Maven project object model",
                ))
            } else if lower == "build.gradle" || lower == "build.gradle.kts" {
                Some((
                    ImportantFileRole::Manifest,
                    DetectionConfidence::Definitive,
                    "Gradle build configuration",
                ))
            } else if lower == "cmakelists.txt" {
                Some((
                    ImportantFileRole::Manifest,
                    DetectionConfidence::Definitive,
                    "CMake build specification",
                ))
            } else if lower == "makefile" {
                Some((
                    ImportantFileRole::Build,
                    DetectionConfidence::High,
                    "Make build orchestration script",
                ))
            } else if lower == "composer.json" {
                Some((
                    ImportantFileRole::Manifest,
                    DetectionConfidence::Definitive,
                    "PHP Composer package manifest",
                ))
            } else if lower == "gemfile" {
                Some((
                    ImportantFileRole::Manifest,
                    DetectionConfidence::Definitive,
                    "Ruby Bundler gem manifest",
                ))
            } else if lower.ends_with(".csproj")
                || lower.ends_with(".fsproj")
                || lower.ends_with(".sln")
            {
                Some((
                    ImportantFileRole::Manifest,
                    DetectionConfidence::Definitive,
                    ".NET project / solution manifest",
                ))
            } else if lower.starts_with("readme") {
                Some((
                    ImportantFileRole::Documentation,
                    DetectionConfidence::Definitive,
                    "Primary project documentation",
                ))
            } else if lower.starts_with("license") || lower.starts_with("copying") {
                Some((
                    ImportantFileRole::License,
                    DetectionConfidence::Definitive,
                    "Software license terms",
                ))
            } else if lower == "changelog.md"
                || lower == "contributing.md"
                || lower == "architecture.md"
            {
                Some((
                    ImportantFileRole::Documentation,
                    DetectionConfidence::High,
                    "Project development / architecture documentation",
                ))
            } else if lower == "tsconfig.json" || lower == "jsconfig.json" {
                Some((
                    ImportantFileRole::Configuration,
                    DetectionConfidence::Definitive,
                    "TypeScript / JavaScript compiler options",
                ))
            } else if lower == ".editorconfig" || lower == ".gitignore" || lower.starts_with(".env")
            {
                Some((
                    ImportantFileRole::Configuration,
                    DetectionConfidence::High,
                    "Environment / editor configuration",
                ))
            } else if lower == "dockerfile" || lower == "containerfile" {
                Some((
                    ImportantFileRole::Container,
                    DetectionConfidence::Definitive,
                    "Container image definition",
                ))
            } else if lower == "docker-compose.yml"
                || lower == "docker-compose.yaml"
                || lower == "compose.yml"
                || lower == "compose.yaml"
            {
                Some((
                    ImportantFileRole::Container,
                    DetectionConfidence::Definitive,
                    "Multi-container application orchestration",
                ))
            } else if lower == ".gitlab-ci.yml"
                || lower == "jenkinsfile"
                || lower == ".travis.yml"
                || lower == "azure-pipelines.yml"
            {
                Some((
                    ImportantFileRole::CI,
                    DetectionConfidence::Definitive,
                    "CI/CD automation pipeline",
                ))
            } else if lower == "pnpm-workspace.yaml"
                || lower == "lerna.json"
                || lower == "turbo.json"
                || lower == "go.work"
            {
                Some((
                    ImportantFileRole::Manifest,
                    DetectionConfidence::Definitive,
                    "Monorepo / workspace orchestration manifest",
                ))
            } else if matches!(
                lower.as_str(),
                "main.rs" | "index.ts" | "index.js" | "main.py" | "app.py" | "main.go" | "app.java"
            ) {
                Some((
                    ImportantFileRole::EntryPoint,
                    DetectionConfidence::High,
                    "Primary application entrypoint",
                ))
            } else {
                None
            };

            if let Some((role, confidence, desc)) = role_info {
                classified.push(ImportantFile::new(path, role, confidence, desc));
            }
        }

        classified
    }

    /// Constructs a `ProjectNode` by partitioning classified directories and files.
    fn build_project_node(input: ProjectNodeInput<'_>) -> ProjectNode {
        let mut source_directories = Vec::new();
        let mut test_directories = Vec::new();
        let mut documentation_directories = Vec::new();
        let mut configuration_directories = Vec::new();
        let mut build_output = Vec::new();
        let mut generated_output = Vec::new();
        let mut dependencies = Vec::new();

        for d in input.directories {
            match d.role {
                DirectoryRole::Source => source_directories.push(d.clone()),
                DirectoryRole::Tests => test_directories.push(d.clone()),
                DirectoryRole::Documentation => documentation_directories.push(d.clone()),
                DirectoryRole::Configuration => configuration_directories.push(d.clone()),
                DirectoryRole::BuildOutput => build_output.push(d.clone()),
                DirectoryRole::Generated => generated_output.push(d.clone()),
                DirectoryRole::Dependencies => dependencies.push(d.clone()),
                _ => {}
            }
        }

        let manifests = input
            .files
            .iter()
            .filter(|f| f.role.is_manifest())
            .cloned()
            .collect();

        ProjectNode {
            id: input.id.to_string(),
            name: input.name.to_string(),
            root: input.root.to_path_buf(),
            project_type: input
                .project_types
                .first()
                .copied()
                .unwrap_or(ProjectType::Generic),
            languages: input.languages.to_vec(),
            manifests,
            source_directories,
            test_directories,
            documentation_directories,
            configuration_directories,
            build_output,
            generated_output,
            dependencies,
            important_files: input.files.to_vec(),
            parent_workspace: input.parent_workspace,
            is_root_project: input.is_root_project,
        }
    }

    /// Checks whether `dir` directly contains any project manifest.
    fn has_project_manifest(dir: &Path) -> bool {
        dir.join("Cargo.toml").is_file()
            || dir.join("package.json").is_file()
            || dir.join("pyproject.toml").is_file()
            || dir.join("setup.py").is_file()
            || dir.join("requirements.txt").is_file()
            || dir.join("pom.xml").is_file()
            || dir.join("build.gradle").is_file()
            || dir.join("build.gradle.kts").is_file()
            || dir.join("go.mod").is_file()
            || dir.join("CMakeLists.txt").is_file()
            || dir.join("Makefile").is_file()
            || dir.join("composer.json").is_file()
            || dir.join("Gemfile").is_file()
    }
}

/// Convenience function to analyze workspace structure for a path.
pub fn analyze_workspace(path: &Path) -> WorkspaceContext {
    WorkspaceAnalyzer::analyze(path)
}
