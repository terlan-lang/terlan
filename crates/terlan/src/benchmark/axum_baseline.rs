//! Minimal Axum/Tokio comparison server for the HTTP AOT benchmark.

#![forbid(unsafe_code)]

use std::env;

use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
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
    let application = Router::new()
        .route("/api/bench", post(echo))
        .route("/api/json", post(json))
        .route("/api/metadata", post(metadata))
        .route("/api/static", get(|| async { "static-benchmark-response" }));
    if let Err(error) = axum::serve(listener, application).await {
        eprintln!("error[axum-baseline.serve]: {error}");
        std::process::exit(1);
    }
}

async fn echo(body: Bytes) -> impl IntoResponse {
    let mut response = Vec::with_capacity("generation-one:".len() + body.len());
    response.extend_from_slice(b"generation-one:");
    response.extend_from_slice(&body);
    benchmark_response(response)
}

async fn json(body: Bytes) -> Response {
    match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(value) => (
            StatusCode::OK,
            [("content-type", "application/json")],
            serde_json::to_vec(&value).unwrap_or_default(),
        )
            .into_response(),
        Err(_) => (StatusCode::BAD_REQUEST, "invalid-json").into_response(),
    }
}

async fn metadata(uri: Uri, headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let query = uri.query().unwrap_or("");
    let accept = header(&headers, "accept");
    let cookie = header(&headers, "cookie");
    let mut response = format!("POST:{query}:{accept}:{cookie}:").into_bytes();
    response.extend_from_slice(&body);
    benchmark_response(response)
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("missing")
}

fn benchmark_response(body: Vec<u8>) -> impl IntoResponse {
    (
        [
            ("content-type", "text/plain; charset=utf-8"),
            ("cache-control", "no-cache"),
            ("x-content-type-options", "nosniff"),
        ],
        body,
    )
}
