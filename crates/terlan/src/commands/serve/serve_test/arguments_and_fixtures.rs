use super::*;

use super::super::args::{
    ServeHandlerRuntime, DEFAULT_POLL_MS, DEFAULT_SERVE_HOST, DEFAULT_SERVE_PORT,
};
use crate::support::test_fs;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

/// Creates a unique temporary test directory.
///
/// Inputs:
/// - `name`: readable test stem.
///
/// Output:
/// - Path to a not-yet-existing directory under the system temp directory.
///
/// Transformation:
/// - Delegates to the shared test filesystem helper with the serve namespace.
pub(super) fn temp_dir(name: &str) -> PathBuf {
    test_fs::temp_path("serve", name)
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
pub(super) fn write_valid_package(web_root: &Path) {
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
pub(super) fn write_project_manifest(path: &Path, tls: &str) {
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
///   tests without requiring VM handler execution.
pub(super) fn write_package_with_handler(web_root: &Path, route: &str) {
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
pub(super) fn write_package_with_error_handler(web_root: &Path, arity: usize) {
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
pub(super) fn parse_serve_args_defaults_to_build_web() {
    let state = CliState {
        out_dir: PathBuf::from("custom_build"),
        ..CliState::default()
    };

    let parsed = parse_serve_args(&["--check".to_string()], &state).expect("parse serve args");

    assert_eq!(parsed.web_root, PathBuf::from("custom_build/web"));
    assert_eq!(parsed.host, DEFAULT_SERVE_HOST);
    assert_eq!(parsed.port, DEFAULT_SERVE_PORT);
    assert_eq!(parsed.poll_ms, DEFAULT_POLL_MS);
    assert_eq!(parsed.handler_runtime, ServeHandlerRuntime::Static);
    assert!(parsed.check_only);
}

#[test]
pub(super) fn parse_serve_args_accepts_release_check_config_alias() {
    let parsed = parse_serve_args(&["--check-config".to_string()], &CliState::default())
        .expect("parse release check-config alias");

    assert!(parsed.check_only);
}

#[test]
pub(super) fn parse_serve_args_accepts_explicit_web_root_host_port_and_handler_runtime() {
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
            "--handler-runtime".to_string(),
            "static".to_string(),
        ],
        &state,
    )
    .expect("parse serve args");

    assert_eq!(parsed.web_root, PathBuf::from("dist/web"));
    assert_eq!(parsed.host, "0.0.0.0");
    assert_eq!(parsed.port, 8080);
    assert_eq!(parsed.poll_ms, 250);
    assert_eq!(parsed.handler_runtime, ServeHandlerRuntime::Static);
    assert!(!parsed.check_only);
}

#[test]
pub(super) fn parse_serve_args_rejects_explicit_beam_handler_runtime() {
    let err = parse_serve_args(
        &[
            "--handler-runtime".to_string(),
            "beam".to_string(),
            "--check".to_string(),
        ],
        &CliState::default(),
    )
    .expect_err("beam handler runtime should be removed");

    assert_eq!(
        err,
        "handler runtime `beam` was removed from the public CLI; use `static`"
    );
}

#[test]
pub(super) fn parse_serve_args_rejects_unknown_handler_runtime() {
    let err = parse_serve_args(
        &[
            "--handler-runtime".to_string(),
            "otp".to_string(),
            "--check".to_string(),
        ],
        &CliState::default(),
    )
    .expect_err("unknown handler runtime should fail");

    assert!(err.contains("expects static"));
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
pub(super) fn request_header_value<'a>(request: &'a str, header_name: &str) -> Option<&'a str> {
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
pub(super) fn request_header_value_matches_case_insensitive_headers() {
    let request = "GET /api HTTP/1.1\r\nHost: localhost\r\nCookie: session=abc\r\n\r\nCookie: body";

    assert_eq!(request_header_value(request, "cookie"), Some("session=abc"));
    assert_eq!(request_header_value(request, "host"), Some("localhost"));
    assert_eq!(request_header_value(request, "authorization"), None);
}

#[test]
pub(super) fn request_header_pairs_normalizes_hyper_headers_for_handler_request_maps() {
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
pub(super) fn request_body_text(request: &str) -> &str {
    request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("")
}

#[test]
pub(super) fn request_body_text_returns_buffered_body_after_header_terminator() {
    let request =
        "POST /api HTTP/1.1\r\nHost: localhost\r\nContent-Length: 14\r\n\r\n{\"name\":\"Ada\"}";

    assert_eq!(request_body_text(request), "{\"name\":\"Ada\"}");
    assert_eq!(
        request_body_text("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n"),
        ""
    );
}

/// Runs an async serve handler fixture that must complete without host async.
///
/// Inputs:
/// - `future`: async test body that may await immediately-ready response
///   bodies.
///
/// Output:
/// - The future output when the first poll completes.
///
/// Transformation:
/// - Allows direct handler tests to consume in-memory Hyper bodies while
///   rejecting accidental dependence on host async scheduling.
pub(super) fn run_async_serve_test<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("serve handler fixture unexpectedly required host async polling"),
    }
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
pub(super) fn typed_request(method: &str, uri: &str, body: &str) -> Request<Full<Bytes>> {
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
pub(super) async fn serve_response_text(response: Response<ServeBody>) -> String {
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
pub(super) async fn next_body_frame_text(body: &mut ServeBody) -> String {
    let frame = body
        .frame()
        .await
        .expect("next body frame")
        .expect("valid body frame");
    let data = frame.data_ref().expect("data frame");
    String::from_utf8(data.to_vec()).expect("utf8 frame")
}

#[test]
pub(super) fn hyper_request_handler_serves_static_get_response() {
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
pub(super) fn hyper_request_handler_serves_static_file_with_query_string() {
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
pub(super) fn hyper_request_handler_rejects_non_get_websocket_route_before_static_fallback() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_websocket_non_get");
        let web_root = dir.join("web");
        write_valid_package(&web_root);
        fs::write(
            web_root.join("manifest.json"),
            r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "websockets": [
    { "route": "/ws", "protocol": "app.room.v1" }
  ],
  "static_responses": [
    { "method": "POST", "route": "*", "status": 200, "content_type": "text/plain; charset=utf-8", "body": "fallback" }
  ],
  "assets": []
}
"#,
        )
        .expect("write websocket manifest");

        let response = handle_hyper_request(
            typed_request("POST", "/ws", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(
            response.headers().get(http::header::ALLOW),
            Some(&http::HeaderValue::from_static("GET"))
        );
        assert_eq!(
            response.headers().get(http::header::UPGRADE),
            Some(&http::HeaderValue::from_static("websocket"))
        );
        let body = serve_response_text(response).await;
        assert_eq!(body, "websocket upgrades require GET");
        assert!(!body.contains("fallback"), "{body}");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

/// Verifies Hyper WebSocket HEAD rejections do not emit a body.
///
/// Inputs:
/// - A web package with a reserved WebSocket route and a fallback static
///   response.
/// - A HEAD request for that WebSocket route.
///
/// Output:
/// - Test passes when the transitional Hyper path rejects the request with
///   `405 Method Not Allowed`, preserves the WebSocket method headers, and
///   emits no body.
///
/// Transformation:
/// - Aligns Hyper's reserved WebSocket route handling with VM-stream HEAD
///   semantics while preventing fallback static routes from taking precedence.
#[test]
pub(super) fn hyper_request_handler_heads_non_get_websocket_route_without_body() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_websocket_head");
        let web_root = dir.join("web");
        write_valid_package(&web_root);
        fs::write(
            web_root.join("manifest.json"),
            r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "websockets": [
    { "route": "/ws", "protocol": "app.room.v1" }
  ],
  "static_responses": [
    { "method": "HEAD", "route": "*", "status": 200, "content_type": "text/plain; charset=utf-8", "body": "fallback" }
  ],
  "assets": []
}
"#,
        )
        .expect("write websocket manifest");

        let response = handle_hyper_request(
            typed_request("HEAD", "/ws", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(
            response.headers().get(http::header::ALLOW),
            Some(&http::HeaderValue::from_static("GET"))
        );
        assert_eq!(
            response.headers().get(http::header::UPGRADE),
            Some(&http::HeaderValue::from_static("websocket"))
        );
        let body = serve_response_text(response).await;
        assert_eq!(body, "");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

#[test]
pub(super) fn hyper_request_handler_reports_websocket_upgrade_required_before_static_fallback() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_websocket_upgrade_required");
        let web_root = dir.join("web");
        write_valid_package(&web_root);
        fs::write(
            web_root.join("manifest.json"),
            r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "websockets": [
    { "route": "/ws", "protocol": "app.room.v1" }
  ],
  "static_responses": [
    { "method": "GET", "route": "*", "status": 200, "content_type": "text/plain; charset=utf-8", "body": "fallback" }
  ],
  "assets": []
}
"#,
        )
        .expect("write websocket manifest");

        let response = handle_hyper_request(
            typed_request("GET", "/ws", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::UPGRADE_REQUIRED);
        assert_eq!(
            response.headers().get(http::header::UPGRADE),
            Some(&http::HeaderValue::from_static("websocket"))
        );
        let body = serve_response_text(response).await;
        assert_eq!(body, "websocket upgrade required");
        assert!(!body.contains("fallback"), "{body}");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

#[test]
pub(super) fn hyper_request_handler_rejects_malformed_websocket_upgrade_before_static_fallback() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_websocket_malformed_upgrade");
        let web_root = dir.join("web");
        write_valid_package(&web_root);
        fs::write(
            web_root.join("manifest.json"),
            r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "websockets": [
    { "route": "/ws", "protocol": "app.room.v1" }
  ],
  "static_responses": [
    { "method": "GET", "route": "*", "status": 200, "content_type": "text/plain; charset=utf-8", "body": "fallback" }
  ],
  "assets": []
}
"#,
        )
        .expect("write websocket manifest");

        let request = Request::builder()
            .method("GET")
            .uri("/ws")
            .header(http::header::UPGRADE, "websocket")
            .header(http::header::CONNECTION, "Upgrade")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .body(Full::new(Bytes::new()))
            .expect("websocket request");
        let response = handle_hyper_request(
            request,
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
        let body = serve_response_text(response).await;
        assert_eq!(body, "malformed websocket upgrade request");
        assert!(!body.contains("fallback"), "{body}");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

#[test]
pub(super) fn hyper_request_handler_returns_websocket_upgrade_handshake() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_websocket_upgrade_handshake");
        let web_root = dir.join("web");
        write_valid_package(&web_root);
        fs::write(
            web_root.join("manifest.json"),
            r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "websockets": [
    { "route": "/ws", "protocol": "app.room.v1" }
  ],
  "static_responses": [
    { "method": "GET", "route": "*", "status": 200, "content_type": "text/plain; charset=utf-8", "body": "fallback" }
  ],
  "assets": []
}
"#,
        )
        .expect("write websocket manifest");

        let request = Request::builder()
            .method("GET")
            .uri("/ws")
            .header(http::header::UPGRADE, "websocket")
            .header(http::header::CONNECTION, "Upgrade")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .header("sec-websocket-version", "13")
            .body(Full::new(Bytes::new()))
            .expect("websocket request");
        let response = handle_hyper_request(
            request,
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::SWITCHING_PROTOCOLS);
        assert_eq!(
            response.headers().get(http::header::CONNECTION),
            Some(&http::HeaderValue::from_static("Upgrade"))
        );
        assert_eq!(
            response.headers().get(http::header::UPGRADE),
            Some(&http::HeaderValue::from_static("websocket"))
        );
        assert_eq!(
            response.headers().get("sec-websocket-accept"),
            Some(&http::HeaderValue::from_static(
                "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
            ))
        );
        assert_eq!(serve_response_text(response).await, "");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}

/// Verifies `terlc serve` can serve static assets emitted from `[web.assets]`.
///
/// Inputs:
/// - Project fixture with `[web.assets] directory = "assets"`.
/// - Browser build output produced by the real `terlc build` command.
///
/// Output:
/// - Test passes when the generated `_build/web` package serves the copied
///   manifest asset by its public path.
///
/// Transformation:
/// - Bridges the browser package build path and Hyper serve path so manifest
///   asset copying and request routing are validated together.
#[test]
pub(super) fn hyper_request_handler_serves_manifest_declared_static_assets_from_build_output() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_manifest_static_assets_from_build");
        let project_dir = dir.join("project");
        let source_dir = project_dir.join("src/demo");
        let asset_dir = project_dir.join("assets/nested");
        let out_dir = dir.join("build");
        fs::create_dir_all(&source_dir).expect("create source dir");
        fs::create_dir_all(&asset_dir).expect("create asset dir");
        fs::write(
            project_dir.join("terlan.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.0.7\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n\n[web.assets]\ndirectory = \"assets\"\npublic_path = \"/assets\"\n",
        )
        .expect("write project manifest");
        fs::write(
            source_dir.join("Main.terl"),
            "module demo.Main.\n\npub value(): Int ->\n    1.\n",
        )
        .expect("write source");
        fs::write(asset_dir.join("logo.txt"), "terlan asset\n").expect("write asset");

        let status = crate::commands::build::run(
            CliCommand {
                verb: Some("build".to_string()),
                args: vec![
                    project_dir.display().to_string(),
                    "--target".to_string(),
                    "js.browser".to_string(),
                ],
            },
            CliState {
                out_dir: out_dir.clone(),
                ..CliState::default()
            },
        );
        assert_eq!(status, ExitCode::SUCCESS);

        let web_root = out_dir.join("web");
        let response = handle_hyper_request(
            typed_request("GET", "/assets/nested/logo.txt?cache=1", ""),
            web_root,
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(serve_response_text(response).await, "terlan asset\n");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}
