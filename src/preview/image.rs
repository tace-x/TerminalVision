//! Image format inspection and metadata decoding.
//!
//! Provides pure, fast, safe decoding of image headers for PNG, JPEG, GIF, BMP,
//! and WEBP formats without heavy external libraries or memory overhead.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Supported image file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Webp,
}

impl ImageFormat {
    /// Format display name.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Png => "PNG (Portable Network Graphics)",
            Self::Jpeg => "JPEG / JPG",
            Self::Gif => "GIF (Graphics Interchange Format)",
            Self::Bmp => "BMP (Windows Bitmap)",
            Self::Webp => "WEBP (Web Picture Format)",
        }
    }

    /// Short format code.
    pub fn short_name(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Gif => "GIF",
            Self::Bmp => "BMP",
            Self::Webp => "WEBP",
        }
    }
}

/// Structured image metadata preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePreview {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub color_info: String,
    pub file_size: u64,
}

impl ImagePreview {
    /// Inspects and decodes the image file at `path`.
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        let file_size = meta.len();

        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let mut header = [0u8; 64];
        let n = file.read(&mut header).map_err(|e| e.to_string())?;

        if n < 8 {
            return Err("File too small to be a valid image".into());
        }

        // 1. PNG check: \x89PNG\r\n\x1a\n
        if header.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) && n >= 24 {
            let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
            let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
            let bit_depth = header.get(24).copied().unwrap_or(8);
            let color_type = header.get(25).copied().unwrap_or(2);
            let color_str = match color_type {
                0 => format!("{bit_depth}-bit Grayscale"),
                2 => format!("{bit_depth}-bit RGB Truecolor"),
                3 => format!("{bit_depth}-bit Indexed/Palette"),
                4 => format!("{bit_depth}-bit Grayscale + Alpha"),
                6 => format!("{bit_depth}-bit RGBA Truecolor + Alpha"),
                _ => format!("{bit_depth}-bit Color"),
            };

            return Ok(Self {
                format: ImageFormat::Png,
                width,
                height,
                color_info: color_str,
                file_size,
            });
        }

        // 2. GIF check: GIF87a / GIF89a
        if (header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a")) && n >= 10 {
            let width = u16::from_le_bytes([header[6], header[7]]) as u32;
            let height = u16::from_le_bytes([header[8], header[9]]) as u32;
            let ver = if header.starts_with(b"GIF89a") {
                "89a"
            } else {
                "87a"
            };
            return Ok(Self {
                format: ImageFormat::Gif,
                width,
                height,
                color_info: format!("GIF{ver} Indexed Color (Palette)"),
                file_size,
            });
        }

        // 3. BMP check: 'BM'
        if header.starts_with(b"BM") && n >= 26 {
            let width =
                i32::from_le_bytes([header[18], header[19], header[20], header[21]]).unsigned_abs();
            let height =
                i32::from_le_bytes([header[22], header[23], header[24], header[25]]).unsigned_abs();
            let bpp = if n >= 30 {
                u16::from_le_bytes([header[28], header[29]])
            } else {
                24
            };
            return Ok(Self {
                format: ImageFormat::Bmp,
                width,
                height,
                color_info: format!("{bpp}-bit Bitmap RGB"),
                file_size,
            });
        }

        // 4. WEBP check: RIFF....WEBP
        if header.starts_with(b"RIFF") && n >= 16 && &header[8..12] == b"WEBP" {
            if n >= 30 && &header[12..16] == b"VP8 " {
                // Lossy VP8
                let width = (u16::from_le_bytes([header[26], header[27]]) & 0x3FFF) as u32;
                let height = (u16::from_le_bytes([header[28], header[29]]) & 0x3FFF) as u32;
                return Ok(Self {
                    format: ImageFormat::Webp,
                    width,
                    height,
                    color_info: "VP8 Lossy 24-bit RGB".into(),
                    file_size,
                });
            } else if n >= 25 && &header[12..16] == b"VP8L" {
                // Lossless VP8L
                let b0 = header[21] as u32;
                let b1 = header[22] as u32;
                let b2 = header[23] as u32;
                let b3 = header[24] as u32;
                let width = 1 + (((b1 & 0x3F) << 8) | b0);
                let height = 1 + (((b3 & 0xF) << 10) | (b2 << 2) | ((b1 & 0xC0) >> 6));
                return Ok(Self {
                    format: ImageFormat::Webp,
                    width,
                    height,
                    color_info: "VP8L Lossless RGBA".into(),
                    file_size,
                });
            } else if n >= 30 && &header[12..16] == b"VP8X" {
                // Extended VP8X
                let width = 1
                    + (header[24] as u32
                        | ((header[25] as u32) << 8)
                        | ((header[26] as u32) << 16));
                let height = 1
                    + (header[27] as u32
                        | ((header[28] as u32) << 8)
                        | ((header[29] as u32) << 16));
                return Ok(Self {
                    format: ImageFormat::Webp,
                    width,
                    height,
                    color_info: "VP8X Extended Canvas".into(),
                    file_size,
                });
            }
        }

        // 5. JPEG check: \xFF\xD8
        if header.starts_with(&[0xFF, 0xD8])
            && let Ok((width, height, channels)) = parse_jpeg_dimensions(&mut file)
        {
            let color_str = match channels {
                1 => "8-bit Grayscale",
                3 => "24-bit YCbCr / sRGB",
                4 => "32-bit CMYK",
                _ => "JPEG Truecolor",
            };
            return Ok(Self {
                format: ImageFormat::Jpeg,
                width: width as u32,
                height: height as u32,
                color_info: color_str.into(),
                file_size,
            });
        }

        // Fallback by extension
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        match ext.as_str() {
            "png" => Ok(Self {
                format: ImageFormat::Png,
                width: 0,
                height: 0,
                color_info: "PNG Image".into(),
                file_size,
            }),
            "jpg" | "jpeg" => Ok(Self {
                format: ImageFormat::Jpeg,
                width: 0,
                height: 0,
                color_info: "JPEG Image".into(),
                file_size,
            }),
            "gif" => Ok(Self {
                format: ImageFormat::Gif,
                width: 0,
                height: 0,
                color_info: "GIF Image".into(),
                file_size,
            }),
            "bmp" => Ok(Self {
                format: ImageFormat::Bmp,
                width: 0,
                height: 0,
                color_info: "Bitmap Image".into(),
                file_size,
            }),
            "webp" => Ok(Self {
                format: ImageFormat::Webp,
                width: 0,
                height: 0,
                color_info: "WebP Image".into(),
                file_size,
            }),
            _ => Err("Unsupported or unrecognized image format".into()),
        }
    }

    /// Computes aspect ratio string (e.g. "16:9", "4:3", "1:1").
    pub fn aspect_ratio_str(&self) -> String {
        if self.width == 0 || self.height == 0 {
            return "Unknown".into();
        }
        let gcd = gcd(self.width, self.height);
        let w_ratio = self.width / gcd;
        let h_ratio = self.height / gcd;

        // Simplify common aspect ratios
        if (w_ratio == 16 && h_ratio == 9)
            || (w_ratio == 4 && h_ratio == 3)
            || (w_ratio == 1 && h_ratio == 1)
            || (w_ratio == 3 && h_ratio == 2)
            || (w_ratio == 21 && h_ratio == 9)
        {
            format!("{w_ratio}:{h_ratio}")
        } else {
            let float_ratio = (self.width as f64) / (self.height as f64);
            format!("{float_ratio:.2}:1 ({}×{})", self.width, self.height)
        }
    }
}

