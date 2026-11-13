//! Fallback when no model is installed: a fixed mood → palette table matched by keywords.

use crate::plan::MoodPlan;
use lumen_hue::Inventory;

struct Entry {
    words: &'static [&'static str],
    palette: &'static [&'static str],
    brightness: f64,
    motion: &'static str,
}

const TABLE: &[Entry] = &[
    Entry { words: &["sunset", "golden hour", "dusk"], palette: &["#ff6a00", "#ff2e63", "#8a2be2", "#ffb347"], brightness: 55.0, motion: "drift" },
    Entry { words: &["sunrise", "morning", "dawn", "wake"], palette: &["#ffd29d", "#ffb27a", "#ffe8c7"], brightness: 75.0, motion: "still" },
    Entry { words: &["ocean", "sea", "beach", "underwater", "wave"], palette: &["#0077be", "#00c2c7", "#004e7c", "#7fdbda"], brightness: 50.0, motion: "drift" },
    Entry { words: &["forest", "woods", "jungle", "nature", "moss"], palette: &["#1b5e20", "#4caf50", "#a5d610", "#2e3b1f"], brightness: 45.0, motion: "drift" },
    Entry { words: &["fire", "fireplace", "campfire", "candle", "flame"], palette: &["#ff4500", "#ff8c00", "#ffb000", "#b22222"], brightness: 45.0, motion: "flicker" },
    Entry { words: &["neon", "cyberpunk", "synthwave", "tokyo", "vaporwave", "retro"], palette: &["#ff00a0", "#00e5ff", "#7a00ff", "#ff2bd6"], brightness: 70.0, motion: "drift" },
    Entry { words: &["rain", "rainy", "storm", "thunder"], palette: &["#2a4b7c", "#5d7ea8", "#1c2c44", "#9fb6d6"], brightness: 35.0, motion: "breathe" },
    Entry { words: &["space", "galaxy", "nebula", "cosmic", "stars"], palette: &["#2b0a5c", "#6a00f4", "#db00b6", "#0b1d51"], brightness: 40.0, motion: "drift" },
    Entry { words: &["aurora", "northern lights", "arctic"], palette: &["#00ff9c", "#00b3ff", "#7b2ff7", "#0a3d62"], brightness: 50.0, motion: "drift" },
    Entry { words: &["calm", "relax", "chill", "meditat", "zen", "spa"], palette: &["#5e60ce", "#48bfe3", "#80ffdb", "#6930c3"], brightness: 30.0, motion: "breathe" },
    Entry { words: &["cozy", "cosy", "warm", "hygge", "autumn", "fall"], palette: &["#ff9e57", "#ffc489", "#d9480f", "#ffe3b3"], brightness: 45.0, motion: "still" },
    Entry { words: &["focus", "work", "study", "read", "productive"], palette: &["#f4f8ff", "#dce9ff", "#fff5e6"], brightness: 95.0, motion: "still" },
    Entry { words: &["sleep", "bedtime", "night light", "wind down"], palette: &["#ff7a1a", "#b33c00", "#5a1e00"], brightness: 8.0, motion: "still" },
    Entry { words: &["party", "dance", "club", "rave", "disco"], palette: &["#ff0054", "#00f5d4", "#fee440", "#9b5de5", "#00bbf9"], brightness: 90.0, motion: "drift" },
    Entry { words: &["romantic", "date", "love", "valentine"], palette: &["#ff1f5a", "#c9184a", "#ff8fab", "#590d22"], brightness: 30.0, motion: "breathe" },
    Entry { words: &["horror", "scary", "creepy", "halloween", "spooky"], palette: &["#8b0000", "#ff4f00", "#2d0036", "#3cff00"], brightness: 25.0, motion: "flicker" },
    Entry { words: &["christmas", "xmas", "festive", "holiday"], palette: &["#c1121f", "#2d6a4f", "#ffd166", "#fefae0"], brightness: 60.0, motion: "breathe" },
    Entry { words: &["desert", "sand", "dune", "sahara"], palette: &["#e9a15b", "#c86b25", "#f6d7a7", "#8c4a1c"], brightness: 60.0, motion: "still" },
    Entry { words: &["matrix", "hacker", "terminal", "code"], palette: &["#00ff41", "#008f11", "#003b00", "#0d0208"], brightness: 50.0, motion: "flicker" },
    Entry { words: &["ice", "frozen", "winter", "snow", "cold"], palette: &["#e0f7ff", "#9bd7ff", "#4fa3d9", "#ffffff"], brightness: 65.0, motion: "still" },
    Entry { words: &["lava", "volcano", "hell", "inferno"], palette: &["#ff2a00", "#ff7b00", "#600000", "#ffbb00"], brightness: 55.0, motion: "flicker" },
    Entry { words: &["cinema", "movie", "film", "theater", "theatre"], palette: &["#1a1a40", "#3a0ca3", "#ff9f1c"], brightness: 12.0, motion: "still" },
    Entry { words: &["gaming", "game", "esports", "rgb"], palette: &["#7b2cff", "#00e1ff", "#ff2079"], brightness: 70.0, motion: "drift" },
    Entry { words: &["tropical", "summer", "miami", "palm"], palette: &["#ff5d8f", "#00d4b4", "#ffbe0b", "#3a86ff"], brightness: 75.0, motion: "drift" },
    Entry { words: &["lavender", "purple", "violet"], palette: &["#b388eb", "#8338ec", "#e0aaff"], brightness: 50.0, motion: "still" },
    Entry { words: &["boss", "fight", "battle", "combat", "war", "intense", "action", "danger", "alarm"], palette: &["#a00000", "#ff4500", "#ff0000", "#2b0a5c"], brightness: 85.0, motion: "flicker" },
    Entry { words: &["sad", "melanchol", "lonely", "blue mood"], palette: &["#2a4b7c", "#3a0ca3", "#0b1d51"], brightness: 20.0, motion: "breathe" },
    Entry { words: &["happy", "joy", "cheer", "sunny", "spring"], palette: &["#ffc93c", "#7dffc4", "#ff9ec4", "#48bfe3"], brightness: 80.0, motion: "drift" },
];

