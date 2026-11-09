//! ICA's WAF cookie on phones. On desktop the app opens ICA's shop in a hidden window and
//! reads its cookies through Tauri (src-tauri/src/ica.rs), but on a phone an app has one
//! window and Android's webview doesn't hand its cookies to Tauri. So this plugin's
//! native half (android/, ios/) loads the shop in an offscreen webview of its own, waits
//! for the cookie and returns the shop's cookies. If ICA asks for a person, that webview
//! is shown to the person in a dialog (Android) or sheet (iOS) until they have answered
//! it or the time is up; nothing here answers the check itself.
//!
//! Only phones have a native half; on desktop the crate is empty.

#[cfg(mobile)]
pub use mobile::*;

#[cfg(mobile)]
mod mobile {
    use std::time::Duration;

    use serde::{Deserialize, Serialize};
    use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
    use tauri::{Manager, Runtime};

    #[cfg(target_os = "ios")]
    tauri::ios_plugin_binding!(init_plugin_ica_session);

    /// One cookie of the shop, as the webview holds it.
    #[derive(Debug, Clone, Deserialize)]
    pub struct Cookie {
        pub name: String,
        pub value: String,
    }

    /// What to open, which cookie to wait for and for how long.
    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MintRequest<'a> {
        /// The shop page; the cookies returned are the ones the webview would send to it.
        pub url: &'a str,
        /// The cookie that means the visit passed (`aws-waf-token`). Any copy left from an
        /// earlier visit is forgotten first, so a stale token never comes back as new.
        pub cookie: &'a str,
        /// How long a clean visit may take before the person is asked.
        #[serde(serialize_with = "millis")]
        pub quiet: Duration,
        /// How long the person gets once the webview is shown to them.
        #[serde(serialize_with = "millis")]
        pub person: Duration,
        /// The dialog's title when the person is asked.
        pub title: &'a str,
    }

    fn millis<S: serde::Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(d.as_millis() as u64)
    }

    #[derive(Deserialize)]
    struct Minted {
        cookies: Vec<Cookie>,
    }

    pub struct IcaSession<R: Runtime>(PluginHandle<R>);

    impl<R: Runtime> IcaSession<R> {
        /// The shop's cookies once `request.cookie` is among them, or the reason it isn't
        /// (the time ran out, or the person closed the check).
        pub async fn mint(&self, request: MintRequest<'_>) -> Result<Vec<Cookie>, String> {
            self.0
                .run_mobile_plugin_async::<Minted>("mint", request)
                .await
                .map(|m| m.cookies)
                .map_err(|e| match e {
                    tauri::plugin::mobile::PluginInvokeError::InvokeRejected(r) => {
                        r.message.unwrap_or_else(|| "ICA's check didn't finish".into())
                    }
                    e => e.to_string(),
                })
        }
    }

    pub trait IcaSessionExt<R: Runtime> {
        fn ica_session(&self) -> &IcaSession<R>;
    }

    impl<R: Runtime, T: Manager<R>> IcaSessionExt<R> for T {
        fn ica_session(&self) -> &IcaSession<R> {
            self.state::<IcaSession<R>>().inner()
        }
    }

    pub fn init<R: Runtime>() -> TauriPlugin<R> {
        Builder::new("ica-session")
            .setup(|app, api| {
                #[cfg(target_os = "android")]
                let handle = api.register_android_plugin("se.matpris.icasession", "IcaSessionPlugin")?;
                #[cfg(target_os = "ios")]
                let handle = api.register_ios_plugin(init_plugin_ica_session)?;
                app.manage(IcaSession(handle));
                Ok(())
            })
            .build()
    }
}
