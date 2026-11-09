//! Willys and Hemköp, ported from src/lib/server/chains/axfood.ts.
//!
//! Willys and Hemköp both run on Axfood's platform and expose the JSON their own
//! web shops use:
//!   GET https://www.<site>/search?q=<query>&size=<n>     product search
//!   GET https://www.<site>/axfood/rest/store?online=false all stores incl. coordinates
//! Search returns the price list of the site's default store; there is no
//! unauthenticated way to pick another store, so prices are treated as chain-wide.
//! robots.txt asks for Crawl-delay: 10, which `http::fetch_json` enforces per host. That
//! one turn per 10 s goes to searches only: the store lists are prebuilt into core/data/
//! by scripts/build-axfood-stores.mjs (bun run stores:axfood).

use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Deserializer};

use super::ChainAdapter;
use crate::http::{self, Error, FetchOptions};
use crate::normalize::{parse_sek, round2, to_unit};
use crate::types::{ChainId, Deal, Pricing, ProductHit, Store};

const CRAWL_DELAY: Duration = Duration::from_secs(10);

/// JSON `null` reads as the type's default, like a missing field.
fn null_default<'de, D: Deserializer<'de>, T: Default + Deserialize<'de>>(d: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// A number, or a string JS arithmetic would coerce to one (`"0.18"`); anything else
/// (null, "", objects) reads as `None` instead of making the whole product unreadable.
fn lenient_num<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok().filter(|x| x.is_finite()),
        _ => None,
    })
}

/// `lenient_num`, with what isn't a number reading as 0, so the search drops the product
/// (as the TS `priceValue > 0` filter does for null).
fn lenient_num_or_zero<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(lenient_num(d)?.unwrap_or(0.0))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxfoodPromotionPrice {
    #[serde(default, deserialize_with = "lenient_num")]
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxfoodPromotion {
    #[serde(default, deserialize_with = "lenient_num")]
    pub qualifying_count: Option<f64>,
    #[serde(default)]
    pub reward_label: Option<String>,
    #[serde(default)]
    pub price: Option<AxfoodPromotionPrice>,
    /// Comparison price at the promotion price, e.g. "7,33 kr".
    #[serde(default)]
    pub compare_price: Option<String>,
    #[serde(default)]
    pub promotion_type: Option<String>,
    /// e.g. "Max 5 köp"
    #[serde(default)]
    pub redeem_limit_label: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxfoodImage {
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxfoodProduct {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub display_volume: Option<String>,
    /// Missing reads as 0, which the search drops (as the TS `priceValue > 0` filter does).
    #[serde(default, deserialize_with = "lenient_num_or_zero")]
    pub price_value: f64,
    /// "kr/kg" when `price_value` is per kilo (sold by weight), else "kr/st".
    #[serde(default)]
    pub price_unit: Option<String>,
    /// Average weight of one piece in kg, for products sold by weight.
    #[serde(default, deserialize_with = "lenient_num")]
    pub average_weight: Option<f64>,
    #[serde(default)]
    pub compare_price: Option<String>,
    #[serde(default)]
    pub compare_price_unit: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub out_of_stock: bool,
    #[serde(default)]
    pub image: Option<AxfoodImage>,
    #[serde(default, deserialize_with = "null_default")]
    pub potential_promotions: Vec<AxfoodPromotion>,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Option<Vec<serde_json::Value>>,
}

/// JS `encodeURIComponent`: everything but `A-Z a-z 0-9 - _ . ! ~ * ' ( )` is
/// percent-encoded as UTF-8 (spaces as %20, not '+').
fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `Math.round(x)` with JS's halves-toward-+∞ rule (normalize's `round2` covers ×100/100
/// only when the ×100 comes last; `scaled` multiplies before dividing).
fn js_round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// The value when JS would find it truthy: present, non-zero, not NaN.
fn truthy(x: Option<f64>) -> Option<f64> {
    x.filter(|w| *w != 0.0 && !w.is_nan())
}

static DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9]+").unwrap());
static PANT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)pant").unwrap());

pub fn to_hit(chain: ChainId, base: &str, p: &AxfoodProduct) -> ProductHit {
    let promo = p.potential_promotions.first();
    let promo_price = promo.and_then(|pr| pr.price.as_ref()).and_then(|pp| pp.value);
    let regular_unit_price = p.compare_price.as_deref().and_then(parse_sek);
    // comparePrice reflects the regular price; scale it when the promotion has none.
    let scaled = |price: f64| regular_unit_price.map(|u| js_round((u * price * 100.0) / p.price_value) / 100.0);
    let mut hit = ProductHit {
        chain,
        id: p.code.clone(),
        name: p.name.clone(),
        brand: p.manufacturer.clone(),
        size: p.display_volume.clone(),
        price: p.price_value,
        unit_price: regular_unit_price,
        unit: p.compare_price_unit.as_deref().and_then(to_unit),
        categories: None,
        per_kg: None,
        approx_each: None,
        old_price: None,
        deal: None,
        image: p.image.as_ref().and_then(|i| i.url.clone()),
        url: format!("{base}/sok?q={}", encode_uri_component(&p.name)),
        pieces: None,
        pricing: Pricing::Chain,
    };
    let average_weight = truthy(p.average_weight);
    if p.price_unit.as_deref() == Some("kr/kg") {
        hit.per_kg = Some(true);
        if let Some(w) = average_weight {
            hit.approx_each = Some(round2(p.price_value * w));
        }
    }
    let (Some(promo), Some(promo_price)) = (promo, promo_price) else {
        return hit;
    };
    if promo_price >= p.price_value {
        return hit;
    }

    let quantity = promo.qualifying_count.unwrap_or(1.0);
    let unit_price = promo.compare_price.as_deref().and_then(parse_sek).or_else(|| scaled(promo_price));
    if quantity <= 1.0 {
        // Single-item sale: it is simply the current price.
        let approx_each = match (average_weight, hit.per_kg) {
            (Some(w), Some(true)) => Some(round2(promo_price * w)),
            _ => None,
        };
        return ProductHit { price: promo_price, old_price: Some(p.price_value), unit_price, approx_each, ..hit };
    }
    let limit = promo
        .redeem_limit_label
        .as_deref()
        .and_then(|l| DIGITS.find(l))
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .filter(|&n| n > 0);
    hit.deal = Some(Deal {
        quantity: quantity as u32,
        each: promo_price,
        total: round2(promo_price * quantity),
        unit_price,
        mix_and_match: promo.promotion_type.as_deref() == Some("MixMatchPricePromotion"),
        plus_deposit: PANT.is_match(promo.reward_label.as_deref().unwrap_or("")),
        limit,
    });
    hit
}

fn load_stores(json: &str) -> Vec<Store> {
    serde_json::from_str(json).expect("bundled Axfood store list")
}

static WILLYS_STORES: LazyLock<Vec<Store>> = LazyLock::new(|| load_stores(include_str!("../../data/willys-stores.json")));
static HEMKOP_STORES: LazyLock<Vec<Store>> = LazyLock::new(|| load_stores(include_str!("../../data/hemkop-stores.json")));

pub struct Axfood {
    pub chain: ChainId,
}
pub fn willys() -> Axfood {
    Axfood { chain: ChainId::Willys }
}
pub fn hemkop() -> Axfood {
    Axfood { chain: ChainId::Hemkop }
}

impl Axfood {
    fn base(&self) -> &'static str {
        match self.chain {
            ChainId::Hemkop => "https://www.hemkop.se",
            _ => "https://www.willys.se",
        }
    }
    fn stores(&self) -> &'static [Store] {
        match self.chain {
            ChainId::Hemkop => &HEMKOP_STORES,
            _ => &WILLYS_STORES,
        }
    }
}

