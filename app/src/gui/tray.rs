//! Tray icon. A hidden window gets no egui frames, so the tray shows and hides the window
//! directly through Win32; the app picks up anything else on its next frame.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Set by the tray's Quit item; the next frame closes the app.
pub static QUIT: AtomicBool = AtomicBool::new(false);

pub struct Tray {
    #[cfg(windows)]
    _icon: tray_icon::TrayIcon,
    hwnd: isize,
    hidden: Arc<AtomicBool>,
    live: std::cell::Cell<bool>,
}

impl Tray {
    #[cfg(windows)]
    pub fn new(cc: &eframe::CreationContext<'_>) -> Option<Self> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
        use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

        let hwnd = match cc.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get(),
            _ => return None,
        };
        let open = MenuItem::new("Open RokSync", true, None);
        let quit = MenuItem::new("Quit", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit]).ok()?;
        let icon = TrayIconBuilder::new()
            .with_tooltip("RokSync")
            .with_icon(icon(false))
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .ok()?;
        let hidden = Arc::new(AtomicBool::new(false));

        let (h, ctx) = (hidden.clone(), cc.egui_ctx.clone());
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show(hwnd, &h);
                ctx.request_repaint();
            }
        }));
        let (h, ctx) = (hidden.clone(), cc.egui_ctx.clone());
        let (open_id, quit_id) = (open.id().clone(), quit.id().clone());
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if e.id == quit_id {
                QUIT.store(true, Ordering::Relaxed);
            } else if e.id != open_id {
                return;
            }
            // quitting needs a frame, so the window is shown briefly before it closes
            show(hwnd, &h);
            ctx.request_repaint();
        }));
        Some(Self { _icon: icon, hwnd, hidden, live: std::cell::Cell::new(false) })
    }

    #[cfg(not(windows))]
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Option<Self> {
        None
    }

    pub fn hide(&self) {
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
            self.hidden.store(true, Ordering::Relaxed);
            // SAFETY: our own top-level window handle, valid for the app's lifetime.
            unsafe { ShowWindow(self.hwnd as _, SW_HIDE) };
        }
    }

    /// Shows the window; callable from any thread (e.g. when a second instance starts).
    pub fn shower(&self, ctx: eframe::egui::Context) -> impl Fn() + Send + 'static {
        let (hwnd, hidden) = (self.hwnd, self.hidden.clone());
        move || {
            #[cfg(windows)]
            show(hwnd, &hidden);
            let _ = (&hwnd, &hidden);
            ctx.request_repaint();
        }
    }

    pub fn set_live(&self, live: bool) {
        if self.live.replace(live) == live {
            return;
        }
        #[cfg(windows)]
        {
            let _ = self._icon.set_icon(Some(icon(live)));
            let _ = self._icon.set_tooltip(Some(if live { "RokSync · sync live" } else { "RokSync" }));
        }
    }

    pub fn is_hidden(&self) -> bool {
        self.hidden.load(Ordering::Relaxed)
    }
}

#[cfg(windows)]
fn show(hwnd: isize, hidden: &AtomicBool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE};
    hidden.store(false, Ordering::Relaxed);
    // SAFETY: our own top-level window handle, valid for the app's lifetime.
    unsafe {
        ShowWindow(hwnd as _, SW_RESTORE);
        SetForegroundWindow(hwnd as _);
    }
}

/// Rendered at exactly the size the tray draws (16 px, 20 at 125%, 32 at 200%), so Windows
/// never rescales it.
#[cfg(windows)]
fn icon(live: bool) -> tray_icon::Icon {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};
    // SAFETY: no pointers.
    let n = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64) as u32;
    tray_icon::Icon::from_rgba(crate::icon_art::rgba(n, live), n, n).expect("square RGBA")
}
