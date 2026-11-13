//! Colour words → values tuned for Hue lights. The model writes colours as people say them
//! ("canary yellow", "navy", "pastel pink"); `resolve` turns them into light-friendly hex values.

/// Exact phrases. Values are pushed toward saturation: a light can't show muddy or near-black colours.
pub const NAMED: &[(&str, &str)] = &[
    ("blood red", "#a00000"),
    ("crimson", "#dc143c"),
    ("scarlet", "#ff2400"),
    ("ferrari red", "#ff1a00"),
    ("racing red", "#ff1a00"),
    ("cherry", "#d2042d"),
    ("pure red", "#ff0000"),
    ("red", "#ff0000"),
    ("maroon", "#800020"),
    ("burgundy", "#800020"),
    ("wine", "#8b0030"),
    ("ruby", "#e0115f"),
    ("rose", "#ff3366"),
    ("coral", "#ff6f61"),
    ("salmon", "#ff8c69"),
    ("hot pink", "#ff1f8f"),
    ("neon pink", "#ff00a0"),
    ("bubblegum", "#ff6fb5"),
    ("barbie pink", "#ff1fa0"),
    ("pink", "#ff4fa3"),
    ("soft pink", "#ff9ec4"),
    ("pastel pink", "#ffb3d1"),
    ("blush", "#ffb3c6"),
    ("fuchsia", "#ff00c8"),
    ("magenta", "#ff00ff"),
    ("ember orange", "#ff4500"),
    ("sunset orange", "#ff6a00"),
    ("orange", "#ff6a00"),
    ("tangerine", "#ff8c00"),
    ("saffron", "#ff7f1a"),
    ("peach", "#ffb07a"),
    ("apricot", "#ffa060"),
    ("rust", "#c0400a"),
    ("terracotta", "#d2552a"),
    ("copper", "#d2691e"),
    ("bronze", "#cd7f32"),
    ("brown", "#8c4a1c"),
    ("chocolate", "#7b3f00"),
    ("caramel", "#d2802a"),
    ("tan", "#e0a060"),
    ("sand", "#f0c080"),
    ("beige", "#f5d6a8"),
    ("cream", "#fff0d0"),
    ("ivory", "#fff6e0"),
    ("amber", "#ffb000"),
    ("gold", "#ffc93c"),
    ("golden", "#ffc93c"),
    ("honey", "#ffb030"),
    ("mustard", "#e0b000"),
    ("canary yellow", "#ffe400"),
    ("lemon yellow", "#fff200"),
    ("lemon", "#fff200"),
    ("yellow", "#ffd400"),
    ("sunshine", "#ffd23c"),
    ("candlelight", "#ff9329"),
    ("firelight", "#ff8020"),
    ("lamplight", "#ffa040"),
    ("warm white", "#ffd8a8"),
    ("soft white", "#fff1e0"),
    ("white", "#fff4ea"),
    ("snow", "#f4f8ff"),
    ("daylight white", "#f4f8ff"),
    ("daylight", "#f4f8ff"),
    ("cool white", "#dce9ff"),
    ("silver", "#d8e2f0"),
    ("chrome", "#d8e2f0"),
    ("grey", "#5a5a5a"),
    ("gray", "#5a5a5a"),
    ("dim grey", "#3a3a3a"),
    ("charcoal", "#303030"),
    ("lime", "#a6ff00"),
    ("chartreuse", "#a6ff00"),
    ("neon green", "#39ff14"),
    ("toxic green", "#39ff14"),
    ("matrix green", "#00ff41"),
    ("green", "#00c040"),
    ("emerald", "#00c060"),
    ("jade", "#00a86b"),
    ("kelly green", "#20b040"),
    ("forest green", "#1b6e20"),
    ("pine", "#1b5e3a"),
    ("olive", "#6b8e23"),
    ("moss", "#6b8e23"),
    ("sage", "#8fbc8f"),
    ("mint", "#7dffc4"),
    ("aurora green", "#00ff9c"),
    ("turquoise", "#00e0c0"),
    ("aqua", "#00ffe0"),
    ("aquamarine", "#40ffc8"),
    ("teal", "#00a6a6"),
    ("cyan", "#00e5ff"),
    ("ice blue", "#9bd7ff"),
    ("baby blue", "#9bd0ff"),
    ("light blue", "#80c8ff"),
    ("sky blue", "#48bfe3"),
    ("azure", "#2090ff"),
    ("electric blue", "#0077ff"),
    ("blue", "#0050ff"),
    ("royal blue", "#2040e0"),
    ("cobalt", "#0047ab"),
    ("ocean blue", "#0057b8"),
    ("sapphire", "#0f52ba"),
    ("navy", "#0b1d51"),
    ("navy blue", "#0b1d51"),
    ("deep navy", "#0b1d51"),
    ("midnight blue", "#101850"),
    ("storm blue", "#2a4b7c"),
    ("slate", "#4a6080"),
    ("indigo", "#3a0ca3"),
    ("ultraviolet", "#6a00f4"),
    ("violet", "#8338ec"),
    ("purple", "#9d00ff"),
    ("royal purple", "#7020c0"),
    ("lavender", "#b388eb"),
    ("lilac", "#c8a2f0"),
    ("orchid", "#da70d6"),
    ("plum", "#5a189a"),
    ("midnight purple", "#2b0a5c"),
    ("mauve", "#c080c0"),
];

