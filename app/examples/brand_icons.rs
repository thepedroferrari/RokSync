//! Rebuilds the brand files in app/assets/brand/ from the source logo art:
//!
//!   cargo run -p roksync --example brand_icons --release -- <logo-icon.png> <logo-large.png>
//!
//! logo-icon.png is a sheet with the R tile on the left and the wordmark on the right; the tile is
//! found by its orange border and cut out with transparent rounded corners. logo-large.png is the
//! wide banner.

use image::{imageops, imageops::FilterType, ImageEncoder, Rgba, RgbaImage};
use std::path::Path;

const ICO_SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

fn bright(p: &Rgba<u8>) -> bool {
    p[0] > 150 && p[3] > 0
}

/// Outer edge of the tile's orange border: (left, top, right, bottom, corner radius) in pixels.
fn find_tile(img: &RgbaImage) -> (f32, f32, f32, f32, f32) {
    let (w, h) = img.dimensions();
    // the tile sits in the left half; scan through its middle from the outside in
    let (mx, my) = (w / 4, h / 2);
    let left = (0..mx).find(|&x| bright(img.get_pixel(x, my))).expect("left border");
    let top = (0..my).find(|&y| bright(img.get_pixel(mx, y))).expect("top border");
    let bottom = (my..h).rev().find(|&y| bright(img.get_pixel(mx, y))).expect("bottom border");
    // the tile is square; look for its right edge just past that, short of the wordmark's glow
    let side = bottom - top;
    let right = (left + side - 20..left + side + 20).rev().find(|&x| bright(img.get_pixel(x, my))).expect("right border");
    // the border meets the top-left diagonal at r·(1 − 1/√2) from the corner
    let t = (0..(right - left) / 2).find(|&t| bright(img.get_pixel(left + t, top + t))).expect("corner");
    let r = t as f32 / (1.0 - std::f32::consts::FRAC_1_SQRT_2);
    (left as f32, top as f32, right as f32 + 1.0, bottom as f32 + 1.0, r)
}

/// Fraction of the pixel at (x, y) inside the rounded rectangle (1 px soft edge).
fn coverage(x: f32, y: f32, (l, t, r, b, rad): (f32, f32, f32, f32, f32)) -> f32 {
    let qx = (l + rad - x).max(x - (r - rad)).max(0.0);
    let qy = (t + rad - y).max(y - (b - rad)).max(0.0);
    let outside_corner = (qx * qx + qy * qy).sqrt() - rad;
    let outside_edge = (l - x).max(x - r).max(t - y).max(y - b);
    let d = if qx > 0.0 && qy > 0.0 { outside_corner } else { outside_edge };
    (0.5 - d).clamp(0.0, 1.0)
}

fn tile(sheet: &RgbaImage) -> RgbaImage {
    let (l, t, r, b, rad) = find_tile(sheet);
    println!("tile: left {l} top {t} right {r} bottom {b} radius {rad:.1}");
    let side = (r - l).max(b - t);
    let (x0, y0) = ((l + r - side) / 2.0, (t + b - side) / 2.0);
    let mut out = imageops::crop_imm(sheet, x0 as u32, y0 as u32, side as u32, side as u32).to_image();
    for (x, y, p) in out.enumerate_pixels_mut() {
        let a = coverage(x0 + x as f32 + 0.5, y0 + y as f32 + 0.5, (l, t, r, b, rad));
        p[3] = (p[3] as f32 * a).round() as u8;
    }
    out
}

/// The R mark and ROKSYNC from the right half of the sheet, 128 px tall, black keyed out: the art
/// is light on pure black, so each pixel's brightest channel becomes its alpha and the colour is
/// un-premultiplied, which preserves glow and anti-aliasing.
fn wordmark(sheet: &RgbaImage) -> RgbaImage {
    let (w, h) = sheet.dimensions();
    let lit = |x: u32, y: u32| sheet.get_pixel(x, y).0[..3].iter().copied().max().unwrap_or(0) > 24;
    let xs = w / 2 - 60..w; // right of the tile and its glow
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in xs.clone() {
            if lit(x, y) {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    let pad = 6;
    let (x0, y0) = (x0.saturating_sub(pad), y0.saturating_sub(pad));
    let (x1, y1) = ((x1 + pad).min(w - 1), (y1 + pad).min(h - 1));
    println!("wordmark: {x0},{y0} to {x1},{y1}");
    let mut out = imageops::crop_imm(sheet, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image();
    for p in out.pixels_mut() {
        let a = p[0].max(p[1]).max(p[2]);
        if a == 0 {
            *p = Rgba([0, 0, 0, 0]);
            continue;
        }
        let k = 255.0 / a as f32;
        *p = Rgba([(p[0] as f32 * k).min(255.0) as u8, (p[1] as f32 * k).min(255.0) as u8, (p[2] as f32 * k).min(255.0) as u8, a]);
    }
    let th = 128;
    let tw = (out.width() as f32 * th as f32 / out.height() as f32).round() as u32;
    imageops::resize(&out, tw, th, FilterType::Lanczos3)
}

fn png(img: &RgbaImage) -> Vec<u8> {
    let mut v = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(&mut v, image::codecs::png::CompressionType::Best, image::codecs::png::FilterType::Adaptive)
        .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)
        .expect("png");
    v
}

/// A Windows .ico with one PNG image per size.
fn ico(src: &RgbaImage) -> Vec<u8> {
    let images: Vec<Vec<u8>> = ICO_SIZES.iter().map(|&n| png(&imageops::resize(src, n, n, FilterType::Lanczos3))).collect();
    let mut out = vec![0, 0, 1, 0];
    out.extend_from_slice(&(ICO_SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * ICO_SIZES.len() as u32;
    for (&n, img) in ICO_SIZES.iter().zip(&images) {
        let d = if n >= 256 { 0 } else { n as u8 }; // 0 means 256
        out.extend_from_slice(&[d, d, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in &images {
        out.extend_from_slice(img);
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [icon_sheet, banner] = args.as_slice() else {
        eprintln!("usage: brand_icons <logo-icon.png> <logo-large.png>");
        std::process::exit(2);
    };
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("brand");
    std::fs::create_dir_all(&dir).unwrap();

    let t = tile(&image::open(icon_sheet).expect("logo-icon.png").to_rgba8());
    std::fs::write(dir.join("roksync-icon.png"), png(&imageops::resize(&t, 256, 256, FilterType::Lanczos3))).unwrap();
    std::fs::write(dir.join("roksync.ico"), ico(&t)).unwrap();

    let w = wordmark(&image::open(icon_sheet).expect("logo-icon.png").to_rgba8());
    std::fs::write(dir.join("roksync-wordmark.png"), png(&w)).unwrap();

    let b = image::open(banner).expect("logo-large.png").to_rgba8();
    let w = 1200;
    let h = (b.height() as f32 * w as f32 / b.width() as f32).round() as u32;
    std::fs::write(dir.join("roksync-banner.png"), png(&imageops::resize(&b, w, h, FilterType::Lanczos3))).unwrap();
    println!("wrote {}", dir.display());
}
