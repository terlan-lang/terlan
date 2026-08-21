use super::*;

#[test]
pub(super) fn hyper_request_handler_prefers_dynamic_handler_over_file_fallback() {
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
pub(super) fn hyper_request_handler_prefers_dynamic_handler_over_static_response_fallback() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_handler_before_static_response_fallback");
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
  "static_responses": [
    { "method": "GET", "route": "*", "status": 404, "content_type": "text/plain; charset=utf-8", "body": "fallback" }
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
pub(super) fn hyper_request_handler_executes_dynamic_handler_with_vm_runtime() {
    run_async_serve_test(async {
        clear_vm_handler_module_cache_for_test();
        let dir = temp_dir("hyper_vm_dynamic_handler_success");
        let project_root = &dir;
        let web_root = project_root.join("_build/web");
        let source_dir = project_root.join("src/app");
        fs::create_dir_all(&source_dir).expect("create source dir");
        fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
        fs::write(
            project_root.join("terlan.toml"),
            "[package]\nname = \"serve_vm_demo\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
        )
        .expect("write project manifest");
        fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
        fs::write(
            web_root.join("assets/js/modules/app.js"),
            "export const value = 1;\n",
        )
        .expect("write js asset");
        fs::write(
            source_dir.join("Api.terl"),
            "module app.Api.\n\nimport std.http.Response.\nimport std.core.Option.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub handle(request: Request): Response ->\n    Response.text(request.method() + \":\" + Option.with_default(request.query(\"page\"), \"missing\") + \":\" + Option.with_default(request.header(\"accept\"), \"missing\") + \":\" + Option.with_default(request.cookie(\"session\"), \"missing\") + \":\" + request.body_text()).with_status(203).\n",
        )
        .expect("write handler source");
        fs::write(
            web_root.join("manifest.json"),
            r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    {
      "method": "POST",
      "route": "/api/users",
      "module": "app.Api",
      "function": "handle",
      "arity": 1,
      "source": {
        "path": "src/app/Api.terl",
        "line": 7,
        "column": 5
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
        .expect("write manifest");
        prewarm_dynamic_handler_sources(&web_root).expect("prewarm handler cache");

        let request = Request::builder()
            .method("POST")
            .uri("/api/users?page=2")
            .header("Accept", "application/json")
            .header("Cookie", "session=abc")
            .body(Full::new(Bytes::from("payload")))
            .expect("request");
        let response = handle_hyper_request(
            request,
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        let status = response.status();
        let body = serve_response_text(response).await;
        assert_eq!(
            status,
            http::StatusCode::NON_AUTHORITATIVE_INFORMATION,
            "{body}"
        );
        assert_eq!(body, "POST:2:application/json:abc:payload");

        let request = Request::builder()
            .method("POST")
            .uri("/api/users?page=2")
            .header("Accept", "application/json")
            .header("Cookie", "session=abc")
            .body(Full::new(Bytes::from("payload")))
            .expect("request");
        let response = handle_hyper_request(
            request,
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;
        let status = response.status();
        let body = serve_response_text(response).await;
        assert_eq!(
            status,
            http::StatusCode::NON_AUTHORITATIVE_INFORMATION,
            "{body}"
        );
        assert_eq!(body, "POST:2:application/json:abc:payload");

        fs::write(
            source_dir.join("Api.terl"),
            "module app.Api.\n\nimport std.http.Response.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub handle(_request: Request): Response ->\n    Response.text(\"changed handler\").with_status(202).\n",
        )
        .expect("rewrite handler source");
        // The direct request harness does not spawn the production file
        // watcher, so deliver the same generation-invalidation event here.
        clear_vm_handler_module_cache_for_test();
        let request = Request::builder()
            .method("POST")
            .uri("/api/users?page=2")
            .header("Accept", "application/json")
            .header("Cookie", "session=abc")
            .body(Full::new(Bytes::from("payload")))
            .expect("request");
        let response = handle_hyper_request(
            request,
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;
        let status = response.status();
        let body = serve_response_text(response).await;
        assert_eq!(status, http::StatusCode::ACCEPTED, "{body}");
        assert_eq!(body, "changed handler");

        fs::remove_dir_all(dir).expect("cleanup");
        clear_vm_handler_module_cache_for_test();
    });
}

#[test]
pub(super) fn vm_stream_request_executes_dynamic_handler_without_hyper() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_dynamic_handler_success");
    let project_root = &dir;
    let web_root = project_root.join("_build/web");
    let source_dir = project_root.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        project_root.join("terlan.toml"),
        "[package]\nname = \"serve_vm_stream_demo\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
    )
    .expect("write project manifest");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        source_dir.join("Api.terl"),
        "module app.Api.\n\nimport std.http.Response.\nimport std.core.Option.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub handle(request: Request): Response ->\n    Response.text(request.method() + \":\" + Option.with_default(request.query(\"page\"), \"missing\") + \":\" + Option.with_default(request.header(\"accept\"), \"missing\") + \":\" + Option.with_default(request.cookie(\"session\"), \"missing\") + \":\" + request.body_text()).with_status(203).\n",
    )
    .expect("write handler source");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    {
      "method": "POST",
      "route": "/api/users",
      "module": "app.Api",
      "function": "handle",
      "arity": 1,
      "source": {
        "path": "src/app/Api.terl",
        "line": 7,
        "column": 5
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
    .expect("write manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm handler cache");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"POST /api/users?page=2 HTTP/1.1\r\nHost: localhost\r\nAccept: application/json\r\nCookie: session=abc\r\nContent-Length: 7\r\n\r\npayload",
    )
    .expect("VM stream request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 203 "), "{response}");
    assert!(response.contains("content-type: text/plain; charset=utf-8\r\n"));
    assert!(response.ends_with("\r\n\r\nPOST:2:application/json:abc:payload"));

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

/// Verifies production VM-stream dispatch activates source router middleware.
///
/// Inputs:
/// - A source module with `router/0`, one conditional middleware function, and
///   a dynamic handler with a distinct response.
/// - Generated-package manifest rows locating that source module and routes.
///
/// Output:
/// - Test passes when the real VM HTTP socket path returns the middleware's
///   typed response for one route and continues into the handler for another.
///
/// Transformation:
/// - Exercises manifest selection, cached source compilation, router
///   materialization, typed middleware dispatch, response bridging, and HTTP/1
///   serialization as one production request path.
#[test]
pub(super) fn vm_stream_request_activates_materialized_router_middleware_without_hyper() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_materialized_router_middleware");
    let project_root = &dir;
    let web_root = project_root.join("_build/web");
    let source_dir = project_root.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        project_root.join("terlan.toml"),
        "[package]\nname = \"serve_vm_router_middleware\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
    )
    .expect("write project manifest");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        source_dir.join("Api.terl"),
        r#"module app.Api.

import std.http.{Response, Router}.
import std.http.Router.{Continue, Respond}.
import type std.http.{Request, Response, Router}.
import type std.http.Error.HttpError.
import type std.http.Router.MiddlewareResult.

pub gate(request: Request): MiddlewareResult ->
    case request.path() {
        "/api/users" -> Respond(Response.text("blocked by middleware", status = 401));
        _ -> Continue
    }.

pub handle(_request: Request): Response ->
    Response.text("handler reached", status = 299).

pub cached(_request: Request): Response ->
    Response.text("source handler must not run", status = 599).

pub broken(_request: Request): Response ->
    Response.file("../secret").

pub recover(error: HttpError): Response ->
    Response.text(error.message(), status = error.status()).

pub not_found(request: Request): Response ->
    Response.text("fallback:" + request.path(), status = 404).

pub outer_after(request: Request, response: Response): Response ->
    case request.path() {
        "/api/users" -> response.with_status(407);
        _ -> response.with_status(207)
    }.

pub inner_after(_request: Request, response: Response): Response ->
    response.with_status(208).

pub router(): Router ->
    Router.use(Router.new(), gate)
        .map_response(outer_after)
        .map_response(inner_after)
        .post("/api/users", handle)
        .post("/api/allowed", handle)
        .get("/cached", cached)
        .get("/broken", broken)
        .fallback(not_found)
        .error(recover).
"#,
    )
    .expect("write router source");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    {
      "method": "POST",
      "route": "/api/users",
      "module": "app.Api",
      "function": "handle",
      "arity": 1,
      "source": {
        "path": "src/app/Api.terl",
        "line": 11,
        "column": 5
      }
    },
    {
      "method": "POST",
      "route": "/api/allowed",
      "module": "app.Api",
      "function": "handle",
      "arity": 1,
      "source": {
        "path": "src/app/Api.terl",
        "line": 14,
        "column": 5
      }
    },
    {
      "method": "GET",
      "route": "/broken",
      "module": "app.Api",
      "function": "broken",
      "arity": 1,
      "source": {
        "path": "src/app/Api.terl",
        "line": 17,
        "column": 5
      }
    },
    {
      "method": "GET",
      "route": "/stale/*",
      "module": "app.Api",
      "function": "handle",
      "arity": 1,
      "source": {
        "path": "src/app/Api.terl",
        "line": 14,
        "column": 5
      }
    },
    {
      "method": "GET",
      "route": "*",
      "module": "app.Api",
      "function": "not_found",
      "arity": 1,
      "source": {
        "path": "src/app/Api.terl",
        "line": 20,
        "column": 5
      }
    }
  ],
  "static_responses": [
    {
      "module": "app.Api",
      "function": "cached",
      "arity": 1,
      "method": "GET",
      "route": "/cached",
      "status": 209,
      "content_type": "text/plain; charset=utf-8",
      "body": "cached response",
      "source": {
        "path": "src/app/Api.terl",
        "line": 17,
        "column": 5
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
    .expect("write manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm handler cache");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"POST /api/users HTTP/1.1\r\nHost: localhost\r\nContent-Length: 7\r\n\r\npayload",
    )
    .expect("VM stream middleware request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 407 "), "{response}");
    assert!(response.ends_with("\r\n\r\nblocked by middleware"));
    assert!(!response.contains("handler reached"), "{response}");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"POST /api/allowed HTTP/1.1\r\nHost: localhost\r\nContent-Length: 7\r\n\r\npayload",
    )
    .expect("VM stream middleware continuation request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 207 "), "{response}");
    assert!(response.ends_with("\r\n\r\nhandler reached"));

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /cached HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream static router request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 207 "), "{response}");
    assert!(response.ends_with("\r\n\r\ncached response"));

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /broken HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream router recovery request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 500 "), "{response}");
    assert!(response.contains("../secret"), "{response}");
    assert!(!response.ends_with("\r\n\r\nrecovered"), "{response}");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /missing HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream graph fallback request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 207 "), "{response}");
    assert!(
        response.ends_with("\r\n\r\nfallback:/missing"),
        "{response}"
    );

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /stale/path HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream stale fallback request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 502 "), "{response}");
    assert!(
        response.ends_with(
            "\r\n\r\nerror[serve_router]: manifest route `GET` `/stale/*` does not match materialized route `GET` `*`"
        ),
        "{response}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

#[test]
pub(super) fn vm_plain_http1_connection_serves_static_response_without_host_async_or_hyper() {
    let dir = temp_dir("vm_plain_connection_static");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n".to_vec();
    let request_len = request.len();
    let mut stream = Cursor::new(request);

    serve_vm_plain_http1_connection(&mut stream, &web_root).expect("serve plain VM connection");

    let output = &stream.get_ref()[request_len..];
    let text = String::from_utf8_lossy(output);
    assert!(text.starts_with("HTTP/1.1 200 OK"), "{text}");
    assert!(text.contains("content-type: text/html"), "{text}");
    assert!(text.contains("<!doctype html>"), "{text}");
    assert!(text.contains(RELOAD_ENDPOINT), "{text}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_plain_http1_request_complete_tracks_declared_body_length() {
    assert!(
        vm_plain_http1_request_complete(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .expect("complete get")
    );
    assert!(!vm_plain_http1_request_complete(
        b"POST /api HTTP/1.1\r\nHost: localhost\r\nContent-Length: 4\r\n\r\nab"
    )
    .expect("partial post"));
    assert!(vm_plain_http1_request_complete(
        b"POST /api HTTP/1.1\r\nHost: localhost\r\nContent-Length: 4\r\n\r\nabcd"
    )
    .expect("complete post"));
}

#[test]
pub(super) fn vm_plain_http1_request_complete_rejects_invalid_content_length() {
    let message = vm_plain_http1_request_complete(
        b"POST /api HTTP/1.1\r\nHost: localhost\r\nContent-Length: nope\r\n\r\n",
    )
    .expect_err("invalid content length should fail");

    assert_eq!(
        message.to_string(),
        "invalid VM plain HTTP content-length value"
    );
}

/// Verifies malformed HTTP/1 input receives a serve-level bad request.
///
/// Inputs:
/// - A generated web package root.
/// - Raw malformed HTTP/1 request bytes injected through the VM stream bridge.
///
/// Output:
/// - Test passes when the bridge returns a stable `400 Bad Request` wire
///   response instead of leaking a VM parser diagnostic as a command error.
///
/// Transformation:
/// - Keeps the lower VM HTTP parser strict while locking the production-facing
///   serve adapter to user-visible HTTP semantics.
#[test]
pub(super) fn vm_stream_request_returns_bad_request_for_malformed_http_without_hyper() {
    let dir = temp_dir("vm_stream_malformed_http");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    let response = handle_vm_stream_http1_request(&web_root, b"GET / HTTP/1.1\r\nbad\r\n\r\n")
        .expect("VM stream malformed request should produce response bytes");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 400 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(
        response.contains("bad request: failed to parse VM HTTP request: invalid header name"),
        "{response}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies incomplete VM-stream request headers receive HTTP 400 after EOF.
///
/// Inputs:
/// - A generated web package root.
/// - Raw HTTP/1 bytes with no terminating header boundary before client EOF.
///
/// Output:
/// - Test passes when the serve adapter returns a stable `400 Bad Request`
///   response rather than treating the half-closed stream as idle keep-alive.
///
/// Transformation:
/// - Locks the production-facing diagnostic for VM TCP write-side EOF before a
///   request can be parsed.
#[test]
pub(super) fn vm_stream_request_returns_bad_request_for_incomplete_headers_without_hyper() {
    let dir = temp_dir("vm_stream_incomplete_headers");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    let response =
        handle_vm_stream_http1_request(&web_root, b"GET / HTTP/1.1\r\nHost: localhost\r\n")
            .expect("VM stream incomplete headers should produce response bytes");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 400 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(
        response.contains("bad request: VM HTTP request closed before headers completed"),
        "{response}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies malformed VM-stream request body framing becomes HTTP 400.
///
/// Inputs:
/// - A generated web package root.
/// - Raw HTTP/1 bytes with an invalid `Content-Length` value.
///
/// Output:
/// - Test passes when the serve adapter returns a stable `400 Bad Request`
///   response instead of exposing the VM parser error as a command failure.
///
/// Transformation:
/// - Locks the protocol boundary for production-facing VM streams: strict VM
///   HTTP parsing remains internal, and clients receive HTTP diagnostics.
#[test]
pub(super) fn vm_stream_request_returns_bad_request_for_invalid_content_length_without_hyper() {
    let dir = temp_dir("vm_stream_invalid_content_length");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"POST /api HTTP/1.1\r\nHost: localhost\r\nContent-Length: nope\r\n\r\n",
    )
    .expect("VM stream invalid content length should produce response bytes");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 400 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(
        response.contains("bad request: VM HTTP Content-Length `nope` is invalid"),
        "{response}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies oversized VM-stream body declarations become HTTP 400.
///
/// Inputs:
/// - A generated web package root.
/// - Raw HTTP/1 bytes declaring a request body beyond the VM parser limit.
///
/// Output:
/// - Test passes when the serve adapter returns a stable `400 Bad Request`
///   response instead of treating the parser rejection as a command failure.
///
/// Transformation:
/// - Keeps request-size enforcement inside VM HTTP parsing while preserving
///   HTTP-shaped client diagnostics at the production-facing serve boundary.
#[test]
pub(super) fn vm_stream_request_returns_bad_request_for_oversized_body_without_hyper() {
    let dir = temp_dir("vm_stream_oversized_body");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"POST /api HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1048577\r\n\r\n",
    )
    .expect("VM stream oversized body should produce response bytes");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 400 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(
        response.contains("bad request: VM HTTP request exceeded 1 MiB body limit"),
        "{response}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies truncated VM-stream request bodies become HTTP 400.
///
/// Inputs:
/// - A generated web package root.
/// - Raw HTTP/1 bytes declaring a larger body than the client sends before EOF.
///
/// Output:
/// - Test passes when VM TCP half-close semantics let the serve adapter return
///   a stable `400 Bad Request` response while keeping the response path open.
///
/// Transformation:
/// - Locks pollable VM HTTP EOF handling so incomplete bodies do not park
///   forever or surface as missing response bytes.
#[test]
pub(super) fn vm_stream_request_returns_bad_request_for_truncated_body_without_hyper() {
    let dir = temp_dir("vm_stream_truncated_body");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"POST /api HTTP/1.1\r\nHost: localhost\r\nContent-Length: 8\r\n\r\nshort",
    )
    .expect("VM stream truncated body should produce response bytes");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 400 "), "{response}");
    assert!(
        response.contains("content-type: text/plain; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(
        response.contains("bad request: VM HTTP request body ended early"),
        "{response}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_stream_request_passes_route_params_to_dynamic_handler_without_hyper() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_dynamic_handler_params");
    let project_root = &dir;
    let web_root = project_root.join("_build/web");
    let source_dir = project_root.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        project_root.join("terlan.toml"),
        "[package]\nname = \"serve_vm_stream_params_demo\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
    )
    .expect("write project manifest");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        source_dir.join("Api.terl"),
        "module app.Api.\n\nimport std.http.Response.\nimport std.core.Option.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub show(request: Request): Response ->\n    Response.text(Option.with_default(request.param(\"id\"), \"missing\") + \":\" + Option.with_default(request.param(\"name\"), \"missing\")).with_status(218).\n",
    )
    .expect("write handler source");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    {
      "method": "GET",
      "route": "/api/users/{id:Int}/files/:name",
      "module": "app.Api",
      "function": "show",
      "arity": 1,
      "source": {
        "path": "src/app/Api.terl",
        "line": 7,
        "column": 5
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
    .expect("write manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm handler cache");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /api/users/42/files/read%20me HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream route-param request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 218 "), "{response}");
    assert!(response.contains("content-type: text/plain; charset=utf-8\r\n"));
    assert!(response.ends_with("\r\n\r\n42:read me"));

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}
