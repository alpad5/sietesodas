//! What a sietesodas post looks like. The engine stores the payload as opaque
//! JSON; this module is the only place that knows its shape.

use serde_json::Value;

/// Category id (stored in the post and used by the API) and its display label.
pub const CATEGORIES: &[(&str, &str)] = &[
    ("guitarras", "Guitarras"),
    ("bajos", "Bajos"),
    ("teclados", "Teclados"),
    ("bateria", "Batería y percusión"),
    ("amplificadores", "Amplificadores"),
    ("pedales", "Pedales y efectos"),
    ("grabacion", "Grabación"),
    ("sonido-e-iluminacion", "Sonido e iluminación"),
    ("otros", "Otros"),
];

const MAX_TITLE: usize = 100;
const MAX_DESCRIPTION: usize = 2000;

/// Validate a post payload: `title`, `category`, optional `description` and `price`.
/// Error messages are user-facing, so they are in Spanish.
/// The payload is public (it is returned by search), so it never holds contact data.
pub fn validate(payload: &Value) -> Result<(), String> {
    let title = payload
        .get("title")
        .and_then(Value::as_str)
        .ok_or("falta el título")?;
    if title.trim().is_empty() || title.chars().count() > MAX_TITLE {
        return Err(format!("el título debe tener entre 1 y {MAX_TITLE} caracteres"));
    }

    let category = payload
        .get("category")
        .and_then(Value::as_str)
        .ok_or("falta la categoría")?;
    if !CATEGORIES.iter().any(|(id, _)| *id == category) {
        return Err("categoría no válida".into());
    }

    if let Some(description) = payload.get("description") {
        let description = description.as_str().ok_or("la descripción debe ser texto")?;
        if description.chars().count() > MAX_DESCRIPTION {
            return Err(format!("la descripción admite como máximo {MAX_DESCRIPTION} caracteres"));
        }
    }

    if let Some(price) = payload.get("price") {
        match price.as_f64() {
            Some(p) if p >= 0.0 => {}
            _ => return Err("el precio debe ser un número igual o mayor que 0".into()),
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
