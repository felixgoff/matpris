//! The phone's half of the Galaxy Watch bridge (the watch's half is wear/, Api.kt). The
//! watch asks the phone over the Wear OS Data Layer; WearSearchService.kt (in
//! gen/android) receives the question and calls [`ffi`]'s `search` here, which runs the
//! same search as the page's (matpris_core::search) and hands each event back as JSON,
//! one Data Layer message each.
//!
//! The service can start the app's process without its window, so nothing here needs
//! Tauri to be running: the search only needs matpris_core. ICA then has no cookie jar
//! and answers "busy", which the watch shows like any busy chain.

use std::collections::BTreeSet;
use std::time::Duration;

use matpris_core::geo::in_sweden;
use matpris_core::{ChainId, ChainStatus, LatLng, SearchEvent};
use serde_json::{json, Map, Value};

/// Where the owner tests from and the page's default, for a watch without a usable fix.
pub const LUND: LatLng = LatLng { lat: 55.7047, lng: 13.1910 };

/// The Data Layer refuses messages over 100 KB; keep each one well under it.
pub const MAX_MESSAGE: usize = 90 * 1024;

/// The watch gives up after 90 s without a message. A chain still working after this
/// (ICA waiting for a person on the phone, a slow host) is reported busy, so the watch
/// always hears "done".
pub const DEADLINE: Duration = Duration::from_secs(60);

/// The query, as the Tauri `search` command accepts it.
pub fn query(q: &str) -> Result<&str, String> {
    let q = q.trim();
    if q.chars().count() < 2 {
        return Err("Search for at least 2 letters".into());
    }
    Ok(q)
}

/// The watch's location, or Lund when it sent none we can use (no fix, abroad, NaN).
pub fn center(lat: f64, lng: f64) -> LatLng {
    let c = LatLng { lat, lng };
    if in_sweden(c) {
        c
    } else {
        LUND
    }
}

/// The fields of a result row that the watch reads (Ranking.kt, `parseRow`). Photos,
/// links, categories and the store's address stay on the phone: they cost Bluetooth
/// time and the 100 KB message limit, and the watch shows none of them.
const ROW_FIELDS: [&str; 13] = [
    "chain", "id", "name", "brand", "size", "price", "unitPrice", "unit", "perKg", "approxEach", "oldPrice", "deal",
    "match",
];
const DEAL_FIELDS: [&str; 4] = ["quantity", "each", "total", "unitPrice"];
const STORE_FIELDS: [&str; 2] = ["name", "distanceKm"];

fn pick(from: &Value, keys: &[&str]) -> Map<String, Value> {
    keys.iter().filter_map(|&k| from.get(k).map(|v| (k.to_string(), v.clone()))).collect()
}

fn slim_row(row: &Value) -> Value {
    let mut out = pick(row, &ROW_FIELDS);
    if let Some(deal) = row.get("deal") {
        out.insert("deal".into(), Value::Object(pick(deal, &DEAL_FIELDS)));
    }
    if let Some(store) = row.get("store") {
        out.insert("store".into(), Value::Object(pick(store, &STORE_FIELDS)));
    }
    Value::Object(out)
}

fn chain_message(chain: &Value, status: &Value, rows: &[String]) -> String {
    let head = json!({ "type": "chain", "chain": chain, "status": status }).to_string();
    // `{"type":…,"status":…}` + `,"results":[` + rows + `]}`
    format!("{},\"results\":[{}]}}", &head[..head.len() - 1], rows.join(","))
}

