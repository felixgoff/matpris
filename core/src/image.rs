//! A product photo fetched by the backend, for drawing the shared answer image: a page
//! may only copy another site's image into a picture if that site allows it, and City
//! Gross's image server doesn't. Only the chains' image hosts are fetched.

use std::time::Duration;

use crate::http::{self, client};

const HOSTS: [&str; 6] = ["axfood.se", "citygross.se", "lidl.se", "cloudinary.com", "ica.se", "icanet.se"];

/// Whether `src` is an https URL on one of the chains' image hosts.
pub fn allowed(src: &str) -> bool {
    let Ok(u) = url::Url::parse(src) else { return false };
    let host = u.host_str().unwrap_or_default();
    u.scheme() == "https" && HOSTS.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

/// The image's bytes and content type.
pub async fn fetch_image(src: &str) -> http::Result<(Vec<u8>, String)> {
    if !allowed(src) {
        return Err(http::Error::Other("src is not a product image host".into()));
    }
    let res = client().get(src).timeout(Duration::from_secs(8)).send().await?;
    let kind = res.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    if !res.status().is_success() || !kind.starts_with("image/") {
        return Err(http::Error::Other("image unavailable".into()));
    }
    Ok((res.bytes().await?.to_vec(), kind))
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_chain_image_hosts() {
        assert!(super::allowed("https://assets.axfood.se/image/x.jpg"));
        assert!(!super::allowed("http://assets.axfood.se/image/x.jpg"));
        assert!(!super::allowed("https://evil.example/axfood.se.jpg"));
    }
}
