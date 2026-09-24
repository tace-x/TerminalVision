//! Read-only project identity and type detection based on repository and directory markers.

use std::fs;
use std::path::{Path, PathBuf};

use super::status::GitStatus;

/// Supported project types identified by project root marker files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Java,
    Go,
    Cpp,
    Php,
    Ruby,
    DotNet,
    Generic,
}

impl ProjectType {
    /// Human-readable display name for this project type.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Node => "Node.js",
            Self::Python => "Python",
            Self::Java => "Java",
            Self::Go => "Go",
            Self::Cpp => "C/C++",
            Self::Php => "PHP",
            Self::Ruby => "Ruby",
            Self::DotNet => ".NET",
            Self::Generic => "Generic",
        }
    }

    /// Alias for display_name.
    pub fn name(self) -> &'static str {
        self.display_name()
    }
}

/// Consolidated project identity and structure info.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectInfo {
    /// The root directory of the project.
    pub root: Option<PathBuf>,
    /// Display name of the project folder.
    pub name: Option<String>,
    /// Detected project types based on root marker files.
    pub project_types: Vec<ProjectType>,
    /// Primary manifest file if detected (e.g. `Cargo.toml`, `package.json`, `pom.xml`, etc.).
    pub manifest_file: Option<PathBuf>,
    /// Primary README document if detected (e.g. `README.md`).
    pub readme_file: Option<PathBuf>,
    /// Primary license file if detected (e.g. `LICENSE`).
    pub license_file: Option<PathBuf>,
    /// Confidently detected source directory (e.g. `src/`, `app/`, `lib/`).
    pub source_dir: Option<PathBuf>,
    /// List of other important detected project files.
    pub useful_files: Vec<PathBuf>,
}

impl ProjectInfo {
    /// Creates an inactive project info (when not in a project/repository).
    pub fn inactive() -> Self {
        Self {
            root: None,
            name: None,
            project_types: Vec::new(),
            manifest_file: None,
            readme_file: None,
            license_file: None,
            source_dir: None,
            useful_files: Vec::new(),
        }
    }

    /// The display name of the project.
    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("Project")
    }

    /// The primary project type if any was detected.
    pub fn primary_type(&self) -> Option<ProjectType> {
        self.project_types.first().copied()
    }

    /// Whether project awareness is active.
    pub fn is_active(&self) -> bool {
        self.root.is_some()
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

    /// Compact summary string for UI rendering (e.g. `"[project: MyProject (Rust) | main | clean]"`).
    pub fn summary_string(&self, git_status: &GitStatus) -> Option<String> {
        let name = self.name.as_deref()?;
        let branch_disp = git_status.branch.display();
        let types = self.types_display();

        let type_suffix = if types.is_empty() {
            String::new()
        } else {
            format!(" ({types})")
        };

        if git_status.is_clean {
            Some(format!(
                "[project: {name}{type_suffix} | {branch_disp} | clean]"
            ))
        } else {
            let mut parts = Vec::new();
            if git_status.added_count > 0 {
                parts.push(format!("+{}", git_status.added_count));
            }
            if git_status.modified_count > 0 {
                parts.push(format!("~{}", git_status.modified_count));
            }
            if git_status.deleted_count > 0 {
                parts.push(format!("-{}", git_status.deleted_count));
            }
            if git_status.renamed_count > 0 {
                parts.push(format!("R{}", git_status.renamed_count));
            }
            if git_status.untracked_count > 0 {
                parts.push(format!("?{}", git_status.untracked_count));
            }
            let diff_str = parts.join(" ");
            Some(format!(
                "[project: {name}{type_suffix} | {branch_disp} | {diff_str}]"
            ))
        }
    }
}

