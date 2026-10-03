use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tempfile::TempDir;

use terminalvision::app::actions::Action;
use terminalvision::app::modes::Mode;
use terminalvision::app::state::{ActivePane, App};
use terminalvision::filesystem::classification::{
    FileCategory, FileIntelligence, classify_file_by_extension_and_magic,
};
use terminalvision::operations::conflict::{
    ConflictPrompt, ConflictResolution, generate_unique_rename,
};
use terminalvision::operations::manager::OperationManager;
use terminalvision::operations::progress::OperationMetrics;
use terminalvision::preview::image::TerminalGraphicsProtocol;
use terminalvision::preview::{PreviewContent, load_preview};
use terminalvision::ui::render;

// =========================================================================
// PART 1 — FILE INTELLIGENCE & CLASSIFICATION TESTS
// =========================================================================

#[test]
fn test_01_file_category_classification_by_extension() {
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("main.rs")).0,
        FileCategory::SourceCode
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("script.py")).0,
        FileCategory::SourceCode
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("index.ts")).0,
        FileCategory::SourceCode
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("photo.png")).0,
        FileCategory::Image
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("photo.jpg")).0,
        FileCategory::Image
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("video.mp4")).0,
        FileCategory::Video
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("song.mp3")).0,
        FileCategory::Audio
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("manual.pdf")).0,
        FileCategory::Pdf
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("bundle.zip")).0,
        FileCategory::Archive
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("archive.tar")).0,
        FileCategory::Archive
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("dataset.csv")).0,
        FileCategory::Data
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("config.toml")).0,
        FileCategory::Config
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("config.yaml")).0,
        FileCategory::Config
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("readme.md")).0,
        FileCategory::Document
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("notes.txt")).0,
        FileCategory::Text
    );
    assert_eq!(
        classify_file_by_extension_and_magic(Path::new("unknown_xyz")).0,
        FileCategory::Unknown
    );
}

#[test]
fn test_02_file_intelligence_inspect_regular_file_and_directory() {
    let temp = TempDir::new().unwrap();
    let file_path = temp.path().join("main.rs");
    fs::write(&file_path, "fn main() { println!(\"hello\"); }\n").unwrap();

    let intel = FileIntelligence::from_path(&file_path);
    assert_eq!(intel.category, FileCategory::SourceCode);
    assert!(!intel.is_directory);
    assert_eq!(intel.size, 33);
    assert!(intel.mime_hint.contains("rust") || intel.mime_hint.contains("text"));
    assert!(intel.modified.is_some());

    let dir_intel = FileIntelligence::from_path(temp.path());
    assert_eq!(dir_intel.category, FileCategory::Directory);
    assert!(dir_intel.is_directory);
}

#[test]
fn test_03_file_intelligence_magic_bytes_detection() {
    let temp = TempDir::new().unwrap();

    // PNG magic bytes: \x89PNG\r\n\x1a\n
    let png_no_ext = temp.path().join("image_without_ext");
    let mut png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    png_data.extend_from_slice(&[0u8; 32]);
    fs::write(&png_no_ext, png_data).unwrap();

    let intel = FileIntelligence::from_path(&png_no_ext);
    assert_eq!(intel.category, FileCategory::Image);

    // PDF magic bytes: %PDF-
    let pdf_no_ext = temp.path().join("doc_without_ext");
    fs::write(&pdf_no_ext, b"%PDF-1.7\n%sample pdf content").unwrap();

    let pdf_intel = FileIntelligence::from_path(&pdf_no_ext);
    assert_eq!(pdf_intel.category, FileCategory::Pdf);

    // ZIP magic bytes: PK\x03\x04
    let zip_no_ext = temp.path().join("archive_without_ext");
    let mut zip_data = vec![0x50, 0x4B, 0x03, 0x04];
    zip_data.extend_from_slice(&[0u8; 64]);
    fs::write(&zip_no_ext, zip_data).unwrap();

    let zip_intel = FileIntelligence::from_path(&zip_no_ext);
    assert_eq!(zip_intel.category, FileCategory::Archive);
}

// =========================================================================
// PART 2 — UNIVERSAL QUICK PREVIEW TESTS
// =========================================================================

