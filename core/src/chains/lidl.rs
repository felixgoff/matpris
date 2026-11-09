//! Lidl Sweden has no full online grocery shop; its site search covers the current
//! assortment and weekly deals, with one national price list:
//!   GET https://www.lidl.se/q/api/search?q=<query>&assortment=SE&locale=sv_SE&version=v2.0.0&fetchsize=<n>
//! Store locations come from data/lidl-stores.json (see scripts/build-lidl-stores.mjs).
//! The search answers 401 to `accept: application/json`; `http` sends `*/*`.

use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;

use super::ChainAdapter;
use crate::http::{self, FetchOptions};
use crate::normalize::{parse_unit_price, unit_price_from_size};
use crate::types::{ChainId, Pricing, ProductHit, Store, Unit};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LidlPrice {
    pub price: Option<f64>,
    pub old_price: Option<f64>,
    pub base_price: Option<Text>,
    pub packaging: Option<Text>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Text {
    pub text: Option<String>,
}

/// Lidl sends the brand either as `{ name }` or as a bare string.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Brand {
    Name(String),
    Object {
        #[serde(default)]
        name: Option<String>,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Region {
    pub is_default: Option<bool>,
    pub region_price_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RegionPrices {
    pub current_lidl_plus_price: Option<PriceWrap>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PriceWrap {
    pub price: Option<LidlPrice>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LidlData {
    pub full_title: String,
    pub brand: Option<Brand>,
    pub category: Option<String>,
    pub canonical_url: Option<String>,
    pub image: Option<String>,
    pub price: Option<LidlPrice>,
    pub regions_v2: Option<BTreeMap<String, Region>>,
    /// Per price region; an item on a Lidl Plus offer has only `currentLidlPlusPrice`.
    pub regions_prices: Option<BTreeMap<String, RegionPrices>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Gridbox {
    pub data: Option<LidlData>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LidlItem {
    pub code: String,
    pub gridbox: Option<Gridbox>,
}

const FOOD_CATEGORIES: [&str; 2] = ["Food", "F+V"];

// JS truthiness for a number: present, not 0, not NaN.
fn truthy(n: Option<f64>) -> Option<f64> {
    n.filter(|x| *x != 0.0 && !x.is_nan())
}

fn text_of(p: Option<&LidlPrice>, field: fn(&LidlPrice) -> Option<&Text>) -> Option<String> {
    p.and_then(field).and_then(|t| t.text.clone())
}

static PER_KG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^\s*/\s*kg(?-u:\b)").unwrap());

pub fn to_hit(item: &LidlItem) -> Option<ProductHit> {
    let d = item.gridbox.as_ref()?.data.as_ref()?;
    if !FOOD_CATEGORIES.contains(&d.category.as_deref().unwrap_or("")) {
        return None;
    }
    // An item on a Lidl Plus offer has no plain price: the offer's `price` is the member
    // price and its `oldPrice` (struck through on the site) is what everyone else pays.
    // Member prices are skipped, like City Gross's and Coop's.
    let region = d
        .regions_v2
        .as_ref()
        .and_then(|rs| rs.values().find(|r| r.is_default == Some(true)))
        .and_then(|r| r.region_price_id.clone())
        .unwrap_or_else(|| "1".to_string());
    let plain = d.price.as_ref();
    let plus: Option<&LidlPrice> = if truthy(plain.and_then(|p| p.price)).is_some() {
        None
    } else {
        d.regions_prices
            .as_ref()
            .and_then(|rp| rp.get(&region))
            .and_then(|r| r.current_lidl_plus_price.as_ref())
            .and_then(|w| w.price.as_ref())
    };
    let price = truthy(plain.and_then(|p| p.price).or_else(|| plus.and_then(|p| p.old_price)))?;
    let size = text_of(plain, |p| p.packaging.as_ref()).or_else(|| text_of(plus, |p| p.packaging.as_ref()));
    let base = text_of(plus.or(plain), |p| p.base_price.as_ref());
    // A bare "/kg" base price means the price itself is per kilo (loose fruit). A Lidl Plus
    // offer's kr/l or kr/kg is the member's, so the unit price comes from the size instead.
    let per_kg = PER_KG.is_match(base.as_deref().unwrap_or(""));
    let unit = if per_kg {
        Some((price, Unit::Kg))
    } else {
        (if plus.is_some() { None } else { base.as_deref().and_then(parse_unit_price) })
            .or_else(|| unit_price_from_size(price, size.as_deref()))
    };
    let brand = match &d.brand {
        Some(Brand::Name(s)) => Some(s.clone()),
        Some(Brand::Object { name }) => name.clone(),
        None => None,
    };
    // Lidl titles often start with the brand in caps ("MATRIKET Standardmjölk").
    let name = match &brand {
        Some(b) if !b.is_empty() && d.full_title.starts_with(b.as_str()) => d.full_title[b.len()..].trim().to_string(),
        _ => d.full_title.clone(),
    };
    let old_price = plain.and_then(|p| truthy(p.old_price)).filter(|o| *o > price);
    Some(ProductHit {
        chain: ChainId::Lidl,
        id: item.code.clone(),
        name,
        brand,
        size,
        price,
        unit_price: unit.map(|u| u.0),
        unit: unit.map(|u| u.1),
        categories: None,
        per_kg: per_kg.then_some(true),
        approx_each: None,
        old_price,
        deal: None,
        image: d.image.clone(),
        url: format!("https://www.lidl.se{}", d.canonical_url.as_deref().unwrap_or("")),
        pricing: Pricing::Chain,
    })
}

// Like JS encodeURIComponent.
fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

static STORES: LazyLock<Vec<Store>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../data/lidl-stores.json")).expect("data/lidl-stores.json")
});

#[derive(Deserialize)]
struct SearchResponse {
    items: Vec<serde_json::Value>,
}

pub struct Lidl;

#[async_trait]
impl ChainAdapter for Lidl {
    fn chain(&self) -> ChainId {
        ChainId::Lidl
    }
    fn pricing(&self) -> Pricing {
        Pricing::Chain
    }
    async fn list_stores(&self) -> http::Result<Vec<Store>> {
        Ok(STORES.clone())
    }
    async fn search(&self, query: &str, _store: Option<&Store>) -> http::Result<Vec<ProductHit>> {
        let url = format!(
            "https://www.lidl.se/q/api/search?q={}&assortment=SE&locale=sv_SE&version=v2.0.0&fetchsize=40",
            encode_uri_component(query)
        );
        let data: SearchResponse = http::fetch_json(&url, &FetchOptions::every(Duration::from_millis(1000))).await?;
        // An item whose shape we don't understand is skipped, not the whole answer.
        Ok(data
            .items
            .into_iter()
            .filter_map(|v| serde_json::from_value::<LidlItem>(v).ok())
            .filter_map(|item| to_hit(&item))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(v: serde_json::Value) -> LidlItem {
        serde_json::from_value(v).unwrap()
    }

    // Shapes taken from real lidl.se search results (trimmed).
    fn regular() -> serde_json::Value {
        json!({
            "code": "10001",
            "gridbox": { "data": {
                "fullTitle": "Chokladmjölk",
                "category": "Food",
                "price": { "price": 17.9, "basePrice": { "text": "29,83 kr/l" }, "packaging": { "text": "600 ml" } }
            }}
        })
    }

    // On a Lidl Plus offer: no plain price, only the member price and the price before it.
    fn lidl_plus() -> serde_json::Value {
        json!({
            "code": "10002",
            "gridbox": { "data": {
                "fullTitle": "MATRIKET Standardmjölk med längre hållbarhet",
                "brand": { "name": "MATRIKET" },
                "category": "Food",
                "price": {},
                "regionsV2": { "1": { "isDefault": true, "regionPriceId": "1" }, "2": { "isDefault": false, "regionPriceId": "1" } },
                "regionsPrices": { "1": { "currentLidlPlusPrice": {
                    "price": { "price": 12.9, "oldPrice": 15.9, "basePrice": { "text": "8,60 kr/l" }, "packaging": { "text": "1,5 l" } }
                }}}
            }}
        })
    }

    #[test]
    fn maps_a_regular_price() {
        let hit = to_hit(&item(regular())).unwrap();
        assert_eq!(hit.price, 17.9);
        assert_eq!(hit.unit_price, Some(29.83));
        assert_eq!(hit.unit, Some(Unit::L));
        assert_eq!(hit.size.as_deref(), Some("600 ml"));
    }

    #[test]
    fn uses_the_non_member_price_of_a_lidl_plus_offer_with_its_own_unit_price() {
        let hit = to_hit(&item(lidl_plus())).unwrap();
        assert_eq!(hit.name, "Standardmjölk med längre hållbarhet");
        assert_eq!(hit.brand.as_deref(), Some("MATRIKET"));
        assert_eq!(hit.price, 15.9);
        assert_eq!(hit.unit, Some(Unit::L));
        assert_eq!(hit.size.as_deref(), Some("1,5 l"));
        assert_eq!(hit.old_price, None);
        assert!((hit.unit_price.unwrap() - 10.6).abs() < 0.005);
    }

    #[test]
    fn skips_items_without_any_price_and_non_food() {
        let mut bare = lidl_plus();
        bare["gridbox"]["data"]["regionsPrices"] = json!({});
        assert!(to_hit(&item(bare)).is_none());
        let mut non_food = regular();
        non_food["gridbox"]["data"]["category"] = json!("NonFood");
        assert!(to_hit(&item(non_food)).is_none());
    }

    #[test]
    fn a_bare_per_kg_base_price_means_the_price_is_per_kilo() {
        let mut v = regular();
        v["gridbox"]["data"]["price"] = json!({ "price": 24.9, "oldPrice": 29.9, "basePrice": { "text": " / kg" } });
        v["gridbox"]["data"]["brand"] = json!("Lidl");
        let hit = to_hit(&item(v)).unwrap();
        assert_eq!((hit.unit_price, hit.unit, hit.per_kg), (Some(24.9), Some(Unit::Kg), Some(true)));
        assert_eq!(hit.old_price, Some(29.9));
        assert_eq!(hit.brand.as_deref(), Some("Lidl"));
        assert_eq!(hit.url, "https://www.lidl.se");
    }

    #[test]
    fn the_stores_load_and_queries_encode_like_js() {
        assert!(STORES.len() > 100 && STORES.iter().all(|s| s.chain == ChainId::Lidl));
        assert_eq!(encode_uri_component("mjölk & bröd"), "mj%C3%B6lk%20%26%20br%C3%B6d");
    }
}
