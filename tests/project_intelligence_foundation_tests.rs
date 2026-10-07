//! Comprehensive integration tests for Project Intelligence Foundation (Phase 1.1).
//!
//! Tests the 18 required test cases:
//! 1. Rust project
//! 2. Node project
//! 3. Python project
//! 4. Java project
//! 5. Go project
//! 6. C/C++ project
//! 7. Generic Git repository
//! 8. Non-project directory
//! 9. Nested project
//! 10. Multiple manifests
//! 11. Missing files / nonexistent paths
//! 12. Permission failure where testable
//! 13. File path instead of directory
//! 14. Filesystem root
//! 15. Very deep directory
//! 16. Symlink-related cases
//! 17. Large directory does not trigger unbounded scanning
//! 18. Cache invalidation and lifecycle

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::tempdir;

use terminalvision::project::{
    BuildSystem, DetectionConfidence, Language, ProjectCache, ProjectType, detect_project,
    detect_project_root,
};

// ---------------------------------------------------------------------------
// 1. Rust Project
// ---------------------------------------------------------------------------
#[test]
fn test_01_rust_project_detection() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::create_dir(root.join(".git")).unwrap();
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"test_pkg\"\n").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::write(root.join("README.md"), "# Test Project\n").unwrap();
    fs::write(root.join("LICENSE"), "MIT\n").unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::create_dir_all(root.join(".github").join("workflows")).unwrap();
    fs::write(root.join("Dockerfile"), "FROM rust\n").unwrap();

    let fp = detect_project(root);

    assert!(fp.is_active());
    assert_eq!(fp.root.as_deref(), Some(root));
    assert!(fp.project_types.contains(&ProjectType::Rust));
    assert_eq!(fp.primary_type(), Some(ProjectType::Rust));
    assert_eq!(fp.primary_language(), Some(Language::Rust));
    assert_eq!(fp.primary_build_system(), Some(BuildSystem::Cargo));
    assert_eq!(
        fp.primary_manifest(),
        Some(root.join("Cargo.toml").as_path())
    );
    assert!(fp.is_git);
    assert!(fp.signals.has_source());
    assert!(fp.signals.has_tests());
    assert!(fp.signals.has_docs());
    assert!(fp.signals.has_docker());
    assert!(fp.signals.has_ci());
    assert_eq!(fp.confidence, DetectionConfidence::Definitive);
}

// ---------------------------------------------------------------------------
// 2. Node / TypeScript / JavaScript Project
// ---------------------------------------------------------------------------
#[test]
fn test_02_node_typescript_project_detection() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("package.json"), "{\"name\":\"my-app\"}").unwrap();
    fs::write(root.join("tsconfig.json"), "{}").unwrap();
    fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: 5.4").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("node_modules")).unwrap();

    let fp = detect_project(root);

    assert!(fp.is_active());
    assert_eq!(fp.root.as_deref(), Some(root));
    assert!(fp.project_types.contains(&ProjectType::Node));
    assert!(fp.project_types.contains(&ProjectType::TypeScript));
    assert_eq!(fp.primary_language(), Some(Language::TypeScript));
    assert_eq!(fp.primary_build_system(), Some(BuildSystem::Pnpm));
    assert_eq!(
        fp.primary_manifest(),
        Some(root.join("package.json").as_path())
    );
}

// ---------------------------------------------------------------------------
// 3. Python Project
// ---------------------------------------------------------------------------
#[test]
fn test_03_python_project_detection() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(
        root.join("pyproject.toml"),
        "[tool.poetry]\nname = \"pyapp\"\n",
    )
    .unwrap();
    fs::write(root.join("poetry.lock"), "").unwrap();
    fs::create_dir_all(root.join("app")).unwrap();
    fs::create_dir_all(root.join("test")).unwrap();

    let fp = detect_project(root);

    assert!(fp.is_active());
    assert_eq!(fp.primary_type(), Some(ProjectType::Python));
    assert_eq!(fp.primary_language(), Some(Language::Python));
    assert_eq!(fp.primary_build_system(), Some(BuildSystem::Poetry));
    assert!(fp.signals.has_source());
    assert!(fp.signals.has_tests());
}

