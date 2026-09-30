//! Tests for closure-free AOT router-plan extraction.

use std::collections::HashMap;

use crate::terlan_hir::{
    resolve_syntax_module_output, resolve_syntax_module_output_with_interfaces,
};
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
    assert_eq!(websocket.max_pending_frames(), 4);
    assert_eq!(websocket.max_frame_bytes(), 1024);
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

/// Verifies paired endpoints retain source-owned payloads and callback identities.
#[test]
fn aot_router_plan_materializes_websocket_pairing() {
    let source = r#"module app.PairedSocket.

import std.core.Unit.
import std.http.{Router, WebSocket}.
import type std.http.Router.
pub inbound(_frame: String): String -> "update".
pub cancelled(_reason: String): Unit -> Unit.
pub router(): Router ->
    Router.new().websocket(
        "/paired",
        WebSocket.endpoint(4, 1024).paired_callbacks(
            "waiting", "first", "second", "left", inbound, cancelled
        )
    ).
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse paired router fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    let (_, plan) = prepare_aot_router_module(&core).expect("extract paired router plan");
    let plan = plan.expect("router plan");
    let AotRouterRouteTarget::WebSocket(websocket) = &plan.routes[0].target else {
        panic!("expected WebSocket target")
    };
    let pairing = websocket.pairing().expect("pairing plan");
    assert_eq!(pairing.waiting, "waiting");
    assert_eq!(pairing.first_matched, "first");
    assert_eq!(pairing.second_matched, "second");
    assert_eq!(pairing.peer_left, "left");
    assert!(!pairing.stateful);
    assert_eq!(pairing.inbound.function, "inbound");
    assert_eq!(pairing.cancellation.function, "cancelled");
}

/// Verifies stateful paired endpoints retain the five-argument transition callback.
#[test]
fn aot_router_plan_materializes_stateful_websocket_pairing() {
    let source = r#"module app.StatefulSocket.

import std.core.Unit.
import std.http.{Router, WebSocket}.
import type std.http.Router.
pub inbound(
    state: String,
    _role: Int,
    _frame: String,
    _first: String,
    _second: String,
): {String, String, String} -> {state, "first update", "second update"}.
pub cancelled(_reason: String): Unit -> Unit.
pub router(): Router ->
    Router.new().websocket(
        "/paired",
        WebSocket.endpoint(4, 1024).stateful_paired_callbacks(
            "waiting", "first", "second", "left", inbound, cancelled
        )
    ).
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse stateful router fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    let (_, plan) = prepare_aot_router_module(&core).expect("extract stateful router plan");
    let plan = plan.expect("router plan");
    let AotRouterRouteTarget::WebSocket(websocket) = &plan.routes[0].target else {
        panic!("expected WebSocket target")
    };
    let pairing = websocket.pairing().expect("pairing plan");
    assert!(pairing.stateful);
    assert_eq!(pairing.inbound.arity, 5);
}

/// Verifies restorable endpoints retain source-owned identity and entry callbacks.
#[test]
fn aot_router_plan_materializes_restorable_websocket_pairing() {
    let source = r#"module app.RestorableSocket.

import std.core.Unit.
import std.http.{Router, WebSocket}.
import type std.http.Router.
pub waiting(): String -> "waiting".
pub peer_left(): String -> "left".
pub matched(room: String, _role: Int, _first: String, _second: String): String -> room.
pub restored(room: String, _state: String, _role: Int, _first: String, _second: String): String -> room.
pub inbound(state: String, _role: Int, _frame: String, _first: String, _second: String): {String, String, String} -> {state, "first", "second"}.
pub cancelled(_reason: String): Unit -> Unit.
pub router(): Router ->
    Router.new().websocket(
        "/paired",
        WebSocket.endpoint(4, 1024).restorable_stateful_paired_callbacks(
            waiting, peer_left, "room_id", "player_id", "room-",
            "player-1", "player-2", 300000, 1024,
            matched, restored, inbound, cancelled
        )
    ).
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse restorable router fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    let (_, plan) = prepare_aot_router_module(&core).expect("extract restorable router plan");
    let plan = plan.expect("router plan");
    let AotRouterRouteTarget::WebSocket(websocket) = &plan.routes[0].target else {
        panic!("expected WebSocket target")
    };
    let pairing = websocket.pairing().expect("pairing plan");
    let restoration = pairing.restoration.as_ref().expect("restoration plan");
    assert!(pairing.stateful);
    assert_eq!(restoration.room_query, "room_id");
    assert_eq!(restoration.player_query, "player_id");
    assert_eq!(restoration.room_prefix, "room-");
    assert_eq!(restoration.first_player, "player-1");
    assert_eq!(restoration.second_player, "player-2");
    assert_eq!(restoration.retention_ms, 300000);
    assert_eq!(restoration.retained_room_capacity, 1024);
    assert_eq!(restoration.waiting.function, "waiting");
    assert_eq!(restoration.waiting.arity, 0);
    assert_eq!(restoration.peer_left.function, "peer_left");
    assert_eq!(restoration.peer_left.arity, 0);
    assert_eq!(restoration.matched.arity, 4);
    assert_eq!(restoration.restored.arity, 5);
}

/// Verifies selected callback imports retain their provider identity.
#[test]
fn aot_router_plan_materializes_imported_websocket_callbacks() {
    let provider_source = r#"module app.SocketHandlers.

import std.core.Unit.

pub waiting(): String -> "waiting".
pub peer_left(): String -> "left".
pub matched(room: String, _role: Int, _first: String, _second: String): String -> room.
pub restored(room: String, _state: String, _role: Int, _first: String, _second: String): String -> room.
pub inbound(state: String, _role: Int, _frame: String, _first: String, _second: String): {String, String, String} -> {state, "first", "second"}.
pub cancelled(_reason: String): Unit -> Unit.
"#;
    let provider_syntax =
        parse_module_as_syntax_output(provider_source).expect("parse callback provider");
    let provider = resolve_syntax_module_output(&provider_syntax).module;
    let interfaces = HashMap::from([(provider.name.clone(), provider.interface.clone())]);
    let router_source = r#"module app.Socket.

import app.SocketHandlers.{cancelled, inbound, matched, peer_left, restored, waiting}.
import std.http.{Router, WebSocket}.
import type std.http.Router.

pub router(): Router ->
    Router.new().websocket(
        "/paired",
        WebSocket.endpoint(4, 1024).restorable_stateful_paired_callbacks(
            waiting, peer_left, "room_id", "player_id", "room-",
            "player-1", "player-2", 300000, 1024,
            matched, restored, inbound, cancelled
        )
    ).
"#;
    let router_syntax =
        parse_module_as_syntax_output(router_source).expect("parse imported callback router");
    let router = resolve_syntax_module_output_with_interfaces(&router_syntax, &interfaces).module;
    let core = lower_syntax_module_output_to_core(&router_syntax, &router);

    let (_, plan) = prepare_aot_router_module(&core).expect("extract imported callback router");
    let plan = plan.expect("router plan");
    let AotRouterRouteTarget::WebSocket(websocket) = &plan.routes[0].target else {
        panic!("expected WebSocket target")
    };
    let pairing = websocket.pairing().expect("pairing plan");
    let restoration = pairing.restoration.as_ref().expect("restoration plan");

    assert_eq!(restoration.waiting.module, "app.SocketHandlers");
    assert_eq!(restoration.waiting.function, "waiting");
    assert_eq!(restoration.waiting.arity, 0);
    assert_eq!(pairing.inbound.module, "app.SocketHandlers");
    assert_eq!(pairing.inbound.arity, 5);
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
