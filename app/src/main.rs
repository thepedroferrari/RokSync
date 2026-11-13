// A GUI app: Windows opens no console window for it. The CLI subcommands borrow the console
// of the terminal they were started from (see `attach_console`).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod autolive;
mod config;
mod demo;
mod games_fx;
mod gui;
mod icon_art;
mod instance;
mod mix;
mod modes;
mod perf;
mod screen_service;
mod sound;
mod steam;
mod system;
mod sync;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use config::{AppConfig, BridgeCfg};
use lumen_hue::{api::Target, LightState, Rgb};
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(name = "roksync", about = "RokSync: gaming-focused ambient lighting sync for Philips Hue. Your game. Your lights. In sync.", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
    /// Open minimised (used when starting with Windows)
    #[arg(long, global = true)]
    minimized: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Open the desktop app (default)
    Gui,
    /// Open the app with a sample home. No bridge needed, nothing is saved
    Demo,
    /// Look for bridges on the local network
    Discover,
    /// Pair with a bridge (press its link button when asked)
    Pair { ip: Option<String> },
    /// List rooms, zones, lights, entertainment areas and scenes
    List,
    /// Set a light, room or zone to a colour: roksync set "Desk gradient" "#ff00aa"
    Set {
        name: String,
        color: String,
        #[arg(long)]
        brightness: Option<f64>,
    },
    /// Set a gradient light's points: roksync gradient "Desk gradient" "#ff0000" "#0000ff"
    Gradient { name: String, colors: Vec<String> },
    /// Turn a light/room/zone on or off
    Power { name: String, state: String },
    /// Stream a look to an entertainment area for a few seconds
    Stream {
        #[arg(long)]
        area: Option<String>,
        #[arg(long, default_value_t = 10)]
        seconds: u64,
        #[arg(long)]
        breathe: Option<f32>,
        colors: Vec<String>,
    },
    /// Describe a mood and get a lighting plan: roksync mood "rainy neon night" --scope "Battlestation" --apply
    Mood {
        prompt: String,
        #[arg(long)]
        scope: Option<String>,
        /// Apply it to the lights (otherwise only print the plan)
        #[arg(long)]
        apply: bool,
        /// Use Quick mode even if a model is installed
        #[arg(long)]
        quick: bool,
    },
    /// Counter-Strike 2 integration
    Cs2 {
        #[command(subcommand)]
        cmd: Cs2Cmd,
    },
}

#[derive(Subcommand)]
enum Cs2Cmd {
    /// Add RokSync's game state config to your CS2 folder
    Install,
    /// Print game events as they arrive (start CS2 after installing)
    Watch,
}

/// Started from a terminal: print there (help, errors, CLI output). From Explorer or the Run
/// key there is no parent console, and this does nothing.
#[cfg(windows)]
fn attach_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // SAFETY: no pointers; fails harmlessly when there is no parent console.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}
#[cfg(not(windows))]
fn attach_console() {}

fn main() -> Result<()> {
    attach_console();
    let cli = Cli::parse();
    gui::START_MINIMIZED.store(cli.minimized, std::sync::atomic::Ordering::Relaxed);
    match cli.cmd.unwrap_or(Cmd::Gui) {
        Cmd::Gui => {
            if !instance::claim() {
                return Ok(()); // already running: that copy brings its window forward
            }
            let moved = config::migrate_from_lumen();
            let (cfg, mut report) = AppConfig::load_reported();
            report.notes.extend(moved);
            perf::set(cfg.perf);
            gui::run(cfg, report.notes, false)
        }
        Cmd::Demo => {
            let mut cfg = AppConfig::load();
            cfg.bridge = None;
            cfg.cs2.enabled = true;
            cfg.dota.enabled = true;
            cfg.cod.enabled = true;
            cfg.fx_paused = false;
            perf::set(cfg.perf);
            gui::run(cfg, Vec::new(), true)
        }
        Cmd::Discover => {
            let found = lumen_hue::discover::discover(Duration::from_secs(4));
            if found.is_empty() {
                println!("No bridges found. Try `roksync pair <ip>`.");
            }
            for f in found {
                let info = lumen_hue::pair::bridge_info(&f.ip).map(|i| format!("{} ({}, {})", i.name, i.model_id, i.bridge_id));
                println!("{:<16} via {:<16} {}", f.ip, f.source, info.unwrap_or_else(|e| format!("unreachable: {e}")));
            }
            Ok(())
        }
        Cmd::Pair { ip } => pair_cli(ip),
        Cmd::List => list(),
        Cmd::Set { name, color, brightness } => {
            let c = parse_color(&color)?;
            apply(&name, LightState { brightness, ..LightState::color(c) })
        }
        Cmd::Gradient { name, colors } => {
            let pts = colors.iter().map(|c| parse_color(c)).collect::<Result<Vec<_>>>()?;
            if pts.len() < 2 {
                bail!("give at least 2 colours");
            }
            apply(&name, LightState::gradient(pts))
        }
        Cmd::Power { name, state } => apply(&name, LightState::power(matches!(state.as_str(), "on" | "1" | "true"))),
        Cmd::Stream { area, seconds, breathe, colors } => stream_cli(area, seconds, breathe, colors),
        Cmd::Cs2 { cmd } => cs2_cli(cmd),
        Cmd::Mood { prompt, scope, apply, quick } => mood_cli(&prompt, scope.as_deref(), apply, quick),
    }
}

