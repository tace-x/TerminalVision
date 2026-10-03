//! File intelligence and classification system.
//!
//! Provides fast, local classification of files and directories using
//! file extensions, filesystem metadata, executable status, and lightweight magic bytes.
//! Operates completely locally without external APIs or cloud dependencies.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// High-level categorization of filesystem entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FileCategory {
    /// A directory or folder.
    Directory,
    /// Static or animated raster/vector image.
    Image,
    /// Video or animated media file.
    Video,
    /// Audio or sound recording file.
    Audio,
    /// Plain text, documentation, or markup.
    Text,
    /// Programming language source code.
    SourceCode,
    /// Formatted document or office file.
    Document,
    /// Portable Document Format (PDF).
    Pdf,
    /// Compressed archive or container file.
    Archive,
    /// Executable binary, script, or application bundle.
    Executable,
    /// Configuration or environment file.
    Config,
    /// Structured database, dataset, or serialized data.
    Data,
    /// Unknown or unsupported file type.
    Unknown,
}

impl FileCategory {
    /// User-visible display name for the category.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Directory => "Directory",
            Self::Image => "Image",
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Text => "Text",
            Self::SourceCode => "Source Code",
            Self::Document => "Document",
            Self::Pdf => "PDF Document",
            Self::Archive => "Archive",
            Self::Executable => "Executable",
            Self::Config => "Configuration",
            Self::Data => "Data / Database",
            Self::Unknown => "Unknown",
        }
    }

    /// Short icon or badge prefix for terminal display.
    pub const fn badge(self) -> &'static str {
        match self {
            Self::Directory => "DIR",
            Self::Image => "IMG",
            Self::Video => "VID",
            Self::Audio => "AUD",
            Self::Text => "TXT",
            Self::SourceCode => "SRC",
            Self::Document => "DOC",
            Self::Pdf => "PDF",
            Self::Archive => "ARC",
            Self::Executable => "EXE",
            Self::Config => "CFG",
            Self::Data => "DAT",
            Self::Unknown => "FILE",
        }
    }
}

/// Comprehensive local metadata and intelligence for a filesystem path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIntelligence {
    /// The full canonical or relative path.
    pub path: PathBuf,
    /// The file or directory name.
    pub name: String,
    /// The determined high-level category.
    pub category: FileCategory,
    /// Detected MIME type or format hint.
    pub mime_hint: &'static str,
    /// File size in bytes (0 for directories where size is not calculated).
    pub size: u64,
    /// Whether this entry is a directory.
    pub is_directory: bool,
    /// Whether this entry is a symbolic link.
    pub is_symlink: bool,
    /// Whether this entry has executable permissions or extension.
    pub is_executable: bool,
    /// Human-readable permissions string (e.g. "rwxr-xr-x" or "0o755").
    pub permissions: String,
    /// Last modification timestamp if available.
    pub modified: Option<SystemTime>,
    /// Creation timestamp if available.
    pub created: Option<SystemTime>,
}

impl FileIntelligence {
    /// Classifies and extracts metadata for the entry at `path`.
    pub fn from_path(path: &Path) -> Self {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let sym_meta = fs::symlink_metadata(path).ok();
        let is_symlink = sym_meta
            .as_ref()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);

        let fs_meta = fs::metadata(path).ok();
        let is_dir = fs_meta.as_ref().map(|m| m.is_dir()).unwrap_or(false)
            || sym_meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);

        let size = if is_dir {
            0
        } else {
            fs_meta.as_ref().map(|m| m.len()).unwrap_or(0)
        };

        let modified = fs_meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .or_else(|| sym_meta.as_ref().and_then(|m| m.modified().ok()));

        let created = fs_meta
            .as_ref()
            .and_then(|m| m.created().ok())
            .or_else(|| sym_meta.as_ref().and_then(|m| m.created().ok()));

        let is_executable = if is_dir {
            false
        } else {
            check_is_executable(path, fs_meta.as_ref())
        };

        let permissions = extract_permissions_string(fs_meta.as_ref().or(sym_meta.as_ref()));

        let (category, mime_hint) = if is_dir {
            (FileCategory::Directory, "inode/directory")
        } else {
            classify_file_by_extension_and_magic(path)
        };

        // If classified as source or text but marked executable and not standard text extension,
        // classify as Executable
        let category = if is_executable
            && matches!(category, FileCategory::Unknown | FileCategory::Text)
            && !is_plain_text_name(&name)
        {
            FileCategory::Executable
        } else {
            category
        };

        Self {
            path: path.to_path_buf(),
            name,
            category,
            mime_hint,
            size,
            is_directory: is_dir,
            is_symlink,
            is_executable,
            permissions,
            modified,
            created,
        }
    }
}

