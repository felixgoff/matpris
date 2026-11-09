//! Self-update on the desktop, as the Electrobun app did: on launch a newer release is
//! downloaded and installed before the window opens (a small progress window shows
//! meanwhile); while the app is open it looks hourly, downloads quietly, and the page
//! offers a restart. An update is never a reason not to start: offline, no login, no
//! signing key or any failure opens the installed version.
//!
//! Releases are on the private felixgoff/matpris repo, so the updater reads them
//! through the GitHub API with this computer's `gh` login: the latest release's
//! `latest.json` (written by the Release workflow with API asset URLs) is the endpoint,
//! and the same token rides along to download the update. GitHub answers an asset with
//! a redirect to signed storage, and the HTTP client drops the token on that redirect.
//! The token is asked for when needed and kept in memory only.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Set by the release build (TAURI_UPDATER_PUBKEY); dev builds have none and never update.
const PUBKEY: Option<&str> = option_env!("TAURI_UPDATER_PUBKEY");
const LATEST: &str = "https://api.github.com/repos/felixgoff/matpris/releases/latest";
const CHECK_TIMEOUT: Duration = Duration::from_secs(8);
const WATCH_EVERY: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Idle,
    Downloading,
    Ready,
    Installing,
}

/// What the page and the progress window are told.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    phase: Phase,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    /// Percent, while downloading with a known size.
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<u32>,
}

pub struct Updates {
    status: Mutex<Status>,
    /// A downloaded update waiting for a restart.
    ready: Mutex<Option<(Update, Vec<u8>)>>,
    token: Mutex<Option<Option<String>>>,
}

impl Default for Updates {
    fn default() -> Self {
        Updates {
            status: Mutex::new(Status { phase: Phase::Idle, version: None, progress: None }),
            ready: Mutex::new(None),
            token: Mutex::new(None),
        }
    }
}

impl Updates {
    fn set(&self, phase: Phase, version: Option<String>) {
        *self.status.lock().unwrap() = Status { phase, version, progress: None };
    }
}

/// Apps opened from Finder, the Start menu or a desktop entry don't get the shell's PATH.
fn gh() -> Option<&'static str> {
    const CANDIDATES: [&str; 5] = [
        "/opt/homebrew/bin/gh",
        "/usr/local/bin/gh",
        "/usr/bin/gh",
        "/home/linuxbrew/.linuxbrew/bin/gh",
        "C:\\Program Files\\GitHub CLI\\gh.exe",
    ];
    CANDIDATES.into_iter().find(|p| Path::new(p).exists())
}

fn token(updates: &Updates) -> Option<String> {
    updates
        .token
        .lock()
        .unwrap()
        .get_or_insert_with(|| {
            let mut cmd = std::process::Command::new(gh()?);
            cmd.args(["auth", "token"]);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000); // no console window
            }
            let out = cmd.output().ok()?;
            let t = String::from_utf8(out.stdout).ok()?.trim().to_string();
            (out.status.success() && !t.is_empty()).then_some(t)
        })
        .clone()
}

