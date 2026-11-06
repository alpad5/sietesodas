//! sietesodas: anonymous local classifieds, built on the qhlv engine.

mod guard;
mod post;
mod search;
mod web;

use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
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

/// The web page, `/categorias`, `/search`, `/desafio`, and the engine's routes
/// behind the spam guard. `pow_secret` signs the proof-of-work challenges.
pub fn router(state: AppState, pow_secret: &str) -> Router {
    let guard = guard::Guard::new(state.redis.clone(), pow_secret);
    let challenge = Router::new()
        .route("/desafio", get(guard::challenge))
        .with_state(guard.clone());
    let engine = qhlv::router(state.clone()).layer(from_fn_with_state(guard, guard::check));

    Router::new()
        .route("/", get(web::index))
        .route("/categorias", get(web::categorias))
        .route("/search", post(search::search))
        .with_state(state)
        .merge(challenge)
        .merge(engine)
}
