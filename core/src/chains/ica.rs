//! TODO(port): src/lib/server/chains/ica.ts
//!
//! ICA's shop answers its search API to plain requests when a fresh `aws-waf-token`
//! cookie rides along. Who mints that cookie depends on where the backend runs, so it
//! comes from a `CookieJar` set at startup with `set_cookie_jar`: the Tauri app opens
//! the shop in a hidden webview and reads its cookies; the dev server runs
//! scripts/ica-warm.mjs (Playwright + Chrome). Without a jar, ICA is "busy".

use std::sync::{Arc, LazyLock, RwLock};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::ChainAdapter;
use crate::http;
use crate::types::{ChainId, Pricing, ProductHit, Store};

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
    async fn warm(&self) -> http::Result<()>;
}

static JAR: LazyLock<RwLock<Option<Arc<dyn CookieJar>>>> = LazyLock::new(|| RwLock::new(None));

/// Called once at startup by whatever hosts the backend.
pub fn set_cookie_jar(jar: Arc<dyn CookieJar>) {
    *JAR.write().unwrap() = Some(jar);
}

pub(crate) fn cookie_jar() -> Option<Arc<dyn CookieJar>> {
    JAR.read().unwrap().clone()
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
        Ok(vec![])
    }
    async fn search(&self, _query: &str, _store: Option<&Store>) -> http::Result<Vec<ProductHit>> {
        Ok(vec![])
    }
}
