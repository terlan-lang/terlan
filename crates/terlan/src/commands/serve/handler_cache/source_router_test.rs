use super::*;
use crate::commands::serve::handler::HandlerResponse;
use crate::commands::serve::handler_cache::handler_cache_test_support::compile_native_handler_fixture;
use crate::runtime::vm::http_router::{VmHttpRouteMethod, VmHttpRouteTarget, VmHttpRouterOutcome};
use crate::runtime::vm::ReplValue;

fn request(path: &str) -> ReplValue {
    crate::commands::serve::handler::request_materialization::vm_request_descriptor_owned(
        crate::runtime::native::http::Request::from_parts("GET", path, "").into_parts(),
        crate::runtime::native::http::RequestFieldProjection::Complete,
    )
}

#[test]
fn computed_router_and_captured_callbacks_execute_from_source() {
    let fixture = compile_native_handler_fixture(
        "source_router_admission",
        "src/app/SourceRouter.terl",
        "app_SourceRouter",
        r#"module app.SourceRouter.
import std.core.Unit.
import std.http.{Router, Response, Sse, WebSocket, Error}.
import std.http.Router.{Continue}.
import type std.http.Router.Router.
import type std.http.Request.Request.
import type std.http.Error.HttpError.

pub idle(): Unit -> Unit.
pub event(_value: String): Unit -> Unit.
pub router(): Router ->
    let part = "computed";
    Router.new()
        .group("/api", (child: Router) ->
            child.use((_request: Request) -> Continue)
                .get("/" + part, (_request: Request) -> Response.text(part, 202))
                .error((error: HttpError) ->
                    if {
                        Error.code(error) == Atom["router_execution_failed"] and Error.status(error) == 500 -> Response.text(part + ":" + Error.message(error), 503);
                        true -> Response.text("invalid recovery", 500)
                    }))
        .sse("/events", Sse.endpoint_with_keep_alive(4, 1024, 50)
            .callbacks(idle, event, idle, idle, event))
        .websocket("/socket", WebSocket.endpoint(3, 512)
            .callbacks(idle, event, idle, idle, event)).
"#,
    );
    assert!(fixture.router.is_none(), "no static compiler plan");
    let runtime = AotHandlerRuntime::load_with_shard_count(
        "app.SourceRouter".into(),
        &fixture.image,
        None,
        1,
    )
    .unwrap();
    assert!(runtime.has_function("app.SourceRouter", "router", 0));
    let router = runtime
        .execute_http_router("app.SourceRouter", "router", &mut |_| {})
        .unwrap();
    let VmHttpRouterOutcome::Matched(route) = router
        .dispatch(VmHttpRouteMethod::Get, "/api/computed")
        .unwrap()
    else {
        panic!("computed grouped route was not admitted");
    };
    assert_eq!(route.middleware.len(), 1);
    let VmHttpRouteTarget::Handler(handler) = route.target else {
        panic!("handler")
    };
    assert!(matches!(handler, ReplValue::Closure(_)));
    let response = runtime
        .execute_callable(
            "app.SourceRouter",
            &handler,
            vec![request("/api/computed")],
            &mut |_| {},
        )
        .unwrap();
    let response =
        HandlerResponse::from_owned_vm_response_with_package_root(response, &fixture.root).unwrap();
    assert_eq!(response.status, 202);
    assert_eq!(response.body.as_bytes(), b"computed");
    let recovery = router
        .error_handler()
        .expect("source-lifted group recovery");
    let cause = ReplValue::String("failure".into());
    let recovered = runtime
        .execute_callable("app.SourceRouter", recovery, vec![cause], &mut |_| {})
        .unwrap();
    let recovered =
        HandlerResponse::from_owned_vm_response_with_package_root(recovered, &fixture.root)
            .unwrap();
    assert_eq!(recovered.status, 503);
    assert_eq!(recovered.body.as_bytes(), b"computed:failure");
    let recovered = crate::commands::serve::handler::execute_router_recovery(
        &runtime,
        "app.SourceRouter",
        &router,
        "host failure".into(),
        &mut |_| {},
    )
    .unwrap();
    let recovered =
        HandlerResponse::from_owned_vm_response_with_package_root(recovered, &fixture.root)
            .unwrap();
    assert_eq!(recovered.status, 503);
    assert_eq!(recovered.body.as_bytes(), b"computed:host failure");
    for path in ["/events", "/socket"] {
        let VmHttpRouterOutcome::Matched(route) =
            router.dispatch(VmHttpRouteMethod::Get, path).unwrap()
        else {
            panic!("channel route")
        };
        let open = match route.target {
            VmHttpRouteTarget::SseEndpoint(plan) => {
                assert_eq!(plan.keep_alive_ms(), Some(50));
                assert_eq!(plan.max_pending_events(), 4);
                plan.callbacks().unwrap().open.clone()
            }
            VmHttpRouteTarget::WebSocketEndpoint(plan) => {
                assert_eq!(plan.max_pending_frames(), 3);
                plan.callbacks().unwrap().open.clone()
            }
            _ => panic!("channel endpoint"),
        };
        assert_eq!(
            runtime
                .execute_callable("app.SourceRouter", &open, vec![], &mut |_| {})
                .unwrap(),
            ReplValue::Unit
        );
    }
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn source_router_persisted_generations_reexecute_source_and_keep_captures_isolated() {
    use crate::commands::serve::handler_cache::{
        cached_source_entry, invalidate_vm_handler_cache, source_generation,
    };
    use std::fs;

    const MODULE: &str = "app.RouterGeneration";
    let root = crate::support::test_fs::temp_path("serve", "source_router_generation");
    let web_root = root.join("_build/web");
    let source_path = root.join("src/app/RouterGeneration.terl");
    fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    fs::create_dir_all(&web_root).unwrap();
    let source = |marker: &str| {
        format!(
            r#"module app.RouterGeneration.
import std.http.{{Router, Response}}.
import type std.http.Router.Router.
import type std.http.Request.Request.
marker(): String -> "{marker}".
pub router(): Router ->
    let value = marker();
    Router.new().get("/" + value, (_request: Request) -> Response.text(value)).
"#
        )
    };
    let execute = |runtime: &AotHandlerRuntime, marker: &str| {
        let router = runtime
            .execute_http_router(MODULE, "router", &mut |_| {})
            .unwrap();
        let VmHttpRouterOutcome::Matched(route) = router
            .dispatch(VmHttpRouteMethod::Get, &format!("/{marker}"))
            .unwrap()
        else {
            panic!("source route missing")
        };
        let VmHttpRouteTarget::Handler(handler) = route.target else {
            panic!("handler")
        };
        let value = runtime
            .execute_callable(
                MODULE,
                &handler,
                vec![request(&format!("/{marker}"))],
                &mut |_| {},
            )
            .unwrap();
        let response =
            HandlerResponse::from_owned_vm_response_with_package_root(value, &root).unwrap();
        assert_eq!(response.body.as_bytes(), marker.as_bytes());
    };
    fs::write(&source_path, source("first")).unwrap();
    let first = cached_source_entry(&web_root, &source_path, MODULE).unwrap();
    execute(&first.runtime, "first");
    let metadata_path = source_generation::active_generation_metadata_path(&web_root, MODULE)
        .unwrap()
        .unwrap();
    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(metadata_path).unwrap()).unwrap();
    assert!(
        metadata.get("router").is_none(),
        "route plans must not be persisted"
    );
    invalidate_vm_handler_cache();
    let restored = cached_source_entry(&web_root, &source_path, MODULE).unwrap();
    execute(&restored.runtime, "first");
    fs::write(&source_path, source("second")).unwrap();
    invalidate_vm_handler_cache();
    let second = cached_source_entry(&web_root, &source_path, MODULE).unwrap();
    execute(&second.runtime, "second");
    execute(&first.runtime, "first");
    execute(&restored.runtime, "first");
    invalidate_vm_handler_cache();
    drop((first, restored, second));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn source_router_rejects_invalid_executed_policy_before_admission() {
    for (name, expression, expected) in [
        ("duplicate_fallback", "Router.new().fallback(handle).fallback(handle)", "duplicate fallback"),
        ("duplicate_group_fallback", "Router.new().group(\"/api\", (child: Router) -> child.fallback(handle).fallback(handle))", "duplicate fallback"),
        ("duplicate_group_error", "Router.new().group(\"/api\", (child: Router) -> child.error(recover).error(recover))", "duplicate error handler"),
        ("error_after_inheritance", "Router.new().group(\"/api\", (child: Router) -> child.error(recover)).error(recover)", "duplicate error handler"),
        ("nested_error_after_inheritance", "Router.new().group(\"/api\", (child: Router) -> child.group(\"/nested\", (inner: Router) -> inner.error(recover)).error(recover))", "duplicate error handler"),
        ("duplicate_sse", "Router.new().sse(\"/events\", Sse.endpoint(1, 8).callbacks(idle, event, idle, idle, event).callbacks(idle, event, idle, idle, event))", "SSE callbacks already configured"),
        ("invalid_websocket", "Router.new().websocket(\"/socket\", WebSocket.endpoint(0, 8))", "positive Int limit"),
    ] {
        let source = format!(r#"module app.InvalidRouter.
import std.core.Unit.
import std.http.{{Router, Response, Sse, WebSocket}}.
import type std.http.Router.Router.
import type std.http.Request.Request.
import type std.http.Response.Response.
import type std.http.Error.HttpError.
pub idle(): Unit -> Unit.
pub event(_value: String): Unit -> Unit.
pub handle(_request: Request): Response -> Response.text("unused").
pub recover(_error: HttpError): Response -> Response.text("recovered").
pub router(): Router -> {expression}.
"#);
        let fixture = compile_native_handler_fixture(name, "src/app/InvalidRouter.terl", "app_InvalidRouter", &source);
        let error = match AotHandlerRuntime::load_with_shard_count("app.InvalidRouter".into(), &fixture.image, None, 1) {
            Ok(_) => panic!("invalid executed router was admitted"),
            Err(error) => error,
        };
        assert!(error.contains(expected), "{name}: {error}");
        std::fs::remove_dir_all(fixture.root).unwrap();
    }
}

#[test]
fn source_group_composes_nested_paths_channels_and_fallbacks_before_admission() {
    let fixture = compile_native_handler_fixture(
        "source_group_composition",
        "src/app/Groups.terl",
        "app_Groups",
        r#"module app.Groups.
import std.http.{Router, Response, Sse, WebSocket}.
import std.http.Router.{Continue}.
import type std.http.Router.Router.
import type std.http.Request.Request.
import type std.http.Response.Response.
pub handle(_request: Request): Response -> Response.text("group").
pub router(): Router ->
    Router.new()
        .group("/", (root: Router) -> root.get("/", handle))
        .group("/api///", (outer: Router) ->
            outer.use((_request: Request) -> Continue)
                .fallback(handle)
                .group("/nested/", (inner: Router) ->
                    inner.use((_request: Request) -> Continue)
                        .get("/", handle)
                        .sse("///events", Sse.endpoint(2, 512))
                        .websocket("/socket", WebSocket.endpoint(2, 512))
                        .fallback(handle))
                .get("/users", handle))
        .group("/caf\u00e9/", (unicode: Router) -> unicode.get("/menu", handle)).
"#,
    );
    let runtime =
        AotHandlerRuntime::load_with_shard_count("app.Groups".into(), &fixture.image, None, 1)
            .unwrap();
    // Inspect the executed source value itself: native admission must not be
    // responsible for constructing these paths or synthesizing fallback routes.
    let value = runtime
        .generation
        .image
        .spawn_shard()
        .unwrap()
        .call("app.Groups.router", &[])
        .unwrap();
    let ReplValue::Record { fields, .. } = &value else {
        panic!("router")
    };
    let ReplValue::List(entries) = &fields.iter().find(|(key, _)| key == "entries").unwrap().1
    else {
        panic!("entries")
    };
    let paths: Vec<_> = entries
        .iter()
        .filter_map(|entry| {
            let ReplValue::Record { fields, .. } = entry else {
                return None;
            };
            fields
                .iter()
                .find_map(|(key, value)| match (key.as_str(), value) {
                    ("path", ReplValue::String(path)) => Some(path.as_str()),
                    _ => None,
                })
        })
        .collect();
    assert_eq!(
        &paths[..4],
        &[
            "/",
            "/api/nested",
            "/api/nested/events",
            "/api/nested/socket"
        ]
    );
    assert_eq!(
        paths
            .iter()
            .filter(|path| **path == "/api/nested/*")
            .count(),
        7
    );
    assert_eq!(paths.iter().filter(|path| **path == "/api/*").count(), 7);
    assert_eq!(paths.last(), Some(&"/caf\u{e9}/menu"));
    let plan = source_descriptor::router(&value, |callback, _| Ok(callback.clone())).unwrap();
    assert_eq!(plan.routes.len(), 20);
    for route in &plan.routes {
        let expected = if route.path.starts_with("/api/nested") {
            2
        } else if route.path.starts_with("/api/") {
            1
        } else {
            0
        };
        assert_eq!(route.middleware.len(), expected, "{}", route.path);
    }
    for prefix in ["/api", "/api/nested"] {
        let methods: Vec<_> = plan
            .routes
            .iter()
            .filter(|route| route.path == format!("{prefix}/*"))
            .map(|route| route.method.as_str())
            .collect();
        assert_eq!(
            methods,
            ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]
        );
    }
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
