use std::env;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".into());
    let bind = env::var("BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());

    let client = redis::Client::open(redis_url).expect("invalid REDIS_URL");
    let con = redis::aio::ConnectionManager::new(client)
        .await
        .expect("cannot connect to redis");

    let state = sietesodas::app_state(con);
    qhlv::spawn_sweep(state.clone());

    let listener = tokio::net::TcpListener::bind(&bind).await.expect("cannot bind");
    tracing::info!("listening on {bind}");
    axum::serve(listener, sietesodas::router(state)).await.unwrap();
}
