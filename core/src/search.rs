//! TODO: src/lib/server/search.ts — one search across every chain, as a stream of events.

use crate::types::{LatLng, SearchEvent};

/// Runs a search and sends its events to `tx` as they happen: the area first, then each
/// chain's results as soon as they arrive, then `Done`. Stops early when `tx` closes.
pub async fn search(_query: &str, _center: LatLng, _picked_km: Option<f64>, tx: tokio::sync::mpsc::Sender<SearchEvent>) {
    let _ = tx.send(SearchEvent::Done).await;
}
