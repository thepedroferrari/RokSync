use crate::sync::Source;
use anyhow::Result;
use lumen_engine::Program;
use lumen_screen::ScreenSettings;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeCfg {
    pub ip: String,
    pub bridge_id: String,
    pub name: String,
    pub app_key: String,
    pub client_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cs2Cfg {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
    #[serde(default)]
    pub off: Vec<String>,
    /// Strength per effect id, 1..=5 (missing = 5).
    #[serde(default)]
    pub levels: std::collections::BTreeMap<String, u8>,
    /// Reduce flashing: no full-room white or strobe.
    #[serde(default)]
    pub calm: bool,
}

impl Default for Cs2Cfg {
    fn default() -> Self {
        Self { enabled: false, port: 3121, token: String::new(), off: Vec::new(), levels: Default::default(), calm: false }
    }
}

impl Cs2Cfg {
    pub fn level(&self, fx: lumen_cs2::Fx) -> u8 {
        self.levels.get(fx.id()).copied().unwrap_or(5).clamp(1, 5)
    }

    pub fn is_on(&self, fx: lumen_cs2::Fx) -> bool {
        !self.off.iter().any(|o| o == fx.id())
    }

    pub fn push_to(&self, s: &mut lumen_cs2::Cs2State) {
        s.fx_off = lumen_cs2::Fx::ALL.iter().copied().filter(|f| !self.is_on(*f)).collect();
        s.fx_level = lumen_cs2::Fx::ALL.iter().map(|f| (*f, self.level(*f) as f32 / 5.0)).collect();
        s.calm = self.calm;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRules {
    /// Start screen sync when the game launches.
    #[serde(default)]
    pub sync: bool,
    /// Layer native game effects on top (only for games that have them).
    #[serde(default = "yes")]
    pub fx: bool,
    /// Restore the previous light state when the game exits.
    #[serde(default = "yes")]
    pub restore: bool,
    /// What the lights follow while this game runs. None = the Game mix.
    #[serde(default)]
    pub mix: Option<Mix>,
}

impl Default for GameRules {
    fn default() -> Self {
        Self { sync: false, fx: true, restore: true, mix: None }
    }
}

/// What the lights follow: one colour source, plus sound and game events on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mix {
    pub source: Source,
    pub sound: bool,
    pub game: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SoundStyle {
    Pulse,
    #[default]
    Spectrum,
    Beat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundCfg {
    #[serde(default)]
    pub on: bool,
    #[serde(default)]
    pub style: SoundStyle,
    /// How much sound moves mood colours, 0..1.
    #[serde(default = "half")]
    pub reaction: f32,
    /// How hard bass and hits lift screen colours, 0..1.
    #[serde(default = "punch")]
    pub punch: f32,
    /// Output to listen to; None = whatever Windows plays to.
    #[serde(default)]
    pub device: Option<String>,
}

impl Default for SoundCfg {
    fn default() -> Self {
        Self { on: false, style: SoundStyle::default(), reaction: half(), punch: punch(), device: None }
    }
}

impl SoundCfg {
    pub fn layer(&self, source: Source) -> crate::sound::SoundLayer {
        let amount = if source == Source::Screen { self.punch } else { self.reaction };
        crate::sound::SoundLayer { style: self.style, amount }
    }
}

fn half() -> f32 {
    0.5
}

fn punch() -> f32 {
    0.4
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedProgram {
    pub name: String,
    pub program: Program,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScreenCfg {
    #[serde(default)]
    pub settings: ScreenSettings,
    /// DXGI device name, e.g. \\.\DISPLAY1
    #[serde(default)]
    pub monitor: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// The source of truth is `bridge.json` (see [`BridgeCfg::save`]). A mirror copy stays in
    /// config.json so older builds can still read it; on load, bridge.json always wins.
    #[serde(default)]
    pub bridge: Option<BridgeCfg>,
    #[serde(default)]
    pub cs2: Cs2Cfg,
    #[serde(default)]
    pub saved: Vec<SavedProgram>,
    #[serde(default)]
    pub last_area: Option<String>,
    /// Room or zone id RokSync controls, or "home". None = pick a room on first run.
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub screen: ScreenCfg,
    #[serde(default)]
    pub source: Source,
    /// None = Quick mode (keyword matching, no model).
    #[serde(default)]
    pub mood_model: Option<String>,
    #[serde(default)]
    pub palettes: Vec<SavedPalette>,
    #[serde(default)]
    pub perf: crate::perf::Perf,
    /// Active game mode id; None = custom settings.
    #[serde(default)]
    pub game_mode: Option<String>,
    /// How hard game events hit the lights, 0..1.
    #[serde(default = "one")]
    pub fx_gain: f32,
    /// Mode chosen per Steam game (appid → mode id).
    #[serde(default)]
    pub game_modes: std::collections::BTreeMap<u32, String>,
    /// Launch rules per Steam game (appid).
    #[serde(default)]
    pub game_rules: std::collections::BTreeMap<u32, GameRules>,
    #[serde(default = "yes")]
    pub eco_on_battery: bool,
    /// Global Ctrl+Alt+L stops screen sync and pauses game effects.
    #[serde(default = "yes")]
    pub panic_key: bool,
    #[serde(default = "yes")]
    pub to_tray: bool,
    /// Game events layer off: switched off on Live, by a mode, or by the stop-all key.
    #[serde(default)]
    pub fx_paused: bool,
    #[serde(default)]
    pub dota: GameFxCfg,
    #[serde(default)]
    pub cod: GameFxCfg,
    #[serde(default)]
    pub sound: SoundCfg,
    /// First-run setup finished or skipped.
    #[serde(default)]
    pub setup_done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameFxCfg {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub off: Vec<String>,
    /// Strength (Dota) or sensitivity (CoD) per effect id, 1..=5.
    #[serde(default)]
    pub levels: std::collections::BTreeMap<String, u8>,
    #[serde(default)]
    pub calm: bool,
    /// Dota: "cycle" | "faction" | "screen". CoD: unused.
    #[serde(default)]
    pub base: String,
    /// CoD: only react while the game window is in front.
    #[serde(default = "yes")]
    pub only_in_front: bool,
    /// CoD: learned HUD calibration (opaque numbers from lumen-cod).
    #[serde(default)]
    pub calibration: Vec<f32>,
}

impl Default for GameFxCfg {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 0,
            token: String::new(),
            off: Vec::new(),
            levels: Default::default(),
            calm: false,
            base: String::new(),
            only_in_front: true,
            calibration: Vec::new(),
        }
    }
}

impl GameFxCfg {
    pub fn is_on(&self, id: &str) -> bool {
        !self.off.iter().any(|o| o == id)
    }

    pub fn set_on(&mut self, id: &str, on: bool) {
        self.off.retain(|o| o != id);
        if !on {
            self.off.push(id.to_string());
        }
    }

    pub fn level(&self, id: &str, default: u8) -> u8 {
        self.levels.get(id).copied().unwrap_or(default).clamp(1, 5)
    }
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPalette {
    pub name: String,
    /// "#rrggbb"
    pub colors: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct LoadReport {
    pub notes: Vec<String>,
}

/// %APPDATA%\RokSync, or the legacy %APPDATA%\Lumen until [`migrate_from_lumen`] has moved it.
fn settings_dir() -> PathBuf {
    lumen_mood::app_dir(dirs::config_dir())
}

/// Moves the legacy Lumen folders (%APPDATA%\Lumen: pairing and settings, %LOCALAPPDATA%\Lumen:
/// models) to their RokSync names and carries Start with Windows across. Folders are renamed,
/// never copied or deleted; one that can't be moved stays in use. Skipped while a legacy
/// instance is running, since it would keep writing to the old folder. Returns a note when
/// something needs the user.
pub fn migrate_from_lumen() -> Option<String> {
    crate::system::migrate_autostart();
    if crate::instance::legacy_running() {
        return Some("Lumen is still running. Quit it and restart RokSync to bring your settings over.".into());
    }
    let mut failed = false;
    for base in [dirs::config_dir(), dirs::data_local_dir()] {
        failed |= lumen_mood::migrate_app_dir(base).is_err();
    }
    failed.then(|| "Some files from Lumen are in use, so they stay where they are for now. Restart RokSync to move them.".into())
}

/// Write via a temp file + rename, so a crash or full disk mid-write never leaves half a file.
pub fn write_atomic(path: &std::path::Path, data: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?; // replaces the old file on Windows too
    Ok(())
}

fn keep_broken(path: &std::path::Path) -> Option<PathBuf> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = format!("{}.broken-{stamp}", path.file_name()?.to_string_lossy());
    let to = path.with_file_name(name);
    std::fs::copy(path, &to).ok().map(|_| to)
}

impl BridgeCfg {
    pub fn path() -> PathBuf {
        settings_dir().join("bridge.json")
    }

    /// A damaged file is copied aside and reported, never deleted.
    pub fn load(report: &mut LoadReport) -> Option<BridgeCfg> {
        let p = Self::path();
        let text = std::fs::read_to_string(&p).ok()?;
        match serde_json::from_str::<BridgeCfg>(&text) {
            Ok(b) if !b.app_key.is_empty() => Some(b),
            _ => {
                let kept = keep_broken(&p);
                report.notes.push(match kept {
                    Some(_) => "Bridge: saved pairing was damaged. A copy is in the data folder.".into(),
                    None => "Bridge: saved pairing was damaged".into(),
                });
                None
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        write_atomic(&Self::path(), &serde_json::to_string_pretty(self)?)
    }

    pub fn forget() -> Result<()> {
        let p = Self::path();
        if p.exists() {
            std::fs::remove_file(p)?;
        }
        Ok(())
    }
}

impl AppConfig {
    pub fn path() -> PathBuf {
        settings_dir().join("config.json")
    }

    /// A brand-new config with every serde default applied.
    pub fn fresh() -> Self {
        serde_json::from_str("{}").unwrap_or_default()
    }

    pub fn load() -> Self {
        Self::load_reported().0
    }

    /// A damaged config.json is copied aside and the pairing salvaged from it if possible.
    pub fn load_reported() -> (Self, LoadReport) {
        let mut report = LoadReport::default();
        let p = Self::path();
        let mut cfg = match std::fs::read_to_string(&p) {
            // serde defaults (effect strength 1.0, eco on battery…) differ from Default
            Err(_) => AppConfig::fresh(),
            Ok(text) => match serde_json::from_str::<AppConfig>(&text) {
                Ok(c) => c,
                Err(_) => {
                    let kept = keep_broken(&p);
                    report.notes.push(format!(
                        "Settings: file was damaged, started with defaults.{}",
                        if kept.is_some() { " A copy is in the data folder." } else { "" }
                    ));
                    let bridge = serde_json::from_str::<serde_json::Value>(&text)
                        .ok()
                        .and_then(|v| v.get("bridge").cloned())
                        .and_then(|b| serde_json::from_value::<BridgeCfg>(b).ok());
                    AppConfig { bridge, ..AppConfig::fresh() }
                }
            },
        };
        match BridgeCfg::load(&mut report) {
            Some(b) => cfg.bridge = Some(b),
            None => {
                // older installs kept the pairing only in config.json: copy it to bridge.json
                if let Some(b) = &cfg.bridge {
                    if let Err(e) = b.save() {
                        report.notes.push(format!("Bridge: couldn't save the pairing ({e:#})"));
                    }
                }
            }
        }
        (cfg, report)
    }

    pub fn save(&self) -> Result<()> {
        write_atomic(&Self::path(), &serde_json::to_string_pretty(self)?)
    }

    pub fn bridge(&self) -> Result<lumen_hue::Bridge> {
        let b = self.bridge.as_ref().ok_or_else(|| anyhow::anyhow!("not paired yet: run `roksync pair`"))?;
        lumen_hue::Bridge::new(&b.ip, &b.app_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_and_reads_old_files() {
        let cfg = AppConfig {
            bridge: Some(BridgeCfg { ip: "1.2.3.4".into(), bridge_id: "ID".into(), name: "B".into(), app_key: "k".into(), client_key: "ck".into() }),
            ..Default::default()
        };
        let back: AppConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(back.bridge.unwrap().bridge_id, "ID");
        // a minimal file from an old build still loads, with every newer field defaulted
        let old = r#"{"bridge":{"ip":"1.2.3.4","bridge_id":"ID","name":"B","app_key":"k","client_key":"c"}}"#;
        let c: AppConfig = serde_json::from_str(old).unwrap();
        assert_eq!(c.bridge.unwrap().app_key, "k");
        assert_eq!(c.fx_gain, 1.0);
    }

    #[test]
    fn atomic_write_replaces_existing_file() {
        let dir = std::env::temp_dir().join(format!("roksync-test-{}", std::process::id()));
        let p = dir.join("x.json");
        write_atomic(&p, "one").unwrap();
        write_atomic(&p, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        assert!(!p.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