/// Helper to calculate greatest common divisor.
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.max(1)
}

/// Parses JPEG frame header markers to find SOF0/SOF2 width and height.
fn parse_jpeg_dimensions(file: &mut File) -> io::Result<(u16, u16, u8)> {
    file.seek(SeekFrom::Start(2))?;
    let mut buf = [0u8; 4];

    loop {
        if file.read_exact(&mut buf[..2]).is_err() {
            break;
        }

        if buf[0] != 0xFF {
            break;
        }

        let marker = buf[1];
        // Skip padding 0xFF
        if marker == 0xFF || marker == 0x00 {
            continue;
        }

        // End of image or Start of Scan
        if marker == 0xD9 || marker == 0xDA {
            break;
        }

        if file.read_exact(&mut buf[2..4]).is_err() {
            break;
        }
        let length = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        if length < 2 {
            break;
        }

        // SOF0 (Baseline), SOF1 (Extended), SOF2 (Progressive)
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let mut sof_buf = [0u8; 6];
            if file.read_exact(&mut sof_buf).is_ok() {
                let _precision = sof_buf[0];
                let height = u16::from_be_bytes([sof_buf[1], sof_buf[2]]);
                let width = u16::from_be_bytes([sof_buf[3], sof_buf[4]]);
                let channels = sof_buf[5];
                return Ok((width, height, channels));
            }
            break;
        } else {
            // Skip marker payload
            let skip_len = (length - 2) as i64;
            if file.seek(SeekFrom::Current(skip_len)).is_err() {
                break;
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "SOF marker not found",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gcd() {
        assert_eq!(gcd(1920, 1080), 120);
        assert_eq!(gcd(100, 100), 100);
    }
}
