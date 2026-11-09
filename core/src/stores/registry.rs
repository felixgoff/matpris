//! Every chain's stores, cached a day (port of src/lib/server/stores/registry.ts).

use std::sync::LazyLock;
use std::time::Duration;

use futures::future::join_all;

use crate::cache::TtlCache;
use crate::chains::adapters;
use crate::http;
use crate::stores::radius::{pick_area, DEFAULT_AREA};
use crate::types::{Area, ChainId, LatLng, Store};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
static CACHE: LazyLock<TtlCache<Vec<Store>>> = LazyLock::new(|| TtlCache::new(DAY));

/// Every known store of every chain with prices. A chain whose store list fails is skipped.
pub async fn all_stores() -> http::Result<Vec<Store>> {
    CACHE
        .get("all", || async {
            let lists = join_all(adapters().iter().map(|a| async move {
                match a.list_stores().await {
                    Ok(stores) => stores,
                    Err(e) => {
                        eprintln!("[stores] {}: {e}", a.chain().as_str());
                        Vec::new()
                    }
                }
            }))
            .await;
            let stores: Vec<Store> = lists.into_iter().flatten().collect();
            if stores.is_empty() {
                return Err(http::Error::Other("no store lists available".into()));
            }
            Ok(stores)
        })
        .await
}

/// The search circle, sized by the stores that have prices, or as the shopper picked it.
pub async fn priced_area(center: LatLng, picked_km: Option<f64>) -> http::Result<Area> {
    Ok(pick_area(center, &all_stores().await?, &DEFAULT_AREA, picked_km))
}

/// Chains whose stores are shown without prices (`info().online == false`). None today.
pub fn map_only_chains() -> Vec<ChainId> {
    ChainId::ALL.into_iter().filter(|c| !c.info().online).collect()
}

/// Adds the nearest stores of chains without online prices inside the same circle.
/// Every chain has online prices now and the OpenStreetMap lookups that used to find
/// map-only stores (stores/osm.ts) were retired, so the area comes back unchanged.
pub async fn with_map_only_stores(_center: LatLng, area: Area) -> Area {
    // A chain going map-only again would add its DEFAULT_AREA.max_per_chain nearest
    // stores inside area.radius_km here, from a store list of its own.
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chain_has_online_prices_today() {
        assert!(map_only_chains().is_empty());
    }

    #[tokio::test]
    async fn map_only_stores_leave_the_area_unchanged() {
        let area = Area { radius_km: 2.5, stores: vec![] };
        let c = LatLng { lat: 55.7047, lng: 13.1910 };
        assert_eq!(with_map_only_stores(c, area.clone()).await, area);
    }
}
