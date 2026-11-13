//! Live modes: named starting mixes of one colour source plus sound and game events.

use crate::config::Mix;
use crate::sync::Source;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LiveMode {
    Scene,
    Video,
    Music,
    Game,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Extra {
    None,
    PlusSound,
    NoSound,
}

impl LiveMode {
    pub const ALL: [LiveMode; 4] = [LiveMode::Scene, LiveMode::Video, LiveMode::Music, LiveMode::Game];

    pub fn mix(self) -> Mix {
        let (source, sound, game) = match self {
            LiveMode::Scene => (Source::Look, false, false),
            LiveMode::Video => (Source::Screen, false, false),
            LiveMode::Music => (Source::Look, true, false),
            LiveMode::Game => (Source::Screen, true, true),
        };
        Mix { source, sound, game }
    }

    pub fn name(self) -> &'static str {
        match self {
            LiveMode::Scene => "Scene",
            LiveMode::Video => "Video",
            LiveMode::Music => "Music",
            LiveMode::Game => "Game",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            LiveMode::Scene => "A mood, still or slowly drifting. Nothing reacts.",
            LiveMode::Video => "Films, shows, music videos. Add sound for concerts and clips.",
            LiveMode::Music => "Your mood colours, moving to whatever is playing.",
            LiveMode::Game => "Screen colours and game events. Sound hits on top, or off for focus.",
        }
    }

    pub fn recipe(self) -> &'static str {
        match self {
            LiveMode::Scene => "MOOD",
            LiveMode::Video => "SCREEN · SOUND OPTIONAL",
            LiveMode::Music => "MOOD + SOUND",
            LiveMode::Game => "SCREEN + GAME · SOUND OPTIONAL",
        }
    }

    /// The mode a mix belongs to. A screen mix that only differs in sound keeps its mode, so
    /// music videos read as Video + sound and a quiet match as Game · no sound.
    pub fn of(m: Mix) -> Option<(LiveMode, Extra)> {
        if let Some(exact) = Self::ALL.into_iter().find(|l| l.mix() == m) {
            return Some((exact, Extra::None));
        }
        if m.source != Source::Screen {
            return None;
        }
        let near = Self::ALL.into_iter().find(|l| {
            let x = l.mix();
            x.source == Source::Screen && x.game == m.game
        })?;
        Some((near, if m.sound { Extra::PlusSound } else { Extra::NoSound }))
    }

    /// "VIDEO + SOUND", "GAME · NO SOUND", "YOUR MIX".
    pub fn label(m: Mix) -> String {
        match Self::of(m) {
            Some((l, Extra::None)) => l.name().to_uppercase(),
            Some((l, Extra::PlusSound)) => format!("{} + SOUND", l.name().to_uppercase()),
            Some((l, Extra::NoSound)) => format!("{} · NO SOUND", l.name().to_uppercase()),
            None => "YOUR MIX".into(),
        }
    }
}

/// What a running game uses: its saved mix, or Game.
pub fn for_game(saved: Option<Mix>) -> Mix {
    saved.unwrap_or(LiveMode::Game.mix())
}

/// "screen colours + game events + sound"
pub fn describe(m: Mix) -> String {
    let mut parts = vec![if m.source == Source::Screen { "screen colours" } else { "mood colours" }];
    if m.game {
        parts.push("game events");
    }
    if m.sound {
        parts.push(if m.source == Source::Screen { "sound punch" } else { "sound" });
    }
    parts.join(" + ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mix(source: Source, sound: bool, game: bool) -> Mix {
        Mix { source, sound, game }
    }

    #[test]
    fn every_mode_reads_back_as_itself() {
        for l in LiveMode::ALL {
            assert_eq!(LiveMode::of(l.mix()), Some((l, Extra::None)));
        }
    }

    #[test]
    fn sound_on_top_keeps_the_mode() {
        assert_eq!(LiveMode::label(mix(Source::Screen, true, false)), "VIDEO + SOUND");
        assert_eq!(LiveMode::label(mix(Source::Screen, false, true)), "GAME · NO SOUND");
        assert_eq!(LiveMode::label(mix(Source::Look, true, true)), "YOUR MIX");
        assert_eq!(for_game(None), LiveMode::Game.mix());
        assert_eq!(describe(LiveMode::Game.mix()), "screen colours + game events + sound punch");
    }
}
