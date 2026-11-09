//! The chains. To add one: implement `ChainAdapter` in its own file, register it in
//! `adapters()`, and mark it online in `ChainId::info`.

use std::sync::LazyLock;

use async_trait::async_trait;

use crate::http;
use crate::types::{ChainId, Pricing, ProductHit, Store};

pub mod axfood;
pub mod citygross;
pub mod coop;
pub mod ica;
pub mod lidl;

#[async_trait]
pub trait ChainAdapter: Send + Sync {
    fn chain(&self) -> ChainId;
    /// `Chain`: one price list for the whole chain, so one search covers every store.
    /// `Store`: prices differ per store, so `search` is called once per store in the circle.
    fn pricing(&self) -> Pricing;
    async fn list_stores(&self) -> http::Result<Vec<Store>>;
    async fn search(&self, query: &str, store: Option<&Store>) -> http::Result<Vec<ProductHit>>;
}

static ADAPTERS: LazyLock<Vec<Box<dyn ChainAdapter>>> = LazyLock::new(|| {
    vec![
        Box::new(axfood::willys()),
        Box::new(axfood::hemkop()),
        Box::new(lidl::Lidl),
        Box::new(citygross::CityGross),
        Box::new(coop::Coop),
        Box::new(ica::Ica),
    ]
});

/// Every chain with online prices, in the order the page lists them.
pub fn adapters() -> &'static [Box<dyn ChainAdapter>] {
    &ADAPTERS
}
