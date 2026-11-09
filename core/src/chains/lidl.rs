//! Lidl Sweden has no full online grocery shop; its site search covers the current
//! assortment and weekly deals, with one national price list:
//!   GET https://www.lidl.se/q/api/search?q=<query>&assortment=SE&locale=sv_SE&version=v2.0.0&fetchsize=<n>
//! Store locations come from data/lidl-stores.json (see scripts/build-lidl-stores.mjs).
//! The search answers 401 to `accept: application/json`; `http` sends `*/*`.

use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use serde::de::{self, DeserializeOwned, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use super::ChainAdapter;
use crate::http::{self, FetchOptions};
use crate::normalize::{parse_unit_price, unit_price_from_size};
use crate::types::{ChainId, Pricing, ProductHit, Store, Unit};

// Every field below is read leniently: a value of an unexpected type reads as absent
// (as a JS property access would leave it unusable) instead of failing the whole item,
// so one odd field never hides an otherwise good product.

/// Any JSON; `None` when it doesn't fit `T` (or is null).
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Option<T>, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(d)?).ok())
}

/// A string, or a number written like JS `String(n)`: in JS `obj[1]` and `obj["1"]`
/// are the same key, so a numeric `regionPriceId` still finds its region.
fn js_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(match (n.as_i64(), n.as_f64()) {
            (Some(i), _) => i.to_string(),
            (None, Some(f)) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{f:.0}"),
            _ => n.to_string(),
        }),
        _ => None,
    }
}

fn lenient_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(js_string(&Value::deserialize(d)?))
}

/// A number, or a numeric string read like JS `Number()` (a blank string is 0).
fn lenient_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                Some(0.0)
            } else {
                t.parse::<f64>().ok().filter(|x| x.is_finite())
            }
        }
        _ => None,
    })
}

/// JS truthiness of any JSON value.
fn lenient_truthy<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Null => false,
        Value::Bool(b) => b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    })
}