/// What one search event becomes on the watch: zero or more messages, each a JSON
/// SearchEvent under [`MAX_MESSAGE`] bytes.
///
/// - `area` events are left out: the watch has no map.
/// - A chain's rows are slimmed to what the watch reads, and spread over several
///   `chain` events of the same chain and status when they don't fit in one (the watch
///   adds every event's rows to its list, so they add up to the same list).
/// - An error's message is cut short, never the event dropped: the watch must hear it.
pub fn messages(event: &SearchEvent) -> Vec<String> {
    match event {
        SearchEvent::Area { .. } => vec![],
        SearchEvent::Done => vec![json!({ "type": "done" }).to_string()],
        SearchEvent::Error { message } => {
            let short: String = message.chars().take(500).collect();
            vec![json!({ "type": "error", "message": short }).to_string()]
        }
        SearchEvent::Chain { .. } => {
            let Ok(full) = serde_json::to_value(event) else { return vec![] };
            let (chain, status) = (&full["chain"], &full["status"]);
            let rows: Vec<String> = full["results"]
                .as_array()
                .map(|rows| rows.iter().map(|r| slim_row(r).to_string()).collect())
                .unwrap_or_default();
            let empty = chain_message(chain, status, &[]).len();
            let mut out = vec![];
            let mut batch: Vec<String> = vec![];
            let mut size = empty;
            for row in rows {
                // A row that can't fit even alone (a name of tens of KB) would never
                // arrive; leave it out rather than lose the chain.
                if empty + row.len() > MAX_MESSAGE {
                    continue;
                }
                let grown = size + row.len() + usize::from(!batch.is_empty());
                if grown > MAX_MESSAGE && !batch.is_empty() {
                    out.push(chain_message(chain, status, &batch));
                    batch.clear();
                    size = empty;
                }
                size += row.len() + usize::from(!batch.is_empty());
                batch.push(row);
            }
            if !batch.is_empty() || out.is_empty() {
                out.push(chain_message(chain, status, &batch));
            }
            out
        }
    }
}

/// The chains that hadn't answered by the [`DEADLINE`], reported busy so the watch
/// shows them as such instead of waiting.
pub fn unanswered(answered: &BTreeSet<ChainId>) -> Vec<SearchEvent> {
    ChainId::ALL
        .into_iter()
        .filter(|c| !answered.contains(c))
        .map(|chain| SearchEvent::Chain {
            chain,
            status: ChainStatus::Throttled,
            results: vec![],
            message: Some("No answer in time".into()),
        })
        .collect()
}

/// Runs a search for the watch and hands each message to `send`, in order, ending with
/// `done` or `error`. `send` returns false when the watch can't be reached any more,
/// which stops the search.
pub async fn run(q: &str, lat: f64, lng: f64, mut send: impl FnMut(String) -> bool) {
    let q = match query(q) {
        Ok(q) => q.to_string(),
        Err(message) => {
            for m in messages(&SearchEvent::Error { message }) {
                send(m);
            }
            return;
        }
    };
    let c = center(lat, lng);
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let task = tauri::async_runtime::spawn(async move { matpris_core::search(&q, c, None, tx).await });
    // Aborts the search when the watch is gone, or at the deadline.
    let _stop = AbortOnDrop(task);

    let deadline = tokio::time::Instant::now() + DEADLINE;
    let mut answered = BTreeSet::new();
    loop {
        let event = match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(event)) => event,
            // The search ended without `done` (it panicked): say so.
            Ok(None) => SearchEvent::Error { message: "The search stopped".into() },
            Err(_) => {
                for event in unanswered(&answered) {
                    for m in messages(&event) {
                        if !send(m) {
                            return;
                        }
                    }
                }
                SearchEvent::Done
            }
        };
        if let SearchEvent::Chain { chain, .. } = &event {
            answered.insert(*chain);
        }
        let last = matches!(event, SearchEvent::Done | SearchEvent::Error { .. });
        for m in messages(&event) {
            if !send(m) {
                return;
            }
        }
        if last {
            return;
        }
    }
}

