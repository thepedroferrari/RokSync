//! cargo run -p lumen-mood --example mood_probe --release -- lfm2.5-1.2b "Brazil" "Japan" ...
//! Prints the model's art direction for each prompt and the palette RokSync ends up with.
use lumen_mood::{catalog, colors, server::LlmServer};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let id = args.next().unwrap_or_else(|| "lfm2.5-1.2b".into());
    // a catalog id, or any .gguf file in the models folder
    let model: &'static catalog::ModelSpec = catalog::find(&id).unwrap_or_else(|| {
        let f: &'static str = Box::leak(id.clone().into_boxed_str());
        Box::leak(Box::new(catalog::ModelSpec { id: "probe", name: f, file: f, url: "", sha256: "", bytes: 0, license: "", blurb: "", temperature: 0.2, retired: false }))
    });
    let prompts: Vec<String> = args.collect();
    // LUMEN_CPU=1: run like the Low preset (no GPU)
    let gpu = std::env::var("LUMEN_CPU").is_err();
    let srv = LlmServer::start_with(model, lumen_mood::server::RunOpts { gpu, threads: None })?;
    let home = "Room \"Game den\":\n  - light \"Desk strip\" (gradient strip, 5 colour points)\n  - light \"Lamp\" (colour)\n  - light \"Ceiling\" (colour)\n";
    let targets = vec!["Ceiling".to_string(), "Desk strip".into(), "Game den".into(), "Lamp".into()];
    for p in &prompts {
        let t = Instant::now();
        match srv.direct(p, home, &targets) {
            Ok(r) => {
                let words = r.palette.join(", ");
                let evokes = r.evokes.clone();
                let plan = r.into_plan(&[]);
                let named: Vec<&str> = plan.palette.iter().map(|h| colors::nearest_name(h)).collect();
                println!("{p:<26} {:.1}s  [{}]  \"{}\" {}/{}", t.elapsed().as_secs_f32(), named.join(", "), plan.title, plan.motion, plan.speed);
                println!("{:>28}model said: {words}\n{:>28}{evokes}", "", "");
            }
            Err(e) => println!("{p:<26} FAILED {e:#}"),
        }
    }
    Ok(())
}
