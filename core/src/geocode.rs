//! Address → coordinates via OpenStreetMap Nominatim (max 1 request/s per their usage
//! policy), cached a week. Was src/routes/api/geocode.

use std::sync::LazyLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cache::TtlCache;
use crate::http::{self, fetch_json, FetchOptions};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub lat: f64,
    pub lng: f64,
    pub label: String,
}

#[derive(Deserialize)]
struct Nominatim {
    lat: String,
    lon: String,
    display_name: String,
}

static CACHE: LazyLock<TtlCache<Option<Place>>> = LazyLock::new(|| TtlCache::new(Duration::from_secs(7 * 24 * 3600)));

/// The best match for an address in Sweden, or None when Nominatim knows none.
pub async fn geocode(q: &str) -> http::Result<Option<Place>> {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return Ok(None);
    }
    CACHE
        .get(&q, || async {
            let url = url::Url::parse_with_params(
                "https://nominatim.openstreetmap.org/search",
                &[("format", "json"), ("limit", "1"), ("countrycodes", "se"), ("q", q.as_str())],
            )
            .map_err(http::Error::other)?;
            let res: Vec<Nominatim> = fetch_json(url.as_str(), &FetchOptions::every(Duration::from_millis(1100))).await?;
            Ok(res.into_iter().next().and_then(|p| {
                Some(Place { lat: p.lat.parse().ok()?, lng: p.lon.parse().ok()?, label: p.display_name })
            }))
        })
        .await
}
