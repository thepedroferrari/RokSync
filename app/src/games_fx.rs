//! Glue between the app and the Dota 2 and Call of Duty effect crates. CoD reads only the screen
//! (DXGI duplication), PC audio (WASAPI loopback) and the foreground window title.

use crate::config::{AppConfig, GameFxCfg};
use lumen_cod::{AudioHandle, CodState, SharedAudio};
use lumen_dota::{DotaState, SharedDota};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DOTA_PORT: u16 = lumen_dota::DEFAULT_PORT;
pub const DOTA_APPID: u32 = 570;
pub const CS2_APPID: u32 = 730;

pub type SharedCod = Arc<Mutex<CodState>>;

#[derive(Clone)]
pub struct FxSources {
    pub cs2: lumen_cs2::SharedCs2,
    pub dota: SharedDota,
    pub cod: SharedCod,
    pub sound: crate::sound::SharedSound,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Native {
    Cs2,
    Dota,
    Cod,
}

pub fn native_for(appid: u32, name: &str) -> Option<Native> {
    match appid {
        CS2_APPID => Some(Native::Cs2),
        DOTA_APPID => Some(Native::Dota),
        _ if name.to_lowercase().contains("call of duty") => Some(Native::Cod),
        _ => None,
    }
}

pub fn dota_port(cfg: &AppConfig) -> u16 {
    if cfg.dota.port == 0 {
        DOTA_PORT
    } else {
        cfg.dota.port
    }
}

/// Writes each game's default switches into its config once; an empty `base` marks "not seeded".
pub fn seed_defaults(cfg: &mut AppConfig) {
    if cfg.dota.base.is_empty() {
        cfg.dota.base = lumen_dota::Base::Cycle.id().into();
        cfg.dota.off = lumen_dota::Fx::ALL.iter().filter(|f| !f.default_on()).map(|f| f.id().to_string()).collect();
    }
    // CoD has no base; "estimated" marks that its defaults were written.
    if cfg.cod.base.is_empty() {
        cfg.cod.base = "estimated".into();
        cfg.cod.off = lumen_cod::Fx::ALL.iter().filter(|f| !f.default_on()).map(|f| f.id().to_string()).collect();
    }
}

pub fn push_dota(c: &GameFxCfg, s: &mut DotaState) {
    use lumen_dota::Fx;
    s.fx_off = Fx::ALL.iter().copied().filter(|f| !c.is_on(f.id())).collect();
    s.fx_level = Fx::ALL.iter().map(|f| (*f, c.level(f.id(), f.default_level()) as f32 / 5.0)).collect();
    s.calm = c.calm;
    s.base = lumen_dota::Base::from_id(&c.base).unwrap_or_default();
}

/// Calibration is loaded separately (`load_cod_calibration`), so pushing settings never
/// interrupts a calibration in progress.
pub fn push_cod(c: &GameFxCfg, s: &mut CodState) {
    use lumen_cod::Fx;
    s.fx_off = Fx::ALL.iter().copied().filter(|f| !c.is_on(f.id())).collect();
    for f in Fx::ALL {
        s.set_sensitivity(f, c.level(f.id(), f.default_sensitivity()));
    }
    s.calm = c.calm;
    s.only_in_front = c.only_in_front;
}

pub fn load_cod_calibration(c: &GameFxCfg, s: &mut CodState) {
    s.calibration = lumen_cod::Calibration::from_vec(&c.calibration);
}

/// PC-audio loopback plus a foreground-window poll for CoD. Dropping it stops both.
pub struct CodRuntime {
    stop: Arc<AtomicBool>,
    _audio: Option<AudioHandle>,
    pub audio: SharedAudio,
    pub error: Option<String>,
}

impl CodRuntime {
    pub fn start(cod: SharedCod) -> Self {
        let audio = SharedAudio::default();
        let (handle, error) = match lumen_cod::spawn_loopback(audio.clone()) {
            Ok(h) => (Some(h), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        let stop = Arc::new(AtomicBool::new(false));
        let (stop2, audio2) = (stop.clone(), audio.clone());
        std::thread::Builder::new()
            .name("cod-fx".into())
            .spawn(move || {
                let mut fg_at = Instant::now() - Duration::from_secs(1);
                while !stop2.load(Ordering::Relaxed) {
                    let now = Instant::now();
                    let fg = if now - fg_at > Duration::from_millis(300) {
                        fg_at = now;
                        Some(lumen_cod::foreground_title().is_some_and(|t| lumen_cod::is_cod_window_title(&t)))
                    } else {
                        None
                    };
                    let levels = audio2.lock().unwrap().numbers();
                    {
                        let mut s = cod.lock().unwrap();
                        if let Some(fg) = fg {
                            s.set_foreground(fg);
                        }
                        s.update_audio(&levels, now);
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            })
            .ok();
        Self { stop, _audio: handle, audio, error }
    }
}

impl Drop for CodRuntime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub struct GamesFx {
    pub dota: SharedDota,
    pub dota_listening: bool,
    pub cod: SharedCod,
    pub cod_rt: Option<CodRuntime>,
}

impl Default for GamesFx {
    fn default() -> Self {
        Self {
            dota: Arc::new(Mutex::new(DotaState::with_defaults())),
            dota_listening: false,
            cod: Arc::new(Mutex::new(CodState::default())),
            cod_rt: None,
        }
    }
}

impl crate::gui::LumenApp {
    pub fn fx_sources(&self) -> FxSources {
        FxSources { cs2: self.cs2.clone(), dota: self.gfx.dota.clone(), cod: self.gfx.cod.clone(), sound: self.sound.clone() }
    }

    pub fn push_game_fx(&mut self) {
        push_dota(&self.cfg.dota, &mut self.gfx.dota.lock().unwrap());
        push_cod(&self.cfg.cod, &mut self.gfx.cod.lock().unwrap());
    }

    pub fn start_dota_listener(&mut self) {
        if self.gfx.dota_listening || self.demo {
            return;
        }
        if self.cfg.dota.token.is_empty() {
            self.cfg.dota.token = lumen_dota::new_token();
            self.save_cfg();
        }
        match lumen_dota::spawn_server(dota_port(&self.cfg), self.cfg.dota.token.clone(), self.gfx.dota.clone()) {
            Ok(()) => self.gfx.dota_listening = true,
            Err(e) => {
                self.cfg.dota.enabled = false;
                self.toast(format!("Dota 2: can't receive game events ({e:#})"), true);
            }
        }
    }

    /// Writes the game-state integration file into Dota's cfg folder and starts listening.
    pub fn setup_dota(&mut self) {
        if self.demo {
            self.toast("Demo: nothing installed", false);
            return;
        }
        if self.cfg.dota.token.is_empty() {
            self.cfg.dota.token = lumen_dota::new_token();
        }
        let r = lumen_dota::cfg_dir()
            .ok_or_else(|| anyhow::anyhow!("not found in your Steam libraries"))
            .and_then(|d| lumen_dota::install_cfg(&d, dota_port(&self.cfg), &self.cfg.dota.token));
        match r {
            Ok(_) => {
                self.cfg.dota.enabled = true;
                self.cfg.fx_paused = false;
                self.save_cfg();
                self.start_dota_listener();
                self.settings_recheck();
                let note = match lumen_dota::launch_option_set() {
                    Some(true) => "Dota 2: linked. Restart Dota if it's running.",
                    _ => "Dota 2: linked. Add -gamestateintegration to its Steam launch options, then restart Dota.",
                };
                self.toast(note, false);
            }
            Err(e) => self.toast(format!("Dota 2 setup: {e:#}"), true),
        }
    }

    /// Called every frame: runs the Dota listener and CoD helper only while needed, and
    /// persists a finished CoD calibration.
    pub fn games_fx_tick(&mut self) {
        if self.cfg.dota.enabled && !self.gfx.dota_listening && !self.demo {
            self.start_dota_listener();
        }
        let page = self.page == crate::gui::Page::Cod;
        let cod_active = self.cfg.cod.enabled && !self.cfg.fx_paused && !self.demo && (self.is_streaming() || page);
        if cod_active && self.gfx.cod_rt.is_none() {
            let rt = CodRuntime::start(self.gfx.cod.clone());
            if let Some(e) = &rt.error {
                self.bg.lock().unwrap().note("Call of Duty", &format!("No PC sound: {e}"), crate::gui::theme::WARN);
            }
            self.gfx.cod_rt = Some(rt);
        } else if !cod_active && self.gfx.cod_rt.is_some() {
            self.gfx.cod_rt = None;
        }
        {
            let mut s = self.screen.lock().unwrap();
            s.cod_wanted = cod_active;
            if s.cod.is_none() {
                s.cod = Some(self.gfx.cod.clone());
            }
        }
        let cal = {
            let s = self.gfx.cod.lock().unwrap();
            (!s.calibration.in_progress() && s.calibration.is_done()).then(|| s.calibration.to_vec())
        };
        if let Some(v) = cal {
            if v != self.cfg.cod.calibration {
                self.cfg.cod.calibration = v;
                self.save_cfg();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_switches_and_routing() {
        let mut cfg = AppConfig::fresh();
        seed_defaults(&mut cfg);
        let mut d = DotaState::default();
        push_dota(&cfg.dota, &mut d);
        assert!(d.fx_off.contains(&lumen_dota::Fx::Aghs) && d.fx_off.contains(&lumen_dota::Fx::Silenced));
        assert_eq!(d.fx_off.len(), 2);
        let mut c = CodState::default();
        push_cod(&cfg.cod, &mut c);
        assert!(c.fx_off.contains(&lumen_cod::Fx::Gas) && c.fx_off.contains(&lumen_cod::Fx::Streak));
        // seeding twice keeps the user's choices
        cfg.dota.set_on("aghs", true);
        seed_defaults(&mut cfg);
        assert!(cfg.dota.is_on("aghs"));
        // strength 5 of 5 reaches the crate as 1.0; sensitivity passes straight through
        cfg.dota.levels.insert("kill".into(), 5);
        cfg.cod.levels.insert("explosion".into(), 4);
        push_dota(&cfg.dota, &mut d);
        push_cod(&cfg.cod, &mut c);
        assert_eq!(d.level(lumen_dota::Fx::Kill), 1.0);
        assert_eq!(c.sensitivity(lumen_cod::Fx::Explosion), 4);
        load_cod_calibration(&cfg.cod, &mut c);
        assert!(!c.calibration.is_done());
        assert_eq!(native_for(730, "Counter-Strike 2"), Some(Native::Cs2));
        assert_eq!(native_for(570, "Dota 2"), Some(Native::Dota));
        assert_eq!(native_for(1, "Call of Duty®"), Some(Native::Cod));
        assert_eq!(native_for(2, "Hades"), None);
    }
}
