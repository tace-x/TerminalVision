//! Text and code preview domain logic.
//!
//! Provides read-only inspection of text and code files with bounded memory limits,
//! UTF-8 validation, binary detection, ANSI sanitization, language detection,
//! and structured preview results.
//!
//! This module is independent of terminal rendering, Crossterm, and Ratatui.

pub mod language;
pub mod metadata;
pub mod syntax;

pub use language::Language;
pub use metadata::{MetadataPreview, format_size, format_system_time, load_metadata_preview};
pub use syntax::{StyledSpan, TokenKind, tokenize_line};

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

/// The maximum number of bytes read for a file preview (64 KiB).
pub const MAX_PREVIEW_BYTES: usize = 64 * 1024;

/// The maximum number of lines retained in a preview.
pub const MAX_PREVIEW_LINES: usize = 1000;

/// Checks whether a file extension corresponds to a known text or source code format.
pub fn is_supported_text_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "txt"
            | "md"
            | "markdown"
            | "rs"
            | "toml"
            | "yaml"
            | "yml"
            | "json"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "ts"
            | "tsx"
            | "mts"
            | "cts"
            | "py"
            | "pyw"
            | "java"
            | "c"
            | "cpp"
            | "cc"
            | "cxx"
            | "h"
            | "hpp"
            | "hxx"
            | "go"
            | "php"
            | "phtml"
            | "php3"
            | "php4"
            | "php5"
            | "phps"
            | "rb"
            | "rake"
            | "gemspec"
            | "cs"
            | "csx"
            | "sql"
            | "csproj"
            | "sln"
            | "css"
            | "scss"
            | "sass"
            | "less"
            | "html"
            | "htm"
            | "xml"
            | "svg"
            | "sh"
            | "bash"
            | "zsh"
            | "conf"
            | "ini"
            | "env"
            | "log"
    )
}

/// Computes the column width required to display line numbers for a total count of lines.
pub fn line_number_width(total_lines: usize) -> usize {
    if total_lines == 0 {
        1
    } else {
        total_lines.to_string().len()
    }
}

/// The structured result of preparing a preview for a filesystem entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewContent {
    /// The entry is a directory.
    Directory,
    /// The file is empty (0 bytes).
    Empty,
    /// A successfully prepared text/code preview.
    Text(TextPreview),
    /// A structured metadata preview (for directories, symlinks, binary/unsupported files).
    Metadata(MetadataPreview),
    /// The file extension is not supported for text preview.
    UnsupportedExtension(String),
    /// The file contains binary content (e.g., null bytes).
    Binary,
    /// The file cannot be decoded as valid UTF-8 text.
    InvalidUtf8,
    /// The file could not be read or opened.
    Error(String),
}

impl PreviewContent {
    /// Whether the preview successfully produced text.
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    /// Returns the text preview if available.
    pub fn as_text(&self) -> Option<&TextPreview> {
        match self {
            Self::Text(preview) => Some(preview),
            _ => None,
        }
    }

    /// Whether the preview represents metadata.
    pub fn is_metadata(&self) -> bool {
        matches!(self, Self::Metadata(_))
    }

    /// Returns the metadata preview if available.
    pub fn as_metadata(&self) -> Option<&MetadataPreview> {
        match self {
            Self::Metadata(preview) => Some(preview),
            _ => None,
        }
    }
}

/// The contents of a text/code file preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPreview {
    language: Language,
    lines: Vec<String>,
    bytes_read: usize,
    total_bytes: u64,
    is_truncated: bool,
}

impl TextPreview {
    /// Creates a new text preview.
    pub fn new(
        language: Language,
        lines: Vec<String>,
        bytes_read: usize,
        total_bytes: u64,
        is_truncated: bool,
    ) -> Self {
        Self {
            language,
            lines,
            bytes_read,
            total_bytes,
            is_truncated,
        }
    }

    /// The detected programming or markup language.
    pub fn language(&self) -> Language {
        self.language
    }

