//! TODO(port): src/lib/server/chains/coop.ts
use async_trait::async_trait;

use super::ChainAdapter;
use crate::http;
use crate::types::{ChainId, Pricing, ProductHit, Store};

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
        Ok(vec![])
    }
    async fn search(&self, _query: &str, _store: Option<&Store>) -> http::Result<Vec<ProductHit>> {
        Ok(vec![])
    }
}
