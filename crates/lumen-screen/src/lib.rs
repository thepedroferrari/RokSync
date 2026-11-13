//! Screen → light colours.
//!
//! * `capture`: grabs the desktop with DXGI Desktop Duplication (works with fullscreen games),
//!   downscaled on the GPU through a mip chain before readback.
//! * `sampler`: reads only the selected region, removes black bars (stabilised, so a dark scene
//!   isn't mistaken for letterboxing), and maps screen areas to entertainment channels.

#[cfg(windows)]
pub mod capture;
#[cfg(windows)]
pub mod windows_list;
pub mod sampler;

pub use sampler::*;

/// A downscaled frame, tightly packed BGRA8.
#[derive(Clone, Default)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl Frame {
    #[inline]
    pub fn px(&self, x: u32, y: u32) -> [f32; 3] {
        let i = ((y * self.width + x) * 4) as usize;
        let b = &self.bgra[i..i + 3];
        [b[2] as f32 / 255.0, b[1] as f32 / 255.0, b[0] as f32 / 255.0]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    pub adapter: u32,
    pub output: u32,
    pub name: String,
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl MonitorInfo {
    pub fn label(&self) -> String {
        format!("{} ({}×{})", self.name.trim_start_matches(r"\\.\"), self.width, self.height)
    }
}

/// The primary display: Windows always places its top-left corner at (0, 0). Falls back to the
/// first display. Output order alone is not reliable; devices such as soundbars on HDMI show up
/// as extra displays and can be listed first.
pub fn primary(monitors: Vec<MonitorInfo>) -> Option<MonitorInfo> {
    let i = monitors.iter().position(|m| m.left == 0 && m.top == 0).unwrap_or(0);
    monitors.into_iter().nth(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(name: &str, left: i32, top: i32) -> MonitorInfo {
        MonitorInfo { adapter: 0, output: 0, name: name.into(), left, top, width: 1280, height: 720 }
    }

    #[test]
    fn primary_is_the_display_at_the_origin_not_the_first_listed() {
        let list = vec![mon("soundbar", 3840, 2160), mon("main", 0, 0)];
        assert_eq!(primary(list).map(|m| m.name), Some("main".into()));
        assert_eq!(primary(vec![mon("only", -1920, 0)]).map(|m| m.name), Some("only".into()));
        assert_eq!(primary(Vec::new()), None);
    }
}
