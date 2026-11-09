//! One search across every chain, as a stream of events (was src/lib/server/search.ts).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use futures::FutureExt;
use tokio::sync::mpsc::Sender;

use crate::cache::TtlCache;
use crate::chains::{adapters, ChainAdapter};
use crate::http;
use crate::normalize::relevance;
use crate::stores::registry::{map_only_chains, priced_area, with_map_only_stores};
use crate::types::{ChainId, ChainStatus, LatLng, NearbyStore, Pricing, ProductHit, ResultRow, SearchEvent};

static PRICES: LazyLock<TtlCache<Vec<ProductHit>>> = LazyLock::new(|| TtlCache::new(Duration::from_secs(15 * 60)));

/// "  Mjölk   Laktosfri " → "mjölk laktosfri", at most 60 characters: equal searches share a cache entry.
pub fn normalize_query(q: &str) -> String {
    q.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase().chars().take(60).collect()
}

/// A chain's hits that answer the query, each tied to `store`. A chain can list one
/// product twice (ICA repeats it across product groups); the page keys rows by id.
fn rate(query: &str, hits: Vec<ProductHit>, store: &NearbyStore) -> Vec<ResultRow> {
    let mut seen = HashSet::new();
    hits.into_iter()
        .filter(|h| seen.insert(h.id.clone()))
        .filter_map(|h| {
            let categories = h.categories.clone().unwrap_or_default();
            let m = relevance(query, &h.name, h.brand.as_deref(), &categories)?;
            Some(ResultRow { hit: h, store: store.clone(), match_: m })
        })
        .collect()
}

async fn search_chain(adapter: &dyn ChainAdapter, query: &str, stores: &[NearbyStore]) -> http::Result<Vec<ResultRow>> {
    let chain = adapter.chain().as_str();
    if adapter.pricing() == Pricing::Chain {
        // One national price list: price it once, point at the nearest store.
        let hits = PRICES.get(&format!("{chain}:{query}"), || adapter.search(query, None)).await?;
        return Ok(rate(query, hits, &stores[0]));
    }

    // Per-store prices: query every store in the circle, keep each product's best offer.
    let per_store = futures::future::join_all(stores.iter().map(|s| {
        let key = format!("{chain}:{}:{query}", s.store.id);
        async move { PRICES.get(&key, || adapter.search(query, Some(&s.store))).await }
    }))
    .await;
    let mut best: HashMap<String, ResultRow> = HashMap::new();
    let mut first_error = None;
    let mut any_ok = false;
    for (result, store) in per_store.into_iter().zip(stores) {
        match result {
            Ok(hits) => {
                any_ok = true;
                for row in rate(query, hits, store) {
                    match best.get(&row.hit.id) {
                        Some(prev) if prev.hit.price <= row.hit.price => {}
                        _ => {
                            best.insert(row.hit.id.clone(), row);
                        }
                    }
                }
            }
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    if !any_ok {
        if let Some(e) = first_error {
            return Err(e);
        }
    }
    Ok(best.into_values().collect())
}

fn chain_event(chain: ChainId, result: http::Result<Vec<ResultRow>>) -> SearchEvent {
    match result {
        Ok(results) => SearchEvent::Chain { chain, status: ChainStatus::Ok, results, message: None },
        Err(e) => SearchEvent::Chain {
            chain,
            status: if e.is_throttled() { ChainStatus::Throttled } else { ChainStatus::Error },
            results: vec![],
            message: Some(e.to_string()),
        },
    }
}

/// Sends the search area first, then each chain's results as soon as they arrive, then
/// `Done`. Stores of chains without online prices are looked up alongside and sent as a
/// second, complete `Area` event, so they never delay the prices. Stops as soon as `tx`
/// is closed (the page went away or searched again).
pub async fn search(raw_query: &str, center: LatLng, picked_km: Option<f64>, tx: Sender<SearchEvent>) {
    let query = normalize_query(raw_query);
    let area = match priced_area(center, picked_km).await {
        Ok(a) => a,
        Err(e) => {
            let _ = tx.send(SearchEvent::Error { message: e.to_string() }).await;
            return;
        }
    };
    if tx
        .send(SearchEvent::Area { center, radius_km: area.radius_km, stores: area.stores.clone() })
        .await
        .is_err()
    {
        return;
    }

    let mut by_chain: BTreeMap<ChainId, Vec<NearbyStore>> = BTreeMap::new();
    for s in &area.stores {
        by_chain.entry(s.store.chain).or_default().push(s.clone());
    }

    let mut pending: FuturesUnordered<BoxFuture<'_, Vec<SearchEvent>>> = FuturesUnordered::new();
    for adapter in adapters() {
        let chain = adapter.chain();
        match by_chain.get(&chain) {
            None => pending.push(
                async move { vec![SearchEvent::Chain { chain, status: ChainStatus::NoStores, results: vec![], message: None }] }
                    .boxed(),
            ),
            Some(stores) => {
                let query = query.as_str();
                pending.push(async move { vec![chain_event(chain, search_chain(adapter.as_ref(), query, stores).await)] }.boxed())
            }
        }
    }
    let area_for_map = area.clone();
    pending.push(
        async move {
            let full = with_map_only_stores(center, area_for_map).await;
            let mut events = vec![SearchEvent::Area { center, radius_km: full.radius_km, stores: full.stores }];
            events.extend(map_only_chains().into_iter().map(|chain| SearchEvent::Chain {
                chain,
                status: ChainStatus::NoOnlinePrices,
                results: vec![],
                message: None,
            }));
            events
        }
        .boxed(),
    );

    while let Some(events) = pending.next().await {
        for e in events {
            if tx.send(e).await.is_err() {
                return;
            }
        }
    }
    let _ = tx.send(SearchEvent::Done).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_are_normalized() {
        assert_eq!(normalize_query("  Mjölk   Laktosfri "), "mjölk laktosfri");
        assert_eq!(normalize_query(&"a".repeat(80)).len(), 60);
    }
}
