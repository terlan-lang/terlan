#![forbid(unsafe_code)]

//! Plain Hyper/Tokio control server without Axum routing or extractors.

use std::convert::Infallible;
use std::env;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;

#[cfg(test)]
#[path = "hyper_baseline_test.rs"]
mod test;

fn main() {
    let port = env::args()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_else(|| {
            eprintln!("usage: terlan-hyper-baseline <port>");
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
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
        .unwrap_or_else(|error| {
            eprintln!("error[hyper-baseline.runtime]: {error}");
            std::process::exit(1);
        })
        .block_on(serve(port));
}

async fn serve(port: u16) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap_or_else(|error| {
            eprintln!("error[hyper-baseline.bind]: {error}");
            std::process::exit(1);
        });
    loop {
        let (stream, _) = listener.accept().await.unwrap_or_else(|error| {
            eprintln!("error[hyper-baseline.accept]: {error}");
            std::process::exit(1);
        });
        tokio::spawn(async move {
            let connection = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service_fn(handle));
            if let Err(error) = connection.await {
                eprintln!("error[hyper-baseline.connection]: {error}");
            }
        });
    }
}

async fn handle(request: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let query = request.uri().query().unwrap_or("").to_string();
    let accept = header(&request, "accept");
    let cookie = header(&request, "cookie");
    let body = request
        .into_body()
        .collect()
        .await
        .map(|body| body.to_bytes())
        .unwrap_or_default();
    if method == Method::GET {
        if let Some((left, right)) = add_route_parameters(&path) {
            return Ok(text_response(
                StatusCode::OK,
                (left + right).to_string().into_bytes(),
            ));
        }
    }
    if let Some(id) = item_route_parameter(&path) {
        let response = match method {
            Method::GET => text_response(StatusCode::OK, format!("item-{id}").into_bytes()),
            Method::PUT => {
                let mut output = format!("{id}:").into_bytes();
                output.extend_from_slice(&body);
                text_response(StatusCode::OK, output)
            }
            Method::DELETE => text_response(StatusCode::NO_CONTENT, Vec::new()),
            _ => text_response(StatusCode::NOT_FOUND, b"not-found".to_vec()),
        };
        return Ok(response);
    }
    let response = match (method, path.as_str()) {
        (Method::POST, "/api/bench") => {
            let mut output = Vec::with_capacity("generation-one:".len() + body.len());
            output.extend_from_slice(b"generation-one:");
            output.extend_from_slice(&body);
            text_response(StatusCode::OK, output)
        }
        (Method::POST, "/api/json") => match serde_json::from_slice::<serde_json::Value>(&body) {
            Ok(value) => response(
                StatusCode::OK,
                "application/json",
                serde_json::to_vec(&value).unwrap_or_default(),
            ),
            Err(_) => text_response(StatusCode::BAD_REQUEST, b"invalid-json".to_vec()),
        },
        (Method::POST, "/api/metadata") => {
            let mut output = format!("POST:{query}:{accept}:{cookie}:").into_bytes();
            output.extend_from_slice(&body);
            text_response(StatusCode::OK, output)
        }
        (Method::GET, "/api/static") => {
            text_response(StatusCode::OK, b"static-benchmark-response".to_vec())
        }
        (Method::POST, "/api/items") => response(
            StatusCode::CREATED,
            "text/plain; charset=utf-8",
            body.to_vec(),
        ),
        _ => text_response(StatusCode::NOT_FOUND, b"not-found".to_vec()),
    };
    Ok(response)
}

fn add_route_parameters(path: &str) -> Option<(i64, i64)> {
    let route = path.strip_prefix("/api/add/")?;
    let (left, right) = route.split_once('/')?;
    if right.contains('/') {
        return None;
    }
    Some((left.parse().ok()?, right.parse().ok()?))
}

fn item_route_parameter(path: &str) -> Option<&str> {
    let id = path.strip_prefix("/api/items/")?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

fn header(request: &Request<Incoming>, name: &str) -> String {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("missing")
        .to_string()
}

fn text_response(status: StatusCode, body: Vec<u8>) -> Response<Full<Bytes>> {
    response(status, "text/plain; charset=utf-8", body)
}

fn response(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, content_type)
        .header(CACHE_CONTROL, "no-cache")
        .header("x-content-type-options", "nosniff")
        .body(Full::new(Bytes::from(body)))
        .expect("static benchmark response is valid")
}
