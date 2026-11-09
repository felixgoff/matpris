//! TODO(port): src/lib/server/stores/registry.ts — every chain's stores, cached a day.

use crate::http;
use crate::types::{Area, LatLng, Store};

/// Every known store of every chain with prices. A chain whose list fails is skipped.
pub async fn all_stores() -> http::Result<Vec<Store>> {
    Ok(vec![])
}

/// The search circle, sized by the stores that have prices, or as the shopper picked it.
pub async fn priced_area(_center: LatLng, _picked_km: Option<f64>) -> http::Result<Area> {
    Ok(Area { radius_km: 0.0, stores: vec![] })
}
