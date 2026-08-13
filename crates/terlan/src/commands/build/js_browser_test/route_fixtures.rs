use super::*;

use super::super::manifest::{WebErrorHandlerArtifact, WebHandlerArtifact};
use super::super::routes::{
    discover_web_error_handler_from_modules, discover_web_handlers_from_modules,
    discover_web_route_manifest_from_sources,
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
    test_fs::temp_path("js_browser", name).with_extension("terl")
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
    test_fs::temp_dir("js_browser", name)
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
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    let router = Router.get(Router.new(), \"/\", home);\n    let router = Router.get(router, \"/users/:id\", show_user);\n    let router = Router.options(router, \"/probe\", home);\n    Router.fallback(router, not_found).\n\npub home(_request: Request): Response ->\n    Response.text(\"home\").\n\npub show_user(_request: Request): Response ->\n    Response.text(\"user\").\n\npub not_found(_request: Request): Response ->\n    Response.text(\"not found\").\n",
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
/// - Produces source that route discovery can classify into manifest
///   `file_responses` without invoking typechecking or backend emission.
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
/// - Produces source that route discovery can classify into manifest
///   `static_responses` with a `Location` header.
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

/// Writes a source fixture with one invalid middleware reference.
///
/// Inputs:
/// - `path`: destination file path.
/// - `middleware_decl`: optional middleware declaration text appended after
///   the handler.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Produces a router chain containing `Router.use(...).get(...)`, allowing
///   tests to vary whether the middleware is missing or has the wrong shape
///   while keeping the route handler valid.
pub(super) fn write_invalid_middleware_source(path: &Path, middleware_decl: &str) {
    fs::write(
        path,
        format!(
            "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.{{MiddlewareResult, Router}}.\n\npub router(): Router ->\n    Router.use(Router.new(), require_user).get(\"/\", home).\n\npub home(_request: Request): Response ->\n    Response.text(\"home\").\n\n{middleware_decl}\n"
        ),
    )
    .expect("write invalid middleware source");
}

/// Writes a source fixture with one response middleware reference.
pub(super) fn write_response_middleware_source(path: &Path, middleware_decl: &str) {
    fs::write(
        path,
        format!(
            "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.map_response(Router.new(), decorate).get(\"/\", home).\n\npub home(_request: Request): Response ->\n    Response.text(\"home\").\n\n{middleware_decl}\n"
        ),
    )
    .expect("write response middleware source");
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
/// - Exercises source reparse plus router-call extraction without running the
///   full JS browser build.
#[test]
pub(super) fn discover_web_route_manifest_extracts_websocket_metadata() {
    let protocol_path = temp_source_path("room_protocol");
    let websocket_path = temp_source_path("websocket_metadata");
    write_websocket_sources(&protocol_path, &websocket_path);
    let sources = vec![
        WebRouteSourceArtifact {
            module: "app.RoomProtocol".to_string(),
            source_path: protocol_path.display().to_string(),
        },
        WebRouteSourceArtifact {
            module: "app.WebSocket".to_string(),
            source_path: websocket_path.display().to_string(),
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
    let source_path = temp_source_path("grouped_sse_route");
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
    }];

    let rows = discover_web_route_manifest_from_sources(&sources).expect("route manifest");

    assert_eq!(rows.sse.len(), 1);
    assert_eq!(rows.sse[0].module, "app.Events");
    assert_eq!(rows.sse[0].route, "/live/events");
    assert!(rows.sse[0].source.is_some());

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

/// Verifies route handlers may accept route captures as direct parameters.
///
/// Inputs:
/// - One source module with `Router.get("/users/:id", show_user)`.
/// - A `show_user(request: Request, id: String): Response` handler.
///
/// Output:
/// - Handler manifest row with arity 2.
///
/// Transformation:
/// - Locks the build-time half of route-param handler support so generated
///   manifests preserve the function arity that `terlc serve` will invoke.
#[test]
pub(super) fn discover_web_handlers_from_modules_preserves_route_param_handler_arity() {
    let source_path = temp_source_path("route_param_handler_arity");
    fs::write(
        &source_path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.get(Router.new(), \"/users/:id\", show_user).\n\npub show_user(_request: Request, id: String): Response ->\n    Response.text(id).\n",
    )
    .expect("write route-param handler source");
    let modules = vec![module_artifact("app.Http", &source_path)];

    let handlers = discover_web_handlers_from_modules(&modules).expect("discover handlers");
    let handler = handlers
        .iter()
        .find(|handler| {
            handler.method == "GET"
                && handler.route == "/users/:id"
                && handler.function == "show_user"
        })
        .expect("route-param handler row");

    assert_eq!(handler.arity, 2);
    fs::remove_file(source_path).expect("cleanup route-param handler source");
}

/// Verifies typed route params must match handler parameter types.
///
/// Inputs:
/// - One source module with route `/users/{id:Int}`.
/// - Handler signature `show_user(request: Request, id: String): Response`.
///
/// Output:
/// - Stable `error[web_router]` diagnostic.
///
/// Transformation:
/// - Proves typed route params are validated before a manifest can reach
///   `terlc serve`.
#[test]
pub(super) fn discover_web_handlers_from_modules_rejects_route_param_type_mismatch() {
    let source_path = temp_source_path("route_param_type_mismatch");
    fs::write(
        &source_path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.get(Router.new(), \"/users/{id:Int}\", show_user).\n\npub show_user(_request: Request, id: String): Response ->\n    Response.text(id).\n",
    )
    .expect("write route-param mismatch source");
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules).expect_err("type mismatch");

    assert!(error.contains("capture `id` must be `Int`, got `String`"));
    fs::remove_file(source_path).expect("cleanup route-param mismatch source");
}

/// Verifies direct route-param handlers must preserve capture names.
///
/// Inputs:
/// - One source module with route `/users/{id:Int}`.
/// - Handler signature `show_user(request: Request, user_id: Int): Response`.
///
/// Output:
/// - Stable `error[web_router]` diagnostic.
///
/// Transformation:
/// - Proves typed route params are mapped to handler arguments by name before
///   a manifest can preserve direct route-param arity.
#[test]
pub(super) fn discover_web_handlers_from_modules_rejects_route_param_name_mismatch() {
    let source_path = temp_source_path("route_param_name_mismatch");
    fs::write(
        &source_path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.get(Router.new(), \"/users/{id:Int}\", show_user).\n\npub show_user(_request: Request, user_id: Int): Response ->\n    Response.text(user_id.to_string()).\n",
    )
    .expect("write route-param name mismatch source");
    let modules = vec![module_artifact("app.Http", &source_path)];

    let error = discover_web_handlers_from_modules(&modules).expect_err("name mismatch");

    assert!(error.contains("error[web_router]: handler `show_user` parameter 2"));
    assert!(error.contains("capture `id` must be named `id`, got `user_id`"));
    fs::remove_file(source_path).expect("cleanup route-param name mismatch source");
}

/// Verifies boolean typed route params can be bound to handler parameters.
///
/// Inputs:
/// - One source module with route `/users/{active:Bool}`.
/// - Handler signature `filter(request: Request, active: Bool): Response`.
///
/// Output:
/// - Handler manifest row with arity 2.
///
/// Transformation:
/// - Keeps browser manifest extraction aligned with the supported route-param
///   decoder types used by `terlc serve`.
#[test]
pub(super) fn discover_web_handlers_from_modules_accepts_bool_route_param_handler() {
    let source_path = temp_source_path("bool_route_param_handler");
    fs::write(
        &source_path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    Router.get(Router.new(), \"/users/{active:Bool}\", filter).\n\npub filter(_request: Request, active: Bool): Response ->\n    Response.text(\"ok\").\n",
    )
    .expect("write bool route-param handler source");
    let modules = vec![module_artifact("app.Http", &source_path)];

    let handlers = discover_web_handlers_from_modules(&modules).expect("discover handlers");
    let handler = handlers
        .iter()
        .find(|handler| {
            handler.method == "GET"
                && handler.route == "/users/{active:Bool}"
                && handler.function == "filter"
        })
        .expect("bool route-param handler row");

    assert_eq!(handler.arity, 2);
    fs::remove_file(source_path).expect("cleanup bool route-param handler source");
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
/// - Exercises compile-time group extraction without introducing route
///   declaration syntax or evaluating the router value.
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

/// Verifies router-level error builders become error-handler manifest rows.
///
/// Inputs:
/// - One source module with `Router.new().get(...).error(render_error)`.
///
/// Output:
/// - One error-handler row for the source-visible error callback.
///
/// Transformation:
/// - Exercises error-handler discovery separately from normal route discovery
///   so the manifest can later support runtime error dispatch.
#[test]
pub(super) fn discover_web_error_handler_from_modules_extracts_router_error_handler() {
    let source_path = temp_source_path("router_error_handler");
    write_error_router_source(&source_path);
    let modules = vec![module_artifact("app.Http", &source_path)];

    let handler =
        discover_web_error_handler_from_modules(&modules).expect("discover error handler");

    assert_eq!(
        handler,
        Some(WebErrorHandlerArtifact {
            module: "app.Http".to_string(),
            function: "render_error".to_string(),
            arity: 1,
        })
    );
    fs::remove_file(source_path).expect("cleanup router error source");
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

    let source_path = root.join("Http.terl");
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
