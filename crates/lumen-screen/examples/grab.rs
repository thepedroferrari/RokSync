//! cargo run -p lumen-screen --example grab --release
use lumen_screen::capture::{monitors, Capturer, Grab};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let mons = monitors()?;
    for m in &mons {
        println!("monitor {}:{} {} at ({},{})", m.adapter, m.output, m.label(), m.left, m.top);
    }
    let m = mons.first().expect("no monitor");
    let mut cap = Capturer::new(m, 256)?;
    let mut got = 0;
    let t0 = Instant::now();
    let mut times = Vec::new();
    while t0.elapsed().as_secs_f32() < 3.0 {
        let t = Instant::now();
        if let Grab::Frame(f) = cap.grab(50)? {
            times.push(t.elapsed().as_secs_f32() * 1000.0);
            got += 1;
            if got == 1 {
                let n = (f.width * f.height) as f32;
                let (mut r, mut g, mut b) = (0.0, 0.0, 0.0);
                for y in 0..f.height {
                    for x in 0..f.width {
                        let p = f.px(x, y);
                        r += p[0];
                        g += p[1];
                        b += p[2];
                    }
                }
                println!("frame {}x{} avg rgb {:.2} {:.2} {:.2}", f.width, f.height, r / n, g / n, b / n);
            }
        }
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = times.get(times.len() / 2).copied().unwrap_or(0.0);
    println!("{got} changed frames in 3 s, median grab {med:.2} ms");
    println!("windows: {:?}", lumen_screen::windows_list::list().iter().take(8).map(|w| &w.title).collect::<Vec<_>>());
    Ok(())
}
