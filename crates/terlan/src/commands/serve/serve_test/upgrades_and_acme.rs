use super::*;

#[test]
pub(super) fn vm_stream_head_reload_sse_handshake_omits_body_without_hyper() {
    let dir = temp_dir("vm_stream_reload_sse_head");
    let web_root = dir.join("web");
    fs::create_dir_all(&web_root).expect("create web root");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"HEAD /__terlan/reload HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream reload SSE HEAD request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    assert!(
        response.contains("content-type: text/event-stream\r\n"),
        "{response}"
    );
    assert!(response.contains("Content-Length: 13\r\n"), "{response}");
    assert!(response.ends_with("\r\n\r\n"), "{response}");
    assert!(!response.ends_with(": connected\n\n"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies reload SSE rejects mutating methods before stream setup.
///
/// Inputs:
/// - An otherwise valid VM-stream request for the reserved reload endpoint.
/// - A POST method with a request body.
///
/// Output:
/// - Test passes when the VM-stream adapter returns `405 Method Not Allowed`
///   with `Allow: GET, HEAD`.
///
/// Transformation:
/// - Keeps the live-reload endpoint reserved while preventing mutating
///   requests from opening or emulating SSE streams.
#[test]
pub(super) fn vm_stream_request_rejects_reload_sse_mutating_method_without_hyper() {
    let dir = temp_dir("vm_stream_reload_sse_post");
    let web_root = dir.join("web");
    fs::create_dir_all(&web_root).expect("create web root");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"POST /__terlan/reload HTTP/1.1\r\nHost: localhost\r\nContent-Length: 7\r\n\r\npayload",
    )
    .expect("VM stream reload SSE POST request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 405 "), "{response}");
    assert!(response.contains("allow: GET, HEAD\r\n"), "{response}");
    assert!(!response.contains("text/event-stream"), "{response}");
    assert!(response.ends_with("\r\n\r\nmethod not allowed"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_stream_request_reports_websocket_upgrade_required_without_hyper() {
    let dir = temp_dir("vm_stream_websocket_upgrade_required");
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

    let response =
        handle_vm_stream_http1_request(&web_root, b"GET /ws HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .expect("VM stream websocket missing-upgrade request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 426 "), "{response}");
    assert!(response.contains("upgrade: websocket\r\n"), "{response}");
    assert!(response.ends_with("\r\n\r\nwebsocket upgrade required"));
    assert!(!response.contains("fallback"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_stream_request_rejects_non_get_websocket_route_without_hyper() {
    let dir = temp_dir("vm_stream_websocket_non_get");
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

    let response =
        handle_vm_stream_http1_request(&web_root, b"POST /ws HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .expect("VM stream websocket non-GET request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 405 "), "{response}");
    assert!(response.contains("allow: GET\r\n"), "{response}");
    assert!(response.contains("upgrade: websocket\r\n"), "{response}");
    assert!(response.ends_with("\r\n\r\nwebsocket upgrades require GET"));
    assert!(!response.contains("fallback"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_stream_request_returns_websocket_upgrade_handshake_without_hyper() {
    let dir = temp_dir("vm_stream_websocket_upgrade_handshake");
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

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
    )
    .expect("VM stream websocket upgrade handshake request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(
        response.starts_with("HTTP/1.1 101 Switching Protocols\r\n"),
        "{response}"
    );
    assert!(response.contains("Connection: Upgrade\r\n"), "{response}");
    assert!(response.contains("upgrade: websocket\r\n"), "{response}");
    assert!(
        response.contains("sec-websocket-accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\n"), "{response}");
    assert!(!response.contains("fallback"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies source-owned WebSocket routes mediate production upgrade admission.
#[test]
pub(super) fn vm_stream_websocket_upgrade_activates_materialized_router_middleware() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_websocket_router_middleware");
    let web_root = dir.join("_build/web");
    let source_dir = dir.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        dir.join("terlan.toml"),
        "[package]\nname = \"serve_websocket_router\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
    )
    .expect("write project manifest");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        source_dir.join("WebSocket.terl"),
        r#"module app.WebSocket.

import std.core.Option.{Some}.
import std.http.{Response, Router}.
import std.http.WebSocket.{endpoint}.
import std.http.Router.{Continue, Respond}.
import type std.http.{Request, Response, Router}.
import type std.http.Router.MiddlewareResult.

pub route(): String -> "/ws".

pub protocol(): String -> "app.room.v1".

pub gate(request: Request): MiddlewareResult ->
    case request.header("x-deny-upgrade") {
        Some(_) -> Respond(Response.text("upgrade denied", status = 401));
        _ -> Continue
    }.

pub decorate(_request: Request, response: Response): Response ->
    response.with_status(403).

pub router(): Router ->
    Router.use(Router.new(), gate)
        .map_response(decorate)
        .websocket("/ws", endpoint(4, 1024)).
"#,
    )
    .expect("write WebSocket router source");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "websockets": [
    {
      "module": "app.WebSocket",
      "route": "/ws",
      "protocol": "app.room.v1",
      "source": {
        "path": "src/app/WebSocket.terl",
        "line": 9,
        "column": 1
      }
    }
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
    .expect("write websocket manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm WebSocket router");

    let denied = handle_vm_stream_http1_request(
        &web_root,
        b"GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nX-Deny-Upgrade: yes\r\n\r\n",
    )
    .expect("denied WebSocket upgrade");
    let denied = String::from_utf8(denied).expect("denied response should be UTF-8");
    assert!(denied.starts_with("HTTP/1.1 403 "), "{denied}");
    assert!(denied.ends_with("\r\n\r\nupgrade denied"), "{denied}");
    assert!(!denied.contains("sec-websocket-accept"), "{denied}");

    let accepted = handle_vm_stream_http1_request(
        &web_root,
        b"GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
    )
    .expect("accepted WebSocket upgrade");
    let accepted = String::from_utf8(accepted).expect("upgrade response should be UTF-8");
    assert!(
        accepted.starts_with("HTTP/1.1 101 Switching Protocols\r\n"),
        "{accepted}"
    );
    assert!(
        accepted.contains("sec-websocket-accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n"),
        "{accepted}"
    );

    fs::write(
        source_dir.join("WebSocket.terl"),
        r#"module app.WebSocket.

import std.http.{Response, Router}.
import type std.http.{Request, Response, Router}.

pub route(): String -> "/ws".

pub protocol(): String -> "app.room.v1".

pub wrong_target(_request: Request): Response -> Response.text("wrong target").

pub router(): Router -> Router.get(Router.new(), "/ws", wrong_target).
"#,
    )
    .expect("replace WebSocket router with wrong target");
    clear_vm_handler_module_cache_for_test();
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm mismatched WebSocket router");
    let mismatched = handle_vm_stream_http1_request(
        &web_root,
        b"GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
    )
    .expect("mismatched WebSocket target response");
    let mismatched = String::from_utf8(mismatched).expect("mismatch should be UTF-8");
    assert!(mismatched.starts_with("HTTP/1.1 502 "), "{mismatched}");
    assert!(
        mismatched.ends_with(
            "\r\n\r\nerror[serve_router]: websocket route `GET` `/ws` did not resolve to a WebSocket endpoint"
        ),
        "{mismatched}"
    );
    assert!(!mismatched.contains("sec-websocket-accept"), "{mismatched}");

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

#[test]
pub(super) fn vm_stream_request_rejects_malformed_websocket_upgrade_without_hyper() {
    let dir = temp_dir("vm_stream_websocket_malformed_upgrade");
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

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
    )
    .expect("VM stream websocket malformed-upgrade request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 400 "), "{response}");
    assert!(response.ends_with("\r\n\r\nmalformed websocket upgrade request"));
    assert!(!response.contains("fallback"), "{response}");
    assert!(!response.contains("not yet available"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_stream_head_static_asset_omits_body_without_hyper() {
    let dir = temp_dir("vm_stream_static_asset_head");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"HEAD /assets/hello.txt HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream static asset HEAD request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\n"));
    assert!(!response.ends_with("hello asset\n"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_stream_request_serves_manifest_static_response_without_hyper() {
    let dir = temp_dir("vm_stream_manifest_static_response");
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
  "static_responses": [
    { "method": "GET", "route": "/api/status", "status": 202, "content_type": "text/plain; charset=utf-8", "body": "route ok" }
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

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /api/status HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream static response request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 202 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\nroute ok"));

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies manifest static responses suppress bodies for HEAD requests.
///
/// Inputs:
/// - A browser package with a manifest-declared GET static response.
/// - A VM-stream HEAD request for the same route.
///
/// Output:
/// - Test passes when the VM-stream adapter uses the GET route metadata,
///   preserves content length, and omits the response body.
///
/// Transformation:
/// - Locks manifest response HEAD semantics on the VM-owned HTTP path without
///   going through Hyper.
#[test]
pub(super) fn vm_stream_head_manifest_static_response_omits_body_without_hyper() {
    let dir = temp_dir("vm_stream_manifest_static_response_head");
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
  "static_responses": [
    { "method": "GET", "route": "/api/status", "status": 202, "content_type": "text/plain; charset=utf-8", "body": "route ok" }
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

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"HEAD /api/status HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream static response HEAD request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 202 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.contains("Content-Length: 8\r\n"), "{response}");
    assert!(response.ends_with("\r\n\r\n"), "{response}");
    assert!(!response.ends_with("route ok"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies manifest static response routes advertise supported methods.
///
/// Inputs:
/// - A browser package with a manifest-declared GET static response.
/// - A VM-stream OPTIONS request for the same route.
///
/// Output:
/// - Test passes when the VM-stream adapter rejects the unsupported method
///   with a stable `Allow: GET, HEAD` header.
///
/// Transformation:
/// - Locks method-discovery diagnostics for manifest static responses on the
///   VM-owned HTTP path without going through Hyper.
#[test]
pub(super) fn vm_stream_options_manifest_static_response_reports_get_head_allow_without_hyper() {
    let dir = temp_dir("vm_stream_manifest_static_response_options");
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
  "static_responses": [
    { "method": "GET", "route": "/api/status", "status": 202, "content_type": "text/plain; charset=utf-8", "body": "route ok" }
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

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"OPTIONS /api/status HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream static response OPTIONS request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 405 "), "{response}");
    assert!(response.contains("allow: GET, HEAD\r\n"), "{response}");
    assert!(response.ends_with("\r\n\r\nmethod not allowed"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_stream_request_serves_manifest_file_response_without_hyper() {
    let dir = temp_dir("vm_stream_manifest_file_response");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    fs::create_dir_all(web_root.join("downloads")).expect("create downloads");
    fs::write(web_root.join("downloads/report.txt"), "report body\n").expect("write report");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "file_responses": [
    { "method": "GET", "route": "/download", "path": "downloads/report.txt", "status": 206, "content_type": "text/plain; charset=utf-8" }
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

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /download HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream file response request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 206 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\nreport body\n"));

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies manifest file responses suppress bodies for HEAD requests.
///
/// Inputs:
/// - A browser package with a manifest-declared GET file response.
/// - A VM-stream HEAD request for the same route.
///
/// Output:
/// - Test passes when the VM-stream adapter uses the GET file metadata,
///   preserves content length, and omits the response body.
///
/// Transformation:
/// - Locks manifest file response HEAD semantics on the VM-owned HTTP path
///   without going through Hyper.
#[test]
pub(super) fn vm_stream_head_manifest_file_response_omits_body_without_hyper() {
    let dir = temp_dir("vm_stream_manifest_file_response_head");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    fs::create_dir_all(web_root.join("downloads")).expect("create downloads");
    fs::write(web_root.join("downloads/report.txt"), "report body\n").expect("write report");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "file_responses": [
    { "method": "GET", "route": "/download", "path": "downloads/report.txt", "status": 206, "content_type": "text/plain; charset=utf-8" }
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

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"HEAD /download HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream file response HEAD request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 206 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.contains("Content-Length: 12\r\n"), "{response}");
    assert!(response.ends_with("\r\n\r\n"), "{response}");
    assert!(!response.ends_with("report body\n"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies manifest file response routes advertise supported methods.
///
/// Inputs:
/// - A browser package with a manifest-declared GET file response.
/// - A VM-stream OPTIONS request for the same route.
///
/// Output:
/// - Test passes when the VM-stream adapter rejects the unsupported method
///   with a stable `Allow: GET, HEAD` header.
///
/// Transformation:
/// - Locks method-discovery diagnostics for manifest file responses on the
///   VM-owned HTTP path without going through Hyper.
#[test]
pub(super) fn vm_stream_options_manifest_file_response_reports_get_head_allow_without_hyper() {
    let dir = temp_dir("vm_stream_manifest_file_response_options");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    fs::create_dir_all(web_root.join("downloads")).expect("create downloads");
    fs::write(web_root.join("downloads/report.txt"), "report body\n").expect("write report");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "file_responses": [
    { "method": "GET", "route": "/download", "path": "downloads/report.txt", "status": 206, "content_type": "text/plain; charset=utf-8" }
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

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"OPTIONS /download HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream file response OPTIONS request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 405 "), "{response}");
    assert!(response.contains("allow: GET, HEAD\r\n"), "{response}");
    assert!(response.ends_with("\r\n\r\nmethod not allowed"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_stream_request_serves_acme_http01_challenge_without_hyper() {
    let dir = temp_dir("vm_stream_acme_challenge");
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

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /.well-known/acme-challenge/token_123 HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream ACME challenge request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\ntoken_123.account-thumbprint"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_stream_head_request_serves_acme_http01_challenge_without_body() {
    let dir = temp_dir("vm_stream_acme_challenge_head");
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

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"HEAD /.well-known/acme-challenge/token_123 HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream ACME challenge HEAD request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.contains("Content-Length: 28\r\n"), "{response}");
    assert!(response.ends_with("\r\n\r\n"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_stream_request_rejects_invalid_acme_http01_token_without_hyper() {
    let dir = temp_dir("vm_stream_acme_invalid");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /.well-known/acme-challenge/bad.token HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream invalid ACME request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 400 "), "{response}");
    assert!(
        response.contains("ACME HTTP-01 token `bad.token` is invalid"),
        "{response}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_stream_request_returns_404_for_missing_acme_http01_challenge_without_hyper() {
    let dir = temp_dir("vm_stream_acme_missing");
    let web_root = dir.join("_build/web");
    write_valid_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "auto"
domains = ["example.test"]
email = "admin@example.test""#,
    );

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /.well-known/acme-challenge/missing HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream missing ACME request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 404 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\nnot found"), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}
