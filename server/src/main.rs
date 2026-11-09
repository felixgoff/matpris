//! Matpris's backend over HTTP: the routes the page calls in a browser (`/api/search`
//! as NDJSON, `/api/stores`, `/api/geocode`, `/api/image`), for development with
//! `bun run dev`, which proxies /api here. The apps call the same core through Tauri
//! commands instead. Listens on 127.0.0.1:8787 (PORT to change it).

mod ica_jar;

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Query;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use matpris_core::geo::in_sweden;
use matpris_core::stores::registry::{priced_area, with_map_only_stores};
use matpris_core::{geocode, image, LatLng};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

type Params = Query<HashMap<String, String>>;

fn bad(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, msg.to_string()).into_response()
}

fn center(p: &HashMap<String, String>) -> Result<LatLng, Response> {
    let num = |k: &str| p.get(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(f64::NAN);
    let c = LatLng { lat: num("lat"), lng: num("lng") };
    if in_sweden(c) { Ok(c) } else { Err(bad("lat/lng must be a location in Sweden")) }
}

/// An optional search radius the shopper picked, in km; absent means automatic.
fn radius(p: &HashMap<String, String>) -> Result<Option<f64>, Response> {
    match p.get("radius").filter(|r| !r.is_empty()) {
        None => Ok(None),
        Some(r) => match r.parse::<f64>() {
            Ok(km) if (0.5..=30.0).contains(&km) => Ok(Some(km)),
            _ => Err(bad("radius must be between 0.5 and 30 km")),
        },
    }
}

/// Newline-delimited JSON SearchEvents, so fast chains render first.
async fn search(Query(p): Params) -> Response {
    let q = p.get("q").map(|q| q.trim().to_string()).unwrap_or_default();
    if q.chars().count() < 2 {
        return bad("q must be at least 2 characters");
    }
    let (c, r) = match (center(&p), radius(&p)) {
        (Ok(c), Ok(r)) => (c, r),
        (Err(e), _) | (_, Err(e)) => return e,
    };
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    // Dropping the response (the page aborted) closes the channel, which ends the search.
    tokio::spawn(async move { matpris_core::search(&q, c, r, tx).await });
    let lines = ReceiverStream::new(rx).map(|e| {
        let mut line = serde_json::to_vec(&e).unwrap_or_default();
        line.push(b'\n');
        Ok::<_, std::convert::Infallible>(line)
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "application/x-ndjson; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from_stream(lines))
        .unwrap()
}

async fn stores(Query(p): Params) -> Response {
    let (c, r) = match (center(&p), radius(&p)) {
        (Ok(c), Ok(r)) => (c, r),
        (Err(e), _) | (_, Err(e)) => return e,
    };
    match priced_area(c, r).await {
        Ok(area) => {
            let full = with_map_only_stores(c, area).await;
            Json(serde_json::json!({ "center": c, "radiusKm": full.radius_km, "stores": full.stores })).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

async fn geocode_route(Query(p): Params) -> Response {
    let Some(q) = p.get("q").filter(|q| !q.trim().is_empty()) else { return bad("q is required") };
    match geocode::geocode(q).await {
        Ok(Some(place)) => Json(place).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Address not found").into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

async fn image_route(Query(p): Params) -> Response {
    let src = p.get("src").cloned().unwrap_or_default();
    if !image::allowed(&src) {
        return bad("src is not a product image host");
    }
    match image::fetch_image(&src).await {
        Ok((bytes, kind)) => {
            ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "public, max-age=86400".into())], bytes).into_response()
        }
        Err(_) => (StatusCode::BAD_GATEWAY, "image unavailable").into_response(),
    }
}

#[tokio::main]
async fn main() {
    matpris_core::chains::ica::set_cookie_jar(Arc::new(ica_jar::ScriptJar::discover()));
    let app = Router::new()
        .route("/api/search", get(search))
        .route("/api/stores", get(stores))
        .route("/api/geocode", get(geocode_route))
        .route("/api/image", get(image_route));
    let port = std::env::var("PORT").ok().and_then(|p| p.parse::<u16>().ok()).unwrap_or(8787);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await.expect("port in use?");
    eprintln!("Matpris backend on http://127.0.0.1:{port}/");
    axum::serve(listener, app).await.unwrap();
}