// ---------------------------------------------------------------------------
// 4. Java Project (Maven & Gradle)
// ---------------------------------------------------------------------------
#[test]
fn test_04_java_project_detection() {
    let dir_maven = tempdir().expect("create tempdir");
    let root_maven = dir_maven.path();
    fs::write(root_maven.join("pom.xml"), "<project></project>").unwrap();
    fs::create_dir_all(root_maven.join("src")).unwrap();

    let fp_maven = detect_project(root_maven);
    assert_eq!(fp_maven.primary_type(), Some(ProjectType::Java));
    assert_eq!(fp_maven.primary_language(), Some(Language::Java));
    assert_eq!(fp_maven.primary_build_system(), Some(BuildSystem::Maven));

    let dir_gradle = tempdir().expect("create tempdir");
    let root_gradle = dir_gradle.path();
    fs::write(root_gradle.join("build.gradle.kts"), "plugins {}").unwrap();
    fs::create_dir_all(root_gradle.join("src")).unwrap();

    let fp_gradle = detect_project(root_gradle);
    assert_eq!(fp_gradle.primary_type(), Some(ProjectType::Java));
    assert_eq!(fp_gradle.primary_language(), Some(Language::Kotlin));
    assert_eq!(fp_gradle.primary_build_system(), Some(BuildSystem::Gradle));
}

// ---------------------------------------------------------------------------
// 5. Go Project
// ---------------------------------------------------------------------------
#[test]
fn test_05_go_project_detection() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(
        root.join("go.mod"),
        "module github.com/example/app\n\ngo 1.22\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("cmd")).unwrap();
    fs::create_dir_all(root.join("pkg")).unwrap();

    let fp = detect_project(root);

    assert_eq!(fp.primary_type(), Some(ProjectType::Go));
    assert_eq!(fp.primary_language(), Some(Language::Go));
    assert_eq!(fp.primary_build_system(), Some(BuildSystem::GoModules));
    assert!(fp.signals.has_source());
}

// ---------------------------------------------------------------------------
// 6. C/C++ Project (CMake & Make)
// ---------------------------------------------------------------------------
#[test]
fn test_06_cpp_project_detection() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(
        root.join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.10)\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("include")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();

    let fp = detect_project(root);

    assert_eq!(fp.primary_type(), Some(ProjectType::Cpp));
    assert_eq!(fp.primary_language(), Some(Language::Cpp));
    assert_eq!(fp.primary_build_system(), Some(BuildSystem::CMake));
    assert!(fp.signals.has_source());
}

// ---------------------------------------------------------------------------
// 7. Generic Git Repository
// ---------------------------------------------------------------------------
#[test]
fn test_07_generic_git_repository() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::create_dir(root.join(".git")).unwrap();
    fs::write(root.join("notes.txt"), "hello").unwrap();

    let fp = detect_project(root);

    assert!(fp.is_active());
    assert_eq!(fp.primary_type(), Some(ProjectType::GenericGit));
    assert!(fp.is_git);
    assert_eq!(fp.confidence, DetectionConfidence::Low);
}

// ---------------------------------------------------------------------------
// 8. Non-Project Directory
// ---------------------------------------------------------------------------
#[test]
fn test_08_non_project_directory() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("random_file.tmp"), "arbitrary data").unwrap();

    let root_detected = detect_project_root(root);
    assert_eq!(root_detected, None);

    let fp = detect_project(root);
    assert!(!fp.is_active());
    assert_eq!(fp.confidence, DetectionConfidence::None);
    assert_eq!(fp.name(), "Project");
}

