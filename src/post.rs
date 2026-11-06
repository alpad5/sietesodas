//! What a sietesodas post looks like. The engine stores the payload as opaque
//! JSON; this module is the only place that knows its shape.

use serde_json::Value;

pub const CATEGORIES: &[&str] = &[
    "guitars",
    "basses",
    "keyboards",
    "drums",
    "amps",
    "pedals",
    "recording",
    "pa-lighting",
    "other",
];

const MAX_TITLE: usize = 100;
const MAX_DESCRIPTION: usize = 2000;

/// Validate a post payload: `title`, `category`, optional `description` and `price`.
/// The payload is public (it is returned by search), so it never holds contact data.
pub fn validate(payload: &Value) -> Result<(), String> {
    let title = payload
        .get("title")
        .and_then(Value::as_str)
        .ok_or("title is required")?;
    if title.trim().is_empty() || title.chars().count() > MAX_TITLE {
        return Err(format!("title must be 1 to {MAX_TITLE} characters"));
    }

    let category = payload
        .get("category")
        .and_then(Value::as_str)
        .ok_or("category is required")?;
    if !CATEGORIES.contains(&category) {
        return Err(format!("category must be one of: {}", CATEGORIES.join(", ")));
    }

    if let Some(description) = payload.get("description") {
        let description = description.as_str().ok_or("description must be text")?;
        if description.chars().count() > MAX_DESCRIPTION {
            return Err(format!("description must be at most {MAX_DESCRIPTION} characters"));
        }
    }

    if let Some(price) = payload.get("price") {
        match price.as_f64() {
            Some(p) if p >= 0.0 => {}
            _ => return Err("price must be a number of 0 or more".into()),
        }
    }
    Ok(())
}

/// Does a post match the optional category and free-text filters? Every word of
/// `q` must appear (case-insensitive) in the title or description.
pub fn matches(payload: &Value, category: Option<&str>, q: Option<&str>) -> bool {
    if let Some(category) = category {
        if payload.get("category").and_then(Value::as_str) != Some(category) {
            return false;
        }
    }
    let Some(q) = q else { return true };
    let text = |key: &str| payload.get(key).and_then(Value::as_str).unwrap_or("");
    let haystack = format!("{} {}", text("title"), text("description")).to_lowercase();
    q.to_lowercase()
        .split_whitespace()
        .all(|term| haystack.contains(term))
}
