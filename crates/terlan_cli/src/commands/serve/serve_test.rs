use super::args::{DEFAULT_POLL_MS, DEFAULT_SERVE_HOST, DEFAULT_SERVE_PORT};
use super::handler::{HandlerLogIdentity, WebPackageSourceSpan};
use super::logging::{
    render_file_route_log_line, render_handler_log_line, render_static_log_line,
    render_static_route_log_line,
};
use super::response::build_http_response;
use super::*;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Creates a unique temporary test directory.
///
/// Inputs:
/// - `name`: readable test stem.
///
/// Output:
/// - Path to a not-yet-existing directory under the system temp directory.
///
/// Transformation:
/// - Combines process id and current nanoseconds to avoid collisions.
fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("timestamp")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan_serve_{name}_{}_{}",
        std::process::id(),
        nanos
    ))
}

/// Writes a minimal valid browser package fixture.
///
/// Inputs:
/// - `web_root`: target package directory.
///
/// Output:
/// - Filesystem fixture containing `index.html`, one JS asset, and manifest.
///
/// Transformation:
/// - Creates the same minimal shape consumed by `terlc serve --check`.
fn write_valid_package(web_root: &Path) {
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(web_root.join("assets/hello.txt"), "hello asset\n").expect("write static asset");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    },
    {
      "module": "",
      "kind": "static-asset",
      "source_relative_path": "assets/hello.txt",
      "web_relative_path": "assets/hello.txt",
      "fingerprint": 2
    }
  ]
}
"#,
    )
    .expect("write manifest");
}

/// Writes project metadata with a `[server.tls]` table.
///
/// Inputs:
/// - `path`: target `terlan.toml` path.
/// - `tls`: raw TLS table body.
///
/// Output:
/// - Filesystem fixture containing package metadata and TLS config.
///
/// Transformation:
/// - Creates the adjacent project manifest shape consumed by `terlc serve`
///   without depending on a full generated project.
fn write_project_manifest(path: &Path, tls: &str) {
    fs::write(
        path,
        format!(
            r#"[package]
name = "serve_tls_demo"
version = "0.0.1"

[server.tls]
{tls}
"#
        ),
    )
    .expect("write project manifest");
}

/// Writes a browser package fixture with one dynamic handler.
///
/// Inputs:
/// - `web_root`: target package directory.
/// - `route`: handler route to record.
///
/// Output:
/// - Filesystem fixture containing static assets and one manifest handler.
///
/// Transformation:
/// - Creates a deterministic handler manifest row for validation and matching
///   tests without requiring BEAM handler execution.
fn write_package_with_handler(web_root: &Path, route: &str) {
    write_valid_package(web_root);
    fs::write(
        web_root.join("manifest.json"),
        format!(
            r#"{{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    {{
      "method": "GET",
      "route": "{route}",
      "module": "app.Api",
      "function": "handle",
      "arity": 1
    }}
  ],
  "assets": [
    {{
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }}
  ]
}}
"#
        ),
    )
    .expect("write handler manifest");
}

/// Writes a browser package fixture with one router-level error handler.
///
/// Inputs:
/// - `web_root`: target package directory.
/// - `arity`: error handler arity to record.
///
/// Output:
/// - Filesystem fixture containing static assets and one manifest error
///   handler.
///
/// Transformation:
/// - Creates a deterministic error-handler manifest row for package validation
///   tests without requiring runtime dispatch.
fn write_package_with_error_handler(web_root: &Path, arity: usize) {
    write_valid_package(web_root);
    fs::write(
        web_root.join("manifest.json"),
        format!(
            r#"{{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "error_handler": {{
    "module": "app.Api",
    "function": "render_error",
    "arity": {arity}
  }},
  "assets": [
    {{
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }}
  ]
}}
"#
        ),
    )
    .expect("write error handler manifest");
}

#[test]
fn parse_serve_args_defaults_to_build_web() {
    let state = CliState {
        out_dir: PathBuf::from("custom_build"),
        ..CliState::default()
    };

    let parsed = parse_serve_args(&["--check".to_string()], &state).expect("parse serve args");

    assert_eq!(parsed.web_root, PathBuf::from("custom_build/web"));
    assert_eq!(parsed.host, DEFAULT_SERVE_HOST);
    assert_eq!(parsed.port, DEFAULT_SERVE_PORT);
    assert_eq!(parsed.poll_ms, DEFAULT_POLL_MS);
    assert!(parsed.check_only);
}

