use super::*;
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
fn render_http_response_headers_preserves_server_response_contract() {
    let headers = render_http_response_headers(200, "OK", "text/plain; charset=utf-8", 12);
    let text = String::from_utf8(headers).expect("headers should be utf8");

    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(text.contains("Content-Type: text/plain; charset=utf-8\r\n"));
    assert!(text.contains("Content-Length: 12\r\n"));
    assert!(text.contains("Cache-Control: no-cache\r\n"));
    assert!(text.contains("X-Content-Type-Options: nosniff\r\n"));
    assert!(text.contains("Connection: close\r\n"));
    assert!(text.ends_with("\r\n\r\n"));
}

#[test]
fn render_reload_sse_headers_preserves_live_reload_response_contract() {
    let text = render_reload_sse_headers();

    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(text.contains("Content-Type: text/event-stream\r\n"));
    assert!(text.contains("Cache-Control: no-cache\r\n"));
    assert!(text.contains("X-Content-Type-Options: nosniff\r\n"));
    assert!(text.contains("Connection: keep-alive\r\n"));
    assert!(text.contains("Access-Control-Allow-Origin: *\r\n"));
    assert!(text.ends_with(": connected\n\n"));
}
