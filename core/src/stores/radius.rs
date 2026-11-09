//! TODO(port): src/lib/server/stores/radius.ts — the automatic search circle.

use crate::types::{Area, LatLng, Store};

#[derive(Debug, Clone, Copy)]
pub struct AreaOptions {
    pub start_km: f64,
    pub growth: f64,
    pub max_km: f64,
    /// Grow until the circle holds at least this many stores…
    pub min_stores: usize,
    /// …from at least this many different chains (or every chain that exists).
    pub min_chains: usize,
    /// Never query more than this many stores of one chain, or in total.
    pub max_per_chain: usize,
    pub max_stores: usize,
}

pub const DEFAULT_AREA: AreaOptions = AreaOptions {
    start_km: 1.5,
    growth: 1.6,
    max_km: 30.0,
    min_stores: 12,
    min_chains: 3,
    max_per_chain: 5,
    max_stores: 25,
};

/// A circle the shopper sized themselves holds every store inside it, up to this many.
pub const PICKED_MAX_STORES: usize = 150;

/// Pick a search circle around `center` that is just big enough. `picked_km` is a size
/// the shopper chose.
pub fn pick_area(_center: LatLng, _all: &[Store], _opts: &AreaOptions, _picked_km: Option<f64>) -> Area {
    Area { radius_km: 0.0, stores: vec![] }
}
