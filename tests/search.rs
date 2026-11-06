//! End-to-end: publish posts and search them, against a throwaway redis-server
//! (port 6392). Requires `redis-server` on PATH.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::process::{Child, Command, Stdio};
use tower::ServiceExt;

struct Redis(Child);
impl Drop for Redis {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

async fn start() -> (Redis, axum::Router, redis::aio::ConnectionManager) {
    let child = Command::new("redis-server")
        .args(["--port", "6392", "--save", "", "--appendonly", "no"])
        .stdout(Stdio::null())
        .spawn()
        .expect("redis-server on PATH");
    let guard = Redis(child);
    let client = redis::Client::open("redis://127.0.0.1:6392/").unwrap();
    let mut con = None;
    for _ in 0..50 {
        if let Ok(c) = redis::aio::ConnectionManager::new(client.clone()).await {
            con = Some(c);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let con = con.expect("redis did not start");
    let router = sietesodas::router(sietesodas::app_state(con.clone()));
    (guard, router, con)
}

async fn post(app: &axum::Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
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
    let (_redis, app, mut con) = start().await;

    for (title, category) in [
        ("Fender Stratocaster", "guitars"),
        ("Yamaha acoustic", "guitars"),
        ("Marshall stack", "amps"),
    ] {
        let (status, created) = post(&app, "/objects", publish(title, category)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(created["owner_token"].is_string());
    }

    // Bad category and missing title are rejected by the post validator.
    let (status, _) = post(&app, "/objects", publish("Tractor", "vehicles")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let bad = json!({"payload": {"category": "guitars"}, "locator": {"lat": 40.0, "lng": -3.0}});
    let (status, _) = post(&app, "/objects", bad).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Radius only, category, text, and both combined.
    let (_, all) = post(&app, "/search", search(json!({}))).await;
    assert_eq!(all.as_array().unwrap().len(), 3);
    let (_, guitars) = post(&app, "/search", search(json!({"category": "guitars"}))).await;
    assert_eq!(titles(&guitars), ["Fender Stratocaster", "Yamaha acoustic"]);
    let (_, fender) = post(&app, "/search", search(json!({"q": "FENDER strato"}))).await;
    assert_eq!(titles(&fender), ["Fender Stratocaster"]);
    let (_, none) = post(&app, "/search", search(json!({"category": "amps", "q": "fender"}))).await;
    assert!(none.as_array().unwrap().is_empty());

    // Far away finds nothing.
    let (_, far) = post(&app, "/search", json!({"lat": 41.5, "lng": 2.0, "radius_km": 5.0})).await;
    assert!(far.as_array().unwrap().is_empty());

    // New posts live 20 days.
    let key = format!("object:{}", all[0]["id"].as_str().unwrap());
    let ttl: i64 = redis::cmd("TTL").arg(key).query_async(&mut con).await.unwrap();
    assert!((20 * 86400 - 5..=20 * 86400).contains(&ttl), "ttl = {ttl}");
}
