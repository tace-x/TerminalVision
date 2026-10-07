//! Structured Workspace Model and Project Graph representations.

use std::path::{Path, PathBuf};

use super::signals::ProjectSignals;
use super::types::{DetectionConfidence, Language, ProjectType};

/// Structural roles that a directory can fulfill within a project or workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum DirectoryRole {
    /// Source code directory (e.g., `src/`, `app/`, `lib/`, `packages/`, `components/`, `crates/`, `cmd/`, `pkg/`).
    Source,
    /// Automated test directory (e.g., `tests/`, `test/`, `__tests__/`, `spec/`, `testing/`).
    Tests,
    /// Documentation directory (e.g., `docs/`, `doc/`, `documentation/`).
    Documentation,
    /// Project configuration or IDE settings (e.g., `config/`, `.config/`, `.vscode/`, `.idea/`, `.settings/`).
    Configuration,
    /// Build output / compiler artifacts (e.g., `target/`, `dist/`, `build/`, `out/`, `bin/`, `obj/`).
    BuildOutput,
    /// Generated sources or framework artifacts (e.g., `.next/`, `.nuxt/`, `.turbo/`, `generated/`).
    Generated,
    /// Caching directory (e.g., `.cache/`, `.pytest_cache/`, `.mypy_cache/`, `.cargo-cache/`).
    Cache,
    /// Third-party dependencies / vendored packages (e.g., `node_modules/`, `vendor/`, `third_party/`).
    Dependencies,
    /// Continuous integration workflows / deployment (e.g., `.github/`, `.gitlab/`, `.circleci/`, `.buildkite/`).
    CI,
    /// Development tooling, scripts, or hooks (e.g., `scripts/`, `tools/`, `.husky/`, `bin/scripts/`).
    Tooling,
    /// Static assets, media, fonts, or templates (e.g., `assets/`, `static/`, `public/`, `media/`, `templates/`).
    Assets,
    /// Generic or unclassified directory.
    #[default]
    Unknown,
}

impl DirectoryRole {
    /// Human-readable display title for this directory role.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Tests => "Tests",
            Self::Documentation => "Documentation",
            Self::Configuration => "Configuration",
            Self::BuildOutput => "Build Output",
            Self::Generated => "Generated",
            Self::Cache => "Cache",
            Self::Dependencies => "Dependencies",
            Self::CI => "CI/CD",
            Self::Tooling => "Tooling",
            Self::Assets => "Assets",
            Self::Unknown => "Directory",
        }
    }

    /// Whether this directory role typically contains generated/cache/build artifacts
    /// that should be bounded or ignored by deep traversals.
    pub fn is_ignored_by_default(self) -> bool {
        matches!(
            self,
            Self::BuildOutput | Self::Generated | Self::Cache | Self::Dependencies
        )
    }

    /// Whether this role represents user source code or test suites.
    pub fn is_code(self) -> bool {
        matches!(self, Self::Source | Self::Tests | Self::Tooling)
    }
}

/// Structural roles that an important file fulfills within a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum ImportantFileRole {
    /// Package or project manifest (e.g., `Cargo.toml`, `package.json`, `go.mod`, `pom.xml`, `pyproject.toml`).
    Manifest,
    /// Human documentation (e.g., `README.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `ARCHITECTURE.md`).
    Documentation,
    /// Legal license file (e.g., `LICENSE`, `LICENSE.md`, `COPYING`).
    License,
    /// Project/tooling configuration (e.g., `tsconfig.json`, `.editorconfig`, `.gitignore`, `.env.example`).
    Configuration,
    /// Build orchestration script (e.g., `Makefile`, `CMakeLists.txt`, `build.gradle`, `build.rs`).
    Build,
    /// Continuous integration configuration (e.g., `.gitlab-ci.yml`, `Jenkinsfile`, `azure-pipelines.yml`).
    CI,
    /// Container/virtualization definition (e.g., `Dockerfile`, `docker-compose.yml`, `compose.yml`).
    Container,
    /// Primary binary entrypoint file (e.g., `main.rs`, `index.ts`, `main.py`, `main.go`, `App.java`).
    EntryPoint,
    /// Unclassified file.
    #[default]
    Unknown,
}

impl ImportantFileRole {
    /// Human-readable title for this file role.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Manifest => "Manifest",
            Self::Documentation => "Documentation",
            Self::License => "License",
            Self::Configuration => "Configuration",
            Self::Build => "Build Script",
            Self::CI => "CI/CD Config",
            Self::Container => "Container Spec",
            Self::EntryPoint => "Entry Point",
            Self::Unknown => "Important File",
        }
    }

    /// Whether this file acts as a project manifest.
    pub fn is_manifest(self) -> bool {
        matches!(self, Self::Manifest)
    }
}

