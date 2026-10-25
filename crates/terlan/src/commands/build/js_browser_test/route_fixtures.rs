use super::*;

use super::super::manifest::WebHandlerArtifact;
use super::super::routes::{
    discover_web_handlers_from_modules, discover_web_route_manifest_from_sources,
};
use crate::commands::emit_js::target_contract::js_target_contract;
use crate::support::test_fs;
use crate::validation::target_profile::TargetProfile;

/// Creates a unique temporary source file path for browser-manifest tests.
///
/// Inputs:
/// - `name`: readable test stem.
///
/// Output:
/// - Path under the system temp directory.
///
/// Transformation:
/// - Delegates to the shared test filesystem helper with the JS-browser
///   namespace and `.terl` suffix.
pub(super) fn temp_source_path(name: &str) -> PathBuf {
    temp_dir(name).join("app/Http.terl")
}

/// Creates a unique temporary directory for browser package tests.
///
/// Inputs:
/// - `name`: readable test stem.
///
/// Output:
/// - Directory path under the system temp directory.
///
/// Transformation:
/// - Delegates to the shared test filesystem helper with the JS-browser
///   namespace.
pub(super) fn temp_dir(name: &str) -> PathBuf {
    let root = test_fs::temp_dir("js_browser", name);
    fs::create_dir_all(root.join("app")).expect("create module directory");
    root
}

/// Builds a module artifact pointing at one source fixture.
///
/// Inputs:
/// - `module`: Terlan module name.
/// - `source_path`: source file path.
///
/// Output:
/// - Minimal `JsModuleArtifact` accepted by handler discovery.
///
/// Transformation:
/// - Fills non-discovery manifest fields with deterministic placeholders.
pub(super) fn module_artifact(module: &str, source_path: &Path) -> JsModuleArtifact {
    JsModuleArtifact {
        module: module.to_string(),
        source_path: source_path.display().to_string(),
        artifact_path: "target/app.js".to_string(),
        relative_path: "modules/app.js".to_string(),
        core_ir_hash: 1,
        target_profile: "js.browser".to_string(),
        validation_status: "ok".to_string(),
        runtime_smoke_status: "not-run".to_string(),
        declaration_path: None,
        declaration_relative_path: None,
        asset_imports: Vec::new(),
    }
}

/// Returns a discovered handler matching the source-visible route fields.
///
/// Inputs:
/// - `handlers`: route rows produced by browser manifest discovery.
/// - `method`: HTTP method expected on the row.
/// - `route`: route pattern expected on the row.
/// - `function`: Terlan handler function expected on the row.
///
/// Output:
/// - Matching handler row.
///
/// Transformation:
/// - Compares only semantic routing fields so tests can separately assert
///   source metadata without hard-coding temporary line numbers.
pub(super) fn matching_handler<'a>(
    handlers: &'a [WebHandlerArtifact],
    method: &str,
    route: &str,
    function: &str,
) -> &'a WebHandlerArtifact {
    handlers
        .iter()
        .find(|handler| {
            handler.method == method
                && handler.route == route
                && handler.module == "app.Http"
                && handler.function == function
                && handler.arity == 1
        })
        .expect("matching handler")
}

/// Asserts a handler row carries runtime-safe source metadata.
///
/// Inputs:
/// - `handler`: handler row to inspect.
/// - `source_path`: original source path used by the test fixture.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Checks the generated source path avoids absolute directories and that the
///   location uses one-based line and column coordinates.
pub(super) fn assert_handler_source(handler: &WebHandlerArtifact, source_path: &Path) {
    let source = handler.source.as_ref().expect("handler source");
    assert_eq!(
        source.path,
        source_path
            .file_name()
            .expect("source file name")
            .to_string_lossy()
    );
    assert!(source.line >= 1);
    assert!(source.column >= 1);
    assert!(!Path::new(&source.path).is_absolute());
}

/// Asserts a manifest JSON row carries runtime-safe source metadata.
///
/// Inputs:
/// - `row`: serialized handler/static/file response manifest row.
/// - `source_path`: original source path used by the test fixture.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Checks serialized source metadata without depending on exact parser span
///   offsets, which may become more precise as syntax output evolves.
pub(super) fn assert_json_source(row: &serde_json::Value, source_path: &Path) {
    assert_eq!(
        row["source"]["path"],
        source_path
            .file_name()
            .expect("source file name")
            .to_string_lossy()
            .as_ref()
    );
    assert!(row["source"]["line"].as_u64().expect("source line") >= 1);
    assert!(row["source"]["column"].as_u64().expect("source column") >= 1);
}

