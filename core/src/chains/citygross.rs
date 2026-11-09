//! City Gross exposes the JSON its own web shop uses:
//!   GET https://www.citygross.se/api/v1/Loop54/search?SearchQuery=<query>&type=product&Take=<n>
//!   GET https://www.citygross.se/api/v1/PageData/stores   every store page, with coordinates
//! Search returns the prices of the site's default store, so prices are treated as
//! chain-wide (like Hemköp). robots.txt disallows the /loop54/ page route, not /api/v1/.

use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use regex::{Captures, Regex};
use serde::Deserialize;

use super::ChainAdapter;
use crate::http::{self, FetchOptions};
use crate::normalize::{parse_size, round2};
use crate::types::{ChainId, Deal, Pricing, ProductHit, Store, Unit};

const BASE: &str = "https://www.citygross.se";
/// No Crawl-delay in robots.txt; stay well below what a shopper's browser does.
const INTERVAL: Duration = Duration::from_millis(2000);

/// JavaScript's `\s`, as a regex character class body (for `title_case`).
const JS_SPACE: &str = r"\t\n\x0B\x0C\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";

fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\x0B' | '\x0C' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}'
            | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// `String.prototype.trim`.
fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CgPrice {
    pub price: Option<f64>,
    /// "KGM" when the price is per kilo (sold by weight), "PCE" per item.
    pub unit: Option<String>,
    pub comparative_price: Option<f64>,
    /// "KGM", "LTR" or "PCE".
    pub comparative_price_unit: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CgPromotion {
    pub effect_type: Option<String>,
    pub members_only: Option<bool>,
    pub min_quantity: Option<f64>,
    /// Total price for `min_quantity` items.
    pub value: Option<f64>,
    pub max_applied_per_receipt: Option<f64>,
    pub price_details: Option<CgPrice>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CgPrices {
    pub current_price: Option<CgPrice>,
    /// An odd ordinary price or promotion drops only that part (the TS kept the
    /// product and just didn't read them), never the whole product.
    #[serde(deserialize_with = "lenient")]
    pub ordinary_price: Option<CgPrice>,
    #[serde(deserialize_with = "lenient")]
    pub active_promotion: Option<CgPromotion>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CgStoreDetails {
    pub prices: Option<CgPrices>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CgImage {
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CgProduct {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub brand: Option<String>,
    #[serde(default)]
    pub descriptive_size: Option<String>,
    #[serde(default)]
    pub super_category: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    /// The most specific category, e.g. "Bryggkaffe", "Bananer".
    #[serde(default)]
    pub bf_category: Option<String>,
    pub url: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub images: Vec<CgImage>,
    #[serde(default)]
    pub product_store_details: Option<CgStoreDetails>,
}

fn lenient<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).ok())
}

fn null_as_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct CgLocation {
    coordinates: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CgAddress {
    street_address: Option<String>,
    city: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CgStoreData {
    #[serde(rename = "type", default)]
    type_: Option<String>,
    #[serde(default)]
    ispublished: Option<bool>,
    /// A number in practice; kept as JSON so a string id still makes `cg-<id>`, as in the TS.
    site_id: serde_json::Value,
    store_name: String,
    url: String,
    #[serde(default)]
    store_location: Option<CgLocation>,
    #[serde(default)]
    address: Option<CgAddress>,
}

#[derive(Debug, Clone, Deserialize)]
struct CgStorePage {
    data: CgStoreData,
}

fn unit_of(code: Option<&str>) -> Option<Unit> {
    match code? {
        "KGM" => Some(Unit::Kg),
        "LTR" => Some(Unit::L),
        "PCE" => Some(Unit::St),
        _ => None,
    }
}

static TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"(^|[{JS_SPACE}&-])(\p{{L}})")).unwrap());

/// Brands come in capitals ("DAVE & JON'S"); sizes too ("CA 170G").
fn title_case(s: &str) -> String {
    TITLE.replace_all(&s.to_lowercase(), |c: &Captures| c[0].to_uppercase()).into_owned()
}

/// JS truthiness for an optional number: present, not 0, not NaN.
fn truthy(n: Option<f64>) -> Option<f64> {
    n.filter(|v| *v != 0.0 && !v.is_nan())
}

pub fn to_hit(p: &CgProduct) -> Option<ProductHit> {
    let prices = p.product_store_details.as_ref()?.prices.as_ref()?;
    let current = prices.current_price.as_ref()?;
    let price = truthy(current.price)?;
    let per_kg = current.unit.as_deref() == Some("KGM");
    // A piece of loose fruit: its price per kilo times the approximate weight ("CA 170G").
    let weight = if per_kg { p.descriptive_size.as_deref().and_then(parse_size).map(|(amount, _)| amount) } else { None };
    let mut hit = ProductHit {
        chain: ChainId::Citygross,
        id: p.id.clone(),
        name: p.name.clone(),
        brand: p.brand.as_deref().filter(|b| !b.is_empty()).map(title_case),
        size: p.descriptive_size.as_ref().map(|s| s.to_lowercase()),
        categories: Some(
            [&p.super_category, &p.category, &p.bf_category]
                .into_iter()
                .filter_map(|c| c.as_ref().filter(|c| !c.is_empty()).cloned())
                .collect(),
        ),
        price,
        unit_price: current.comparative_price,
        unit: unit_of(current.comparative_price_unit.as_deref()),
        per_kg: per_kg.then_some(true),
        approx_each: truthy(weight).map(|w| round2(price * w)),
        old_price: None,
        deal: None,
        image: p
            .images
            .first()
            .and_then(|i| i.url.as_deref())
            .filter(|u| !u.is_empty())
            .map(|u| format!("{BASE}/images/products/{u}")),
        url: format!("{BASE}{}", p.url),
        pricing: Pricing::Chain,
    };
    if let Some(ordinary) = truthy(prices.ordinary_price.as_ref().and_then(|o| o.price)) {
        if ordinary > price {
            hit.old_price = Some(ordinary);
        }
    }

    // Multi-buy offers ("2 för 22 kr"). Members-only prices need a City Gross account, so they're left out.
    if let Some(promo) = &prices.active_promotion {
        let each = truthy(promo.price_details.as_ref().and_then(|d| d.price));
        let quantity = promo.min_quantity.unwrap_or(0.0);
        if let Some(each) = each {
            if !promo.members_only.unwrap_or(false)
                && promo.effect_type.as_deref() == Some("ItemsTotal")
                && quantity > 1.0
                && each < price
            {
                let max = promo.max_applied_per_receipt.unwrap_or(0.0);
                hit.deal = Some(Deal {
                    quantity: quantity as u32,
                    each,
                    total: promo.value.unwrap_or_else(|| round2(each * quantity)),
                    unit_price: promo.price_details.as_ref().and_then(|d| d.comparative_price),
                    mix_and_match: false,
                    plus_deposit: false,
                    limit: (max > 0.0).then_some(max as u32),
                });
            }
        }
    }
    Some(hit)
}

/// `Number(s)` for a coordinate: whitespace trimmed, empty is 0, junk is NaN.
/// Rust also reads "inf"/"nan"/"infinity" (any case), which JS doesn't: only
/// `Infinity` spelled that way is a number there.
fn js_number(s: &str) -> f64 {
    let t = js_trim(s);
    if t.is_empty() {
        return 0.0;
    }
    let digits = t.trim_start_matches(['+', '-']);
    if digits.chars().any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E') && digits != "Infinity" {
        return f64::NAN;
    }
    t.parse().unwrap_or(f64::NAN)
}

