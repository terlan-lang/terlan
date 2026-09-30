use super::*;
use crate::commands::serve::handler::HandlerResponse;
use crate::commands::serve::handler_cache::handler_cache_test_support::compile_native_handler_fixture;
use crate::runtime::vm::http_router::VmHttpRouterOutcome;
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
import std.http.{Router, Response, Sse, WebSocket}.
import std.http.Router.{Continue}.
import type std.http.Router.Router.
import type std.http.Request.Request.

pub idle(): Unit -> Unit.
pub event(_value: String): Unit -> Unit.
pub router(): Router ->
    let part = "computed";
    Router.new()
        .group("/api", (child: Router) ->
            child.use((_request: Request) -> Continue)
                .get("/" + part, (_request: Request) -> Response.text(part, 202)))
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
        ("duplicate_sse", "Router.new().sse(\"/events\", Sse.endpoint(1, 8).callbacks(idle, event, idle, idle, event).callbacks(idle, event, idle, idle, event))", "SSE callbacks already configured"),
        ("invalid_websocket", "Router.new().websocket(\"/socket\", WebSocket.endpoint(0, 8))", "positive Int limit"),
    ] {
        let source = format!(r#"module app.InvalidRouter.
import std.core.Unit.
import std.http.{{Router, Response, Sse, WebSocket}}.
import type std.http.Router.Router.
import type std.http.Request.Request.
import type std.http.Response.Response.
pub idle(): Unit -> Unit.
pub event(_value: String): Unit -> Unit.
pub handle(_request: Request): Response -> Response.text("unused").
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
