//! Small Windows integrations: start with Windows, battery state, the global "stop all effects"
//! key, opening folders.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const RUN_VALUE: &str = "RokSync";
#[cfg(windows)]
const LEGACY_RUN_VALUE: &str = "Lumen";

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
pub fn autostart_enabled() -> bool {
    run_value_exists(RUN_VALUE)
}

#[cfg(windows)]
fn run_value_exists(name: &str) -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_ANY};
    let (key, value) = (wide(RUN_KEY), wide(name));
    // SAFETY: NUL-terminated strings that outlive the call; null data pointers ask only whether
    // the value exists.
    let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_ANY, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) };
    r == 0
}

#[cfg(windows)]
pub fn set_autostart(on: bool) -> anyhow::Result<()> {
    use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
    if on {
        let exe = std::env::current_exe()?;
        let cmd = wide(&format!("\"{}\" --minimized", exe.display()));
        // SAFETY: NUL-terminated strings; the data length is the buffer's size in bytes.
        let r = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), REG_SZ, cmd.as_ptr().cast(), (cmd.len() * 2) as u32) };
        if r != 0 {
            anyhow::bail!("couldn't write to the registry: {}", std::io::Error::from_raw_os_error(r as i32));
        }
    } else {
        // SAFETY: NUL-terminated strings. A missing value is fine: it's already off.
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) };
    }
    Ok(())
}

/// If the legacy "Lumen" Run value exists, register this exe as "RokSync" and delete the old value.
#[cfg(windows)]
pub fn migrate_autostart() {
    use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, HKEY_CURRENT_USER};
    if !run_value_exists(LEGACY_RUN_VALUE) || set_autostart(true).is_err() {
        return;
    }
    let (key, value) = (wide(RUN_KEY), wide(LEGACY_RUN_VALUE));
    // SAFETY: NUL-terminated strings that outlive the call.
    unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) };
}

/// None on machines without a battery or when Windows doesn't report the AC line status.
#[cfg(windows)]
pub fn on_battery() -> Option<bool> {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    // SAFETY: plain out-parameter struct, zero-initialised, owned by us.
    if unsafe { GetSystemPowerStatus(&mut s) } == 0 {
        return None;
    }
    // BatteryFlag 128 = no system battery
    if s.BatteryFlag == 128 {
        return None;
    }
    match s.ACLineStatus {
        0 => Some(true),
        1 => Some(false),
        _ => None,
    }
}

/// Set by the global hotkey thread; the UI takes it once per frame.
pub static PANIC: AtomicBool = AtomicBool::new(false);

pub fn take_panic() -> bool {
    PANIC.swap(false, Ordering::SeqCst)
}

#[cfg(windows)]
pub fn spawn_panic_key(ctx: eframe::egui::Context) -> anyhow::Result<()> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    std::thread::Builder::new().name("panic-key".into()).spawn(move || {
        // SAFETY: registers for this thread's message queue (null hwnd); the id is ours.
        let ok = unsafe { RegisterHotKey(std::ptr::null_mut(), 1, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, b'L' as u32) };
        if ok == 0 {
            let _ = tx.send(Err("Ctrl+Alt+L is already used by another app".into()));
            return;
        }
        let _ = tx.send(Ok(()));
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        // SAFETY: standard message loop on the thread that owns the hotkey.
        while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
            if msg.message == WM_HOTKEY {
                PANIC.store(true, Ordering::SeqCst);
                ctx.request_repaint();
            }
        }
    })?;
    match rx.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => anyhow::bail!(e),
        Err(_) => anyhow::bail!("couldn't register Ctrl+Alt+L"),
    }
}

pub fn reveal(path: &Path) {
    let mut cmd = std::process::Command::new("explorer");
    if path.is_file() {
        cmd.arg(format!("/select,{}", path.display()));
    } else {
        cmd.arg(path);
    }
    let _ = cmd.spawn();
}

#[cfg(not(windows))]
pub fn autostart_enabled() -> bool {
    false
}
#[cfg(not(windows))]
pub fn migrate_autostart() {}
#[cfg(not(windows))]
pub fn set_autostart(_on: bool) -> anyhow::Result<()> {
    anyhow::bail!("only on Windows")
}
#[cfg(not(windows))]
pub fn on_battery() -> Option<bool> {
    None
}
#[cfg(not(windows))]
pub fn spawn_panic_key(_ctx: eframe::egui::Context) -> anyhow::Result<()> {
    anyhow::bail!("only on Windows")
}
