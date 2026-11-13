//! Reads installed Steam games and their cover art from disk. Steam caches the art it shows in
//! its library under appcache/librarycache.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Game {
    pub appid: u32,
    pub name: String,
}

const STEAM: &str = r"C:\Program Files (x86)\Steam";

/// Every Steam library folder (the main install plus extra drives).
pub fn libraries() -> Vec<PathBuf> {
    let mut libs = vec![PathBuf::from(STEAM)];
    if let Ok(vdf) = std::fs::read_to_string(Path::new(STEAM).join(r"steamapps\libraryfolders.vdf")) {
        for line in vdf.lines() {
            if let Some(rest) = line.trim().strip_prefix("\"path\"") {
                let p = PathBuf::from(rest.trim().trim_matches('"').replace("\\\\", "\\"));
                if !libs.iter().any(|l| l.to_string_lossy().eq_ignore_ascii_case(&p.to_string_lossy())) {
                    libs.push(p);
                }
            }
        }
    }
    libs
}

fn field(acf: &str, key: &str) -> Option<String> {
    let k = format!("\"{key}\"");
    acf.lines().find_map(|l| l.trim().strip_prefix(&k).map(|v| v.trim().trim_matches('"').to_string()))
}

fn is_tool(name: &str) -> bool {
    const SKIP: &[&str] = &["redistributable", "proton", "steam linux runtime", "steamworks", "steamvr", "easyanticheat", "soundtrack", "dedicated server", " sdk"];
    let n = name.to_lowercase();
    SKIP.iter().any(|s| n.contains(s))
}

pub fn installed() -> Vec<Game> {
    let mut out: Vec<Game> = Vec::new();
    for lib in libraries() {
        let Ok(rd) = std::fs::read_dir(lib.join("steamapps")) else { continue };
        for e in rd.flatten() {
            let fname = e.file_name().to_string_lossy().to_string();
            if !(fname.starts_with("appmanifest_") && fname.ends_with(".acf")) {
                continue;
            }
            let Ok(acf) = std::fs::read_to_string(e.path()) else { continue };
            let (Some(id), Some(name)) = (field(&acf, "appid").and_then(|s| s.parse().ok()), field(&acf, "name")) else { continue };
            if is_tool(&name) || id == 228980 || out.iter().any(|g| g.appid == id) {
                continue;
            }
            out.push(Game { appid: id, name: name.trim().to_string() });
        }
    }
    out.sort_by_key(|g| g.name.to_lowercase());
    out
}

#[derive(Clone, Copy)]
pub enum Art {
    Cover,
    Hero,
    #[allow(dead_code)]
    Logo,
}

/// Local path of a game's artwork, if Steam has cached it. Handles both the flat layout
/// (`<appid>/library_600x900.jpg`) and the newer hashed sub-folders.
pub fn art(appid: u32, kind: Art) -> Option<PathBuf> {
    let dir = Path::new(STEAM).join(r"appcache\librarycache").join(appid.to_string());
    let names: &[&str] = match kind {
        Art::Cover => &["library_600x900.jpg", "library_capsule.jpg"],
        Art::Hero => &["library_hero.jpg"],
        Art::Logo => &["logo.png"],
    };
    for n in names {
        let p = dir.join(n);
        if p.is_file() {
            return Some(p);
        }
    }
    let subs = std::fs::read_dir(&dir).ok()?;
    for s in subs.flatten() {
        if s.path().is_dir() {
            for n in names {
                let p = s.path().join(n);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    // legacy flat cache: <appid>_library_600x900.jpg
    if let Art::Cover = kind {
        let p = Path::new(STEAM).join(r"appcache\librarycache").join(format!("{appid}_library_600x900.jpg"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Decode an image file and shrink it to fit `max` (w, h), as straight RGBA.
pub fn load_rgba(path: &Path, max: (u32, u32)) -> Option<(usize, usize, Vec<u8>)> {
    let img = image::open(path).ok()?;
    let img = if img.width() > max.0 || img.height() > max.1 { img.thumbnail(max.0, max.1) } else { img };
    let rgba = img.to_rgba8();
    Some((rgba.width() as usize, rgba.height() as usize, rgba.into_raw()))
}