/// Checks if the file name represents standard plain text documentation.
fn is_plain_text_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "readme"
            | "readme.md"
            | "readme.txt"
            | "license"
            | "license.md"
            | "license.txt"
            | "changelog"
            | "changelog.md"
            | "authors"
            | "copying"
            | "contributing"
    )
}

/// Determines whether a file is executable on the current platform.
fn check_is_executable(path: &Path, meta: Option<&fs::Metadata>) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Some(m) = meta {
            let mode = m.permissions().mode();
            return (mode & 0o111) != 0;
        }
    }

    #[cfg(windows)]
    {
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            let ext_lower = ext.to_ascii_lowercase();
            return matches!(
                ext_lower.as_str(),
                "exe" | "bat" | "cmd" | "ps1" | "com" | "msi"
            );
        }
    }

    let _ = (path, meta);
    false
}

/// Formats filesystem permissions into human-readable notation (e.g. `rwxr-xr-x`).
fn extract_permissions_string(meta: Option<&fs::Metadata>) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Some(m) = meta {
            let mode = m.permissions().mode();
            let mut s = String::with_capacity(9);
            s.push(if mode & 0o400 != 0 { 'r' } else { '-' });
            s.push(if mode & 0o200 != 0 { 'w' } else { '-' });
            s.push(if mode & 0o100 != 0 { 'x' } else { '-' });
            s.push(if mode & 0o040 != 0 { 'r' } else { '-' });
            s.push(if mode & 0o020 != 0 { 'w' } else { '-' });
            s.push(if mode & 0o010 != 0 { 'x' } else { '-' });
            s.push(if mode & 0o004 != 0 { 'r' } else { '-' });
            s.push(if mode & 0o002 != 0 { 'w' } else { '-' });
            s.push(if mode & 0o001 != 0 { 'x' } else { '-' });
            return format!("{s} (0o{:03o})", mode & 0o777);
        }
    }

    #[cfg(not(unix))]
    {
        if let Some(m) = meta {
            if m.permissions().readonly() {
                return "r-- (read-only)".to_string();
            } else {
                return "rw- (read-write)".to_string();
            }
        }
    }

    "---------".to_string()
}

