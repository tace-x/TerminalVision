//! Project signals collected from bounded, shallow filesystem inspection.

use super::types::SignalKind;
use std::path::{Path, PathBuf};

/// An individual project signal discovered during inspection.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSignal {
    /// The structural category of this signal.
    pub kind: SignalKind,
    /// Absolute or relative path to the signal artifact.
    pub path: PathBuf,
    /// Name or title of the signal (e.g. `"Cargo.toml"`, `"src/"`, `".github"`).
    pub name: String,
    /// Human-readable explanation of what this signal implies.
    pub description: String,
    /// Relative confidence weight contributed by this signal.
    pub confidence_weight: f32,
}

/// Consolidated collection of signals detected in a project root and its shallow immediate surroundings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectSignals {
    /// Manifest and build configuration files (e.g., `Cargo.toml`, `package.json`, `go.mod`).
    pub manifest_files: Vec<PathBuf>,
    /// Confidently identified source directories (e.g., `src/`, `app/`, `lib/`, `packages/`, `cmd/`, `pkg/`).
    pub source_dirs: Vec<PathBuf>,
    /// Test directories (e.g., `tests/`, `test/`, `__tests__/`, `spec/`).
    pub test_dirs: Vec<PathBuf>,
    /// Key documentation files (e.g., `README.md`, `LICENSE`, `CHANGELOG.md`, `CONTRIBUTING.md`).
    pub doc_files: Vec<PathBuf>,
    /// Documentation directories (e.g., `docs/`, `doc/`).
    pub doc_dirs: Vec<PathBuf>,
    /// Configuration files (e.g., `.env.example`, `.editorconfig`, `.gitignore`, `tsconfig.json`).
    pub config_files: Vec<PathBuf>,
    /// Configuration directories (e.g., `config/`, `.config/`).
    pub config_dirs: Vec<PathBuf>,
    /// Continuous integration / deployment definitions (e.g., `.github/`, `.gitlab-ci.yml`, `Jenkinsfile`).
    pub ci_cd_signals: Vec<PathBuf>,
    /// Container and virtualization definitions (e.g., `Dockerfile`, `docker-compose.yml`, `compose.yml`).
    pub container_signals: Vec<PathBuf>,
    /// Generated build outputs, caches, or package artifacts (e.g., `target/`, `node_modules/`, `dist/`, `build/`).
    pub build_artifact_indicators: Vec<PathBuf>,
    /// Git repository root if present.
    pub git_root: Option<PathBuf>,
    /// Whether the Git repository uses a `.git` worktree or submodule file.
    pub is_git_worktree: bool,
}

impl ProjectSignals {
    /// Returns true if a Git repository was detected.
    pub fn has_git(&self) -> bool {
        self.git_root.is_some()
    }

    /// Returns true if any source directories were detected.
    pub fn has_source(&self) -> bool {
        !self.source_dirs.is_empty()
    }

    /// Returns true if any test directories were detected.
    pub fn has_tests(&self) -> bool {
        !self.test_dirs.is_empty()
    }

    /// Returns true if any documentation files or directories were detected.
    pub fn has_docs(&self) -> bool {
        !self.doc_files.is_empty() || !self.doc_dirs.is_empty()
    }

    /// Returns true if Docker or container configuration was detected.
    pub fn has_docker(&self) -> bool {
        !self.container_signals.is_empty()
    }

    /// Returns true if CI/CD workflows were detected.
    pub fn has_ci(&self) -> bool {
        !self.ci_cd_signals.is_empty()
    }

    /// The primary source directory if any.
    pub fn primary_source_dir(&self) -> Option<&Path> {
        self.source_dirs.first().map(PathBuf::as_path)
    }

    /// The primary test directory if any.
    pub fn primary_test_dir(&self) -> Option<&Path> {
        self.test_dirs.first().map(PathBuf::as_path)
    }

    /// The primary README file if any.
    pub fn primary_readme(&self) -> Option<&Path> {
        for file in &self.doc_files {
            if let Some(name) = file.file_name().and_then(|n| n.to_str()) {
                let lower = name.to_ascii_lowercase();
                if lower.starts_with("readme") {
                    return Some(file.as_path());
                }
            }
        }
        None
    }

    /// The primary LICENSE file if any.
    pub fn primary_license(&self) -> Option<&Path> {
        for file in &self.doc_files {
            if let Some(name) = file.file_name().and_then(|n| n.to_str()) {
                let lower = name.to_ascii_lowercase();
                if lower.starts_with("license") || lower.starts_with("copying") {
                    return Some(file.as_path());
                }
            }
        }
        None
    }

