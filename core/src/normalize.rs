//! TODO(port): src/lib/server/normalize.ts — price/size parsing and relevance.

use crate::types::{Match, Unit};

/// "1 299,50 kr" → 1299.5
pub fn parse_sek(_text: &str) -> Option<f64> {
    None
}

pub fn to_unit(_text: &str) -> Option<Unit> {
    None
}

/// "299,50 kr/kg" → (299.5, Kg)
pub fn parse_unit_price(_text: &str) -> Option<(f64, Unit)> {
    None
}

/// "500 g" → (0.5, Kg), "6 x 33 cl" → (1.98, L)
pub fn parse_size(_text: &str) -> Option<(f64, Unit)> {
    None
}

/// Comparison price from the item price and its size, rounded to öre.
pub fn unit_price_from_size(_price: f64, _size: Option<&str>) -> Option<(f64, Unit)> {
    None
}

/// 'Best': the product is the thing searched for; 'Mention': the query only flavours or
/// fills something else; 'Weak': the query only starts a longer word; None: no match.
pub fn relevance(_query: &str, _name: &str, _brand: Option<&str>, _categories: &[String]) -> Option<Match> {
    None
}