/// The search answer's products that are in stock and priced, skipping any that don't parse.
fn parse_results(chain: ChainId, base: &str, data: SearchResponse) -> http::Result<Vec<ProductHit>> {
    let host = base.trim_start_matches("https://");
    let results = data.results.ok_or_else(|| Error::Other(format!("{host}: unexpected answer (no results)")))?;
    Ok(results
        .into_iter()
        .filter_map(|v| serde_json::from_value::<AxfoodProduct>(v).ok())
        .filter(|p| !p.out_of_stock && p.price_value > 0.0)
        .map(|p| to_hit(chain, base, &p))
        .collect())
}

#[async_trait]
impl ChainAdapter for Axfood {
    fn chain(&self) -> ChainId {
        self.chain
    }
    fn pricing(&self) -> Pricing {
        Pricing::Chain
    }
    async fn list_stores(&self) -> http::Result<Vec<Store>> {
        Ok(self.stores().to_vec())
    }
    async fn search(&self, query: &str, _store: Option<&Store>) -> http::Result<Vec<ProductHit>> {
        let base = self.base();
        let data: SearchResponse = http::fetch_json(
            &format!("{base}/search?q={}&size=40", encode_uri_component(query)),
            // Wait for the next turn rather than fail; a newer search takes over a queued
            // turn, so the wait is never more than one Crawl-delay.
            &FetchOptions::every(CRAWL_DELAY).max_wait(Duration::from_secs(30)).latest_only(),
        )
        .await?;
        parse_results(self.chain, base, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Unit;

    const WILLYS: &str = "https://www.willys.se";

    // Shapes taken from real willys.se search results.
    fn base() -> AxfoodProduct {
        AxfoodProduct {
            code: "101233933_ST".into(),
            name: "Orange Zero Läsk Pet".into(),
            manufacturer: Some("Fanta".into()),
            display_volume: Some("1,5l".into()),
            price_value: 19.4,
            compare_price: Some("12,93 kr".into()),
            compare_price_unit: Some("l".into()),
            out_of_stock: false,
            image: None,
            potential_promotions: vec![],
            ..Default::default()
        }
    }

    fn promo(count: f64, reward: &str, price: f64, compare: &str, limit: &str) -> AxfoodPromotion {
        AxfoodPromotion {
            qualifying_count: Some(count),
            reward_label: Some(reward.into()),
            price: Some(AxfoodPromotionPrice { value: Some(price) }),
            compare_price: Some(compare.into()),
            promotion_type: Some("MixMatchPricePromotion".into()),
            redeem_limit_label: Some(limit.into()),
        }
    }

    #[test]
    fn maps_a_regular_price() {
        let hit = to_hit(ChainId::Willys, WILLYS, &base());
        assert_eq!(hit.price, 19.4);
        assert_eq!(hit.unit_price, Some(12.93));
        assert_eq!(hit.unit, Some(Unit::L));
        assert_eq!(hit.deal, None);
        assert_eq!(hit.old_price, None);
        assert_eq!(hit.url, "https://www.willys.se/sok?q=Orange%20Zero%20L%C3%A4sk%20Pet");
        assert_eq!(hit.pricing, Pricing::Chain);
    }

    #[test]
    fn estimates_the_price_of_one_piece_for_products_sold_by_weight() {
        let hit = to_hit(
            ChainId::Willys,
            WILLYS,
            &AxfoodProduct {
                name: "Banan Klass 1".into(),
                display_volume: Some("ca: 180g".into()),
                price_value: 19.9,
                price_unit: Some("kr/kg".into()),
                average_weight: Some(0.18),
                compare_price: Some("19,90 kr".into()),
                compare_price_unit: Some("kg".into()),
                ..base()
            },
        );
        assert_eq!(hit.price, 19.9);
        assert_eq!(hit.per_kg, Some(true));
        assert_eq!(hit.approx_each, Some(3.58));
        assert_eq!(hit.unit_price, Some(19.9));
        assert_eq!(hit.unit, Some(Unit::Kg));
    }

    #[test]
    fn maps_a_multi_buy_5_for_55_plus_pant_deal() {
        let hit = to_hit(
            ChainId::Willys,
            WILLYS,
            &AxfoodProduct {
                potential_promotions: vec![promo(5.0, "55,00 +pant", 11.0, "7,33 kr", "Max 5 köp")],
                ..base()
            },
        );
        assert_eq!(hit.price, 19.4); // buying one still costs the regular price
        assert_eq!(
            hit.deal,
            Some(Deal {
                quantity: 5,
                each: 11.0,
                total: 55.0,
                unit_price: Some(7.33),
                mix_and_match: true,
                plus_deposit: true,
                limit: Some(5),
            })
        );
    }

    #[test]
    fn maps_a_single_item_sale_as_the_current_price() {
        let hit = to_hit(
            ChainId::Willys,
            WILLYS,
            &AxfoodProduct {
                price_value: 79.9,
                compare_price: Some("177,56 kr".into()),
                compare_price_unit: Some("kg".into()),
                potential_promotions: vec![promo(1.0, "72,90/st", 72.9, "162,00 kr", "")],
                ..base()
            },
        );
        assert_eq!(hit.price, 72.9);
        assert_eq!(hit.old_price, Some(79.9));
        assert_eq!(hit.unit_price, Some(162.0));
        assert_eq!(hit.deal, None);
    }

    #[test]
    fn scales_the_regular_comparison_price_when_the_promotion_has_none() {
        let mut p = promo(2.0, "2 för 30", 15.0, "", "");
        p.compare_price = None;
        p.promotion_type = None;
        let hit = to_hit(ChainId::Willys, WILLYS, &AxfoodProduct { potential_promotions: vec![p], ..base() });
        let deal = hit.deal.unwrap();
        // 12.93 * 15 / 19.4 = 9.997…
        assert_eq!(deal.unit_price, Some(10.0));
        assert!(!deal.mix_and_match);
        assert!(!deal.plus_deposit);
        assert_eq!(deal.limit, None);
        assert_eq!(deal.total, 30.0);
    }

    #[test]
    fn a_promotion_that_is_not_cheaper_is_ignored() {
        let hit = to_hit(
            ChainId::Willys,
            WILLYS,
            &AxfoodProduct { potential_promotions: vec![promo(1.0, "", 19.4, "12,93 kr", "")], ..base() },
        );
        assert_eq!(hit.price, 19.4);
        assert_eq!(hit.old_price, None);
    }

    #[test]
    fn parses_a_search_answer_and_skips_bad_or_unavailable_products() {
        let json = r#"{"results": [
            {"code":"100254920_KG","name":"Banan Klass 1","manufacturer":null,"displayVolume":"ca: 180g",
             "priceValue":21.9,"priceUnit":"kr/kg","averageWeight":0.18,"comparePrice":"21,90 kr",
             "comparePriceUnit":"kg","outOfStock":false,
             "image":{"imageType":"PRIMARY","url":"https://assets.axfood.se/image/upload/f_auto,t_200/07311042002499_C1N1_s01"},
             "potentialPromotions":[]},
            {"code":"1","name":"Slut","priceValue":10,"outOfStock":true,"potentialPromotions":[]},
            {"code":"2","name":"Gratis","priceValue":0,"potentialPromotions":[]},
            {"code":"3","priceValue":10},
            {"code":"4","name":"Utan kampanjer","priceValue":12.5,"potentialPromotions":null,"outOfStock":null,"image":null}
        ]}"#;
        let hits = parse_results(ChainId::Willys, WILLYS, serde_json::from_str(json).unwrap()).unwrap();
        let ids: Vec<_> = hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, ["100254920_KG", "4"]);
        assert_eq!(hits[0].approx_each, Some(3.94));
        assert_eq!(hits[0].brand, None);
        assert!(hits[0].image.as_deref().unwrap().starts_with("https://assets.axfood.se/"));
        assert!(parse_results(ChainId::Willys, WILLYS, serde_json::from_str("{}").unwrap()).is_err());
    }

    #[test]
    fn odd_number_types_do_not_drop_the_product() {
        // JS coerces "0.18" in arithmetic and treats a null priceValue as not > 0.
        let json = r#"{"results": [
            {"code":"a","name":"Strängvikt","priceValue":"20","priceUnit":"kr/kg","averageWeight":"0.5",
             "potentialPromotions":[{"qualifyingCount":"2","price":{"value":"8"}}]},
            {"code":"b","name":"Nollpris","priceValue":null},
            {"code":"c","name":"Konstig vikt","priceValue":10,"priceUnit":"kr/kg","averageWeight":{"x":1}}
        ]}"#;
        let hits = parse_results(ChainId::Willys, WILLYS, serde_json::from_str(json).unwrap()).unwrap();
        let ids: Vec<_> = hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, ["a", "c"]);
        assert_eq!(hits[0].price, 20.0);
        assert_eq!(hits[0].approx_each, Some(10.0));
        let deal = hits[0].deal.as_ref().unwrap();
        assert_eq!((deal.quantity, deal.each, deal.total), (2, 8.0, 16.0));
        assert_eq!(hits[1].approx_each, None);
        assert_eq!(hits[1].per_kg, Some(true));
    }

    #[test]
    fn a_zero_average_weight_gives_no_estimate_and_a_zero_count_is_a_single_sale() {
        let hit = to_hit(
            ChainId::Willys,
            WILLYS,
            &AxfoodProduct {
                price_unit: Some("kr/kg".into()),
                average_weight: Some(0.0),
                potential_promotions: vec![promo(0.0, "", 15.0, "", "Max 0 köp")],
                ..base()
            },
        );
        assert_eq!(hit.approx_each, None);
        assert_eq!(hit.price, 15.0);
        assert_eq!(hit.old_price, Some(19.4));
        // An empty promotion comparePrice is falsy in JS, so the regular one is scaled: 12.93*15/19.4.
        assert_eq!(hit.unit_price, Some(10.0));
        assert_eq!(hit.deal, None);
    }

    #[test]
    fn a_limit_label_without_a_positive_number_gives_no_limit() {
        for label in ["", "Max 0 köp", "Begränsat"] {
            let hit = to_hit(
                ChainId::Willys,
                WILLYS,
                &AxfoodProduct { potential_promotions: vec![promo(3.0, "3 för 30", 10.0, "", label)], ..base() },
            );
            assert_eq!(hit.deal.unwrap().limit, None, "{label}");
        }
        let hit = to_hit(
            ChainId::Willys,
            WILLYS,
            &AxfoodProduct {
                potential_promotions: vec![promo(3.0, "3 för 30 PANT", 10.0, "", "Max 05 köp/hushåll")],
                ..base()
            },
        );
        let deal = hit.deal.unwrap();
        assert_eq!(deal.limit, Some(5));
        assert!(deal.plus_deposit);
    }

    #[test]
    fn encodes_like_encode_uri_component() {
        assert_eq!(encode_uri_component("mjölk & fil (3%)!*'~"), "mj%C3%B6lk%20%26%20fil%20(3%25)!*'~");
    }

    #[test]
    fn loads_the_store_lists() {
        assert!(!willys().stores().is_empty());
        assert!(hemkop().stores().iter().all(|s| s.chain == ChainId::Hemkop));
        assert!(willys().stores().iter().all(|s| s.chain == ChainId::Willys));
    }
}
