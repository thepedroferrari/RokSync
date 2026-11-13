use crate::catalog::{self, ModelSpec, RUNTIME};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

/// Streams `url` to `dest`, verifying SHA-256. `progress(done, total)` is called as bytes arrive.
pub fn fetch(url: &str, dest: &Path, sha256: &str, progress: &dyn Fn(u64, u64)) -> Result<()> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = dest.with_extension("part");
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(None)
        .user_agent("roksync")
        .build()?;
    let mut resp = client.get(url).send().with_context(|| format!("downloading {url}"))?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    let mut file = std::fs::File::create(&tmp)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut done = 0u64;
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        done += n as u64;
        progress(done, total);
    }
    file.flush()?;
    drop(file);
    let got = hex::encode(hasher.finalize());
    if !got.eq_ignore_ascii_case(sha256) {
        let _ = std::fs::remove_file(&tmp);
        bail!("download came through damaged. Try again");
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

pub fn install_runtime(progress: &dyn Fn(u64, u64)) -> Result<()> {
    if catalog::server_exe().is_some() {
        return Ok(());
    }
    let dir = catalog::runtime_dir();
    let zip_path = dir.with_extension("zip");
    fetch(RUNTIME.url, &zip_path, RUNTIME.sha256, progress)?;
    let f = std::fs::File::open(&zip_path)?;
    let mut z = zip::ZipArchive::new(f)?;
    std::fs::create_dir_all(&dir)?;
    for i in 0..z.len() {
        let mut entry = z.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else { continue }; // refuses ../ paths
        let out = dir.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut o = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut o)?;
    }
    let _ = std::fs::remove_file(&zip_path);
    if catalog::server_exe().is_none() {
        bail!("llama-server.exe not found in the runtime archive");
    }
    Ok(())
}

pub fn install_model(m: &ModelSpec, progress: &dyn Fn(u64, u64)) -> Result<()> {
    if catalog::model_installed(m) {
        return Ok(());
    }
    fetch(m.url, &catalog::model_path(m), m.sha256, progress)
}

pub fn remove_model(m: &ModelSpec) -> Result<()> {
    let p = catalog::model_path(m);
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}
