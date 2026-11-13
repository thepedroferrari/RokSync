//! CLIP v2 resource shapes. Lenient: every field defaults, so firmware changes don't break parsing.

use crate::color::Rgb;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ResourceRef {
    pub rid: String,
    pub rtype: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Metadata {
    pub name: String,
    pub archetype: String,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default)]
pub struct Xy {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct OnState {
    pub on: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Dimming {
    pub brightness: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LightColor {
    pub xy: Xy,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct GradientPoint {
    pub color: LightColor,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Gradient {
    pub points: Vec<GradientPoint>,
    pub points_capable: u32,
}

/// A light's white mode: colour temperature in mirek (1e6 / kelvin).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct LightCt {
    pub mirek: Option<f64>,
    pub mirek_valid: bool,
}

/// Zigbee link state of a device: "connected", "disconnected", "connectivity_issue", "unidirectional_incoming".
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ZigbeeConn {
    pub id: String,
    pub owner: ResourceRef,
    pub status: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Light {
    pub id: String,
    pub id_v1: String,
    pub owner: ResourceRef,
    pub metadata: Metadata,
    pub on: OnState,
    pub dimming: Option<Dimming>,
    pub color: Option<LightColor>,
    pub gradient: Option<Gradient>,
    pub color_temperature: Option<LightCt>,
}

impl Light {
    pub fn supports_ct(&self) -> bool {
        self.color_temperature.is_some()
    }

    /// Current white temperature in kelvin, when the light is in white mode.
    pub fn kelvin(&self) -> Option<u32> {
        let ct = self.color_temperature.as_ref()?;
        if !ct.mirek_valid {
            return None;
        }
        ct.mirek.filter(|m| *m > 0.0).map(|m| (1_000_000.0 / m).round() as u32)
    }

    pub fn supports_color(&self) -> bool {
        self.color.is_some()
    }

    pub fn gradient_points(&self) -> u32 {
        self.gradient.as_ref().map(|g| g.points_capable).unwrap_or(0)
    }

    pub fn brightness(&self) -> f64 {
        self.dimming.as_ref().map(|d| d.brightness).unwrap_or(100.0)
    }

    pub fn current_rgb(&self) -> Rgb {
        if !self.on.on {
            return Rgb::BLACK;
        }
        if let Some(m) = self.color_temperature.as_ref().filter(|c| c.mirek_valid).and_then(|c| c.mirek) {
            return Rgb::from_mirek(m).scale((self.brightness() / 100.0).max(0.15) as f32);
        }
        let base = match &self.color {
            Some(c) => Rgb::from_xy(c.xy.x, c.xy.y),
            None => Rgb::new(1.0, 0.85, 0.6),
        };
        base.scale((self.brightness() / 100.0).max(0.15) as f32)
    }

    /// Gradient points as colours (empty while the light is in white mode).
    pub fn current_gradient(&self) -> Vec<Rgb> {
        if self.kelvin().is_some() {
            return Vec::new();
        }
        self.gradient
            .as_ref()
            .map(|g| g.points.iter().map(|p| Rgb::from_xy(p.color.xy.x, p.color.xy.y)).collect())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ProductData {
    pub model_id: String,
    pub product_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Device {
    pub id: String,
    pub metadata: Metadata,
    pub product_data: ProductData,
    pub services: Vec<ResourceRef>,
}

/// Rooms and zones share this shape.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Group {
    pub id: String,
    pub metadata: Metadata,
    pub children: Vec<ResourceRef>,
    pub services: Vec<ResourceRef>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct GroupedLight {
    pub id: String,
    pub owner: ResourceRef,
    pub on: OnState,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ChannelMember {
    pub service: ResourceRef,
    pub index: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Channel {
    pub channel_id: u8,
    pub position: Position,
    pub members: Vec<ChannelMember>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct EntConfig {
    pub id: String,
    pub metadata: Metadata,
    pub configuration_type: String,
    pub status: String,
    pub channels: Vec<Channel>,
}

/// The `entertainment` service of a device (links a channel member to a physical device).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct EntService {
    pub id: String,
    pub owner: ResourceRef,
    pub renderer: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SceneActionBody {
    pub on: Option<OnState>,
    pub dimming: Option<Dimming>,
    pub color: Option<LightColor>,
    pub color_temperature: Option<ColorTemperature>,
    pub gradient: Option<Gradient>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ColorTemperature {
    pub mirek: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SceneAction {
    pub target: ResourceRef,
    pub action: SceneActionBody,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PaletteColor {
    pub color: LightColor,
    pub dimming: Option<Dimming>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Palette {
    pub color: Vec<PaletteColor>,
    pub color_temperature: Vec<PaletteTemp>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PaletteTemp {
    pub color_temperature: ColorTemperature,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Scene {
    pub id: String,
    pub metadata: Metadata,
    pub group: ResourceRef,
    pub actions: Vec<SceneAction>,
    pub palette: Option<Palette>,
    pub speed: f64,
}

impl Scene {
    /// The scene's colours: its palette if it has one (gallery scenes do), else the colours of its actions.
    pub fn colors(&self) -> Vec<Rgb> {
        let mut out: Vec<Rgb> = Vec::new();
        if let Some(p) = &self.palette {
            out.extend(p.color.iter().map(|c| Rgb::from_xy(c.color.xy.x, c.color.xy.y)));
        }
        if out.is_empty() {
            for a in &self.actions {
                if let Some(g) = &a.action.gradient {
                    out.extend(g.points.iter().map(|p| Rgb::from_xy(p.color.xy.x, p.color.xy.y)));
                }
                if let Some(c) = &a.action.color {
                    out.push(Rgb::from_xy(c.xy.x, c.xy.y));
                }
            }
        }
        if let Some(p) = &self.palette {
            if out.is_empty() {
                // white-only gallery palettes carry colour temperatures
                out.extend(p.color_temperature.iter().filter_map(|c| c.color_temperature.mirek).map(Rgb::from_mirek));
            }
        }
        if out.is_empty() {
            // white scenes: colour temperature, dimmed by brightness
            for a in &self.actions {
                if a.action.on.as_ref().map_or(false, |o| !o.on) {
                    continue;
                }
                let bri = a.action.dimming.as_ref().map(|d| d.brightness).unwrap_or(100.0);
                let k = (0.35 + 0.65 * bri / 100.0) as f32;
                let base = match a.action.color_temperature.as_ref().and_then(|c| c.mirek) {
                    Some(m) => Rgb::from_mirek(m),
                    None if a.action.dimming.is_some() => Rgb::from_mirek(366.0),
                    None => continue,
                };
                out.push(base.scale(k));
            }
        }
        let mut dedup: Vec<Rgb> = Vec::new();
        for c in out {
            let close = dedup
                .iter()
                .any(|d| (d.r - c.r).abs() + (d.g - c.g).abs() + (d.b - c.b).abs() < 0.06);
            if !close {
                dedup.push(c);
            }
        }
        dedup
    }
}
