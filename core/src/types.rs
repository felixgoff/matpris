//! The data the backend sends to the page. Field names and shapes match what the page
//! reads (camelCase JSON, absent optional fields left out), so `/api/search` and the
//! Tauri `search` command stream the same `SearchEvent`s.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChainId {
    Willys,
    Hemkop,
    Lidl,
    Citygross,
    Ica,
    Coop,
}

pub struct ChainInfo {
    pub name: &'static str,
    /// `false`: the chain's stores are shown, its prices aren't (none at the moment).
    pub online: bool,
}

impl ChainId {
    pub const ALL: [ChainId; 6] = [
        ChainId::Willys,
        ChainId::Hemkop,
        ChainId::Lidl,
        ChainId::Citygross,
        ChainId::Ica,
        ChainId::Coop,
    ];

    /// Mirrors `CHAINS` in src/lib/types.ts.
    pub fn info(self) -> ChainInfo {
        let (name, online) = match self {
            ChainId::Willys => ("Willys", true),
            ChainId::Hemkop => ("Hemköp", true),
            ChainId::Lidl => ("Lidl", true),
            ChainId::Citygross => ("City Gross", true),
            ChainId::Ica => ("ICA", true),
            ChainId::Coop => ("Coop", true),
        };
        ChainInfo { name, online }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ChainId::Willys => "willys",
            ChainId::Hemkop => "hemkop",
            ChainId::Lidl => "lidl",
            ChainId::Citygross => "citygross",
            ChainId::Ica => "ica",
            ChainId::Coop => "coop",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    pub id: String,
    pub chain: ChainId,
    pub name: String,
    pub lat: f64,
    pub lng: f64,
    pub address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NearbyStore {
    #[serde(flatten)]
    pub store: Store,
    pub distance_km: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Kg,
    L,
    St,
}

/// A multi-buy offer such as "5 for 55 kr".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deal {
    /// Items you must buy to get the deal price.
    pub quantity: u32,
    /// Price per item when buying `quantity`.
    pub each: f64,
    pub total: f64,
    /// Comparison price per unit at the deal price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_price: Option<f64>,
    /// The quantity can be any mix of the products in the offer.
    pub mix_and_match: bool,
    /// Bottle deposit ("pant") is charged on top.
    pub plus_deposit: bool,
    /// Maximum number of deal purchases per customer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pricing {
    Store,
    Chain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductHit {
    pub chain: ChainId,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Price you pay for one item right now (sale price if a single-item sale applies).
    pub price: f64,
    /// Comparison price ("jämförpris") in kr per unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<Unit>,
    /// The chain's own category path, broad to specific, where it publishes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
    /// `price` is per kilo: the product is sold by weight (loose bananas, tomatoes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_kg: Option<bool>,
    /// Estimated price of one piece of a product sold by weight, from its average weight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approx_each: Option<f64>,
    /// How many pieces a multipack holds ("4x60g" → 4): `price` buys them all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pieces: Option<u32>,
    /// Regular price when `price` is a sale price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deal: Option<Deal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub url: String,
    /// Where the price applies: one store, or the whole chain.
    pub pricing: Pricing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Match {
    Best,
    Mention,
    Weak,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultRow {
    #[serde(flatten)]
    pub hit: ProductHit,
    /// Nearest store in the search circle where this price applies.
    pub store: NearbyStore,
    /// How well it answers the query; see `normalize::relevance`.
    #[serde(rename = "match")]
    pub match_: Match,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChainStatus {
    Ok,
    Error,
    Throttled,
    NoStores,
    NoOnlinePrices,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Area {
    pub radius_km: f64,
    pub stores: Vec<NearbyStore>,
}

/// One event of a search: one line of `/api/search`'s NDJSON, or one message on the
/// Tauri `search` command's channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SearchEvent {
    #[serde(rename_all = "camelCase")]
    Area {
        center: LatLng,
        radius_km: f64,
        stores: Vec<NearbyStore>,
    },
    Chain {
        chain: ChainId,
        status: ChainStatus,
        results: Vec<ResultRow>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    Done,
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_serialize_like_the_page_expects() {
        let e = SearchEvent::Chain { chain: ChainId::Citygross, status: ChainStatus::NoStores, results: vec![], message: None };
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"type":"chain","chain":"citygross","status":"no-stores","results":[]}"#
        );
        let a = SearchEvent::Area { center: LatLng { lat: 55.7, lng: 13.2 }, radius_km: 1.5, stores: vec![] };
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            r#"{"type":"area","center":{"lat":55.7,"lng":13.2},"radiusKm":1.5,"stores":[]}"#
        );
    }
}
