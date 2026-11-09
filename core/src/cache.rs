//! Small in-memory TTL cache with in-flight de-duplication: concurrent callers asking
//! for the same key share one upstream request. Failures are not cached.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::OnceCell;
use tokio::time::Instant;

struct Entry<T> {
    expires: Instant,
    cell: Arc<OnceCell<T>>,
    /// Insertion order, to evict the oldest entry when the cache is full.
    seq: u64,
}

pub struct TtlCache<T> {
    ttl: Duration,
    max_entries: usize,
    inner: Mutex<(HashMap<String, Entry<T>>, u64)>,
}

impl<T: Clone + Send + Sync + 'static> TtlCache<T> {
    pub fn new(ttl: Duration) -> Self {
        Self::with_capacity(ttl, 1000)
    }

    pub fn with_capacity(ttl: Duration, max_entries: usize) -> Self {
        TtlCache { ttl, max_entries, inner: Mutex::new((HashMap::new(), 0)) }
    }

    /// The cached value for `key`, or `load()`'s, shared with everyone asking meanwhile.
    pub async fn get<E, F, Fut>(&self, key: &str, load: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let cell = {
            let mut guard = self.inner.lock().unwrap();
            let (entries, seq) = &mut *guard;
            let now = Instant::now();
            match entries.get(key) {
                Some(e) if e.expires > now => e.cell.clone(),
                _ => {
                    *seq += 1;
                    let cell = Arc::new(OnceCell::new());
                    entries.insert(key.to_string(), Entry { expires: now + self.ttl, cell: cell.clone(), seq: *seq });
                    if entries.len() > self.max_entries {
                        if let Some(oldest) = entries.iter().min_by_key(|(_, e)| e.seq).map(|(k, _)| k.clone()) {
                            entries.remove(&oldest);
                        }
                    }
                    cell
                }
            }
        };
        match cell.get_or_try_init(load).await {
            Ok(v) => Ok(v.clone()),
            Err(e) => {
                // Failures must not be cached: the next caller tries again.
                let mut guard = self.inner.lock().unwrap();
                if guard.0.get(key).is_some_and(|en| Arc::ptr_eq(&en.cell, &cell)) {
                    guard.0.remove(key);
                }
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn shares_one_load_and_does_not_cache_failures() {
        let cache = TtlCache::<u32>::new(Duration::from_secs(60));
        let calls = AtomicU32::new(0);
        let load = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(7)
        };
        let (a, b) = tokio::join!(cache.get("k", load), cache.get("k", load));
        assert_eq!((a, b), (Ok(7), Ok(7)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        assert_eq!(cache.get("bad", || async { Err::<u32, _>("no") }).await, Err("no"));
        assert_eq!(cache.get("bad", || async { Ok::<_, &str>(1) }).await, Ok(1));
    }

    #[tokio::test(start_paused = true)]
    async fn entries_expire() {
        let cache = TtlCache::<u32>::new(Duration::from_secs(60));
        assert_eq!(cache.get("k", || async { Ok::<_, ()>(1) }).await, Ok(1));
        tokio::time::advance(Duration::from_secs(61)).await;
        assert_eq!(cache.get("k", || async { Ok::<_, ()>(2) }).await, Ok(2));
    }
}
