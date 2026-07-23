//! Minimal Axum/Tokio comparison server for the HTTP AOT benchmark.

#![forbid(unsafe_code)]

use std::env;

use axum::body::Bytes;
use axum::routing::post;
use axum::Router;

fn main() {
    let port = env::args()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_else(|| {
            eprintln!("usage: terlan-axum-baseline <port>");
            std::process::exit(2);
        });
    let workers = env::var("TERLAN_BENCH_HTTP_AOT_REACTORS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|workers| *workers > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
        });
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
        .unwrap_or_else(|error| {
            eprintln!("error[axum-baseline.runtime]: {error}");
            std::process::exit(1);
        });
    runtime.block_on(serve(port));
}

async fn serve(port: u16) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap_or_else(|error| {
            eprintln!("error[axum-baseline.bind]: {error}");
            std::process::exit(1);
        });
    let application = Router::new().route(
        "/api/bench",
        post(|body: Bytes| async move {
            let mut response = Vec::with_capacity("generation-one:".len() + body.len());
            response.extend_from_slice(b"generation-one:");
            response.extend_from_slice(&body);
            response
        }),
    );
    if let Err(error) = axum::serve(listener, application).await {
        eprintln!("error[axum-baseline.serve]: {error}");
        std::process::exit(1);
    }
}
