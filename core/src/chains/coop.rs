//! Coop Sweden exposes the APIs its own web shop calls via Azure APIM:
//!   POST https://external.api.coop.se/personalization/search/global?api-version=v2&…
//!   GET  https://proxy.api.coop.se/external/store/stores/map?conceptIds=12,6,95&…&api-version=v2
//! Both take an Ocp-Apim-Subscription-Key, and coop.se hands those keys to every visitor
//! in its page source (they ship in the site's own config JSON). We read them from there,
//! like a browser does, and cache them for a day. Like the other chains' online shops the
//! prices are one national list (store=251300), so they are treated as chain-wide.
//! Ported from src/lib/server/chains/coop.ts.

use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};

use super::ChainAdapter;
use crate::cache::TtlCache;
use crate::http::{self, FetchOptions};
use crate::normalize::{round2, to_unit};
use crate::types::{ChainId, Deal, Pricing, ProductHit, Store};

const SEARCH: &str = "https://external.api.coop.se/personalization/search/global";
const STORES: &str = "https://proxy.api.coop.se/external/store/stores/map";
const PAGE: &str = "https://www.coop.se/"; // page source carries the subscription keys

// The site spaces its own API traffic loosely; match that.
const INTERVAL: Duration = Duration::from_secs(2);
/// Key for the adapter-wide gap: the search and the store list live on different hosts,
/// but the 2 s apply to Coop's API traffic as a whole.
const GAP_KEY: &str = "coop-api (adapter-wide)";
/// The TS queued without limit; give up (shown as "busy") after this instead.
const GAP_MAX_WAIT: Duration = Duration::from_secs(30);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