/// Words a light can't show. They mean "this colour is darkness" and are dropped from the palette.
const DARK: &[&str] = &["black", "off", "dark", "darkness", "shadow", "jet", "obsidian", "onyx", "ink"];

pub fn names() -> Vec<&'static str> {
    NAMED.iter().map(|(n, _)| *n).collect()
}

pub fn to_hex(name: &str) -> Option<&'static str> {
    let n = name.trim().to_lowercase();
    NAMED.iter().find(|(k, _)| *k == n).map(|(_, h)| *h)
}

fn mix(hex: &str, toward: (f32, f32, f32), t: f32) -> String {
    let c = lumen_hue::Rgb::from_hex(hex).unwrap();
    let m = |a: f32, b: f32| ((a + (b - a) * t).clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", m(c.r, toward.0), m(c.g, toward.1), m(c.b, toward.2))
}

/// A colour phrase → a hex for the lights. `None` for black/darkness or unknown words.
///
/// Tries the exact phrase first, then the longest known phrase inside it, adjusted by light/dark
/// modifier words. Hex input passes through.
pub fn resolve(word: &str) -> Option<String> {
    let s = word.trim().to_lowercase();
    if s.starts_with('#') {
        return lumen_hue::Rgb::from_hex(&s).map(|c| c.to_hex());
    }
    let s: String = s.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    if let Some(h) = to_hex(&words.join(" ")) {
        return Some(h.to_string());
    }
    // the longest known phrase, preferring the one nearest the end ("sky blue" in "bright sky blue")
    let mut best: Option<(usize, usize, &str)> = None; // (len, end, hex)
    for len in (1..=words.len().min(3)).rev() {
        for start in 0..=words.len() - len {
            let phrase = words[start..start + len].join(" ");
            if let Some(h) = to_hex(&phrase) {
                let end = start + len;
                if best.map_or(true, |(bl, be, _)| len > bl || (len == bl && end > be)) {
                    best = Some((len, end, h));
                }
            }
        }
        if best.is_some() {
            break;
        }
    }
    let Some((_, _, hex)) = best else {
        return None; // "black", "darkness" and unknown words all land here
    };
    let has = |ws: &[&str]| words.iter().any(|w| ws.contains(w));
    if has(DARK) && !has(&["dark", "deep"]) {
        return None;
    }
    let out = if has(&["pastel", "pale", "light", "soft", "baby", "faded"]) {
        mix(hex, (1.0, 0.97, 0.95), 0.45)
    } else if has(&["dark", "deep", "dim", "muted", "dusky"]) {
        mix(hex, (0.0, 0.0, 0.0), 0.35)
    } else {
        hex.to_string()
    };
    Some(out)
}

/// Every colour named in free text ("crimson and white flag; soft moonlight"), in order, deduplicated.
/// Used when the model's palette has too few real colours but its notes name more.
pub fn extract(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for chunk in text.split([',', ';', '.', '/']).flat_map(|c| c.split(" and ")) {
        if let Some(h) = resolve(chunk) {
            if !out.contains(&h) {
                out.push(h);
            }
        }
    }
    out
}

/// Closest vocabulary name to a hex colour (for turning curated hex palettes into hints).
pub fn nearest_name(hex: &str) -> &'static str {
    let Some(c) = lumen_hue::Rgb::from_hex(hex) else { return "warm white" };
    NAMED
        .iter()
        .min_by(|a, b| {
            let d = |h: &str| {
                let o = lumen_hue::Rgb::from_hex(h).unwrap();
                (o.r - c.r).powi(2) + (o.g - c.g).powi(2) + (o.b - c.b).powi(2)
            };
            d(a.1).partial_cmp(&d(b.1)).unwrap()
        })
        .map(|(n, _)| *n)
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn plain_words() {
        assert_eq!(resolve("red").as_deref(), Some("#ff0000"));
        assert_eq!(resolve("Canary Yellow").as_deref(), Some("#ffe400"));
        assert_eq!(resolve("navy").as_deref(), Some("#0b1d51"));
    }

    #[test]
    fn phrases_with_extra_words() {
        assert_eq!(resolve("Brazil flag green").as_deref(), Some("#00c040"));
        assert_eq!(resolve("bright sky blue").as_deref(), Some("#48bfe3"));
        assert_eq!(resolve("Ferrari red"), resolve("racing red"));
    }

    #[test]
    fn modifiers_shift_the_colour() {
        assert_ne!(resolve("pale green"), resolve("green"));
        assert_ne!(resolve("dark green"), resolve("green"));
    }

    #[test]
    fn darkness_is_dropped() {
        assert_eq!(resolve("black"), None);
        assert_eq!(resolve("jet black"), None);
        assert_eq!(resolve("glorp"), None);
    }

    #[test]
    fn hex_passes_through() {
        assert_eq!(resolve("#FF8800").as_deref(), Some("#ff8800"));
    }
}
