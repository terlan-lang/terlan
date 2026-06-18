use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use terlan_safenative::http as native_http;

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
        "terlan_serve_handler_{name}_{}_{}",
        std::process::id(),
        nanos
    ))
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
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
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
fn manifest_handler_for_request_matches_get_and_head() {
    let dir = temp_dir("handler_match");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "/api/users");

    let get = manifest_handler_for_request(&web_root, "GET", "/api/users")
        .expect("GET handler should match");
    let head = manifest_handler_for_request(&web_root, "HEAD", "/api/users")
        .expect("HEAD should reuse GET handler metadata");

    assert_eq!(get.function, "handle");
    assert_eq!(head.module, "app.Api");
    assert!(manifest_handler_for_request(&web_root, "POST", "/api/users").is_none());
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn beam_ebin_dir_for_web_root_uses_build_root_sibling() {
    let web_root = PathBuf::from("_build/web");

    let ebin = beam_ebin_dir_for_web_root(&web_root).expect("resolve ebin");

    assert_eq!(ebin, PathBuf::from("_build/ebin"));
}

#[test]
fn render_beam_handler_eval_passes_request_map_and_target() {
    let request = native_http::Request::from_parts("GET", "/api/users", "");
    let eval = render_beam_handler_eval("app_api", "handle", &request);

    assert!(eval.contains(
        "Request = #{method => <<71,69,84>>, path => <<47,97,112,105,47,117,115,101,114,115>>}"
    ));
    assert!(eval.contains("catch app_api:handle(Request)"));
    assert!(eval.contains("{terlan_response, Status, ContentType, Body}"));
}

#[test]
fn parse_beam_handler_stdout_accepts_stable_response_protocol() {
    let parsed = parse_beam_handler_stdout(b"201\napplication/json; charset=utf-8\n{\"ok\":true}")
        .expect("parse handler response");

    assert_eq!(
        parsed,
        BeamHandlerResponse {
            status: 201,
            content_type: "application/json; charset=utf-8".to_string(),
            body: br#"{"ok":true}"#.to_vec(),
        }
    );
}

#[test]
fn beam_handler_response_converts_from_native_http_response() {
    let mut native = native_http::text("created");
    native_http::status(&mut native, 201);

    let response =
        BeamHandlerResponse::from_native_response(&native).expect("convert native response");

    assert_eq!(
        response,
        BeamHandlerResponse {
            status: 201,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: b"created".to_vec(),
        }
    );
}

#[test]
fn beam_handler_response_rejects_invalid_native_status() {
    let mut native = native_http::text("bad");
    native_http::status(&mut native, 900);

    let err = BeamHandlerResponse::from_native_response(&native)
        .expect_err("invalid native status should fail");

    assert!(err.contains("native HTTP response status `900`"));
}

#[test]
fn parse_beam_handler_stdout_rejects_bad_status() {
    let err =
        parse_beam_handler_stdout(b"900\ntext/plain\nbad").expect_err("invalid status should fail");

    assert!(err.contains("outside HTTP range"));
}

#[test]
fn execute_beam_handler_reports_missing_ebin_before_running_erl() {
    let dir = temp_dir("missing_ebin");
    let web_root = dir.join("_build/web");
    fs::create_dir_all(&web_root).expect("create web root");
    let handler = WebPackageHandler {
        method: "GET".to_string(),
        route: "/api".to_string(),
        module: "app.Api".to_string(),
        function: "handle".to_string(),
        arity: 1,
    };

    let err = execute_beam_handler(&web_root, &handler, "GET", "/api")
        .expect_err("missing ebin should fail");

    assert!(err.contains("BEAM ebin directory"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn execute_beam_handler_reports_missing_beam_before_running_erl() {
    let dir = temp_dir("missing_beam");
    let web_root = dir.join("_build/web");
    fs::create_dir_all(dir.join("_build/ebin")).expect("create ebin");
    fs::create_dir_all(&web_root).expect("create web root");
    let handler = WebPackageHandler {
        method: "GET".to_string(),
        route: "/api".to_string(),
        module: "app.Api".to_string(),
        function: "handle".to_string(),
        arity: 1,
    };

    let err = execute_beam_handler(&web_root, &handler, "GET", "/api")
        .expect_err("missing beam should fail");

    assert!(err.contains("BEAM module `app.Api`"));
    assert!(err.contains("app_api.beam"));
    fs::remove_dir_all(dir).expect("cleanup");
}