/// Checks if `dir` contains any specific language or framework project markers.
fn has_project_markers(dir: &Path) -> bool {
    dir.join("Cargo.toml").is_file()
        || dir.join("package.json").is_file()
        || dir.join("pyproject.toml").is_file()
        || dir.join("requirements.txt").is_file()
        || dir.join("setup.py").is_file()
        || dir.join("pom.xml").is_file()
        || dir.join("build.gradle").is_file()
        || dir.join("build.gradle.kts").is_file()
        || dir.join("go.mod").is_file()
        || dir.join("CMakeLists.txt").is_file()
        || dir.join("Makefile").is_file()
        || dir.join("composer.json").is_file()
        || dir.join("Gemfile").is_file()
        || find_dotnet_marker(dir).is_some()
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

/// Searches `dir` for `.csproj` or `.sln` marker files.
fn find_dotnet_marker(dir: &Path) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                let lower = name.to_ascii_lowercase();
                if (lower.ends_with(".csproj") || lower.ends_with(".sln"))
                    && entry.file_type().map(|t| t.is_file()).unwrap_or(false)
                {
                    return Some(entry.path());
                }
            }
        }
    }
    None
}

/// Traverses up from `start_dir` to determine the project root directory.
///
/// Order of preference:
/// 1. Specific project marker root (nearest parent containing `Cargo.toml`, `package.json`, etc.)
/// 2. Git repository root if active
/// 3. Generic marker root (containing `README.md` or `LICENSE`)
pub fn detect_project_root(start_dir: &Path, git_root: Option<&Path>) -> Option<PathBuf> {
    if start_dir.as_os_str().is_empty() || !start_dir.exists() {
        return git_root.map(Path::to_path_buf);
    }

    let mut current = if start_dir.is_dir() {
        start_dir.to_path_buf()
    } else if let Some(parent) = start_dir.parent() {
        parent.to_path_buf()
    } else {
        return git_root.map(Path::to_path_buf);
    };

    let mut hops = 0;
    while hops < 12 {
        if has_project_markers(&current) {
            return Some(current);
        }

        if let Some(gr) = git_root
            && current == gr
        {
            if has_generic_markers(&current) || current.join(".git").exists() {
                return Some(current);
            }
            break;
        }

        if !current.pop() {
            break;
        }
        hops += 1;
    }

    // Fall back to git_root if known, or generic markers at start_dir
    if let Some(gr) = git_root {
        return Some(gr.to_path_buf());
    }

    if has_generic_markers(start_dir) {
        return Some(start_dir.to_path_buf());
    }

    None
}