/// Curated palette for the moods mentioned in `prompt`, if any.
pub fn hint(prompt: &str) -> Option<Vec<String>> {
    let p = prompt.to_lowercase();
    let hits: Vec<&Entry> = TABLE.iter().filter(|e| e.words.iter().any(|w| p.contains(w))).collect();
    if hits.is_empty() {
        return None;
    }
    let mut pal: Vec<String> = Vec::new();
    for i in 0..6 {
        for h in &hits {
            if let Some(c) = h.palette.get(i) {
                if pal.len() < 6 && !pal.contains(&c.to_string()) {
                    pal.push(c.to_string());
                }
            }
        }
    }
    Some(pal)
}

pub fn interpret(prompt: &str, inv: &Inventory, scope: Option<&str>) -> MoodPlan {
    let p = prompt.to_lowercase();
    let hits: Vec<&Entry> = TABLE.iter().filter(|e| e.words.iter().any(|w| p.contains(w))).collect();
    let (palette, brightness, motion): (Vec<String>, f64, String) = if hits.is_empty() {
        (vec!["#ffb46b".into(), "#ff7a59".into(), "#ffd6a5".into()], 55.0, "still".into())
    } else {
        let mut pal = Vec::new();
        // interleave so two moods ("rainy neon") mix rather than stack
        for i in 0..6 {
            for h in &hits {
                if let Some(c) = h.palette.get(i) {
                    if pal.len() < 6 && !pal.contains(&c.to_string()) {
                        pal.push(c.to_string());
                    }
                }
            }
        }
        let bri = hits.iter().map(|h| h.brightness).sum::<f64>() / hits.len() as f64;
        (pal, bri, hits[0].motion.to_string())
    };
    // named subjects (Brazil, Barbie, Ferrari) lead with their real colours; the mood adds accents
    let facts = crate::facts::find(prompt);
    let palette = if facts.is_empty() {
        palette
    } else {
        let mut pal: Vec<String> = Vec::new();
        for c in facts.iter().flat_map(|f| f.colors.iter()).filter_map(|c| crate::colors::resolve(c)) {
            if pal.len() < 5 && !pal.contains(&c) {
                pal.push(c);
            }
        }
        if !hits.is_empty() {
            let accents: Vec<String> = palette.into_iter().filter(|c| !pal.contains(c)).take(2).collect();
            pal.extend(accents);
        }
        pal
    };
    let brightness = if p.contains("dim") || p.contains("dark") { brightness * 0.5 } else if p.contains("bright") { (brightness * 1.4).min(100.0) } else { brightness };

    let assignments = crate::plan::arrange(&crate::plan::scope_lights(inv, scope), &palette, brightness, None);
    let speed = if p.contains("fast") || p.contains("energetic") || p.contains("party") { "fast" } else { "slow" };
    let title = prompt.split_whitespace().take(4).collect::<Vec<_>>().join(" ");
    MoodPlan { title, palette, assignments, motion, speed: speed.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_two_moods() {
        let plan = interpret("rainy neon tokyo night", &Inventory::default(), None);
        assert!(plan.palette.contains(&"#ff00a0".to_string()));
        assert!(plan.palette.contains(&"#2a4b7c".to_string()));
    }

    #[test]
    fn named_subjects_use_their_real_colours() {
        let plan = interpret("Brazil", &Inventory::default(), None);
        assert_eq!(plan.palette[..3], ["#00c040", "#ffe400", "#2040e0"]);
        let plan = interpret("Brazil beach", &Inventory::default(), None);
        assert_eq!(plan.palette[0], "#00c040");
        assert!(plan.palette.contains(&"#0077be".to_string()));
    }

    #[test]
    fn plan_schema_has_targets() {
        let s = crate::plan::schema(&["Desk".into()]);
        assert_eq!(s["properties"]["focus"]["enum"][1], "Desk");
    }
}