#[test]
fn test_04_universal_quick_preview_text_and_source_code() {
    let temp = TempDir::new().unwrap();
    let rs_path = temp.path().join("lib.rs");
    let content = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
    fs::write(&rs_path, content).unwrap();

    let preview = load_preview(&rs_path);
    assert!(matches!(preview, PreviewContent::Text(_)));
    if let PreviewContent::Text(text_preview) = preview {
        assert_eq!(text_preview.lines().len(), 3);
        assert_eq!(
            text_preview.lines()[0],
            "pub fn add(a: i32, b: i32) -> i32 {"
        );
        assert!(!text_preview.is_truncated());
    }
}

#[test]
fn test_05_universal_quick_preview_image_metadata_and_protocol() {
    let temp = TempDir::new().unwrap();
    let img_path = temp.path().join("sample.png");

    // Construct valid 16x16 PNG header
    let mut png_bytes = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, // IHDR chunk length (13)
        0x49, 0x48, 0x44, 0x52, // "IHDR"
        0x00, 0x00, 0x00, 0x10, // width: 16
        0x00, 0x00, 0x00, 0x10, // height: 16
        0x08, // bit depth 8
        0x06, // color type 6 (RGBA)
        0x00, 0x00, 0x00, // comp, filter, interlace
        0x00, 0x00, 0x00, 0x00, // CRC
    ];
    png_bytes.extend_from_slice(&[0u8; 64]);
    fs::write(&img_path, png_bytes).unwrap();

    let preview = load_preview(&img_path);
    assert!(matches!(preview, PreviewContent::Image(_)));
    if let PreviewContent::Image(img) = preview {
        assert_eq!(img.width, 16);
        assert_eq!(img.height, 16);
        assert_eq!(img.format.short_name(), "PNG");
    }

    let proto = TerminalGraphicsProtocol::detect();
    assert!(matches!(
        proto,
        TerminalGraphicsProtocol::Kitty
            | TerminalGraphicsProtocol::Iterm2
            | TerminalGraphicsProtocol::Sixel
            | TerminalGraphicsProtocol::None
    ));
}

#[test]
fn test_06_universal_quick_preview_pdf_parsing() {
    let temp = TempDir::new().unwrap();
    let pdf_path = temp.path().join("document.pdf");
    let pdf_raw = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Count 3 /Kids [3 0 R] >>\nendobj\n/Title (Project Specification)\n/Author (Architecture Team)\nstream\nTerminalVision PDF Preview Sample Text\nendstream\nxref\ntrailer\n<< /Root 1 0 R >>\n%%EOF";
    fs::write(&pdf_path, pdf_raw).unwrap();

    let preview = load_preview(&pdf_path);
    assert!(matches!(preview, PreviewContent::Pdf(_)));
    if let PreviewContent::Pdf(pdf) = preview {
        assert_eq!(pdf.version, "PDF-1.4");
        assert_eq!(pdf.page_count, Some(3));
        assert_eq!(pdf.title.as_deref(), Some("Project Specification"));
        assert_eq!(pdf.author.as_deref(), Some("Architecture Team"));
    }
}

