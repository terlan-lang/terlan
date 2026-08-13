use super::*;

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
pub(super) fn render_handler_log_line_includes_handler_metadata() {
    let identity = HandlerLogIdentity {
        method: "GET",
        route: "/users/:id",
        module: "app.Api",
        function: "show_user",
        arity: 1,
        source: None,
    };

    assert_eq!(
        render_handler_log_line(42, "web-abc", "GET", "/users/1", &identity, 200, 7),
        "terlc serve request_id=42 connection_id=42 build_id=web-abc method=GET path=/users/1 route_method=GET route=/users/:id handler=app.Api.show_user status=200 duration_ms=7"
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
pub(super) fn render_handler_log_line_includes_optional_source_metadata() {
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
        arity: 1,
        source: Some(&source),
    };

    assert_eq!(
        render_handler_log_line(42, "web-abc", "GET", "/users/1", &identity, 200, 7),
        "terlc serve request_id=42 connection_id=42 build_id=web-abc method=GET path=/users/1 route_method=GET route=/users/:id handler=app.Api.show_user status=200 duration_ms=7 source=src/app/Api.terl:12:5"
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
pub(super) fn render_static_log_line_includes_asset_metadata() {
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
        "terlc serve request_id=7 connection_id=7 build_id=web-abc method=GET path=/assets/app.js static=_build/web/assets/app.js status=200 duration_ms=3"
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
pub(super) fn render_static_route_log_line_includes_route_metadata() {
    let source = WebPackageSourceSpan {
        path: "src/app/Http.terl".to_string(),
        line: 11,
        column: 5,
    };

    assert_eq!(
        render_static_route_log_line(&RouteLogEvent {
            request_id: 8,
            build_id: "web-abc",
            request_method: "HEAD",
            request_path: "/about",
            route_method: "GET",
            route: "/about",
            response_path: None,
            source: Some(&source),
            status: 200,
            duration_ms: 4,
        }),
        "terlc serve request_id=8 connection_id=8 build_id=web-abc method=HEAD path=/about static_route_method=GET static_route=/about status=200 duration_ms=4 source=src/app/Http.terl:11:5"
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
pub(super) fn render_file_route_log_line_includes_route_and_file_metadata() {
    let source = WebPackageSourceSpan {
        path: "src/app/Downloads.terl".to_string(),
        line: 17,
        column: 9,
    };

    assert_eq!(
        render_file_route_log_line(&RouteLogEvent {
            request_id: 9,
            build_id: "web-abc",
            request_method: "GET",
            request_path: "/download",
            route_method: "GET",
            route: "/download",
            response_path: Some(Path::new("_build/web/downloads/report.txt")),
            source: Some(&source),
            status: 200,
            duration_ms: 5,
        }),
        "terlc serve request_id=9 connection_id=9 build_id=web-abc method=GET path=/download file_route_method=GET file_route=/download file=_build/web/downloads/report.txt status=200 duration_ms=5 source=src/app/Downloads.terl:17:9"
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
/// - Locks the local development error shape without requiring a failing VM
///   handler process.
#[test]
pub(super) fn render_dev_error_page_includes_escaped_handler_metadata() {
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
        arity: 1,
        source: Some(&source),
    };

    let page = render_dev_error_page(
        42,
        "web-abc",
        "GET",
        "/users/<1>",
        &identity,
        "VM failed: <badarg> & \"quoted\"",
    );

    assert!(page.contains("serve_handler.execution_failed"));
    assert!(page.contains("Message:</strong> Handler execution failed."));
    assert!(page.contains("GET /users/&lt;1&gt;"));
    assert!(page.contains("GET /users/:id"));
    assert!(page.contains("app.Api.show_user"));
    assert!(page.contains("Source:</strong> <code>src/app/&lt;Api&gt;.terl:12:5</code>"));
    assert!(page.contains("Request id:</strong> <code>42</code>"));
    assert!(page.contains("Build id:</strong> <code>web-abc</code>"));
    assert!(page.contains("VM failed: &lt;badarg&gt; &amp; &quot;quoted&quot;"));
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
///   without requiring a failing dynamic handler process.
#[test]
pub(super) fn render_dev_error_page_omits_absent_source_metadata() {
    let identity = HandlerLogIdentity {
        method: "POST",
        route: "/api/events",
        module: "app.Events",
        function: "create",
        arity: 1,
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

/// Verifies typed HTML fragments survive the complete native HTTP handler path.
///
/// Inputs:
/// - A temporary web package whose handler composes three trusted fragments.
///
/// Output:
/// - Test passes when direct AOT build and stream dispatch return the joined
///   HTML body, status, and content type.
///
/// Transformation:
/// - Compiles public `Template.Html` operations into managed string values,
///   loads the image, invokes the request handler, and bridges its response.
#[test]
pub(super) fn vm_stream_request_executes_managed_template_html_handler() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_managed_template_handler");
    let web_root = dir.join("_build/web");
    let source_dir = dir.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        dir.join("terlan.toml"),
        "[package]\nname = \"serve_template_demo\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
    )
    .expect("write project manifest");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        source_dir.join("View.terl"),
        "module app.View.\n\nimport std.collections.List.\nimport std.http.Response.\nimport std.template.Template.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub handle(_request: Request): Response ->\n    Response.html(Template.join(List(\n        Template.trusted(\"<main>\"),\n        Template.trusted(\"managed\"),\n        Template.trusted(\"</main>\")\n    ))).with_status(218).\n",
    )
    .expect("write template handler source");

    let status = crate::commands::build::run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.join("View.terl").display().to_string(),
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
    assert!(web_root.join("vm/app_View.tvm").exists());
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    { "method": "GET", "route": "/template", "module": "app.View", "function": "handle", "arity": 1,
      "source": { "path": "src/app/View.terl", "line": 9, "column": 5 } }
  ],
  "assets": [
    { "module": "app", "kind": "javascript-module", "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js", "fingerprint": 1 }
  ]
}
"#,
    )
    .expect("write template handler manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm template handler cache");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /template HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream template handler request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 218 "), "{response}");
    assert!(response.contains("content-type: text/html; charset=utf-8\r\n"));
    assert!(
        response.ends_with("\r\n\r\n<main>managed</main>"),
        "{response}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

/// Verifies external templates compile into native render plans and escaped HTML.
///
/// Inputs:
/// - A `.terl` handler with one checked `.terl.html` declaration.
///
/// Output:
/// - The native HTTP response contains compile-time markup and context-escaped
///   dynamic props without runtime template parsing.
///
/// Transformation:
/// - Carries the validated parsed tree through CoreIR, lowers its instantiation,
///   executes managed escaping, and bridges the resulting `Template.Html`.
#[test]
pub(super) fn vm_stream_request_executes_external_template_render_plan() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_external_template_handler");
    let web_root = dir.join("_build/web");
    let source_dir = dir.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(dir.join("templates")).expect("create template dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        dir.join("terlan.toml"),
        "[package]\nname = \"serve_external_template_demo\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
    )
    .expect("write project manifest");
    fs::write(
        dir.join("templates/page.terl.html"),
        "<!DOCTYPE html><main data-label=\"${label}\"><h1>${title}</h1></main>",
    )
    .expect("write external template");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        source_dir.join("View.terl"),
        "module app.View.\n\nimport std.http.Response.\nimport std.template.Template.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\ntemplate Page from \"../../templates/page.terl.html\" {\n    title: String,\n    label: String\n}.\n\npub page(): Template.Html ->\n    Page(title = \"<Admin & Ops>\", label = \"a\\\"b&c\").\n\npub handle(_request: Request): Response ->\n    Response.html(page()).with_status(219).\n",
    )
    .expect("write external template handler");

    let status = crate::commands::build::run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.join("View.terl").display().to_string(),
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
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    { "method": "GET", "route": "/external", "module": "app.View", "function": "handle", "arity": 1,
      "source": { "path": "src/app/View.terl", "line": 17, "column": 5 } }
  ],
  "assets": [
    { "module": "app", "kind": "javascript-module", "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js", "fingerprint": 1 }
  ]
}
"#,
    )
    .expect("write external template handler manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm external template handler");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /external HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream external template request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 219 "), "{response}");
    assert!(response.ends_with(
        "\r\n\r\n<!DOCTYPE html><main data-label=\"a&quot;b&amp;c\"><h1>&lt;Admin&#32;&amp;&#32;Ops&gt;</h1></main>"
    ), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

/// Verifies the complete checked template matrix executes through native HTTP.
///
/// Inputs:
/// - Scalar, optional, token-list, URL, nested record, expression, trusted
///   HTML, component prop, and component children values.
///
/// Output:
/// - One byte-exact HTML response with omission, escaping, and expansion.
///
/// Transformation:
/// - Compiles the shared typed-template semantics through CoreIR render plans,
///   NativeIR managed operations, component inlining, and HTTP serialization.
#[test]
pub(super) fn vm_stream_request_executes_complete_typed_template_matrix() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_typed_template_matrix");
    let web_root = dir.join("_build/web");
    let source_dir = dir.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(dir.join("templates")).expect("create template dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        dir.join("terlan.toml"),
        "[package]\nname = \"typed_template_matrix\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
    )
    .expect("write project manifest");
    fs::write(
        dir.join("templates/page.terl.html"),
        "<main class={classes} title={tooltip} data-count={count}><h1>{title}</h1><a href={href}>{user.name}</a><button disabled={disabled}>{count * 2}</button>{trusted}<typed-template-badge label={title}><span>child</span></typed-template-badge></main>",
    )
    .expect("write page template");
    fs::write(
        dir.join("templates/typed-template-badge.terl.html"),
        "<strong>{label}:{children}</strong>",
    )
    .expect("write component template");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        source_dir.join("View.terl"),
        r#"module app.View.

import std.core.Option.{Some}.
import std.http.Response.
import std.template.Template.
import type std.core.Option.Option.
import type std.http.Request.{Request}.
import type std.http.Response.{Response}.
import type std.http.Session.{Session}.

pub struct User {
    id: Int,
    name: String
}.

template Page from "../../templates/page.terl.html" {
    title: String,
    href: String,
    disabled: Bool,
    classes: List[String],
    tooltip: Option[String],
    count: Int,
    trusted: Template.Html,
    user: User
}.

template Badge from "../../templates/typed-template-badge.terl.html" {
    label: String
}.

pub page(): Template.Html ->
    Page(
        title = "<Terlan>",
        href = "/users/7?x=1&y=2",
        disabled = true,
        classes = ["hero", "wide"],
        tooltip = Some("profile"),
        count = 7,
        trusted = Template.trusted("<em>trusted</em>"),
        user = User(id = 7, name = "Ada & Bob")
    ).

pub handle(_request: Request): Response ->
    Response.html(page()).with_status(220).
"#,
    )
    .expect("write typed template handler");

    let status = crate::commands::build::run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.join("View.terl").display().to_string(),
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
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    { "method": "GET", "route": "/typed", "module": "app.View", "function": "handle", "arity": 1,
      "source": { "path": "src/app/View.terl", "line": 45, "column": 5 } }
  ],
  "assets": [
    { "module": "app", "kind": "javascript-module", "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js", "fingerprint": 1 }
  ]
}
"#,
    )
    .expect("write typed template manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm typed template handler");

    let response = handle_vm_stream_http1_request(
        &web_root,
        b"GET /typed HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("VM stream typed template request");
    let response = String::from_utf8(response).expect("response should be UTF-8");
    assert!(response.starts_with("HTTP/1.1 220 "), "{response}");
    assert!(response.ends_with(
        "\r\n\r\n<main class=\"hero wide\" title=\"profile\" data-count=\"7\"><h1>&lt;Terlan&gt;</h1><a href=\"/users/7?x=1&amp;y=2\">Ada&#32;&amp;&#32;Bob</a><button disabled>14</button><em>trusted</em><strong>&lt;Terlan&gt;:<span>child</span></strong></main>"
    ), "{response}");

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

/// Verifies typed JSON body decoding through the complete native HTTP path.
///
/// Inputs:
/// - One valid and one malformed JSON POST request sent to the same compiled
///   Terlan handler.
///
/// Output:
/// - Valid JSON selects `Ok(Json)` and returns 201; malformed JSON selects
///   `Err(Error)` and returns 400.
///
/// Transformation:
/// - Materializes the request body in the actor heap, parses it through the
///   maintained JSON adapter, branches over managed result variants, and
///   bridges the selected managed response back to HTTP/1.1 bytes.
#[test]
pub(super) fn vm_stream_request_decodes_managed_json_body_result() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_managed_json_body");
    let web_root = dir.join("_build/web");
    let source_dir = dir.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        dir.join("terlan.toml"),
        "[package]\nname = \"serve_json_body_demo\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
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
        "module app.Api.\n\nimport std.core.Result.{Err, Ok}.\nimport std.http.Response.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub handle(request: Request): Response ->\n    case request.body_json() {\n        Ok(_json) -> Response.text(\"valid\").with_status(201);\n        Err(_error) -> Response.text(\"invalid\").with_status(400)\n    }.\n",
    )
    .expect("write JSON handler source");

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
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    { "method": "POST", "route": "/json", "module": "app.Api", "function": "handle", "arity": 1,
      "source": { "path": "src/app/Api.terl", "line": 8, "column": 5 } }
  ],
  "assets": [
    { "module": "app", "kind": "javascript-module", "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js", "fingerprint": 1 }
  ]
}
"#,
    )
    .expect("write JSON handler manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm JSON handler");

    let valid = handle_vm_stream_http1_request(
        &web_root,
        b"POST /json HTTP/1.1\r\nHost: localhost\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
    )
    .expect("valid JSON request");
    let valid = String::from_utf8(valid).expect("valid response UTF-8");
    assert!(valid.starts_with("HTTP/1.1 201 "), "{valid}");
    assert!(valid.ends_with("\r\n\r\nvalid"), "{valid}");

    let invalid = handle_vm_stream_http1_request(
        &web_root,
        b"POST /json HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1\r\n\r\n{",
    )
    .expect("invalid JSON request");
    let invalid = String::from_utf8(invalid).expect("invalid response UTF-8");
    assert!(invalid.starts_with("HTTP/1.1 400 "), "{invalid}");
    assert!(invalid.ends_with("\r\n\r\ninvalid"), "{invalid}");

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}

