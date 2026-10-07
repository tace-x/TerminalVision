//! Comprehensive integration tests for Workspace Structure & Project Graph (Phase 1.2).
//!
//! Tests the 20 required scenarios:
//! 1. Simple Rust project
//! 2. Simple Node project
//! 3. Simple Python project
//! 4. Nested frontend/backend projects
//! 5. Rust workspace (monorepo with multiple crates)
//! 6. Node workspace (pnpm-workspace / lerna monorepo)
//! 7. Project with source / tests / docs
//! 8. Project with build output
//! 9. Project with generated / cache directories
//! 10. Project with missing optional directories
//! 11. Multiple manifests at root
//! 12. Nested project inside parent project
//! 13. Symlink loop protection
//! 14. Permission failure graceful handling
//! 15. Huge-directory bounded scanning protection
//! 16. Non-project directory
//! 17. Filesystem root
//! 18. Project path deep inside source
//! 19. Project path inside build output
//! 20. Cache reuse and invalidation

use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;

use terminalvision::project::{
    DirectoryRole, ImportantFileRole, Language, ProjectCache, ProjectType, analyze_workspace,
};

// ---------------------------------------------------------------------------
// 1. Simple Rust Project
// ---------------------------------------------------------------------------
#[test]
fn test_01_simple_rust_project() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("Cargo.toml"), "[package]\nname = \"tv\"\n").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("main.rs"), "fn main() {}\n").unwrap();

    let ws = analyze_workspace(root);

    assert!(ws.is_active());
    assert_eq!(ws.project_root(), Some(root));
    assert!(!ws.is_monorepo());
    assert_eq!(ws.projects().len(), 1);

    let proj = &ws.projects()[0];
    assert_eq!(proj.project_type, ProjectType::Rust);
    assert_eq!(proj.primary_source_dir(), Some(root.join("src").as_path()));
    assert_eq!(ws.directory_role(&root.join("src")), DirectoryRole::Source);
}

// ---------------------------------------------------------------------------
// 2. Simple Node Project
// ---------------------------------------------------------------------------
#[test]
fn test_02_simple_node_project() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("package.json"), "{\"name\": \"app\"}").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("node_modules")).unwrap();

    let ws = analyze_workspace(root);

    assert!(ws.is_active());
    assert_eq!(
        ws.directory_role(&root.join("node_modules")),
        DirectoryRole::Dependencies
    );
    let manifests: Vec<_> = ws
        .important_files()
        .iter()
        .filter(|f| f.role == ImportantFileRole::Manifest)
        .collect();
    assert!(!manifests.is_empty());
}

// ---------------------------------------------------------------------------
// 3. Simple Python Project
// ---------------------------------------------------------------------------
#[test]
fn test_03_simple_python_project() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("pyproject.toml"), "[tool.poetry]").unwrap();
    fs::create_dir_all(root.join("app")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();

    let ws = analyze_workspace(root);

    assert!(ws.is_active());
    assert_eq!(ws.directory_role(&root.join("app")), DirectoryRole::Source);
    assert_eq!(ws.directory_role(&root.join("tests")), DirectoryRole::Tests);
    assert_eq!(ws.projects()[0].languages, vec![Language::Python]);
}

