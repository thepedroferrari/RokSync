//! Scores local models on picking recognisable colours.
//!
//! cargo run -p lumen-mood --example mood_bench --release -- <file.gguf> [<file.gguf> ...]
//! (files in %LOCALAPPDATA%\RokSync\models). Prints a table and per-prompt details.
//!
//! Two passes per model: "alone" (no fact sheet, no mood hints) and "with notes".
//! Each prompt lists the colour families a person would expect. A palette scores the share of
//! expected families it hits, minus a penalty for colours that don't belong.
use lumen_mood::{catalog::ModelSpec, server::LlmServer};
use std::time::Instant;

const HOME: &str = "Room \"Game den\":\n  - light \"Desk strip\" (gradient strip, 5 colour points)\n  - light \"Lamp\" (colour)\n  - light \"Ceiling\" (colour)\n";

/// prompt, expected families, also-fine families (not penalised)
const CASES: &[(&str, &[&str], &[&str])] = &[
    ("Brazil", &["green", "yellow", "blue"], &["white"]),
    ("Japan", &["red", "white"], &["pink"]),
    ("Sweden", &["blue", "yellow"], &["white"]),
    ("Ireland", &["green", "orange"], &["white"]),
    ("Italy", &["green", "red"], &["white"]),
    ("Germany", &["red", "yellow"], &["orange"]),
    ("Argentina", &["blue", "white"], &["yellow"]),
    ("Netherlands", &["orange"], &["red", "blue", "white"]),
    ("Greece", &["blue", "white"], &["cyan"]),
    ("Ferrari", &["red"], &["yellow", "white"]),
    ("McDonald's", &["yellow", "red"], &["white"]),
    ("Spotify", &["green"], &["white"]),
    ("Starbucks", &["green"], &["white", "orange"]),
    ("Coca-Cola", &["red"], &["white"]),
    ("Barbie", &["pink"], &["white", "purple"]),
    ("Shrek", &["green"], &["yellow", "orange"]),
    ("Minecraft", &["green"], &["orange", "cyan", "blue"]),
    ("the Simpsons", &["yellow", "blue"], &["orange", "pink", "white"]),
    ("Pikachu", &["yellow"], &["red", "orange"]),
    ("Super Mario", &["red", "blue"], &["yellow"]),
    ("the Hulk", &["green", "purple"], &[]),
    ("Superman", &["blue", "red"], &["yellow"]),
    ("Mars", &["red", "orange"], &["yellow"]),
    ("a pine forest", &["green"], &["yellow", "orange"]),
    ("the deep ocean", &["blue"], &["cyan", "purple"]),
    ("Christmas", &["red", "green"], &["yellow", "white"]),
    ("Halloween", &["orange", "purple"], &["green", "red"]),
    ("St Patrick's Day", &["green"], &["yellow", "white"]),
    ("Valentine's Day", &["red", "pink"], &["purple"]),
    ("tropical sunset", &["orange", "pink"], &["purple", "red", "yellow"]),
];

fn family(hex: &str) -> &'static str {
    let c = lumen_hue::Rgb::from_hex(hex).unwrap();
    let (r, g, b) = (c.r, c.g, c.b);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let s = if max > 0.0 { (max - min) / max } else { 0.0 };
    if s < 0.22 {
        return if max > 0.6 { "white" } else { "grey" };
    }
    let d = max - min;
    let h = if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    match h {
        h if h < 12.0 || h >= 340.0 => "red",
        h if h < 38.0 => "orange",
        h if h < 70.0 => "yellow",
        h if h < 160.0 => "green",
        h if h < 195.0 => "cyan",
        h if h < 255.0 => "blue",
        h if h < 290.0 => "purple",
        _ => "pink",
    }
}

fn score(palette: &[String], want: &[&str], ok: &[&str]) -> f32 {
    let fams: Vec<&str> = palette.iter().map(|h| family(h)).collect();
    let hit = want.iter().filter(|w| fams.contains(w)).count() as f32 / want.len() as f32;
    let junk = fams.iter().filter(|f| !want.contains(f) && !ok.contains(f)).count() as f32;
    (hit - 0.15 * junk).max(0.0)
}

fn main() -> anyhow::Result<()> {
    let files: Vec<String> = std::env::args().skip(1).collect();
    let targets = vec!["Ceiling".to_string(), "Desk strip".into(), "Game den".into(), "Lamp".into()];
    let mut summary = Vec::new();
    for file in &files {
        let spec: &'static ModelSpec = Box::leak(Box::new(ModelSpec {
            id: "bench",
            name: Box::leak(file.clone().into_boxed_str()),
            file: Box::leak(file.clone().into_boxed_str()),
            url: "",
            sha256: "",
            bytes: 0,
            license: "",
            blurb: "",
            temperature: 0.2,
            retired: false,
        }));
        println!("\n===== {file}");
        let t = Instant::now();
        let srv = match LlmServer::start(spec) {
            Ok(s) => s,
            Err(e) => {
                println!("failed to start: {e:#}");
                continue;
            }
        };
        let load = t.elapsed().as_secs_f32();
        let mut totals = [0f32; 2];
        let mut fails = 0;
        let mut secs = 0f32;
        for (pass, notes) in [(0, false), (1, true)] {
            for (p, want, ok) in CASES {
                let t = Instant::now();
                let r = srv.direct_with(p, HOME, &targets, notes);
                secs += t.elapsed().as_secs_f32();
                match r {
                    Ok(raw) => {
                        let words = raw.palette.join(", ");
                        let plan = raw.into_plan(&[]);
                        let s = score(&plan.palette, want, ok);
                        totals[pass] += s;
                        if pass == 0 {
                            let fams: Vec<&str> = plan.palette.iter().map(|h| family(h)).collect();
                            println!("  {:>4.0}%  {p:<18} {words}   [{}]", s * 100.0, fams.join(" "));
                        }
                    }
                    Err(e) => {
                        fails += 1;
                        println!("  FAIL  {p:<18} {}", format!("{e:#}").chars().take(120).collect::<String>());
                    }
                }
            }
        }
        let n = CASES.len() as f32;
        summary.push(format!(
            "{file:<42} alone {:>4.0}%   with notes {:>4.0}%   {:.2}s/answer   load {:.1}s   fails {fails}",
            totals[0] / n * 100.0,
            totals[1] / n * 100.0,
            secs / (2.0 * n),
            load
        ));
    }
    println!("\n===== summary");
    for s in summary {
        println!("{s}");
    }
    Ok(())
}
