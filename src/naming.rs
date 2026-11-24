//! Name normalization helpers.
//!
//! Spec names can contain spaces, accents or mixed casing
//! (e.g. "Détails de la commande", "orderId"). All generated identifiers go
//! through these helpers so output is deterministic and language-safe.

use unicode_normalization::UnicodeNormalization;

/// Strip accents (NFKD + drop combining marks); non-alphanumeric characters
/// become word separators. Mirrors Python: NFKD, drop chars with ccc != 0,
/// `re.split(r"[^0-9A-Za-z]+")` and join non-empty parts with a space.
pub fn normalize(text: &str) -> String {
    let mut out = String::new();
    let mut pending_sep = false;
    for c in text.nfkd() {
        if unicode_normalization::char::canonical_combining_class(c) != 0 {
            continue;
        }
        if c.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push(' ');
            }
            pending_sep = false;
            out.push(c);
        } else {
            pending_sep = true;
        }
    }
    out
}

/// Split an identifier into words, handling camelCase/PascalCase/ALL_CAPS too.
pub fn words(text: &str) -> Vec<String> {
    let n = normalize(text);
    if n.is_empty() {
        return vec![];
    }
    let chars: Vec<char> = n.chars().collect();
    let mut parts: Vec<String> = vec![];
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == ' ' {
            if !cur.is_empty() {
                parts.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if !cur.is_empty() {
            let prev = chars[i - 1];
            // (?<=[a-z0-9])(?=[A-Z])
            let rule1 = (prev.is_ascii_lowercase() || prev.is_ascii_digit()) && c.is_ascii_uppercase();
            // (?<=[A-Z])(?=[A-Z][a-z])
            let rule2 = prev.is_ascii_uppercase()
                && c.is_ascii_uppercase()
                && i + 1 < chars.len()
                && chars[i + 1].is_ascii_lowercase();
            if rule1 || rule2 {
                parts.push(std::mem::take(&mut cur));
            }
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
        .into_iter()
        .map(|p| {
            // Python: if len(p) > 1 and p.isupper(): p = p.lower()
            let chars: Vec<char> = p.chars().collect();
            let is_upper = chars.len() > 1
                && chars.iter().any(|c| c.is_ascii_uppercase())
                && !chars.iter().any(|c| c.is_ascii_lowercase());
            if is_upper { p.to_lowercase() } else { p }
        })
        .collect()
}

pub fn pascal(text: &str) -> String {
    let mut s = String::new();
    for p in words(text) {
        let mut ch = p.chars();
        if let Some(f) = ch.next() {
            s.extend(f.to_uppercase());
            s.push_str(ch.as_str());
        }
    }
    if s.is_empty() { "X".to_string() } else { s }
}

pub fn camel(text: &str) -> String {
    let p = pascal(text);
    let mut ch = p.chars();
    match ch.next() {
        Some(f) => f.to_lowercase().collect::<String>() + ch.as_str(),
        None => p,
    }
}

pub fn snake(text: &str) -> String {
    let w = words(text);
    let s = w.iter().map(|p| p.to_lowercase()).collect::<Vec<_>>().join("_");
    if s.is_empty() { "x".to_string() } else { s }
}

pub fn kebab(text: &str) -> String {
    let w = words(text);
    let s = w.iter().map(|p| p.to_lowercase()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "x".to_string() } else { s }
}

pub fn screaming(text: &str) -> String {
    let w = words(text);
    let s = w.iter().map(|p| p.to_uppercase()).collect::<Vec<_>>().join("_");
    if s.is_empty() { "X".to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_and_spaces() {
        assert_eq!(pascal("Détails de la commande"), "DetailsDeLaCommande");
        assert_eq!(snake("Détails de la commande"), "details_de_la_commande");
        assert_eq!(camel("orderId"), "orderId");
        assert_eq!(snake("orderId"), "order_id");
        assert_eq!(pascal("OrderStatus"), "OrderStatus");
        assert_eq!(screaming("order status"), "ORDER_STATUS");
        assert_eq!(kebab("Order Service API"), "order-service-api");
    }
}