/// A JSON scalar in a JS template string (`${s.siteId}`).
fn js_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{f}"),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

fn to_store(s: &CgStoreData) -> Option<Store> {
    let coords = s.store_location.as_ref().and_then(|l| l.coordinates.as_deref()).unwrap_or("");
    let mut parts = coords.split(',').map(js_number);
    let lat = truthy(parts.next())?;
    let lng = truthy(parts.next())?;
    if s.type_.as_deref() != Some("StorePage") || !s.ispublished.unwrap_or(false) {
        return None;
    }
    let street = s.address.as_ref().and_then(|a| a.street_address.as_deref()).map(js_trim);
    let city = s.address.as_ref().and_then(|a| a.city.as_deref()).map(js_trim);
    let address = [street, city].into_iter().flatten().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(", ");
    Some(Store {
        id: format!("cg-{}", js_string(&s.site_id)),
        chain: ChainId::Citygross,
        name: format!("City Gross {}", js_trim(&s.store_name)),
        lat,
        lng,
        address,
        url: Some(format!("{BASE}{}", s.url)),
    })
}

/// `encodeURIComponent`.
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

/// Each element parsed on its own, so one odd product or store page is skipped
/// rather than failing the whole list.
fn parse_each<T: serde::de::DeserializeOwned>(items: Vec<serde_json::Value>) -> impl Iterator<Item = T> {
    items.into_iter().filter_map(|v| serde_json::from_value(v).ok())
}