/// Detects rich project information for a specific directory path and Git status.
pub fn detect_project_for_path(start_dir: &Path, git_status: &GitStatus) -> ProjectInfo {
    let root = match detect_project_root(start_dir, git_status.root()) {
        Some(r) => r,
        None => return ProjectInfo::inactive(),
    };

    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .map(ToString::to_string);

    let mut project_types = Vec::new();
    let mut manifest_file = None;
    let mut useful_files = Vec::new();

    // 1. Rust
    let cargo_toml = root.join("Cargo.toml");
    if cargo_toml.is_file() {
        project_types.push(ProjectType::Rust);
        if manifest_file.is_none() {
            manifest_file = Some(cargo_toml.clone());
        }
        useful_files.push(cargo_toml);
    }

    // 2. Node.js
    let package_json = root.join("package.json");
    if package_json.is_file() {
        project_types.push(ProjectType::Node);
        if manifest_file.is_none() {
            manifest_file = Some(package_json.clone());
        }
        useful_files.push(package_json);
    }

    // 3. Python
    let pyproject = root.join("pyproject.toml");
    let reqs = root.join("requirements.txt");
    let setup_py = root.join("setup.py");
    if pyproject.is_file() || reqs.is_file() || setup_py.is_file() {
        project_types.push(ProjectType::Python);
        if pyproject.is_file() {
            if manifest_file.is_none() {
                manifest_file = Some(pyproject.clone());
            }
            useful_files.push(pyproject);
        } else if reqs.is_file() {
            if manifest_file.is_none() {
                manifest_file = Some(reqs.clone());
            }
            useful_files.push(reqs);
        } else if setup_py.is_file() {
            if manifest_file.is_none() {
                manifest_file = Some(setup_py.clone());
            }
            useful_files.push(setup_py);
        }
    }

    // 4. Java
    let pom = root.join("pom.xml");
    let gradle = root.join("build.gradle");
    let gradle_kts = root.join("build.gradle.kts");
    if pom.is_file() || gradle.is_file() || gradle_kts.is_file() {
        project_types.push(ProjectType::Java);
        if pom.is_file() {
            if manifest_file.is_none() {
                manifest_file = Some(pom.clone());
            }
            useful_files.push(pom);
        } else if gradle.is_file() {
            if manifest_file.is_none() {
                manifest_file = Some(gradle.clone());
            }
            useful_files.push(gradle);
        } else if gradle_kts.is_file() {
            if manifest_file.is_none() {
                manifest_file = Some(gradle_kts.clone());
            }
            useful_files.push(gradle_kts);
        }
    }

    // 5. Go
    let go_mod = root.join("go.mod");
    if go_mod.is_file() {
        project_types.push(ProjectType::Go);
        if manifest_file.is_none() {
            manifest_file = Some(go_mod.clone());
        }
        useful_files.push(go_mod);
    }

    // 6. C/C++
    let cmake = root.join("CMakeLists.txt");
    let makefile = root.join("Makefile");
    if cmake.is_file() || makefile.is_file() {
        project_types.push(ProjectType::Cpp);
        if cmake.is_file() {
            if manifest_file.is_none() {
                manifest_file = Some(cmake.clone());
            }
            useful_files.push(cmake);
        } else if makefile.is_file() {
            if manifest_file.is_none() {
                manifest_file = Some(makefile.clone());
            }
            useful_files.push(makefile);
        }
    }

    // 7. PHP
    let composer = root.join("composer.json");
    if composer.is_file() {
        project_types.push(ProjectType::Php);
        if manifest_file.is_none() {
            manifest_file = Some(composer.clone());
        }
        useful_files.push(composer);
    }

    // 8. Ruby
    let gemfile = root.join("Gemfile");
    if gemfile.is_file() {
        project_types.push(ProjectType::Ruby);
        if manifest_file.is_none() {
            manifest_file = Some(gemfile.clone());
        }
        useful_files.push(gemfile);
    }

    // 9. .NET
    if let Some(dotnet_file) = find_dotnet_marker(&root) {
        project_types.push(ProjectType::DotNet);
        if manifest_file.is_none() {
            manifest_file = Some(dotnet_file.clone());
        }
        useful_files.push(dotnet_file);
    }

    // 10. Generic Documentation & License
    let mut readme_file = None;
    for name in &["README.md", "README", "README.txt", "README.rst"] {
        let p = root.join(name);
        if p.is_file() {
            readme_file = Some(p.clone());
            useful_files.push(p);
            break;
        }
    }

    let mut license_file = None;
    for name in &["LICENSE", "LICENSE.md", "LICENSE.txt", "COPYING"] {
        let p = root.join(name);
        if p.is_file() {
            license_file = Some(p.clone());
            useful_files.push(p);
            break;
        }
    }

    if project_types.is_empty() && (readme_file.is_some() || license_file.is_some()) {
        project_types.push(ProjectType::Generic);
    }

    // Source directory detection
    let mut source_dir = None;
    for src_candidate in &["src", "app", "lib", "include"] {
        let p = root.join(src_candidate);
        if p.is_dir() {
            source_dir = Some(p);
            break;
        }
    }

    ProjectInfo {
        root: Some(root),
        name,
        project_types,
        manifest_file,
        readme_file,
        license_file,
        source_dir,
        useful_files,
    }
}

