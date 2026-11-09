//! The Matpris app (Tauri 2) for macOS, Windows, Linux, Android and iOS. The page is
//! the SvelteKit app (static build); everything it asks for is answered here by the
//! Rust backend in core/ (matpris-core), through the commands below.

mod ica;
#[cfg(desktop)]
mod update;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use matpris_core::geo::in_sweden;
use matpris_core::stores::registry::{priced_area, with_map_only_stores};
use matpris_core::{geocode, image, LatLng, NearbyStore, SearchEvent};
use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::ipc::Channel;
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

/// The search that is running, so a newer one (or leaving) stops it: one person, one search.
#[derive(Default)]
struct Searches(Mutex<Option<JoinHandle<()>>>);

impl Searches {
    fn replace(&self, next: Option<JoinHandle<()>>) {
        if let Some(old) = std::mem::replace(&mut *self.0.lock().unwrap(), next) {
            old.abort();
        }
    }
}

fn center(lat: f64, lng: f64) -> Result<LatLng, String> {
    let c = LatLng { lat, lng };
    if in_sweden(c) { Ok(c) } else { Err("lat/lng must be a location in Sweden".into()) }
}

fn radius(radius: Option<f64>) -> Result<Option<f64>, String> {
    match radius {
        Some(km) if !(0.5..=30.0).contains(&km) => Err("radius must be between 0.5 and 30 km".into()),
        r => Ok(r),
    }
}

/// Streams a search's events to the page as they happen; returns when it is done or
/// replaced by a newer search.
#[tauri::command]
async fn search(
    q: String,
    lat: f64,
    lng: f64,
    radius: Option<f64>,
    channel: Channel<SearchEvent>,
    searches: State<'_, Searches>,
) -> Result<(), String> {
    if q.trim().chars().count() < 2 {
        return Err("q must be at least 2 characters".into());
    }
    let c = center(lat, lng)?;
    let r = self::radius(radius)?;
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    searches.replace(Some(tauri::async_runtime::spawn(async move { matpris_core::search(&q, c, r, tx).await })));
    // Ends when the search finishes or is aborted (its sender is dropped either way).
    while let Some(event) = rx.recv().await {
        if channel.send(event).is_err() {
            break;
        }
    }
    Ok(())
}

#[tauri::command]
fn cancel_search(searches: State<'_, Searches>) {
    searches.replace(None);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoresAnswer {
    center: LatLng,
    radius_km: f64,
    stores: Vec<NearbyStore>,
}

/// The search circle around a location and its stores.
#[tauri::command]
async fn stores(lat: f64, lng: f64, radius: Option<f64>) -> Result<StoresAnswer, String> {
    let c = center(lat, lng)?;
    let area = priced_area(c, self::radius(radius)?).await.map_err(|e| e.to_string())?;
    let full = with_map_only_stores(c, area).await;
    Ok(StoresAnswer { center: c, radius_km: full.radius_km, stores: full.stores })
}

#[tauri::command]
async fn geocode(q: String) -> Result<Option<geocode::Place>, String> {
    geocode::geocode(&q).await.map_err(|e| e.to_string())
}

/// A product photo as a data: URL, which the page may draw into the shared image.
#[tauri::command]
async fn image(src: String) -> Result<String, String> {
    let (bytes, kind) = image::fetch_image(&src).await.map_err(|e| e.to_string())?;
    Ok(format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[cfg(desktop)]
use update::{update_restart, update_status};

/// Phones update through their own stores (or a new APK): nothing to report.
#[cfg(mobile)]
#[tauri::command]
fn update_status() -> Option<()> {
    None
}
#[cfg(mobile)]
#[tauri::command]
fn update_restart() -> Result<(), String> {
    Err("Phones update from their app store".into())
}

/// Set once the main window has opened. Until then, closing the update progress window
/// must not quit the app.
static MAIN_OPENED: AtomicBool = AtomicBool::new(false);

/// The main window: the page, at the size the desktop app has always opened at.
pub(crate) fn open_main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("main").is_some() {
        return Ok(());
    }
    MAIN_OPENED.store(true, Ordering::SeqCst);
    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()));
    #[cfg(desktop)]
    let builder = builder.title("Matpris").inner_size(1280.0, 820.0).min_inner_size(380.0, 600.0);
    // WebKitGTK denies any permission request nobody answers, so on Linux "Use my
    // location" always said location access was off. The page is our own; where the
    // position comes from (GeoClue) is still up to the system.
    #[cfg(target_os = "linux")]
    let builder = builder.on_permission_request(|_, kind| match kind {
        tauri::webview::PermissionKind::Geolocation => tauri::webview::PermissionResponse::Allow,
        _ => tauri::webview::PermissionResponse::Default,
    });
    builder.build()?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_geolocation::init());
    #[cfg(desktop)]
    let builder = {
        let builder = builder.plugin(tauri_plugin_process::init()).manage(update::Updates::default());
        match update::plugin() {
            Some(updater) => builder.plugin(updater),
            None => builder,
        }
    };

    builder
        .manage(Searches::default())
        .setup(|app| {
            matpris_core::chains::ica::set_cookie_jar(Arc::new(ica::WebviewJar::new(app.handle().clone())));
            #[cfg(desktop)]
            {
                // A newer release installs before the window opens; then the app restarts as it.
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    update::update_before_launch(&handle).await;
                    if let Err(e) = open_main_window(&handle) {
                        eprintln!("could not open the window: {e}");
                        handle.exit(1);
                    }
                    update::watch_for_updates(handle);
                });
            }
            #[cfg(mobile)]
            open_main_window(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search,
            cancel_search,
            stores,
            geocode,
            image,
            update_status,
            update_restart
        ])
        .build(tauri::generate_context!())
        .expect("error while building Matpris")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { code: None, api, .. } = event {
                if !MAIN_OPENED.load(Ordering::SeqCst) {
                    api.prevent_exit();
                }
            }
        });
}