struct AbortOnDrop(tauri::async_runtime::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The JNI entry point WatchBridge.kt calls (`external fun search`).
#[cfg(target_os = "android")]
pub mod ffi {
    use jni::objects::{JClass, JObject, JString, JValue};
    use jni::sys::jdouble;
    use jni::JNIEnv;

    /// Blocks the calling thread (the service's message thread, never the main one) until
    /// the search is done, and calls `callback.onEvent(json): Boolean` on that same
    /// thread for each message. Calling back from the thread Java called us on means it
    /// is already attached to the JVM, and messages reach the watch in order.
    #[no_mangle]
    pub extern "system" fn Java_se_matpris_matpris_WatchBridge_search<'local>(
        mut env: JNIEnv<'local>,
        _class: JClass<'local>,
        q: JString<'local>,
        lat: jdouble,
        lng: jdouble,
        callback: JObject<'local>,
    ) {
        let q: String = match env.get_string(&q) {
            Ok(s) => s.into(),
            // Calling back into Java with an exception pending is undefined: clear it,
            // and the empty query answers with the usual "at least 2 letters" error.
            Err(_) => {
                let _ = env.exception_clear();
                String::new()
            }
        };
        // A panic must not unwind into the JVM (that aborts the process).
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tauri::async_runtime::block_on(super::run(&q, lat, lng, |message| deliver(&mut env, &callback, &message)))
        }));
        if result.is_err() {
            let error = super::messages(&matpris_core::SearchEvent::Error { message: "The search failed".into() });
            for m in error {
                deliver(&mut env, &callback, &m);
            }
        }
    }

    fn deliver(env: &mut JNIEnv, callback: &JObject, message: &str) -> bool {
        // A local frame per message: a search makes a few dozen, and the JVM's local
        // reference table is small.
        let sent = env.with_local_frame(4, |env| -> jni::errors::Result<bool> {
            let text = env.new_string(message)?;
            env.call_method(callback, "onEvent", "(Ljava/lang/String;)Z", &[JValue::Object(&text)])?.z()
        });
        match sent {
            Ok(keep_going) => keep_going,
            Err(_) => {
                // The callback threw: clear it so the JVM isn't left with a pending
                // exception, and stop.
                let _ = env.exception_clear();
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matpris_core::{Deal, Match, NearbyStore, Pricing, ProductHit, ResultRow, Store};

    fn row(i: usize, name: &str) -> ResultRow {
        ResultRow {
            hit: ProductHit {
                chain: ChainId::Willys,
                id: format!("p{i}"),
                name: name.into(),
                brand: Some("Fanta".into()),
                size: Some("33cl".into()),
                price: 12.9,
                unit_price: Some(39.09),
                unit: Some(matpris_core::Unit::L),
                categories: Some(vec!["Dryck".into(), "Läsk".into()]),
                per_kg: None,
                approx_each: None,
                old_price: Some(14.9),
                deal: Some(Deal {
                    quantity: 3,
                    each: 10.0,
                    total: 30.0,
                    unit_price: Some(30.3),
                    mix_and_match: true,
                    plus_deposit: true,
                    limit: Some(2),
                }),
                image: Some("https://assets.axfood.se/image/upload/f_auto,t_200/07310350118342_C1N1_s01".into()),
                url: "https://www.willys.se/produkt/fanta-101".into(),
                pieces: None,
                pricing: Pricing::Chain,
            },
            store: NearbyStore {
                store: Store {
                    id: "2110".into(),
                    chain: ChainId::Willys,
                    name: "Willys Lund Nova".into(),
                    lat: 55.72,
                    lng: 13.21,
                    address: "Företagsvägen 10, Lund".into(),
                    url: Some("https://www.willys.se/butik/2110".into()),
                },
                distance_km: 2.3,
            },
            match_: Match::Best,
        }
    }

    fn chain(results: Vec<ResultRow>) -> SearchEvent {
        SearchEvent::Chain { chain: ChainId::Willys, status: ChainStatus::Ok, results, message: Some("x".into()) }
    }

    #[test]
    fn rows_keep_only_what_the_watch_reads() {
        let out = messages(&chain(vec![row(1, "Fanta Orange")]));
        assert_eq!(out.len(), 1);
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(
            v,
            json!({
                "type": "chain", "chain": "willys", "status": "ok",
                "results": [{
                    "chain": "willys", "id": "p1", "name": "Fanta Orange", "brand": "Fanta", "size": "33cl",
                    "price": 12.9, "unitPrice": 39.09, "unit": "l", "oldPrice": 14.9, "match": "best",
                    "deal": { "quantity": 3, "each": 10.0, "total": 30.0, "unitPrice": 30.3 },
                    "store": { "name": "Willys Lund Nova", "distanceKm": 2.3 }
                }]
            })
        );
    }

    #[test]
    fn a_chain_without_rows_is_still_one_event() {
        let out = messages(&SearchEvent::Chain {
            chain: ChainId::Ica,
            status: ChainStatus::Throttled,
            results: vec![],
            message: None,
        });
        assert_eq!(out.len(), 1);
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v, json!({ "type": "chain", "chain": "ica", "status": "throttled", "results": [] }));
    }

    #[test]
    fn many_rows_are_split_under_the_limit_and_all_arrive() {
        let long = "Läsk ".repeat(400); // ~2 KB a row
        let rows: Vec<_> = (0..120).map(|i| row(i, &long)).collect();
        let out = messages(&chain(rows));
        assert!(out.len() > 1, "{} messages", out.len());
        let mut ids = vec![];
        for m in &out {
            assert!(m.len() <= MAX_MESSAGE, "{} bytes", m.len());
            let v: Value = serde_json::from_str(m).unwrap();
            assert_eq!((v["type"].as_str(), v["chain"].as_str(), v["status"].as_str()), (Some("chain"), Some("willys"), Some("ok")));
            ids.extend(v["results"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()));
        }
        let expected: Vec<_> = (0..120).map(|i| format!("p{i}")).collect();
        assert_eq!(ids, expected, "every row, once, in order");
    }

    #[test]
    fn a_row_too_big_for_any_message_is_left_out() {
        let out = messages(&chain(vec![row(1, "Fanta"), row(2, &"x".repeat(MAX_MESSAGE)), row(3, "Fanta Zero")]));
        assert_eq!(out.len(), 1);
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        let ids: Vec<_> = v["results"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["p1", "p3"]);
    }

    #[test]
    fn area_is_not_sent_and_errors_are_cut_short() {
        assert!(messages(&SearchEvent::Area { center: LUND, radius_km: 1.5, stores: vec![] }).is_empty());
        assert_eq!(messages(&SearchEvent::Done), vec![r#"{"type":"done"}"#.to_string()]);
        let out = messages(&SearchEvent::Error { message: "ö".repeat(10_000) });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["type"], "error");
        assert_eq!(v["message"].as_str().unwrap().chars().count(), 500);
    }

    #[test]
    fn input_is_checked_like_the_page_commands() {
        assert!(query(" a ").is_err());
        assert_eq!(query(" fanta ").unwrap(), "fanta");
        assert_eq!(center(59.33, 18.07), LatLng { lat: 59.33, lng: 18.07 });
        // Berlin, the null island and a missing fix (NaN) all mean Lund.
        assert_eq!(center(52.52, 13.405), LUND);
        assert_eq!(center(0.0, 0.0), LUND);
        assert_eq!(center(f64::NAN, f64::NAN), LUND);
    }

    #[test]
    fn chains_that_never_answered_are_reported_busy() {
        let answered = BTreeSet::from([ChainId::Willys, ChainId::Lidl]);
        let late = unanswered(&answered);
        let chains: Vec<_> = late
            .iter()
            .map(|e| match e {
                SearchEvent::Chain { chain, status: ChainStatus::Throttled, .. } => *chain,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(chains, [ChainId::Hemkop, ChainId::Citygross, ChainId::Ica, ChainId::Coop]);
    }

    #[test]
    fn a_short_query_answers_with_an_error_and_nothing_else() {
        let mut sent = vec![];
        tauri::async_runtime::block_on(run("a", 55.7, 13.2, |m| {
            sent.push(m);
            true
        }));
        assert_eq!(sent.len(), 1);
        let v: Value = serde_json::from_str(&sent[0]).unwrap();
        assert_eq!(v, json!({ "type": "error", "message": "Search for at least 2 letters" }));
    }
}
