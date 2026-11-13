//! Top-level window enumeration and bounds, used to follow a window's region on screen.

use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, IsIconic, IsWindow, IsWindowVisible,
};

#[derive(Clone, Debug, PartialEq)]
pub struct WindowInfo {
    pub hwnd: isize,
    pub title: String,
}

/// Screen rectangle (virtual-desktop pixels): left, top, right, bottom.
pub type ScreenRect = (i32, i32, i32, i32);

struct Collect {
    out: Vec<WindowInfo>,
    minimized: bool,
}

unsafe extern "system" fn collect(hwnd: HWND, lp: LPARAM) -> BOOL {
    let c = &mut *(lp.0 as *mut Collect);
    let iconic = IsIconic(hwnd).as_bool();
    if !IsWindowVisible(hwnd).as_bool() || (iconic && !c.minimized) {
        return true.into();
    }
    let mut cloaked: u32 = 0;
    let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4);
    if cloaked != 0 {
        return true.into();
    }
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return true.into();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = GetWindowTextW(hwnd, &mut buf);
    let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
    if title == "Program Manager" {
        return true.into();
    }
    let big = window_rect(hwnd.0 as isize).is_some_and(|(l, t, r, b)| r - l >= 200 && b - t >= 150);
    if big || iconic {
        c.out.push(WindowInfo { hwnd: hwnd.0 as isize, title });
    }
    true.into()
}

fn enumerate(minimized: bool) -> Vec<WindowInfo> {
    let mut c = Collect { out: Vec::new(), minimized };
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut c as *mut _ as isize));
    }
    c.out
}

/// Windows that can be captured: visible, not minimised, at least 200 × 150.
pub fn list() -> Vec<WindowInfo> {
    enumerate(false)
}

/// Every titled top-level window, minimised ones included. A full-screen game minimises when
/// it loses focus but is still running.
pub fn list_with_minimized() -> Vec<WindowInfo> {
    enumerate(true)
}

/// Visible bounds of a window (without the invisible resize border), or None if it's gone/minimised.
pub fn window_rect(hwnd: isize) -> Option<ScreenRect> {
    unsafe {
        let h = HWND(hwnd as *mut _);
        if !IsWindow(h).as_bool() || IsIconic(h).as_bool() {
            return None;
        }
        let mut r = RECT::default();
        DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut _ as *mut _, std::mem::size_of::<RECT>() as u32).ok()?;
        Some((r.left, r.top, r.right, r.bottom))
    }
}