/// Detects project awareness information based on `git_status`.
pub fn detect_project(git_status: &GitStatus) -> ProjectInfo {
    let start_dir = git_status.root().unwrap_or(Path::new(""));
    detect_project_for_path(start_dir, git_status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;
    use crate::git::status::compute_status;
    use std::fs;

    #[test]
    fn test_rust_project_detection() {
        let temp = TempDir::new("proj-rust");
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("README.md"), "# Title").unwrap();

        let status = compute_status(root);
        let proj = detect_project(&status);

        assert!(proj.is_active());
        assert_eq!(proj.root, Some(root.to_path_buf()));
        assert_eq!(proj.project_types, vec![ProjectType::Rust]);
        assert_eq!(proj.types_display(), "Rust");
        assert_eq!(proj.manifest_file, Some(root.join("Cargo.toml")));
        assert_eq!(proj.readme_file, Some(root.join("README.md")));
        assert_eq!(proj.source_dir, Some(root.join("src")));
    }

    #[test]
    fn test_node_project_detection() {
        let temp = TempDir::new("proj-node");
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::create_dir_all(root.join("app")).unwrap();

        let status = compute_status(root);
        let proj = detect_project(&status);

        assert!(proj.is_active());
        assert_eq!(proj.project_types, vec![ProjectType::Node]);
        assert_eq!(proj.types_display(), "Node.js");
        assert_eq!(proj.manifest_file, Some(root.join("package.json")));
        assert_eq!(proj.source_dir, Some(root.join("app")));
    }

    #[test]
    fn test_python_project_detection() {
        let temp = TempDir::new("proj-python");
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join("pyproject.toml"), "").unwrap();
        fs::write(root.join("requirements.txt"), "").unwrap();

        let status = compute_status(root);
        let proj = detect_project(&status);

        assert!(proj.is_active());
        assert_eq!(proj.project_types, vec![ProjectType::Python]);
        assert_eq!(proj.types_display(), "Python");
        assert_eq!(proj.manifest_file, Some(root.join("pyproject.toml")));
    }

    #[test]
    fn test_java_project_detection() {
        let temp = TempDir::new("proj-java");
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join("pom.xml"), "").unwrap();

        let status = compute_status(root);
        let proj = detect_project(&status);

        assert!(proj.is_active());
        assert_eq!(proj.project_types, vec![ProjectType::Java]);
        assert_eq!(proj.types_display(), "Java");
        assert_eq!(proj.manifest_file, Some(root.join("pom.xml")));
    }

    #[test]
    fn test_go_project_detection() {
        let temp = TempDir::new("proj-go");
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join("go.mod"), "module example").unwrap();

        let status = compute_status(root);
        let proj = detect_project(&status);

        assert!(proj.is_active());
        assert_eq!(proj.project_types, vec![ProjectType::Go]);
        assert_eq!(proj.types_display(), "Go");
        assert_eq!(proj.manifest_file, Some(root.join("go.mod")));
    }

    #[test]
    fn test_cpp_project_detection() {
        let temp = TempDir::new("proj-cpp");
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join("CMakeLists.txt"), "").unwrap();

        let status = compute_status(root);
        let proj = detect_project(&status);

        assert!(proj.is_active());
        assert_eq!(proj.project_types, vec![ProjectType::Cpp]);
        assert_eq!(proj.types_display(), "C/C++");
        assert_eq!(proj.manifest_file, Some(root.join("CMakeLists.txt")));
    }

    #[test]
    fn test_php_ruby_dotnet_detection() {
        let temp_php = TempDir::new("proj-php");
        fs::write(temp_php.path().join("composer.json"), "{}").unwrap();
        let status_php = compute_status(temp_php.path());
        let proj_php = detect_project_for_path(temp_php.path(), &status_php);
        assert_eq!(proj_php.project_types, vec![ProjectType::Php]);

        let temp_rb = TempDir::new("proj-rb");
        fs::write(
            temp_rb.path().join("Gemfile"),
            "source 'https://rubygems.org'",
        )
        .unwrap();
        let status_rb = compute_status(temp_rb.path());
        let proj_rb = detect_project_for_path(temp_rb.path(), &status_rb);
        assert_eq!(proj_rb.project_types, vec![ProjectType::Ruby]);

        let temp_net = TempDir::new("proj-net");
        fs::write(temp_net.path().join("App.csproj"), "<Project />").unwrap();
        let status_net = compute_status(temp_net.path());
        let proj_net = detect_project_for_path(temp_net.path(), &status_net);
        assert_eq!(proj_net.project_types, vec![ProjectType::DotNet]);
        assert_eq!(
            proj_net.manifest_file,
            Some(temp_net.path().join("App.csproj"))
        );
    }

    #[test]
    fn test_generic_project_detection() {
        let temp = TempDir::new("proj-generic");
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join("README.md"), "# Project").unwrap();
        fs::write(root.join("LICENSE"), "MIT").unwrap();

        let status = compute_status(root);
        let proj = detect_project(&status);

        assert!(proj.is_active());
        assert_eq!(proj.project_types, vec![ProjectType::Generic]);
        assert_eq!(proj.types_display(), "Generic");
        assert_eq!(proj.readme_file, Some(root.join("README.md")));
        assert_eq!(proj.license_file, Some(root.join("LICENSE")));
    }

    #[test]
    fn test_multiple_project_types() {
        let temp = TempDir::new("proj-multi");
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join("Cargo.toml"), "").unwrap();
        fs::write(root.join("package.json"), "").unwrap();

        let status = compute_status(root);
        let proj = detect_project(&status);

        assert!(proj.is_active());
        assert_eq!(
            proj.project_types,
            vec![ProjectType::Rust, ProjectType::Node]
        );
        assert_eq!(proj.types_display(), "Rust, Node.js");
    }

    #[test]
    fn test_no_project_inactive() {
        let temp = TempDir::new("no-proj");
        let root = temp.path();

        let status = compute_status(root);
        let proj = detect_project(&status);

        assert!(!proj.is_active());
        assert_eq!(proj, ProjectInfo::inactive());
        assert_eq!(proj.summary_string(&status), None);
    }

    #[test]
    fn test_nested_directory_inside_project() {
        let temp = TempDir::new("proj-nested");
        let root = temp.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join("Cargo.toml"), "").unwrap();

        let sub = root.join("src").join("ui");
        fs::create_dir_all(&sub).unwrap();

        let status = compute_status(&sub);
        let proj = detect_project_for_path(&sub, &status);

        assert!(proj.is_active());
        assert_eq!(proj.root, Some(root.to_path_buf()));
        assert_eq!(proj.project_types, vec![ProjectType::Rust]);
    }

    #[test]
    fn test_unicode_and_spaces_project_path() {
        let temp = TempDir::new("proj-unicode");
        let root = temp.path().join("📁 Proj 🦀 with spaces");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("Cargo.toml"), "").unwrap();

        let status = compute_status(&root);
        let proj = detect_project(&status);

        assert!(proj.is_active());
        assert_eq!(proj.name, Some("📁 Proj 🦀 with spaces".to_string()));
        assert_eq!(proj.project_types, vec![ProjectType::Rust]);
    }

    #[test]
    fn test_linux_project_layout_detection() {
        let temp = TempDir::new("proj-linux-layout");
        let root = temp.path().join("opt").join("my_service");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("Makefile"), "all: build\n").unwrap();
        fs::write(root.join("requirements.txt"), "flask\n").unwrap();

        let status = compute_status(&root);
        let proj = detect_project(&status);

        assert!(proj.is_active());
        assert_eq!(proj.name, Some("my_service".to_string()));
        assert_eq!(
            proj.project_types,
            vec![ProjectType::Python, ProjectType::Cpp]
        );
    }

    #[test]
    fn test_project_without_git_repository() {
        let temp = TempDir::new("proj-no-git");
        let root = temp.path();
        fs::write(root.join("Cargo.toml"), "[package]").unwrap();
        let sub = root.join("src");
        fs::create_dir_all(&sub).unwrap();

        let status = compute_status(&sub);
        assert!(!status.is_repo());

        let proj = detect_project_for_path(&sub, &status);
        assert!(proj.is_active());
        assert_eq!(proj.root, Some(root.to_path_buf()));
        assert_eq!(proj.project_types, vec![ProjectType::Rust]);
        assert_eq!(proj.manifest_file, Some(root.join("Cargo.toml")));
    }

    #[test]
    fn test_worktree_dot_git_file() {
        let temp = TempDir::new("proj-worktree");
        let root = temp.path();
        // A git worktree has a .git file containing `gitdir: ...`
        fs::write(
            root.join(".git"),
            "gitdir: /path/to/main/.git/worktrees/wt1\n",
        )
        .unwrap();
        fs::write(root.join("Cargo.toml"), "[package]").unwrap();

        let status = compute_status(root);
        let proj = detect_project_for_path(root, &status);
        assert!(proj.is_active());
        assert_eq!(proj.project_types, vec![ProjectType::Rust]);
    }

    #[test]
    fn test_deep_parent_traversal_detection() {
        let temp = TempDir::new("proj-deep");
        let root = temp.path();
        fs::write(root.join("go.mod"), "module deep\n").unwrap();

        let mut current = root.to_path_buf();
        for i in 0..8 {
            current = current.join(format!("level_{i}"));
        }
        fs::create_dir_all(&current).unwrap();

        let status = compute_status(&current);
        let proj = detect_project_for_path(&current, &status);
        assert!(proj.is_active());
        assert_eq!(proj.root, Some(root.to_path_buf()));
        assert_eq!(proj.project_types, vec![ProjectType::Go]);
    }
}