// Shapes verified against live responses (2026-10-29).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CoopPromotion {
    #[serde(rename = "type")]
    pub type_: Option<String>,
    /// Total bundle price for MULTI_BUY promos, item price for FIXED_PRICE.
    pub price_data: Option<CoopPrice>,
    /// How many items a MULTI_BUY price covers.
    pub number_of_product_required: Option<f64>,
    /// Coop MedMera member price: needs a logged-in account, skipped like City Gross's.
    pub med_mera_required: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CoopPrice {
    pub b2c_price: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CoopUnit {
    pub unit: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CoopProduct {
    pub id: String,
    pub name: String,
    pub manufacturer_name: Option<String>,
    pub package_size_information: Option<String>,
    pub sales_price_data: Option<CoopPrice>,
    pub comparative_price_data: Option<CoopPrice>,
    pub comparative_price_unit: Option<CoopUnit>,
    pub online_promotions: Option<Vec<CoopPromotion>>,
    pub image_url: Option<String>,
    pub available_online: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CoopStore {
    store_id: Option<serde_json::Number>,
    name: Option<String>,
    address: Option<String>,
    postal_code: Option<String>,
    city: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    url: Option<String>,
}

#[derive(Debug, Clone)]
struct Keys {
    personalization: String,
    store: String,
}

/// Minimum time between Coop API calls, shared across the adapter's requests.
async fn throttled() -> http::Result<()> {
    http::take_slot(GAP_KEY, INTERVAL, GAP_MAX_WAIT, false).await
}

fn api_options(timeout: Duration) -> FetchOptions {
    FetchOptions::every(INTERVAL).max_wait(GAP_MAX_WAIT).timeout(timeout)
}

static KEY_PATTERNS: LazyLock<(Regex, Regex)> = LazyLock::new(|| {
    let re = |name: &str| Regex::new(&format!(r#""{name}"\s*:\s*"([0-9a-fA-F]{{32}})""#)).unwrap();
    (re("personalizationApiSubscriptionKey"), re("storeApiSubscriptionKey"))
});

/// The two subscription keys from coop.se's page source.
fn keys_from_page(html: &str) -> Option<Keys> {
    let get = |re: &Regex| re.captures(html).map(|c| c[1].to_string());
    Some(Keys { personalization: get(&KEY_PATTERNS.0)?, store: get(&KEY_PATTERNS.1)? })
}

// The keys are printed in coop.se's own page config, refreshed rarely.
static KEYS: LazyLock<TtlCache<Keys>> = LazyLock::new(|| TtlCache::new(DAY));

async fn subscription_keys() -> http::Result<Keys> {
    KEYS.get("keys", || async {
        let html = http::fetch_text(PAGE, &FetchOptions::every(INTERVAL).timeout(Duration::from_secs(15))).await?;
        keys_from_page(&html).ok_or_else(|| http::Error::Other("no Coop subscription keys in page source".into()))
    })
    .await
}

fn positive(x: Option<f64>) -> Option<f64> {
    // JS truthiness of a number: 0 and NaN are falsy.
    x.filter(|v| *v != 0.0 && !v.is_nan())
}

pub fn to_hit(p: &CoopProduct, chain: ChainId) -> Option<ProductHit> {
    let price = positive(p.sales_price_data.as_ref().and_then(|d| d.b2c_price))?;
    let unit = p.comparative_price_unit.as_ref().and_then(|u| u.unit.as_deref()).and_then(to_unit);
    let unit_price = positive(p.comparative_price_data.as_ref().and_then(|d| d.b2c_price));
    let (unit_price, unit) = match (unit, unit_price) {
        (Some(u), Some(up)) => (Some(up), Some(u)),
        _ => (None, None),
    };
    let hit = ProductHit {
        chain,
        id: p.id.clone(),
        name: p.name.clone(),
        brand: p.manufacturer_name.clone(),
        size: p.package_size_information.clone(),
        price,
        unit_price,
        unit,
        // Coop's names are self-descriptive ("Fanta Orange"), so — like the Axfood chains —
        // no category path goes to the relevance tiers: its shelf names ("Läsk") would
        // wrongly read the query word as a flavour of another product.
        categories: None,
        per_kg: None,
        approx_each: None,
        old_price: None,
        deal: None,
        // cloudinary TIFF originals: request what the site itself serves (webp, compressed)
        image: p.image_url.as_deref().and_then(|u| u.split("/upload/").nth(1)).map(|rest| {
            format!("https://res.cloudinary.com/coopsverige/image/upload/f_auto,q_80/{rest}")
        }),
        url: format!("https://www.coop.se/handla/varor/{}-{}", slug(&p.name), p.id),
        pricing: Pricing::Chain,
    };
    // Deals: skip MedMera member prices (like City Gross), take the cheapest remaining.
    let mut best = hit.clone();
    for promo in p.online_promotions.iter().flatten() {
        if promo.med_mera_required == Some(true) {
            continue;
        }
        let Some(promo_price) = positive(promo.price_data.as_ref().and_then(|d| d.b2c_price)) else {
            continue;
        };
        let quantity = if promo.type_.as_deref() == Some("MULTI_BUY_FIXED_PRICE") {
            promo.number_of_product_required.unwrap_or(1.0)
        } else {
            1.0
        };
        if quantity > 1.0 {
            let each = round2(promo_price / quantity);
            if each >= best.price {
                continue;
            }
            best.deal = Some(Deal {
                quantity: quantity as u32,
                each,
                total: promo_price,
                unit_price: None,
                mix_and_match: false,
                plus_deposit: false,
                limit: None,
            });
        } else if promo_price < best.price {
            // Same formula as the TS: the current comparison price scaled by the sale
            // against the regular price.
            let par = match (hit.unit_price, best.unit, best.unit_price) {
                (Some(_), Some(_), Some(bup)) => Some(bup * promo_price / hit.price),
                _ => None,
            };
            best.old_price = Some(best.price);
            best.price = promo_price;
            best.unit = par.and(best.unit);
            best.unit_price = par.map(round2);
        }
    }
    Some(best)
}

static NON_SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-z0-9]+").unwrap());

fn slug(name: &str) -> String {
    let s = NON_SLUG.replace_all(&name.to_lowercase(), "-").into_owned();
    // The runs are collapsed, so at most one dash sits at either end.
    let s = s.strip_prefix('-').unwrap_or(&s);
    s.strip_suffix('-').unwrap_or(s).to_string()
}

fn to_store(s: CoopStore) -> Option<Store> {
    let lat = positive(s.latitude)?;
    let name = s.name.filter(|n| !n.is_empty())?;
    let lng = s.longitude?;
    let id = s.store_id?.to_string();
    let place = [s.postal_code, s.city].into_iter().flatten().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
    let address = [s.address.unwrap_or_default(), place].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(", ");
    Some(Store {
        id,
        chain: ChainId::Coop,
        name,
        lat,
        lng,
        address,
        url: s.url.filter(|u| !u.is_empty()).map(|u| format!("https://www.coop.se{u}")),
    })
}

/// Each element on its own, so one odd entry is skipped instead of failing the list.
fn each_of<T: for<'de> Deserialize<'de>>(items: Vec<Value>) -> impl Iterator<Item = T> {
    items.into_iter().filter_map(|v| serde_json::from_value(v).ok())
}

static STORE_LIST: LazyLock<TtlCache<Vec<Store>>> = LazyLock::new(|| TtlCache::new(DAY));

fn localize(url: &str) -> String {
    format!("{url}?api-version=v2&store=251300&groups=CUSTOMER_PRIVATE&device=desktop&direct=false")
}

pub struct Coop;

#[async_trait]
impl ChainAdapter for Coop {
    fn chain(&self) -> ChainId {
        ChainId::Coop
    }
    fn pricing(&self) -> Pricing {
        Pricing::Chain
    }
    async fn list_stores(&self) -> http::Result<Vec<Store>> {
        let Keys { store, .. } = subscription_keys().await?;
        STORE_LIST
            .get("all", || async {
                throttled().await?;
                let url = format!("{STORES}?conceptIds=12,6,95&invertFilter=true&api-version=v2");
                let opts = api_options(Duration::from_secs(15)).header("ocp-apim-subscription-key", store);
                let raw: Vec<Value> = http::fetch_json(&url, &opts).await?;
                Ok(each_of::<CoopStore>(raw).filter_map(to_store).collect())
            })
            .await
    }
    async fn search(&self, query: &str, _store: Option<&Store>) -> http::Result<Vec<ProductHit>> {
        let Keys { personalization, .. } = subscription_keys().await?;
        throttled().await?;
        let body = json!({
            "query": query,
            "resultsOptions": { "skip": 0, "take": 40, "sortBy": [], "facets": [] },
            "storesResultsOptions": { "skip": 0, "take": 1, "sortBy": [], "facets": [] }
        });
        let opts = api_options(Duration::from_secs(8))
            .header("ocp-apim-subscription-key", personalization)
            .header("origin", "https://www.coop.se");
        let data: Value = http::post_json(&localize(SEARCH), &body, &opts).await?;
        let items = match data.pointer("/results/items") {
            Some(Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        Ok(each_of::<CoopProduct>(items)
            .filter(|p| p.available_online != Some(false))
            .filter_map(|p| to_hit(&p, ChainId::Coop))
            .filter(|h| h.price > 0.0)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Unit;

    fn price(x: f64) -> Option<CoopPrice> {
        Some(CoopPrice { b2c_price: Some(x) })
    }

    // Shapes taken from real external.api.coop.se responses (2026-10-29).
    fn base() -> CoopProduct {
        CoopProduct {
            id: "5000112667660".into(),
            name: "Fanta Orange 6-pack".into(),
            manufacturer_name: Some("Fanta".into()),
            package_size_information: Some("6x330ml".into()),
            sales_price_data: price(45.38),
            comparative_price_data: price(22.92),
            comparative_price_unit: Some(CoopUnit { unit: Some("liter".into()), text: Some("kr/lit".into()) }),
            available_online: Some(true),
            ..Default::default()
        }
    }

    fn promo(type_: &str, required: Option<f64>, b2c: f64, med_mera: bool) -> CoopPromotion {
        CoopPromotion {
            type_: Some(type_.into()),
            price_data: price(b2c),
            number_of_product_required: required,
            med_mera_required: Some(med_mera),
        }
    }

    #[test]
    fn maps_a_regular_price() {
        let hit = to_hit(&base(), ChainId::Coop).unwrap();
        assert_eq!((hit.price, hit.unit_price, hit.unit), (45.38, Some(22.92), Some(Unit::L)));
        // names are self-descriptive; no categories go to the relevance tiers
        assert_eq!(hit.categories, None);
        assert_eq!(hit.deal, None);
        assert_eq!(hit.old_price, None);
        assert_eq!(hit.url, "https://www.coop.se/handla/varor/fanta-orange-6-pack-5000112667660");
        assert_eq!(hit.pricing, Pricing::Chain);
    }

    #[test]
    fn maps_a_multi_buy_2_for_29_deal() {
        let hit = to_hit(
            &CoopProduct {
                id: "5000112689792".into(),
                name: "Coca-Cola Zero 1,5l".into(),
                sales_price_data: price(22.9),
                comparative_price_data: price(15.27),
                package_size_information: Some("1,5l".into()),
                online_promotions: Some(vec![promo("MULTI_BUY_FIXED_PRICE", Some(2.0), 29.0, false)]),
                ..base()
            },
            ChainId::Coop,
        )
        .unwrap();
        assert_eq!(hit.price, 22.9); // buying one still costs the regular price
        assert_eq!(
            hit.deal,
            Some(Deal {
                quantity: 2,
                each: 14.5,
                total: 29.0,
                unit_price: None,
                mix_and_match: false,
                plus_deposit: false,
                limit: None
            })
        );
    }

    #[test]
    fn maps_a_single_item_sale_as_the_current_price() {
        let hit = to_hit(
            &CoopProduct {
                id: "345070".into(),
                name: "Chokladmjölk Pucko Original".into(),
                package_size_information: Some("1l".into()),
                sales_price_data: price(20.93),
                comparative_price_data: price(20.93),
                online_promotions: Some(vec![promo("FIXED_PRICE", None, 17.9, false)]),
                ..base()
            },
            ChainId::Coop,
        )
        .unwrap();
        assert_eq!((hit.price, hit.old_price, hit.unit_price), (17.9, Some(20.93), Some(17.9)));
        assert_eq!(hit.deal, None);
        // Non-ASCII letters become dashes, like the TS's /[^a-z0-9]+/.
        assert_eq!(hit.url, "https://www.coop.se/handla/varor/chokladmj-lk-pucko-original-345070");
    }

    #[test]
    fn ignores_medmera_member_prices() {
        let hit = to_hit(
            &CoopProduct {
                online_promotions: Some(vec![promo("MULTI_BUY_FIXED_PRICE", Some(2.0), 29.0, true)]),
                ..base()
            },
            ChainId::Coop,
        )
        .unwrap();
        assert_eq!(hit.deal, None);
        assert_eq!(hit.old_price, None);
    }

    #[test]
    fn deserializes_nulls_and_skips_unpriced_products() {
        let p: CoopProduct = serde_json::from_value(json!({
            "id": "1", "name": "X", "manufacturerName": null, "packageSizeInformation": null,
            "imageUrl": "https://res.cloudinary.com/coopsverige/image/upload/v1/abc.tiff",
            "salesPriceData": { "b2cPrice": 10 }, "comparativePriceUnit": { "unit": "kilogram" }
        }))
        .unwrap();
        let hit = to_hit(&p, ChainId::Coop).unwrap();
        assert_eq!((hit.brand, hit.unit, hit.unit_price), (None, None, None));
        assert_eq!(hit.image.as_deref(), Some("https://res.cloudinary.com/coopsverige/image/upload/f_auto,q_80/v1/abc.tiff"));
        let unpriced: CoopProduct = serde_json::from_value(json!({ "id": "2", "name": "Y" })).unwrap();
        assert!(to_hit(&unpriced, ChainId::Coop).is_none());
    }

    #[test]
    fn reads_the_keys_from_the_page_source() {
        let html = r#"{"personalizationApiSubscriptionKey": "0123456789abcdef0123456789ABCDEF","storeApiSubscriptionKey":"ffffffffffffffffffffffffffffffff"}"#;
        let k = keys_from_page(html).unwrap();
        assert_eq!(k.personalization, "0123456789abcdef0123456789ABCDEF");
        assert_eq!(k.store, "ffffffffffffffffffffffffffffffff");
        assert!(keys_from_page(r#""personalizationApiSubscriptionKey":"abc""#).is_none());
    }

    #[test]
    fn maps_stores_and_skips_incomplete_ones() {
        let raw = vec![
            json!({ "storeId": 251300, "name": "Coop Lund", "address": "Storgatan 1", "postalCode": "222 22",
                    "city": "Lund", "latitude": 55.7, "longitude": 13.19, "url": "/butiker/coop-lund" }),
            json!({ "storeId": 2, "name": "No position", "latitude": 0, "longitude": 13.0 }),
            json!({ "storeId": 3, "name": "Only city", "address": "", "city": "Malmö", "latitude": 55.6, "longitude": 13.0 }),
            json!({ "storeId": "bad", "name": 5 }),
        ];
        let stores: Vec<Store> = each_of::<CoopStore>(raw).filter_map(to_store).collect();
        assert_eq!(stores.len(), 2);
        assert_eq!(stores[0].id, "251300");
        assert_eq!(stores[0].address, "Storgatan 1, 222 22 Lund");
        assert_eq!(stores[0].url.as_deref(), Some("https://www.coop.se/butiker/coop-lund"));
        assert_eq!(stores[1].address, "Malmö");
        assert_eq!(stores[1].url, None);
    }
}