// ---------------------------------------------------------------------------
// 4. Nested Frontend / Backend Projects
// ---------------------------------------------------------------------------
#[test]
fn test_04_nested_frontend_backend_projects() {
    let dir = tempdir().expect("create tempdir");
    let ws_root = dir.path();

    fs::create_dir(ws_root.join(".git")).unwrap();

    // Frontend (Node)
    let fe = ws_root.join("frontend");
    fs::create_dir_all(&fe).unwrap();
    fs::write(fe.join("package.json"), "{\"name\":\"fe\"}").unwrap();
    fs::create_dir_all(fe.join("src")).unwrap();

    // Backend (Rust)
    let be = ws_root.join("backend");
    fs::create_dir_all(&be).unwrap();
    fs::write(be.join("Cargo.toml"), "[package]\nname = \"be\"").unwrap();
    fs::create_dir_all(be.join("src")).unwrap();

    let ws = analyze_workspace(ws_root);

    assert!(ws.is_active());
    assert!(ws.is_monorepo());
    assert_eq!(ws.projects().len(), 2);

    let fe_proj = ws.find_project("frontend").expect("frontend project");
    assert_eq!(fe_proj.project_type, ProjectType::Node);

    let be_proj = ws.find_project("backend").expect("backend project");
    assert_eq!(be_proj.project_type, ProjectType::Rust);

    // Query project for path deep inside frontend
    let deep_fe_file = fe.join("src").join("App.tsx");
    let found = ws.project_for_path(&deep_fe_file).expect("match project");
    assert_eq!(found.id, "frontend");
}

// ---------------------------------------------------------------------------
// 5. Rust Workspace (Crates Monorepo)
// ---------------------------------------------------------------------------
#[test]
fn test_05_rust_workspace() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .unwrap();

    let crate_a = root.join("crates").join("crate-a");
    fs::create_dir_all(&crate_a).unwrap();
    fs::write(crate_a.join("Cargo.toml"), "[package]\nname = \"crate-a\"").unwrap();
    fs::create_dir_all(crate_a.join("src")).unwrap();

    let crate_b = root.join("crates").join("crate-b");
    fs::create_dir_all(&crate_b).unwrap();
    fs::write(crate_b.join("Cargo.toml"), "[package]\nname = \"crate-b\"").unwrap();
    fs::create_dir_all(crate_b.join("src")).unwrap();

    let ws = analyze_workspace(root);

    assert!(ws.is_active());
    assert!(ws.is_monorepo());
    assert!(ws.projects().len() >= 2);
}

// ---------------------------------------------------------------------------
// 6. Node Workspace (pnpm / lerna)
// ---------------------------------------------------------------------------
#[test]
fn test_06_node_workspace() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(
        root.join("pnpm-workspace.yaml"),
        "packages:\n  - 'packages/*'\n",
    )
    .unwrap();
    fs::write(root.join("package.json"), "{\"private\":true}").unwrap();

    let pkg1 = root.join("packages").join("ui");
    fs::create_dir_all(&pkg1).unwrap();
    fs::write(pkg1.join("package.json"), "{\"name\":\"@app/ui\"}").unwrap();
    fs::create_dir_all(pkg1.join("src")).unwrap();

    let ws = analyze_workspace(root);

    assert!(ws.is_active());
    assert!(ws.is_monorepo());
    assert!(ws.find_project("packages/ui").is_some());
}

// ---------------------------------------------------------------------------
// 7. Project with Source / Tests / Docs
// ---------------------------------------------------------------------------
#[test]
fn test_07_project_with_source_tests_docs() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("Cargo.toml"), "[package]").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("README.md"), "# Project").unwrap();
    fs::write(root.join("LICENSE"), "MIT").unwrap();

    let ws = analyze_workspace(root);

    assert_eq!(ws.source_directories(), vec![root.join("src").as_path()]);
    assert_eq!(ws.test_directories(), vec![root.join("tests").as_path()]);
    assert_eq!(ws.documentation(), vec![root.join("docs").as_path()]);
    assert_eq!(
        ws.directory_role(&root.join("docs")),
        DirectoryRole::Documentation
    );
}

// ---------------------------------------------------------------------------
// 8. Project with Build Output
// ---------------------------------------------------------------------------
#[test]
fn test_08_project_with_build_output() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("Cargo.toml"), "[package]").unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::create_dir_all(root.join("dist")).unwrap();

    let ws = analyze_workspace(root);

    assert_eq!(
        ws.directory_role(&root.join("target")),
        DirectoryRole::BuildOutput
    );
    assert_eq!(
        ws.directory_role(&root.join("dist")),
        DirectoryRole::BuildOutput
    );
}

