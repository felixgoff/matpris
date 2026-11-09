//! TODO(port): src/lib/server/chains/citygross.ts
use async_trait::async_trait;

use super::ChainAdapter;
use crate::http;
use crate::types::{ChainId, Pricing, ProductHit, Store};

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
        Ok(vec![])
    }
    async fn search(&self, _query: &str, _store: Option<&Store>) -> http::Result<Vec<ProductHit>> {
        Ok(vec![])
    }
}