/// A classified project-important file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportantFile {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// File name (e.g. `"Cargo.toml"`).
    pub name: String,
    /// Classified role.
    pub role: ImportantFileRole,
    /// Confidence rating of this classification.
    pub confidence: DetectionConfidence,
    /// Human-readable summary of the file's significance.
    pub description: String,
}

impl ImportantFile {
    /// Creates a new `ImportantFile` descriptor.
    pub fn new(
        path: PathBuf,
        role: ImportantFileRole,
        confidence: DetectionConfidence,
        description: impl Into<String>,
    ) -> Self {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        Self {
            path,
            name,
            role,
            confidence,
            description: description.into(),
        }
    }
}

/// A classified directory with its assigned structural role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedDirectory {
    /// Absolute path to the directory.
    pub path: PathBuf,
    /// Directory name (e.g. `"src"`).
    pub name: String,
    /// Classified role.
    pub role: DirectoryRole,
    /// Confidence rating.
    pub confidence: DetectionConfidence,
    /// Human-readable summary of this directory's role.
    pub description: String,
}

impl ClassifiedDirectory {
    /// Creates a new `ClassifiedDirectory` descriptor.
    pub fn new(
        path: PathBuf,
        role: DirectoryRole,
        confidence: DetectionConfidence,
        description: impl Into<String>,
    ) -> Self {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("dir")
            .to_string();
        Self {
            path,
            name,
            role,
            confidence,
            description: description.into(),
        }
    }
}

/// A project node within a workspace or standalone project graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectNode {
    /// Unique identifier for this project within the workspace (e.g. `"backend"`, `"packages/ui"`).
    pub id: String,
    /// Display name of the project.
    pub name: String,
    /// Root directory of this specific project.
    pub root: PathBuf,
    /// High-level project type.
    pub project_type: ProjectType,
    /// Recognized programming languages.
    pub languages: Vec<Language>,
    /// Primary and secondary manifests belonging to this project.
    pub manifests: Vec<ImportantFile>,
    /// Source directories belonging directly to this project.
    pub source_directories: Vec<ClassifiedDirectory>,
    /// Automated test directories belonging directly to this project.
    pub test_directories: Vec<ClassifiedDirectory>,
    /// Documentation directories belonging directly to this project.
    pub documentation_directories: Vec<ClassifiedDirectory>,
    /// Configuration directories belonging directly to this project.
    pub configuration_directories: Vec<ClassifiedDirectory>,
    /// Build output directories belonging to this project.
    pub build_output: Vec<ClassifiedDirectory>,
    /// Generated output directories.
    pub generated_output: Vec<ClassifiedDirectory>,
    /// Dependency directories.
    pub dependencies: Vec<ClassifiedDirectory>,
    /// Important files discovered at the project root.
    pub important_files: Vec<ImportantFile>,
    /// If nested inside a workspace, the root of the enclosing workspace.
    pub parent_workspace: Option<PathBuf>,
    /// Whether this node represents the top-level root project.
    pub is_root_project: bool,
}

impl ProjectNode {
    /// Returns the primary manifest file for this project if any.
    pub fn primary_manifest(&self) -> Option<&ImportantFile> {
        self.manifests.first()
    }

    /// Returns the primary source directory path if any.
    pub fn primary_source_dir(&self) -> Option<&Path> {
        self.source_directories.first().map(|d| d.path.as_path())
    }

    /// Returns the primary test directory path if any.
    pub fn primary_test_dir(&self) -> Option<&Path> {
        self.test_directories.first().map(|d| d.path.as_path())
    }

    /// Returns the primary documentation directory path if any.
    pub fn primary_doc_dir(&self) -> Option<&Path> {
        self.documentation_directories
            .first()
            .map(|d| d.path.as_path())
    }

    /// Checks if a path is located within this project's directory boundary.
    pub fn contains_path(&self, path: &Path) -> bool {
        path == self.root || path.starts_with(&self.root)
    }
}

/// Comprehensive, read-only workspace context and project structural graph.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceContext {
    /// The top-level workspace root directory.
    pub root: Option<PathBuf>,
    /// The display name of the workspace.
    pub name: Option<String>,
    /// Whether this workspace represents a multi-project monorepo.
    pub is_monorepo: bool,
    /// The primary/root project node if any.
    pub primary_project: Option<ProjectNode>,
    /// All detected project nodes (root and nested packages/crates/services).
    pub projects: Vec<ProjectNode>,
    /// All classified directories across the workspace root and immediate projects.
    pub directories: Vec<ClassifiedDirectory>,
    /// All classified important files across the workspace.
    pub important_files: Vec<ImportantFile>,
    /// Low-level project signals summary.
    pub signals: ProjectSignals,
}