#[test]
fn test_07_universal_quick_preview_archive_zip_reading() {
    let temp = TempDir::new().unwrap();
    let zip_path = temp.path().join("bundle.zip");

    // Construct valid in-memory ZIP with 2 local file entries
    let mut zip_bytes = Vec::new();

    // Entry 1: "README.md" (10 bytes uncompressed)
    let e1_name = b"README.md";
    zip_bytes.extend_from_slice(b"PK\x03\x04"); // Local file header signature
    zip_bytes.extend_from_slice(&[20, 0]); // Version needed
    zip_bytes.extend_from_slice(&[0, 0]); // Flags
    zip_bytes.extend_from_slice(&[0, 0]); // Compression (stored)
    zip_bytes.extend_from_slice(&[0, 0, 0, 0]); // Mod time/date
    zip_bytes.extend_from_slice(&[0, 0, 0, 0]); // CRC32
    zip_bytes.extend_from_slice(&(10u32).to_le_bytes()); // Compressed size
    zip_bytes.extend_from_slice(&(10u32).to_le_bytes()); // Uncompressed size
    zip_bytes.extend_from_slice(&(e1_name.len() as u16).to_le_bytes()); // Name len
    zip_bytes.extend_from_slice(&[0, 0]); // Extra field len
    zip_bytes.extend_from_slice(e1_name);
    zip_bytes.extend_from_slice(b"# Readme!\n"); // Payload

    // Entry 2: "src/main.rs" (15 bytes uncompressed)
    let e2_name = b"src/main.rs";
    zip_bytes.extend_from_slice(b"PK\x03\x04");
    zip_bytes.extend_from_slice(&[20, 0]);
    zip_bytes.extend_from_slice(&[0, 0]);
    zip_bytes.extend_from_slice(&[0, 0]);
    zip_bytes.extend_from_slice(&[0, 0, 0, 0]);
    zip_bytes.extend_from_slice(&[0, 0, 0, 0]);
    zip_bytes.extend_from_slice(&(15u32).to_le_bytes());
    zip_bytes.extend_from_slice(&(15u32).to_le_bytes());
    zip_bytes.extend_from_slice(&(e2_name.len() as u16).to_le_bytes());
    zip_bytes.extend_from_slice(&[0, 0]);
    zip_bytes.extend_from_slice(e2_name);
    zip_bytes.extend_from_slice(b"fn main() {}\n  ");

    fs::write(&zip_path, zip_bytes).unwrap();

    let preview = load_preview(&zip_path);
    assert!(matches!(preview, PreviewContent::Archive(_)));
    if let PreviewContent::Archive(arch) = preview {
        assert_eq!(arch.format, "ZIP Archive");
        assert_eq!(arch.total_entries, 2);
        assert_eq!(arch.entries.len(), 2);
        assert_eq!(arch.entries[0].path, "README.md");
        assert_eq!(arch.entries[0].size, Some(10));
        assert_eq!(arch.entries[1].path, "src/main.rs");
        assert_eq!(arch.entries[1].size, Some(15));
    }
}

#[test]
fn test_08_universal_quick_preview_directory_summary() {
    let temp = TempDir::new().unwrap();
    let sub_dir = temp.path().join("sub_folder");
    fs::create_dir(&sub_dir).unwrap();
    fs::write(temp.path().join("file1.txt"), b"12345").unwrap();
    fs::write(temp.path().join("file2.txt"), b"1234567890").unwrap();

    let preview = load_preview(temp.path());
    assert!(matches!(preview, PreviewContent::Directory(_)));
    if let PreviewContent::Directory(dir_prev) = preview {
        assert_eq!(dir_prev.item_count, 3);
        assert_eq!(dir_prev.file_count, 2);
        assert_eq!(dir_prev.dir_count, 1);
        assert_eq!(dir_prev.immediate_size, 15);
    }
}

#[test]
fn test_09_quick_preview_toggle_and_leave_mode() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.txt"), b"File A content").unwrap();
    fs::write(temp.path().join("b.txt"), b"File B content").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    assert_eq!(app.mode(), Mode::Normal);

    // Space toggles Quick Preview mode on
    app.handle_action(Action::Preview);
    assert_eq!(app.mode(), Mode::Preview);

    // Pressing Space again closes Quick Preview mode
    app.handle_action(Action::Preview);
    assert_eq!(app.mode(), Mode::Normal);

    // Space opens Quick Preview mode again
    app.handle_action(Action::Preview);
    assert_eq!(app.mode(), Mode::Preview);

    // Esc (Cancel) closes Quick Preview mode
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);
}

// =========================================================================
// PART 3 — SMART FILE OPERATIONS & OPERATION CENTER TESTS
// =========================================================================

#[test]
fn test_10_operation_metrics_speed_eta_and_progress_rendering() {
    let mut metrics = OperationMetrics::new(10, 1_000_000);
    metrics.start_time = Instant::now() - Duration::from_millis(500);
    metrics.update_chunk(500_000);

    assert_eq!(metrics.percentage(), 50.0);
    assert!(metrics.speed_bytes_per_sec > 0.0);
    assert!(!metrics.format_eta().is_empty());

    let bar = metrics.render_progress_bar(20);
    assert_eq!(bar.chars().count(), 20);
    assert!(bar.contains('█'));
}

