//! Single-instance guard. The first copy holds a named mutex; a later launch finds it, signals
//! the running copy to show its window (a named event), and exits.

#[cfg(windows)]
mod imp {
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE, INFINITE,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    const MUTEX: &str = "Local\\RokSync.Instance";
    const SHOW: &str = "Local\\RokSync.Show";
    const LEGACY_MUTEX: &str = "Local\\Lumen.Instance";

    pub fn legacy_running() -> bool {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::OpenMutexW;
        const SYNCHRONIZE: u32 = 0x0010_0000;
        // SAFETY: valid NUL-terminated name; a handle we get is closed straight away.
        unsafe {
            let m = OpenMutexW(SYNCHRONIZE, 0, wide(LEGACY_MUTEX).as_ptr());
            if m.is_null() {
                return false;
            }
            CloseHandle(m);
        }
        true
    }

    /// The show-window event, created by the copy that owns the mutex.
    static EVENT: OnceLock<isize> = OnceLock::new();

    pub fn claim() -> bool {
        // SAFETY: plain Win32 calls with valid NUL-terminated names; the handles live for the
        // whole process on purpose (Windows closes them when RokSync exits, which frees the name).
        unsafe {
            let m = CreateMutexW(std::ptr::null(), 0, wide(MUTEX).as_ptr());
            if !m.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
                let e = OpenEventW(EVENT_MODIFY_STATE, 0, wide(SHOW).as_ptr());
                if !e.is_null() {
                    SetEvent(e);
                }
                return false;
            }
            let e = CreateEventW(std::ptr::null(), 0, 0, wide(SHOW).as_ptr());
            if !e.is_null() {
                let _ = EVENT.set(e as isize);
            }
        }
        true
    }

    pub fn on_show(f: impl Fn() + Send + 'static) {
        let Some(&e) = EVENT.get() else { return };
        std::thread::Builder::new()
            .name("instance".into())
            .spawn(move || loop {
                // SAFETY: the event handle stays open for the life of the process.
                unsafe { WaitForSingleObject(e as _, INFINITE) };
                f();
            })
            .ok();
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn claim() -> bool {
        true
    }
    pub fn on_show(_f: impl Fn() + Send + 'static) {}
    pub fn legacy_running() -> bool {
        false
    }
}

/// True if this is the only RokSync. False means another copy was asked to show itself; exit.
pub fn claim() -> bool {
    imp::claim()
}

/// True if a process holds the legacy `Local\Lumen.Instance` mutex.
pub fn legacy_running() -> bool {
    imp::legacy_running()
}

/// Runs `f` (on a background thread) each time a second launch asks this copy to show itself.
pub fn on_show(f: impl Fn() + Send + 'static) {
    imp::on_show(f)
}
