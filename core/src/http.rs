//! Outbound HTTP for the chain adapters: identifies itself, times out, and spaces
//! requests per host so we stay within each site's stated crawl rate.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::oneshot;
use tokio::time::Instant;

pub const USER_AGENT: &str = "matpris/0.1 (grocery price comparison)";

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// The host's request spacing would make us wait too long: try again shortly.
    #[error("{0}")]
    Throttled(String),
    /// A queued request dropped because a newer one for the same host took over its turn.
    #[error("{0}")]
    Superseded(String),
    #[error("{host} answered {status}")]
    Status { host: String, status: u16 },
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Shown as "busy" rather than as a failure.
    pub fn is_throttled(&self) -> bool {
        matches!(self, Error::Throttled(_) | Error::Superseded(_))
    }
    pub fn other(e: impl std::fmt::Display) -> Self {
        Error::Other(e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        let host = e.url().and_then(|u| u.host_str()).unwrap_or("the chain").to_string();
        if e.is_timeout() {
            Error::Other(format!("{host} timed out"))
        } else if let Some(status) = e.status() {
            Error::Status { host, status: status.as_u16() }
        } else {
            Error::Other(format!("{host}: {e}"))
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// One client for every request: connection reuse, rustls on every platform (phones included).
pub fn client() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(8))
            .build()
            .expect("HTTP client")
    });
    &CLIENT
}

#[derive(Debug, Clone)]
pub struct FetchOptions {
    /// Minimum time between requests to the same host.
    pub interval: Duration,
    /// Give up instead of queueing longer than this.
    pub max_wait: Duration,
    pub timeout: Duration,
    /// Extra headers (e.g. a cookie a session refresher maintains).
    pub headers: Vec<(String, String)>,
    /// A newer `latest_only` request to the same host replaces this one while it waits.
    pub latest_only: bool,
}

impl FetchOptions {
    pub fn every(interval: Duration) -> Self {
        FetchOptions {
            interval,
            max_wait: Duration::from_secs(8),
            timeout: Duration::from_secs(8),
            headers: Vec::new(),
            latest_only: false,
        }
    }
    pub fn max_wait(mut self, d: Duration) -> Self {
        self.max_wait = d;
        self
    }
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }
    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }
    pub fn latest_only(mut self) -> Self {
        self.latest_only = true;
        self
    }
}

struct Waiting {
    slot: Instant,
    id: u64,
    drop: oneshot::Sender<()>,
}

#[derive(Default)]
struct Slots {
    next: HashMap<String, Instant>,
    /// Per host, the `latest_only` request still waiting for its turn.
    waiting: HashMap<String, Waiting>,
    ids: u64,
}

static SLOTS: LazyLock<Mutex<Slots>> = LazyLock::new(|| Mutex::new(Slots::default()));

/// Reserve the next request slot for `host`. Requests are spaced `interval` apart; if
/// the wait would exceed `max_wait` we give up instead of queueing people behind each
/// other. A `latest_only` request takes over the turn of one that is still waiting
/// (which fails with `Superseded`) instead of queueing behind it: when someone searches
/// again, only the newest search matters.
// shortcut: "newest wins" assumes one person searching at a time (true in the app; the
// dev server is for one developer).
pub async fn take_slot(host: &str, interval: Duration, max_wait: Duration, latest_only: bool) -> Result<()> {
    let now = Instant::now();
    let (slot, waiter) = {
        let mut s = SLOTS.lock().unwrap();
        let queued = if latest_only { s.waiting.remove(host) } else { None };
        let slot = match queued {
            Some(q) => {
                let _ = q.drop.send(());
                q.slot
            }
            None => {
                let slot = s.next.get(host).copied().map_or(now, |n| n.max(now));
                if slot - now > max_wait {
                    return Err(Error::Throttled(format!("{host} is rate limited, try again shortly")));
                }
                s.next.insert(host.to_string(), slot + interval);
                slot
            }
        };
        if slot <= now {
            return Ok(());
        }
        let waiter = if latest_only {
            s.ids += 1;
            let id = s.ids;
            let (tx, rx) = oneshot::channel();
            s.waiting.insert(host.to_string(), Waiting { slot, id, drop: tx });
            Some((id, rx))
        } else {
            None
        };
        (slot, waiter)
    };
    match waiter {
        None => {
            tokio::time::sleep_until(slot).await;
            Ok(())
        }
        Some((id, rx)) => {
            tokio::select! {
                _ = tokio::time::sleep_until(slot) => {
                    let mut s = SLOTS.lock().unwrap();
                    if s.waiting.get(host).is_some_and(|w| w.id == id) {
                        s.waiting.remove(host);
                    }
                    Ok(())
                }
                _ = rx => Err(Error::Superseded(format!("{host}: replaced by a newer search"))),
            }
        }
    }
}