/// The newer release, if there is one this app can update to.
async fn newer_release(app: &AppHandle) -> Result<Option<Update>, String> {
    if PUBKEY.is_none_or(str::is_empty) || cfg!(debug_assertions) {
        return Ok(None);
    }
    let Some(token) = token(&app.state::<Updates>()) else { return Ok(None) };
    let auth = format!("Bearer {token}");
    // The latest release's latest.json, as an API asset URL the token can read.
    let release: serde_json::Value = reqwest::Client::new()
        .get(LATEST)
        .header("authorization", &auth)
        .header("user-agent", "matpris-updater")
        .header("x-github-api-version", "2022-11-28")
        .timeout(CHECK_TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let endpoint = release["assets"]
        .as_array()
        .and_then(|assets| assets.iter().find(|a| a["name"] == "latest.json"))
        .and_then(|a| a["url"].as_str())
        .ok_or("the latest release has no latest.json")?
        .parse()
        .map_err(|e: url::ParseError| e.to_string())?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .and_then(|b| b.header("authorization", &auth))
        .and_then(|b| b.header("accept", "application/octet-stream"))
        .and_then(|b| b.build())
        .map_err(|e| e.to_string())?;
    // Only the check is bounded: the updater's own timeout would cut the download short too.
    tokio::time::timeout(CHECK_TIMEOUT, updater.check())
        .await
        .map_err(|_| "the update check timed out".to_string())?
        .map_err(|e| e.to_string())
}

async fn download(app: &AppHandle, update: &Update) -> Result<Vec<u8>, String> {
    let updates = app.state::<Updates>();
    updates.set(Phase::Downloading, Some(update.version.clone()));
    let mut received = 0u64;
    update
        .download(
            |chunk, total| {
                received += chunk as u64;
                if let Some(total) = total.filter(|t| *t > 0) {
                    updates.status.lock().unwrap().progress = Some((received * 100 / total).min(100) as u32);
                }
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())
}

/// On launch: install a newer release before the window opens. Returns if there was
/// none or it failed; on success the app restarts as the new version.
pub async fn update_before_launch(app: &AppHandle) {
    let Ok(Some(update)) = newer_release(app).await.inspect_err(|e| eprintln!("update check skipped: {e}")) else {
        return;
    };
    let progress = WebviewWindowBuilder::new(app, "updating", WebviewUrl::App("updating.html".into()))
        .title("Updating Matpris")
        .inner_size(380.0, 150.0)
        .resizable(false)
        .decorations(false)
        .center()
        .build()
        .ok();
    let result = async {
        let bytes = download(app, &update).await?;
        app.state::<Updates>().set(Phase::Installing, Some(update.version.clone()));
        update.install(&bytes).map_err(|e| e.to_string())
    }
    .await;
    match result {
        Ok(()) => app.restart(),
        Err(e) => {
            eprintln!("update skipped: {e}");
            app.state::<Updates>().set(Phase::Idle, None);
            if let Some(w) = progress {
                let _ = w.close();
            }
        }
    }
}

/// While the app is open: look hourly and download quietly; the page offers a restart.
pub fn watch_for_updates(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(WATCH_EVERY).await;
            if app.state::<Updates>().status.lock().unwrap().phase != Phase::Idle {
                continue;
            }
            let found = match newer_release(&app).await {
                Ok(Some(u)) => u,
                Ok(None) => continue,
                Err(e) => {
                    eprintln!("update check failed: {e}");
                    continue;
                }
            };
            match download(&app, &found).await {
                Ok(bytes) => {
                    let updates = app.state::<Updates>();
                    updates.set(Phase::Ready, Some(found.version.clone()));
                    *updates.ready.lock().unwrap() = Some((found, bytes));
                }
                Err(e) => {
                    eprintln!("update download failed: {e}");
                    app.state::<Updates>().set(Phase::Idle, None);
                }
            }
        }
    });
}

/// The update state, for the page's toast and the launch progress window.
#[tauri::command]
pub fn update_status(updates: State<'_, Updates>) -> Status {
    updates.status.lock().unwrap().clone()
}

/// Install the downloaded update and restart into it.
#[tauri::command]
pub fn update_restart(app: AppHandle, updates: State<'_, Updates>) -> Result<(), String> {
    let Some((update, bytes)) = updates.ready.lock().unwrap().take() else {
        return Err("No update is waiting".into());
    };
    updates.set(Phase::Installing, Some(update.version.clone()));
    match update.install(&bytes) {
        Ok(()) => app.restart(),
        Err(e) => {
            // Keep it: opening the app next time installs it anyway.
            updates.set(Phase::Ready, Some(update.version.clone()));
            *updates.ready.lock().unwrap() = Some((update, bytes));
            Err(e.to_string())
        }
    }
}

/// Registers the updater plugin when this build can verify updates.
pub fn plugin<R: tauri::Runtime>() -> Option<tauri::plugin::TauriPlugin<R, tauri_plugin_updater::Config>> {
    PUBKEY.filter(|k| !k.is_empty()).map(|k| tauri_plugin_updater::Builder::new().pubkey(k).build())
}