/// `regionsV2`'s entries in the order JS `Object.values` walks them (integer-like keys
/// ascending, then the other keys in document order), so `.find(isDefault)` picks the
/// same region when several claim to be the default. `None` if it isn't an object.
fn js_ordered_regions<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<(String, Region)>>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Option<Vec<(String, Region)>>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("any JSON")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some((k, v)) = map.next_entry::<String, Value>()? {
                // A non-object entry has no `isDefault`, so it is never the default.
                if let Ok(r) = serde_json::from_value::<Region>(v) {
                    out.push((k, r));
                }
            }
            // JS array-index keys: canonical decimal below 2^32 - 1.
            let index = |k: &str| -> Option<u32> {
                if k != "0" && k.starts_with('0') {
                    return None;
                }
                k.parse::<u32>().ok().filter(|n| *n != u32::MAX && k.bytes().all(|b| b.is_ascii_digit()))
            };
            out.sort_by_key(|(k, _)| index(k).map_or((1, 0), |n| (0, n)));
            Ok(Some(out))
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D2: Deserializer<'de>>(self, d: D2) -> Result<Self::Value, D2::Error> {
            d.deserialize_any(self)
        }
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            while seq.next_element::<de::IgnoredAny>()?.is_some() {}
            Ok(None)
        }
    }
    d.deserialize_any(V)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LidlPrice {
    #[serde(deserialize_with = "lenient_f64")]
    pub price: Option<f64>,
    #[serde(deserialize_with = "lenient_f64")]
    pub old_price: Option<f64>,
    #[serde(deserialize_with = "lenient")]
    pub base_price: Option<Text>,
    #[serde(deserialize_with = "lenient")]
    pub packaging: Option<Text>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Text {
    #[serde(deserialize_with = "lenient_string")]
    pub text: Option<String>,
}

/// Lidl sends the brand either as `{ name }` or as a bare string (anything else: none).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Brand {
    Name(String),
    Object {
        #[serde(default, deserialize_with = "lenient_string")]
        name: Option<String>,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Region {
    /// JS truthiness, as in the TS's `.find((r) => r.isDefault)`.
    #[serde(deserialize_with = "lenient_truthy")]
    pub is_default: bool,
    #[serde(deserialize_with = "lenient_string")]
    pub region_price_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RegionPrices {
    #[serde(deserialize_with = "lenient")]
    pub current_lidl_plus_price: Option<PriceWrap>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PriceWrap {
    #[serde(deserialize_with = "lenient")]
    pub price: Option<LidlPrice>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LidlData {
    #[serde(deserialize_with = "lenient_string")]
    pub full_title: Option<String>,
    #[serde(deserialize_with = "lenient")]
    pub brand: Option<Brand>,
    #[serde(deserialize_with = "lenient_string")]
    pub category: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub canonical_url: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub image: Option<String>,
    #[serde(deserialize_with = "lenient")]
    pub price: Option<LidlPrice>,
    /// In JS `Object.values` order (see `js_ordered_regions`).
    #[serde(deserialize_with = "js_ordered_regions")]
    pub regions_v2: Option<Vec<(String, Region)>>,
    /// Per price region; an item on a Lidl Plus offer has only `currentLidlPlusPrice`.
    /// Entries are read only for the region looked up, so an odd one elsewhere is harmless.
    #[serde(deserialize_with = "lenient")]
    pub regions_prices: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Gridbox {
    #[serde(deserialize_with = "lenient")]
    pub data: Option<LidlData>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LidlItem {
    #[serde(deserialize_with = "lenient_string")]
    pub code: Option<String>,
    #[serde(deserialize_with = "lenient")]
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

// JS `/^\s*\/\s*kg\b/i`: `\s` is Unicode in both; `\b` is ASCII in JS, and JS's `/i`
// without `u` doesn't fold the Kelvin sign onto `k` as Rust's Unicode `(?i)` would.
static PER_KG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*/\s*[kK][gG](?-u:\b)").unwrap());

/// JS `String.prototype.trim`: Unicode white space plus the BOM.
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

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
        .and_then(|rs| rs.iter().find(|(_, r)| r.is_default))
        .and_then(|(_, r)| r.region_price_id.clone())
        .unwrap_or_else(|| "1".to_string());
    let plain = d.price.as_ref();
    let plus: Option<LidlPrice> = if truthy(plain.and_then(|p| p.price)).is_some() {
        None
    } else {
        d.regions_prices
            .as_ref()
            .and_then(|rp| rp.get(&region))
            .and_then(|v| serde_json::from_value::<RegionPrices>(v.clone()).ok())
            .and_then(|r| r.current_lidl_plus_price)
            .and_then(|w| w.price)
    };
    let plus = plus.as_ref();
    // `??`, not `||`: a plain price of 0 is kept (and then skipped below), not replaced.
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
    let full_title = d.full_title.as_deref().unwrap_or("");
    // Lidl titles often start with the brand in caps ("MATRIKET Standardmjölk").
    let name = match &brand {
        Some(b) if !b.is_empty() && full_title.starts_with(b.as_str()) => js_trim(&full_title[b.len()..]).to_string(),
        _ => full_title.to_string(),
    };
    let old_price = plain.and_then(|p| truthy(p.old_price)).filter(|o| *o > price);
    Some(ProductHit {
        chain: ChainId::Lidl,
        id: item.code.clone().unwrap_or_default(),
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
    items: Vec<Value>,
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
    fn per_kg_needs_a_word_end_and_ignores_the_kelvin_sign() {
        let with_base = |text: &str| {
            let mut v = regular();
            v["gridbox"]["data"]["price"] = json!({ "price": 24.9, "basePrice": { "text": text }, "packaging": { "text": "500 g" } });
            to_hit(&item(v)).unwrap()
        };
        assert_eq!(with_base("/KG").per_kg, Some(true));
        // `\b` is ASCII in JS: "å" is not a word character there, so "/kgå" still matches.
        assert_eq!(with_base("/kgå").per_kg, Some(true));
        assert_eq!(with_base("/kgs").per_kg, None);
        // U+212A KELVIN SIGN: JS `/i` (no `u` flag) does not match it to "k".
        let kelvin = with_base("/\u{212A}g");
        assert_eq!((kelvin.per_kg, kelvin.unit_price, kelvin.unit), (None, Some(49.8), Some(Unit::Kg)));
    }

    #[test]
    fn a_zero_plain_price_is_not_replaced_by_the_lidl_plus_price() {
        // `d.price?.price ?? plus?.oldPrice`: 0 is not nullish, so the item has no price.
        let mut v = lidl_plus();
        v["gridbox"]["data"]["price"] = json!({ "price": 0 });
        assert!(to_hit(&item(v)).is_none());
    }

    #[test]
    fn an_odd_field_does_not_hide_the_product() {
        // Each of these failed the whole item before; the TS reads them as unusable/absent.
        let mut v = lidl_plus();
        v["code"] = json!(10002);
        v["gridbox"]["data"]["brand"] = json!(42);
        v["gridbox"]["data"]["image"] = json!({ "src": "x" });
        v["gridbox"]["data"]["regionsV2"]["1"]["isDefault"] = json!(1);
        v["gridbox"]["data"]["regionsPrices"]["9"] = json!(null);
        let hit = to_hit(&item(v)).unwrap();
        assert_eq!((hit.id.as_str(), hit.price, hit.brand, hit.image), ("10002", 15.9, None, None));
        assert_eq!(hit.name, "MATRIKET Standardmjölk med längre hållbarhet");
    }

    #[test]
    fn a_numeric_region_price_id_finds_its_region_like_js_keys() {
        let mut v = lidl_plus();
        v["gridbox"]["data"]["regionsV2"] = json!({ "1": { "isDefault": true, "regionPriceId": 2 } });
        v["gridbox"]["data"]["regionsPrices"] = json!({ "2": { "currentLidlPlusPrice": { "price": { "oldPrice": 19.9 } } } });
        assert_eq!(to_hit(&item(v)).unwrap().price, 19.9);
    }

    #[test]
    fn the_default_region_is_found_in_js_object_order() {
        // JS walks integer keys ascending ("2" before "10"), then the rest in document order.
        let mut v = lidl_plus();
        v["gridbox"]["data"]["regionsV2"] = serde_json::from_str(
            r#"{ "x": { "isDefault": true, "regionPriceId": "x" },
                 "10": { "isDefault": true, "regionPriceId": "10" },
                 "2": { "isDefault": true, "regionPriceId": "2" } }"#,
        )
        .unwrap();
        v["gridbox"]["data"]["regionsPrices"] = json!({
            "2": { "currentLidlPlusPrice": { "price": { "oldPrice": 2.0 } } },
            "10": { "currentLidlPlusPrice": { "price": { "oldPrice": 10.0 } } },
            "x": { "currentLidlPlusPrice": { "price": { "oldPrice": 99.0 } } }
        });
        assert_eq!(to_hit(&item(v)).unwrap().price, 2.0);
        // No default at all: region "1".
        let mut v = lidl_plus();
        v["gridbox"]["data"]["regionsV2"] = json!({ "1": { "isDefault": false } });
        assert_eq!(to_hit(&item(v)).unwrap().price, 15.9);
    }

    #[test]
    fn the_stores_load_and_queries_encode_like_js() {
        assert!(STORES.len() > 100 && STORES.iter().all(|s| s.chain == ChainId::Lidl));
        assert_eq!(encode_uri_component("mjölk & bröd"), "mj%C3%B6lk%20%26%20br%C3%B6d");
    }

    /// Compares with the TS on a saved live answer: LIDL_PARITY=<search.json>,<ts-hits.json>.
    #[test]
    fn parity_with_saved_live_answer() {
        let Ok(paths) = std::env::var("LIDL_PARITY") else { return };
        let (a, b) = paths.split_once(',').unwrap();
        let data: SearchResponse = serde_json::from_str(&std::fs::read_to_string(a).unwrap()).unwrap();
        let hits: Vec<ProductHit> = data
            .items
            .into_iter()
            .filter_map(|v| serde_json::from_value::<LidlItem>(v).ok())
            .filter_map(|i| to_hit(&i))
            .collect();
        let ours = serde_json::to_value(&hits).unwrap();
        let theirs: Value = serde_json::from_str(&std::fs::read_to_string(b).unwrap()).unwrap();
        assert_eq!(ours, theirs);
    }
}
