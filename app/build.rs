//! Embeds assets/brand/roksync.ico and a VERSIONINFO block in roksync.exe through a generated .rc
//! resource script, so Explorer, the taskbar and Task Manager show the icon and file details.

use std::io::Write;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/brand/roksync.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let icon = manifest.join("assets").join("brand").join("roksync.ico");
    let path = icon.display().to_string().replace('\\', "\\\\");

    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let mut parts: Vec<u16> = version.split(['.', '-', '+']).take(3).map(|p| p.parse().unwrap_or(0)).collect();
    parts.resize(4, 0);
    let numeric = parts.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",");

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc = out.join("roksync.rc");
    let mut f = std::fs::File::create(&rc).unwrap();
    writeln!(f, "1 ICON \"{path}\"").unwrap();
    writeln!(
        f,
        r#"1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "RokSync"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "roksync"
      VALUE "OriginalFilename", "roksync.exe"
      VALUE "ProductName", "RokSync"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END"#
    )
    .unwrap();
    drop(f);
    embed_resource::compile(&rc, embed_resource::NONE).manifest_optional().unwrap();
}