fn mood_cli(prompt: &str, scope: Option<&str>, apply: bool, quick: bool) -> Result<()> {
    use lumen_mood::catalog;
    let cfg = AppConfig::load();
    let bridge = cfg.bridge()?;
    let inv = bridge.inventory()?;
    let model = cfg
        .mood_model
        .as_deref()
        .and_then(catalog::find)
        .filter(|m| !quick && catalog::model_installed(m) && catalog::server_exe().is_some());
    let plan = match model {
        Some(m) => {
            eprintln!("Using {}…", m.name);
            lumen_mood::server::LlmServer::start(m)?.ask(prompt, &inv, scope)?
        }
        None => lumen_mood::keywords::interpret(prompt, &inv, scope),
    };
    println!("{}  ({} / {})", plan.title, plan.motion, plan.speed);
    println!("palette: {}", plan.palette.join(" "));
    for a in &plan.assignments {
        println!("  {:<30} {:>3.0}%  {}", a.target.trim(), a.brightness, a.colors.join(" "));
    }
    if apply {
        for (id, _, st) in plan.to_states(&inv) {
            bridge.set_light(&id, &st)?;
            std::thread::sleep(Duration::from_millis(110));
        }
        println!("applied");
    }
    Ok(())
}

fn parse_color(s: &str) -> Result<Rgb> {
    Rgb::from_hex(s).with_context(|| format!("'{s}' is not a #rrggbb colour"))
}

fn pair_cli(ip: Option<String>) -> Result<()> {
    let ip = match ip {
        Some(ip) => ip,
        None => {
            let found = lumen_hue::discover::discover(Duration::from_secs(4));
            found.first().map(|f| f.ip.clone()).context("no bridge found; pass its IP: roksync pair 192.168.x.x")?
        }
    };
    let info = lumen_hue::pair::bridge_info(&ip)?;
    println!("Bridge \"{}\" ({}) at {ip}", info.name, info.model_id);
    println!("Press the round link button on the bridge now (waiting 60 s)…");
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if let Some(r) = lumen_hue::pair::try_pair(&ip, "lumen#windows")? {
            let b = BridgeCfg { ip: ip.clone(), bridge_id: info.bridge_id, name: info.name, app_key: r.app_key, client_key: r.client_key };
            b.save()?;
            let mut cfg = AppConfig::load();
            cfg.bridge = Some(b);
            cfg.save()?;
            println!("Paired. Saved to {}", BridgeCfg::path().display());
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(1500));
    }
    bail!("timed out waiting for the link button")
}

fn list() -> Result<()> {
    let inv = AppConfig::load().bridge()?.inventory()?;
    println!("ROOMS");
    for r in &inv.rooms {
        println!("  {}", r.metadata.name);
        for l in inv.group_lights(r) {
            print_light(l);
        }
    }
    println!("ZONES");
    for z in &inv.zones {
        let names: Vec<_> = inv.group_lights(z).iter().map(|l| l.metadata.name.clone()).collect();
        println!("  {}: {}", z.metadata.name, names.join(", "));
    }
    println!("ENTERTAINMENT AREAS");
    for e in &inv.ent_configs {
        println!("  {} [{}] {} channels, {}", e.metadata.name, e.configuration_type, e.channels.len(), e.status);
        for ch in &e.channels {
            let lights: Vec<_> = inv
                .channel_lights(ch)
                .iter()
                .map(|(l, i)| format!("{}#{}", l.metadata.name, i))
                .collect();
            println!(
                "    ch{:<2} ({:+.2},{:+.2},{:+.2}) {}",
                ch.channel_id, ch.position.x, ch.position.y, ch.position.z, lights.join(", ")
            );
        }
    }
    println!("SCENES ({})", inv.scenes.len());
    for s in &inv.scenes {
        let cols: Vec<_> = s.colors().iter().map(|c| c.to_hex()).collect();
        println!("  {:<28} {:<16} {}", s.metadata.name, inv.group_name(&s.group.rid).unwrap_or("?"), cols.join(" "));
    }
    Ok(())
}

