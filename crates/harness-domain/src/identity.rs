//! Public vacancy and employer identities, with byte-compatible legacy semantics.
use crate::{HarnessError as Error, Result};
use unicode_casefold::UnicodeCaseFold;
use url::Url;

pub fn casefold(text: &str) -> String {
    text.case_fold().collect()
}
pub fn company_key(company: &str) -> String {
    casefold(company)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}
pub fn normal_url(value: &str) -> Result<String> {
    let url = Url::parse(value).map_err(|_| {
        Error::new(
            "invalid_arguments",
            "URL: valid http(s) host and port required",
        )
    })?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::new(
            "invalid_arguments",
            "A public vacancy/evidence URL must use http(s), without credentials",
        ));
    }
    // Preserve original authority/path identity: WHATWG Url drops explicit default
    // ports and re-encodes paths, unlike the historical Python public contract.
    let (scheme, remainder) = value.split_once("://").ok_or_else(|| {
        Error::new(
            "invalid_arguments",
            "A public URL requires an explicit http(s) scheme",
        )
    })?;
    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_end];
    if authority.is_empty() || authority.chars().any(char::is_whitespace) {
        return Err(Error::new(
            "invalid_arguments",
            "A public URL requires a valid host",
        ));
    }
    let tail = &remainder[authority_end..];
    let path = tail
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, value)| {
            let key = key.to_lowercase();
            !value.is_empty()
                && !key.starts_with("utm_")
                && !["source", "ref", "gh_src"].contains(&key.as_str())
        })
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    pairs.sort();
    let query = pairs
        .iter()
        .map(|(key, value)| format!("{}={}", query_quote(key), query_quote(value)))
        .collect::<Vec<_>>()
        .join("&");
    Ok(format!(
        "{}://{}{}{}",
        scheme.to_lowercase(),
        authority.to_lowercase(),
        path,
        if query.is_empty() {
            String::new()
        } else {
            format!("?{query}")
        }
    ))
}
fn query_quote(text: &str) -> String {
    let mut result = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            result.push(byte as char);
        } else if byte == b' ' {
            result.push('+');
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    #[test]
    fn identities_match_python_vectors() {
        let vectors: Value =
            serde_json::from_str(include_str!("../tests/identity-vectors.json")).unwrap();
        for row in vectors["urls"].as_array().unwrap() {
            let input = row["input"].as_str().unwrap();
            if row.get("invalid") == Some(&Value::Bool(true)) {
                assert!(normal_url(input).is_err(), "{input}");
            } else {
                assert_eq!(
                    normal_url(input).unwrap(),
                    row["output"].as_str().unwrap(),
                    "{input}"
                );
            }
        }
        for row in vectors["companies"].as_array().unwrap() {
            assert_eq!(
                company_key(row["input"].as_str().unwrap()),
                row["output"].as_str().unwrap()
            );
        }
    }
}
