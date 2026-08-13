#[test]
fn validate_web_package_accepts_valid_manifest_and_assets() {
    let dir = temp_dir("valid_package");
    let web_root = dir.join("web");
    write_valid_package(&web_root);

    validate_web_package(&web_root).expect("valid package");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_rejects_non_browser_target_profile() {
    let dir = temp_dir("non_browser_target_profile");
    let web_root = dir.join("web");
    write_valid_package(&web_root);
    let manifest = fs::read_to_string(web_root.join("manifest.json")).expect("read manifest");
    fs::write(
        web_root.join("manifest.json"),
        manifest.replace(
            "\"target_profile\": \"js.browser\"",
            "\"target_profile\": \"js.shared\"",
        ),
    )
    .expect("write non-browser manifest");

    let err = validate_web_package(&web_root).expect_err("non-browser target profile should fail");

    assert!(err.contains("browser package target profile must be `js.browser`"));
    assert!(err.contains("js.shared"));
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
fn validate_web_package_rejects_sse_route_conflicting_with_http_handler() {
    let dir = temp_dir("sse_handler_route_conflict");
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
    {
      "method": "GET",
      "route": "/events/:id",
      "module": "app.Events",
      "function": "show",
      "arity": 2,
      "source": { "path": "src/app/Events.terl", "line": 10, "column": 1 }
    }
  ],
  "sse": [
    {
      "module": "app.Events",
      "route": "/events/:stream",
      "source": { "path": "src/app/Events.terl", "line": 20, "column": 1 }
    }
  ],
  "assets": []
}
"#,
    )
    .expect("write conflicting SSE manifest");

    let error = validate_web_package(&web_root).expect_err("SSE route conflict must fail");

    assert!(error.contains("error[serve_package]"), "{error}");
    assert!(
        error.contains("SSE route `GET` `/events/:stream` conflicts"),
        "{error}"
    );
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
  "target_profile": "js.browser",
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
fn run_serve_check_rejects_dynamic_handlers_missing_source_metadata() {
    let dir = temp_dir("run_check_handler_runtime_default");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "/api/users");

    let status = run(
        CliCommand {
            verb: Some("serve".to_string()),
            args: vec![web_root.display().to_string(), "--check".to_string()],
        },
        CliState::default(),
    );

    assert_eq!(status, ExitCode::from(1));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn run_serve_check_rejects_dynamic_handlers_with_removed_beam_runtime() {
    let dir = temp_dir("run_check_handler_runtime_beam");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "/api/users");

    let status = run(
        CliCommand {
            verb: Some("serve".to_string()),
            args: vec![
                web_root.display().to_string(),
                "--handler-runtime".to_string(),
                "beam".to_string(),
                "--check".to_string(),
            ],
        },
        CliState::default(),
    );

    assert_eq!(status, ExitCode::from(2));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
#[cfg(not(feature = "acme-live"))]
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
/// - Test passes when the server startup boundary reaches the VM-owned live
///   ACME worker path and then returns the default-build feature diagnostic.
///
/// Transformation:
/// - Calls the serve package startup helper directly so the diagnostic string
///   is asserted without needing to capture process stderr.
#[test]
#[cfg(not(feature = "acme-live"))]
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
        handler_runtime: ServeHandlerRuntime::Static,
        check_only: false,
        overrides: super::super::args::ServeCliOverrides::default(),
    })
    .expect_err("auto TLS should fail before listener binding without a certificate cache");

    assert!(message.starts_with(
        "error[serve_tls]: live ACME issuance requires a compiler build with the `acme-live` feature"
    ));
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
    run_async_serve_test(async {
        let reload_hub = Arc::new(Mutex::new(Vec::new()));
        let response = reload_sse_response(reload_hub, false);

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
#[cfg(not(feature = "acme-live"))]
use super::super::args::{ServeHandlerRuntime, DEFAULT_POLL_MS, DEFAULT_SERVE_HOST};
use super::*;
