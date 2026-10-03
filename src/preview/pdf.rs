//! Safe, lightweight, local PDF metadata and information scanner.
//!
//! Provides read-only PDF header inspection, page count approximation, and metadata
//! parsing without pulling in heavy C libraries or executing PDF code.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Structured PDF preview data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPreview {
    /// PDF specification version (e.g. "PDF-1.7").
    pub version: String,
    /// Detected or parsed page count if discovered.
    pub page_count: Option<usize>,
    /// Document title if present in metadata.
    pub title: Option<String>,
    /// Document author if present in metadata.
    pub author: Option<String>,
    /// Creator application if present.
    pub creator: Option<String>,
    /// PDF producer software if present.
    pub producer: Option<String>,
    /// File size in bytes.
    pub file_size: u64,
    /// Extracted preview text snippets if available.
    pub sample_text: Vec<String>,
}

impl PdfPreview {
    /// Inspects and parses the PDF file at `path` up to a bounded byte limit (128 KiB).
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        let file_size = meta.len();

        let mut file = File::open(path).map_err(|e| e.to_string())?;
        // Read at most 128 KiB for fast, bounded metadata inspection
        let mut buffer = Vec::new();
        let _ = (&mut file).take(128 * 1024).read_to_end(&mut buffer);

        if buffer.len() < 8 || !buffer.starts_with(b"%PDF-") {
            return Err("Not a valid PDF file (missing %PDF- header)".to_string());
        }

        // 1. Extract Version line
        let version =
            if let Some(newline_pos) = buffer.iter().position(|&b| b == b'\r' || b == b'\n') {
                String::from_utf8_lossy(&buffer[1..newline_pos])
                    .trim()
                    .to_string()
            } else {
                "PDF".to_string()
            };

        // 2. Scan for Page Count (/Count <N> or count occurrences of /Type /Page)
        let text_repr = String::from_utf8_lossy(&buffer);
        let mut page_count = None;

        // Try /Count (\d+)
        for line in text_repr.split(&['\n', '\r', '>', '<'][..]) {
            if let Some(pos) = line.find("/Count") {
                let rest = line[pos + 6..].trim();
                let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(num) = num_str.parse::<usize>()
                    && num > 0
                {
                    page_count = Some(num);
                    break;
                }
            }
        }

        if page_count.is_none() {
            // Count /Type /Page (and not /Type /Pages)
            let page_tags =
                text_repr.matches("/Type /Page").count() + text_repr.matches("/Type/Page").count();
            let pages_tags = text_repr.matches("/Type /Pages").count()
                + text_repr.matches("/Type/Pages").count();
            let pure_pages = page_tags.saturating_sub(pages_tags);
            if pure_pages > 0 {
                page_count = Some(pure_pages);
            }
        }

        // 3. Scan metadata keys
        let title = extract_pdf_field(&text_repr, "/Title");
        let author = extract_pdf_field(&text_repr, "/Author");
        let creator = extract_pdf_field(&text_repr, "/Creator");
        let producer = extract_pdf_field(&text_repr, "/Producer");

        // 4. Extract sample readable text snippets (BT ... ET blocks)
        let mut sample_text = Vec::new();
        let mut current_idx = 0;
        while let Some(bt_pos) = text_repr[current_idx..].find("BT") {
            let actual_bt = current_idx + bt_pos;
            if let Some(et_pos) = text_repr[actual_bt..].find("ET") {
                let block = &text_repr[actual_bt..actual_bt + et_pos];
                // Extract strings in parentheses: (Text) Tj or [(T) (e) (x) (t)] TJ
                for chunk in block.split('(') {
                    if let Some(end_paren) = chunk.find(')') {
                        let snippet = chunk[..end_paren].trim();
                        if !snippet.is_empty()
                            && snippet.chars().all(|c| c.is_ascii_graphic() || c == ' ')
                        {
                            sample_text.push(snippet.to_string());
                            if sample_text.len() >= 5 {
                                break;
                            }
                        }
                    }
                }
                current_idx = actual_bt + et_pos + 2;
                if sample_text.len() >= 5 {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(Self {
            version,
            page_count,
            title,
            author,
            creator,
            producer,
            file_size,
            sample_text,
        })
    }
}

/// Helper to extract string from /Key (Value) or /Key <Hex>
fn extract_pdf_field(content: &str, key: &str) -> Option<String> {
    if let Some(pos) = content.find(key) {
        let rest = &content[pos + key.len()..];
        if let Some(start_paren) = rest.find('(')
            && start_paren < 20
            && let Some(end_paren) = rest[start_paren..].find(')')
        {
            let val = &rest[start_paren + 1..start_paren + end_paren];
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;
    use std::fs;

    #[test]
    fn test_pdf_preview_parsing() {
        let temp = TempDir::new("pdf-preview");
        let file = temp.path().join("sample.pdf");
        let raw_pdf = b"%PDF-1.7\n1 0 obj\n<< /Title (My Rust Document) /Author (Ferris) /Count 42 >>\nendobj\n";
        fs::write(&file, raw_pdf).unwrap();

        let preview = PdfPreview::from_path(&file).expect("should parse PDF preview");
        assert_eq!(preview.version, "PDF-1.7");
        assert_eq!(preview.page_count, Some(42));
        assert_eq!(preview.title.as_deref(), Some("My Rust Document"));
        assert_eq!(preview.author.as_deref(), Some("Ferris"));
    }
}
