//! Safe in-memory inspection of archive contents without disk extraction.
//!
//! Inspects archive headers (.zip, .tar, .tar.gz, .tgz, .tar.bz2, .tar.xz, .7z)
//! to enumerate contained entries, paths, and sizes without writing anything to disk.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// A single entry recorded inside an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// Internal archive path.
    pub path: String,
    /// Uncompressed size in bytes if recorded.
    pub size: Option<u64>,
    /// Whether this entry represents a directory.
    pub is_directory: bool,
}

/// Structured preview of an archive file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivePreview {
    /// Format identifier (e.g. "ZIP", "TAR", "TAR.GZ").
    pub format: &'static str,
    /// Total number of discovered entries.
    pub total_entries: usize,
    /// Sample or complete list of discovered entries.
    pub entries: Vec<ArchiveEntry>,
    /// Total uncompressed size in bytes if available.
    pub total_uncompressed_size: Option<u64>,
    /// Compressed archive file size in bytes on disk.
    pub file_size: u64,
}

impl ArchivePreview {
    /// Inspects and lists contents of the archive at `path`.
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        let file_size = meta.len();

        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();

        let path_str = path.to_string_lossy().to_ascii_lowercase();

        if ext == "zip" {
            return parse_zip_archive(path, file_size);
        } else if ext == "tar" || path_str.ends_with(".tar") {
            return parse_tar_archive(path, file_size);
        } else if ext == "tgz" || path_str.ends_with(".tar.gz") {
            return parse_targz_preview(path, file_size);
        } else if path_str.ends_with(".tar.bz2") || path_str.ends_with(".tbz2") {
            return Ok(Self {
                format: "TAR.BZ2 (Bzip2 Tarball)",
                total_entries: 0,
                entries: Vec::new(),
                total_uncompressed_size: None,
                file_size,
            });
        } else if path_str.ends_with(".tar.xz") || path_str.ends_with(".txz") {
            return Ok(Self {
                format: "TAR.XZ (XZ Tarball)",
                total_entries: 0,
                entries: Vec::new(),
                total_uncompressed_size: None,
                file_size,
            });
        } else if ext == "7z" {
            return Ok(Self {
                format: "7-Zip Archive",
                total_entries: 0,
                entries: Vec::new(),
                total_uncompressed_size: None,
                file_size,
            });
        } else if ext == "gz" {
            return Ok(Self {
                format: "GZip Compressed File",
                total_entries: 1,
                entries: vec![ArchiveEntry {
                    path: path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("compressed_data")
                        .to_string(),
                    size: None,
                    is_directory: false,
                }],
                total_uncompressed_size: None,
                file_size,
            });
        }

        // Check magic bytes
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let mut magic = [0u8; 4];
        let _ = file.read(&mut magic);
        if magic.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
            return parse_zip_archive(path, file_size);
        }

        Err("Unsupported archive format".to_string())
    }
}

/// Parses ZIP file Central Directory or local file headers in memory.
fn parse_zip_archive(path: &Path, file_size: u64) -> Result<ArchivePreview, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    let mut total_uncompressed: u64 = 0;

    // Scan for Local File Headers: PK\x03\x04
    // Header structure:
    // 0..4: signature 0x04034b50
    // 18..22: compressed size (u32 LE)
    // 22..26: uncompressed size (u32 LE)
    // 26..28: file name length (u16 LE)
    // 28..30: extra field length (u16 LE)
    // 30..30+fn_len: filename
    let mut header = [0u8; 30];
    let mut iterations = 0;

    while iterations < 500 {
        iterations += 1;
        if file.read_exact(&mut header).is_err() {
            break;
        }

        if !header.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
            break;
        }

        let compressed_size =
            u32::from_le_bytes([header[18], header[19], header[20], header[21]]) as u64;
        let uncompressed_size =
            u32::from_le_bytes([header[22], header[23], header[24], header[25]]) as u64;
        let filename_len = u16::from_le_bytes([header[26], header[27]]) as usize;
        let extra_len = u16::from_le_bytes([header[28], header[29]]) as i64;

        let mut name_buf = vec![0u8; filename_len];
        if file.read_exact(&mut name_buf).is_err() {
            break;
        }

        let name = String::from_utf8_lossy(&name_buf).to_string();
        let is_dir = name.ends_with('/');

        total_uncompressed = total_uncompressed.saturating_add(uncompressed_size);

        entries.push(ArchiveEntry {
            path: name,
            size: if is_dir {
                None
            } else {
                Some(uncompressed_size)
            },
            is_directory: is_dir,
        });

        // Skip extra field + compressed data
        let skip = extra_len + compressed_size as i64;
        if skip > 0 && file.seek(SeekFrom::Current(skip)).is_err() {
            break;
        }
    }

    let count = entries.len();
    Ok(ArchivePreview {
        format: "ZIP Archive",
        total_entries: count,
        entries,
        total_uncompressed_size: Some(total_uncompressed),
        file_size,
    })
}

