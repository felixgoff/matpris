//! Price/size parsing and relevance, ported from src/lib/server/normalize.ts.
//!
//! JS regex details are matched on purpose: `\d` is ASCII only, `\s` is JavaScript's
//! whitespace set (with NBSP and BOM, without U+0085) and `\b` is the ASCII word boundary
//! of a non-`u` JS regex (so "år" followed by "ä" still ends a word there).

use std::sync::LazyLock;

use regex::Regex;

use crate::types::{Match, Unit};

/// JavaScript's `\s`, as a regex character class body.
const JS_SPACE: &str = r"\t\n\x0B\x0C\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";

fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\x0B' | '\x0C' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}'
            | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// `Math.round(x * 100) / 100`: JS rounds halves toward +∞ (`-2.5` → `-2`), unlike
/// `f64::round`, which rounds them away from zero.
pub fn round2(x: f64) -> f64 {
    js_round(x * 100.0) / 100.0
}

fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    // Exact: subtracting the floor loses nothing.
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// JS `String.length`: UTF-16 code units.
fn js_len(s: &str) -> usize {
    s.encode_utf16().count()
}

static SEK_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-?[0-9]+(?:[.,][0-9]+)?").unwrap());

/// "1 299,50 kr" → 1299.5
pub fn parse_sek(text: &str) -> Option<f64> {
    if text.is_empty() {
        return None;
    }
    let compact: String = text.chars().filter(|&c| !is_js_space(c)).collect();
    let m = SEK_NUMBER.find(&compact)?;
    m.as_str().replacen(',', ".", 1).parse().ok()
}

pub fn to_unit(text: &str) -> Option<Unit> {
    match text.trim_matches(is_js_space).to_lowercase().as_str() {
        "kg" => Some(Unit::Kg),
        "l" | "liter" | "lit" => Some(Unit::L),
        "st" | "styck" => Some(Unit::St),
        _ => None,
    }
}

static UNIT_PRICE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"([0-9{s} .,]+)[{s}]*kr[{s}]*/[{s}]*([a-zA-Z]+)", s = JS_SPACE)).unwrap()
});

/// "299,50 kr/kg" → (299.5, Kg)
pub fn parse_unit_price(text: &str) -> Option<(f64, Unit)> {
    let m = UNIT_PRICE.captures(text)?;
    let unit = to_unit(&m[2])?;
    let unit_price = parse_sek(&m[1])?;
    Some((unit_price, unit))
}

static SIZE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?:([0-9]+)[{s}]*x[{s}]*)?([0-9]+(?:\.[0-9]+)?)[{s}]*(kg|g|l|dl|cl|ml|st)(?-u:\b)",
        s = JS_SPACE
    ))
    .unwrap()
});

/// "500 g" → (0.5, Kg), "6 x 33 cl" → (1.98, L)
pub fn parse_size(text: &str) -> Option<(f64, Unit)> {
    let lower = text.to_lowercase().replacen(',', ".", 1);
    let m = SIZE.captures(&lower)?;
    let count: f64 = match m.get(1) {
        Some(c) => c.as_str().parse().ok()?,
        None => 1.0,
    };
    let n = m[2].parse::<f64>().ok()? * count;
    let (f, unit) = match &m[3] {
        "kg" => (1.0, Unit::Kg),
        "g" => (0.001, Unit::Kg),
        "l" => (1.0, Unit::L),
        "dl" => (0.1, Unit::L),
        "cl" => (0.01, Unit::L),
        "ml" => (0.001, Unit::L),
        "st" => (1.0, Unit::St),
        _ => return None,
    };
    (n > 0.0).then_some((n * f, unit))
}

/// Comparison price from the item price and its size, rounded to öre.
pub fn unit_price_from_size(price: f64, size: Option<&str>) -> Option<(f64, Unit)> {
    let (amount, unit) = parse_size(size?)?;
    Some((round2(price / amount), unit))
}

static IN_WORD_HYPHEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\p{L})-(\p{L})").unwrap());
static NON_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\p{L}\p{N}]+").unwrap());

// Hyphens inside a word are dropped first, so "Yo-ghurt" reads as "yoghurt".
fn tokens(s: &str) -> Vec<String> {
    let lower = s.to_lowercase();
    let joined = IN_WORD_HYPHEN.replace_all(&lower, "$1$2");
    NON_WORD.split(&joined).filter(|w| !w.is_empty()).map(str::to_owned).collect()
}

// Swedish plural/definite endings, so "banan" matches "bananer" and "mjölk" "mjölken".
const ENDINGS: [&str; 14] = ["", "r", "n", "s", "t", "er", "ar", "or", "en", "et", "na", "erna", "arna", "orna"];

