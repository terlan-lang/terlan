use super::beam_eval::parse_query_params;
use super::*;
use crate::commands::serve::validate_web_package;
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

/// Writes a browser package fixture with one dynamic handler source span.
///
/// Inputs:
/// - `web_root`: target package directory.
/// - `source_path`: source path to record on the handler.
/// - `line`: one-based source line.
/// - `column`: one-based source column.
///
/// Output:
/// - Filesystem fixture containing a manifest handler with source metadata.
///
/// Transformation:
/// - Creates source-aware handler manifest rows for validation and matching
///   tests without requiring compiler-generated web packages.
fn write_package_with_handler_source(
    web_root: &Path,
    source_path: &str,
    line: usize,
    column: usize,
) {
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
      "route": "/api/users",
      "module": "app.Api",
      "function": "handle",
      "arity": 1,
      "source": {{
        "path": "{source_path}",
        "line": {line},
        "column": {column}
      }}
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
    .expect("write handler source manifest");
}

/// Writes a browser package fixture with static responses.
///
/// Inputs:
/// - `web_root`: target package directory.
///
/// Output:
/// - Filesystem fixture containing static-response route rows.
///
/// Transformation:
/// - Creates exact and fallback static-response rows so route selection can be
///   tested without starting the local HTTP server.
fn write_package_with_static_responses(web_root: &Path) {
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "static_responses": [
    {
      "method": "GET",
      "route": "/about",
      "status": 200,
      "content_type": "text/html; charset=utf-8",
      "body": "<main>About</main>",
      "source": {
        "path": "src/app/Http.terl",
        "line": 11,
        "column": 5
      }
    },
    {
      "method": "GET",
      "route": "*",
      "status": 404,
      "content_type": "text/plain; charset=utf-8",
      "body": "missing"
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
    .expect("write static response manifest");
}

/// Writes a browser package fixture with file responses.
///
/// Inputs:
/// - `web_root`: target package directory.
///
/// Output:
/// - Filesystem fixture containing exact and fallback file-response route rows.
///
/// Transformation:
/// - Creates route-backed file response rows so route selection can be tested
///   without starting the local HTTP server.
fn write_package_with_file_responses(web_root: &Path) {
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::create_dir_all(web_root.join("downloads")).expect("create downloads");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(web_root.join("downloads/report.txt"), "report\n").expect("write report");
    fs::write(web_root.join("downloads/missing.txt"), "missing\n").expect("write missing");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "file_responses": [
    {
      "method": "GET",
      "route": "/download",
      "path": "downloads/report.txt",
      "status": 200,
      "content_type": "text/plain; charset=utf-8",
      "source": {
        "path": "src/app/Http.terl",
        "line": 14,
        "column": 5
      }
    },
    {
      "method": "GET",
      "route": "/files/*",
      "path": "downloads/missing.txt",
      "status": 404
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
    .expect("write file response manifest");
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

    assert_eq!(get.handler.function, "handle");
    assert_eq!(head.handler.module, "app.Api");
    assert!(manifest_handler_for_request(&web_root, "POST", "/api/users").is_none());
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies handler source metadata is preserved after route selection.
///
/// Inputs:
/// - Browser package manifest with one handler source span.
///
/// Output:
/// - Test passes when package validation accepts the source span and route
///   matching returns it.
///
/// Transformation:
/// - Locks the manifest-side source-aware debug metadata contract for local
///   serving and future observability.
#[test]
fn manifest_handler_for_request_preserves_source_metadata() {
    let dir = temp_dir("handler_source_match");
    let web_root = dir.join("web");
    write_package_with_handler_source(&web_root, "src/app/Api.terl", 12, 5);

    super::validate_handler(
        &manifest_handler_for_request(&web_root, "GET", "/api/users")
            .expect("handler should match")
            .handler,
    )
    .expect("source metadata should validate");
    let matched =
        manifest_handler_for_request(&web_root, "GET", "/api/users").expect("handler should match");
    let source = matched.handler.source.as_ref().expect("source metadata");

    assert_eq!(source.path, "src/app/Api.terl");
    assert_eq!(source.line, 12);
    assert_eq!(source.column, 5);
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies unsafe handler source paths fail package validation.
///
/// Inputs:
/// - Browser package manifest with a parent-directory source path.
///
/// Output:
/// - Test passes when validation rejects the unsafe source path.
///
/// Transformation:
/// - Prevents generated manifests from leaking arbitrary filesystem paths into
///   local serve logs or development error pages.
#[test]
fn validate_handler_rejects_unsafe_source_path() {
    let dir = temp_dir("handler_source_unsafe");
    let web_root = dir.join("web");
    write_package_with_handler_source(&web_root, "../src/app/Api.terl", 12, 5);

    let err = validate_web_package(&web_root).expect_err("unsafe source path should fail");

    assert!(err.contains("has unsafe source path"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies zero source positions fail package validation.
///
/// Inputs:
/// - Browser package manifest with line zero.
///
/// Output:
/// - Test passes when validation rejects zero-based source positions.
///
/// Transformation:
/// - Keeps source spans one-based like compiler diagnostics.
#[test]
fn validate_handler_rejects_zero_source_position() {
    let dir = temp_dir("handler_source_zero");
    let web_root = dir.join("web");
    write_package_with_handler_source(&web_root, "src/app/Api.terl", 0, 5);

    let err = validate_web_package(&web_root).expect_err("zero source position should fail");

    assert!(err.contains("source span must use one-based line and column"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_static_response_for_request_matches_exact_and_head() {
    let dir = temp_dir("static_response_match");
    let web_root = dir.join("web");
    write_package_with_static_responses(&web_root);

    let get = manifest_static_response_for_request(&web_root, "GET", "/about")
        .expect("GET static response should match");
    let head = manifest_static_response_for_request(&web_root, "HEAD", "/about")
        .expect("HEAD should reuse GET static response");

    assert_eq!(get.status, 200);
    assert_eq!(get.body, "<main>About</main>");
    assert_eq!(
        get.source.as_ref().expect("static source").path,
        "src/app/Http.terl"
    );
    assert_eq!(head.route, "/about");
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_static_response_for_request_applies_fallback() {
    let dir = temp_dir("static_response_fallback");
    let web_root = dir.join("web");
    write_package_with_static_responses(&web_root);

    let fallback = manifest_static_response_for_request(&web_root, "GET", "/missing")
        .expect("fallback static response should match");

    assert_eq!(fallback.status, 404);
    assert_eq!(fallback.route, "*");
    assert_eq!(fallback.body, "missing");
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_file_response_for_request_matches_exact_and_head() {
    let dir = temp_dir("file_response_match");
    let web_root = dir.join("web");
    write_package_with_file_responses(&web_root);

    let (get, get_path) = manifest_file_response_for_request(&web_root, "GET", "/download")
        .expect("GET file response should match");
    let (head, head_path) = manifest_file_response_for_request(&web_root, "HEAD", "/download")
        .expect("HEAD should reuse GET file response");

    assert_eq!(get.status, 200);
    assert_eq!(get.path, "downloads/report.txt");
    assert_eq!(
        get.source.as_ref().expect("file source").path,
        "src/app/Http.terl"
    );
    assert_eq!(get_path, web_root.join("downloads/report.txt"));
    assert_eq!(head.route, "/download");
    assert_eq!(head_path, web_root.join("downloads/report.txt"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_file_response_for_request_matches_wildcard() {
    let dir = temp_dir("file_response_wildcard");
    let web_root = dir.join("web");
    write_package_with_file_responses(&web_root);

    let (matched, path) = manifest_file_response_for_request(&web_root, "GET", "/files/anything")
        .expect("wildcard file response should match");

    assert_eq!(matched.status, 404);
    assert_eq!(matched.route, "/files/*");
    assert_eq!(path, web_root.join("downloads/missing.txt"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Proves explicit HEAD handlers are preferred over GET fallback handlers.
#[test]
fn manifest_handler_for_request_prefers_explicit_head() {
    let dir = temp_dir("handler_explicit_head");
    let web_root = dir.join("web");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    { "method": "GET", "route": "/health", "module": "app.Api", "function": "get_health", "arity": 1 },
    { "method": "HEAD", "route": "/health", "module": "app.Api", "function": "head_health", "arity": 1 }
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
    .expect("write handler manifest");

    let matched = manifest_handler_for_request(&web_root, "HEAD", "/health")
        .expect("HEAD handler should match");

    assert_eq!(matched.handler.function, "head_health");
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_handler_for_request_matches_route_params() {
    let dir = temp_dir("handler_param_match");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "/users/:id");

    let matched = manifest_handler_for_request(&web_root, "GET", "/users/42")
        .expect("param route should match");

    assert_eq!(matched.handler.route, "/users/:id");
    assert_eq!(matched.params, vec![("id".to_string(), "42".to_string())]);
    assert!(manifest_handler_for_request(&web_root, "GET", "/users/42/edit").is_none());
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies typed brace route params match and capture like colon params.
///
/// Inputs:
/// - Package manifest route `/users/{id:Int}`.
/// - Concrete request path `/users/42`.
///
/// Output:
/// - Test passes when the handler matches and captures `id = 42`.
///
/// Transformation:
/// - Exercises the documented typed route parameter form without changing
///   runtime request payload shape.
#[test]
fn manifest_handler_for_request_matches_typed_route_params() {
    let dir = temp_dir("handler_typed_param_match");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "/users/{id:Int}");

    let matched = manifest_handler_for_request(&web_root, "GET", "/users/42")
        .expect("typed param route should match");

    assert_eq!(matched.handler.route, "/users/{id:Int}");
    assert_eq!(matched.params, vec![("id".to_string(), "42".to_string())]);
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies typed integer route params reject non-integer segments.
///
/// Inputs:
/// - Package manifest route `/users/{id:Int}`.
/// - Concrete request path `/users/alice`.
///
/// Output:
/// - Test passes when no handler is selected.
///
/// Transformation:
/// - Applies typed route validation during route matching before a handler can
///   receive an invalid `Int` route argument.
#[test]
fn manifest_handler_for_request_rejects_invalid_int_route_param() {
    let dir = temp_dir("handler_typed_int_invalid");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "/users/{id:Int}");

    assert!(manifest_handler_for_request(&web_root, "GET", "/users/alice").is_none());
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies typed boolean route params match only boolean text.
///
/// Inputs:
/// - Package manifest route `/users/{active:Bool}`.
/// - Concrete request paths with `true`, `false`, and `maybe`.
///
/// Output:
/// - Test passes when only `true` and `false` dispatch to the handler.
///
/// Transformation:
/// - Applies typed route validation during route matching before serve-time
///   BEAM argument decoding.
#[test]
fn manifest_handler_for_request_matches_bool_route_params() {
    let dir = temp_dir("handler_typed_bool_match");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "/users/{active:Bool}");

    let active =
        manifest_handler_for_request(&web_root, "GET", "/users/true").expect("true should match");
    let inactive =
        manifest_handler_for_request(&web_root, "GET", "/users/false").expect("false should match");

    assert_eq!(
        active.params,
        vec![("active".to_string(), "true".to_string())]
    );
    assert_eq!(
        inactive.params,
        vec![("active".to_string(), "false".to_string())]
    );
    assert!(manifest_handler_for_request(&web_root, "GET", "/users/maybe").is_none());
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_handler_for_request_decodes_route_params() {
    let dir = temp_dir("handler_param_decode");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "/users/:id");

    let matched = manifest_handler_for_request(&web_root, "GET", "/users/alice%20smith")
        .expect("encoded param route should match");

    assert_eq!(
        matched.params,
        vec![("id".to_string(), "alice smith".to_string())]
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_handler_for_request_matches_wildcard_route() {
    let dir = temp_dir("handler_wildcard_match");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "/assets/*");

    let matched = manifest_handler_for_request(&web_root, "GET", "/assets/js/app.js")
        .expect("wildcard route should match");

    assert_eq!(matched.handler.route, "/assets/*");
    assert_eq!(
        matched.params,
        vec![("*".to_string(), "js/app.js".to_string())]
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_handler_for_request_decodes_wildcard_route_params() {
    let dir = temp_dir("handler_wildcard_decode");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "/assets/*");

    let matched = manifest_handler_for_request(&web_root, "GET", "/assets/a%20b/c%2Fd.txt")
        .expect("encoded wildcard route should match");

    assert_eq!(
        matched.params,
        vec![("*".to_string(), "a b/c/d.txt".to_string())]
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_handler_for_request_rejects_invalid_utf8_route_param() {
    let dir = temp_dir("handler_param_invalid_utf8");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "/users/:id");

    assert!(manifest_handler_for_request(&web_root, "GET", "/users/%FF").is_none());
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_handler_for_request_applies_route_precedence() {
    let dir = temp_dir("handler_precedence");
    let web_root = dir.join("web");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    { "method": "GET", "route": "/users/*", "module": "app.Api", "function": "wild", "arity": 1 },
    { "method": "GET", "route": "/users/:id", "module": "app.Api", "function": "show", "arity": 1 },
    { "method": "GET", "route": "/users/all", "module": "app.Api", "function": "all", "arity": 1 },
    { "method": "GET", "route": "/*", "module": "app.Api", "function": "fallback", "arity": 1 }
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
    .expect("write handler manifest");

    let exact =
        manifest_handler_for_request(&web_root, "GET", "/users/all").expect("exact should match");
    let param =
        manifest_handler_for_request(&web_root, "GET", "/users/42").expect("param should match");
    let wildcard = manifest_handler_for_request(&web_root, "GET", "/users/42/edit")
        .expect("wildcard should match");
    let fallback =
        manifest_handler_for_request(&web_root, "GET", "/other").expect("fallback should match");

    assert_eq!(exact.handler.function, "all");
    assert_eq!(param.handler.function, "show");
    assert_eq!(wildcard.handler.function, "wild");
    assert_eq!(fallback.handler.function, "fallback");
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies the canonical `*` fallback route matches otherwise unmatched paths.
///
/// Inputs:
/// - A browser package manifest with one `GET *` handler.
///
/// Output:
/// - Test passes when arbitrary request paths select the fallback handler.
///
/// Transformation:
/// - Exercises the manifest matcher route layer without requiring BEAM handler
///   execution.
#[test]
fn manifest_handler_for_request_matches_canonical_fallback_route() {
    let dir = temp_dir("handler_canonical_fallback");
    let web_root = dir.join("web");
    write_package_with_handler(&web_root, "*");

    let matched = manifest_handler_for_request(&web_root, "GET", "/anything/here")
        .expect("fallback route should match");

    assert_eq!(matched.handler.route, "*");
    assert!(matched.params.is_empty());
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies canonical fallback has lower precedence than other route classes.
///
/// Inputs:
/// - A manifest containing exact, parameter, wildcard, and canonical fallback
///   handlers for the same HTTP method.
///
/// Output:
/// - Test passes when route selection follows exact, parameter, wildcard, then
///   fallback precedence.
///
/// Transformation:
/// - Locks the `*` fallback into the same precedence model as generated
///   router manifests.
#[test]
fn manifest_handler_for_request_applies_canonical_fallback_precedence() {
    let dir = temp_dir("handler_canonical_fallback_precedence");
    let web_root = dir.join("web");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    { "method": "GET", "route": "*", "module": "app.Api", "function": "fallback", "arity": 1 },
    { "method": "GET", "route": "/users/*", "module": "app.Api", "function": "wild", "arity": 1 },
    { "method": "GET", "route": "/users/:id", "module": "app.Api", "function": "show", "arity": 1 },
    { "method": "GET", "route": "/users/all", "module": "app.Api", "function": "all", "arity": 1 }
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
    .expect("write handler manifest");

    let exact =
        manifest_handler_for_request(&web_root, "GET", "/users/all").expect("exact should match");
    let param =
        manifest_handler_for_request(&web_root, "GET", "/users/42").expect("param should match");
    let wildcard = manifest_handler_for_request(&web_root, "GET", "/users/42/edit")
        .expect("wildcard should match");
    let fallback =
        manifest_handler_for_request(&web_root, "GET", "/other").expect("fallback should match");

    assert_eq!(exact.handler.function, "all");
    assert_eq!(param.handler.function, "show");
    assert_eq!(wildcard.handler.function, "wild");
    assert_eq!(fallback.handler.function, "fallback");
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies `OPTIONS` handlers are accepted by serve-package validation.
///
/// Inputs:
/// - One manifest handler with method `OPTIONS`.
///
/// Output:
/// - Test passes when the route validator accepts the method.
///
/// Transformation:
/// - Locks local server validation to the route methods generated by
///   `std.http.Router` manifest extraction.
#[test]
fn validate_handler_accepts_options_method() {
    let handler = WebPackageHandler {
        method: "OPTIONS".to_string(),
        route: "/api".to_string(),
        module: "app.Api".to_string(),
        function: "handle".to_string(),
        arity: 1,
        source: None,
    };

    validate_handler(&handler).expect("OPTIONS handler should validate");
}

/// Verifies WebSocket manifest rows accept the Battleship room protocol.
///
/// Inputs:
/// - One `/ws` route using `battleship.room.v1`.
///
/// Output:
/// - Test passes when the row validates.
///
/// Transformation:
/// - Locks the first local WebSocket runtime protocol exposed by `terlc serve`.
#[test]
fn validate_websocket_accepts_battleship_room_protocol() {
    let websocket = WebPackageWebSocket {
        route: "/ws".to_string(),
        protocol: "battleship.room.v1".to_string(),
        source: None,
    };

    validate_websocket(&websocket).expect("Battleship websocket should validate");
}

/// Verifies unsupported WebSocket protocols fail package validation.
///
/// Inputs:
/// - One `/ws` route with an unknown protocol name.
///
/// Output:
/// - Test passes when validation reports an unsupported protocol.
///
/// Transformation:
/// - Keeps runtime-owned WebSocket protocol support closed until a generic
///   source-level socket handler ABI exists.
#[test]
fn validate_websocket_rejects_unknown_protocol() {
    let websocket = WebPackageWebSocket {
        route: "/ws".to_string(),
        protocol: "chat.v1".to_string(),
        source: None,
    };

    let err = validate_websocket(&websocket).expect_err("unknown protocol should fail");

    assert!(err.contains("unsupported protocol"));
}

#[test]
fn validate_handler_rejects_non_final_wildcard() {
    let handler = WebPackageHandler {
        method: "GET".to_string(),
        route: "/assets/*/tail".to_string(),
        module: "app.Api".to_string(),
        function: "handle".to_string(),
        arity: 1,
        source: None,
    };

    let err = validate_handler(&handler).expect_err("non-final wildcard should fail");

    assert!(err.contains("wildcard"));
}

#[test]
fn validate_handler_rejects_empty_route_segment() {
    let handler = WebPackageHandler {
        method: "GET".to_string(),
        route: "/users//:id".to_string(),
        module: "app.Api".to_string(),
        function: "handle".to_string(),
        arity: 1,
        source: None,
    };

    let err = validate_handler(&handler).expect_err("empty segment should fail");

    assert!(err.contains("empty segment"));
}

#[test]
fn validate_handler_routes_rejects_same_shape_param_routes() {
    let handlers = vec![
        WebPackageHandler {
            method: "GET".to_string(),
            route: "/users/:id".to_string(),
            module: "app.Api".to_string(),
            function: "show_id".to_string(),
            arity: 1,
            source: None,
        },
        WebPackageHandler {
            method: "GET".to_string(),
            route: "/users/:name".to_string(),
            module: "app.Api".to_string(),
            function: "show_name".to_string(),
            arity: 1,
            source: None,
        },
    ];

    let err = validate_handler_routes(&handlers).expect_err("same-shape params should fail");

    assert!(err.contains("duplicate or ambiguous"));
}

/// Rejects ambiguous colon and typed brace parameter routes.
///
/// Inputs:
/// - Two `GET` handlers with `/users/:id` and `/users/{name:Int}`.
///
/// Output:
/// - Test passes when route validation reports duplicate or ambiguous shape.
///
/// Transformation:
/// - Normalizes both parameter syntaxes to the same ambiguity key so teams
///   cannot accidentally define two routes that match the same requests.
#[test]
fn validate_handler_routes_rejects_colon_and_typed_param_same_shape() {
    let handlers = vec![
        WebPackageHandler {
            method: "GET".to_string(),
            route: "/users/:id".to_string(),
            module: "app.Api".to_string(),
            function: "show_id".to_string(),
            arity: 1,
            source: None,
        },
        WebPackageHandler {
            method: "GET".to_string(),
            route: "/users/{name:Int}".to_string(),
            module: "app.Api".to_string(),
            function: "show_name".to_string(),
            arity: 1,
            source: None,
        },
    ];

    let err = validate_handler_routes(&handlers).expect_err("same-shape params should fail");

    assert!(err.contains("duplicate or ambiguous"));
}

/// Verifies `*` and `/*` are treated as ambiguous fallback routes.
///
/// Inputs:
/// - Two `GET` handlers whose fallback shapes both match every path.
///
/// Output:
/// - Test passes when manifest validation rejects the duplicate fallback
///   surface.
///
/// Transformation:
/// - Normalizes canonical fallback and slash wildcard fallback into one route
///   ambiguity key.
#[test]
fn validate_handler_routes_rejects_duplicate_fallback_shapes() {
    let handlers = vec![
        WebPackageHandler {
            method: "GET".to_string(),
            route: "*".to_string(),
            module: "app.Api".to_string(),
            function: "fallback".to_string(),
            arity: 1,
            source: None,
        },
        WebPackageHandler {
            method: "GET".to_string(),
            route: "/*".to_string(),
            module: "app.Api".to_string(),
            function: "legacy_fallback".to_string(),
            arity: 1,
            source: None,
        },
    ];

    let err = validate_handler_routes(&handlers).expect_err("fallback routes should collide");

    assert!(err.contains("duplicate or ambiguous"));
}

/// Verifies route handlers may accept route captures after `Request`.
///
/// Inputs:
/// - Handler manifest row for `/users/:id` with arity 2.
///
/// Output:
/// - Test passes when serve-package validation accepts the handler shape.
///
/// Transformation:
/// - Locks the server-side half of typed route params: `handler(request, id)`
///   is valid while the old `handler(request)` shape remains valid.
#[test]
fn validate_handler_accepts_request_plus_route_params() {
    let handler = WebPackageHandler {
        method: "GET".to_string(),
        route: "/users/:id".to_string(),
        module: "app.Api".to_string(),
        function: "show".to_string(),
        arity: 2,
        source: None,
    };

    validate_handler(&handler).expect("route-param handler arity should validate");
}

/// Verifies route handler arity must match declared route captures.
///
/// Inputs:
/// - Handler manifest row for `/users/:id` with arity 3.
///
/// Output:
/// - Test passes when serve-package validation rejects the extra argument.
///
/// Transformation:
/// - Prevents generated manifests from binding handler parameters that cannot
///   be supplied from the request route.
#[test]
fn validate_handler_rejects_route_param_arity_mismatch() {
    let handler = WebPackageHandler {
        method: "GET".to_string(),
        route: "/users/:id".to_string(),
        module: "app.Api".to_string(),
        function: "show".to_string(),
        arity: 3,
        source: None,
    };

    let error = validate_handler(&handler).expect_err("arity mismatch should fail");

    assert!(error.contains("Request plus route parameter"));
}

#[test]
fn beam_ebin_dir_for_web_root_uses_build_root_sibling() {
    let web_root = PathBuf::from("_build/web");

    let ebin = beam_ebin_dir_for_web_root(&web_root).expect("resolve ebin");

    assert_eq!(ebin, PathBuf::from("_build/ebin"));
}

#[test]
fn render_beam_handler_eval_passes_request_map_and_target() {
    let request = native_http::Request::from_parts("GET", "/api/users", "payload");
    let params = vec![("id".to_string(), "42".to_string())];
    let eval = render_beam_handler_eval(
        "app_api",
        "handle",
        &request,
        &params,
        &[],
        1,
        "q=terlan+lang",
        &[("accept".to_string(), "application/json".to_string())],
        "session=abc; theme=dark",
    );

    assert!(eval.contains(
        "Request = #{method => <<71,69,84>>, path => <<47,97,112,105,47,117,115,101,114,115>>, body => <<112,97,121,108,111,97,100>>, params => #{<<105,100>> => <<52,50>>}, query_string => <<113,61,116,101,114,108,97,110,43,108,97,110,103>>, query => #{<<113>> => <<116,101,114,108,97,110,32,108,97,110,103>>}, headers => #{<<97,99,99,101,112,116>> => <<97,112,112,108,105,99,97,116,105,111,110,47,106,115,111,110>>}, cookie_header => <<115,101,115,115,105,111,110,61,97,98,99,59,32,116,104,101,109,101,61,100,97,114,107>>, cookies => #{<<115,101,115,115,105,111,110>> => <<97,98,99>>, <<116,104,101,109,101>> => <<100,97,114,107>>}}"
    ));
    assert!(eval.contains("catch app_api:handle(Request)"));
    assert!(eval.contains("{terlan_response, Status, ContentType, Body}"));
}

/// Verifies route params can be passed as direct BEAM handler arguments.
///
/// Inputs:
/// - Handler arity 2 and one captured `id` route param.
///
/// Output:
/// - Test passes when the generated `erl -eval` text invokes `show/2`.
///
/// Transformation:
/// - Preserves the request map while appending route captures after `Request`
///   in route order.
#[test]
fn render_beam_handler_eval_passes_route_params_as_handler_args() {
    let request = native_http::Request::from_parts("GET", "/users/42", "");
    let params = vec![("id".to_string(), "42".to_string())];
    let eval = render_beam_handler_eval(
        "app_api",
        "show",
        &request,
        &params,
        &[("id".to_string(), "String".to_string())],
        2,
        "",
        &[],
        "",
    );

    assert!(eval.contains("catch app_api:show(Request, <<52,50>>)"));
}

/// Verifies typed integer route params are decoded before handler invocation.
///
/// Inputs:
/// - Handler arity 2, one captured `id` route param, and route type `Int`.
///
/// Output:
/// - Test passes when the generated `erl -eval` text decodes the segment with
///   `string:to_integer/1` before calling the handler.
///
/// Transformation:
/// - Locks the first typed route-param conversion supported by the local BEAM
///   handler bridge.
#[test]
fn render_beam_handler_eval_decodes_int_route_param_args() {
    let request = native_http::Request::from_parts("GET", "/users/42", "");
    let params = vec![("id".to_string(), "42".to_string())];
    let eval = render_beam_handler_eval(
        "app_api",
        "show",
        &request,
        &params,
        &[("id".to_string(), "Int".to_string())],
        2,
        "",
        &[],
        "",
    );

    assert!(eval.contains("catch app_api:show(Request, (case string:to_integer(binary_to_list(<<52,50>>)) of {Value, []} -> Value; _ -> erlang:error({invalid_route_param, <<105,100>>, <<\"Int\">>, <<52,50>>}) end))"));
}

/// Verifies typed boolean route params are decoded before handler invocation.
///
/// Inputs:
/// - Handler arity 2, one captured `active` route param, and route type `Bool`.
///
/// Output:
/// - Test passes when the generated `erl -eval` text converts `true` and
///   `false` URL segments to BEAM booleans before calling the handler.
///
/// Transformation:
/// - Extends typed route-param conversion beyond integer IDs without adding
///   custom per-application converter support.
#[test]
fn render_beam_handler_eval_decodes_bool_route_param_args() {
    let request = native_http::Request::from_parts("GET", "/users/true", "");
    let params = vec![("active".to_string(), "true".to_string())];
    let eval = render_beam_handler_eval(
        "app_api",
        "filter",
        &request,
        &params,
        &[("active".to_string(), "Bool".to_string())],
        2,
        "",
        &[],
        "",
    );

    assert!(eval.contains("catch app_api:filter(Request, (case <<116,114,117,101>> of <<\"true\">> -> true; <<\"false\">> -> false; _ -> erlang:error({invalid_route_param, <<97,99,116,105,118,101>>, <<\"Bool\">>, <<116,114,117,101>>}) end))"));
}

#[test]
fn parse_query_params_decodes_query_semantics() {
    let params = parse_query_params("q=terlan+lang&tag=web%20runtime");

    assert_eq!(
        params,
        vec![
            ("q".to_string(), "terlan lang".to_string()),
            ("tag".to_string(), "web runtime".to_string()),
        ]
    );
}

#[test]
fn parse_cookie_header_splits_request_cookie_pairs() {
    let cookies =
        native_http::parse_request_cookie_header("session=abc; theme = dark; empty; user=Ada");

    assert_eq!(
        cookies,
        vec![
            ("session".to_string(), "abc".to_string()),
            ("theme".to_string(), "dark".to_string()),
            ("user".to_string(), "Ada".to_string()),
        ]
    );
}

#[test]
fn render_beam_error_handler_eval_passes_http_error_record() {
    let eval = beam_eval::render_beam_error_handler_eval(
        "app_api",
        "render_error",
        "handler failed: <badarg>",
    );

    assert!(eval.contains("Error = {http_error, serve_handler_execution_failed,"));
    assert!(eval.contains(
        "<<104,97,110,100,108,101,114,32,102,97,105,108,101,100,58,32,60,98,97,100,97,114,103,62>>"
    ));
    assert!(eval.contains("Result = catch app_api:render_error(Error)"));
    assert!(eval.contains("{terlan_response, Status, ContentType, Headers, Body}"));
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
            headers: Vec::new(),
            body: br#"{"ok":true}"#.to_vec(),
        }
    );
}

#[test]
fn parse_beam_handler_stdout_accepts_response_headers() {
    let parsed = parse_beam_handler_stdout(
        b"200\ntext/plain; charset=utf-8\n#terlan-headers:2\nSet-Cookie\tsession=abc; HttpOnly\nx-terlan\tyes\nhello",
    )
    .expect("parse handler response with headers");

    assert_eq!(
        parsed,
        BeamHandlerResponse {
            status: 200,
            content_type: "text/plain; charset=utf-8".to_string(),
            headers: vec![
                (
                    "Set-Cookie".to_string(),
                    "session=abc; HttpOnly".to_string()
                ),
                ("x-terlan".to_string(), "yes".to_string()),
            ],
            body: b"hello".to_vec(),
        }
    );
}

#[test]
fn parse_beam_handler_stdout_rejects_header_injection() {
    let err = parse_beam_handler_stdout(
        b"200\ntext/plain; charset=utf-8\n#terlan-headers:1\nx-terlan\tbad\rvalue\nhello",
    )
    .expect_err("line break in header value should fail");

    assert!(err.contains("response header `x-terlan` contains a line break"));
}

#[test]
fn parse_beam_handler_stdout_rejects_server_owned_response_headers() {
    let err = parse_beam_handler_stdout(
        b"200\ntext/plain; charset=utf-8\n#terlan-headers:1\nContent-Length\t999\nhello",
    )
    .expect_err("server-owned header should fail");

    assert!(err.contains("response header `Content-Length` is owned by the server bridge"));
}

#[test]
fn beam_handler_response_converts_from_native_http_response() {
    let mut native = native_http::text("created", 200);
    native_http::status(&mut native, 201);

    let response =
        BeamHandlerResponse::from_native_response(&native).expect("convert native response");

    assert_eq!(
        response,
        BeamHandlerResponse {
            status: 201,
            content_type: "text/plain; charset=utf-8".to_string(),
            headers: Vec::new(),
            body: b"created".to_vec(),
        }
    );
}

#[test]
fn beam_handler_response_converts_native_headers() {
    let mut native = native_http::text("created", 200);
    native_http::header(&mut native, "Set-Cookie", "session=abc; Path=/");
    native_http::header(&mut native, "x-terlan", "yes");

    let response =
        BeamHandlerResponse::from_native_response(&native).expect("convert native response");

    assert_eq!(
        response.headers,
        vec![
            ("Set-Cookie".to_string(), "session=abc; Path=/".to_string()),
            ("x-terlan".to_string(), "yes".to_string()),
        ]
    );
}

#[test]
fn beam_handler_response_rejects_invalid_native_status() {
    let mut native = native_http::text("bad", 200);
    native_http::status(&mut native, 900);

    let err = BeamHandlerResponse::from_native_response(&native)
        .expect_err("invalid native status should fail");

    assert!(err.contains("native HTTP response status `900`"));
}

#[test]
fn beam_handler_response_rejects_invalid_native_header_name() {
    let mut native = native_http::text("bad", 200);
    native_http::header(&mut native, "bad header", "value");

    let err = BeamHandlerResponse::from_native_response(&native)
        .expect_err("invalid header name should fail");

    assert!(err.contains("response header name `bad header`"));
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
        source: None,
    };
    let matched = MatchedWebPackageHandler {
        handler,
        params: Vec::new(),
    };

    let err = execute_beam_handler(&web_root, &matched, "GET", "/api", "", &[], "", "")
        .expect_err("missing ebin should fail");

    assert!(err.contains("BEAM ebin directory"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn execute_beam_error_handler_reports_missing_ebin_before_running_erl() {
    let dir = temp_dir("missing_error_ebin");
    let web_root = dir.join("_build/web");
    fs::create_dir_all(&web_root).expect("create web root");
    let handler = WebPackageErrorHandler {
        module: "app.Api".to_string(),
        function: "render_error".to_string(),
        arity: 1,
    };

    let err = execute_beam_error_handler(&web_root, &handler, "handler failed")
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
        source: None,
    };
    let matched = MatchedWebPackageHandler {
        handler,
        params: Vec::new(),
    };

    let err = execute_beam_handler(&web_root, &matched, "GET", "/api", "", &[], "", "")
        .expect_err("missing beam should fail");

    assert!(err.contains("BEAM module `app.Api`"));
    assert!(err.contains("app_api.beam"));
    fs::remove_dir_all(dir).expect("cleanup");
}
