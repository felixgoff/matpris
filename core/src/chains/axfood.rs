//! TODO(port): src/lib/server/chains/axfood.ts
use async_trait::async_trait;

use super::ChainAdapter;
use crate::http;
use crate::types::{ChainId, Pricing, ProductHit, Store};

pub struct Axfood {
    pub chain: ChainId,
}
pub fn willys() -> Axfood {
    Axfood { chain: ChainId::Willys }
}
pub fn hemkop() -> Axfood {
    Axfood { chain: ChainId::Hemkop }
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
        Ok(vec![])
    }
    async fn search(&self, _query: &str, _store: Option<&Store>) -> http::Result<Vec<ProductHit>> {
        Ok(vec![])
    }
}
