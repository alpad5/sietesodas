//! The web page and the data it needs. The page is one static file; it talks to
//! the same JSON API as any other client.

use axum::response::Html;
use axum::Json;
use serde_json::{json, Value};

use crate::post::CATEGORIES;

pub async fn index() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}

/// `GET /categorias`: the single source of categories for the page.
pub async fn categorias() -> Json<Value> {
    Json(CATEGORIES.iter().map(|(id, label)| json!({"id": id, "nombre": label})).collect())
}
