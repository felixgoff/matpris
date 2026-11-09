//! The automatic search circle (port of src/lib/server/stores/radius.ts).

use std::collections::{HashMap, HashSet};

use crate::geo::distance_km;
use crate::types::{Area, ChainId, LatLng, NearbyStore, Store};

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
/// Every chain prices nationally (one request per chain), so more stores cost nothing.
/// Shortcut: assumes no per-store-priced chain; one would need its own cap here.
pub const PICKED_MAX_STORES: usize = 150;

/// `s` with its distance from `center`, rounded to 10 m (`Math.round(d * 100) / 100`).
pub(crate) fn nearby(center: LatLng, s: &Store) -> NearbyStore {
    let d = distance_km(center, LatLng { lat: s.lat, lng: s.lng });
    // JS Math.round rounds halves up, f64::round away from zero: the same for the
    // non-negative distances here.
    NearbyStore { store: s.clone(), distance_km: (d * 100.0).round() / 100.0 }
}

/// Pick a search circle around `center` that is just big enough: dense cities get a
/// small circle, the countryside a large one. `picked_km` is a size the shopper chose.
pub fn pick_area(center: LatLng, all: &[Store], opts: &AreaOptions, picked_km: Option<f64>) -> Area {
    // The TS tests `if (pickedKm)`: 0 and NaN count as "not picked".
    let picked = picked_km.filter(|k| *k != 0.0 && !k.is_nan());

    let mut sorted: Vec<NearbyStore> = all.iter().map(|s| nearby(center, s)).collect();
    // A stable sort, like Array.prototype.sort.
    sorted.sort_by(|a, b| a.distance_km.total_cmp(&b.distance_km));
    let chain_count = all.iter().map(|s| s.chain).collect::<HashSet<_>>().len();
    let min_chains = opts.min_chains.min(chain_count);

    let radius_km = match picked {
        Some(k) => k,
        None => {
            let mut r = opts.start_km;
            while r < opts.max_km {
                let inside: Vec<&NearbyStore> = sorted.iter().filter(|s| s.distance_km <= r).collect();
                if inside.len() >= opts.min_stores
                    && inside.iter().map(|s| s.store.chain).collect::<HashSet<_>>().len() >= min_chains
                {
                    break;
                }
                r *= opts.growth;
            }
            r = r.min(opts.max_km);
            // Nothing within the cap (very remote): stretch to the single nearest store.
            if let Some(first) = sorted.first() {
                if first.distance_km > r {
                    r = first.distance_km;
                }
            }
            (r * 10.0).ceil() / 10.0
        }
    };
    let max_per_chain = if picked.is_some() { usize::MAX } else { opts.max_per_chain };
    let max_stores = if picked.is_some() { PICKED_MAX_STORES } else { opts.max_stores };

    let mut per_chain: HashMap<ChainId, usize> = HashMap::new();
    let mut stores = Vec::new();
    for s in sorted {
        if s.distance_km > radius_km || stores.len() >= max_stores {
            break;
        }
        let n = per_chain.entry(s.store.chain).or_insert(0);
        if *n >= max_per_chain {
            continue;
        }
        *n += 1;
        stores.push(s);
    }
    Area { radius_km, stores }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAINS: [ChainId; 3] = [ChainId::Willys, ChainId::Hemkop, ChainId::Lidl];
    const CENTER: LatLng = LatLng { lat: 59.0, lng: 18.0 };

    // ~0.009° latitude ≈ 1 km
    fn ring(n: usize, km: f64, start: usize) -> Vec<Store> {
        (0..n)
            .map(|i| {
                let fi = i as f64;
                Store {
                    id: format!("{}", start + i),
                    chain: CHAINS[(start + i) % 3],
                    name: format!("S{}", start + i),
                    lat: 59.0 + (km / 111.0) * fi.cos(),
                    lng: 18.0 + ((km / 111.0) * fi.sin()) / (59.0 * std::f64::consts::PI / 180.0).cos(),
                    address: String::new(),
                    url: None,
                }
            })
            .collect()
    }

    fn cat(a: Vec<Store>, b: Vec<Store>) -> Vec<Store> {
        a.into_iter().chain(b).collect()
    }

    #[test]
    fn keeps_the_circle_small_in_a_dense_city() {
        let a = pick_area(CENTER, &cat(ring(15, 1.0, 0), ring(30, 10.0, 15)), &DEFAULT_AREA, None);
        assert_eq!(a.radius_km, 1.5);
        assert_eq!(a.stores.len(), 15);
    }

    #[test]
    fn grows_the_circle_in_the_countryside() {
        let a = pick_area(CENTER, &ring(12, 14.0, 0), &DEFAULT_AREA, None);
        assert!(a.radius_km > 14.0);
        assert!(a.radius_km <= 30.0);
        assert_eq!(a.stores.len(), 12);
    }

    #[test]
    fn stops_at_the_cap_or_stretches_to_the_nearest_store_when_nothing_is_inside() {
        assert!(pick_area(CENTER, &ring(3, 20.0, 0), &DEFAULT_AREA, None).radius_km <= 30.0);
        let remote = pick_area(CENTER, &ring(2, 80.0, 0), &DEFAULT_AREA, None);
        // toBeCloseTo(80, 0): |x − 80| < 0.5
        assert!((remote.radius_km - 80.0).abs() < 0.5, "{}", remote.radius_km);
        assert!(!remote.stores.is_empty());
    }

    #[test]
    fn caps_stores_per_chain() {
        let a = pick_area(CENTER, &ring(60, 1.0, 0), &DEFAULT_AREA, None);
        assert_eq!(a.stores.iter().filter(|s| s.store.chain == ChainId::Willys).count(), 5);
        assert_eq!(a.stores.len(), 15);
    }

    #[test]
    fn keeps_a_size_the_shopper_picked_with_every_store_inside_it() {
        let all = cat(ring(60, 1.0, 0), ring(30, 10.0, 60));
        let near = pick_area(CENTER, &all, &DEFAULT_AREA, Some(2.0));
        assert_eq!(near.radius_km, 2.0);
        assert_eq!(near.stores.len(), 60); // no 5-per-chain cap
        let wide = pick_area(CENTER, &all, &DEFAULT_AREA, Some(12.0));
        assert_eq!(wide.stores.len(), 90);
        // Nothing inside: the picked size stays, it doesn't stretch to the nearest store.
        let empty = pick_area(CENTER, &ring(2, 80.0, 0), &DEFAULT_AREA, Some(1.0));
        assert_eq!((empty.radius_km, empty.stores.len()), (1.0, 0));
    }

    #[test]
    fn a_picked_circle_holds_at_most_150_stores() {
        let a = pick_area(CENTER, &ring(200, 1.0, 0), &DEFAULT_AREA, Some(5.0));
        assert_eq!(a.stores.len(), PICKED_MAX_STORES);
    }

    #[test]
    fn a_picked_size_of_zero_means_auto_like_the_ts_truthiness_check() {
        let a = pick_area(CENTER, &cat(ring(15, 1.0, 0), ring(30, 10.0, 15)), &DEFAULT_AREA, Some(0.0));
        assert_eq!(a.radius_km, 1.5);
        assert_eq!(a.stores.len(), 15);
    }

    #[test]
    fn distances_round_to_hundredths_and_the_radius_up_to_a_tenth() {
        // One store ~80 km away: the radius stretches to it and rounds up.
        let a = pick_area(CENTER, &ring(1, 80.03, 0), &DEFAULT_AREA, None);
        let d = a.stores[0].distance_km;
        assert_eq!((d * 100.0).round() / 100.0, d);
        assert_eq!(a.radius_km, (d * 10.0).ceil() / 10.0);
        assert!(a.radius_km >= d);
    }

    #[test]
    fn stores_come_nearest_first() {
        let a = pick_area(CENTER, &cat(ring(3, 10.0, 0), ring(3, 1.0, 3)), &DEFAULT_AREA, Some(20.0));
        assert!(a.stores.windows(2).all(|w| w[0].distance_km <= w[1].distance_km));
        assert_eq!(a.stores[0].store.id, "3");
    }
    #[test]
    fn the_float_growth_step_rounds_up_like_math_ceil() {
        // 1.5 × 1.6 = 2.4000000000000004 in f64 (and in JS), so Math.ceil(r * 10) / 10 = 2.5.
        let a = pick_area(CENTER, &cat(ring(3, 1.0, 0), ring(12, 2.2, 3)), &DEFAULT_AREA, None);
        assert_eq!(a.radius_km, 2.5);
    }

    #[test]
    fn min_chains_shrinks_to_the_chains_that_exist() {
        let willys: Vec<Store> = ring(36, 1.0, 0).into_iter().filter(|s| s.chain == ChainId::Willys).collect();
        assert_eq!(willys.len(), 12);
        let a = pick_area(CENTER, &willys, &DEFAULT_AREA, None);
        assert_eq!(a.radius_km, 1.5);
        assert_eq!(a.stores.len(), 5); // still five per chain
    }

    #[test]
    fn a_nan_picked_size_means_auto() {
        let a = pick_area(CENTER, &cat(ring(15, 1.0, 0), ring(30, 10.0, 15)), &DEFAULT_AREA, Some(f64::NAN));
        assert_eq!(a.radius_km, 1.5);
    }
}
