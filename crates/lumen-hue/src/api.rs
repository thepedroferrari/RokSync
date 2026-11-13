use crate::{color::Rgb, model::*};
use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::time::Duration;

/// The bridge serves a self-signed certificate, so certificate validation is off.
pub(crate) fn lan_client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(8))
        .build()?)
}

#[derive(Clone)]
pub struct Bridge {
    pub ip: String,
    pub app_key: String,
    http: reqwest::blocking::Client,
}

#[derive(Deserialize)]
struct ApiError {
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    errors: Vec<ApiError>,
    #[serde(default = "Vec::new")]
    data: Vec<T>,
}

fn short(s: &str) -> &str {
    &s[..s.len().min(300)]
}

/// Unset fields are left untouched.
#[derive(Default, Clone, Debug)]
pub struct LightState {
    pub on: Option<bool>,
    /// 0..=100. If unset and a colour is given, brightness follows the colour's value (dark red = dim).
    pub brightness: Option<f64>,
    pub color: Option<Rgb>,
    pub gradient: Option<Vec<Rgb>>,
    pub transition_ms: Option<u32>,
    /// White mode: colour temperature in mirek (153 = 6500 K daylight … 454 = 2200 K candle).
    pub mirek: Option<u16>,
}

impl LightState {
    pub fn color(c: Rgb) -> Self {
        Self { on: Some(true), color: Some(c), ..Default::default() }
    }

    pub fn gradient(points: Vec<Rgb>) -> Self {
        Self { on: Some(true), gradient: Some(points), ..Default::default() }
    }

    pub fn power(on: bool) -> Self {
        Self { on: Some(on), ..Default::default() }
    }

    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        if let Some(on) = self.on {
            m.insert("on".into(), json!({ "on": on }));
        }
        let mut bri = self.brightness;
        if let Some(c) = self.color {
            let (x, y) = c.to_xy();
            m.insert("color".into(), json!({ "xy": { "x": x, "y": y } }));
            bri.get_or_insert((c.max_channel() as f64 * 100.0).max(1.0));
        }
        if let Some(points) = &self.gradient {
            let pts: Vec<Value> = points
                .iter()
                .map(|c| {
                    let (x, y) = c.to_xy();
                    json!({ "color": { "xy": { "x": x, "y": y } } })
                })
                .collect();
            m.insert("gradient".into(), json!({ "points": pts }));
            let max = points.iter().map(|c| c.max_channel()).fold(0.0f32, f32::max);
            bri.get_or_insert((max as f64 * 100.0).max(1.0));
        }
        if let Some(mk) = self.mirek {
            m.insert("color_temperature".into(), json!({ "mirek": mk.clamp(153, 500) }));
        }
        if let Some(b) = bri {
            m.insert("dimming".into(), json!({ "brightness": b.clamp(0.0, 100.0) }));
        }
        if let Some(t) = self.transition_ms {
            m.insert("dynamics".into(), json!({ "duration": t }));
        }
        Value::Object(m)
    }
}

impl Bridge {
    pub fn new(ip: impl Into<String>, app_key: impl Into<String>) -> Result<Self> {
        Ok(Self { ip: ip.into(), app_key: app_key.into(), http: lan_client()? })
    }

    fn url(&self, path: &str) -> String {
        format!("https://{}/clip/v2/resource/{}", self.ip, path)
    }

    pub fn get<T: DeserializeOwned>(&self, rtype: &str) -> Result<Vec<T>> {
        let resp = self
            .http
            .get(self.url(rtype))
            .header("hue-application-key", &self.app_key)
            .send()
            .with_context(|| format!("GET {rtype}"))?;
        let status = resp.status();
        let text = resp.text()?;
        let env: Envelope<T> = serde_json::from_str(&text)
            .with_context(|| format!("GET {rtype}: HTTP {status}: {}", short(&text)))?;
        if !env.errors.is_empty() && env.data.is_empty() {
            let msgs: Vec<_> = env.errors.iter().map(|e| e.description.as_str()).collect();
            bail!("GET {rtype}: {}", msgs.join("; "));
        }
        Ok(env.data)
    }

    fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut req = self.http.request(method.clone(), self.url(path)).header("hue-application-key", &self.app_key);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().with_context(|| format!("{method} {path}"))?;
        let status = resp.status();
        let text = resp.text()?;
        let env: Envelope<Value> = serde_json::from_str(&text)
            .with_context(|| format!("{method} {path}: HTTP {status}: {}", short(&text)))?;
        if !env.errors.is_empty() || !status.is_success() {
            let msgs: Vec<_> = env.errors.iter().map(|e| e.description.as_str()).collect();
            bail!("{method} {path}: HTTP {status}: {}", msgs.join("; "));
        }
        Ok(env.data.into_iter().next().unwrap_or(Value::Null))
    }

    pub fn put(&self, path: &str, body: Value) -> Result<()> {
        self.send(reqwest::Method::PUT, path, Some(body)).map(|_| ())
    }

    pub fn post(&self, rtype: &str, body: Value) -> Result<Value> {
        self.send(reqwest::Method::POST, rtype, Some(body))
    }

    pub fn delete(&self, path: &str) -> Result<()> {
        self.send(reqwest::Method::DELETE, path, None).map(|_| ())
    }

    pub fn inventory(&self) -> Result<Inventory> {
        Ok(Inventory {
            lights: self.get("light")?,
            devices: self.get("device")?,
            rooms: self.get("room")?,
            zones: self.get("zone")?,
            grouped_lights: self.get("grouped_light")?,
            ent_configs: self.get("entertainment_configuration")?,
            ent_services: self.get("entertainment")?,
            scenes: self.get("scene")?,
            // older firmware may not expose this; then every light counts as reachable
            connectivity: self.get("zigbee_connectivity").unwrap_or_default(),
        })
    }

    pub fn set_light(&self, light_id: &str, s: &LightState) -> Result<()> {
        self.put(&format!("light/{light_id}"), s.to_json())
    }

    /// Gradients don't apply to grouped lights.
    pub fn set_grouped_light(&self, grouped_light_id: &str, s: &LightState) -> Result<()> {
        let mut s = s.clone();
        s.gradient = None;
        self.put(&format!("grouped_light/{grouped_light_id}"), s.to_json())
    }

    pub fn rename(&self, rtype: &str, id: &str, name: &str) -> Result<()> {
        self.put(&format!("{rtype}/{id}"), json!({ "metadata": { "name": name } }))
    }

    pub fn set_zone_lights(&self, zone_id: &str, light_ids: &[String]) -> Result<()> {
        let children: Vec<Value> = light_ids.iter().map(|id| json!({ "rid": id, "rtype": "light" })).collect();
        self.put(&format!("zone/{zone_id}"), json!({ "children": children }))
    }

    pub fn create_zone(&self, name: &str, light_ids: &[String]) -> Result<()> {
        let children: Vec<Value> = light_ids.iter().map(|id| json!({ "rid": id, "rtype": "light" })).collect();
        self.post("zone", json!({ "metadata": { "name": name, "archetype": "other" }, "children": children }))
            .map(|_| ())
    }

    pub fn identify(&self, device_id: &str) -> Result<()> {
        self.put(&format!("device/{device_id}"), json!({ "identify": { "action": "identify" } }))
    }

    pub fn recall_scene(&self, scene_id: &str) -> Result<()> {
        self.put(&format!("scene/{scene_id}"), json!({ "recall": { "action": "active" } }))
    }

    pub fn entertainment_action(&self, config_id: &str, start: bool) -> Result<()> {
        let action = if start { "start" } else { "stop" };
        self.put(&format!("entertainment_configuration/{config_id}"), json!({ "action": action }))
    }
}

#[derive(Default, Clone, Debug)]
pub struct Inventory {
    pub lights: Vec<Light>,
    pub devices: Vec<Device>,
    pub rooms: Vec<Group>,
    pub zones: Vec<Group>,
    pub grouped_lights: Vec<GroupedLight>,
    pub ent_configs: Vec<EntConfig>,
    pub ent_services: Vec<EntService>,
    pub scenes: Vec<Scene>,
    pub connectivity: Vec<ZigbeeConn>,
}

impl Inventory {
    pub fn light(&self, id: &str) -> Option<&Light> {
        self.lights.iter().find(|l| l.id == id)
    }

    pub fn device(&self, id: &str) -> Option<&Device> {
        self.devices.iter().find(|d| d.id == id)
    }

    pub fn room(&self, id: &str) -> Option<&Group> {
        self.rooms.iter().find(|g| g.id == id)
    }

    pub fn zone(&self, id: &str) -> Option<&Group> {
        self.zones.iter().find(|g| g.id == id)
    }

    pub fn ent_config(&self, id: &str) -> Option<&EntConfig> {
        self.ent_configs.iter().find(|e| e.id == id)
    }

    pub fn lights_of_device(&self, device_id: &str) -> Vec<&Light> {
        self.lights.iter().filter(|l| l.owner.rid == device_id).collect()
    }

    /// Lights in a room (children are devices) or zone (children are lights).
    pub fn group_lights(&self, g: &Group) -> Vec<&Light> {
        let mut out = Vec::new();
        for c in &g.children {
            match c.rtype.as_str() {
                "light" => out.extend(self.light(&c.rid)),
                "device" => out.extend(self.lights_of_device(&c.rid)),
                _ => {}
            }
        }
        out
    }

    pub fn grouped_light_of<'g>(&self, g: &'g Group) -> Option<&'g str> {
        g.services.iter().find(|s| s.rtype == "grouped_light").map(|s| s.rid.as_str())
    }

    /// Lights driven by an entertainment channel, with the gradient segment index.
    pub fn channel_lights(&self, ch: &Channel) -> Vec<(&Light, u32)> {
        let mut out = Vec::new();
        for m in &ch.members {
            if let Some(es) = self.ent_services.iter().find(|e| e.id == m.service.rid) {
                for l in self.lights_of_device(&es.owner.rid) {
                    out.push((l, m.index));
                }
            }
        }
        out
    }

    pub fn group_name(&self, rid: &str) -> Option<&str> {
        self.rooms
            .iter()
            .chain(self.zones.iter())
            .find(|g| g.id == rid)
            .map(|g| g.metadata.name.as_str())
    }

    pub fn reachable(&self, light: &Light) -> bool {
        self.connectivity.iter().find(|c| c.owner.rid == light.owner.rid).map(|c| c.status == "connected").unwrap_or(true)
    }

    pub fn room_of_light(&self, light: &Light) -> Option<&Group> {
        self.rooms.iter().find(|r| r.children.iter().any(|c| c.rid == light.owner.rid))
    }

    pub fn find_target(&self, name: &str) -> Option<Target<'_>> {
        let n = name.trim().to_lowercase();
        if let Some(l) = self.lights.iter().find(|l| l.metadata.name.trim().to_lowercase() == n) {
            return Some(Target::Light(l));
        }
        if let Some(g) = self.rooms.iter().find(|g| g.metadata.name.trim().to_lowercase() == n) {
            return Some(Target::Group(g));
        }
        if let Some(g) = self.zones.iter().find(|g| g.metadata.name.trim().to_lowercase() == n) {
            return Some(Target::Group(g));
        }
        None
    }
}

pub enum Target<'a> {
    Light(&'a Light),
    Group(&'a Group),
}
