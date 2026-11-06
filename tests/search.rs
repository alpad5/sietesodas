//! End-to-end: publish posts behind the spam guard and search them, against
//! throwaway redis-servers (ports 6392 and 6394). Requires `redis-server` on PATH.

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use tower::ServiceExt;

struct Redis(Child);
impl Drop for Redis {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

async fn start(port: u16) -> (Redis, axum::Router, redis::aio::ConnectionManager) {
    let child = Command::new("redis-server")
        .args(["--port", &port.to_string(), "--save", "", "--appendonly", "no"])
        .stdout(Stdio::null())
        .spawn()
        .expect("redis-server on PATH");
    let guard = Redis(child);
    let client = redis::Client::open(format!("redis://127.0.0.1:{port}/")).unwrap();
    let mut con = None;
    for _ in 0..50 {
        if let Ok(c) = redis::aio::ConnectionManager::new(client.clone()).await {
            con = Some(c);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let con = con.expect("redis did not start");
    let router = sietesodas::router(sietesodas::app_state(con.clone()), "test-secret")
        .layer(MockConnectInfo(SocketAddr::from(([203, 0, 113, 7], 5000))));
    (guard, router, con)
}

async fn post(app: &axum::Router, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, "POST", uri, body, None).await
}

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Value,
    pow: Option<String>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(pow) = pow {
        req = req.header("x-pow", pow);
    }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// Fetch a challenge and solve it, returning the value for the `x-pow` header.
async fn solved_pow(app: &axum::Router) -> String {
    let (status, text) = get(app, "/desafio").await;
    assert_eq!(status, StatusCode::OK);
    let challenge: altcha::Challenge = serde_json::from_str(&text).unwrap();
    let solution = altcha::solve_challenge(altcha::SolveChallengeOptions::new(&challenge))
        .unwrap()
        .expect("challenge solved");
    serde_json::to_string(&altcha::Payload { challenge, solution }).unwrap()
}

/// Publish a post the way the page does: solve a challenge first.
async fn publish_post(app: &axum::Router, body: Value) -> (StatusCode, Value) {
    let pow = solved_pow(app).await;
    send(app, "POST", "/objects", body, Some(pow)).await
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, String) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn publish(title: &str, category: &str) -> Value {
    json!({
        "payload": {"title": title, "category": category, "description": "great shape", "price": 250},
        "locator": {"lat": 40.0, "lng": -3.0},
        "owned": true
    })
}

fn search(extra: Value) -> Value {
    let mut body = json!({"lat": 40.0, "lng": -3.0, "radius_km": 5.0});
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    body
}

fn titles(results: &Value) -> Vec<String> {
    let mut t: Vec<String> = results
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["payload"]["title"].as_str().unwrap().to_string())
        .collect();
    t.sort();
    t
}

#[tokio::test]
async fn publish_validate_and_search() {
    let (_redis, app, mut con) = start(6392).await;

    for (title, category) in [
        ("Fender Stratocaster", "guitarras"),
        ("Yamaha acoustic", "guitarras"),
        ("Marshall stack", "amplificadores"),
    ] {
        let (status, created) = publish_post(&app, publish(title, category)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(created["owner_token"].is_string());
    }

    // The page and its category list are served in Spanish.
    let (status, page) = get(&app, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("<html lang=\"es\">"));
    let (_, cats) = get(&app, "/categorias").await;
    let cats: Value = serde_json::from_str(&cats).unwrap();
    assert_eq!(cats[0], json!({"id": "guitarras", "nombre": "Guitarras"}));

    // Bad category and missing title are rejected by the post validator.
    let (status, _) = publish_post(&app, publish("Tractor", "vehiculos")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let bad = json!({"payload": {"category": "guitarras"}, "locator": {"lat": 40.0, "lng": -3.0}});
    let (status, _) = publish_post(&app, bad).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Radius only, category, text, and both combined.
    let (_, all) = post(&app, "/search", search(json!({}))).await;
    assert_eq!(all.as_array().unwrap().len(), 3);
    let (_, guitars) = post(&app, "/search", search(json!({"category": "guitarras"}))).await;
    assert_eq!(titles(&guitars), ["Fender Stratocaster", "Yamaha acoustic"]);
    let (_, fender) = post(&app, "/search", search(json!({"q": "FENDER strato"}))).await;
    assert_eq!(titles(&fender), ["Fender Stratocaster"]);
    let (_, none) = post(&app, "/search", search(json!({"category": "amplificadores", "q": "fender"}))).await;
    assert!(none.as_array().unwrap().is_empty());

    // Far away finds nothing.
    let (_, far) = post(&app, "/search", json!({"lat": 41.5, "lng": 2.0, "radius_km": 5.0})).await;
    assert!(far.as_array().unwrap().is_empty());

    // New posts live 20 days.
    let key = format!("object:{}", all[0]["id"].as_str().unwrap());
    let ttl: i64 = redis::cmd("TTL").arg(key).query_async(&mut con).await.unwrap();
    assert!((20 * 86400 - 5..=20 * 86400).contains(&ttl), "ttl = {ttl}");
}

#[tokio::test]
async fn guard_blocks_spam_and_closed_routes() {
    let (_redis, app, mut con) = start(6394).await;

    // Publishing without a proof-of-work, or with a forged one, is refused.
    let (status, _) = post(&app, "/objects", publish("Sin prueba", "otros")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let forged = solved_pow(&app).await.replace("\"nonce\":\"", "\"nonce\":\"00");
    let (status, _) = send(&app, "POST", "/objects", publish("Falsa", "otros"), Some(forged)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A solved challenge works exactly once.
    let pow = solved_pow(&app).await;
    let (status, created) =
        send(&app, "POST", "/objects", publish("Primera", "otros"), Some(pow.clone())).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(&app, "POST", "/objects", publish("Repetida", "otros"), Some(pow)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // The engine's other routes are closed: nobody can bump a post's expiry.
    let id = created["id"].as_str().unwrap();
    let activity = json!({"payload": {"title": "x", "category": "otros"}});
    let (status, _) = post(&app, &format!("/objects/{id}/activities"), activity).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post(&app, "/feed", json!({"lat": 0.0, "lng": 0.0, "radius_km": 1.0})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Editing needs the owner token, not a proof-of-work.
    let (status, _) = send(&app, "DELETE", &format!("/objects/{id}"), json!({}), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Per-IP limit: 10 per hour. One is used, so seed the counter to 9 and the
    // next post is the 10th (allowed) and the one after is refused.
    redis::cmd("SET")
        .arg("rate:publish:203.0.113.7")
        .arg(9)
        .arg("EX")
        .arg(3600)
        .query_async::<_, ()>(&mut con)
        .await
        .unwrap();
    let (status, _) = publish_post(&app, publish("Décima", "otros")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = publish_post(&app, publish("Undécima", "otros")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}
