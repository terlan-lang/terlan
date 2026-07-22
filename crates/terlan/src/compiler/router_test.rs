//! Tests for closure-free AOT router-plan extraction.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::lower_syntax_module_output_to_core;

use super::{prepare_aot_router_module, AotRouterRoute, AotRouterRouteTarget};

/// Returns the ordinary handler target attached to one static route.
fn route_handler(route: &AotRouterRoute) -> &super::AotRouterCallable {
    let AotRouterRouteTarget::Handler(handler) = &route.target else {
        panic!("expected ordinary handler route")
    };
    handler
}

/// Verifies chained builders become ordered static callback metadata.
#[test]
fn aot_router_plan_extracts_routes_middleware_fallback_and_error() {
    let source = r#"module app.Api.

import std.http.{Response, Router}.
import std.http.Router.Continue.
import type std.http.{Request, Response, Router}.
import type std.http.Error.HttpError.
import type std.http.Router.MiddlewareResult.

pub gate(_request: Request): MiddlewareResult -> Continue.
pub after_response(_request: Request, response: Response): Response -> response.
pub home(_request: Request): Response -> Response.text("home").
pub missing(_request: Request): Response -> Response.text("missing").
pub recover(_error: HttpError): Response -> Response.text("error").
pub router(): Router ->
    Router.new().use(gate).map_response(after_response).get("/", home).fallback(missing).error(recover).
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse router fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    let (executable, plan) = prepare_aot_router_module(&core).expect("extract router plan");
    let plan = plan.expect("router plan");
    assert!(!executable
        .functions
        .iter()
        .any(|function| function.name == "router"));
    assert_eq!(plan.routes.len(), 1);
    assert_eq!(plan.routes[0].method, "GET");
    assert_eq!(plan.routes[0].path, "/");
    assert_eq!(route_handler(&plan.routes[0]).function, "home");
    assert_eq!(plan.middleware[0].function, "gate");
    assert_eq!(plan.response_middleware[0].function, "after_response");
    assert_eq!(plan.fallback.expect("fallback").function, "missing");
    assert_eq!(plan.error.expect("error").function, "recover");
}

/// Verifies grouped callbacks retain scoped middleware and prefixed fallback routes.
#[test]
fn aot_router_plan_flattens_group_scope_without_closures() {
    let source = r#"module app.Api.

import std.http.{Response, Router}.
import std.http.Router.Continue.
import type std.http.{Request, Response, Router}.
import type std.http.Router.MiddlewareResult.

pub gate(_request: Request): MiddlewareResult -> Continue.
pub home(_request: Request): Response -> Response.text("home").
pub missing(_request: Request): Response -> Response.text("missing").
pub router(): Router ->
    Router.new().group("/users", (router) -> router.use(gate).get("/", home).fallback(missing)).
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse grouped fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    let (_, plan) = prepare_aot_router_module(&core).expect("extract group plan");
    let plan = plan.expect("router plan");
    assert!(plan
        .routes
        .iter()
        .any(|route| route.path == "/users" && route_handler(route).function == "home"));
    assert!(plan.routes.iter().any(|route| {
        route.path == "/users/*"
            && route_handler(route).function == "missing"
            && route.middleware[0].function == "gate"
    }));
}

/// Verifies channel builders become canonical VM plans without CoreIR residue.
#[test]
fn aot_router_plan_materializes_canonical_channel_targets() {
    let source = r#"module app.Channels.

import std.http.{Router, Sse, WebSocket}.
import type std.http.Router.

pub router(): Router ->
    Router.new()
        .sse("/events", Sse.endpoint_with_keep_alive(8, 4096, 15000))
        .websocket("/socket", WebSocket.endpoint(4, 1024)).
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse channel router fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    let (executable, plan) = prepare_aot_router_module(&core).expect("extract channel plan");
    assert!(!executable
        .functions
        .iter()
        .any(|function| function.name == "router"));
    let plan = plan.expect("channel router plan");
    assert_eq!(plan.routes.len(), 2);
    let AotRouterRouteTarget::Sse(sse) = &plan.routes[0].target else {
        panic!("expected SSE target")
    };
    assert_eq!(sse.max_pending_events(), 8);
    assert_eq!(sse.max_event_bytes(), 4096);
    assert_eq!(sse.keep_alive_ms(), Some(15000));
    let AotRouterRouteTarget::WebSocket(websocket) = &plan.routes[1].target else {
        panic!("expected WebSocket target")
    };
    assert_eq!(websocket.max_pending_frames, 4);
    assert_eq!(websocket.max_frame_bytes, 1024);
}

/// Verifies WebSocket callback builders retain one complete static callback set.
#[test]
fn aot_router_plan_materializes_websocket_callbacks() {
    let source = r#"module app.Socket.

import std.core.Unit.
import std.http.{Router, WebSocket}.
import type std.http.Router.
pub opened(): Unit -> Unit.
pub inbound(_frame: String): Unit -> Unit.
pub writable(): Unit -> Unit.
pub closed(): Unit -> Unit.
pub cancelled(_reason: String): Unit -> Unit.
pub router(): Router ->
    Router.new().websocket(
        "/socket",
        WebSocket.endpoint(4, 1024).callbacks(opened, inbound, writable, closed, cancelled)
    ).
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse callback router fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    let (_, plan) = prepare_aot_router_module(&core).expect("extract callback router plan");
    let plan = plan.expect("router plan");
    let AotRouterRouteTarget::WebSocket(websocket) = &plan.routes[0].target else {
        panic!("expected WebSocket target")
    };
    let callbacks = websocket.callbacks().expect("callback plan");
    assert_eq!(callbacks.open.function, "opened");
    assert_eq!(callbacks.inbound.function, "inbound");
    assert_eq!(callbacks.writable.function, "writable");
    assert_eq!(callbacks.close.function, "closed");
    assert_eq!(callbacks.cancellation.function, "cancelled");
}

/// Verifies SSE callback builders retain one complete static callback set.
#[test]
fn aot_router_plan_materializes_sse_callbacks() {
    let source = r#"module app.Events.

import std.core.Unit.
import std.http.{Router, Sse}.
import type std.http.Router.
pub opened(): Unit -> Unit.
pub event_ready(_data: String): Unit -> Unit.
pub keep_alive(): Unit -> Unit.
pub drained(): Unit -> Unit.
pub cancelled(_reason: String): Unit -> Unit.
pub router(): Router ->
    Router.new().sse(
        "/events",
        Sse.endpoint_with_keep_alive(4, 1024, 15000)
            .callbacks(opened, event_ready, keep_alive, drained, cancelled)
    ).
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse callback router fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    let (_, plan) = prepare_aot_router_module(&core).expect("extract callback router plan");
    let plan = plan.expect("router plan");
    let AotRouterRouteTarget::Sse(sse) = &plan.routes[0].target else {
        panic!("expected SSE target")
    };
    let callbacks = sse.callbacks().expect("callback plan");
    assert_eq!(callbacks.open.function, "opened");
    assert_eq!(callbacks.event_ready.function, "event_ready");
    assert_eq!(callbacks.keep_alive.function, "keep_alive");
    assert_eq!(callbacks.drain.function, "drained");
    assert_eq!(callbacks.cancellation.function, "cancelled");
    assert_eq!(
        sse.clone()
            .with_callbacks(callbacks.clone())
            .expect_err("a second callback set must fail"),
        crate::runtime::vm::sse::VmSseError::CallbacksAlreadyConfigured
    );
}
