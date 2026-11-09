use crate::types::LatLng;

const R: f64 = 6371.0; // km

/// Great-circle distance in km (haversine), like src/lib/geo.ts.
pub fn distance_km(a: LatLng, b: LatLng) -> f64 {
    let rad = std::f64::consts::PI / 180.0;
    let d_lat = (b.lat - a.lat) * rad;
    let d_lng = (b.lng - a.lng) * rad;
    let h = (d_lat / 2.0).sin().powi(2) + (a.lat * rad).cos() * (b.lat * rad).cos() * (d_lng / 2.0).sin().powi(2);
    2.0 * R * h.sqrt().asin()
}

/// Roughly Sweden's bounding box: the backend only searches inside it.
pub fn in_sweden(p: LatLng) -> bool {
    p.lat > 55.0 && p.lat < 69.1 && p.lng > 10.5 && p.lng < 24.2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lund_to_malmo() {
        let d = distance_km(LatLng { lat: 55.7047, lng: 13.1910 }, LatLng { lat: 55.6050, lng: 13.0038 });
        assert!((d - 16.0).abs() < 0.5, "{d}");
    }
}