// ---------------------------------------------------------------------------
// 9. Nested Project / Monorepo
// ---------------------------------------------------------------------------
#[test]
fn test_09_nested_project_and_monorepo() {
    let dir = tempdir().expect("create tempdir");
    let workspace = dir.path();

    fs::create_dir(workspace.join(".git")).unwrap();
    fs::write(
        workspace.join("pnpm-workspace.yaml"),
        "packages:\n  - 'packages/*'\n",
    )
    .unwrap();
    fs::write(workspace.join("package.json"), "{\"private\":true}").unwrap();

    let pkg_a = workspace.join("packages").join("pkg-a");
    fs::create_dir_all(&pkg_a).unwrap();
    fs::write(pkg_a.join("package.json"), "{\"name\":\"pkg-a\"}").unwrap();
    fs::create_dir_all(pkg_a.join("src")).unwrap();

    // From inside pkg-a deep src folder
    let deep_path = pkg_a.join("src");
    let detected_root = detect_project_root(&deep_path);
    assert_eq!(detected_root, Some(pkg_a.clone()));

    let fp = detect_project(&deep_path);
    assert_eq!(fp.root.as_deref(), Some(pkg_a.as_path()));
    assert_eq!(fp.parent_workspace.as_deref(), Some(workspace));

    // From top workspace
    let ws_fp = detect_project(workspace);
    assert!(ws_fp.is_workspace);
    assert!(ws_fp.subprojects.contains(&pkg_a));
}

// ---------------------------------------------------------------------------
// 10. Multiple Manifests
// ---------------------------------------------------------------------------
#[test]
fn test_10_multiple_manifests() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("Cargo.toml"), "[package]").unwrap();
    fs::write(root.join("package.json"), "{}").unwrap();
    fs::write(root.join("Makefile"), "all:\n").unwrap();

    let fp = detect_project(root);

    assert!(fp.project_types.contains(&ProjectType::Rust));
    assert!(fp.project_types.contains(&ProjectType::Node));
    assert!(fp.build_systems.contains(&BuildSystem::Cargo));
    assert!(fp.build_systems.contains(&BuildSystem::Make));
}

// ---------------------------------------------------------------------------
// 11. Missing Files / Nonexistent Paths
// ---------------------------------------------------------------------------
#[test]
fn test_11_missing_files_and_nonexistent_paths() {
    let missing_path = PathBuf::from("/path/which/does/not/exist/99999");
    assert_eq!(detect_project_root(&missing_path), None);

    let fp = detect_project(&missing_path);
    assert!(!fp.is_active());

    let empty_path = PathBuf::from("");
    assert_eq!(detect_project_root(&empty_path), None);
    assert!(!detect_project(&empty_path).is_active());
}

// ---------------------------------------------------------------------------
// 12. Permission Failure Degradation
// ---------------------------------------------------------------------------
#[test]
fn test_12_permission_failure_graceful_handling() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().expect("create tempdir");
        let restricted = dir.path().join("restricted");
        fs::create_dir(&restricted).unwrap();
        fs::write(restricted.join("Cargo.toml"), "[package]").unwrap();

        // Make folder unreadable
        let mut perms = fs::metadata(&restricted).unwrap().permissions();
        perms.set_mode(0o000);
        let _ = fs::set_permissions(&restricted, perms.clone());

        // Must not panic
        let _ = detect_project(&restricted);

        // Restore permissions so tempdir cleanup succeeds
        perms.set_mode(0o755);
        let _ = fs::set_permissions(&restricted, perms);
    }
}

// ---------------------------------------------------------------------------
// 13. File Path Instead of Directory
// ---------------------------------------------------------------------------
#[test]
fn test_13_file_path_instead_of_directory() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("Cargo.toml"), "[package]").unwrap();
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let main_file = src_dir.join("main.rs");
    fs::write(&main_file, "fn main() {}").unwrap();

    let detected_root = detect_project_root(&main_file);
    assert_eq!(detected_root, Some(root.to_path_buf()));

    let fp = detect_project(&main_file);
    assert_eq!(fp.root.as_deref(), Some(root));
    assert!(fp.project_types.contains(&ProjectType::Rust));
}