#[test]
fn parse_serve_args_accepts_explicit_web_root_host_and_port() {
    let state = CliState::default();

    let parsed = parse_serve_args(
        &[
            "dist/web".to_string(),
            "--host".to_string(),
            "0.0.0.0".to_string(),
            "--port".to_string(),
            "8080".to_string(),
            "--poll-ms".to_string(),
            "250".to_string(),
        ],
        &state,
    )
    .expect("parse serve args");

    assert_eq!(parsed.web_root, PathBuf::from("dist/web"));
    assert_eq!(parsed.host, "0.0.0.0");
    assert_eq!(parsed.port, 8080);
    assert_eq!(parsed.poll_ms, 250);
    assert!(!parsed.check_only);
}

/// Finds one HTTP request header value in a raw request fixture.
///
/// Inputs:
/// - `request`: raw HTTP request fixture text.
/// - `header_name`: lowercase header name to find.
///
/// Output:
/// - Trimmed header value when present.
///
/// Transformation:
/// - Scans only buffered fixture header lines, stops at the empty header
///   terminator, and performs ASCII-insensitive name matching.
fn request_header_value<'a>(request: &'a str, header_name: &str) -> Option<&'a str> {
    for line in request.lines().skip(1) {
        if line.trim().is_empty() {
            break;
        }
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case(header_name) {
            return Some(value.trim());
        }
    }
    None
}

#[test]
fn request_header_value_matches_case_insensitive_headers() {
    let request = "GET /api HTTP/1.1\r\nHost: localhost\r\nCookie: session=abc\r\n\r\nCookie: body";

    assert_eq!(request_header_value(request, "cookie"), Some("session=abc"));
    assert_eq!(request_header_value(request, "host"), Some("localhost"));
    assert_eq!(request_header_value(request, "authorization"), None);
}

#[test]
fn request_header_pairs_normalizes_hyper_headers_for_beam_request_maps() {
    let mut headers = http::HeaderMap::new();
    headers.insert("Accept", http::HeaderValue::from_static("application/json"));
    headers.insert("X-Terlan", http::HeaderValue::from_static("yes"));
    let mut pairs = request_header_pairs(&headers);
    pairs.sort();

    assert_eq!(
        pairs,
        vec![
            ("accept".to_string(), "application/json".to_string()),
            ("x-terlan".to_string(), "yes".to_string()),
        ]
    );
}

/// Returns the buffered HTTP request body text from a raw request fixture.
///
/// Inputs:
/// - `request`: raw HTTP request fixture text.
///
/// Output:
/// - Request body text after the CRLF header terminator, or an empty string
///   when the fixture has no body.
///
/// Transformation:
/// - Splits once at the HTTP header/body delimiter without parsing content
///   type because production request parsing is owned by Hyper.
fn request_body_text(request: &str) -> &str {
    request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("")
}

#[test]
fn request_body_text_returns_buffered_body_after_header_terminator() {
    let request =
        "POST /api HTTP/1.1\r\nHost: localhost\r\nContent-Length: 14\r\n\r\n{\"name\":\"Ada\"}";

    assert_eq!(request_body_text(request), "{\"name\":\"Ada\"}");
    assert_eq!(
        request_body_text("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n"),
        ""
    );
}

/// Runs an async serve handler fixture inside a local Tokio runtime.
///
/// Inputs:
/// - `future`: async test body that may await Hyper response bodies.
///
/// Output:
/// - The future output after the runtime completes.
///
/// Transformation:
/// - Creates the same Tokio runtime family used by `terlc serve` without
///   binding a socket or starting the long-running accept loop.
fn run_async_serve_test<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(future)
}

#[test]
fn async_serve_test_runtime_supports_timers() {
    run_async_serve_test(async {
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    });
}

/// Builds one typed HTTP request for direct serve-handler tests.
///
/// Inputs:
/// - `method`: HTTP method to place on the request.
/// - `uri`: request URI to route.
/// - `body`: request body text.
///
/// Output:
/// - Hyper-compatible request with a fixed in-memory body.
///
/// Transformation:
/// - Uses Rust HTTP request construction so handler tests exercise the same
///   typed boundary as the Hyper server path.
fn typed_request(method: &str, uri: &str, body: &str) -> Request<Full<Bytes>> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Full::new(Bytes::from(body.to_string())))
        .expect("typed request")
}

