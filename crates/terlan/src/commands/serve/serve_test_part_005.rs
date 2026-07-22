
#[test]
fn vm_stream_request_serves_acme_like_static_file_for_plain_http_package_without_hyper() {
    let dir = temp_dir("vm_stream_acme_plain_static");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    let static_dir = web_root.join(".well-known/acme-challenge");
    fs::create_dir_all(&static_dir).expect("create static ACME-like directory");
    fs::write(static_dir.join("token_123"), "ordinary static challenge")
        .expect("write static ACME-like file");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /.well-known/acme-challenge/token_123 HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream plain ACME-like static request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    assert!(
        response.contains("content-type: application/octet-stream\r\n"),
        "{response}"
    );
    assert!(response.ends_with("\r\n\r\nordinary static challenge"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_stream_request_serves_static_index_fallback_without_hyper() {
    let dir = temp_dir("vm_stream_static_index");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    fs::write(
        web_root.join("index.html"),
        "<!doctype html><html><body>Hello Terlan</body></html>",
    )
    .expect("write index");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /index.html HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream static index request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    assert!(
        response.contains("content-type: text/html; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.contains("Hello Terlan"), "{response}");
    assert!(response.contains(RELOAD_ENDPOINT), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies static index fallback suppresses bodies for HEAD requests.
///
/// Inputs:
/// - A browser package with an `index.html` file.
/// - A VM-stream HEAD request for `/index.html`.
///
/// Output:
/// - Test passes when the VM-stream adapter serves index metadata, preserves
///   content headers, and omits the injected HTML body.
///
/// Transformation:
/// - Locks HTML index fallback HEAD semantics on the VM-owned HTTP path
///   without going through Hyper.
#[test]
fn vm_stream_head_static_index_fallback_omits_body_without_hyper() {
    let dir = temp_dir("vm_stream_static_index_head");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    fs::write(
        web_root.join("index.html"),
        "<!doctype html><html><body>Hello Terlan</body></html>",
    )
    .expect("write index");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"HEAD /index.html HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream static index HEAD request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
    assert!(
        response.contains("content-type: text/html; charset=utf-8\r\n"),
        "{response}"
    );
    assert!(response.contains("Content-Length: "), "{response}");
    assert!(response.ends_with("\r\n\r\n"), "{response}");
    assert!(!response.contains("Hello Terlan"), "{response}");
    assert!(!response.contains(RELOAD_ENDPOINT), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies static index fallback advertises supported methods.
///
/// Inputs:
/// - A browser package with an `index.html` file.
/// - A VM-stream OPTIONS request for `/index.html`.
///
/// Output:
/// - Test passes when the VM-stream adapter rejects the unsupported method
///   with a stable `Allow: GET, HEAD` header.
///
/// Transformation:
/// - Locks method-discovery diagnostics for HTML index fallback on the
///   VM-owned HTTP path without going through Hyper.
#[test]
fn vm_stream_options_static_index_fallback_reports_get_head_allow_without_hyper() {
    let dir = temp_dir("vm_stream_static_index_options");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    fs::write(
        web_root.join("index.html"),
        "<!doctype html><html><body>Hello Terlan</body></html>",
    )
    .expect("write index");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"OPTIONS /index.html HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream static index OPTIONS request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 405 "), "{response}");
    assert!(response.contains("allow: GET, HEAD\r\n"), "{response}");
    assert!(response.ends_with("\r\n\r\nmethod not allowed"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_stream_request_rejects_static_parent_path_without_hyper() {
    let dir = temp_dir("vm_stream_static_parent");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /../secret.txt HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream parent path request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 400 "), "{response}");
    assert!(response.ends_with("\r\n\r\nbad request"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_stream_request_rejects_unmatched_mutating_method_without_hyper() {
    let dir = temp_dir("vm_stream_static_post");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"POST /assets/hello.txt HTTP/1.1\r\nHost: localhost\r\nContent-Length: 7\r\n\r\npayload",
    )
    .expect("VM stream mutating static request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 405 "), "{response}");
    assert!(response.contains("allow: GET, HEAD\r\n"), "{response}");
    assert!(response.ends_with("\r\n\r\nmethod not allowed"));

    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies static OPTIONS requests advertise supported methods.
///
/// Inputs:
/// - A browser package with a generated static asset.
/// - A VM-stream OPTIONS request for that asset.
///
/// Output:
/// - Test passes when the VM-stream adapter rejects the unsupported method
///   with a stable `Allow: GET, HEAD` header.
///
/// Transformation:
/// - Locks method-discovery diagnostics on the VM-owned static file path
///   without going through Hyper.
#[test]
fn vm_stream_options_static_asset_reports_get_head_allow_without_hyper() {
    let dir = temp_dir("vm_stream_static_options");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"OPTIONS /assets/hello.txt HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream OPTIONS static request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 405 "), "{response}");
    assert!(response.contains("allow: GET, HEAD\r\n"), "{response}");
    assert!(response.ends_with("\r\n\r\nmethod not allowed"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn hyper_request_handler_uses_source_bridge_for_managed_only_module() {
    run_async_serve_test(async {
        clear_vm_handler_module_cache_for_test();
        let dir = temp_dir("hyper_vm_managed_source_handler");
        let project_root = &dir;
        let web_root = project_root.join("_build/web");
        let source_dir = project_root.join("src/app");
        fs::create_dir_all(&source_dir).expect("create source dir");
        fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
        fs::write(
            project_root.join("terlan.toml"),
            "[package]\nname = \"serve_vm_managed_source_demo\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
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
            "module app.Api.\n\nimport std.http.Response.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub handle(_request: Request): Response ->\n    Response.text(\"artifact handler\").with_status(209).\n",
        )
        .expect("write artifact handler source");

        let status = crate::commands::build::run(
            CliCommand {
                verb: Some("build".to_string()),
                args: vec![
                    source_dir.join("Api.terl").display().to_string(),
                    "--target".to_string(),
                    "terlan-vm".to_string(),
                ],
            },
            CliState {
                out_dir: web_root.clone(),
                ..CliState::default()
            },
        );
        assert_eq!(status, ExitCode::SUCCESS);
        assert!(!web_root.join("vm/app_Api.tvm").exists());

        fs::write(
            source_dir.join("Api.terl"),
            "module app.Api.\n\nimport std.http.Response.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub handle(_request: Request): Response ->\n    Response.text(\"managed source handler\").with_status(210).\n",
        )
        .expect("rewrite handler source after artifact build");
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
      "route": "/api/source",
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
        .expect("write managed source handler manifest");
        prewarm_dynamic_handler_sources(&web_root).expect("prewarm managed source handler cache");

        let response = handle_hyper_request(
            typed_request("GET", "/api/source", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;
        let status = response.status();
        let body = serve_response_text(response).await;
        assert_eq!(status, http::StatusCode::from_u16(210).expect("status"));
        assert_eq!(body, "managed source handler");

        fs::remove_dir_all(dir).expect("cleanup");
        clear_vm_handler_module_cache_for_test();
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
fn hyper_request_handler_prefers_physical_static_asset_over_static_response_fallback() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_asset_before_static_response_fallback");
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
    { "method": "GET", "route": "*", "status": 404, "content_type": "text/plain; charset=utf-8", "body": "fallback" }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "static-asset",
      "source_relative_path": "assets/hello.txt",
      "web_relative_path": "assets/hello.txt",
      "fingerprint": 1
    }
  ]
}
"#,
        )
        .expect("write manifest");

        let response = handle_hyper_request(
            typed_request("GET", "/assets/hello.txt", ""),
            web_root.clone(),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(serve_response_text(response).await, "hello asset\n");

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

/// Verifies Hyper reload SSE HEAD requests preserve headers without a body.
///
/// Inputs:
/// - A HEAD request for the reserved reload endpoint.
///
/// Output:
/// - Test passes when the transitional Hyper path returns the SSE route
///   headers, an empty body, and no reload subscriber.
///
/// Transformation:
/// - Aligns the transitional Hyper path with VM-stream HEAD behavior while
///   avoiding unnecessary subscriber allocation for metadata-only requests.
#[test]
fn hyper_request_handler_heads_reload_sse_without_opening_stream() {
    run_async_serve_test(async {
        let reload_hub = Arc::new(Mutex::new(Vec::new()));

        let response = handle_hyper_request(
            typed_request("HEAD", RELOAD_ENDPOINT, ""),
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
        assert_eq!(serve_response_text(response).await, "");
        assert!(
            reload_hub.lock().expect("reload subscribers").is_empty(),
            "HEAD should not register an SSE subscriber"
        );
    });
}

/// Verifies Hyper reload SSE rejects mutating methods before stream setup.
///
/// Inputs:
/// - A POST request for the reserved reload endpoint.
///
/// Output:
/// - Test passes when the transitional Hyper path returns `405 Method Not
///   Allowed` with `Allow: GET, HEAD`.
///
/// Transformation:
/// - Keeps Hyper diagnostics aligned with the VM-stream route contract until
///   production serving fully moves to VM TCP streams.
#[test]
fn hyper_request_handler_rejects_reload_sse_mutating_method() {
    run_async_serve_test(async {
        let response = handle_hyper_request(
            typed_request("POST", RELOAD_ENDPOINT, "payload"),
            PathBuf::from("/tmp/no-web-root-needed"),
            Arc::new(Mutex::new(Vec::new())),
            websocket_hub(),
        )
        .await;

        assert_eq!(response.status(), http::StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(
            response.headers().get(http::header::ALLOW),
            Some(&http::HeaderValue::from_static("GET, HEAD"))
        );
        assert_eq!(
            response.headers().get(http::header::CONTENT_TYPE),
            Some(&http::HeaderValue::from_static("text/plain; charset=utf-8"))
        );
        assert_eq!(serve_response_text(response).await, "method not allowed");
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

/// Verifies auto TLS challenge HEAD requests preserve headers without bodies.
///
/// Inputs:
/// - A web package with adjacent auto TLS metadata.
/// - A cached HTTP-01 token response under `.terlan/tls/acme/http-01`.
///
/// Output:
/// - Test passes when the reserved ACME path returns `200 OK`, text content
///   type, original content length, and an empty response body.
///
/// Transformation:
/// - Locks HEAD handling for ACME challenge routes to the same reserved
///   challenge cache as GET without leaking response bodies.
#[test]
fn hyper_request_handler_serves_acme_http01_head_without_body() {
    run_async_serve_test(async {
        let dir = temp_dir("hyper_acme_challenge_head");
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
            typed_request("HEAD", "/.well-known/acme-challenge/token_123", ""),
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
            response.headers().get(http::header::CONTENT_LENGTH),
            Some(&http::HeaderValue::from_static("28"))
        );
        assert_eq!(serve_response_text(response).await, "");

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
        assert_eq!(
            response.headers().get(http::header::ALLOW),
            Some(&http::HeaderValue::from_static("GET, HEAD"))
        );
        assert_eq!(serve_response_text(response).await, "method not allowed");

        fs::remove_dir_all(dir).expect("cleanup");
    });
}