// The reverse, for a query typed in plural or definite form: each ending with what may
// replace it, so "bananer" finds "Banan", "gurkor" "Gurka" and "äpplen" "Äpple".
const INFLECTIONS: [(&str, &[&str]); 10] = [
    ("orna", &["a"]),
    ("arna", &["", "e"]),
    ("erna", &[""]),
    ("or", &["a"]),
    ("ar", &["", "e"]),
    ("er", &["", "e"]),
    ("en", &["e", ""]),
    ("et", &[""]),
    ("na", &[""]),
    ("n", &[""]),
];

/// Possible base forms of a query word: "bananer" → ["banan", "banane"].
/// In first-found order, without duplicates and without the word itself (like the TS Set).
pub fn base_forms(word: &str) -> Vec<String> {
    let mut forms: Vec<String> = Vec::new();
    for (end, replacements) in INFLECTIONS {
        if word.ends_with(end) && js_len(word) - js_len(end) >= 3 {
            let stem = &word[..word.len() - end.len()];
            for r in replacements {
                let form = format!("{stem}{r}");
                if form != word && !forms.contains(&form) {
                    forms.push(form);
                }
            }
        }
    }
    forms
}

/// 2 = matches strongly, 1 = only weakly (prefix of a longer word), 0 = no match.
pub type MatchQuality = u8;

fn form_quality(q: &str, word: &str) -> MatchQuality {
    for end in ENDINGS {
        let form = format!("{q}{end}");
        // Whole word, or the head of a compound ("mellanmjölk", "brieost").
        if word == form || (js_len(q) >= 3 && word.ends_with(&form)) {
            return 2;
        }
    }
    // Merely starts with it ("fanta" → "fantasty", "kaffe" → "kaffefilter"): weak.
    if word.starts_with(q) {
        1
    } else {
        0
    }
}

fn word_quality(q: &str, word: &str) -> MatchQuality {
    let direct = form_quality(q, word);
    if direct != 0 {
        return direct;
    }
    // Base forms only rescue words that don't match at all, so "banan" stays a weak
    // match for "Banana" instead of reaching it through "bana" + "na".
    if base_forms(q).iter().any(|b| form_quality(b, word) == 2) {
        2
    } else {
        0
    }
}

/// How well a product matches the query: 2 = every query word matches strongly,
/// 1 = some only weakly (prefix of a longer word), 0 = no match. Empty fields are ignored.
pub fn match_quality(query: &str, fields: &[&str]) -> MatchQuality {
    let joined = fields.iter().filter(|f| !f.is_empty()).copied().collect::<Vec<_>>().join(" ");
    let words = tokens(&joined);
    let mut worst: MatchQuality = 2;
    for q in tokens(query) {
        let mut best: MatchQuality = 0;
        for w in &words {
            best = best.max(word_quality(&q, w));
            if best == 2 {
                break;
            }
        }
        worst = worst.min(best);
        if worst == 0 {
            break;
        }
    }
    worst
}

// Words that name a kind of product. When a name has one the query didn't ask for,
// the query word is usually a flavour or an ingredient: "Banan & Vanilj Yoghurt".
// shortcut: a hand-kept list; replace it with the chains' category data if they expose it per product.
const PRODUCT_TYPES: &[&str] = &[
    "yoghurt", "kvarg", "smoothie", "juice", "nektar", "dryck", "läsk", "saft", "glass", "sorbet", "gröt",
    "välling", "puré", "bar", "chips", "snacks", "sticks", "puffs", "kex", "kaka", "kakor", "bulle", "bullar",
    "muffins", "paj", "tårta", "godis", "choklad", "kola", "sylt", "marmelad", "müsli", "musli", "granola",
    "flingor", "pulver", "shake", "whey", "snus", "sås", "soppa", "dressing", "rulle", "toffee", "te",
    "pudding", "kräm", "mjölk", "ost", "bröd", "korv", "pålägg", "mousse", "cream", "split",
];

fn is_product_type(word: &str) -> bool {
    // Short words only as whole words, so "te" doesn't match every word ending in "te".
    PRODUCT_TYPES.iter().any(|t| word == *t || (js_len(t) >= 4 && word.ends_with(t)))
}

// Sweets named after what they look or taste like: "Skumbanan", "Chokladbanan".
const SWEET_PREFIXES: [&str; 6] = ["skum", "choklad", "gelé", "lakrits", "marsipan", "godis"];