/// Collects one serve response body into UTF-8 text.
///
/// Inputs:
/// - `response`: typed serve response with a boxed Hyper body.
///
/// Output:
/// - Response body text.
///
/// Transformation:
/// - Drains the response body through `BodyExt::collect` and decodes the
///   resulting bytes losslessly for text fixtures.
async fn serve_response_text(response: Response<ServeBody>) -> String {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect response body")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf8 body")
}

/// Reads one data frame from a streaming serve response body.
///
/// Inputs:
/// - `body`: mutable boxed Hyper body from a streaming response.
///
/// Output:
/// - UTF-8 text carried by the next data frame.
///
/// Transformation:
/// - Awaits exactly one HTTP body frame and decodes its data bytes. This avoids
///   collecting infinite streams such as local live reload.
async fn next_body_frame_text(body: &mut ServeBody) -> String {
    let frame = body
        .frame()
        .await
        .expect("next body frame")
        .expect("valid body frame");
    let data = frame.data_ref().expect("data frame");
    String::from_utf8(data.to_vec()).expect("utf8 frame")
}

#[test]
fn hyper_request_handler_serves_static_get_response() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_static_get");
        let web_root = dir.join("web");
        write_valid_package(&web_root);
        fs::write(
            web_root.join("index.html"),
            "<!doctype html><html><body>Hello Terlan</body></html>",
        )
        .expect("write index");

        let response = handle_hyper_request(
            typed_request("GET", "/index.html", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("text/html; charset=utf-8"))
        );
        let body = serve_response_text(response).await;
        assert!(body.contains("Hello Terlan"));
        assert!(body.contains(RELOAD_ENDPOINT));

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

#[test]
fn hyper_request_handler_serves_static_file_with_query_string() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_static_query");
        let web_root = dir.join("web");
        write_valid_package(&web_root);
        fs::write(web_root.join("assets/hello.txt"), "query-safe").expect("write asset");

        let response = handle_hyper_request(
            typed_request("GET", "/assets/hello.txt?cache=1", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(serve_response_text(response).await, "query-safe");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

#[test]
fn hyper_request_handler_prefers_dynamic_handler_over_file_fallback() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_handler_before_file_fallback");
        let web_root = dir.join("web");
        write_valid_package(&web_root);
        fs::write(
            web_root.join("manifest.json"),
            r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    { "method": "GET", "route": "/api/game/config", "module": "app.Api", "function": "config", "arity": 1 }
  ],
  "file_responses": [
    { "method": "GET", "route": "*", "path": "index.html", "status": 200, "content_type": "text/html; charset=utf-8" }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }
  ]
}
"#,
        )
        .expect("write manifest");

        let response = handle_hyper_request(
            typed_request("GET", "/api/game/config", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::BAD_GATEWAY);
        let body = serve_response_text(response).await;
        assert!(body.contains("app.Api.config"));

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

#[test]
fn hyper_request_handler_prefers_physical_static_asset_over_file_fallback() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_asset_before_file_fallback");
        let web_root = dir.join("web");
        write_valid_package(&web_root);
        fs::write(
            web_root.join("manifest.json"),
            r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "file_responses": [
    { "method": "GET", "route": "*", "path": "index.html", "status": 200, "content_type": "text/html; charset=utf-8" }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }
  ]
}
"#,
        )
        .expect("write manifest");

        let response = handle_hyper_request(
            typed_request("GET", "/assets/js/modules/app.js", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static(
                "text/javascript; charset=utf-8"
            ))
        );
        assert_eq!(
            serve_response_text(response).await,
            "export const value = 1;\n"
        );

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