fn host_of(url: &str) -> Result<String> {
    let u = url::Url::parse(url).map_err(|e| Error::Other(format!("bad URL {url}: {e}")))?;
    Ok(match u.port() {
        Some(p) => format!("{}:{p}", u.host_str().unwrap_or_default()),
        None => u.host_str().unwrap_or_default().to_string(),
    })
}

async fn send(req: reqwest::RequestBuilder, host: &str, opts: &FetchOptions) -> Result<reqwest::Response> {
    take_slot(host, opts.interval, opts.max_wait, opts.latest_only).await?;
    // Not 'application/json': Lidl's search answers 401 to that (it serves its own media type).
    let mut req = req.header("accept", "*/*").timeout(opts.timeout);
    for (k, v) in &opts.headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let res = req.send().await?;
    let status = res.status().as_u16();
    if !res.status().is_success() || status == 202 {
        // 202 is ICA's WAF challenge: an answer, but not the data.
        return Err(Error::Status { host: host.to_string(), status });
    }
    Ok(res)
}

/// GET `url` as JSON, spaced per host by `opts`.
pub async fn fetch_json<T: DeserializeOwned>(url: &str, opts: &FetchOptions) -> Result<T> {
    let host = host_of(url)?;
    let res = send(client().get(url), &host, opts).await?;
    res.json::<T>().await.map_err(|e| Error::Other(format!("{host}: unexpected answer ({e})")))
}

/// GET `url` as text (e.g. a page whose source carries an API key).
pub async fn fetch_text(url: &str, opts: &FetchOptions) -> Result<String> {
    let host = host_of(url)?;
    let res = send(client().get(url), &host, opts).await?;
    Ok(res.text().await?)
}

/// POST `body` as JSON to `url` and read a JSON answer.
pub async fn post_json<B: Serialize + ?Sized, T: DeserializeOwned>(url: &str, body: &B, opts: &FetchOptions) -> Result<T> {
    let host = host_of(url)?;
    let res = send(client().post(url).json(body), &host, opts).await?;
    res.json::<T>().await.map_err(|e| Error::Other(format!("{host}: unexpected answer ({e})")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPACING: Duration = Duration::from_secs(10);
    const MAX: Duration = Duration::from_secs(30);

    #[tokio::test(start_paused = true)]
    async fn the_newest_latest_only_request_takes_over_a_queued_turn() {
        let start = Instant::now();
        take_slot("newest.test", SPACING, MAX, true).await.unwrap(); // goes now
        let second = tokio::spawn(take_slot("newest.test", SPACING, MAX, true)); // queued for t=10 s
        tokio::task::yield_now().await;
        let third = tokio::spawn(take_slot("newest.test", SPACING, MAX, true)); // replaces it
        assert!(matches!(second.await.unwrap(), Err(Error::Superseded(_))));
        third.await.unwrap().unwrap();
        // Still one Crawl-delay after the first.
        assert_eq!(Instant::now() - start, SPACING);
    }

    #[tokio::test(start_paused = true)]
    async fn ordinary_requests_queue_behind_each_other() {
        let start = Instant::now();
        take_slot("queue.test", SPACING, MAX, false).await.unwrap();
        take_slot("queue.test", SPACING, MAX, false).await.unwrap();
        assert_eq!(Instant::now() - start, SPACING);
    }

    #[tokio::test(start_paused = true)]
    async fn a_wait_beyond_max_wait_is_throttled() {
        for _ in 0..4 {
            drop(tokio::spawn(take_slot("busy.test", SPACING, MAX, false)));
        }
        tokio::task::yield_now().await;
        assert!(matches!(take_slot("busy.test", SPACING, MAX, false).await, Err(Error::Throttled(_))));
    }
}