/// `searchResults.products` must be there: the TS threw on a missing or null list, so a
/// changed answer is reported as an error rather than as "no products".
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchResponse {
    search_results: SearchResults,
}

#[derive(Deserialize)]
struct SearchResults {
    products: Vec<serde_json::Value>,
}

pub struct CityGross;

#[async_trait]
impl ChainAdapter for CityGross {
    fn chain(&self) -> ChainId {
        ChainId::Citygross
    }
    fn pricing(&self) -> Pricing {
        Pricing::Chain
    }
    async fn list_stores(&self) -> http::Result<Vec<Store>> {
        let pages: Vec<serde_json::Value> = http::fetch_json(
            &format!("{BASE}/api/v1/PageData/stores"),
            &FetchOptions::every(INTERVAL).timeout(Duration::from_secs(20)),
        )
        .await?;
        Ok(parse_each::<CgStorePage>(pages).filter_map(|p| to_store(&p.data)).collect())
    }
    async fn search(&self, query: &str, _store: Option<&Store>) -> http::Result<Vec<ProductHit>> {
        let data: SearchResponse = http::fetch_json(
            &format!("{BASE}/api/v1/Loop54/search?SearchQuery={}&type=product&Take=40", encode_uri_component(query)),
            &FetchOptions::every(INTERVAL),
        )
        .await?;
        Ok(parse_each::<CgProduct>(data.search_results.products).filter_map(|p| to_hit(&p)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Shapes taken from real citygross.se search results.
    fn price(price: f64, unit: &str, comparative_price: f64, comparative_price_unit: &str) -> serde_json::Value {
        json!({ "price": price, "unit": unit, "comparativePrice": comparative_price, "comparativePriceUnit": comparative_price_unit })
    }

    fn base() -> serde_json::Value {
        json!({
            "id": "100235247_KG",
            "name": "Banan Eko",
            "brand": null,
            "descriptiveSize": "CA 170G",
            "url": "/matvaror/frukt-och-gront/frukt/banan-eko-p100235247_KG",
            "images": [{ "url": "1st_banan-600.jpg" }],
            "productStoreDetails": {
                "prices": {
                    "currentPrice": price(22.95, "KGM", 22.95, "KGM"),
                    "ordinaryPrice": price(22.95, "KGM", 22.95, "KGM"),
                    "activePromotion": null
                }
            }
        })
    }

    fn with(mut v: serde_json::Value, patch: serde_json::Value) -> CgProduct {
        for (k, val) in patch.as_object().unwrap() {
            v[k] = val.clone();
        }
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn maps_a_product_sold_by_weight() {
        let hit = to_hit(&with(base(), json!({}))).unwrap();
        assert_eq!(hit.chain, ChainId::Citygross);
        assert_eq!(hit.price, 22.95);
        assert_eq!(hit.unit_price, Some(22.95));
        assert_eq!(hit.unit, Some(Unit::Kg));
        assert_eq!(hit.per_kg, Some(true));
        assert_eq!(hit.approx_each, Some(3.9));
        assert_eq!(hit.size.as_deref(), Some("ca 170g"));
        assert_eq!(hit.image.as_deref(), Some("https://www.citygross.se/images/products/1st_banan-600.jpg"));
        assert_eq!(hit.url, "https://www.citygross.se/matvaror/frukt-och-gront/frukt/banan-eko-p100235247_KG");
        assert_eq!(hit.brand, None);
        assert_eq!(hit.old_price, None);
        assert_eq!(hit.categories, Some(vec![]));
    }

    #[test]
    fn maps_a_2_for_22_offer() {
        let hit = to_hit(&with(
            base(),
            json!({
                "name": "Chokladbanan",
                "brand": "AROMA",
                "descriptiveSize": "100G",
                "productStoreDetails": { "prices": {
                    "currentPrice": price(13.5, "PCE", 135.0, "KGM"),
                    "ordinaryPrice": price(13.5, "PCE", 135.0, "KGM"),
                    "activePromotion": {
                        "effectType": "ItemsTotal",
                        "membersOnly": false,
                        "minQuantity": 2,
                        "value": 22,
                        "maxAppliedPerReceipt": 0,
                        "priceDetails": price(11.0, "PCE", 110.0, "KGM")
                    }
                } }
            }),
        ))
        .unwrap();
        assert_eq!(hit.brand.as_deref(), Some("Aroma"));
        assert_eq!(hit.price, 13.5);
        assert_eq!(hit.per_kg, None);
        assert_eq!(
            hit.deal,
            Some(Deal {
                quantity: 2,
                each: 11.0,
                total: 22.0,
                unit_price: Some(110.0),
                mix_and_match: false,
                plus_deposit: false,
                limit: None
            })
        );
    }

    #[test]
    fn leaves_out_members_only_offers_and_products_without_a_price() {
        let members = to_hit(&with(
            base(),
            json!({
                "productStoreDetails": { "prices": {
                    "currentPrice": price(13.5, "PCE", 135.0, "KGM"),
                    "ordinaryPrice": null,
                    "activePromotion": {
                        "effectType": "ItemsTotal",
                        "membersOnly": true,
                        "minQuantity": 2,
                        "value": 22,
                        "maxAppliedPerReceipt": 0,
                        "priceDetails": price(11.0, "PCE", 110.0, "KGM")
                    }
                } }
            }),
        ));
        assert_eq!(members.unwrap().deal, None);
        assert!(to_hit(&with(base(), json!({ "productStoreDetails": null }))).is_none());
    }

    #[test]
    fn sale_price_categories_and_missing_deal_total() {
        let hit = to_hit(&with(
            base(),
            json!({
                "brand": "DAVE & JON'S",
                "superCategory": "Frukt och grönt",
                "category": "",
                "bfCategory": "Bananer",
                "productStoreDetails": { "prices": {
                    "currentPrice": price(20.0, "PCE", 200.0, "KGM"),
                    "ordinaryPrice": price(25.0, "PCE", 250.0, "KGM"),
                    "activePromotion": {
                        "effectType": "ItemsTotal",
                        "membersOnly": false,
                        "minQuantity": 3,
                        "value": null,
                        "maxAppliedPerReceipt": 2,
                        "priceDetails": price(16.333, "PCE", 163.33, "KGM")
                    }
                } }
            }),
        ))
        .unwrap();
        assert_eq!(hit.brand.as_deref(), Some("Dave & Jon's"));
        assert_eq!(hit.old_price, Some(25.0));
        assert_eq!(hit.categories, Some(vec!["Frukt och grönt".to_string(), "Bananer".to_string()]));
        let deal = hit.deal.unwrap();
        assert_eq!(deal.total, 49.0);
        assert_eq!(deal.limit, Some(2));
        // Not sold by weight: no per-piece estimate.
        assert_eq!(hit.approx_each, None);
        assert_eq!(hit.unit, Some(Unit::Kg));
    }

    #[test]
    fn title_case_follows_the_ts() {
        assert_eq!(title_case("DAVE & JON'S"), "Dave & Jon's");
        assert_eq!(title_case("ÄNGLAMARK-EKO"), "Änglamark-Eko");
        assert_eq!(title_case("CA 170G"), "Ca 170g");
    }

    #[test]
    fn maps_store_pages() {
        let page = |v: serde_json::Value| serde_json::from_value::<CgStorePage>(json!({ "data": v })).unwrap().data;
        let s = to_store(&page(json!({
            "type": "StorePage", "ispublished": true, "siteId": 101, "storeName": " Malmö Hyllie ",
            "url": "/butiker/malmo-hyllie", "storeLocation": { "coordinates": "55.56, 12.97" },
            "address": { "streetAddress": " Hyllie Boulevard 1 ", "city": "" }
        })))
        .unwrap();
        assert_eq!(s.id, "cg-101");
        assert_eq!(s.name, "City Gross Malmö Hyllie");
        assert_eq!((s.lat, s.lng), (55.56, 12.97));
        assert_eq!(s.address, "Hyllie Boulevard 1");
        assert_eq!(s.url.as_deref(), Some("https://www.citygross.se/butiker/malmo-hyllie"));
        assert!(to_store(&page(json!({
            "type": "StorePage", "ispublished": false, "siteId": 1, "storeName": "X", "url": "/x",
            "storeLocation": { "coordinates": "55.5,13.0" }, "address": null
        })))
        .is_none());
        assert!(to_store(&page(json!({
            "type": "StorePage", "ispublished": true, "siteId": 1, "storeName": "X", "url": "/x",
            "storeLocation": null, "address": null
        })))
        .is_none());
    }

    #[test]
    fn encodes_like_encode_uri_component() {
        assert_eq!(encode_uri_component("mjölk & ägg"), "mj%C3%B6lk%20%26%20%C3%A4gg");
    }

    #[test]
    fn a_search_answer_without_products_is_an_error() {
        assert!(serde_json::from_value::<SearchResponse>(json!({})).is_err());
        assert!(serde_json::from_value::<SearchResponse>(json!({ "searchResults": { "products": null } })).is_err());
        let ok = serde_json::from_value::<SearchResponse>(json!({ "searchResults": { "products": [] } })).unwrap();
        assert!(ok.search_results.products.is_empty());
    }

    #[test]
    fn an_odd_promotion_keeps_the_product() {
        let hit = to_hit(&with(
            base(),
            json!({ "productStoreDetails": { "prices": {
                "currentPrice": price(13.5, "PCE", 135.0, "KGM"),
                "ordinaryPrice": "n/a",
                "activePromotion": { "effectType": "ItemsTotal", "minQuantity": "two" }
            } } }),
        ))
        .unwrap();
        assert_eq!(hit.price, 13.5);
        assert_eq!(hit.deal, None);
        assert_eq!(hit.old_price, None);
    }

    #[test]
    fn a_promotion_without_members_only_counts() {
        // `!undefined` is true in the TS.
        let hit = to_hit(&with(
            base(),
            json!({ "productStoreDetails": { "prices": {
                "currentPrice": price(13.5, "PCE", 135.0, "KGM"),
                "activePromotion": { "effectType": "ItemsTotal", "minQuantity": 2, "priceDetails": price(11.0, "PCE", 110.0, "KGM") }
            } } }),
        ))
        .unwrap();
        let deal = hit.deal.unwrap();
        assert_eq!((deal.quantity, deal.total, deal.limit), (2, 22.0, None));
    }

    #[test]
    fn coordinates_and_ids_read_like_js() {
        assert!(js_number("inf").is_nan());
        assert!(js_number("nan").is_nan());
        assert_eq!(js_number(" 55.5 "), 55.5);
        assert_eq!(js_number("1e1"), 10.0);
        assert_eq!(js_number("Infinity"), f64::INFINITY);
        assert_eq!(js_string(&json!(101)), "101");
        assert_eq!(js_string(&json!(101.0)), "101");
        assert_eq!(js_string(&json!("7")), "7");
        let page = |v: serde_json::Value| serde_json::from_value::<CgStorePage>(json!({ "data": v })).unwrap().data;
        let s = to_store(&page(json!({
            "type": "StorePage", "ispublished": true, "siteId": "42", "storeName": "Lund", "url": "/lund",
            "storeLocation": { "coordinates": "55.7,13.19" }, "address": { "streetAddress": "X 1" }
        })))
        .unwrap();
        assert_eq!(s.id, "cg-42");
        assert!(to_store(&page(json!({
            "type": "StorePage", "ispublished": true, "siteId": 1, "storeName": "X", "url": "/x",
            "storeLocation": { "coordinates": "inf,13.0" }
        })))
        .is_none());
    }

    #[test]
    fn a_bad_product_is_skipped() {
        let items = vec![json!({ "id": 5 }), base()];
        assert_eq!(parse_each::<CgProduct>(items).count(), 1);
    }
}
