//! ICA's session cookie, minted the way a shopper's browser gets it: the app opens ICA's
//! shop in a hidden window, waits for the WAF's `aws-waf-token` cookie and hands the
//! cookies to the backend (matpris_core::chains::ica). If ICA wants a person to prove
//! they're human, the window is shown so the person using the app can do it; nothing
//! here solves or bypasses the check.
//!
//! Phones: Android webviews don't give their cookies to Tauri, and an app there has one
//! window, so a native plugin will have to read them (see mobile notes in the README);
//! until then ICA is "busy" on phones.

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use matpris_core::chains::ica::{Cookie, CookieJar, IcaSession};
use matpris_core::http;
use tauri::AppHandle;

/// The shop page of the store whose prices Matpris shows (Maxi ICA Gunnesbo, Lund).
const SHOP: &str = "https://handlaprivatkund.ica.se/stores/1088004";
const LABEL: &str = "ica-session";
/// A clean visit gets its token in a few seconds.
const QUIET_WAIT: Duration = Duration::from_secs(20);
/// How long a person gets to answer ICA's check.
const PERSON_WAIT: Duration = Duration::from_secs(180);

pub struct WebviewJar {
    app: AppHandle,
    session: Mutex<Option<IcaSession>>,
    /// One warm-up at a time.
    warming: tokio::sync::Mutex<()>,
}

impl WebviewJar {
    pub fn new(app: AppHandle) -> Self {
        WebviewJar { app, session: Mutex::new(None), warming: tokio::sync::Mutex::new(()) }
    }
}

#[cfg(desktop)]
mod desktop {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tauri::webview::PageLoadEvent;
    use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

    /// After the shop page has loaded, how long its WAF script gets to renew the token.
    const SETTLE: Duration = Duration::from_secs(2);

    fn open(app: &AppHandle, loaded: Arc<AtomicBool>) -> Result<WebviewWindow, String> {
        if let Some(w) = app.get_webview_window(LABEL) {
            return Ok(w);
        }
        let url = SHOP.parse().map_err(|e| format!("{e}"))?;
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(url))
            .title("ICA wants to check that you're a person")
            .inner_size(480.0, 640.0)
            .visible(false)
            .on_page_load(move |_, page| {
                if page.event() == PageLoadEvent::Finished {
                    loaded.store(true, Ordering::SeqCst);
                }
            })
            .build()
            .map_err(|e| e.to_string())
    }

    fn waf_token(cookies: &[tauri::webview::Cookie<'static>]) -> Option<String> {
        cookies.iter().find(|c| c.name() == "aws-waf-token" && !c.value().is_empty()).map(|c| c.value().to_string())
    }

    /// The shop's cookies once this visit has a WAF token. The webview keeps its cookies
    /// on disk and the token cookie says it lasts four days, while the WAF accepts it
    /// for minutes: a token that was already there (`old`) only counts once the page
    /// has loaded and had SETTLE to renew it (and `loaded` is given), else every warm
    /// would hand back the token the WAF just turned down. A new token counts at once.
    async fn token(
        window: &WebviewWindow,
        within: Duration,
        old: Option<&str>,
        loaded: Option<&AtomicBool>,
    ) -> Option<Vec<Cookie>> {
        let url: url::Url = SHOP.parse().ok()?;
        let deadline = tokio::time::Instant::now() + within;
        let mut loaded_at = None;
        while tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if loaded_at.is_none() && loaded.is_some_and(|l| l.load(Ordering::SeqCst)) {
                loaded_at = Some(tokio::time::Instant::now());
            }
            // Off the main thread: on Windows reading cookies from it deadlocks.
            let cookies = window.cookies_for_url(url.clone()).unwrap_or_default();
            let Some(token) = waf_token(&cookies) else { continue };
            if Some(token.as_str()) != old || loaded_at.is_some_and(|t| t.elapsed() >= SETTLE) {
                return Some(cookies.iter().map(|c| Cookie { name: c.name().into(), value: c.value().into() }).collect());
            }
        }
        None
    }

    pub async fn warm(app: &AppHandle) -> Result<Vec<Cookie>, http::Error> {
        let loaded = Arc::new(AtomicBool::new(false));
        let window = open(app, loaded.clone()).map_err(|e| http::Error::Throttled(format!("can't open ICA's shop: {e}")))?;
        let url: url::Url = SHOP.parse().map_err(|e| http::Error::Other(format!("{e}")))?;
        // What the cookie store held before this visit (the window shares it).
        let old = waf_token(&window.cookies_for_url(url).unwrap_or_default());
        let mut cookies = token(&window, QUIET_WAIT, old.as_deref(), Some(&loaded)).await;
        if cookies.is_none() {
            // A visual check: only a person can answer it, so show it to the person here.
            // Answering it mints a new token; the old one can't stand in for that.
            let _ = window.show();
            let _ = window.set_focus();
            cookies = token(&window, PERSON_WAIT, old.as_deref(), None).await;
        }
        let _ = window.close();
        cookies.ok_or_else(|| http::Error::Throttled("ICA wants a person to answer its check first".into()))
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[async_trait]
impl CookieJar for WebviewJar {
    async fn read(&self) -> Option<IcaSession> {
        self.session.lock().unwrap().clone()
    }

    async fn warm(&self) -> http::Result<()> {
        let _one = self.warming.lock().await;
        #[cfg(desktop)]
        {
            let cookies = desktop::warm(&self.app).await?;
            *self.session.lock().unwrap() = Some(IcaSession { stored_at: now_ms(), cookies });
            Ok(())
        }
        #[cfg(mobile)]
        {
            let _ = (&self.app, now_ms, SHOP, LABEL, QUIET_WAIT, PERSON_WAIT);
            Err(http::Error::Throttled("ICA isn't available on phones yet".into()))
        }
    }
}
