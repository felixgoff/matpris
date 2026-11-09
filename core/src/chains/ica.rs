//! ICA's shop answers its search API to plain requests when a fresh `aws-waf-token`
//! cookie rides along. Who mints that cookie depends on where the backend runs, so it
//! comes from a `CookieJar` set at startup with `set_cookie_jar`: the Tauri app opens
//! the shop in a hidden webview and reads its cookies; the dev server runs
//! scripts/ica-warm.mjs (Playwright + Chrome). Without a jar, ICA is "busy".
//!
//! The WAF issues a short-lived aws-waf-token cookie (a few minutes of validity; the ICA
//! spider in matval kept its session alive the same way, reporting 33k requests with
//! plain HTTP after periodic token minting). This adapter reads the jar's session, and
//! when it is stale or the WAF challenges us (HTTP 202/403/405), it warms once and
//! retries. A visual challenge can only be cleared by a human (`HEADED=1 bun run
//! warm:ica` on the dev server) — nothing here solves or bypasses the WAF.
//!
//! Prices anchor to one store (Maxi ICA Stormarknad Gunnesbo, Lund), presented like
//! Lidl's list: chain pricing, a good approximation for nearby stores ranked with it.

use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use futures::future::{BoxFuture, FutureExt, Shared};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};

use super::ChainAdapter;
use crate::http;
use crate::types::{ChainId, Pricing, ProductHit, Store, Unit};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
}

/// What the warm step leaves behind (the same shape as scripts/.ica-profile/session.json).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IcaSession {
    /// Milliseconds since the Unix epoch.
    pub stored_at: u64,
    pub cookies: Vec<Cookie>,
}

/// Where ICA's session cookies come from.
#[async_trait]
pub trait CookieJar: Send + Sync {
    /// The latest session, if there is one.
    async fn read(&self) -> Option<IcaSession>;
    /// Mint a fresh session (open the shop the way a browser does). An error means ICA
    /// can't be searched right now.
    async fn warm(&self, why: Warm) -> http::Result<()>;
}

/// Why a session is being minted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warm {
    /// None yet, or older than STALE: the shop page may simply renew the token it has.
    Stale,
    /// The WAF just turned the current token down (a 202 challenge): only a new token
    /// helps, and if the shop won't mint one quietly, a person has to answer its check.
    Rejected,
}

static JAR: LazyLock<RwLock<Option<Arc<dyn CookieJar>>>> = LazyLock::new(|| RwLock::new(None));

/// Called once at startup by whatever hosts the backend.
pub fn set_cookie_jar(jar: Arc<dyn CookieJar>) {
    *JAR.write().unwrap() = Some(jar);
}

pub(crate) fn cookie_jar() -> Option<Arc<dyn CookieJar>> {
    JAR.read().unwrap().clone()
}

const STORE_ID: &str = "1088004";
const BASE: &str = "https://handlaprivatkund.ica.se";
/// Match the browser identification the session was minted with: a different UA on a
/// cookie-bearing request is exactly what WAFs look at.
const BROWSER_UA: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// WAF tokens outlive a session, but matval refreshed every ~4 min.
const STALE_MS: u64 = 3 * 60 * 1000;
/// Between warm attempts; real browser work should stay rare.
const WARM_GAP_MS: u64 = 60 * 1000;
const INTERVAL: Duration = Duration::from_millis(400);

static STORES: LazyLock<Vec<Store>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../data/ica-stores.json")).expect("core/data/ica-stores.json")
});

/// Product ids are strings in the shop's JSON; take numbers (and null) too instead of
/// dropping the product, as the TS passes them through.
fn id<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        Some(serde_json::Value::String(s)) => s,
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    })
}