/// Classifies a non-directory file by inspecting extension and magic bytes.
pub fn classify_file_by_extension_and_magic(path: &Path) -> (FileCategory, &'static str) {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    // Check by extension first
    match ext.as_str() {
        // PDF
        "pdf" => (FileCategory::Pdf, "application/pdf"),

        // Images
        "png" => (FileCategory::Image, "image/png"),
        "jpg" | "jpeg" => (FileCategory::Image, "image/jpeg"),
        "gif" => (FileCategory::Image, "image/gif"),
        "webp" => (FileCategory::Image, "image/webp"),
        "bmp" => (FileCategory::Image, "image/bmp"),
        "tiff" | "tif" => (FileCategory::Image, "image/tiff"),
        "svg" => (FileCategory::Image, "image/svg+xml"),
        "ico" => (FileCategory::Image, "image/x-icon"),
        "avif" => (FileCategory::Image, "image/avif"),
        "heic" | "heif" => (FileCategory::Image, "image/heif"),

        // Video
        "mp4" | "m4v" => (FileCategory::Video, "video/mp4"),
        "mkv" => (FileCategory::Video, "video/x-matroska"),
        "mov" => (FileCategory::Video, "video/quicktime"),
        "avi" => (FileCategory::Video, "video/x-msvideo"),
        "webm" => (FileCategory::Video, "video/webm"),
        "flv" => (FileCategory::Video, "video/x-flv"),
        "wmv" => (FileCategory::Video, "video/x-ms-wmv"),

        // Audio
        "mp3" => (FileCategory::Audio, "audio/mpeg"),
        "wav" => (FileCategory::Audio, "audio/wav"),
        "flac" => (FileCategory::Audio, "audio/flac"),
        "ogg" | "oga" => (FileCategory::Audio, "audio/ogg"),
        "m4a" | "aac" => (FileCategory::Audio, "audio/aac"),
        "wma" => (FileCategory::Audio, "audio/x-ms-wma"),

        // Archives
        "zip" => (FileCategory::Archive, "application/zip"),
        "tar" => (FileCategory::Archive, "application/x-tar"),
        "gz" | "tgz" => (FileCategory::Archive, "application/gzip"),
        "bz2" | "tbz2" => (FileCategory::Archive, "application/x-bzip2"),
        "xz" | "txz" => (FileCategory::Archive, "application/x-xz"),
        "7z" => (FileCategory::Archive, "application/x-7z-compressed"),
        "rar" => (FileCategory::Archive, "application/vnd.rar"),
        "zst" => (FileCategory::Archive, "application/zstd"),
        "deb" | "rpm" | "apk" | "pkg" | "dmg" | "iso" => {
            (FileCategory::Archive, "application/x-archive-package")
        }

        // Source Code
        "rs" => (FileCategory::SourceCode, "text/rust"),
        "py" | "pyw" => (FileCategory::SourceCode, "text/x-python"),
        "js" | "mjs" | "cjs" => (FileCategory::SourceCode, "application/javascript"),
        "ts" | "mts" | "cts" => (FileCategory::SourceCode, "application/typescript"),
        "tsx" | "jsx" => (FileCategory::SourceCode, "text/tsx"),
        "c" | "h" => (FileCategory::SourceCode, "text/x-c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => (FileCategory::SourceCode, "text/x-c++"),
        "go" => (FileCategory::SourceCode, "text/x-go"),
        "java" => (FileCategory::SourceCode, "text/x-java"),
        "kt" | "kts" => (FileCategory::SourceCode, "text/x-kotlin"),
        "swift" => (FileCategory::SourceCode, "text/x-swift"),
        "rb" => (FileCategory::SourceCode, "text/x-ruby"),
        "php" => (FileCategory::SourceCode, "text/x-php"),
        "cs" => (FileCategory::SourceCode, "text/x-csharp"),
        "sh" | "bash" | "zsh" | "fish" => (FileCategory::SourceCode, "application/x-shellscript"),
        "html" | "htm" => (FileCategory::SourceCode, "text/html"),
        "css" | "scss" | "sass" | "less" => (FileCategory::SourceCode, "text/css"),
        "lua" => (FileCategory::SourceCode, "text/x-lua"),
        "zig" => (FileCategory::SourceCode, "text/zig"),
        "scala" => (FileCategory::SourceCode, "text/x-scala"),
        "dart" => (FileCategory::SourceCode, "text/x-dart"),
        "r" => (FileCategory::SourceCode, "text/x-r"),

        // Configuration
        "toml" => (FileCategory::Config, "application/toml"),
        "yaml" | "yml" => (FileCategory::Config, "application/yaml"),
        "json" | "jsonc" => (FileCategory::Config, "application/json"),
        "ini" | "cfg" | "conf" | "config" => (FileCategory::Config, "text/plain"),
        "env" => (FileCategory::Config, "text/plain"),
        "xml" => (FileCategory::Config, "application/xml"),
        "properties" => (FileCategory::Config, "text/plain"),

        // Data
        "csv" => (FileCategory::Data, "text/csv"),
        "tsv" => (FileCategory::Data, "text/tab-separated-values"),
        "sql" => (FileCategory::Data, "application/sql"),
        "db" | "sqlite" | "sqlite3" => (FileCategory::Data, "application/x-sqlite3"),
        "parquet" => (FileCategory::Data, "application/vnd.apache.parquet"),
        "arrow" => (FileCategory::Data, "application/vnd.apache.arrow"),
        "ndjson" | "jsonl" => (FileCategory::Data, "application/x-ndjson"),

        // Documents
        "md" | "markdown" => (FileCategory::Document, "text/markdown"),
        "txt" | "text" | "log" => (FileCategory::Text, "text/plain"),
        "rtf" => (FileCategory::Document, "application/rtf"),
        "doc" | "docx" => (FileCategory::Document, "application/msword"),
        "odt" => (
            FileCategory::Document,
            "application/vnd.oasis.opendocument.text",
        ),
        "xls" | "xlsx" => (FileCategory::Document, "application/vnd.ms-excel"),
        "ppt" | "pptx" => (FileCategory::Document, "application/vnd.ms-powerpoint"),
        "epub" => (FileCategory::Document, "application/epub+zip"),

        // Executables
        "exe" | "bin" | "app" | "elf" => (FileCategory::Executable, "application/x-executable"),

        // Magic bytes fallback check
        _ => classify_by_magic_bytes(path),
    }
}

/// Fallback magic byte detection for extensionless or unknown files.
fn classify_by_magic_bytes(path: &Path) -> (FileCategory, &'static str) {
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return (FileCategory::Unknown, "application/octet-stream"),
    };

    let mut header = [0u8; 32];
    let n = file.read(&mut header).unwrap_or(0);
    if n == 0 {
        return (FileCategory::Unknown, "application/octet-stream");
    }

    // PDF check: %PDF-
    if header.starts_with(b"%PDF-") {
        return (FileCategory::Pdf, "application/pdf");
    }

    // PNG check
    if header.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return (FileCategory::Image, "image/png");
    }

    // JPEG check
    if header.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return (FileCategory::Image, "image/jpeg");
    }

    // GIF check
    if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
        return (FileCategory::Image, "image/gif");
    }

    // WEBP check
    if header.starts_with(b"RIFF") && n >= 12 && &header[8..12] == b"WEBP" {
        return (FileCategory::Image, "image/webp");
    }

    // BMP check
    if header.starts_with(b"BM") {
        return (FileCategory::Image, "image/bmp");
    }

    // TIFF check
    if header.starts_with(&[0x49, 0x49, 0x2A, 0x00])
        || header.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
    {
        return (FileCategory::Image, "image/tiff");
    }

    // ZIP check: PK\x03\x04
    if header.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        return (FileCategory::Archive, "application/zip");
    }

    // GZIP check: \x1F\x8B
    if header.starts_with(&[0x1F, 0x8B]) {
        return (FileCategory::Archive, "application/gzip");
    }

    // ELF executable check: \x7FELF
    if header.starts_with(&[0x7F, 0x45, 0x4C, 0x46]) {
        return (FileCategory::Executable, "application/x-executable");
    }

    // Mach-O executable check
    if header.starts_with(&[0xFE, 0xED, 0xFA, 0xCE])
        || header.starts_with(&[0xFE, 0xED, 0xFA, 0xCF])
        || header.starts_with(&[0xCF, 0xFA, 0xED, 0xFE])
    {
        return (FileCategory::Executable, "application/x-mach-binary");
    }

    // Shebang script: #!
    if header.starts_with(b"#!") {
        return (FileCategory::SourceCode, "application/x-shellscript");
    }

    // UTF-8 plain text inspection
    if !header[..n].contains(&0) && std::str::from_utf8(&header[..n]).is_ok() {
        return (FileCategory::Text, "text/plain");
    }

    (FileCategory::Unknown, "application/octet-stream")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;

    #[test]
    fn test_file_classification_extensions() {
        assert_eq!(
            classify_file_by_extension_and_magic(Path::new("main.rs")).0,
            FileCategory::SourceCode
        );
        assert_eq!(
            classify_file_by_extension_and_magic(Path::new("photo.png")).0,
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
            classify_file_by_extension_and_magic(Path::new("config.toml")).0,
            FileCategory::Config
        );
        assert_eq!(
            classify_file_by_extension_and_magic(Path::new("dataset.csv")).0,
            FileCategory::Data
        );
        assert_eq!(
            classify_file_by_extension_and_magic(Path::new("notes.txt")).0,
            FileCategory::Text
        );
    }

    #[test]
    fn test_file_intelligence_from_path() {
        let temp = TempDir::new("file-intel");
        let file = temp.path().join("test.rs");
        fs::write(&file, "fn main() {}").unwrap();

        let intel = FileIntelligence::from_path(&file);
        assert_eq!(intel.name, "test.rs");
        assert_eq!(intel.category, FileCategory::SourceCode);
        assert_eq!(intel.size, 12);
        assert!(!intel.is_directory);
        assert!(intel.modified.is_some());
    }

    #[test]
    fn test_directory_intelligence() {
        let temp = TempDir::new("dir-intel");
        let intel = FileIntelligence::from_path(temp.path());
        assert_eq!(intel.category, FileCategory::Directory);
        assert!(intel.is_directory);
        assert_eq!(intel.mime_hint, "inode/directory");
    }

    #[test]
    fn test_magic_bytes_detection() {
        let temp = TempDir::new("magic-intel");

        // PDF
        let pdf_file = temp.path().join("doc_no_ext");
        fs::write(&pdf_file, b"%PDF-1.7\n%stream").unwrap();
        let (cat, mime) = classify_file_by_extension_and_magic(&pdf_file);
        assert_eq!(cat, FileCategory::Pdf);
        assert_eq!(mime, "application/pdf");

        // PNG
        let png_file = temp.path().join("image_no_ext");
        fs::write(
            &png_file,
            [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13],
        )
        .unwrap();
        let (cat, mime) = classify_file_by_extension_and_magic(&png_file);
        assert_eq!(cat, FileCategory::Image);
        assert_eq!(mime, "image/png");

        // Script with shebang
        let sh_file = temp.path().join("script_no_ext");
        fs::write(&sh_file, b"#!/bin/bash\necho 123").unwrap();
        let (cat, _) = classify_file_by_extension_and_magic(&sh_file);
        assert_eq!(cat, FileCategory::SourceCode);
    }
}
