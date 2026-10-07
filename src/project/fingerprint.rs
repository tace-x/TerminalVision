//! Consolidated project identity, structure, and multi-faceted fingerprint.

use std::path::{Path, PathBuf};

use super::signals::ProjectSignals;
use super::types::{BuildSystem, DetectionConfidence, Language, ProjectType};

/// A rich, structured, and read-only fingerprint of a project or workspace.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProjectFingerprint {
    /// The root directory of the project or workspace boundary.
    pub root: Option<PathBuf>,
    /// Display name of the project (typically derived from folder or manifest name).
    pub name: Option<String>,
    /// High-level detected project types (e.g. `Rust`, `Node`, `Python`, `Go`).
    pub project_types: Vec<ProjectType>,
    /// Detected programming languages.
    pub languages: Vec<Language>,
    /// Detected build and package management systems.
    pub build_systems: Vec<BuildSystem>,
    /// Primary and secondary manifests detected at root.
    pub manifests: Vec<PathBuf>,
    /// Deeply structured project signals (sources, tests, docs, config, containers, CI/CD).
    pub signals: ProjectSignals,
    /// Whether a Git repository surrounds or anchors this project.
    pub is_git: bool,
    /// Git root path if detected.
    pub git_root: Option<PathBuf>,
    /// Whether this project is hosted inside a Git worktree or submodule file.
    pub is_git_worktree: bool,
    /// The confidence rating of the root and project type detection.
    pub confidence: DetectionConfidence,
    /// If nested within a larger workspace/monorepo, the parent workspace root.
    pub parent_workspace: Option<PathBuf>,
    /// Nested child projects or workspace members if detected.
    pub subprojects: Vec<PathBuf>,
    /// Whether this project acts as a top-level multi-project workspace.
    pub is_workspace: bool,
}

impl ProjectFingerprint {
    /// Creates an inactive/empty fingerprint (representing a plain non-project directory).
    pub fn inactive() -> Self {
        Self {
            root: None,
            name: None,
            project_types: Vec::new(),
            languages: Vec::new(),
            build_systems: Vec::new(),
            manifests: Vec::new(),
            signals: ProjectSignals::default(),
            is_git: false,
            git_root: None,
            is_git_worktree: false,
            confidence: DetectionConfidence::None,
            parent_workspace: None,
            subprojects: Vec::new(),
            is_workspace: false,
        }
    }

    /// Whether this fingerprint represents an active, detected project or workspace.
    pub fn is_active(&self) -> bool {
        self.root.is_some()
    }

    /// The human-readable name of the project.
    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("Project")
    }

    /// The root path of the project if active.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// The primary project type if any was detected.
    pub fn primary_type(&self) -> Option<ProjectType> {
        self.project_types.first().copied()
    }

    /// The primary programming language if any was detected.
    pub fn primary_language(&self) -> Option<Language> {
        self.languages.first().copied()
    }

    /// The primary build system if any was detected.
    pub fn primary_build_system(&self) -> Option<BuildSystem> {
        self.build_systems.first().copied()
    }

    /// The primary manifest file path if any was detected.
    pub fn primary_manifest(&self) -> Option<&Path> {
        self.manifests.first().map(PathBuf::as_path)
    }

    /// Formatted string listing detected project types (e.g., `"Rust, Node.js"`).
    pub fn types_display(&self) -> String {
        if self.project_types.is_empty() {
            return String::new();
        }
        let names: Vec<&str> = self
            .project_types
            .iter()
            .map(|t| t.display_name())
            .collect();
        names.join(", ")
    }

    /// Formatted string listing detected languages (e.g., `"Rust, TypeScript"`).
    pub fn languages_display(&self) -> String {
        if self.languages.is_empty() {
            return String::new();
        }
        let names: Vec<&str> = self.languages.iter().map(|l| l.display_name()).collect();
        names.join(", ")
    }

    /// Formatted string listing detected build systems (e.g., `"Cargo, npm"`).
    pub fn build_systems_display(&self) -> String {
        if self.build_systems.is_empty() {
            return String::new();
        }
        let names: Vec<&str> = self
            .build_systems
            .iter()
            .map(|b| b.display_name())
            .collect();
        names.join(", ")
    }

    /// Converts this rich `ProjectFingerprint` into a legacy `ProjectInfo` structure
    /// for seamless backwards compatibility with earlier components.
    pub fn to_legacy_project_info(&self) -> crate::git::project::ProjectInfo {
        if !self.is_active() {
            return crate::git::project::ProjectInfo::inactive();
        }

        let legacy_types: Vec<crate::git::project::ProjectType> = self
            .project_types
            .iter()
            .map(|t| match t {
                ProjectType::Rust => crate::git::project::ProjectType::Rust,
                ProjectType::Node | ProjectType::JavaScript | ProjectType::TypeScript => {
                    crate::git::project::ProjectType::Node
                }
                ProjectType::Python => crate::git::project::ProjectType::Python,
                ProjectType::Java => crate::git::project::ProjectType::Java,
                ProjectType::Go => crate::git::project::ProjectType::Go,
                ProjectType::C | ProjectType::Cpp => crate::git::project::ProjectType::Cpp,
                ProjectType::Php => crate::git::project::ProjectType::Php,
                ProjectType::Ruby => crate::git::project::ProjectType::Ruby,
                ProjectType::DotNet => crate::git::project::ProjectType::DotNet,
                ProjectType::Generic | ProjectType::GenericGit | ProjectType::GenericWorkspace => {
                    crate::git::project::ProjectType::Generic
                }
            })
            .collect();

        crate::git::project::ProjectInfo {
            root: self.root.clone(),
            name: self.name.clone(),
            project_types: legacy_types,
            manifest_file: self.manifests.first().cloned(),
            readme_file: self.signals.primary_readme().map(Path::to_path_buf),
            license_file: self.signals.primary_license().map(Path::to_path_buf),
            source_dir: self.signals.primary_source_dir().map(Path::to_path_buf),
            useful_files: self.manifests.clone(),
        }
    }
}