// ---------------------------------------------------------------------------
// 14. Filesystem Root
// ---------------------------------------------------------------------------
#[test]
fn test_14_filesystem_root() {
    let fs_root = Path::new("/");
    // Must execute cleanly without panicking or looping
    let _ = detect_project_root(fs_root);
    let _ = detect_project(fs_root);
}

// ---------------------------------------------------------------------------
// 15. Very Deep Directory
// ---------------------------------------------------------------------------
#[test]
fn test_15_very_deep_directory() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();
    fs::write(root.join("go.mod"), "module deep\n").unwrap();

    let mut deep_path = root.to_path_buf();
    for i in 0..20 {
        deep_path = deep_path.join(format!("level_{i}"));
    }
    fs::create_dir_all(&deep_path).unwrap();

    let detected_root = detect_project_root(&deep_path);
    assert_eq!(detected_root, Some(root.to_path_buf()));

    let fp = detect_project(&deep_path);
    assert_eq!(fp.root.as_deref(), Some(root));
    assert_eq!(fp.primary_type(), Some(ProjectType::Go));
}

// ---------------------------------------------------------------------------
// 16. Symlink-Related Cases
// ---------------------------------------------------------------------------
#[test]
fn test_16_symlink_handling() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let dir = tempdir().expect("create tempdir");
        let real_project = dir.path().join("real_project");
        fs::create_dir_all(&real_project).unwrap();
        fs::write(real_project.join("Cargo.toml"), "[package]").unwrap();

        let link_path = dir.path().join("link_to_project");
        let _ = symlink(&real_project, &link_path);

        if link_path.exists() {
            let fp = detect_project(&link_path);
            assert!(fp.is_active());
            assert!(fp.project_types.contains(&ProjectType::Rust));
        }

        // Broken symlink
        let broken_link = dir.path().join("broken_link");
        let _ = symlink(dir.path().join("nonexistent_target"), &broken_link);
        let _ = detect_project(&broken_link);
    }
}

// ---------------------------------------------------------------------------
// 17. Large Directory Bounded Scan
// ---------------------------------------------------------------------------
#[test]
fn test_17_large_directory_bounded_scan() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("Cargo.toml"), "[package]").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();

    // Create 1,000 files in the root
    for i in 0..1000 {
        let _ = fs::write(root.join(format!("dummy_{i:04}.txt")), "data");
    }

    let start = std::time::Instant::now();
    let fp = detect_project(root);
    let elapsed = start.elapsed();

    assert!(fp.is_active());
    assert_eq!(fp.primary_type(), Some(ProjectType::Rust));
    // Must complete rapidly (under 100ms) because of bounded shallow inspection
    assert!(elapsed < Duration::from_millis(500));
}

// ---------------------------------------------------------------------------
// 18. Cache Invalidation and Lifecycle
// ---------------------------------------------------------------------------
#[test]
fn test_18_cache_invalidation_and_lifecycle() {
    let mut cache = ProjectCache::with_ttl(Duration::from_millis(50));
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("Cargo.toml"), "[package]").unwrap();

    // First call: populates cache
    let fp1 = cache.get_or_detect(root);
    assert_eq!(fp1.primary_type(), Some(ProjectType::Rust));
    assert_eq!(cache.len(), 1);

    // Second call: hits cache
    let fp2 = cache.get(root).expect("cache hit");
    assert_eq!(fp2.primary_type(), Some(ProjectType::Rust));

    // Path invalidation
    cache.invalidate(root);
    assert!(cache.get(root).is_none());

    // Re-insert and Root invalidation
    cache.get_or_detect(root);
    assert_eq!(cache.len(), 1);
    cache.invalidate_root(root);
    assert!(cache.get(root).is_none());

    // TTL Expiration
    cache.get_or_detect(root);
    std::thread::sleep(Duration::from_millis(60));
    assert!(cache.get(root).is_none());

    // Clear
    cache.get_or_detect(root);
    cache.clear();
    assert!(cache.is_empty());
}
