//! The real colours of well-known subjects (flags, holidays, brands), given to the model as facts.
//!
//! Colours are plain words that `colors::resolve` understands, most recognisable first.
//! Black is left out (a light can't show it); white is kept last when it's minor.

pub struct Fact {
    /// lowercase words that name the subject; matched as whole words
    pub words: &'static [&'static str],
    /// what the colours come from, for the model ("the flag", "the logo")
    pub what: &'static str,
    pub colors: &'static [&'static str],
}

const fn f(words: &'static [&'static str], what: &'static str, colors: &'static [&'static str]) -> Fact {
    Fact { words, what, colors }
}

pub const FACTS: &[Fact] = &[
    f(&["brazil", "brazilian", "brasil", "rio de janeiro"], "the flag and the football kit", &["green", "canary yellow", "royal blue", "white"]),
    f(&["argentina", "argentinian", "argentine", "buenos aires"], "the flag", &["sky blue", "white", "gold"]),
    f(&["mexico", "mexican"], "the flag", &["green", "white", "red"]),
    f(&["usa", "america", "american", "united states", "new york"], "the flag", &["red", "white", "navy"]),
    f(&["canada", "canadian"], "the flag", &["red", "white"]),
    f(&["uk", "britain", "british", "united kingdom", "england", "english", "london"], "the Union Jack", &["red", "white", "royal blue"]),
    f(&["scotland", "scottish"], "the flag", &["royal blue", "white"]),
    f(&["wales", "welsh"], "the flag", &["red", "green", "white"]),
    f(&["ireland", "irish", "dublin"], "the flag and shamrocks", &["green", "orange", "white"]),
    f(&["france", "french", "paris"], "the flag", &["royal blue", "white", "red"]),
    f(&["italy", "italian", "rome"], "the flag", &["green", "white", "red"]),
    f(&["spain", "spanish", "madrid"], "the flag", &["red", "gold"]),
    f(&["portugal", "portuguese", "lisbon"], "the flag", &["green", "red", "gold"]),
    f(&["germany", "german", "berlin"], "the flag", &["red", "gold"]),
    f(&["netherlands", "dutch", "holland", "amsterdam"], "the flag and the national orange", &["orange", "red", "white", "royal blue"]),
    f(&["belgium", "belgian"], "the flag", &["gold", "red"]),
    f(&["switzerland", "swiss"], "the flag", &["red", "white"]),
    f(&["austria", "austrian", "vienna"], "the flag", &["red", "white"]),
    f(&["sweden", "swedish", "stockholm"], "the flag", &["royal blue", "canary yellow"]),
    f(&["norway", "norwegian", "oslo"], "the flag", &["red", "navy", "white"]),
    f(&["denmark", "danish", "copenhagen"], "the flag", &["red", "white"]),
    f(&["finland", "finnish", "helsinki"], "the flag", &["royal blue", "white"]),
    f(&["iceland", "icelandic"], "the flag", &["royal blue", "red", "white"]),
    f(&["poland", "polish", "warsaw"], "the flag", &["red", "white"]),
    f(&["ukraine", "ukrainian", "kyiv"], "the flag", &["azure", "yellow"]),
    f(&["russia", "russian", "moscow"], "the flag", &["white", "blue", "red"]),
    f(&["greece", "greek", "athens"], "the flag", &["azure", "white"]),
    f(&["turkey", "turkish", "istanbul"], "the flag", &["red", "white"]),
    f(&["croatia", "croatian"], "the flag and the checkerboard kit", &["red", "white", "royal blue"]),
    f(&["serbia", "serbian"], "the flag", &["red", "royal blue", "white"]),
    f(&["czech", "czechia", "prague"], "the flag", &["royal blue", "red", "white"]),
    f(&["hungary", "hungarian", "budapest"], "the flag", &["red", "white", "green"]),
    f(&["romania", "romanian"], "the flag", &["royal blue", "yellow", "red"]),
    f(&["japan", "japanese", "tokyo"], "the flag", &["crimson", "white"]),
    f(&["china", "chinese", "beijing"], "the flag", &["red", "gold"]),
    f(&["korea", "korean", "seoul"], "the flag", &["red", "royal blue", "white"]),
    f(&["india", "indian", "mumbai", "delhi"], "the flag", &["saffron", "green", "navy", "white"]),
    f(&["pakistan", "pakistani"], "the flag", &["forest green", "white"]),
    f(&["thailand", "thai", "bangkok"], "the flag", &["red", "navy", "white"]),
    f(&["vietnam", "vietnamese"], "the flag", &["red", "yellow"]),
    f(&["indonesia", "indonesian", "bali"], "the flag", &["red", "white"]),
    f(&["philippines", "filipino"], "the flag", &["royal blue", "red", "gold", "white"]),
    f(&["australia", "australian", "sydney"], "the flag and the green and gold sporting colours", &["navy", "red", "green", "gold"]),
    f(&["new zealand", "kiwi"], "the flag", &["navy", "red", "white"]),
    f(&["south africa", "south african"], "the flag", &["green", "gold", "red", "royal blue"]),
    f(&["nigeria", "nigerian"], "the flag", &["green", "white"]),
    f(&["ghana", "ghanaian"], "the flag", &["red", "gold", "green"]),
    f(&["kenya", "kenyan"], "the flag", &["red", "green", "white"]),
    f(&["egypt", "egyptian", "cairo"], "the flag and desert sand", &["red", "white", "gold", "sand"]),
    f(&["morocco", "moroccan", "marrakech"], "the flag", &["red", "green"]),
    f(&["israel", "israeli"], "the flag", &["royal blue", "white"]),
    f(&["saudi", "saudi arabia"], "the flag", &["green", "white"]),
    f(&["uae", "emirates", "dubai"], "the flag", &["green", "white", "red"]),
    f(&["iran", "iranian", "persia", "persian"], "the flag", &["green", "white", "red"]),
    f(&["colombia", "colombian"], "the flag", &["yellow", "royal blue", "red"]),
    f(&["peru", "peruvian"], "the flag", &["red", "white"]),
    f(&["chile", "chilean"], "the flag", &["red", "royal blue", "white"]),
    f(&["uruguay", "uruguayan"], "the flag and the sky-blue kit", &["sky blue", "white", "gold"]),
    f(&["venezuela", "venezuelan"], "the flag", &["yellow", "royal blue", "red"]),
    f(&["cuba", "cuban", "havana"], "the flag", &["royal blue", "red", "white"]),
    f(&["jamaica", "jamaican"], "the flag", &["green", "gold"]),
    f(&["christmas", "xmas"], "the decorations", &["red", "pine", "gold", "warm white"]),
    f(&["halloween"], "pumpkins and witches", &["orange", "purple", "neon green"]),
    f(&["easter"], "painted eggs", &["pastel yellow", "lilac", "mint", "baby blue"]),
    f(&["valentine", "valentines"], "hearts and roses", &["red", "hot pink", "soft pink"]),
    f(&["st patrick", "st patricks", "saint patrick"], "shamrocks", &["green", "emerald", "gold"]),
    f(&["diwali"], "oil lamps and rangoli", &["gold", "orange", "magenta", "candlelight"]),
    f(&["pride"], "the rainbow flag", &["red", "orange", "yellow", "green", "blue", "violet"]),
    f(&["carnival", "carnaval", "mardi gras"], "costumes and feathers", &["purple", "gold", "green", "hot pink"]),
    f(&["barbie"], "the brand", &["barbie pink", "hot pink", "soft pink"]),
    f(&["matrix"], "the digital rain", &["matrix green", "green", "forest green"]),
    f(&["star wars"], "lightsabers and Tatooine", &["red", "electric blue", "orange", "cool white"]),
    f(&["simpsons"], "Springfield's yellow skin and blue sky", &["yellow", "sky blue", "orange", "pink"]),
    f(&["vice city", "miami vice"], "the neon of 1980s Miami", &["hot pink", "turquoise", "sunset orange", "violet"]),
    f(&["tron"], "the grid", &["cyan", "electric blue", "orange"]),
    f(&["blade runner"], "the city", &["orange", "teal", "magenta"]),
    f(&["minecraft"], "grass, dirt and diamonds", &["green", "brown", "cyan"]),
    f(&["mario"], "Mario's outfit", &["red", "royal blue", "yellow"]),
    f(&["zelda", "hyrule"], "Link and the Triforce", &["green", "gold", "sky blue"]),
    f(&["pokemon", "pikachu"], "Pikachu and the Poké Ball", &["yellow", "red", "white"]),
    f(&["ferrari"], "Rosso Corsa and the badge", &["ferrari red", "canary yellow"]),
    f(&["mcdonalds", "mcdonald"], "the logo", &["golden", "red"]),
    f(&["spotify"], "the logo", &["green", "white"]),
    f(&["coca cola", "coke"], "the logo", &["red", "white"]),
    f(&["ikea"], "the logo", &["royal blue", "yellow"]),
    f(&["twitch"], "the logo", &["purple", "lavender"]),
    f(&["discord"], "the logo", &["indigo", "violet", "white"]),
    f(&["mars", "martian"], "the dust and sky", &["rust", "terracotta", "sunset orange", "tan"]),
    f(&["lapland"], "snow, northern lights and Christmas", &["ice blue", "aurora green", "red", "warm white"]),
];

/// Facts whose words appear (as whole words or phrases) in `prompt`.
pub fn find(prompt: &str) -> Vec<&'static Fact> {
    let p: String = prompt.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    let p = format!(" {} ", p.split_whitespace().collect::<Vec<_>>().join(" "));
    FACTS.iter().filter(|f| f.words.iter().any(|w| p.contains(&format!(" {w} ")))).collect()
}

/// One line per fact for the model: `- brazil: the flag and the football kit = green, canary yellow, ...`
pub fn brief(prompt: &str) -> String {
    find(prompt)
        .iter()
        .map(|f| format!("- {}: {} = {}\n", f.words[0], f.what, f.colors.join(", ")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_colour_resolves() {
        for f in FACTS {
            for c in f.colors {
                assert!(crate::colors::resolve(c).is_some(), "{c} in {:?}", f.words);
            }
        }
    }

    #[test]
    fn whole_words_only() {
        assert_eq!(find("Brazil")[0].words[0], "brazil");
        assert_eq!(find("Brazilian beach").len(), 1);
        assert!(find("marsh").is_empty()); // not "mars"
        assert_eq!(find("McDonald's")[0].words[0], "mcdonalds"); // apostrophe splits: "mcdonald s"
    }
}
