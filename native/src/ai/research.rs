//! URL identities for request-local research guards; never emitted to diagnostics.
use std::hash::{Hash, Hasher};
pub fn canonical_url(value: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.set_fragment(None);
    let mut pairs = url
        .query_pairs()
        .filter(|(key, _)| {
            let key = key.to_ascii_lowercase();
            !key.starts_with("utm_")
                && !matches!(
                    key.as_str(),
                    "fbclid" | "gclid" | "ot_source" | "src" | "referrer"
                )
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect::<Vec<_>>();
    pairs.sort();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(&pairs);
    }
    Some(url.to_string())
}
fn digest(value: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}
pub fn navigation_keys(value: &str) -> Vec<(u64, &'static str, u8)> {
    let Some(exact) = canonical_url(value) else {
        return vec![];
    };
    let Ok(url) = reqwest::Url::parse(&exact) else {
        return vec![];
    };
    let path = url.path().trim_end_matches('/').to_ascii_lowercase();
    let search = path == "/s"
        || path.contains("search")
        || matches!(
            url.host_str(),
            Some("www.google.com" | "www.bing.com" | "www.duckduckgo.com" | "duckduckgo.com")
        );
    let mut context = Vec::new();
    for (key, value) in url.query_pairs() {
        let key = key.to_ascii_lowercase();
        let field = match key.as_str() {
            "date" | "datetime" | "reservation_date" => "date",
            "covers" | "party_size" | "partysize" | "seats" => "party",
            "id" | "rid" | "restref" | "venue_id" | "restaurantid" | "restaurant_id"
            | "venueid" | "page" | "offset" | "cursor" => key.as_str(),
            _ => continue,
        };
        let value = if field == "date"
            && value.as_bytes().get(4) == Some(&b'-')
            && value.as_bytes().get(7) == Some(&b'-')
        {
            value.chars().take(10).collect::<String>()
        } else {
            value.into_owned()
        };
        context.push((field.to_owned(), value));
    }
    context.sort();
    let family = format!(
        "{}:{}:{}:{}:{:?}",
        url.scheme(),
        url.host_str().unwrap_or(""),
        url.port_or_known_default().unwrap_or(0),
        path,
        context
    );
    vec![
        (digest(&format!("exact:{exact}")), "exact", 5),
        (
            digest(&format!("family:{family}")),
            if search { "search" } else { "source" },
            if search { 12 } else { 8 },
        ),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn canonical_urls_drop_tracking_and_sort_queries_only() {
        assert_eq!(
            super::canonical_url(
                "https://example.com/book?covers=2&utm_source=test&date=2026-10-30#top"
            ),
            super::canonical_url("https://example.com/book?date=2026-10-30&covers=2")
        );
        assert_ne!(
            super::canonical_url("https://example.com/book?id=1"),
            super::canonical_url("https://example.com/book?id=2")
        );
    }
}
