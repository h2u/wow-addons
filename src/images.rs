use crate::grouping::{list, text};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, ImageFormat, RgbaImage};
use serde_json::Value;
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};
pub fn data_url(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() > 2 * 1024 * 1024 {
        return None;
    }
    Some(format!("data:image/png;base64,{}", STANDARD.encode(bytes)))
}
fn u32le(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}
pub fn blp(b: &[u8]) -> Result<DynamicImage, String> {
    if b.len() < 148 || &b[..4] != b"BLP2" || b[8] != 2 {
        return Err("Unsupported BLP texture.".into());
    }
    let (w, h, offset, size) = (
        u32le(b, 12).unwrap(),
        u32le(b, 16).unwrap(),
        u32le(b, 20).unwrap() as usize,
        u32le(b, 84).unwrap() as usize,
    );
    let mode = if b[9] == 0 || b[10] == 0 {
        1
    } else if b[10] == 1 {
        3
    } else if b[10] == 7 {
        5
    } else {
        0
    };
    if mode == 0 || w == 0 || h == 0 || w > 2048 || h > 2048 {
        return Err("Invalid BLP dimensions.".into());
    }
    let bx = w.div_ceil(4);
    let by = h.div_ceil(4);
    let bs = if mode == 1 { 8 } else { 16 };
    if offset < 148
        || size < (bx * by) as usize * bs
        || offset.checked_add(size).is_none_or(|end| end > b.len())
    {
        return Err("Truncated BLP texture.".into());
    }
    let mut rgba = RgbaImage::new(w, h);
    let color = |n: u16| -> [u8; 4] {
        [
            (((n >> 11) & 31) as f32 * 255.0 / 31.0).round() as u8,
            (((n >> 5) & 63) as f32 * 255.0 / 63.0).round() as u8,
            ((n & 31) as f32 * 255.0 / 31.0).round() as u8,
            255,
        ]
    };
    for yb in 0..by {
        for xb in 0..bx {
            let start = offset + ((yb * bx + xb) as usize) * bs;
            let base = start + if mode == 1 { 0 } else { 8 };
            let c0 = u16::from_le_bytes(b[base..base + 2].try_into().unwrap());
            let c1 = u16::from_le_bytes(b[base + 2..base + 4].try_into().unwrap());
            let mut c = [color(c0), color(c1), [0; 4], [0; 4]];
            for ch in 0..3 {
                if c0 > c1 || mode != 1 {
                    c[2][ch] = ((2 * c[0][ch] as u16 + c[1][ch] as u16) / 3) as u8;
                    c[3][ch] = ((c[0][ch] as u16 + 2 * c[1][ch] as u16) / 3) as u8;
                } else {
                    c[2][ch] = ((c[0][ch] as u16 + c[1][ch] as u16) / 2) as u8;
                }
            }
            c[2][3] = 255;
            c[3][3] = if c0 > c1 || mode != 1 { 255 } else { 0 };
            let mut al = [0u8; 8];
            if mode == 5 {
                al[0] = b[start];
                al[1] = b[start + 1];
                if al[0] > al[1] {
                    for i in 1..=6 {
                        al[i + 1] =
                            (((7 - i) as u16 * al[0] as u16 + i as u16 * al[1] as u16) / 7) as u8;
                    }
                } else {
                    for i in 1..=4 {
                        al[i + 1] =
                            (((5 - i) as u16 * al[0] as u16 + i as u16 * al[1] as u16) / 5) as u8;
                    }
                    al[6] = 0;
                    al[7] = 255;
                }
            }
            let indices = u32le(b, base + 4).unwrap();
            for p in 0..16usize {
                let x = xb * 4 + (p % 4) as u32;
                let y = yb * 4 + (p / 4) as u32;
                if x >= w || y >= h {
                    continue;
                }
                let mut pixel = c[((indices >> (p * 2)) & 3) as usize];
                if mode == 3 {
                    pixel[3] = ((b[start + p / 2] >> ((p % 2) * 4)) & 15) * 17;
                }
                if mode == 5 {
                    let bit = p * 3;
                    let byte = start + 2 + bit / 8;
                    let packed = b[byte] as u16
                        | if byte + 1 < start + 8 {
                            (b[byte + 1] as u16) << 8
                        } else {
                            0
                        };
                    pixel[3] = al[((packed >> (bit % 8)) & 7) as usize];
                }
                rgba.put_pixel(x, y, image::Rgba(pixel));
            }
        }
    }
    Ok(DynamicImage::ImageRgba8(rgba))
}
pub fn png(bytes: &[u8], ext: &str) -> Option<Vec<u8>> {
    if bytes.len() > 2 * 1024 * 1024 {
        return None;
    }
    let image = if ext.eq_ignore_ascii_case("blp") {
        blp(bytes).ok()?
    } else {
        let mut reader = if ext.eq_ignore_ascii_case("tga") {
            image::ImageReader::with_format(Cursor::new(bytes), ImageFormat::Tga)
        } else {
            image::ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .ok()?
        };
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().ok()?
    };
    let mut out = Cursor::new(Vec::new());
    image
        .resize_exact(96, 96, image::imageops::FilterType::Triangle)
        .write_to(&mut out, ImageFormat::Png)
        .ok()?;
    Some(out.into_inner())
}
pub fn local(game: &Value, group: &Value) -> Option<Vec<u8>> {
    let base = fs::canonicalize(text(&game["installation"], "addonsPath")).ok()?;
    for a in list(&group["members"]) {
        let mut refs = Vec::<PathBuf>::new();
        if !text(&a, "iconPath").is_empty() {
            refs.push(PathBuf::from(text(&a, "iconPath")));
        }
        let toc = text(&a, "toc");
        if !toc.is_empty() && Path::new(toc).file_name().is_some_and(|f| f == toc) {
            if let Ok(content) = fs::read_to_string(base.join(text(&a, "folder")).join(toc)) {
                let icon = regex::Regex::new(r"(?mi)^##\s*IconTexture:\s*(.+)").unwrap();
                let title = regex::Regex::new(r"(?mi)^##\s*Title:.*?\|T([^:|]+)").unwrap();
                for capture in [icon.captures(&content), title.captures(&content)]
                    .into_iter()
                    .flatten()
                {
                    let reference = capture[1].trim().replace('\\', "/");
                    let lower = reference.to_lowercase();
                    if lower.starts_with("interface/addons/") {
                        refs.push(base.join(&reference[17..]));
                    } else if !lower.starts_with("interface/")
                        && !reference.chars().all(|c| c.is_ascii_digit())
                    {
                        refs.push(base.join(text(&a, "folder")).join(reference));
                    }
                }
            }
        }
        for reference in refs {
            let candidates = if reference.extension().is_some() {
                vec![reference]
            } else {
                ["png", "tga", "blp", "jpg", "webp"]
                    .iter()
                    .map(|ext| reference.with_extension(ext))
                    .collect()
            };
            for candidate in candidates {
                if let Ok(real) = fs::canonicalize(candidate) {
                    if !real.starts_with(&base) || fs::metadata(&real).ok()?.len() > 2 * 1024 * 1024
                    {
                        continue;
                    }
                    if let Ok(bytes) = fs::read(&real) {
                        if let Some(png) = png(
                            &bytes,
                            real.extension().and_then(|s| s.to_str()).unwrap_or(""),
                        ) {
                            return Some(png);
                        }
                    }
                }
            }
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blp_rejects_truncated_and_huge_textures() {
        assert!(blp(b"BLP2").is_err());
        let mut b = vec![0; 164];
        b[..4].copy_from_slice(b"BLP2");
        b[8] = 2;
        b[9] = 8;
        b[10] = 7;
        b[12..16].copy_from_slice(&4u32.to_le_bytes());
        b[16..20].copy_from_slice(&4u32.to_le_bytes());
        b[20..24].copy_from_slice(&148u32.to_le_bytes());
        b[84..88].copy_from_slice(&16u32.to_le_bytes());
        b[148] = 255;
        b[156..158].copy_from_slice(&0xf800u16.to_le_bytes());
        let img = blp(&b).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 0, 255]);
    }
}