/// The shop writes amounts as strings ("19.40"); accept numbers too.
fn amount<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().ok().filter(|x| x.is_finite()),
        _ => None,
    })
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Money {
    #[serde(deserialize_with = "amount")]
    pub amount: Option<f64>,
    pub currency: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UnitPrice {
    pub price: Option<Money>,
    pub unit_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Image {
    pub src: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct IcaProduct {
    #[serde(deserialize_with = "id")]
    pub product_id: String,
    #[serde(deserialize_with = "id")]
    pub retailer_product_id: String,
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub name: String,
    pub brand: Option<String>,
    pub pack_size_description: Option<String>,
    pub available: Option<bool>,
    pub price: Option<Money>,
    pub unit_price: Option<UnitPrice>,
    pub promo_price: Option<Money>,
    pub promo_unit_price: Option<UnitPrice>,
    pub deposit_price: Option<Money>,
    pub image: Option<Image>,
    pub category_path: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ProductGroup {
    /// Kept as raw JSON so one odd product is skipped instead of failing the search.
    /// (Group shapes elsewhere come wrapped as `products: [{ productId, product }]`;
    /// the search journey uses `decoratedProducts`.)
    decorated_products: Vec<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct IcaSearchResponse {
    product_groups: Vec<ProductGroup>,
}

/// "PER_LITRE_WITHOUT_DEPOSIT" | "PER_1KG" | "EACH" (also accepts the fop form).
fn unit_of(name: Option<&str>) -> Option<Unit> {
    let t = name.unwrap_or_default().to_lowercase();
    if t.contains("kg") {
        Some(Unit::Kg)
    } else if t.contains("litre") || t.contains("liter") {
        Some(Unit::L)
    } else if t.contains("each") {
        Some(Unit::St)
    } else {
        None
    }
}

/// "Läsk Orange 1,5l Fanta" → "läsk-orange-1-5l-fanta" (matches the real PDP URLs).
fn slug(name: &str) -> String {
    static SEP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\p{L}\p{N}]+").unwrap());
    // Punctuation and separators collapse to one hyphen; letters (incl. å, ä, ö) and digits stay.
    SEP.replace_all(&name.to_lowercase(), "-").trim_matches('-').to_string()
}

/// JavaScript's encodeURIComponent.
fn encode_component(s: &str) -> String {
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

pub fn to_hit(p: &IcaProduct) -> ProductHit {
    let regular = p.price.as_ref().and_then(|m| m.amount);
    let deal = p.promo_price.as_ref().and_then(|m| m.amount);
    // A full-price promo (deal ≥ regular) is not a deal.
    let is_deal = matches!((deal, regular), (Some(d), Some(r)) if d < r);
    let unit_source = if is_deal { p.promo_unit_price.as_ref() } else { p.unit_price.as_ref() };
    let unit_price = unit_source.and_then(|u| u.price.as_ref()).and_then(|m| m.amount);
    let unit = unit_of(unit_source.and_then(|u| u.unit_name.as_deref()));
    // depositPrice is charged on top and kept out of the price and the comparison price
    // (the unit is PER_LITRE_WITHOUT_DEPOSIT), as on the shop's own page.
    ProductHit {
        chain: ChainId::Ica,
        id: p.product_id.clone(),
        name: p.name.clone(),
        brand: p.brand.clone(),
        size: p.pack_size_description.clone(),
        price: if is_deal { deal.unwrap_or_default() } else { regular.unwrap_or(0.0) },
        unit_price,
        unit,
        categories: p.category_path.as_ref().map(|c| c.iter().take(2).cloned().collect()),
        per_kg: None,
        approx_each: None,
        old_price: if is_deal { regular } else { None },
        deal: None,
        image: p.image.as_ref().and_then(|i| i.src.clone()),
        url: format!("{BASE}/stores/{STORE_ID}/products/{}/{}", slug(&p.name), p.retailer_product_id),
        pricing: Pricing::Chain,
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

type PendingWarm = Shared<BoxFuture<'static, http::Result<()>>>;

#[derive(Default)]
struct WarmState {
    /// When the jar was last asked to warm (ms since the epoch).
    last: u64,
    /// The warm still running, if any.
    pending: Option<PendingWarm>,
    run: u64,
}

/// At most one warm per WARM_GAP from the session check, and never two at once.
#[derive(Default)]
struct WarmGate {
    state: Arc<Mutex<WarmState>>,
}

impl WarmGate {
    fn last_warm(&self) -> u64 {
        self.state.lock().unwrap().last
    }

    /// Warm the jar, or join the warm already running. The warm runs as its own task
    /// (like the TS promise): a search dropped mid-warm neither pauses it nor leaves it
    /// pending, and it stops being pending when it ends, success or not (TS `finally`).
    async fn warm(&self, jar: &Arc<dyn CookieJar>, why: Warm) -> http::Result<()> {
        let fut = {
            let mut s = self.state.lock().unwrap();
            match &s.pending {
                Some(p) => p.clone(),
                None => {
                    s.last = now_ms();
                    s.run += 1;
                    let run = s.run;
                    let jar = jar.clone();
                    let state = self.state.clone();
                    let task = tokio::spawn(async move {
                        let result = jar.warm(why).await;
                        let mut s = state.lock().unwrap();
                        if s.run == run {
                            s.pending = None;
                        }
                        result
                    });
                    let p = async move {
                        task.await.unwrap_or_else(|e| Err(http::Error::Other(format!("ICA warm failed: {e}"))))
                    }
                    .boxed()
                    .shared();
                    s.pending = Some(p.clone());
                    p
                }
            }
        };
        fut.await
    }

    /// The cookie header for the next request, warming first when the session is
    /// missing or stale — unless a warm went out within WARM_GAP: then a stale session
    /// is used as it is, and a missing one is "busy".
    async fn cookie_header(&self, jar: &Arc<dyn CookieJar>) -> http::Result<String> {
        let mut session = jar.read().await;
        if session.as_ref().is_none_or(|s| now_ms().saturating_sub(s.stored_at) > STALE_MS) {
            if self.last_warm() + WARM_GAP_MS > now_ms() {
                if session.is_none() {
                    return Err(http::Error::Throttled(
                        "ICA session warming in progress — try again shortly".into(),
                    ));
                }
            } else {
                self.warm(jar, Warm::Stale).await?;
                session = jar.read().await;
            }
        }
        let Some(session) = session else {
            return Err(http::Error::Throttled(
                "no ICA session — run `bun run warm:ica` once (or visit the shop in the browser profile)".into(),
            ));
        };
        Ok(session.cookies.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; "))
    }

    /// Fetch with the session's cookies. 202 is the WAF's challenge (403/405 too): one
    /// automatic re-mint is worth it — a stale session should not end a search, but a
    /// challenge that survives a fresh token should.
    async fn fetch<T, F, Fut>(&self, jar: &Arc<dyn CookieJar>, fetch: F) -> http::Result<T>
    where
        F: Fn(String) -> Fut,
        Fut: Future<Output = http::Result<T>>,
    {
        let cookie = self.cookie_header(jar).await?;
        match fetch(cookie).await {
            Err(http::Error::Status { status: 202 | 403 | 405, .. }) => {
                self.warm(jar, Warm::Rejected).await?;
                // As in the TS, anything failing after the re-mint (the session read
                // included) means the challenge survived it.
                let retry = match self.cookie_header(jar).await {
                    Ok(cookie) => fetch(cookie).await,
                    Err(e) => Err(e),
                };
                retry.map_err(|_| {
                    http::Error::Throttled(
                        "ICA challenged us again after re-minting — the IP needs a human session".into(),
                    )
                })
            }
            other => other,
        }
    }
}

static GATE: LazyLock<WarmGate> = LazyLock::new(WarmGate::default);

fn search_url(query: &str) -> String {
    format!(
        "{BASE}/stores/{STORE_ID}/api/webproductpagews/v6/product-pages/search?includeAdditionalPageInfo=true\
         &maxPageSize=48&maxProductsToDecorate=48&q={}&tag=web",
        encode_component(query)
    )
}

fn hits_of(data: IcaSearchResponse) -> Vec<ProductHit> {
    data.product_groups
        .into_iter()
        .flat_map(|g| g.decorated_products)
        .filter_map(|v| serde_json::from_value::<IcaProduct>(v).ok())
        .filter(|p| p.available != Some(false))
        .map(|p| to_hit(&p))
        .collect()
}

pub struct Ica;

#[async_trait]
impl ChainAdapter for Ica {
    fn chain(&self) -> ChainId {
        ChainId::Ica
    }
    fn pricing(&self) -> Pricing {
        Pricing::Chain
    }
    async fn list_stores(&self) -> http::Result<Vec<Store>> {
        Ok(STORES.clone())
    }
    async fn search(&self, query: &str, _store: Option<&Store>) -> http::Result<Vec<ProductHit>> {
        let Some(jar) = cookie_jar() else {
            return Err(http::Error::Throttled(
                "ICA needs a browser session to search, and this build has no way to mint one".into(),
            ));
        };
        let url = search_url(query);
        let data: IcaSearchResponse = GATE
            .fetch(&jar, |cookie| {
                let opts =
                    http::FetchOptions::every(INTERVAL).header("user-agent", BROWSER_UA).header("cookie", cookie);
                let url = url.clone();
                async move { http::fetch_json(&url, &opts).await }
            })
            .await?;
        Ok(hits_of(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Shapes captured verbatim from the shop's own search + product APIs (2026-10-29).
    fn fanta_json() -> Value {
        json!({
            "productId": "abc2c8a2-9f59-4c77-9e8f-c8ea08cee898",
            "retailerProductId": "2294810",
            "type": "REGULAR",
            "name": "Läsk Orange 1,5l Fanta",
            "brand": "Fanta",
            "packSizeDescription": "1.5L",
            "price": { "amount": "19.40", "currency": "SEK" },
            "unitPrice": { "price": { "amount": "12.93", "currency": "SEK" }, "unitName": "PER_LITRE_WITHOUT_DEPOSIT" },
            "depositPrice": { "amount": "3.00", "currency": "SEK" },
            "available": true
        })
    }

    fn patched(patch: Value) -> Value {
        let mut base = fanta_json();
        for (k, v) in patch.as_object().unwrap() {
            base[k] = v.clone();
        }
        base
    }

    fn with(patch: Value) -> IcaProduct {
        serde_json::from_value(patched(patch)).unwrap()
    }

    fn fanta() -> IcaProduct {
        serde_json::from_value(fanta_json()).unwrap()
    }

    fn sale() -> IcaProduct {
        with(json!({
            "productId": "c5e3ebc4-2054-42ca-a15a-e67d648703d9",
            "retailerProductId": "1237504",
            "name": "Satsumas ca 100g Klass 1 ICA",
            "packSizeDescription": "96g - 1000000g",
            "price": { "amount": "2.99", "currency": "SEK" },
            "unitPrice": { "price": { "amount": "29.90", "currency": "SEK" }, "unitName": "PER_1KG" },
            "promoPrice": { "amount": "1.50", "currency": "SEK" },
            "promoUnitPrice": { "price": { "amount": "15.00", "currency": "SEK" }, "unitName": "PER_1KG" }
        }))
    }

    #[test]
    fn maps_a_regular_product() {
        let hit = to_hit(&fanta());
        assert_eq!(hit.chain, ChainId::Ica);
        assert_eq!(hit.price, 19.4);
        assert_eq!(hit.unit_price, Some(12.93));
        assert_eq!(hit.unit, Some(Unit::L));
        assert_eq!(hit.brand.as_deref(), Some("Fanta"));
        assert_eq!(hit.pricing, Pricing::Chain);
        assert_eq!(hit.old_price, None);
        assert_eq!(hit.deal, None);
        assert!(hit.url.contains("/products/läsk-orange-1-5l-fanta/2294810"), "{}", hit.url);
    }

    #[test]
    fn maps_a_promotion_price_as_the_current_price_with_the_regular_price_as_old_price() {
        let hit = to_hit(&sale());
        assert_eq!(hit.price, 1.5);
        assert_eq!(hit.old_price, Some(2.99));
        assert_eq!(hit.unit_price, Some(15.0));
        assert_eq!(hit.unit, Some(Unit::Kg));
    }

    #[test]
    fn keeps_a_full_price_promo_as_the_regular_price() {
        let hit = to_hit(&with(json!({ "promoPrice": { "amount": "19.40" } })));
        assert_eq!(hit.price, 19.4);
        assert_eq!(hit.unit_price, Some(12.93));
        assert_eq!(hit.old_price, None);
    }

    #[test]
    fn reads_the_piece_unit_each_for_non_weighed_products() {
        let p = with(json!({ "unitPrice": { "price": { "amount": "2.03" }, "unitName": "EACH" } }));
        assert_eq!(to_hit(&p).unit, Some(Unit::St));
    }

    #[test]
    fn numeric_amounts_and_a_missing_price() {
        assert_eq!(to_hit(&with(json!({ "price": { "amount": 12.5 } }))).price, 12.5);
        // No regular price: the promo is not a deal and the price is 0, as in the TS.
        let hit = to_hit(&with(json!({ "price": null, "promoPrice": { "amount": "5" } })));
        assert_eq!(hit.price, 0.0);
        assert_eq!(hit.old_price, None);
    }

    #[test]
    fn keeps_the_first_two_categories_and_the_image() {
        let hit = to_hit(&with(json!({ "categoryPath": ["Dryck", "Läsk", "Apelsin"], "image": { "src": "https://x/y.jpg" } })));
        assert_eq!(hit.categories, Some(vec!["Dryck".to_string(), "Läsk".to_string()]));
        assert_eq!(hit.image.as_deref(), Some("https://x/y.jpg"));
    }

    #[test]
    fn slugs_keep_unicode_letters_and_digits() {
        assert_eq!(slug("Läsk Orange 1,5l Fanta"), "läsk-orange-1-5l-fanta");
        assert_eq!(slug("  Kaffe -- Bryggmalet 450g! "), "kaffe-bryggmalet-450g");
        assert_eq!(slug("Öl & Ägg"), "öl-ägg");
    }

    #[test]
    fn encodes_the_query_like_encode_uri_component() {
        assert_eq!(encode_component("bär & sylt"), "b%C3%A4r%20%26%20sylt");
        assert!(search_url("kaffe").ends_with("&q=kaffe&tag=web"));
        assert_eq!(
            search_url("kaffe"),
            "https://handlaprivatkund.ica.se/stores/1088004/api/webproductpagews/v6/product-pages/search\
             ?includeAdditionalPageInfo=true&maxPageSize=48&maxProductsToDecorate=48&q=kaffe&tag=web"
        );
    }

    #[test]
    fn skips_unavailable_and_malformed_products() {
        let data: IcaSearchResponse = serde_json::from_value(json!({
            "productGroups": [
                { "decoratedProducts": [fanta_json(), { "name": 5 }, patched(json!({ "available": false }))] },
                { "products": [] }
            ]
        }))
        .unwrap();
        let hits = hits_of(data);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "abc2c8a2-9f59-4c77-9e8f-c8ea08cee898");
    }

    #[test]
    fn the_store_list_loads() {
        assert!(STORES.len() > 300);
        assert!(STORES.iter().all(|s| s.chain == ChainId::Ica));
    }

    /// A jar whose warm takes a moment and, if `mint`, leaves a fresh session behind.
    struct FakeJar {
        session: Mutex<Option<IcaSession>>,
        warms: AtomicUsize,
        mint: bool,
    }

    impl FakeJar {
        fn new(session: Option<IcaSession>, mint: bool) -> Arc<FakeJar> {
            Arc::new(FakeJar { session: Mutex::new(session), warms: AtomicUsize::new(0), mint })
        }
        fn warms(&self) -> usize {
            self.warms.load(Ordering::SeqCst)
        }
    }

    fn session(age_ms: u64, token: &str) -> IcaSession {
        IcaSession {
            stored_at: now_ms() - age_ms,
            cookies: vec![
                Cookie { name: "aws-waf-token".into(), value: token.into() },
                Cookie { name: "x".into(), value: "1".into() },
            ],
        }
    }

    #[async_trait]
    impl CookieJar for FakeJar {
        async fn read(&self) -> Option<IcaSession> {
            self.session.lock().unwrap().clone()
        }
        async fn warm(&self, _why: Warm) -> http::Result<()> {
            let n = self.warms.fetch_add(1, Ordering::SeqCst) + 1;
            tokio::time::sleep(Duration::from_millis(20)).await;
            if self.mint {
                *self.session.lock().unwrap() = Some(session(0, &format!("fresh{n}")));
            }
            Ok(())
        }
    }

    fn dyn_jar(j: &Arc<FakeJar>) -> Arc<dyn CookieJar> {
        j.clone()
    }

    #[tokio::test]
    async fn a_fresh_session_is_used_without_warming() {
        let jar = FakeJar::new(Some(session(1000, "t")), true);
        let gate = WarmGate::default();
        assert_eq!(gate.cookie_header(&dyn_jar(&jar)).await.unwrap(), "aws-waf-token=t; x=1");
        assert_eq!(jar.warms(), 0);
    }

    #[tokio::test]
    async fn a_stale_session_warms_once_per_gap() {
        let jar = FakeJar::new(Some(session(STALE_MS + 1000, "old")), false);
        let gate = WarmGate::default();
        // Warms, but the jar left the old session: it is used as it is.
        assert_eq!(gate.cookie_header(&dyn_jar(&jar)).await.unwrap(), "aws-waf-token=old; x=1");
        assert_eq!(gate.cookie_header(&dyn_jar(&jar)).await.unwrap(), "aws-waf-token=old; x=1");
        assert_eq!(jar.warms(), 1);
    }

    #[tokio::test]
    async fn a_missing_session_warms_then_reads() {
        let jar = FakeJar::new(None, true);
        let gate = WarmGate::default();
        assert_eq!(gate.cookie_header(&dyn_jar(&jar)).await.unwrap(), "aws-waf-token=fresh1; x=1");
        // Within the gap with nothing to read: busy, not another warm.
        *jar.session.lock().unwrap() = None;
        let e = gate.cookie_header(&dyn_jar(&jar)).await.unwrap_err();
        assert!(e.is_throttled() && e.to_string().contains("warming in progress"), "{e}");
        assert_eq!(jar.warms(), 1);
    }

    #[tokio::test]
    async fn a_warm_that_leaves_no_session_is_busy() {
        let jar = FakeJar::new(None, false);
        let e = WarmGate::default().cookie_header(&dyn_jar(&jar)).await.unwrap_err();
        assert!(e.is_throttled() && e.to_string().contains("no ICA session"), "{e}");
    }

    #[tokio::test]
    async fn concurrent_warms_share_one_run() {
        let jar = FakeJar::new(None, true);
        let gate = WarmGate::default();
        let j = dyn_jar(&jar);
        let (a, b) = tokio::join!(gate.warm(&j, Warm::Stale), gate.warm(&j, Warm::Stale));
        a.unwrap();
        b.unwrap();
        assert_eq!(jar.warms(), 1);
        // Once it's done, the next warm is a new run.
        gate.warm(&j, Warm::Stale).await.unwrap();
        assert_eq!(jar.warms(), 2);
    }

    #[tokio::test]
    async fn a_challenge_re_mints_once_and_retries() {
        let jar = FakeJar::new(Some(session(0, "t")), true);
        let gate = WarmGate::default();
        let seen = Mutex::new(Vec::new());
        let out = gate
            .fetch(&dyn_jar(&jar), |cookie| {
                let first = {
                    let mut s = seen.lock().unwrap();
                    s.push(cookie);
                    s.len() == 1
                };
                async move {
                    if first {
                        Err(http::Error::Status { host: "handlaprivatkund.ica.se".into(), status: 202 })
                    } else {
                        Ok(7)
                    }
                }
            })
            .await
            .unwrap();
        assert_eq!(out, 7);
        assert_eq!(*seen.lock().unwrap(), vec!["aws-waf-token=t; x=1", "aws-waf-token=fresh1; x=1"]);
        assert_eq!(jar.warms(), 1);
    }

    #[tokio::test]
    async fn a_challenge_that_survives_a_re_mint_is_throttled() {
        let jar = FakeJar::new(Some(session(0, "t")), true);
        let calls = AtomicUsize::new(0);
        let e = WarmGate::default()
            .fetch(&dyn_jar(&jar), |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Err::<(), _>(http::Error::Status { host: "h".into(), status: 403 }) }
            })
            .await
            .unwrap_err();
        assert!(matches!(&e, http::Error::Throttled(m) if m.contains("challenged us again")), "{e}");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn other_errors_are_not_retried() {
        let jar = FakeJar::new(Some(session(0, "t")), true);
        let calls = AtomicUsize::new(0);
        let e = WarmGate::default()
            .fetch(&dyn_jar(&jar), |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Err::<(), _>(http::Error::Status { host: "h".into(), status: 500 }) }
            })
            .await
            .unwrap_err();
        assert!(matches!(e, http::Error::Status { status: 500, .. }));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(jar.warms(), 0);
    }

    #[test]
    fn numeric_ids_keep_the_product() {
        let hit = to_hit(&with(json!({ "productId": 42, "retailerProductId": 2294810 })));
        assert_eq!(hit.id, "42");
        assert!(hit.url.ends_with("/läsk-orange-1-5l-fanta/2294810"), "{}", hit.url);
    }

    /// A jar whose session is gone after the first read (e.g. deleted by the re-mint).
    struct VanishingJar {
        reads: AtomicUsize,
    }

    #[async_trait]
    impl CookieJar for VanishingJar {
        async fn read(&self) -> Option<IcaSession> {
            (self.reads.fetch_add(1, Ordering::SeqCst) == 0).then(|| session(0, "t"))
        }
        async fn warm(&self, _why: Warm) -> http::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_failed_session_read_after_the_re_mint_is_the_challenge_message() {
        let jar: Arc<dyn CookieJar> = Arc::new(VanishingJar { reads: AtomicUsize::new(0) });
        let e = WarmGate::default()
            .fetch(&jar, |_| async { Err::<(), _>(http::Error::Status { host: "h".into(), status: 202 }) })
            .await
            .unwrap_err();
        assert!(matches!(&e, http::Error::Throttled(m) if m.contains("challenged us again")), "{e}");
    }

    #[tokio::test]
    async fn a_dropped_search_does_not_strand_the_warm() {
        let jar = FakeJar::new(None, true);
        let gate = WarmGate::default();
        let j = dyn_jar(&jar);
        // The search gives up while the warm (20 ms) is still running.
        assert!(tokio::time::timeout(Duration::from_millis(2), gate.warm(&j, Warm::Stale)).await.is_err());
        tokio::time::sleep(Duration::from_millis(80)).await;
        // It finished on its own and is no longer pending: the next warm is a new run.
        assert!(jar.session.lock().unwrap().is_some());
        assert!(gate.state.lock().unwrap().pending.is_none());
        gate.warm(&j, Warm::Stale).await.unwrap();
        assert_eq!(jar.warms(), 2);
    }

    #[tokio::test]
    async fn no_jar_is_busy() {
        // Nothing in the tests sets the global jar.
        let e = Ica.search("kaffe", None).await.unwrap_err();
        assert!(e.is_throttled(), "{e}");
    }
}
