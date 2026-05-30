//! Texture loading: decode BMP (in-crate) and PNG (`png` crate) to RGBA8, and
//! resolve the stale absolute texture paths stored in B3D files to real files
//! by basename. JPG is a later add (needs a decoder crate).

use std::path::{Path, PathBuf};

/// Decoded RGBA8 image, top-down (row 0 = top), 4 bytes/pixel.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Load + decode a texture file, dispatching on extension. Returns `None` for
/// unreadable files or unsupported formats (e.g. JPG, for now).
pub fn load(path: &Path) -> Option<Image> {
    let bytes = std::fs::read(path).ok()?;
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("bmp") => decode_bmp(&bytes),
        Some("png") => decode_png(&bytes),
        _ => None,
    }
}

/// Decode an uncompressed 24/32-bit BMP (`BITMAPINFOHEADER`) to RGBA8.
pub fn decode_bmp(b: &[u8]) -> Option<Image> {
    if b.len() < 54 || &b[0..2] != b"BM" {
        return None;
    }
    let rd_u32 = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let rd_i32 = |o: usize| i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let rd_u16 = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);

    let data_offset = rd_u32(10) as usize;
    let width = rd_i32(18);
    let height_raw = rd_i32(22);
    let bpp = rd_u16(28);
    let compression = rd_u32(30);

    if width <= 0 || height_raw == 0 || !(bpp == 24 || bpp == 32) {
        return None;
    }
    // 0 = BI_RGB, 3 = BI_BITFIELDS (common for 32-bit; we treat channels as BGRA).
    if compression != 0 && compression != 3 {
        return None;
    }
    let width = width as u32;
    let bottom_up = height_raw > 0;
    let height = height_raw.unsigned_abs();
    let bytes_pp = (bpp / 8) as usize;
    let row_stride = ((width as usize * bytes_pp + 3) / 4) * 4;

    let needed = data_offset + row_stride * height as usize;
    if b.len() < needed {
        return None;
    }

    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for y in 0..height as usize {
        let src_row = if bottom_up {
            height as usize - 1 - y
        } else {
            y
        };
        let row = data_offset + src_row * row_stride;
        for x in 0..width as usize {
            let p = row + x * bytes_pp;
            let (bl, gr, re) = (b[p], b[p + 1], b[p + 2]); // BMP is BGR(A)
            let o = (y * width as usize + x) * 4;
            rgba[o] = re;
            rgba[o + 1] = gr;
            rgba[o + 2] = bl;
            rgba[o + 3] = 255; // diffuse textures: force opaque
        }
    }
    Some(Image {
        width,
        height,
        rgba,
    })
}

/// Decode a PNG (RGB/RGBA/grayscale) to RGBA8.
pub fn decode_png(b: &[u8]) -> Option<Image> {
    let decoder = png::Decoder::new(b);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width, info.height);
    let n = (w * h) as usize;
    let src = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => src.to_vec(),
        png::ColorType::Rgb => {
            let mut out = vec![0u8; n * 4];
            for i in 0..n {
                out[i * 4] = src[i * 3];
                out[i * 4 + 1] = src[i * 3 + 1];
                out[i * 4 + 2] = src[i * 3 + 2];
                out[i * 4 + 3] = 255;
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = vec![0u8; n * 4];
            for i in 0..n {
                let g = src[i];
                out[i * 4] = g;
                out[i * 4 + 1] = g;
                out[i * 4 + 2] = g;
                out[i * 4 + 3] = 255;
            }
            out
        }
        _ => return None,
    };
    Some(Image {
        width: w,
        height: h,
        rgba,
    })
}

/// The basename of a (possibly Windows-absolute) texture path.
pub fn basename(stale: &str) -> &str {
    stale
        .rsplit(|c| c == '\\' || c == '/')
        .next()
        .unwrap_or(stale)
}

/// Find a texture file by basename (case-insensitive) under any of `roots`,
/// searched recursively. Returns the first match.
pub fn find_texture(roots: &[PathBuf], stale: &str) -> Option<PathBuf> {
    let target = basename(stale).to_ascii_lowercase();
    if target.is_empty() {
        return None;
    }
    for root in roots {
        if let Some(found) = walk_find(root, &target, 0) {
            return Some(found);
        }
    }
    None
}

fn walk_find(dir: &Path, target_lower: &str, depth: usize) -> Option<PathBuf> {
    if depth > 8 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.to_ascii_lowercase() == target_lower {
                return Some(path);
            }
        }
    }
    for sub in subdirs {
        if let Some(found) = walk_find(&sub, target_lower, depth + 1) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_handles_windows_paths() {
        assert_eq!(basename(r"C:\Users\X\Desktop\Body.bmp"), "Body.bmp");
        assert_eq!(basename("a/b/stag_1.jpg"), "stag_1.jpg");
        assert_eq!(basename("plain.png"), "plain.png");
    }

    #[test]
    fn decode_tiny_24bit_bmp() {
        // 2x2 24-bit BMP, bottom-up. Row stride = ((2*3+3)/4)*4 = 8.
        // Bottom row then top row. Pixels are BGR.
        let mut b = vec![0u8; 54 + 8 * 2];
        b[0] = b'B';
        b[1] = b'M';
        b[10] = 54; // data offset
        b[14] = 40; // header size
        b[18] = 2; // width = 2
        b[22] = 2; // height = 2 (bottom-up)
        b[28] = 24; // bpp
        // bottom row (becomes output row 1): pixel0 = blue (B=255), pixel1 = green
        let base = 54;
        b[base] = 255; // B
        b[base + 3 + 1] = 255; // G of pixel1
        // top row (output row 0): pixel0 = red
        let top = 54 + 8;
        b[top + 2] = 255; // R
        let img = decode_bmp(&b).expect("decode");
        assert_eq!((img.width, img.height), (2, 2));
        // output row 0, pixel0 = red
        assert_eq!(&img.rgba[0..4], &[255, 0, 0, 255]);
        // output row 1, pixel0 = blue
        let r1 = (2 * 1) * 4;
        assert_eq!(&img.rgba[r1..r1 + 4], &[0, 0, 255, 255]);
    }
}