// ---------------------------------------------------------------------------
// 9. Project with Generated / Cache Directories
// ---------------------------------------------------------------------------
#[test]
fn test_09_project_with_generated_and_cache_directories() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("package.json"), "{}").unwrap();
    fs::create_dir_all(root.join(".next")).unwrap();
    fs::create_dir_all(root.join(".cache")).unwrap();

    let ws = analyze_workspace(root);

    assert_eq!(
        ws.directory_role(&root.join(".next")),
        DirectoryRole::Generated
    );
    assert_eq!(
        ws.directory_role(&root.join(".cache")),
        DirectoryRole::Cache
    );
    assert!(
        ws.directory_role(&root.join(".cache"))
            .is_ignored_by_default()
    );
}

// ---------------------------------------------------------------------------
// 10. Project with Missing Optional Directories
// ---------------------------------------------------------------------------
#[test]
fn test_10_missing_optional_directories() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    // Bare minimum rust project without tests/docs/target
    fs::write(root.join("Cargo.toml"), "[package]").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();

    let ws = analyze_workspace(root);

    assert!(ws.is_active());
    assert_eq!(ws.test_directories().len(), 0);
    assert_eq!(ws.documentation().len(), 0);
}

// ---------------------------------------------------------------------------
// 11. Multiple Manifests
// ---------------------------------------------------------------------------
#[test]
fn test_11_multiple_manifests() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("Cargo.toml"), "[package]").unwrap();
    fs::write(root.join("package.json"), "{}").unwrap();
    fs::write(root.join("Makefile"), "all:\n").unwrap();
    fs::write(root.join("Dockerfile"), "FROM alpine\n").unwrap();

    let ws = analyze_workspace(root);

    let roles: Vec<_> = ws.important_files().iter().map(|f| f.role).collect();
    assert!(roles.contains(&ImportantFileRole::Manifest));
    assert!(roles.contains(&ImportantFileRole::Build));
    assert!(roles.contains(&ImportantFileRole::Container));
}

// ---------------------------------------------------------------------------
// 12. Nested Project Inside Parent Project
// ---------------------------------------------------------------------------
#[test]
fn test_12_nested_project_inside_parent_project() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    // Root project
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"parent\"").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();

    // Embedded child project
    let child = root.join("tools").join("helper");
    fs::create_dir_all(&child).unwrap();
    fs::write(child.join("package.json"), "{\"name\": \"helper\"}").unwrap();
    fs::create_dir_all(child.join("src")).unwrap();

    let ws = analyze_workspace(root);

    assert!(ws.is_active());
    let helper_proj = ws.project_for_path(&child).expect("find helper");
    assert_eq!(helper_proj.project_type, ProjectType::Node);
}

// ---------------------------------------------------------------------------
// 13. Symlink Loop Protection
// ---------------------------------------------------------------------------
#[test]
fn test_13_symlink_loop_protection() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let dir = tempdir().expect("create tempdir");
        let root = dir.path();

        fs::write(root.join("Cargo.toml"), "[package]").unwrap();
        let loop_dir = root.join("loop_dir");
        let _ = symlink(root, &loop_dir);

        // Must terminate rapidly and safely without infinite traversal
        let ws = analyze_workspace(root);
        assert!(ws.is_active());
    }
}

// ---------------------------------------------------------------------------
// 14. Permission Failure
// ---------------------------------------------------------------------------
#[test]
fn test_14_permission_failure() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().expect("create tempdir");
        let root = dir.path();

        fs::write(root.join("Cargo.toml"), "[package]").unwrap();
        let restricted = root.join("secret_docs");
        fs::create_dir(&restricted).unwrap();

        let mut perms = fs::metadata(&restricted).unwrap().permissions();
        perms.set_mode(0o000);
        let _ = fs::set_permissions(&restricted, perms.clone());

        // Must not panic
        let ws = analyze_workspace(root);
        assert!(ws.is_active());

        perms.set_mode(0o755);
        let _ = fs::set_permissions(&restricted, perms);
    }
}

