//! Matpris's backend: searches Swedish grocery chains, ranks the hits and finds the
//! stores around you. Ported from the TypeScript server (src/lib/server, removed with
//! the switch to Tauri); behaviour and comments follow it.

pub mod cache;
pub mod chains;
pub mod geo;
pub mod geocode;
pub mod image;
pub mod http;
pub mod normalize;
pub mod search;
pub mod stores;
pub mod types;

pub use search::search;
pub use types::*;