impl WorkspaceContext {
    /// Creates an inactive/empty workspace context.
    pub fn inactive() -> Self {
        Self::default()
    }

    /// Whether this workspace context represents an active project or workspace.
    pub fn is_active(&self) -> bool {
        self.root.is_some()
    }

    /// Display name of the workspace.
    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("Workspace")
    }

    /// The root path of the workspace.
    pub fn project_root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// All project nodes belonging to this workspace.
    pub fn projects(&self) -> &[ProjectNode] {
        &self.projects
    }

    /// Whether the workspace is a multi-project monorepo.
    pub fn is_monorepo(&self) -> bool {
        self.is_monorepo
    }

    /// Finds the closest/most specific project node that contains `path`.
    pub fn project_for_path(&self, path: &Path) -> Option<&ProjectNode> {
        if !self.is_active() {
            return None;
        }

        // Check nested/sub projects first (longest root path matches first)
        let mut matching: Vec<&ProjectNode> = self
            .projects
            .iter()
            .filter(|p| p.contains_path(path))
            .collect();

        // Sort descending by root path length so deeper nested projects take precedence over workspace root
        matching.sort_by_key(|p| std::cmp::Reverse(p.root.as_os_str().len()));
        matching.first().copied().or(self.primary_project.as_ref())
    }

    /// Collects all source directory paths across all projects in the workspace.
    pub fn source_directories(&self) -> Vec<&Path> {
        let mut dirs = Vec::new();
        for proj in &self.projects {
            for s in &proj.source_directories {
                dirs.push(s.path.as_path());
            }
        }
        if dirs.is_empty() {
            for d in &self.directories {
                if d.role == DirectoryRole::Source {
                    dirs.push(d.path.as_path());
                }
            }
        }
        dirs
    }

    /// Collects all test directory paths across all projects in the workspace.
    pub fn test_directories(&self) -> Vec<&Path> {
        let mut dirs = Vec::new();
        for proj in &self.projects {
            for t in &proj.test_directories {
                dirs.push(t.path.as_path());
            }
        }
        if dirs.is_empty() {
            for d in &self.directories {
                if d.role == DirectoryRole::Tests {
                    dirs.push(d.path.as_path());
                }
            }
        }
        dirs
    }

    /// Collects all documentation directory paths across all projects in the workspace.
    pub fn documentation(&self) -> Vec<&Path> {
        let mut dirs = Vec::new();
        for proj in &self.projects {
            for doc in &proj.documentation_directories {
                dirs.push(doc.path.as_path());
            }
        }
        if dirs.is_empty() {
            for d in &self.directories {
                if d.role == DirectoryRole::Documentation {
                    dirs.push(d.path.as_path());
                }
            }
        }
        dirs
    }

    /// All classified important files.
    pub fn important_files(&self) -> &[ImportantFile] {
        &self.important_files
    }

    /// Determines the classified structural role of a given directory path.
    pub fn directory_role(&self, path: &Path) -> DirectoryRole {
        // Direct match in classified directory list
        for d in &self.directories {
            if d.path == path {
                return d.role;
            }
        }

        // Match within projects
        for proj in &self.projects {
            for d in &proj.source_directories {
                if d.path == path {
                    return DirectoryRole::Source;
                }
            }
            for d in &proj.test_directories {
                if d.path == path {
                    return DirectoryRole::Tests;
                }
            }
            for d in &proj.documentation_directories {
                if d.path == path {
                    return DirectoryRole::Documentation;
                }
            }
            for d in &proj.configuration_directories {
                if d.path == path {
                    return DirectoryRole::Configuration;
                }
            }
            for d in &proj.build_output {
                if d.path == path {
                    return DirectoryRole::BuildOutput;
                }
            }
            for d in &proj.generated_output {
                if d.path == path {
                    return DirectoryRole::Generated;
                }
            }
            for d in &proj.dependencies {
                if d.path == path {
                    return DirectoryRole::Dependencies;
                }
            }
        }

        DirectoryRole::Unknown
    }

    /// Looks up a project node by its name or ID.
    pub fn find_project(&self, name_or_id: &str) -> Option<&ProjectNode> {
        self.projects.iter().find(|p| {
            p.id.eq_ignore_ascii_case(name_or_id) || p.name.eq_ignore_ascii_case(name_or_id)
        })
    }

    /// All classified directories.
    pub fn all_directories(&self) -> &[ClassifiedDirectory] {
        &self.directories
    }
}