#[test]
fn test_11_operation_manager_chunked_copy_and_cancellation() {
    let temp = TempDir::new().unwrap();
    let src_file = temp.path().join("large_src.bin");
    let dst_file = temp.path().join("large_dst.bin");

    // Write a 512 KiB file
    let data = vec![0xAAu8; 512 * 1024];
    fs::write(&src_file, &data).unwrap();

    let mut mgr = OperationManager::new();
    let copied = mgr.copy_file_chunked(&src_file, &dst_file).unwrap();
    assert_eq!(copied, 512 * 1024);
    assert_eq!(fs::read(&dst_file).unwrap(), data);

    // Record and verify history
    mgr.finish_operation(
        terminalvision::app::state::OperationKind::Copy,
        "Copied 1 file".to_string(),
        true,
        None,
    );
    assert_eq!(mgr.history.len(), 1);
    let record = mgr.history.entries().next().unwrap();
    assert!(record.success);

    // Test cooperative cancellation
    let cancel_dst = temp.path().join("cancel_dst.bin");
    mgr.request_cancel();
    assert!(mgr.is_cancelled());
    let cancel_res = mgr.copy_file_chunked(&src_file, &cancel_dst);
    assert!(cancel_res.is_err());
    assert!(
        !cancel_dst.exists(),
        "Cancelled copy must clean up partial destination"
    );
}

#[test]
fn test_12_conflict_resolution_and_unique_renaming() {
    let temp = TempDir::new().unwrap();
    let existing_file = temp.path().join("report.txt");
    fs::write(&existing_file, b"existing report").unwrap();

    let renamed = generate_unique_rename(&existing_file);
    assert_eq!(
        renamed.file_name().unwrap().to_str().unwrap(),
        "report (1).txt"
    );

    // When report (1).txt already exists:
    fs::write(&renamed, b"existing copy 1").unwrap();
    let renamed_2 = generate_unique_rename(&existing_file);
    assert_eq!(
        renamed_2.file_name().unwrap().to_str().unwrap(),
        "report (2).txt"
    );

    let mut prompt = ConflictPrompt::new(&PathBuf::from("/src/report.txt"), &existing_file);
    assert_eq!(prompt.current_resolution(), ConflictResolution::Replace);
    prompt.next_option();
    assert_eq!(prompt.current_resolution(), ConflictResolution::Skip);
    prompt.next_option();
    assert_eq!(prompt.current_resolution(), ConflictResolution::Rename);
}

#[test]
fn test_13_selection_preservation_and_reconciliation_after_operations() {
    let temp = TempDir::new().unwrap();
    let file1 = temp.path().join("alpha.txt");
    let file2 = temp.path().join("beta.txt");
    let file3 = temp.path().join("gamma.txt");
    fs::write(&file1, b"1").unwrap();
    fs::write(&file2, b"2").unwrap();
    fs::write(&file3, b"3").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    // Select beta.txt (index 1)
    app.handle_action(Action::MoveDown);
    assert_eq!(
        app.pane(ActivePane::Left).selected_entry().unwrap().name(),
        "beta.txt"
    );

    // Perform rename to "bravo.txt"
    let renamed_path = temp.path().join("bravo.txt");
    fs::rename(&file2, &renamed_path).unwrap();
    app.handle_action(Action::RefreshDirectory);

    // After refresh, selection should be preserved or reconciled gracefully
    assert!(app.pane(ActivePane::Left).selected_entry().is_some());
}

// =========================================================================
// PART 4 — RESPONSIVE LAYOUT & RENDERING STRESS TESTS
// =========================================================================

#[test]
fn test_14_responsive_rendering_across_terminal_resolutions() {
    let temp = TempDir::new().unwrap();
    let sample_file = temp.path().join("test.rs");
    fs::write(&sample_file, "pub fn check() -> bool { true }\n").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    let test_resolutions = [
        (80, 24),
        (100, 30),
        (120, 30),
        (159, 30),
        (160, 40),
        (180, 40),
        (200, 60),
    ];

    for (w, h) in test_resolutions {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();

        // Render normal mode
        terminal.draw(|f| render(f, &app)).unwrap();

        // Render Quick Preview mode
        app.handle_action(Action::Preview);
        terminal.draw(|f| render(f, &app)).unwrap();

        // Render with active conflict dialog
        app.operation_manager_mut().active_conflict = Some(ConflictPrompt::new(
            &PathBuf::from("/src/test.rs"),
            &sample_file,
        ));
        terminal.draw(|f| render(f, &app)).unwrap();
        app.operation_manager_mut().active_conflict = None;

        // Render with active operation progress dialog
        let mut metrics = OperationMetrics::new(10, 10_000_000);
        metrics.update_chunk(4_500_000);
        app.operation_manager_mut().active_metrics = Some(metrics);
        terminal.draw(|f| render(f, &app)).unwrap();
        app.operation_manager_mut().active_metrics = None;

        // Leave preview
        app.handle_action(Action::Cancel);
    }
}