static BABY_FOOD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?-u:\b)från [0-9]+ (?:månader|mån|år)(?-u:\b)|(?-u:\b)[0-9]+[{s}]*-[{s}]*[0-9]+[{s}]*år(?-u:\b)",
        s = JS_SPACE
    ))
    .unwrap()
});

/// 'Best': the product is the thing searched for ("Banan Klass 1" for "banan").
/// 'Mention': the query only flavours or fills something else ("Banan & Vanilj Yoghurt", baby food).
/// 'Weak': the query only starts a longer word ("Fantasty" for "fanta"). None: no match.
///
/// `categories` (broad to specific, e.g. ["Skafferiet", "Kaffe, te & choklad", "Bryggkaffe"])
/// come from chains that publish them (City Gross). The most specific one counts as part of
/// the name, since names there often leave out the product type ("Originalet Mellanrost").
pub fn relevance(query: &str, name: &str, brand: Option<&str>, categories: &[String]) -> Option<Match> {
    let specific = categories.last().map(String::as_str).filter(|s| !s.is_empty());
    let quality = match_quality(query, &[name, brand.unwrap_or(""), specific.unwrap_or("")]);
    match quality {
        0 => return None,
        1 => return Some(Match::Weak),
        _ => {}
    }
    let asked = tokens(query);
    let is_asked = |w: &str| asked.iter().any(|q| word_quality(q, w) == 2);
    // A search for a brand ("fanta") asks for that brand's products, whatever type word
    // their names carry ("Orange Fanta Läsk"): they are the product, not a flavour of läsk.
    let brand_words = tokens(brand.unwrap_or(""));
    if !brand_words.is_empty() && asked.iter().all(|q| brand_words.iter().any(|w| word_quality(q, w) == 2)) {
        return Some(Match::Best);
    }
    if let Some(specific) = specific {
        if tokens(specific).iter().any(|w| is_asked(w)) {
            return Some(Match::Best);
        }
    }
    // The broader levels name the shelf ("Godis", "Barnmat"); the specific one can be a
    // buying code ("DJ FRUKT/BÄR/JUICE") that would misfire here.
    let broader = categories.len().saturating_sub(1);
    let shelf = tokens(&categories[..broader].join(" "));
    if shelf.iter().filter(|w| !is_asked(w)).any(|w| is_product_type(w) || w.starts_with("barn")) {
        return Some(Match::Mention);
    }
    let words = tokens(name);
    let other_is_type = words.iter().filter(|w| !is_asked(w)).any(|w| is_product_type(w));
    let sweet = words
        .iter()
        .any(|w| is_asked(w) && SWEET_PREFIXES.iter().any(|p| w.starts_with(p) && !asked.contains(w)));
    Some(if sweet || other_is_type || BABY_FOOD.is_match(name) { Match::Mention } else { Match::Best })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cats(c: &[&str]) -> Vec<String> {
        c.iter().map(|s| s.to_string()).collect()
    }

    fn rel(query: &str, name: &str) -> Option<Match> {
        relevance(query, name, None, &[])
    }

    // describe('parseSek')
    #[test]
    fn parse_sek_handles_swedish_formatting() {
        assert_eq!(parse_sek("16,50 kr"), Some(16.5));
        assert_eq!(parse_sek("1 299,00 kr"), Some(1299.0));
        assert_eq!(parse_sek("72.9"), Some(72.9));
        assert_eq!(parse_sek(""), None);
    }

    #[test]
    fn parse_sek_drops_nbsp_and_reads_negatives() {
        assert_eq!(parse_sek("1\u{a0}299,50\u{a0}kr"), Some(1299.5));
        assert_eq!(parse_sek("-5,50"), Some(-5.5));
        assert_eq!(parse_sek("kr"), None);
        assert_eq!(parse_sek(" "), None);
    }

    // describe('parseUnitPrice')
    #[test]
    fn parse_unit_price_reads_kr_unit_strings() {
        assert_eq!(parse_unit_price("299,50 kr/kg"), Some((299.5, Unit::Kg)));
        assert_eq!(parse_unit_price("117,16 kr/l"), Some((117.16, Unit::L)));
        assert_eq!(parse_unit_price("nothing"), None);
    }

    #[test]
    fn parse_unit_price_aliases_and_spacing() {
        assert_eq!(parse_unit_price("Jmf-pris 1\u{a0}299,00 kr / Styck"), Some((1299.0, Unit::St)));
        assert_eq!(parse_unit_price("12,00 kr/liter"), Some((12.0, Unit::L)));
        assert_eq!(parse_unit_price("12,00 kr/förp"), None);
    }

    #[test]
    fn to_unit_aliases() {
        assert_eq!(to_unit(" KG "), Some(Unit::Kg));
        assert_eq!(to_unit("lit"), Some(Unit::L));
        assert_eq!(to_unit("styck"), Some(Unit::St));
        assert_eq!(to_unit("g"), None);
        assert_eq!(to_unit(""), None);
    }

    // describe('parseSize')
    #[test]
    fn parse_size_converts_to_kg_l() {
        assert_eq!(parse_size("500 g"), Some((0.5, Unit::Kg)));
        assert_eq!(parse_size("1,5l"), Some((1.5, Unit::L)));
        let (amount, unit) = parse_size("6 x 33 cl").unwrap();
        assert!((amount - 1.98).abs() < 0.005);
        assert_eq!(unit, Unit::L);
        assert_eq!(parse_size("ca 1 kg"), Some((1.0, Unit::Kg)));
    }

    #[test]
    fn parse_size_computes_unit_price() {
        assert_eq!(unit_price_from_size(59.9, Some("200 g")), Some((299.5, Unit::Kg)));
    }

    #[test]
    fn parse_size_word_boundary_is_ascii_like_js() {
        // "l" followed by a letter is not a unit; the search moves on.
        assert_eq!(parse_size("5 lit 300 g"), Some((0.3, Unit::Kg)));
        // A non-ASCII letter ends the word in a non-`u` JS regex.
        assert_eq!(parse_size("2 lä"), Some((2.0, Unit::L)));
        assert_eq!(parse_size("0 g"), None);
        assert_eq!(parse_size("ingen"), None);
        assert_eq!(unit_price_from_size(10.0, None), None);
        assert_eq!(parse_size("4 ST"), Some((4.0, Unit::St)));
    }

    #[test]
    fn round2_rounds_halves_up_like_math_round() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(-2.6), -3.0);
        assert_eq!(round2(1.0 / 3.0), 0.33);
    }

    // describe('matchQuality')
    #[test]
    fn match_quality_whole_words_and_compound_heads() {
        assert_eq!(match_quality("mjölk", &["Mellanmjölk 1,5%"]), 2);
        assert_eq!(match_quality("ost", &["Brieost"]), 2);
        assert_eq!(match_quality("banan", &["Bananer Eko"]), 2);
        assert_eq!(match_quality("fanta", &["Orange Zero Läsk Pet", "Fanta"]), 2);
        assert_eq!(match_quality("kaffe bryggkaffe", &["Bryggkaffe Mellanrost"]), 2);
    }

    #[test]
    fn match_quality_prefix_is_weak() {
        assert_eq!(match_quality("fanta", &["Köttbuljong Pulver", "Fantasty"]), 1);
        assert_eq!(match_quality("kaff", &["Bryggkaffe", "Zoégas"]), 0);
        assert_eq!(match_quality("kaffe", &["Kaffefilter"]), 1);
    }

    #[test]
    fn match_quality_rejects_unrelated() {
        assert_eq!(match_quality("banan", &["Päron"]), 0);
        assert_eq!(match_quality("ost", &["Postej"]), 0);
        assert_eq!(match_quality("ä", &["Päron"]), 0);
    }

    #[test]
    fn tokens_join_hyphenated_words() {
        assert_eq!(tokens("Banan & Vanilj Yo-ghurt 0,1%"), ["banan", "vanilj", "yoghurt", "0", "1"]);
        assert_eq!(tokens("a-b-c"), ["ab", "c"]);
        assert_eq!(tokens("ÄPPLE–päron"), ["äpple", "päron"]);
        assert!(tokens("").is_empty());
    }

    // describe('baseForms')
    #[test]
    fn base_forms_undo_plural_and_definite_endings() {
        assert!(base_forms("bananer").contains(&"banan".to_string()));
        assert!(base_forms("gurkor").contains(&"gurka".to_string()));
        assert!(base_forms("äpplen").contains(&"äpple".to_string()));
        assert!(base_forms("mjölken").contains(&"mjölk".to_string()));
        assert!(base_forms("ägg").is_empty());
        assert_eq!(base_forms("bananer"), ["banan", "banane"]);
    }

    // describe('matchQuality with inflected queries')
    #[test]
    fn match_quality_finds_base_form_of_plural_query() {
        assert_eq!(match_quality("bananer", &["Banan Klass 1"]), 2);
        assert_eq!(match_quality("tomater", &["Tomat Klass 1"]), 2);
        assert_eq!(match_quality("gurkor", &["Gurka Klass 1"]), 2);
    }

    #[test]
    fn match_quality_banan_weak_for_banana() {
        assert_eq!(match_quality("banan", &["Banana Skids"]), 1);
    }

    // describe('relevance')
    #[test]
    fn relevance_product_above_what_it_flavours() {
        assert_eq!(rel("banan", "Banan Klass 1"), Some(Match::Best));
        assert_eq!(rel("bananer", "Banan Eko Klass 1"), Some(Match::Best));
        assert_eq!(rel("banan", "Banan & Vanilj Yo-ghurt 0,1%"), Some(Match::Mention));
        assert_eq!(rel("banan", "Mango Banan Apelsin Drickyoghurt"), Some(Match::Mention));
        assert_eq!(rel("banan", "Fruktsmoothie Äpple & Banan Från 6 Månader"), Some(Match::Mention));
        assert_eq!(rel("banan", "Bananchips"), Some(Match::Weak));
        assert_eq!(rel("bananer", "Jordgubb Skumbananer Sura"), Some(Match::Mention));
        assert_eq!(rel("skumbanan", "Skumbanan"), Some(Match::Best));
        assert_eq!(rel("banan", "Päron"), None);
    }

    #[test]
    fn relevance_asked_product_type_is_the_product() {
        assert_eq!(rel("banan yoghurt", "Banan & Vanilj Yo-ghurt 0,1%"), Some(Match::Best));
        assert_eq!(rel("mjölk", "Mellanmjölk Längre Hållbarhet 1,5%"), Some(Match::Best));
        assert_eq!(rel("kaffe", "Originalet Mellanrost Bryggkaffe"), Some(Match::Best));
    }

    #[test]
    fn relevance_baby_food_age_ranges() {
        assert_eq!(rel("banan", "Banan Gröt 1-3 år"), Some(Match::Mention));
        assert_eq!(rel("banan", "Banan Mos 1 - 3 År"), Some(Match::Mention));
        assert_eq!(rel("banan", "Banan Mos från 8 mån"), Some(Match::Mention));
    }

    // describe('relevance for a brand search')
    #[test]
    fn relevance_brand_products_are_the_product() {
        assert_eq!(relevance("fanta", "Orange Fanta Läsk, Pet", Some("Fanta"), &[]), Some(Match::Best));
        assert_eq!(
            relevance("fanta", "Orange Zero", Some("Fanta Zero"), &cats(&["Dryck", "Läsk", "Läsk PET"])),
            Some(Match::Best)
        );
        // Only whole words of the brand: "Fantasty" stock cubes stay a weak prefix match.
        assert_eq!(relevance("fanta", "Köttbuljong Pulver", Some("Fantasty"), &[]), Some(Match::Weak));
        // Not a brand search: banana yoghurt from Arla is still only flavoured.
        assert_eq!(relevance("banan", "Banan & Vanilj Yo-ghurt 0,1%", Some("Arla"), &[]), Some(Match::Mention));
    }

    // describe('relevance with chain categories')
    #[test]
    fn relevance_specific_category_counts_as_name() {
        let coffee = cats(&["Skafferiet", "Kaffe, te & choklad", "Bryggkaffe"]);
        assert_eq!(relevance("kaffe", "Originalet Mellanrost", Some("Gevalia"), &coffee), Some(Match::Best));
        assert_eq!(
            relevance("banan", "Banan Eko", None, &cats(&["Frukt & grönt", "Frukt", "Bananer"])),
            Some(Match::Best)
        );
        assert_eq!(
            relevance("kaffe", "Earl Grey", Some("Twinings"), &cats(&["Skafferiet", "Kaffe, te & choklad", "Te"])),
            None
        );
    }

    #[test]
    fn relevance_folds_sweets_yoghurt_baby_food_by_shelf() {
        assert_eq!(
            relevance("banan", "Chokladbanan", Some("Aroma"), &cats(&["Godis", "Choklad", "Chokladpåsar"])),
            Some(Match::Mention)
        );
        assert_eq!(
            relevance(
                "banan",
                "Jordgubb Banan",
                Some("Danonino"),
                &cats(&["Mejeri, ost & ägg", "Yoghurt", "Smaksatt yoghurt"])
            ),
            Some(Match::Mention)
        );
        assert_eq!(
            relevance("banan", "Mango & Banan Puré", Some("Semper"), &cats(&["Barn", "Barnmat", "BARNMAT, MAT FÖR BARN"])),
            Some(Match::Mention)
        );
        assert_eq!(
            relevance(
                "banan",
                "Banan Skivad",
                Some("Eldorado"),
                &cats(&["Fryst", "Frysta bär & frukt", "DJ FRUKT/BÄR/JUICE"])
            ),
            Some(Match::Best)
        );
    }
}