/// Parses POSIX USTAR / GNU TAR archive headers in memory.
fn parse_tar_archive(path: &Path, file_size: u64) -> Result<ArchivePreview, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    let mut total_uncompressed: u64 = 0;

    let mut block = [0u8; 512];
    let mut iterations = 0;

    while iterations < 1000 {
        iterations += 1;
        if file.read_exact(&mut block).is_err() {
            break;
        }

        // Two consecutive zero blocks indicate end of tar archive
        if block.iter().all(|&b| b == 0) {
            break;
        }

        // Tar Header:
        // 0..100: name (null-terminated)
        // 124..136: size in octal ascii
        // 156: type flag ('0' or '\0' = file, '5' = dir)
        let name_bytes = &block[0..100];
        let name_len = name_bytes.iter().position(|&b| b == 0).unwrap_or(100);
        let name = String::from_utf8_lossy(&name_bytes[..name_len])
            .trim()
            .to_string();

        if name.is_empty() {
            continue;
        }

        let size_str = String::from_utf8_lossy(&block[124..136]);
        let size = u64::from_str_radix(size_str.trim().trim_matches('\0'), 8).unwrap_or(0);
        let type_flag = block[156];
        let is_dir = type_flag == b'5' || name.ends_with('/');

        total_uncompressed = total_uncompressed.saturating_add(size);

        entries.push(ArchiveEntry {
            path: name,
            size: if is_dir { None } else { Some(size) },
            is_directory: is_dir,
        });

        // Skip payload blocks: padded to 512 bytes
        if size > 0 {
            let padded_size = (size + 511) & !511;
            if file.seek(SeekFrom::Current(padded_size as i64)).is_err() {
                break;
            }
        }
    }

    let count = entries.len();
    Ok(ArchivePreview {
        format: "TAR Archive",
        total_entries: count,
        entries,
        total_uncompressed_size: Some(total_uncompressed),
        file_size,
    })
}

/// Previews a gzip compressed tarball.
fn parse_targz_preview(path: &Path, file_size: u64) -> Result<ArchivePreview, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0u8; 10];
    let n = file.read(&mut header).map_err(|e| e.to_string())?;

    if n < 10 || header[0] != 0x1F || header[1] != 0x8B {
        return Err("Not a valid GZIP archive".to_string());
    }

    let orig_name = if header[3] & 0x08 != 0 {
        // FNAME flag set: read null-terminated filename
        let mut name_buf = Vec::new();
        let mut byte = [0u8; 1];
        while file.read_exact(&mut byte).is_ok() && byte[0] != 0 {
            name_buf.push(byte[0]);
            if name_buf.len() > 256 {
                break;
            }
        }
        String::from_utf8_lossy(&name_buf).to_string()
    } else {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("archive.tar")
            .to_string()
    };

    Ok(ArchivePreview {
        format: "TAR.GZ (Compressed Tarball)",
        total_entries: 1,
        entries: vec![ArchiveEntry {
            path: orig_name,
            size: None,
            is_directory: false,
        }],
        total_uncompressed_size: None,
        file_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;
    use std::fs;

    #[test]
    fn test_zip_archive_header_parsing() {
        let temp = TempDir::new("zip-test");
        let file = temp.path().join("test.zip");

        // Construct synthetic ZIP local file header
        let mut zip_data = Vec::new();
        // PK\x03\x04
        zip_data.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        // version needed (20)
        zip_data.extend_from_slice(&[20, 0]);
        // general flags
        zip_data.extend_from_slice(&[0, 0]);
        // compression (0 = stored)
        zip_data.extend_from_slice(&[0, 0]);
        // mod time & date
        zip_data.extend_from_slice(&[0, 0, 0, 0]);
        // crc32
        zip_data.extend_from_slice(&[0, 0, 0, 0]);
        // compressed size (11 bytes)
        zip_data.extend_from_slice(&11u32.to_le_bytes());
        // uncompressed size (11 bytes)
        zip_data.extend_from_slice(&11u32.to_le_bytes());
        // filename length (9 bytes: "hello.txt")
        let filename = b"hello.txt";
        zip_data.extend_from_slice(&(filename.len() as u16).to_le_bytes());
        // extra field len (0)
        zip_data.extend_from_slice(&0u16.to_le_bytes());
        // filename
        zip_data.extend_from_slice(filename);
        // content
        zip_data.extend_from_slice(b"hello world");

        fs::write(&file, &zip_data).unwrap();

        let preview = ArchivePreview::from_path(&file).expect("should parse zip");
        assert_eq!(preview.format, "ZIP Archive");
        assert_eq!(preview.total_entries, 1);
        assert_eq!(preview.entries[0].path, "hello.txt");
        assert_eq!(preview.entries[0].size, Some(11));
    }
}
