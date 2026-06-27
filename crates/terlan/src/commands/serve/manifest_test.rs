use super::*;
use crate::support::test_fs;
use std::fs;
use std::path::{Path, PathBuf};

/// Creates a unique temporary manifest test directory.
///
/// Inputs:
/// - `name`: readable test stem.
///
/// Output:
/// - Path to a not-yet-existing directory under the system temp directory.
///
/// Transformation:
/// - Delegates to the shared test filesystem helper with the serve-manifest
///   namespace.
fn temp_dir(name: &str) -> PathBuf {
    test_fs::temp_path("serve_manifest", name)
}

/// Writes a browser manifest fixture with one index and one asset.
///
/// Inputs:
/// - `web_root`: target package root.
///
/// Output:
/// - Filesystem fixture containing manifest-declared index and asset files.
///
/// Transformation:
/// - Creates the minimal manifest shape needed by manifest static routing tests
///   without invoking the full browser build pipeline.
fn write_manifest_package(web_root: &Path) {
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create dirs");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(web_root.join("unlisted.txt"), "not routed\n").expect("write unlisted file");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write app asset");
    fs::write(web_root.join("assets/app.css"), "body { color: black; }\n")
        .expect("write css asset");
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
      "kind": "css",
      "source_relative_path": "assets/app.css",
      "web_relative_path": "assets/app.css",
      "fingerprint": 2
    },
    {
      "module": "",
      "kind": "static-asset",
      "source_relative_path": "assets/hello.txt",
      "web_relative_path": "assets/hello.txt",
      "fingerprint": 3
    }
  ]
}
"#,
    )
    .expect("write manifest");
}

/// Writes a project manifest next to a web package fixture.
///
/// Inputs:
/// - `path`: target `terlan.toml` path.
/// - `tls`: raw `[server.tls]` body to append after the package section.
///
/// Output:
/// - Project manifest fixture consumed by serve package validation.
///
/// Transformation:
/// - Keeps package metadata minimal while allowing tests to vary only TLS
///   metadata validated through the build manifest parser.
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

/// Writes the web-profile Postgres Compose fixture beside `terlan.toml`.
///
/// Inputs:
/// - `project_root`: target project directory.
///
/// Output:
/// - `docker-compose.yml` with a valid `services.postgres` development
///   database service.
///
/// Transformation:
/// - Mirrors the init scaffold's Compose shape without depending on init
///   internals from serve manifest tests.
fn write_postgres_compose(project_root: &Path) {
    fs::write(
        project_root.join("docker-compose.yml"),
        r#"services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: terlan
      POSTGRES_PASSWORD: terlan
      POSTGRES_DB: terlan_dev
    ports:
      - "127.0.0.1:5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U terlan -d terlan_dev"]
      interval: 1s
      timeout: 5s
      retries: 30
"#,
    )
    .expect("write docker compose");
}