/// Writes a source fixture for route discovery tests.
///
/// Inputs:
/// - `path`: destination file path.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Writes a Terlan module with exact, parameter, and fallback router calls.
pub(super) fn write_router_source(path: &Path) {
    fs::write(
        path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    let router = Router.get(Router.new(), \"/\", home);\n    let with_users = Router.get(router, \"/users/:id\", show_user);\n    let with_probe = Router.options(with_users, \"/probe\", home);\n    Router.fallback(with_probe, not_found).\n\npub home(_request: Request): Response ->\n    Response.text(\"home\").\n\npub show_user(_request: Request): Response ->\n    Response.text(\"user\").\n\npub not_found(_request: Request): Response ->\n    Response.text(\"not found\").\n",
    )
    .expect("write router source");
}

/// Writes a source fixture whose routes return constant file responses.
///
/// Inputs:
/// - `path`: file path to write.
///
/// Output:
/// - Terlan module source with one router and constant `Response.file` handler
///   bodies.
///
/// Transformation:
/// - Verifies file-producing routes retain source handler identity, including
///   positional and named arguments owned by the response implementation.
pub(super) fn write_file_router_source(path: &Path) {
    fs::write(
        path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.new().get(\"/download\", download).get(\"/manual\", manual).\n\npub download(_request: Request): Response ->\n    Response.file(\"downloads/report.txt\", status = 200, content_type = \"text/plain; charset=utf-8\").\n\npub manual(_request: Request): Response ->\n    Response.file(\"downloads/manual.pdf\", 206, \"application/pdf\").\n",
    )
    .expect("write file router source");
}

/// Writes a source fixture whose route returns a constant redirect response.
///
/// Inputs:
/// - `path`: file path to write.
///
/// Output:
/// - Terlan module source with one router and constant `Response.redirect`
///   handler body.
///
/// Transformation:
/// - Verifies redirect-producing routes retain source handler identity instead
///   of synthesizing status and Location in the compiler.
pub(super) fn write_redirect_router_source(path: &Path) {
    fs::write(
        path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.new().get(\"/old\", old).\n\npub old(_request: Request): Response ->\n    Response.redirect(\"/new\", status = 301).\n",
    )
    .expect("write redirect router source");
}

/// Writes a source fixture using router receiver calls.
///
/// Inputs:
/// - `path`: destination file path.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Writes a Terlan module with `Router.new().get(...).fallback(...)` so
///   route discovery covers the receiver-chain surface documented for 0.0.5.
pub(super) fn write_receiver_router_source(path: &Path) {
    fs::write(
        path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.new().get(\"/\", home).get(\"/users/:id\", show_user).options(\"/probe\", home).fallback(not_found).\n\npub home(_request: Request): Response ->\n    Response.text(\"home\").\n\npub show_user(_request: Request): Response ->\n    Response.text(\"user\").\n\npub not_found(_request: Request): Response ->\n    Response.text(\"not found\").\n",
    )
    .expect("write receiver router source");
}

/// Writes a source fixture using router groups.
///
/// Inputs:
/// - `path`: destination file path.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Writes a Terlan module with `.group(prefix, (router) -> ...)` so manifest
///   extraction can validate ordinary function-value route grouping.
pub(super) fn write_grouped_router_source(path: &Path) {
    fs::write(
        path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.{MiddlewareResult, Router}.\n\npub router(): Router ->\n    Router.new().group(\"/users\", (router) -> router.use(require_user).get(\"/\", users).get(\"/:id\", show_user).fallback(users_not_found)).\n\npub require_user(_request: Request): MiddlewareResult ->\n    Atom[\"continue\"].\n\npub users(_request: Request): Response ->\n    Response.text(\"users\").\n\npub show_user(_request: Request): Response ->\n    Response.text(\"user\").\n\npub users_not_found(_request: Request): Response ->\n    Response.text(\"missing\").\n",
    )
    .expect("write grouped router source");
}

/// Writes source fixtures for WebSocket route discovery.
///
/// Inputs:
/// - `protocol_path`: destination for the protocol constants module.
/// - `websocket_path`: destination for the WebSocket metadata module.
///
/// Output:
/// - Two Terlan modules where `app.WebSocket` forwards to `app.RoomProtocol`.
///
/// Transformation:
/// - Mirrors an application WebSocket metadata pattern without depending on an
///   app repository during compiler unit tests.
pub(super) fn write_websocket_sources(protocol_path: &Path, websocket_path: &Path) {
    fs::write(
        protocol_path,
        "module app.RoomProtocol.\n\npub route(): String ->\n    \"/ws\".\n\npub protocol(): String ->\n    \"app.room.v1\".\n",
    )
    .expect("write protocol source");
    fs::write(
        websocket_path,
        "module app.WebSocket.\n\nimport app.RoomProtocol.\n\npub route(): String ->\n    RoomProtocol.route().\n\npub protocol(): String ->\n    RoomProtocol.protocol().\n",
    )
    .expect("write websocket source");
}

/// Writes a source fixture with non-constant response handlers.
///
/// Inputs:
/// - `path`: destination file path.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Uses request-derived response bodies so browser package generation keeps
///   the routes in the dynamic `handlers` manifest section.
pub(super) fn write_dynamic_router_source(path: &Path) {
    fs::write(
        path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Request.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    let router = Router.get(Router.new(), \"/\", home);\n    Router.fallback(router, not_found).\n\npub home(request: Request): Response ->\n    Response.text(request.path()).\n\npub not_found(request: Request): Response ->\n    Response.text(request.path(), status = 404).\n",
    )
    .expect("write dynamic router source");
}

/// Writes a source fixture with a router-level error handler.
///
/// Inputs:
/// - `path`: destination file path.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Writes a Terlan module with `.error(render_error)` so manifest generation
///   can preserve source-visible router error callbacks.
pub(super) fn write_error_router_source(path: &Path) {
    fs::write(
        path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Error.HttpError.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.new().get(\"/\", home).error(render_error).\n\npub home(_request: Request): Response ->\n    Response.text(\"home\").\n\npub render_error(_error: HttpError): Response ->\n    Response.text(\"error\").\n",
    )
    .expect("write error router source");
}

/// Writes a source fixture with one invalid route handler reference.
///
/// Inputs:
/// - `path`: destination file path.
/// - `handler_decl`: handler declaration text appended after `router`.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Produces a small Terlan module whose `router` references `home`, allowing
///   tests to vary whether that handler is missing or has the wrong shape.
pub(super) fn write_invalid_router_source(path: &Path, handler_decl: &str) {
    fs::write(
        path,
        format!(
            "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.get(Router.new(), \"/\", home).\n\n{handler_decl}\n"
        ),
    )
    .expect("write invalid router source");
}

/// Writes a source fixture with one invalid route pattern.
///
/// Inputs:
/// - `path`: destination file path.
/// - `route`: route pattern passed to `Router.get`.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Produces a valid handler signature while varying only the source route
///   shape, allowing manifest extraction to prove build-time route validation.
pub(super) fn write_invalid_route_source(path: &Path, route: &str) {
    fs::write(
        path,
        format!(
            "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.get(Router.new(), \"{route}\", home).\n\npub home(_request: Request): Response ->\n    Response.text(\"home\").\n"
        ),
    )
    .expect("write invalid route source");
}

/// Writes a source fixture with same-shape route patterns.
///
/// Inputs:
/// - `path`: destination file path.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Produces two valid handlers whose `GET` routes differ only by parameter
///   name, allowing build-time route-set validation to reject ambiguity.
pub(super) fn write_ambiguous_route_source(path: &Path) {
    fs::write(
        path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    let router = Router.get(Router.new(), \"/users/:id\", show_user);\n    Router.get(router, \"/users/:name\", show_named_user).\n\npub show_user(_request: Request): Response ->\n    Response.text(\"user\").\n\npub show_named_user(_request: Request): Response ->\n    Response.text(\"named user\").\n",
    )
    .expect("write ambiguous route source");
}

/// Verifies simple router builders become browser manifest handler rows.
///
/// Inputs:
/// - One source module with `Router.get` and `Router.fallback` calls.
///
/// Output:
/// - Handler rows for exact, parameter, and expanded fallback routes.
///
/// Transformation:
/// - Executes the source router without running the full JS browser build.
#[test]
pub(super) fn discover_web_route_manifest_extracts_websocket_metadata() {
    let websocket_path = temp_source_path("websocket_metadata").with_file_name("WebSocket.terl");
    let protocol_path = websocket_path.with_file_name("RoomProtocol.terl");
    write_websocket_sources(&protocol_path, &websocket_path);
    let sources = vec![
        WebRouteSourceArtifact {
            module: "app.RoomProtocol".to_string(),
            source_path: protocol_path.display().to_string(),
            manifest_path: None,
        },
        WebRouteSourceArtifact {
            module: "app.WebSocket".to_string(),
            source_path: websocket_path.display().to_string(),
            manifest_path: None,
        },
    ];

    let rows = discover_web_route_manifest_from_sources(&sources).expect("route manifest");

    assert_eq!(rows.websockets.len(), 1);
    assert_eq!(rows.websockets[0].module, "app.WebSocket");
    assert_eq!(rows.websockets[0].route, "/ws");
    assert_eq!(rows.websockets[0].protocol, "app.room.v1");
    assert!(rows.websockets[0].source.is_some());

    fs::remove_file(protocol_path).expect("cleanup protocol source");
    fs::remove_file(websocket_path).expect("cleanup websocket source");
}

/// Verifies grouped source routers publish SSE endpoint discovery metadata.
#[test]
pub(super) fn discover_web_route_manifest_extracts_grouped_sse_routes() {
    let source_path = temp_source_path("grouped_sse_route").with_file_name("Events.terl");
    fs::write(
        &source_path,
        r#"module app.Events.

import std.http.{Router, Sse}.
import type std.http.Router.Router.

pub router(): Router ->
    Router.group(Router.new(), "/live", (router) ->
        Router.sse(router, "/events", Sse.endpoint(8, 4096))
    ).
"#,
    )
    .expect("write grouped SSE source");
    let sources = vec![WebRouteSourceArtifact {
        module: "app.Events".to_string(),
        source_path: source_path.display().to_string(),
        manifest_path: None,
    }];

    let rows = discover_web_route_manifest_from_sources(&sources).expect("route manifest");

    assert_eq!(rows.sse.len(), 1);
    assert_eq!(rows.sse[0].module, "app.Events");
    assert_eq!(rows.sse[0].route, "/live/events");
    assert!(rows.sse[0].source.line > 0);
    assert!(rows.sse[0].source.column > 0);

    fs::remove_file(source_path).expect("cleanup SSE source");
}

#[test]
pub(super) fn discover_web_handlers_from_modules_extracts_router_builder_calls() {
    let source_path = temp_source_path("router_handlers");
    write_router_source(&source_path);
    let modules = vec![module_artifact("app.Http", &source_path)];

    let handlers = discover_web_handlers_from_modules(&modules).expect("discover handlers");

    assert_handler_source(
        matching_handler(&handlers, "GET", "/", "home"),
        &source_path,
    );
    assert_handler_source(
        matching_handler(&handlers, "GET", "/users/:id", "show_user"),
        &source_path,
    );
    assert_handler_source(
        matching_handler(&handlers, "OPTIONS", "/probe", "home"),
        &source_path,
    );
    assert_handler_source(
        matching_handler(&handlers, "HEAD", "*", "not_found"),
        &source_path,
    );
    assert_handler_source(
        matching_handler(&handlers, "OPTIONS", "*", "not_found"),
        &source_path,
    );
    assert_eq!(handlers.len(), 10);

    fs::remove_file(source_path).expect("cleanup router source");
}

/// Verifies route assembly can reference aliased handlers from another module.
#[test]
pub(super) fn discover_web_handlers_from_modules_resolves_selected_imports() {
    let root = temp_dir("imported_router_handlers");
    let app = root.join("src/app");
    fs::create_dir_all(app.join("handlers")).unwrap();
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"imported_routes\"\nversion = \"1.0.0\"\nnamespace = \"app\"\n",
    )
    .unwrap();
    let router_path = app.join("Web.terl");
    let handler_path = app.join("handlers/Pages.terl");
    fs::write(
        &router_path,
        "module app.Web.\n\nimport app.handlers.Pages.{index as index_handler}.\nimport std.http.Router.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.new().get(\"/\", index_handler).\n",
    )
    .expect("write imported router source");
    fs::write(
        &handler_path,
        "module app.handlers.Pages.\n\nimport std.http.Request.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\n\npub index(request: Request): Response ->\n    Response.text(request.path()).\n",
    )
    .expect("write imported handler source");
    let modules = vec![
        module_artifact("app.Web", &router_path),
        module_artifact("app.handlers.Pages", &handler_path),
    ];

    let handlers = discover_web_handlers_from_modules(&modules).expect("discover handlers");

    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].module, "app.Web");
    assert_eq!(handlers[0].function, "index");
    assert_eq!(handlers[0].arity, 1);
    assert_eq!(handlers[0].route, "/");
    assert_eq!(
        handlers[0].source.as_ref().expect("handler source").path,
        "src/app/Web.terl"
    );

    fs::remove_file(router_path).expect("cleanup imported router source");
    fs::remove_file(handler_path).expect("cleanup imported handler source");
}

/// Verifies typed brace route params survive browser manifest discovery.
///
/// Inputs:
/// - One source module with `Router.get("/users/{id:Int}", home)`.
///
/// Output:
/// - Handler row preserving the typed route parameter pattern.
///
/// Transformation:
/// - Exercises the documented typed route parameter syntax at build time so
///   generated manifests stay aligned with `terlc serve` matching.
#[test]
pub(super) fn discover_web_handlers_from_modules_extracts_typed_route_params() {
    let source_path = temp_source_path("typed_route_param_handlers");
    write_invalid_route_source(&source_path, "/users/{id:Int}");
    let modules = vec![module_artifact("app.Http", &source_path)];

    let handlers = discover_web_handlers_from_modules(&modules).expect("discover handlers");

    assert_eq!(handlers.len(), 1);
    assert_handler_source(
        matching_handler(&handlers, "GET", "/users/{id:Int}", "home"),
        &source_path,
    );

    fs::remove_file(source_path).expect("cleanup typed route source");
}

/// Route captures are adapted by ordinary source functions, not compiler arity magic.
#[test]
fn discover_web_handlers_executes_source_route_parameter_adapters() {
    let path = temp_source_path("source_param_adapter");
    fs::write(
        &path,
        r#"module app.Http.
import std.http.{Router, Request, Response}.
import std.core.Option.{Some, None}.
import type std.http.Router.Router.
import type std.http.Request.Request.
import type std.http.Response.Response.
pub router(): Router ->
    Router.new().get("/users/:id", (request) ->
        case request.param("id") {
            Some(id) -> show_user(request, id);
            None -> Response.text("missing", 400)
        }
    ).get("/filter/{active:Bool}", (request) ->
        filter(request, request.param("active") == Some("true"))
    ).
pub show_user(_request: Request, id: String): Response -> Response.text(id).
pub filter(_request: Request, active: Bool): Response ->
    if { active -> Response.text("active"); true -> Response.text("inactive") }.
"#,
    )
    .unwrap();
    let handlers = discover_web_handlers_from_modules(&[module_artifact("app.Http", &path)])
        .expect("source adapters compile");
    assert_eq!(handlers.len(), 2);
    for (row, expected) in handlers.iter().zip(["/users/:id", "/filter/{active:Bool}"]) {
        assert_eq!(row.route, expected);
        assert_eq!(row.arity, 1);
        assert_eq!(row.module, "app.Http");
        assert!(row.function.starts_with("closure_"));
    }
    fs::remove_file(path).unwrap();
}

/// Every unsupported callback shape is rejected by the ordinary package type contract.
#[test]
fn discover_web_handlers_rejects_unadapted_route_parameter_callbacks() {
    for (route, parameter) in [
        ("/users/:id", "id: String"),
        ("/users/{id:Int}", "id: String"),
        ("/users/{id:Int}", "user_id: Int"),
        ("/users/{active:Bool}", "active: Bool"),
    ] {
        let path = temp_source_path("unadapted_route_params");
        fs::write(
            &path,
            format!(
                r#"module app.Http.
import std.http.{{Router, Response}}.
import type std.http.Router.Router.
import type std.http.Request.Request.
import type std.http.Response.Response.
pub router(): Router -> Router.new().get("{route}", show_user).
pub show_user(_request: Request, {parameter}): Response -> Response.text("unused").
"#
            ),
        )
        .unwrap();
        let error = discover_web_handlers_from_modules(&[module_artifact("app.Http", &path)])
            .expect_err("Handler requires one Request argument");
        assert!(error.contains("compilation failed"), "{error}");
        fs::remove_file(path).unwrap();
    }
}

/// Verifies receiver-style router builders become handler rows.
///
/// Inputs:
/// - One source module with `Router.new().get(...).fallback(...)`.
///
/// Output:
/// - Handler rows for exact, parameter, and expanded fallback routes.
///
/// Transformation:
/// - Exercises route discovery for the receiver-call surface exposed by
///   `std.http.Router`, not only the static-call lowering shape.
#[test]
pub(super) fn discover_web_handlers_from_modules_extracts_receiver_router_builder_calls() {
    let source_path = temp_source_path("receiver_router_handlers");
    write_receiver_router_source(&source_path);
    let modules = vec![module_artifact("app.Http", &source_path)];

    let handlers = discover_web_handlers_from_modules(&modules).expect("discover handlers");

    assert_handler_source(
        matching_handler(&handlers, "GET", "/", "home"),
        &source_path,
    );
    assert_handler_source(
        matching_handler(&handlers, "GET", "/users/:id", "show_user"),
        &source_path,
    );
    assert_handler_source(
        matching_handler(&handlers, "OPTIONS", "/probe", "home"),
        &source_path,
    );
    assert_handler_source(
        matching_handler(&handlers, "HEAD", "*", "not_found"),
        &source_path,
    );
    assert_handler_source(
        matching_handler(&handlers, "OPTIONS", "*", "not_found"),
        &source_path,
    );
    assert_eq!(handlers.len(), 10);

    fs::remove_file(source_path).expect("cleanup receiver router source");
}

/// Verifies grouped router builders become prefixed handler rows.
///
/// Inputs:
/// - One source module with `Router.new().group("/users", (router) -> ...)`.
///
/// Output:
/// - Handler rows whose routes are prefixed by `/users`.
///
/// Transformation:
/// - Executes package-owned grouping and projects its returned declarations.
#[test]
pub(super) fn discover_web_handlers_from_modules_extracts_grouped_router_builder_calls() {
    let source_path = temp_source_path("grouped_router_handlers");
    write_grouped_router_source(&source_path);
    let modules = vec![module_artifact("app.Http", &source_path)];

    let handlers = discover_web_handlers_from_modules(&modules).expect("discover handlers");

    assert_handler_source(
        matching_handler(&handlers, "GET", "/users", "users"),
        &source_path,
    );
    assert_handler_source(
        matching_handler(&handlers, "GET", "/users/:id", "show_user"),
        &source_path,
    );
    assert_handler_source(
        matching_handler(&handlers, "HEAD", "/users/*", "users_not_found"),
        &source_path,
    );
    assert_handler_source(
        matching_handler(&handlers, "OPTIONS", "/users/*", "users_not_found"),
        &source_path,
    );
    assert_eq!(handlers.len(), 9);

    fs::remove_file(source_path).expect("cleanup grouped router source");
}

/// Verifies browser package manifests serialize discovered handlers.
///
/// Inputs:
/// - A fake JS build root with one emitted module file.
/// - A source module with supported `Router` builder calls.
///
/// Output:
/// - `_build/web/manifest.json` containing dynamic handler rows.
///
/// Transformation:
/// - Exercises the browser package writer boundary so route discovery is proven
///   to affect the actual manifest consumed by `terlc serve`.
#[test]
pub(super) fn write_browser_package_serializes_discovered_router_handlers() {
    let root = temp_dir("package_handlers");
    let js_root = root.join("js");
    let modules_dir = js_root.join("modules");
    fs::create_dir_all(&modules_dir).expect("create modules dir");
    fs::write(modules_dir.join("app.js"), "export {};\n").expect("write js module");

    let source_path = root.join("app/Http.terl");
    write_dynamic_router_source(&source_path);
    let modules = vec![module_artifact("app.Http", &source_path)];
    let contract = js_target_contract(TargetProfile::JsBrowser).expect("browser contract");

    write_browser_package(&js_root, contract, &modules, None, false).expect("write package");

    let manifest_text =
        fs::read_to_string(root.join("web/manifest.json")).expect("read web manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse web manifest");
    let handlers = manifest["handlers"].as_array().expect("handlers");
    assert!(handlers.iter().any(|handler| {
        handler["method"] == "GET" && handler["route"] == "/" && handler["function"] == "home"
    }));
    assert!(handlers.iter().any(|handler| {
        handler["method"] == "HEAD" && handler["route"] == "*" && handler["function"] == "not_found"
    }));
    assert_eq!(handlers.len(), 8);
    let home = handlers
        .iter()
        .find(|handler| {
            handler["method"] == "GET" && handler["route"] == "/" && handler["function"] == "home"
        })
        .expect("home handler");
    assert_eq!(home["source"]["path"], "Http.terl");
    assert!(home["source"]["line"].as_u64().expect("source line") >= 1);
    assert!(home["source"]["column"].as_u64().expect("source column") >= 1);

    fs::remove_dir_all(root).expect("cleanup package dir");
}
