//! Router declarations must be real source values, not unchanged placeholders.

use super::source_constructor_test::check_sources;

const ROUTER: &str = include_str!("../../../../../std/http/Router.terl");

fn check_router(body: &str, owner: &str) {
    check_router_provider(ROUTER, body, owner);
}

fn check_router_provider(provider: &str, body: &str, owner: &str) {
    let source = format!("{provider}\n{body}")
        .replace("std.http.Router", owner)
        .replace(
            "import type std.http.Error.HttpError.",
            "import std.http.{Response, Sse, WebSocket}.\nimport type std.http.Error.HttpError.",
        );
    check_sources(&[
        &source,
        include_str!("../../../../../std/http/Response.terl"),
        include_str!("../../../../../std/http/Request.terl"),
        include_str!("../../../../../std/http/Sse.terl"),
        include_str!("../../../../../std/http/WebSocket.terl"),
    ]);
}

#[test]
fn router_source_preserves_http_methods_order_and_earlier_values() {
    let body = r#"
handler(_request: Request): Response -> Response.text("source").
pub check(): Bool ->
    let empty = new();
    let first = get(empty, "/one", handler);
    let all = first.post("/two", handler).put("/three", handler)
        .patch("/four", handler).delete("/five", handler)
        .head("/six", handler).options("/seven", handler);
    let unchanged = case empty.#entries { [] -> true; _ -> false };
    let first_unchanged = case first.#entries { [RouteEntry("GET", "/one", _)] -> true; _ -> false };
    let ordered = case all.#entries {
        [RouteEntry("GET", "/one", _), RouteEntry("POST", "/two", _),
         RouteEntry("PUT", "/three", _), RouteEntry("PATCH", "/four", _),
         RouteEntry("DELETE", "/five", _), RouteEntry("HEAD", "/six", _),
         RouteEntry("OPTIONS", "/seven", _)] -> true;
        _ -> false
    };
    unchanged and first_unchanged and ordered.
"#;
    check_router(body, "std.http.Router");
    check_router(body, "app.Routes");
    let changed = ROUTER.replace(
        "RouteEntry(\"GET\", pattern, handler)",
        "RouteEntry(\"SOURCE\", pattern, handler)",
    );
    assert_ne!(changed, ROUTER);
    check_router_provider(
        &changed,
        &body.replace("\"GET\"", "\"SOURCE\""),
        "std.http.Router",
    );
}

#[test]
fn router_source_evaluates_group_callback_and_retains_scope() {
    check_router(
        r#"
handler(_request: Request): Response -> Response.text("source").
leaf(scoped: Router): Router -> scoped.get("/leaf", handler).
configure(scoped: Router): Router -> scoped.get("/child", handler).group("/inner", leaf).
pub check(): Bool ->
    let router = new().group("/prefix", configure);
    case router.#entries {
        [GroupStartEntry("/prefix"), RouteEntry("GET", "/child", _),
         GroupStartEntry("/inner"), RouteEntry("GET", "/leaf", _),
         GroupEndEntry, GroupEndEntry] -> true;
        _ -> false
    }.
"#,
        "std.http.Router",
    );
}

#[test]
fn standard_router_test_executes_real_builders_and_imported_helpers() {
    let root = format!(
        "{}\npub check(): Bool -> router_builder_records_route_group_fallback_and_error_steps().",
        include_str!("../../../../../std/http/RouterTest.terl")
    );
    check_sources(&[
        &root,
        ROUTER,
        include_str!("../../../../../std/http/Response.terl"),
        include_str!("../../../../../std/http/Request.terl"),
        include_str!("../../../../../std/http/Sse.terl"),
        include_str!("../../../../../std/http/WebSocket.terl"),
    ]);
}

#[test]
fn router_source_retains_channel_middleware_and_admission_declarations() {
    check_router(
        r#"
handler(_request: Request): Response -> Response.text("source").
middleware(_request: Request): MiddlewareResult -> Continue.
response_middleware(_request: Request, response: Response): Response -> response.
error_handler(_error: HttpError): Response -> Response.text("error").
lifecycle_middleware(_event: LifecycleEvent): LifecycleDecision -> LifecycleAllow.
pub check(): Bool ->
    let router = new().use(middleware).map_response(response_middleware)
        .fallback(handler).error(error_handler).overload(Atom["reject"], 41)
        .lifecycle(lifecycle_middleware)
        .sse("/events", std.http.Sse.endpoint(7, 256))
        .websocket("/socket", std.http.WebSocket.endpoint(9, 512));
    case router.#entries {
        [MiddlewareEntry(_), ResponseMiddlewareEntry(_), FallbackEntry(_), ErrorEntry(_),
         OverloadEntry(Atom["reject"], 41), LifecycleEntry(_), SseEntry("/events", sse),
         WebSocketEntry("/socket", ws)] ->
            sse.max_pending_events == 7 and sse.max_event_bytes == 256
                and ws.max_pending_frames == 9 and ws.max_frame_bytes == 512;
        _ -> false
    }.
"#,
        "std.http.Router",
    );
}
