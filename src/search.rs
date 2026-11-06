//! `POST /search`: nearby posts, narrowed by category and text. The engine only
//! knows "near"; the filtering happens here, after the radius query.

use axum::{extract::State, Json};
use qhlv::AppState;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::post;

#[derive(Deserialize)]
pub struct SearchRequest {
    lat: f64,
    lng: f64,
    radius_km: f64,
    category: Option<String>,
    q: Option<String>,
}

pub async fn search(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> qhlv::Result<Json<Vec<Value>>> {
    let mut con = state.redis.clone();
    let viewport = json!({"lat": req.lat, "lng": req.lng, "radius_km": req.radius_km});
    let ids = state.space.query(&mut con, &viewport).await?;
    let posts = qhlv::get_objects(&mut con, &ids).await?;
    let views = posts
        .iter()
        .filter(|o| post::matches(&o.payload, req.category.as_deref(), req.q.as_deref()))
        .map(|o| qhlv::object_view(o, state.space.as_ref()))
        .collect();
    Ok(Json(views))
}
