//! cargo run -p lumen-mood --example mood_test --release -- "lfm2.5-1.2b,qwen3.5-0.8b" "Game den"
//! Downloads runtime+models if needed, then prints plans for your real lights (read-only).
use lumen_mood::{catalog, download, server::LlmServer};
use std::time::Instant;

const PROMPTS: &[&str] = &[
    "rainy neon Tokyo night, calm",
    "cozy autumn evening with a book",
    "intense boss fight",
    "the moment just before a thunderstorm",
    "underwater cathedral",
];

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ids = args.first().cloned().unwrap_or_else(|| "lfm2.5-1.2b".into());
    let scope = args.get(1).cloned();

    let cfg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(lumen_mood::app_dir(dirs::config_dir()).join("config.json"))?)?;
    let b = &cfg["bridge"];
    let bridge = lumen_hue::Bridge::new(b["ip"].as_str().unwrap(), b["app_key"].as_str().unwrap())?;
    let inv = bridge.inventory()?;
    download::install_runtime(&|_, _| {})?;

    for id in ids.split(',') {
        let model = catalog::find(id).expect("unknown model");
        println!("\n===== {} =====", model.name);
        download::install_model(model, &|_, _| {})?;
        let t = Instant::now();
        let srv = LlmServer::start(model)?;
        println!("loaded in {:.1}s", t.elapsed().as_secs_f32());
        for p in PROMPTS {
            let t = Instant::now();
            match srv.ask(p, &inv, scope.as_deref()) {
                Ok(plan) => {
                    println!("> {p} ({:.1}s) \"{}\" motion={}/{}", t.elapsed().as_secs_f32(), plan.title, plan.motion, plan.speed);
                    for a in &plan.assignments {
                        println!("    {:<28} {:>3}%  {}", a.target, a.brightness, a.colors.join(" "));
                    }
                }
                Err(e) => println!("> {p} FAILED: {e:#}"),
            }
        }
    }
    Ok(())
}
