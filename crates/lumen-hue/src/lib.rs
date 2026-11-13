//! Hue bridge client: CLIP v2 REST, pairing, discovery and Entertainment streaming.

pub mod api;
pub mod color;
pub mod discover;
pub mod model;
pub mod pair;
pub mod stream;

pub use api::{Bridge, Inventory, LightState};
pub use color::Rgb;
pub use model::*;
pub use stream::Streamer;