    /// The previewed lines of text.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// How many lines are in this preview.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// The number of bytes read from the file.
    pub fn bytes_read(&self) -> usize {
        self.bytes_read
    }

    /// The total size of the file in bytes.
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Whether the preview was truncated due to size or line limits.
    pub fn is_truncated(&self) -> bool {
        self.is_truncated
    }

    /// The required width for rendering line numbers.
    pub fn line_number_width(&self) -> usize {
        line_number_width(self.lines.len())
    }
}

/// Sanitizes a line of text by replacing ANSI escape sequences and non-printable control
/// characters so file contents can never inject terminal control sequences.
pub fn sanitize_line(raw: &str) -> String {
    let mut sanitized = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch == '\x1b' {
            // Escape character: neutralize by converting to visible caret notation "^["
            sanitized.push('^');
            sanitized.push('[');
        } else if ch == '\t' {
            sanitized.push('\t');
        } else if ch.is_control() {
            // Replace non-tab control characters with space
            sanitized.push(' ');
        } else {
            sanitized.push(ch);
        }
    }

    sanitized
}

/// Loads and prepares a read-only preview for the file at `path`.
///
/// This function:
/// - Never executes the file or invokes any shells.
/// - Never writes to or modifies the file.
/// - Reads at most [`MAX_PREVIEW_BYTES`].
/// - Performs binary and UTF-8 safety checks.
pub fn load_preview(path: &Path) -> PreviewContent {
    let sym_meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(err) => return PreviewContent::Error(err.to_string()),
    };

    if sym_meta.file_type().is_symlink() || sym_meta.is_dir() {
        return match MetadataPreview::from_path(path) {
            Ok(meta) => PreviewContent::Metadata(meta),
            Err(err) => PreviewContent::Error(err),
        };
    }

    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(err) => return PreviewContent::Error(err.to_string()),
    };

    let total_bytes = metadata.len();
    if total_bytes == 0 {
        return PreviewContent::Empty;
    }

    let language = Language::from_path(path);

    // Check extension if present.
    if let Some(ext) = path.extension().and_then(|s| s.to_str())
        && !is_supported_text_extension(ext)
    {
        return PreviewContent::UnsupportedExtension(ext.to_string());
    }

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(err) => return PreviewContent::Error(err.to_string()),
    };

    let mut buffer = Vec::new();
    let bytes_read = match (&mut file)
        .take(MAX_PREVIEW_BYTES as u64)
        .read_to_end(&mut buffer)
    {
        Ok(n) => n,
        Err(err) => return PreviewContent::Error(err.to_string()),
    };

    // Binary check: null bytes indicate binary content.
    if buffer.contains(&0) {
        return PreviewContent::Binary;
    }

    // UTF-8 validation
    let valid_str = match std::str::from_utf8(&buffer) {
        Ok(s) => s,
        Err(err) => {
            let valid_up_to = err.valid_up_to();
            // If the buffer was cut off by MAX_PREVIEW_BYTES, an incomplete UTF-8 sequence at the end is expected
            if total_bytes > MAX_PREVIEW_BYTES as u64 && valid_up_to + 4 >= buffer.len() {
                std::str::from_utf8(&buffer[..valid_up_to]).unwrap_or("")
            } else {
                return PreviewContent::InvalidUtf8;
            }
        }
    };

    let mut lines = Vec::new();
    let mut hit_line_limit = false;

    for (line_count, raw_line) in valid_str.lines().enumerate() {
        if line_count >= MAX_PREVIEW_LINES {
            hit_line_limit = true;
            break;
        }
        lines.push(sanitize_line(raw_line));
    }

    let is_truncated = total_bytes > MAX_PREVIEW_BYTES as u64 || hit_line_limit;

    PreviewContent::Text(TextPreview::new(
        language,
        lines,
        bytes_read,
        total_bytes,
        is_truncated,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;
    use std::fs;

    #[test]
    fn test_1_plain_text_file() {
        let temp = TempDir::new("preview-txt");
        let file = temp.path().join("sample.txt");
        fs::write(&file, "Hello, world!\nLine 2\nLine 3").unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines(), &["Hello, world!", "Line 2", "Line 3"]);
        assert_eq!(text.language(), Language::PlainText);
        assert!(!text.is_truncated());
    }

    #[test]
    fn test_2_markdown_file() {
        let temp = TempDir::new("preview-md");
        let file = temp.path().join("README.md");
        fs::write(&file, "# Title\n\nContent here").unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines(), &["# Title", "", "Content here"]);
        assert_eq!(text.language(), Language::Markdown);
    }

    #[test]
    fn test_3_rust_source_file() {
        let temp = TempDir::new("preview-rs");
        let file = temp.path().join("main.rs");
        fs::write(&file, "fn main() {\n    println!(\"hi\");\n}").unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines(), &["fn main() {", "    println!(\"hi\");", "}"]);
        assert_eq!(text.language(), Language::Rust);
    }

    #[test]
    fn test_4_json_file() {
        let temp = TempDir::new("preview-json");
        let file = temp.path().join("config.json");
        fs::write(&file, "{\n  \"key\": \"value\"\n}").unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines(), &["{", "  \"key\": \"value\"", "}"]);
        assert_eq!(text.language(), Language::Json);
    }

    #[test]
    fn test_5_toml_file() {
        let temp = TempDir::new("preview-toml");
        let file = temp.path().join("Cargo.toml");
        fs::write(&file, "[package]\nname = \"app\"").unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines(), &["[package]", "name = \"app\""]);
        assert_eq!(text.language(), Language::Toml);
    }

    #[test]
    fn test_6_python_file() {
        let temp = TempDir::new("preview-py");
        let file = temp.path().join("script.py");
        fs::write(&file, "def hello():\n    print('world')").unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines(), &["def hello():", "    print('world')"]);
        assert_eq!(text.language(), Language::Python);
    }

    #[test]
    fn test_7_extensionless_utf8_text() {
        let temp = TempDir::new("preview-extensionless");
        let file = temp.path().join("LICENSE");
        fs::write(&file, "MIT License\nCopyright (c) 2026").unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines(), &["MIT License", "Copyright (c) 2026"]);
        assert_eq!(text.language(), Language::PlainText);
    }

    #[test]
    fn test_8_unsupported_extension() {
        let temp = TempDir::new("preview-unsupported");
        let file = temp.path().join("image.png");
        fs::write(&file, [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]).unwrap();

        let preview = load_preview(&file);
        assert_eq!(
            preview,
            PreviewContent::UnsupportedExtension("png".to_string())
        );
    }

    #[test]
    fn test_9_binary_looking_file() {
        let temp = TempDir::new("preview-binary");
        let file = temp.path().join("data.txt");
        // Supported extension but contains null bytes
        fs::write(&file, b"Text with \0 null byte").unwrap();

        let preview = load_preview(&file);
        assert_eq!(preview, PreviewContent::Binary);
    }

    #[test]
    fn test_10_invalid_utf8() {
        let temp = TempDir::new("preview-invalid-utf8");
        let file = temp.path().join("invalid.txt");
        // Non-UTF8 byte sequence without null bytes
        fs::write(&file, [0xFF, 0xFE, 0xFD, 0xFC, 0xFB]).unwrap();

        let preview = load_preview(&file);
        assert_eq!(preview, PreviewContent::InvalidUtf8);
    }

    #[test]
    fn test_11_empty_file() {
        let temp = TempDir::new("preview-empty");
        let file = temp.path().join("empty.txt");
        fs::write(&file, "").unwrap();

        let preview = load_preview(&file);
        assert_eq!(preview, PreviewContent::Empty);
    }

    #[test]
    fn test_12_large_file() {
        let temp = TempDir::new("preview-large");
        let file = temp.path().join("large.txt");
        let content = "a".repeat(100 * 1024); // 100 KiB
        fs::write(&file, content).unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert!(text.is_truncated());
        assert_eq!(text.bytes_read(), MAX_PREVIEW_BYTES);
        assert_eq!(text.total_bytes(), 100 * 1024);
    }

    #[test]
    fn test_13_preview_size_limit() {
        assert_eq!(MAX_PREVIEW_BYTES, 64 * 1024);
    }

    #[test]
    fn test_14_truncated_preview() {
        let temp = TempDir::new("preview-truncated");
        let file = temp.path().join("truncated.txt");
        let content = "line\n".repeat(2000); // 2000 lines > MAX_PREVIEW_LINES
        fs::write(&file, content).unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert!(text.is_truncated());
        assert_eq!(text.lines().len(), MAX_PREVIEW_LINES);
    }

    #[test]
    fn test_15_long_line() {
        let temp = TempDir::new("preview-long-line");
        let file = temp.path().join("long.txt");
        let long_line = "x".repeat(10_000);
        fs::write(&file, &long_line).unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines().len(), 1);
        assert_eq!(text.lines()[0].len(), 10_000);
    }

    #[test]
    fn test_16_unicode_content() {
        let temp = TempDir::new("preview-unicode");
        let file = temp.path().join("unicode.txt");
        let content = "🦀 Rust\n日本語\nРусский\nÜñîçødé";
        fs::write(&file, content).unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines(), &["🦀 Rust", "日本語", "Русский", "Üñîçødé"]);
    }

    #[test]
    fn test_17_unicode_filename() {
        let temp = TempDir::new("preview-unicode-name");
        let file = temp.path().join("🦀_file_日本語.txt");
        fs::write(&file, "Content").unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines(), &["Content"]);
    }

    #[test]
    fn test_18_spaces_in_filename() {
        let temp = TempDir::new("preview-spaces");
        let file = temp.path().join("my sample document.txt");
        fs::write(&file, "Content with spaces").unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines(), &["Content with spaces"]);
    }

    #[test]
    fn test_19_missing_file() {
        let preview = load_preview(Path::new("/nonexistent/file.txt"));
        assert!(matches!(preview, PreviewContent::Error(_)));
    }

    #[test]
    #[cfg(unix)]
    fn test_20_permission_failure() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new("preview-permission");
        let file = temp.path().join("no_read.txt");
        fs::write(&file, "secret").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();

        // If running as non-root, read will fail
        let preview = load_preview(&file);
        if File::open(&file).is_ok() {
            // Running as root in CI/container
        } else {
            assert!(matches!(preview, PreviewContent::Error(_)));
        }

        // Restore permissions for cleanup
        let _ = fs::set_permissions(&file, fs::Permissions::from_mode(0o644));
    }

    #[test]
    fn test_21_directory_selected() {
        let temp = TempDir::new("preview-dir");
        let preview = load_preview(temp.path());
        let meta = preview.as_metadata().expect("must be metadata preview");
        assert_eq!(meta.kind(), crate::filesystem::entry::EntryKind::Directory);
    }

    #[test]
    #[cfg(unix)]
    fn test_21b_symlink_selected() {
        use std::os::unix::fs::symlink;
        let temp = TempDir::new("preview-sym");
        let target = temp.path().join("target.txt");
        fs::write(&target, "content").unwrap();
        let link = temp.path().join("link.txt");
        symlink(&target, &link).unwrap();

        let preview = load_preview(&link);
        let meta = preview.as_metadata().expect("must be metadata preview");
        assert_eq!(meta.kind(), crate::filesystem::entry::EntryKind::Symlink);
    }

    #[test]
    fn test_22_file_disappearing() {
        let temp = TempDir::new("preview-disappearing");
        let file = temp.path().join("disappearing.txt");
        fs::write(&file, "temporary").unwrap();
        fs::remove_file(&file).unwrap();

        let preview = load_preview(&file);
        assert!(matches!(preview, PreviewContent::Error(_)));
    }

    #[test]
    fn test_23_preview_does_not_modify_file() {
        let temp = TempDir::new("preview-immutable");
        let file = temp.path().join("immutable.txt");
        fs::write(&file, "constant content").unwrap();

        let meta_before = fs::metadata(&file).unwrap();
        let _ = load_preview(&file);
        let meta_after = fs::metadata(&file).unwrap();

        assert_eq!(meta_before.len(), meta_after.len());
        assert_eq!(fs::read_to_string(&file).unwrap(), "constant content");
    }

    #[test]
    fn test_24_preview_does_not_execute_file() {
        let temp = TempDir::new("preview-no-exec");
        let file = temp.path().join("script.sh");
        fs::write(&file, "#!/bin/sh\nexit 42").unwrap();

        let preview = load_preview(&file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines(), &["#!/bin/sh", "exit 42"]);
        assert_eq!(text.language(), Language::Shell);
    }

    #[test]
    fn test_25_ansi_escape_sequences_treated_as_data() {
        let raw = "\x1b[31mRed Text\x1b[0m";
        let sanitized = sanitize_line(raw);
        assert_eq!(sanitized, "^[[31mRed Text^[[0m");
        assert!(!sanitized.contains('\x1b'));
    }

    #[test]
    fn test_line_number_width_calculation() {
        assert_eq!(line_number_width(0), 1);
        assert_eq!(line_number_width(1), 1);
        assert_eq!(line_number_width(9), 1);
        assert_eq!(line_number_width(10), 2);
        assert_eq!(line_number_width(99), 2);
        assert_eq!(line_number_width(100), 3);
        assert_eq!(line_number_width(999), 3);
        assert_eq!(line_number_width(1000), 4);
    }

    #[test]
    fn test_preview_stress_and_edge_cases() {
        let temp = TempDir::new("preview-stress");

        // 1. File > 64 KiB (e.g. 128 KiB)
        let large_file = temp.path().join("large.txt");
        let chunk = "A".repeat(1024) + "\n";
        let large_content = chunk.repeat(128); // 131,072 bytes
        fs::write(&large_file, large_content).unwrap();

        let preview = load_preview(&large_file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.bytes_read(), MAX_PREVIEW_BYTES);
        assert!(text.is_truncated());
        assert_eq!(text.total_bytes(), 128 * 1025);

        // 2. Extremely long line (10,000 characters)
        let long_line_file = temp.path().join("longline.txt");
        let long_line = "x".repeat(10_000);
        fs::write(&long_line_file, &long_line).unwrap();

        let preview = load_preview(&long_line_file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines().len(), 1);
        assert_eq!(text.lines()[0].len(), 10_000);

        // 3. Invalid UTF-8 bytes
        let invalid_utf8_file = temp.path().join("invalid.txt");
        fs::write(&invalid_utf8_file, [0xFF, 0xFE, 0xFD, 0x80, 0x81]).unwrap();
        let preview = load_preview(&invalid_utf8_file);
        assert_eq!(preview, PreviewContent::InvalidUtf8);

        // 4. Binary with embedded null bytes
        let binary_file = temp.path().join("binary.txt");
        fs::write(&binary_file, b"Hello\x00World").unwrap();
        let preview = load_preview(&binary_file);
        assert_eq!(preview, PreviewContent::Binary);

        // 5. Extensionless text file
        let no_ext_file = temp.path().join("Makefile");
        fs::write(&no_ext_file, "all:\n\techo done\n").unwrap();
        let preview = load_preview(&no_ext_file);
        let text = preview.as_text().expect("must be text");
        assert_eq!(text.lines().len(), 2);

        // 6. Sanitization of control characters and multiple ANSI sequences
        let raw = "\x1b[1;32mBright\x1b[0m \x07 Bell \x08 Backspace \t Tab";
        let sanitized = sanitize_line(raw);
        assert!(!sanitized.contains('\x1b'));
        assert!(!sanitized.contains('\x07'));
        assert!(!sanitized.contains('\x08'));
        assert!(sanitized.contains('\t'));
        assert!(sanitized.contains("^[[1;32mBright^[[0m"));
    }
}