#[test]
fn hyper_request_handler_rejects_static_parent_path() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_static_parent");
        let web_root = dir.join("web");
        write_valid_package(&web_root);

        let response = handle_hyper_request(
            typed_request("GET", "/../secret.txt", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
        assert_eq!(serve_response_text(response).await, "bad request");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

#[test]
fn hyper_request_handler_streams_reload_sse_events() {
    run_async_serve_test(async {
        let reload_hub = Arc::new(Mutex::new(Vec::new()));

        let response = handle_hyper_request(
            typed_request("GET", RELOAD_ENDPOINT, ""),
            PathBuf::from("/tmp/no-web-root-needed"),
            Arc::clone(&reload_hub),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("text/event-stream"))
        );
        assert_eq!(
            response
                .headers()
                .get(http::header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&http::HeaderValue::from_static("*"))
        );
        let subscriber = {
            let subscribers = reload_hub.lock().expect("reload subscribers");
            assert_eq!(subscribers.len(), 1);
            subscribers[0].clone()
        };

        let mut body = response.into_body();
        assert_eq!(next_body_frame_text(&mut body).await, ": connected\n\n");
        subscriber.send(7).expect("send reload event");
        assert_eq!(
            next_body_frame_text(&mut body).await,
            "event: reload\ndata: 7\n\n"
        );
    });
}

/// Verifies auto TLS projects serve cached ACME HTTP-01 challenges.
///
/// Inputs:
/// - A web package with adjacent auto TLS metadata.
/// - A cached HTTP-01 token response under `.terlan/tls/acme/http-01`.
///
/// Output:
/// - Test passes when the reserved ACME path returns the cached key
///   authorization body before normal static routing.
///
/// Transformation:
/// - Exercises the local Let’s Encrypt challenge-serving route without opening
///   the network or contacting an ACME provider.
#[test]
fn hyper_request_handler_serves_acme_http01_challenge_from_auto_tls_cache() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_acme_challenge");
        let web_root = dir.join("_build/web");
        write_valid_package(&web_root);
        write_project_manifest(
            &dir.join("terlan.toml"),
            r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
        );
        let cache_dir = dir.join(".terlan/tls/acme/http-01");
        fs::create_dir_all(&cache_dir).expect("create acme challenge cache");
        fs::write(cache_dir.join("token_123"), "token_123.account-thumbprint")
            .expect("write acme challenge");

        let response = handle_hyper_request(
            typed_request("GET", "/.well-known/acme-challenge/token_123", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("text/plain; charset=utf-8"))
        );
        assert_eq!(
            serve_response_text(response).await,
            "token_123.account-thumbprint"
        );

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

/// Verifies missing ACME HTTP-01 challenge files return 404.
///
/// Inputs:
/// - A web package with adjacent auto TLS metadata.
/// - A request for a challenge token that has not been cached.
///
/// Output:
/// - Test passes when the reserved ACME path returns `404 Not Found`.
///
/// Transformation:
/// - Prevents missing ACME challenges from falling through to user static
///   assets or route handlers.
#[test]
fn hyper_request_handler_returns_404_for_missing_acme_http01_challenge() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_acme_missing");
        let web_root = dir.join("_build/web");
        write_valid_package(&web_root);
        write_project_manifest(
            &dir.join("terlan.toml"),
            r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
        );

        let response = handle_hyper_request(
            typed_request("GET", "/.well-known/acme-challenge/missing", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
        assert_eq!(serve_response_text(response).await, "not found");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

/// Verifies unsafe ACME HTTP-01 tokens fail before filesystem lookup.
///
/// Inputs:
/// - A web package with adjacent auto TLS metadata.
/// - A challenge request whose token contains a dot.
///
/// Output:
/// - Test passes when the server returns `400 Bad Request`.
///
/// Transformation:
/// - Locks the token-to-filename boundary to URL-safe ACME token characters.
#[test]
fn hyper_request_handler_rejects_invalid_acme_http01_token() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_acme_invalid");
        let web_root = dir.join("_build/web");
        write_valid_package(&web_root);
        write_project_manifest(
            &dir.join("terlan.toml"),
            r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
        );

        let response = handle_hyper_request(
            typed_request("GET", "/.well-known/acme-challenge/bad.token", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
        assert!(serve_response_text(response)
            .await
            .contains("ACME HTTP-01 token `bad.token` is invalid"));

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

/// Verifies ACME-looking paths remain ordinary static paths without auto TLS.
///
/// Inputs:
/// - A plain web package with a static `.well-known/acme-challenge` file.
///
/// Output:
/// - Test passes when the static file is served normally.
///
/// Transformation:
/// - Ensures the ACME route reservation activates only for auto TLS projects.
#[test]
fn hyper_request_handler_keeps_acme_like_static_files_for_plain_http_package() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_acme_plain");
        let web_root = dir.join("web");
        write_valid_package(&web_root);
        let static_dir = web_root.join(".well-known/acme-challenge");
        fs::create_dir_all(&static_dir).expect("create static acme-like dir");
        fs::write(static_dir.join("token_123"), "ordinary static file")
            .expect("write static acme-like file");

        let response = handle_hyper_request(
            typed_request("GET", "/.well-known/acme-challenge/token_123", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(serve_response_text(response).await, "ordinary static file");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

#[test]
fn hyper_request_handler_omits_static_head_response_body() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_static_head");
        let web_root = dir.join("web");
        write_valid_package(&web_root);

        let response = handle_hyper_request(
            typed_request("HEAD", "/assets/hello.txt", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers().get(http::header::CONTENT_LENGTH),
            Some(&http::HeaderValue::from_static("12"))
        );
        assert_eq!(serve_response_text(response).await, "");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

#[test]
fn hyper_request_handler_rejects_unmatched_mutating_method() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_static_post");
        let web_root = dir.join("web");
        write_valid_package(&web_root);

        let response = handle_hyper_request(
            typed_request("POST", "/assets/hello.txt", "payload"),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(serve_response_text(response).await, "method not allowed");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

/// Verifies dynamic handler logs include request and source handler metadata.
///
/// Inputs:
/// - One synthetic request id and matched handler identity.
///
/// Output:
/// - Test passes when the rendered log line includes route, handler, status,
///   and duration fields.
///
/// Transformation:
/// - Locks the local development log contract without starting a server or
///   capturing stderr.
#[test]
fn render_handler_log_line_includes_handler_metadata() {
    let identity = HandlerLogIdentity {
        method: "GET",
        route: "/users/:id",
        module: "app.Api",
        function: "show_user",
        source: None,
    };

    assert_eq!(
        render_handler_log_line(42, "web-abc", "GET", "/users/1", &identity, 200, 7),
        "terlc serve request_id=42 build_id=web-abc method=GET path=/users/1 route_method=GET route=/users/:id handler=app.Api.show_user status=200 duration_ms=7"
    );
}

/// Verifies dynamic-handler logs include optional source span metadata.
///
/// Inputs:
/// - One synthetic request id and matched handler identity with source span.
///
/// Output:
/// - Test passes when the rendered log line appends a stable source field.
///
/// Transformation:
/// - Locks the source-aware observability contract without requiring a server
///   request or generated manifest.
#[test]
fn render_handler_log_line_includes_optional_source_metadata() {
    let source = WebPackageSourceSpan {
        path: "src/app/Api.terl".to_string(),
        line: 12,
        column: 5,
    };
    let identity = HandlerLogIdentity {
        method: "GET",
        route: "/users/:id",
        module: "app.Api",
        function: "show_user",
        source: Some(&source),
    };

    assert_eq!(
        render_handler_log_line(42, "web-abc", "GET", "/users/1", &identity, 200, 7),
        "terlc serve request_id=42 build_id=web-abc method=GET path=/users/1 route_method=GET route=/users/:id handler=app.Api.show_user status=200 duration_ms=7 source=src/app/Api.terl:12:5"
    );
}

/// Verifies static-file logs include request and selected asset metadata.
///
/// Inputs:
/// - One synthetic request id and response path.
///
/// Output:
/// - Test passes when the rendered log line includes request path, static
///   response path, status, and duration fields.
///
/// Transformation:
/// - Locks the local static serving log contract without binding a socket.
#[test]
fn render_static_log_line_includes_asset_metadata() {
    assert_eq!(
        render_static_log_line(
            7,
            "web-abc",
            "GET",
            "/assets/app.js",
            Path::new("_build/web/assets/app.js"),
            200,
            3,
        ),
        "terlc serve request_id=7 build_id=web-abc method=GET path=/assets/app.js static=_build/web/assets/app.js status=200 duration_ms=3"
    );
}

/// Verifies static-route logs include request and route metadata.
///
/// Inputs:
/// - One synthetic request id and selected manifest route.
///
/// Output:
/// - Test passes when the rendered log line includes request path, route
///   method/pattern, status, and duration fields.
///
/// Transformation:
/// - Locks the local static-response log contract without binding a socket.
#[test]
fn render_static_route_log_line_includes_route_metadata() {
    let source = WebPackageSourceSpan {
        path: "src/app/Http.terl".to_string(),
        line: 11,
        column: 5,
    };

    assert_eq!(
        render_static_route_log_line(
            8,
            "web-abc",
            "HEAD",
            "/about",
            "GET",
            "/about",
            Some(&source),
            200,
            4
        ),
        "terlc serve request_id=8 build_id=web-abc method=HEAD path=/about static_route_method=GET static_route=/about status=200 duration_ms=4 source=src/app/Http.terl:11:5"
    );
}

/// Verifies file-route logs include request, route, and file metadata.
///
/// Inputs:
/// - One synthetic request id and selected manifest file route.
///
/// Output:
/// - Test passes when the rendered log line includes request path, route
///   method/pattern, selected file, status, and duration fields.
///
/// Transformation:
/// - Locks the local route-backed file response log contract without binding a
///   socket.
#[test]
fn render_file_route_log_line_includes_route_and_file_metadata() {
    let source = WebPackageSourceSpan {
        path: "src/app/Downloads.terl".to_string(),
        line: 17,
        column: 9,
    };

    assert_eq!(
        render_file_route_log_line(
            9,
            "web-abc",
            "GET",
            "/download",
            "GET",
            "/download",
            Path::new("_build/web/downloads/report.txt"),
            Some(&source),
            200,
            5,
        ),
        "terlc serve request_id=9 build_id=web-abc method=GET path=/download file_route_method=GET file_route=/download file=_build/web/downloads/report.txt status=200 duration_ms=5 source=src/app/Downloads.terl:17:9"
    );
}

/// Verifies development error pages include source-aware handler metadata.
///
/// Inputs:
/// - One synthetic request id, build id, matched handler identity, and backend
///   error.
///
/// Output:
/// - Test passes when the page includes stable code, request, route, handler,
///   request id, build id, and escaped backend error text.
///
/// Transformation:
/// - Locks the local development error shape without requiring a failing BEAM
///   handler process.
#[test]
fn render_dev_error_page_includes_escaped_handler_metadata() {
    let source = WebPackageSourceSpan {
        path: "src/app/<Api>.terl".to_string(),
        line: 12,
        column: 5,
    };
    let identity = HandlerLogIdentity {
        method: "GET",
        route: "/users/:id",
        module: "app.Api",
        function: "show_user",
        source: Some(&source),
    };

    let page = render_dev_error_page(
        42,
        "web-abc",
        "GET",
        "/users/<1>",
        &identity,
        "BEAM failed: <badarg> & \"quoted\"",
    );

    assert!(page.contains("serve_handler.execution_failed"));
    assert!(page.contains("Message:</strong> Handler execution failed."));
    assert!(page.contains("GET /users/&lt;1&gt;"));
    assert!(page.contains("GET /users/:id"));
    assert!(page.contains("app.Api.show_user"));
    assert!(page.contains("Source:</strong> <code>src/app/&lt;Api&gt;.terl:12:5</code>"));
    assert!(page.contains("Request id:</strong> <code>42</code>"));
    assert!(page.contains("Build id:</strong> <code>web-abc</code>"));
    assert!(page.contains("BEAM failed: &lt;badarg&gt; &amp; &quot;quoted&quot;"));
}

/// Verifies development error pages omit source metadata when none exists.
///
/// Inputs:
/// - One synthetic request id, build id, matched handler identity without
///   source span metadata, and backend error text.
///
/// Output:
/// - Test passes when the rendered page still includes request and handler
///   identity but does not render an empty or misleading Source row.
///
/// Transformation:
/// - Locks the optional-source branch of the local development error contract
///   without requiring a failing BEAM handler process.
#[test]
fn render_dev_error_page_omits_absent_source_metadata() {
    let identity = HandlerLogIdentity {
        method: "POST",
        route: "/api/events",
        module: "app.Events",
        function: "create",
        source: None,
    };

    let page = render_dev_error_page(
        43,
        "web-def",
        "POST",
        "/api/events",
        &identity,
        "handler exited",
    );

    assert!(page.contains("serve_handler.execution_failed"));
    assert!(page.contains("POST /api/events"));
    assert!(page.contains("app.Events.create"));
    assert!(page.contains("Request id:</strong> <code>43</code>"));
    assert!(page.contains("Build id:</strong> <code>web-def</code>"));
    assert!(page.contains("handler exited"));
    assert!(!page.contains("Source:</strong>"));
}

#[test]
fn validate_web_package_accepts_valid_manifest_and_assets() {
    let dir = temp_dir("valid_package");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    validate_web_package(&web_root).expect("valid package");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_accepts_manifest_handler() {
    let dir = temp_dir("valid_handler");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "/api/users");

    validate_web_package(&web_root).expect("valid package handler");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_accepts_manifest_error_handler() {
    let dir = temp_dir("valid_error_handler");
    let web_root = dir.join("web");
    write_package_with_error_handler(&web_root, 1);

    validate_web_package(&web_root).expect("valid package error handler");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_rejects_manifest_error_handler_wrong_arity() {
    let dir = temp_dir("invalid_error_handler_arity");
    let web_root = dir.join("web");
    write_package_with_error_handler(&web_root, 0);

    let err = validate_web_package(&web_root).expect_err("wrong error handler arity should fail");

    assert!(err.contains("error handler `app.Api.render_error`"));
    assert!(err.contains("must have arity 1"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_rejects_unsafe_handler_route() {
    let dir = temp_dir("unsafe_handler_route");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "../api");

    let err = validate_web_package(&web_root).expect_err("unsafe handler route should fail");

    assert!(err.contains("unsafe handler route"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_rejects_missing_manifest_asset() {
    let dir = temp_dir("missing_asset");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    fs::remove_file(web_root.join("assets/js/modules/app.js")).expect("remove asset");

    let err = validate_web_package(&web_root).expect_err("missing asset should fail");

    assert!(err.contains("error[serve_package]"));
    assert!(err.contains("does not exist"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_rejects_unsafe_manifest_path() {
    let dir = temp_dir("unsafe_path");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "index": "../index.html",
  "assets": []
}
"#,
    )
    .expect("write unsafe manifest");

    let err = validate_web_package(&web_root).expect_err("unsafe path should fail");

    assert!(err.contains("unsafe browser package index path"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_serve_check_validates_without_binding_port() {
    let dir = temp_dir("run_check");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    let status = run(
        CliCommand {
            verb: Some("serve".to_string()),
            args: vec![web_root.display().to_string(), "--check".to_string()],
        },
        CliState::default(),
    );

    assert_eq!(status, ExitCode::SUCCESS);
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_live_serve_rejects_auto_tls_without_certificate_cache() {
    let dir = temp_dir("run_tls_auto_cache");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );

    let status = run(
        CliCommand {
            verb: Some("serve".to_string()),
            args: vec![web_root.display().to_string()],
        },
        CliState::default(),
    );

    assert_eq!(status, ExitCode::from(1));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies auto TLS returns the stable local-cache diagnostic before binding.
///
/// Inputs:
/// - A valid web package with adjacent auto `[server.tls]` metadata.
/// - Parsed serve arguments that would otherwise bind a local HTTP listener.
///
/// Output:
/// - Test passes when the server startup boundary returns the ACME cache
///   diagnostic.
///
/// Transformation:
/// - Calls the serve package startup helper directly so the diagnostic string
///   is asserted without needing to capture process stderr.
#[test]
fn serve_web_package_rejects_auto_tls_without_certificate_cache() {
    let dir = temp_dir("serve_tls_auto_cache");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );

    let message = serve_web_package(&ServeArgs {
        web_root,
        host: DEFAULT_SERVE_HOST.to_string(),
        port: 0,
        poll_ms: DEFAULT_POLL_MS,
        check_only: false,
    })
    .expect_err("auto TLS should fail before listener binding without a certificate cache");

    assert!(message.starts_with("error[serve_tls]: automatic ACME TLS"));
    assert!(message.contains("example.test"));
    assert!(message.contains("mode `manual` or `internal`"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn inject_reload_script_inserts_before_body_close() {
    let html = "<!doctype html><html><body><main></main></body></html>";

    let injected = inject_reload_script(html);

    assert!(injected.contains(RELOAD_ENDPOINT));
    assert!(injected.contains("</script></body>"));
}

#[test]
fn inject_reload_script_preserves_existing_reload_reference() {
    let html = "<script>new EventSource('/__terlan/reload')</script>";

    let injected = inject_reload_script(html);

    assert_eq!(injected, html);
}

#[test]
fn content_type_for_path_covers_browser_runtime_assets() {
    let cases = [
        ("index.html", "text/html; charset=utf-8"),
        ("app.css", "text/css; charset=utf-8"),
        ("app.js", "text/javascript; charset=utf-8"),
        ("app.js.map", "application/json; charset=utf-8"),
        ("data.json", "application/json; charset=utf-8"),
        ("module.wasm", "application/wasm"),
        ("font.woff", "font/woff"),
        ("font.woff2", "font/woff2"),
        ("font.ttf", "font/ttf"),
        ("font.otf", "font/otf"),
        ("image.avif", "image/avif"),
        ("asset.bin", "application/octet-stream"),
    ];

    for (path, expected) in cases {
        assert_eq!(content_type_for_path(Path::new(path)), expected, "{path}");
    }
}

#[test]
fn build_http_response_preserves_server_response_contract() {
    let response =
        build_http_response(200, "text/plain; charset=utf-8", &[], b"hello world", false)
            .expect("valid response should build");

    assert_eq!(response.status(), http::StatusCode::OK);
    assert_eq!(
        response.headers()[http::header::CONTENT_TYPE],
        "text/plain; charset=utf-8"
    );
    assert_eq!(response.headers()[http::header::CONTENT_LENGTH], "11");
    assert_eq!(response.headers()[http::header::CACHE_CONTROL], "no-cache");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(response.headers()[http::header::CONNECTION], "close");
    assert_eq!(response.body(), b"hello world");
}

#[test]
fn build_http_response_appends_validated_dynamic_headers() {
    let extra_headers = vec![
        (
            "Set-Cookie".to_string(),
            "session=abc; HttpOnly".to_string(),
        ),
        ("x-terlan".to_string(), "yes".to_string()),
    ];
    let response = build_http_response(
        200,
        "text/plain; charset=utf-8",
        &extra_headers,
        b"hello",
        false,
    )
    .expect("valid response should build");

    assert_eq!(response.headers()["set-cookie"], "session=abc; HttpOnly");
    assert_eq!(response.headers()["x-terlan"], "yes");
}

#[test]
fn build_http_response_omits_body_for_head_responses() {
    let response = build_http_response(200, "text/plain; charset=utf-8", &[], b"hello head", true)
        .expect("valid response should build");

    assert_eq!(response.headers()[http::header::CONTENT_LENGTH], "10");
    assert!(response.body().is_empty());
}

#[test]
fn build_http_response_rejects_invalid_http_metadata() {
    let bad_status = build_http_response(99, "text/plain", &[], b"", false)
        .expect_err("invalid status should fail");
    assert!(bad_status.contains("HTTP status `99` is invalid"));

    let bad_content_type = build_http_response(200, "bad\nvalue", &[], b"", false)
        .expect_err("invalid content type should fail");
    assert!(bad_content_type.contains("Content-Type value is invalid"));

    let bad_header_name = build_http_response(
        200,
        "text/plain",
        &[("bad header".to_string(), "value".to_string())],
        b"",
        false,
    )
    .expect_err("invalid header name should fail");
    assert!(bad_header_name.contains("HTTP header name `bad header` is invalid"));

    let bad_header_value = build_http_response(
        200,
        "text/plain",
        &[("x-terlan".to_string(), "bad\nvalue".to_string())],
        b"",
        false,
    )
    .expect_err("invalid header value should fail");
    assert!(bad_header_value.contains("HTTP header `x-terlan` value is invalid"));
}

#[test]
fn reload_sse_response_preserves_live_reload_response_contract() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build test runtime");
    runtime.block_on(async {
        let reload_hub = Arc::new(Mutex::new(Vec::new()));
        let response = reload_sse_response(reload_hub);

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers()[http::header::CONTENT_TYPE],
            "text/event-stream"
        );
        assert_eq!(response.headers()[http::header::CACHE_CONTROL], "no-cache");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(response.headers()[http::header::CONNECTION], "keep-alive");
        assert_eq!(
            response.headers()[http::header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "*"
        );
    });
}