/// Writes a browser manifest fixture with static response rows.
///
/// Inputs:
/// - `web_root`: target package root.
/// - `responses`: raw JSON response rows to insert.
///
/// Output:
/// - Filesystem fixture containing manifest-declared static responses.
///
/// Transformation:
/// - Reuses the normal package fixture and replaces only manifest metadata so
///   tests can validate static-response rows without invoking a browser build.
fn write_manifest_package_with_static_responses(web_root: &Path, responses: &str) {
    write_manifest_package(web_root);
    fs::write(
        web_root.join("manifest.json"),
        format!(
            r#"{{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "static_responses": [{responses}],
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
    .expect("write static response manifest");
}

/// Writes a browser manifest fixture with file response rows.
///
/// Inputs:
/// - `web_root`: target package root.
/// - `responses`: raw JSON file-response rows to insert.
///
/// Output:
/// - Filesystem fixture containing one routable package file.
///
/// Transformation:
/// - Reuses the normal package fixture and replaces only manifest metadata so
///   tests can validate file-response rows without invoking a browser build.
fn write_manifest_package_with_file_responses(web_root: &Path, responses: &str) {
    write_manifest_package(web_root);
    fs::create_dir_all(web_root.join("downloads")).expect("create downloads");
    fs::write(web_root.join("downloads/report.txt"), "report\n").expect("write report");
    fs::write(
        web_root.join("manifest.json"),
        format!(
            r#"{{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [],
  "file_responses": [{responses}],
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
    .expect("write file response manifest");
}

/// Writes a browser manifest fixture with both dynamic and static routes.
///
/// Inputs:
/// - `web_root`: target package root.
/// - `handler_route`: dynamic handler route pattern.
/// - `static_route`: static response route pattern.
///
/// Output:
/// - Filesystem fixture containing one handler and one static response.
///
/// Transformation:
/// - Creates a compact manifest for validating the shared route namespace
///   between dynamic handlers and manifest-cached static responses.
fn write_manifest_package_with_handler_and_static_response(
    web_root: &Path,
    handler_route: &str,
    static_route: &str,
) {
    write_manifest_package(web_root);
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
      "route": "{handler_route}",
      "module": "app.Api",
      "function": "handle",
      "arity": 1
    }}
  ],
  "static_responses": [
    {{
      "method": "GET",
      "route": "{static_route}",
      "status": 200,
      "content_type": "text/plain; charset=utf-8",
      "body": "static"
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
    .expect("write mixed route manifest");
}

/// Writes a browser manifest fixture with dynamic, static, and file routes.
///
/// Inputs:
/// - `web_root`: target package root.
/// - `handler_route`: dynamic handler route pattern.
/// - `static_route`: static response route pattern.
/// - `file_route`: file response route pattern.
///
/// Output:
/// - Filesystem fixture containing one route row for each route kind.
///
/// Transformation:
/// - Creates a compact manifest for validating the shared route namespace
///   across all route-backed response sections.
fn write_manifest_package_with_all_route_kinds(
    web_root: &Path,
    handler_route: &str,
    static_route: &str,
    file_route: &str,
) {
    write_manifest_package(web_root);
    fs::create_dir_all(web_root.join("downloads")).expect("create downloads");
    fs::write(web_root.join("downloads/report.txt"), "report\n").expect("write report");
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
      "route": "{handler_route}",
      "module": "app.Api",
      "function": "handle",
      "arity": 1
    }}
  ],
  "static_responses": [
    {{
      "method": "GET",
      "route": "{static_route}",
      "status": 200,
      "content_type": "text/plain; charset=utf-8",
      "body": "static"
    }}
  ],
  "file_responses": [
    {{
      "method": "GET",
      "route": "{file_route}",
      "path": "downloads/report.txt",
      "status": 200
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
    .expect("write mixed route manifest");
}

#[test]
fn manifest_static_file_for_request_matches_index_and_assets() {
    let dir = temp_dir("matches_index_assets");
    let web_root = dir.join("web");
    write_manifest_package(&web_root);

    let root_index = manifest_static_file_for_request(&web_root, "/").expect("root index");
    let explicit_index =
        manifest_static_file_for_request(&web_root, "/index.html").expect("explicit index");
    let asset = manifest_static_file_for_request(&web_root, "/assets/js/modules/app.js")
        .expect("manifest asset");
    let static_asset =
        manifest_static_file_for_request(&web_root, "/assets/hello.txt").expect("static asset");

    assert_eq!(root_index, web_root.join("index.html"));
    assert_eq!(explicit_index, web_root.join("index.html"));
    assert_eq!(asset, web_root.join("assets/js/modules/app.js"));
    assert_eq!(static_asset, web_root.join("assets/hello.txt"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_static_file_for_request_ignores_unlisted_files() {
    let dir = temp_dir("ignores_unlisted");
    let web_root = dir.join("web");
    write_manifest_package(&web_root);

    let unlisted = manifest_static_file_for_request(&web_root, "/unlisted.txt");

    assert!(unlisted.is_none());
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn manifest_build_id_defaults_for_older_manifest() {
    let dir = temp_dir("build_id_default");
    let web_root = dir.join("web");
    write_manifest_package(&web_root);

    assert_eq!(manifest_build_id(&web_root), "unknown");
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_accepts_without_project_manifest() {
    let dir = temp_dir("no_project_manifest");
    let web_root = dir.join("_build/web");
    write_manifest_package(&web_root);

    validate_web_package(&web_root).expect("standalone web package");
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_accepts_adjacent_project_manifest_tls() {
    let dir = temp_dir("valid_project_tls");
    let web_root = dir.join("_build/web");
    write_manifest_package(&web_root);
    fs::create_dir_all(dir.join("certs")).expect("create cert dir");
    fs::write(dir.join("certs/dev.pem"), "dev cert").expect("write cert fixture");
    fs::write(dir.join("certs/dev-key.pem"), "dev key").expect("write key fixture");
    fs::write(dir.join("certs/ca.pem"), "dev ca").expect("write ca fixture");
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "manual"
cert = "certs/dev.pem"
key = "certs/dev-key.pem"
ca = "certs/ca.pem""#,
    );

    validate_web_package(&web_root).expect("valid project TLS manifest");
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies manual TLS check mode rejects missing certificate files.
///
/// Inputs:
/// - Browser package fixture under `_build/web`.
/// - Adjacent `terlan.toml` using manual TLS paths that do not exist.
///
/// Output:
/// - Test passes when package validation reports the missing project-local file.
///
/// Transformation:
/// - Locks serve-time filesystem validation separately from build-time TLS
///   shape parsing so future rustls serving can assume manual files exist.
#[test]
fn validate_web_package_rejects_missing_manual_tls_files() {
    let dir = temp_dir("missing_project_tls_files");
    let web_root = dir.join("_build/web");
    write_manifest_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "manual"
cert = "certs/dev.pem"
key = "certs/dev-key.pem""#,
    );

    let err = validate_web_package(&web_root).expect_err("missing TLS files");

    assert!(
        err.contains("[server.tls] manual cert file"),
        "unexpected error: {err}"
    );
    assert!(err.contains("certs/dev.pem"), "unexpected error: {err}");
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies manual TLS check mode rejects missing custom CA files.
///
/// Inputs:
/// - Browser package fixture under `_build/web`.
/// - Adjacent `terlan.toml` using valid manual cert/key paths and a missing
///   custom CA path.
///
/// Output:
/// - Test passes when package validation reports the missing CA file.
///
/// Transformation:
/// - Applies the same project-local file existence rule to optional custom CA
///   bundles as to required manual certificate and key files.
#[test]
fn validate_web_package_rejects_missing_manual_tls_ca_file() {
    let dir = temp_dir("missing_project_tls_ca_file");
    let web_root = dir.join("_build/web");
    write_manifest_package(&web_root);
    fs::create_dir_all(dir.join("certs")).expect("create cert dir");
    fs::write(dir.join("certs/dev.pem"), "dev cert").expect("write cert fixture");
    fs::write(dir.join("certs/dev-key.pem"), "dev key").expect("write key fixture");
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "manual"
cert = "certs/dev.pem"
key = "certs/dev-key.pem"
ca = "certs/ca.pem""#,
    );

    let err = validate_web_package(&web_root).expect_err("missing TLS CA file");

    assert!(
        err.contains("[server.tls] manual ca file"),
        "unexpected error: {err}"
    );
    assert!(err.contains("certs/ca.pem"), "unexpected error: {err}");
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies manual TLS check mode rejects paths outside the project root.
///
/// Inputs:
/// - Browser package fixture under `_build/web`.
/// - Adjacent `terlan.toml` using a parent-directory TLS path.
///
/// Output:
/// - Test passes when package validation reports the path containment rule.
///
/// Transformation:
/// - Prevents local server validation from accepting certificate paths that
///   escape the project before future runtime loading opens files.
#[test]
fn validate_web_package_rejects_manual_tls_paths_outside_project() {
    let dir = temp_dir("escaping_project_tls_files");
    let web_root = dir.join("_build/web");
    write_manifest_package(&web_root);
    fs::create_dir_all(dir.join("certs")).expect("create cert dir");
    fs::write(dir.join("certs/dev-key.pem"), "dev key").expect("write key fixture");
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "manual"
cert = "../dev.pem"
key = "certs/dev-key.pem""#,
    );

    let err = validate_web_package(&web_root).expect_err("escaping TLS path");

    assert!(
        err.contains("must be project-relative and stay inside the project"),
        "unexpected error: {err}"
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies adjacent Docker Compose Postgres metadata is accepted.
///
/// Inputs:
/// - Browser package fixture under `_build/web`.
/// - Adjacent `terlan.toml`.
/// - Adjacent Docker Compose file with `services.postgres`.
///
/// Output:
/// - Test passes when package validation accepts both project metadata files.
///
/// Transformation:
/// - Exercises the Docker-aware dependency validation path used before future
///   `terlc serve` dependency startup.
#[test]
fn validate_web_package_accepts_adjacent_postgres_compose() {
    let dir = temp_dir("valid_project_compose");
    let web_root = dir.join("_build/web");
    write_manifest_package(&web_root);
    write_project_manifest(&dir.join("terlan.toml"), r#"mode = "internal""#);
    write_postgres_compose(&dir);

    validate_web_package(&web_root).expect("valid project compose");
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies malformed adjacent Docker Compose metadata is rejected.
///
/// Inputs:
/// - Browser package fixture under `_build/web`.
/// - Adjacent `terlan.toml`.
/// - Adjacent Docker Compose file with an invalid Postgres image.
///
/// Output:
/// - Test passes when package validation returns a serve-package diagnostic.
///
/// Transformation:
/// - Prevents dev-server dependency management from accepting an unusable
///   service definition.
#[test]
fn validate_web_package_rejects_invalid_adjacent_postgres_compose() {
    let dir = temp_dir("invalid_project_compose");
    let web_root = dir.join("_build/web");
    write_manifest_package(&web_root);
    write_project_manifest(&dir.join("terlan.toml"), r#"mode = "internal""#);
    fs::write(
        dir.join("docker-compose.yml"),
        r#"services:
  postgres:
    image: redis:7
    environment:
      POSTGRES_USER: terlan
      POSTGRES_PASSWORD: terlan
      POSTGRES_DB: terlan_dev
"#,
    )
    .expect("write invalid docker compose");

    let err = validate_web_package(&web_root).expect_err("invalid compose");

    assert!(
        err.contains("service `postgres` must use a postgres image"),
        "{err}"
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_rejects_invalid_adjacent_project_manifest_tls() {
    let dir = temp_dir("invalid_project_tls");
    let web_root = dir.join("_build/web");
    write_manifest_package(&web_root);
    write_project_manifest(
        &dir.join("terlan.toml"),
        r#"mode = "manual"
cert = "certs/dev.pem""#,
    );

    let err = validate_web_package(&web_root).expect_err("invalid project TLS manifest");

    assert!(err.contains("error[serve_package]: invalid project manifest"));
    assert!(err.contains("project manifest [server.tls] mode manual requires cert and key"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_accepts_static_responses() {
    let dir = temp_dir("static_response_valid");
    let web_root = dir.join("web");
    write_manifest_package_with_static_responses(
        &web_root,
        r#"{
      "method": "GET",
      "route": "/about",
      "status": 200,
      "content_type": "text/html; charset=utf-8",
      "body": "<main>About</main>"
    }"#,
    );

    validate_web_package(&web_root).expect("valid static response manifest");
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_accepts_static_response_headers() {
    let dir = temp_dir("static_response_headers_valid");
    let web_root = dir.join("web");
    write_manifest_package_with_static_responses(
        &web_root,
        r#"{
      "method": "GET",
      "route": "/old",
      "status": 301,
      "content_type": "text/plain; charset=utf-8",
      "headers": [
        { "name": "Location", "value": "/new" }
      ],
      "body": ""
    }"#,
    );

    validate_web_package(&web_root).expect("valid static response headers");
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_rejects_static_response_server_owned_header() {
    let dir = temp_dir("static_response_headers_invalid");
    let web_root = dir.join("web");
    write_manifest_package_with_static_responses(
        &web_root,
        r#"{
      "method": "GET",
      "route": "/bad",
      "status": 200,
      "content_type": "text/plain; charset=utf-8",
      "headers": [
        { "name": "Content-Length", "value": "99" }
      ],
      "body": "bad"
    }"#,
    );

    let err = validate_web_package(&web_root).expect_err("server-owned static header");

    assert!(err.contains("static response `GET` `/bad` has invalid header"));
    assert!(err.contains("response header `Content-Length` is owned by the server bridge"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_rejects_ambiguous_static_responses() {
    let dir = temp_dir("static_response_ambiguous");
    let web_root = dir.join("web");
    write_manifest_package_with_static_responses(
        &web_root,
        r#"{
      "method": "GET",
      "route": "/users/:id",
      "status": 200,
      "content_type": "text/plain; charset=utf-8",
      "body": "one"
    },
    {
      "method": "GET",
      "route": "/users/:name",
      "status": 200,
      "content_type": "text/plain; charset=utf-8",
      "body": "two"
    }"#,
    );

    let err = validate_web_package(&web_root).expect_err("ambiguous static response");

    assert!(err.contains("duplicate or ambiguous static response route"));
    assert!(err.contains("/users/:name"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_rejects_static_response_conflicting_with_handler() {
    let dir = temp_dir("static_response_conflicts_handler");
    let web_root = dir.join("web");
    write_manifest_package_with_handler_and_static_response(
        &web_root,
        "/users/:id",
        "/users/:name",
    );

    let err = validate_web_package(&web_root).expect_err("static route should conflict");

    assert!(err.contains("static response route `GET` `/users/:name` conflicts"));
    assert!(err.contains("handler route `GET` `/users/:id`"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_accepts_file_responses() {
    let dir = temp_dir("file_response_valid");
    let web_root = dir.join("web");
    write_manifest_package_with_file_responses(
        &web_root,
        r#"{
      "method": "GET",
      "route": "/download",
      "path": "downloads/report.txt",
      "status": 200,
      "content_type": "text/plain; charset=utf-8"
    }"#,
    );

    validate_web_package(&web_root).expect("valid file response manifest");
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_rejects_missing_file_response_path() {
    let dir = temp_dir("file_response_missing");
    let web_root = dir.join("web");
    write_manifest_package_with_file_responses(
        &web_root,
        r#"{
      "method": "GET",
      "route": "/download",
      "path": "downloads/missing.txt",
      "status": 200
    }"#,
    );

    let err = validate_web_package(&web_root).expect_err("missing file response path");

    assert!(err.contains("file response path"));
    assert!(err.contains("does not exist"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_web_package_rejects_file_response_conflicting_with_static_response() {
    let dir = temp_dir("file_response_conflicts_static");
    let web_root = dir.join("web");
    write_manifest_package_with_all_route_kinds(
        &web_root,
        "/api",
        "/downloads/:id",
        "/downloads/:name",
    );

    let err = validate_web_package(&web_root).expect_err("file route should conflict");

    assert!(err.contains("file response route `GET` `/downloads/:name` conflicts"));
    assert!(err.contains("static response route `GET` `/downloads/:id`"));
    fs::remove_dir_all(dir).expect("cleanup");
}