// ---------------------------------------------------------------------------
// 15. Huge Directory Bounded Scanning Protection
// ---------------------------------------------------------------------------
#[test]
fn test_15_huge_directory_protection() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("Cargo.toml"), "[package]").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();

    // Create 1,500 dummy files
    for i in 0..1500 {
        let _ = fs::write(root.join(format!("file_{i:04}.dat")), "x");
    }

    let start = std::time::Instant::now();
    let ws = analyze_workspace(root);
    let elapsed = start.elapsed();

    assert!(ws.is_active());
    assert!(elapsed < Duration::from_millis(500));
}

// ---------------------------------------------------------------------------
// 16. Non-Project Directory
// ---------------------------------------------------------------------------
#[test]
fn test_16_non_project_directory() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("notes.txt"), "plain notes").unwrap();

    let ws = analyze_workspace(root);
    assert!(!ws.is_active());
    assert_eq!(ws.projects().len(), 0);
    assert_eq!(ws.name(), "Workspace");
}

// ---------------------------------------------------------------------------
// 17. Filesystem Root
// ---------------------------------------------------------------------------
#[test]
fn test_17_filesystem_root() {
    let fs_root = Path::new("/");
    let ws = analyze_workspace(fs_root);
    // Must execute safely without panicking
    let _ = ws.is_active();
}

// ---------------------------------------------------------------------------
// 18. Project Path Deep Inside Source
// ---------------------------------------------------------------------------
#[test]
fn test_18_project_path_deep_inside_source() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("Cargo.toml"), "[package]").unwrap();
    let deep_src = root.join("src").join("ui").join("widgets").join("nested");
    fs::create_dir_all(&deep_src).unwrap();

    let ws = analyze_workspace(&deep_src);

    assert!(ws.is_active());
    assert_eq!(ws.project_root(), Some(root));
    let proj = ws.project_for_path(&deep_src).expect("project found");
    assert_eq!(proj.project_type, ProjectType::Rust);
}

// ---------------------------------------------------------------------------
// 19. Project Path Inside Build Output
// ---------------------------------------------------------------------------
#[test]
fn test_19_project_path_inside_build_output() {
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("Cargo.toml"), "[package]").unwrap();
    let deep_target = root.join("target").join("debug").join("deps");
    fs::create_dir_all(&deep_target).unwrap();

    let ws = analyze_workspace(&deep_target);

    assert!(ws.is_active());
    assert_eq!(ws.project_root(), Some(root));
}

// ---------------------------------------------------------------------------
// 20. Cache Reuse and Invalidation
// ---------------------------------------------------------------------------
#[test]
fn test_20_cache_reuse_and_invalidation() {
    let mut cache = ProjectCache::with_ttl(Duration::from_millis(50));
    let dir = tempdir().expect("create tempdir");
    let root = dir.path();

    fs::write(root.join("Cargo.toml"), "[package]").unwrap();

    // Cache miss -> analyze & insert
    let ws1 = cache.get_or_analyze_workspace(root);
    assert!(ws1.is_active());

    // Cache hit
    let ws2 = cache.get_workspace(root).expect("workspace cache hit");
    assert_eq!(ws1.project_root(), ws2.project_root());

    // Invalidation by path
    cache.invalidate(root);
    assert!(cache.get_workspace(root).is_none());

    // Re-insert & invalidate by root
    cache.get_or_analyze_workspace(root);
    cache.invalidate_root(root);
    assert!(cache.get_workspace(root).is_none());

    // TTL expiration
    cache.get_or_analyze_workspace(root);
    std::thread::sleep(Duration::from_millis(60));
    assert!(cache.get_workspace(root).is_none());
}