fn print_light(l: &lumen_hue::Light) {
    let grad = if l.gradient_points() > 0 { format!(" gradient×{}", l.gradient_points()) } else { String::new() };
    println!(
        "    - {:<28} {:<4} {:>3.0}%  {}{}",
        l.metadata.name,
        if l.on.on { "on" } else { "off" },
        l.brightness(),
        l.current_rgb().to_hex(),
        grad
    );
}

fn apply(name: &str, state: LightState) -> Result<()> {
    let bridge = AppConfig::load().bridge()?;
    let inv = bridge.inventory()?;
    match inv.find_target(name).with_context(|| format!("no light, room or zone called '{name}'"))? {
        Target::Light(l) => bridge.set_light(&l.id, &state)?,
        Target::Group(g) => {
            let gl = inv.grouped_light_of(g).context("group has no grouped_light service")?;
            bridge.set_grouped_light(gl, &state)?
        }
    }
    println!("ok");
    Ok(())
}

fn stream_cli(area: Option<String>, seconds: u64, breathe: Option<f32>, colors: Vec<String>) -> Result<()> {
    use lumen_engine::*;
    let cfg = AppConfig::load();
    let bridge = cfg.bridge()?;
    let client_key = cfg.bridge.as_ref().unwrap().client_key.clone();
    let inv = bridge.inventory()?;
    let ec = match &area {
        Some(n) => inv.ent_configs.iter().find(|e| e.metadata.name.eq_ignore_ascii_case(n)),
        None => inv.ent_configs.first(),
    }
    .context("entertainment area not found")?
    .clone();
    let cols = if colors.is_empty() {
        vec![Rgb::new(1.0, 0.0, 0.6), Rgb::new(0.0, 0.8, 1.0)]
    } else {
        colors.iter().map(|c| parse_color(c)).collect::<Result<Vec<_>>>()?
    };
    let program = Program {
        look: if cols.len() == 1 { Look::Solid(cols[0]) } else { Look::Gradient { stops: cols, axis: Axis::LeftRight } },
        motion: match breathe {
            Some(p) => Motion::Breathe { period_s: p, depth: 0.85, spread: 0.3 },
            None => Motion::Static,
        },
        brightness: 1.0,
    };
    println!("Streaming to \"{}\" for {seconds}s…", ec.metadata.name);
    let mut s = lumen_hue::Streamer::start(&bridge, &client_key, &ec.id)?;
    let chans = sync::channel_positions(&ec);
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(seconds) {
        s.send(&render(&program, &chans, t0.elapsed().as_secs_f32()))?;
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    drop(s);
    println!("done");
    Ok(())
}

fn cs2_cli(cmd: Cs2Cmd) -> Result<()> {
    let mut cfg = AppConfig::load();
    if cfg.cs2.token.is_empty() {
        cfg.cs2.token = lumen_cs2::new_token();
        cfg.save()?;
    }
    match cmd {
        Cs2Cmd::Install => {
            let dir = lumen_cs2::find_cfg_dir().context("couldn't find CS2 in your Steam libraries")?;
            let p = lumen_cs2::install_cfg(&dir, cfg.cs2.port, &cfg.cs2.token)?;
            cfg.cs2.enabled = true;
            cfg.save()?;
            println!("Installed {}\nRestart CS2 for it to take effect.", p.display());
        }
        Cs2Cmd::Watch => {
            let st = lumen_cs2::SharedCs2::default();
            lumen_cs2::spawn_server(cfg.cs2.port, cfg.cs2.token.clone(), st.clone())?;
            println!("Listening on 127.0.0.1:{}. Play CS2, Ctrl+C to stop.", cfg.cs2.port);
            let mut last = String::new();
            loop {
                std::thread::sleep(Duration::from_millis(200));
                let s = st.lock().unwrap();
                let line = format!(
                    "map={} phase={} team={} hp={} flashed={} burning={} spectating={} event={}",
                    s.map, s.round_phase, s.team, s.health, s.flashed, s.burning, s.spectating, s.last_event
                );
                if line != last {
                    println!("{line}");
                    last = line;
                }
            }
        }
    }
    Ok(())
}
