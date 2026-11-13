use serde::{Deserialize, Serialize};

/// sRGB colour, each channel 0..=1.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0.0, 0.0, 0.0);
    pub const WHITE: Rgb = Rgb::new(1.0, 1.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub fn from_u8(c: [u8; 3]) -> Self {
        Self::new(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0)
    }

    pub fn to_u8(self) -> [u8; 3] {
        let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [f(self.r), f(self.g), f(self.b)]
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 {
            return None;
        }
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Self::from_u8([(v >> 16) as u8, (v >> 8) as u8, v as u8]))
    }

    pub fn to_hex(self) -> String {
        let [r, g, b] = self.to_u8();
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    pub fn lerp(self, o: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        Rgb::new(
            self.r + (o.r - self.r) * t,
            self.g + (o.g - self.g) * t,
            self.b + (o.b - self.b) * t,
        )
    }

    pub fn scale(self, k: f32) -> Rgb {
        Rgb::new(self.r * k, self.g * k, self.b * k)
    }

    pub fn max_channel(self) -> f32 {
        self.r.max(self.g).max(self.b)
    }

    pub fn luma(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    pub fn desaturate(self, amount: f32) -> Rgb {
        let l = self.luma();
        self.lerp(Rgb::new(l, l, l), amount)
    }

    /// h in 0..1, s and v in 0..1
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Rgb {
        let h = (h.rem_euclid(1.0)) * 6.0;
        let i = h.floor();
        let f = h - i;
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        match i as i32 {
            0 => Rgb::new(v, t, p),
            1 => Rgb::new(q, v, p),
            2 => Rgb::new(p, v, t),
            3 => Rgb::new(p, q, v),
            4 => Rgb::new(t, p, v),
            _ => Rgb::new(v, p, q),
        }
    }

    /// (h 0..1, s 0..1, v 0..1)
    pub fn to_hsv(self) -> (f32, f32, f32) {
        let max = self.max_channel();
        let min = self.r.min(self.g).min(self.b);
        let d = max - min;
        let h = if d <= 1e-6 {
            0.0
        } else if max == self.r {
            ((self.g - self.b) / d).rem_euclid(6.0) / 6.0
        } else if max == self.g {
            ((self.b - self.r) / d + 2.0) / 6.0
        } else {
            ((self.r - self.g) / d + 4.0) / 6.0
        };
        let s = if max <= 1e-6 { 0.0 } else { d / max };
        (h, s, max)
    }

    /// Approximate colour of a white light at a colour temperature (mirek = 1e6 / kelvin).
    pub fn from_mirek(mirek: f64) -> Rgb {
        let t = (1_000_000.0 / mirek.max(100.0)) / 100.0;
        let r = if t <= 66.0 { 255.0 } else { 329.7 * (t - 60.0).powf(-0.1332) };
        let g = if t <= 66.0 { 99.47 * t.ln() - 161.12 } else { 288.12 * (t - 60.0).powf(-0.0755) };
        let b = if t >= 66.0 { 255.0 } else if t <= 19.0 { 0.0 } else { 138.52 * (t - 10.0).ln() - 305.04 };
        let f = |v: f64| (v.clamp(0.0, 255.0) / 255.0) as f32;
        Rgb::new(f(r), f(g), f(b))
    }

    /// CIE xy chromaticity (brightness is carried separately).
    pub fn to_xy(self) -> (f64, f64) {
        let r = to_linear(self.r);
        let g = to_linear(self.g);
        let b = to_linear(self.b);
        let x = r * 0.4124 + g * 0.3576 + b * 0.1805;
        let y = r * 0.2126 + g * 0.7152 + b * 0.0722;
        let z = r * 0.0193 + g * 0.1192 + b * 0.9505;
        let sum = x + y + z;
        if sum <= 1e-9 {
            return (0.3127, 0.3290); // D65 white
        }
        (round4(x / sum), round4(y / sum))
    }

    /// Full-brightness sRGB from CIE xy.
    pub fn from_xy(x: f64, y: f64) -> Rgb {
        if y <= 1e-6 {
            return Rgb::WHITE;
        }
        let yy = 1.0;
        let xx = yy / y * x;
        let zz = yy / y * (1.0 - x - y);
        let r = xx * 3.2406 - yy * 1.5372 - zz * 0.4986;
        let g = -xx * 0.9689 + yy * 1.8758 + zz * 0.0415;
        let b = xx * 0.0557 - yy * 0.2040 + zz * 1.0570;
        let m = r.max(g).max(b).max(1e-9);
        Rgb::new(to_srgb(r / m), to_srgb(g / m), to_srgb(b / m))
    }
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

fn to_linear(c: f32) -> f64 {
    let c = c.clamp(0.0, 1.0) as f64;
    if c > 0.04045 {
        ((c + 0.055) / 1.055).powf(2.4)
    } else {
        c / 12.92
    }
}

fn to_srgb(c: f64) -> f32 {
    let c = c.clamp(0.0, 1.0);
    (if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = Rgb::from_hex("#7a1fd2").unwrap();
        assert_eq!(c.to_hex(), "#7a1fd2");
    }

    #[test]
    fn xy_roundtrip_is_close() {
        for hex in ["#ff0000", "#00ff00", "#0000ff", "#ff8800", "#8844ff"] {
            let c = Rgb::from_hex(hex).unwrap();
            let (x, y) = c.to_xy();
            let back = Rgb::from_xy(x, y);
            let d = (back.r - c.r).abs() + (back.g - c.g).abs() + (back.b - c.b).abs();
            assert!(d < 0.05, "{hex} -> {back:?}");
        }
    }
}
