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

    // Signs proof-of-work challenges. Set POW_SECRET to keep them valid across
    // restarts; otherwise a random one is used.
    let pow_secret = env::var("POW_SECRET").unwrap_or_else(|_| {
        let bytes: [u8; 32] = rand::random();
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    });

    let state = sietesodas::app_state(con);
    qhlv::spawn_sweep(state.clone());

    let listener = tokio::net::TcpListener::bind(&bind).await.expect("cannot bind");
    tracing::info!("listening on {bind}");
    let app = sietesodas::router(state, &pow_secret)
        .into_make_service_with_connect_info::<std::net::SocketAddr>();
    axum::serve(listener, app).await.unwrap();
}
