//! ICA's session for the dev server: reads scripts/.ica-profile/session.json and mints
//! a fresh one by running scripts/ica-warm.mjs (Playwright + Chrome) — headless first,
//! and, if the WAF wants a human, headed, where the visible window waits for the
//! puzzle to be clicked by a person. Nothing here solves a challenge.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use matpris_core::chains::ica::{CookieJar, IcaSession, Warm};
use matpris_core::http;
use tokio::process::Command;
use tokio::sync::Mutex;

pub struct ScriptJar {
    session: PathBuf,
    warm: PathBuf,
    /// One warm at a time.
    running: Mutex<()>,
}

impl ScriptJar {
    /// The repo's scripts/ (this crate lives in <repo>/server), unless ICA_SESSION_FILE /
    /// ICA_WARM_SCRIPT say otherwise.
    pub fn discover() -> Self {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let session = std::env::var_os("ICA_SESSION_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| repo.join("scripts/.ica-profile/session.json"));
        let warm =
            std::env::var_os("ICA_WARM_SCRIPT").map(PathBuf::from).unwrap_or_else(|| repo.join("scripts/ica-warm.mjs"));
        ScriptJar { session, warm, running: Mutex::new(()) }
    }

    async fn run(&self, headed: bool, timeout: Duration) -> http::Result<()> {
        // Playwright and the .ica-profile live in the repo: run from there.
        let cwd = self.warm.parent().and_then(|p| p.parent()).map(PathBuf::from).unwrap_or_default();
        let mut cmd = Command::new("node");
        cmd.arg(&self.warm).current_dir(cwd).kill_on_drop(true);
        if headed {
            cmd.env("HEADED", "1");
        }
        let out = tokio::time::timeout(timeout, cmd.output())
            .await
            .map_err(|_| http::Error::Throttled("ICA warm-up timed out".into()))?
            .map_err(|e| http::Error::Throttled(format!("can't run the ICA warm script: {e}")))?;
        if out.status.success() {
            Ok(())
        } else {
            let first = String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("warm failed").to_string();
            Err(http::Error::Throttled(first))
        }
    }
}

#[async_trait]
impl CookieJar for ScriptJar {
    async fn read(&self) -> Option<IcaSession> {
        let text = tokio::fs::read_to_string(&self.session).await.ok()?;
        serde_json::from_str(&text).ok()
    }

    async fn warm(&self, _why: Warm) -> http::Result<()> {
        // The script always starts a new browser visit; a challenge opens a visible window.
        let _one = self.running.lock().await;
        if !self.warm.exists() {
            return Err(http::Error::Throttled("no ICA warm script — run `bun run warm:ica` in the matpris repo".into()));
        }
        match self.run(false, Duration::from_secs(70)).await {
            Err(http::Error::Throttled(msg)) if msg.contains("visual challenge") || msg.contains("no WAF token") => {
                self.run(true, Duration::from_secs(620)).await
            }
            other => other,
        }
    }
}
