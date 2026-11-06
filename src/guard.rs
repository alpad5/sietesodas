//! Spam defence for the engine's routes: a proof-of-work on publishing plus a
//! per-IP rate limit. Everything the app does not need is closed off.
//!
//! Flow: the page fetches a signed challenge (`GET /desafio`), solves it in the
//! browser, and sends the solution in the `x-pow` header with `POST /objects`.
//! Each challenge works once; solutions are remembered in Redis until they expire.

use altcha::{
    create_challenge, verify_solution, Challenge, CreateChallengeOptions, Payload,
    VerifySolutionOptions,
};
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use redis::aio::ConnectionManager;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub const POW_HEADER: &str = "x-pow";

/// PBKDF2 iterations per attempt. With a one-byte prefix a solve takes about 256
/// attempts, around a second in a browser, and a single check on the server.
const COST: u32 = 5_000;
/// How long a challenge stays valid, in seconds. Also the lifetime of the
/// single-use marker, so a solved challenge cannot be replayed while valid.
const CHALLENGE_TTL_SECS: i64 = 600;
/// Posts allowed per IP per window.
const PUBLISH_LIMIT: i64 = 10;
const PUBLISH_WINDOW_SECS: i64 = 60 * 60;

#[derive(Clone)]
pub struct Guard {
    redis: ConnectionManager,
    secret: Arc<str>,
}

impl Guard {
    pub fn new(redis: ConnectionManager, secret: &str) -> Self {
        Self { redis, secret: secret.into() }
    }
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

/// `GET /desafio`: a fresh signed challenge.
pub async fn challenge(State(guard): State<Guard>) -> Result<Json<Challenge>, StatusCode> {
    create_challenge(CreateChallengeOptions {
        cost: COST,
        expires_at: Some(now_secs() + CHALLENGE_TTL_SECS),
        hmac_signature_secret: Some(guard.secret.to_string()),
        ..Default::default()
    })
    .map(Json)
    .map_err(|e| {
        tracing::warn!("cannot create challenge: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// Middleware for the engine router. Only publishing, editing and deleting are
/// reachable; publishing needs a valid proof-of-work and is rate limited.
pub async fn check(
    State(guard): State<Guard>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path();
    let is_publish = req.method() == Method::POST && path == "/objects";
    let is_manage = matches!(*req.method(), Method::PUT | Method::DELETE)
        && path.strip_prefix("/objects/").is_some_and(|id| !id.is_empty() && !id.contains('/'));

    if is_manage {
        return next.run(req).await; // protected by the owner token
    }
    if !is_publish {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Copy the header out: the request body is not Sync, so it cannot be held across awaits.
    let pow = req.headers().get(POW_HEADER).and_then(|v| v.to_str().ok()).map(str::to_owned);
    match admit(&guard, addr, pow).await {
        Ok(()) => next.run(req).await,
        Err(response) => response,
    }
}

/// Verify the proof-of-work, burn it, and count the publish against the IP.
async fn admit(guard: &Guard, addr: SocketAddr, pow: Option<String>) -> Result<(), Response> {
    let refuse = |status, msg: &'static str| Err((status, msg).into_response());

    let Some(payload) = pow.and_then(|v| serde_json::from_str::<Payload>(&v).ok()) else {
        return refuse(StatusCode::FORBIDDEN, "falta la verificación");
    };

    let verified = verify_solution(VerifySolutionOptions::new(
        &payload.challenge,
        &payload.solution,
        guard.secret.to_string(),
    ))
    .is_ok_and(|r| r.verified);
    let Some(signature) = payload.challenge.signature.filter(|_| verified) else {
        return refuse(StatusCode::FORBIDDEN, "verificación no válida");
    };

    let internal = |e: redis::RedisError| {
        tracing::warn!("guard redis error: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, "error interno").into_response()
    };
    let mut con = guard.redis.clone();

    // Single use: the first request to claim this challenge wins.
    let claimed: Option<String> = redis::cmd("SET")
        .arg(format!("pow:{signature}"))
        .arg(1)
        .arg("NX")
        .arg("EX")
        .arg(CHALLENGE_TTL_SECS)
        .query_async(&mut con)
        .await
        .map_err(internal)?;
    if claimed.is_none() {
        return refuse(StatusCode::FORBIDDEN, "verificación ya utilizada");
    }

    let key = format!("rate:publish:{}", addr.ip());
    let count: i64 = redis::cmd("INCR").arg(&key).query_async(&mut con).await.map_err(internal)?;
    if count == 1 {
        redis::cmd("EXPIRE")
            .arg(&key)
            .arg(PUBLISH_WINDOW_SECS)
            .query_async::<_, ()>(&mut con)
            .await
            .map_err(internal)?;
    }
    if count > PUBLISH_LIMIT {
        return refuse(StatusCode::TOO_MANY_REQUESTS, "demasiados anuncios, inténtalo más tarde");
    }
    Ok(())
}