    /// Total count of all distinct signals recorded.
    pub fn total_signals_count(&self) -> usize {
        self.manifest_files.len()
            + self.source_dirs.len()
            + self.test_dirs.len()
            + self.doc_files.len()
            + self.doc_dirs.len()
            + self.config_files.len()
            + self.config_dirs.len()
            + self.ci_cd_signals.len()
            + self.container_signals.len()
            + self.build_artifact_indicators.len()
            + if self.git_root.is_some() { 1 } else { 0 }
    }

    /// Flattens all signals into a collection of [`ProjectSignal`] items.
    pub fn all_signals(&self) -> Vec<ProjectSignal> {
        let mut signals = Vec::new();

        if let Some(ref gr) = self.git_root {
            signals.push(ProjectSignal {
                kind: if self.is_git_worktree {
                    SignalKind::GitWorktree
                } else {
                    SignalKind::GitRepo
                },
                path: gr.clone(),
                name: if self.is_git_worktree {
                    ".git (worktree)".to_string()
                } else {
                    ".git".to_string()
                },
                description: "Git version control root".to_string(),
                confidence_weight: 0.9,
            });
        }

        for m in &self.manifest_files {
            let name = m
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("manifest")
                .to_string();
            signals.push(ProjectSignal {
                kind: SignalKind::Manifest,
                path: m.clone(),
                name: name.clone(),
                description: format!("Project manifest: {name}"),
                confidence_weight: 1.0,
            });
        }

        for s in &self.source_dirs {
            let name = s
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("src")
                .to_string();
            signals.push(ProjectSignal {
                kind: SignalKind::SourceDir,
                path: s.clone(),
                name: format!("{name}/"),
                description: format!("Source directory: {name}"),
                confidence_weight: 0.8,
            });
        }

        for t in &self.test_dirs {
            let name = t
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("tests")
                .to_string();
            signals.push(ProjectSignal {
                kind: SignalKind::TestDir,
                path: t.clone(),
                name: format!("{name}/"),
                description: format!("Test directory: {name}"),
                confidence_weight: 0.7,
            });
        }

        for d in &self.doc_files {
            let name = d
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("doc")
                .to_string();
            signals.push(ProjectSignal {
                kind: SignalKind::DocFile,
                path: d.clone(),
                name: name.clone(),
                description: format!("Documentation file: {name}"),
                confidence_weight: 0.5,
            });
        }

        for dd in &self.doc_dirs {
            let name = dd
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("docs")
                .to_string();
            signals.push(ProjectSignal {
                kind: SignalKind::DocDir,
                path: dd.clone(),
                name: format!("{name}/"),
                description: format!("Documentation directory: {name}"),
                confidence_weight: 0.5,
            });
        }

        for c in &self.config_files {
            let name = c
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("config")
                .to_string();
            signals.push(ProjectSignal {
                kind: SignalKind::ConfigFile,
                path: c.clone(),
                name: name.clone(),
                description: format!("Configuration file: {name}"),
                confidence_weight: 0.6,
            });
        }

        for cd in &self.config_dirs {
            let name = cd
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("config")
                .to_string();
            signals.push(ProjectSignal {
                kind: SignalKind::ConfigDir,
                path: cd.clone(),
                name: format!("{name}/"),
                description: format!("Configuration directory: {name}"),
                confidence_weight: 0.6,
            });
        }

        for ci in &self.ci_cd_signals {
            let name = ci
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("ci")
                .to_string();
            signals.push(ProjectSignal {
                kind: SignalKind::CiCd,
                path: ci.clone(),
                name: name.clone(),
                description: format!("CI/CD configuration: {name}"),
                confidence_weight: 0.7,
            });
        }

        for cont in &self.container_signals {
            let name = cont
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("container")
                .to_string();
            signals.push(ProjectSignal {
                kind: SignalKind::Container,
                path: cont.clone(),
                name: name.clone(),
                description: format!("Container specification: {name}"),
                confidence_weight: 0.7,
            });
        }

        for b in &self.build_artifact_indicators {
            let name = b
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("build")
                .to_string();
            signals.push(ProjectSignal {
                kind: SignalKind::BuildArtifact,
                path: b.clone(),
                name: format!("{name}/"),
                description: format!("Build/cache artifact directory: {name}"),
                confidence_weight: 0.3,
            });
        }

        signals
    }
}
