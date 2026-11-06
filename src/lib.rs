//! sietesodas: anonymous local classifieds, built on the qhlv engine.

mod post;
mod search;

use axum::routing::post;
use axum::Router;
use qhlv::AppState;
use redis::aio::ConnectionManager;
use std::sync::Arc;

const DAY_SECS: i64 = 24 * 60 * 60;

/// A post lives 20 days; the first contact extends it straight to the 30-day cap.
fn config() -> qhlv::Config {
    qhlv::Config {
        inactivity_ttl_secs: 20 * DAY_SECS,
        hard_cap_secs: 30 * DAY_SECS,
        max_payload_bytes: 4096,
        activity_extends_to_cap: true,
    }
}

pub fn app_state(redis: ConnectionManager) -> AppState {
    AppState::new(
        redis,
        config(),
        Arc::new(qhlv::GeoSpace::from_env()),
        Arc::new(post::validate),
    )
}

/// The engine's routes plus `/search`.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/search", post(search::search))
        .with_state(state.clone())
        .merge(qhlv::router(state))
}