/// Verifies VM-owned sessions survive request shards and enforce lifecycle changes.
///
/// Inputs:
/// - Five requests covering creation, state reuse, identity rotation, expiration,
///   and stale-cookie recovery against one admitted native handler image.
///
/// Output:
/// - Actor-owned state survives normal and rotated requests, expiration emits a
///   deletion cookie, and the stale identity cannot recover deleted state.
///
/// Transformation:
/// - Exercises `Session.current`, `get`, `set`, `rotate`, `expire`, and
///   `with_response` through generated native code and VM HTTP/1 response bytes.
#[test]
pub(super) fn vm_stream_session_state_and_lifecycle_are_vm_owned() {
    clear_vm_handler_module_cache_for_test();
    let dir = temp_dir("vm_stream_managed_session");
    let web_root = dir.join("_build/web");
    let source_dir = dir.join("src/app");
    fs::create_dir_all(&source_dir).expect("create source dir");
    fs::create_dir_all(web_root.join("assets/js/modules")).expect("create package dirs");
    fs::write(
        dir.join("terlan.toml"),
        "[package]\nname = \"serve_session_demo\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
    )
    .expect("write project manifest");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .expect("write js asset");
    fs::write(
        source_dir.join("SessionApi.terl"),
        r#"module app.SessionApi.

import std.core.Option.{None, Some}.
import std.http.Response.
import std.http.Session.{current, expire, get, rotate, set, with_response}.
import type std.http.Request.{Request}.
import type std.http.Response.{Response}.

pub handle(request: Request): Response ->
    let session = current(request);
    case get(session, "marker") {
        None -> initialize(session);
        Some(value) -> with_response(Response.text(value), session)
    }.

initialize(session: Session): Response ->
    let _written = set(session, "marker", "stored");
    with_response(Response.text("created"), session).

pub rotate_session(request: Request): Response ->
    let session = current(request);
    let rotated = rotate(session);
    with_response(Response.text("rotated"), rotated).

pub expire_session(request: Request): Response ->
    let session = current(request);
    let _expired = expire(session);
    with_response(Response.text("expired"), session).
"#,
    )
    .expect("write session handler source");

    let status = crate::commands::build::run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.join("SessionApi.terl").display().to_string(),
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
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [
    { "method": "GET", "route": "/session", "module": "app.SessionApi", "function": "handle", "arity": 1,
      "source": { "path": "src/app/SessionApi.terl", "line": 9, "column": 5 } },
    { "method": "GET", "route": "/session/rotate", "module": "app.SessionApi", "function": "rotate_session", "arity": 1,
      "source": { "path": "src/app/SessionApi.terl", "line": 21, "column": 5 } },
    { "method": "GET", "route": "/session/expire", "module": "app.SessionApi", "function": "expire_session", "arity": 1,
      "source": { "path": "src/app/SessionApi.terl", "line": 26, "column": 5 } }
  ],
  "assets": [
    { "module": "app", "kind": "javascript-module", "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js", "fingerprint": 1 }
  ]
}
"#,
    )
    .expect("write session handler manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm session handler");

    let created = handle_vm_stream_http1_request(
        &web_root,
        b"GET /session HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("create session request");
    let created = String::from_utf8(created).expect("created response UTF-8");
    assert!(created.ends_with("\r\n\r\ncreated"), "{created}");
    assert!(
        created.contains("set-cookie: terlan_session=s1; Path=/; HttpOnly; SameSite=Lax\r\n"),
        "{created}"
    );

    // Code generations are disposable. VM-owned session actors must survive
    // watcher invalidation and reload of the handler image.
    clear_vm_handler_module_cache_for_test();

    let stored = handle_vm_stream_http1_request(
        &web_root,
        b"GET /session HTTP/1.1\r\nHost: localhost\r\nCookie: terlan_session=s1\r\n\r\n",
    )
    .expect("reuse session request");
    let stored = String::from_utf8(stored).expect("stored response UTF-8");
    assert!(stored.ends_with("\r\n\r\nstored"), "{stored}");
    assert!(!stored.contains("set-cookie:"), "{stored}");

    let rotated = handle_vm_stream_http1_request(
        &web_root,
        b"GET /session/rotate HTTP/1.1\r\nHost: localhost\r\nCookie: terlan_session=s1\r\n\r\n",
    )
    .expect("rotate session request");
    let rotated = String::from_utf8(rotated).expect("rotated response UTF-8");
    assert!(
        rotated.contains("set-cookie: terlan_session=s2; Path=/; HttpOnly; SameSite=Lax\r\n"),
        "{rotated}"
    );

    let rotated_state = handle_vm_stream_http1_request(
        &web_root,
        b"GET /session HTTP/1.1\r\nHost: localhost\r\nCookie: terlan_session=s2\r\n\r\n",
    )
    .expect("read rotated session request");
    let rotated_state = String::from_utf8(rotated_state).expect("rotated state UTF-8");
    assert!(rotated_state.ends_with("\r\n\r\nstored"), "{rotated_state}");

    let expired = handle_vm_stream_http1_request(
        &web_root,
        b"GET /session/expire HTTP/1.1\r\nHost: localhost\r\nCookie: terlan_session=s2\r\n\r\n",
    )
    .expect("expire session request");
    let expired = String::from_utf8(expired).expect("expired response UTF-8");
    assert!(
        expired.contains("set-cookie: terlan_session=;"),
        "{expired}"
    );
    assert!(expired.contains("Max-Age=0"), "{expired}");

    let replacement = handle_vm_stream_http1_request(
        &web_root,
        b"GET /session HTTP/1.1\r\nHost: localhost\r\nCookie: terlan_session=s2\r\n\r\n",
    )
    .expect("stale session replacement request");
    let replacement = String::from_utf8(replacement).expect("replacement response UTF-8");
    assert!(replacement.ends_with("\r\n\r\ncreated"), "{replacement}");
    assert!(
        replacement.contains("set-cookie: terlan_session=s3; Path=/; HttpOnly; SameSite=Lax\r\n"),
        "{replacement}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
    clear_vm_handler_module_cache_for_test();
}
#[cfg(test)]
#[path = "package_validation_test.rs"]
#[cfg(test)]
mod package_validation_test;
