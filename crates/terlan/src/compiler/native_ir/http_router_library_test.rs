//! Router declarations must be real source values, not unchanged placeholders.

use super::source_constructor_test::check_sources;

const ROUTER: &str = include_str!("../../../../../std/http/Router.terl");

#[test]
fn middleware_execution_order_and_short_circuit_are_source_owned() {
    let body = r#"
import std.http.Response.
import std.http.Request.
proceed(_request: Request): MiddlewareResult -> Continue.
stop(_request: Request): MiddlewareResult -> Respond(Response.text("first")).
later(_request: Request): MiddlewareResult -> Respond(Response.text("later")).
outer(_request: Request, response: Response): Response ->
    if { response == Response.text("middle") -> Response.text("done"); true -> Response.text("wrong") }.
inner(_request: Request, response: Response): Response ->
    if { response == Response.text("start") -> Response.text("middle"); true -> Response.text("wrong") }.
pub check(): Bool ->
    let request = Request.make("/pipeline");
    let before = request_pipeline([proceed, stop, later]);
    let post = response_pipeline([outer, inner]);
    let requested = case before.#execute {
        [callback] -> case callback(request) {
            Respond(response) -> response == Response.text("first");
            Continue -> false
        };
        _ -> false
    };
    let responded = case post.#execute {
        [callback] -> callback(request, Response.text("start")) == Response.text("done");
        _ -> false
    };
    requested and responded
        and request_pipeline([]).#execute == [] and response_pipeline([]).#execute == []
        and run_request_pipeline([proceed, proceed], request) == Continue.
"#;
    let verify = |provider: &str, body: &str, owner: &str| {
        check_sources(&[
            &format!("{provider}\n{body}").replace("std.http.Router", owner),
            "module std.http.Request. pub struct Request {path: String}. pub make(path: String): Request -> Request {path: path}.",
            include_str!("../../../../../std/http/Response.terl"),
            include_str!("../../../../../std/http/Error.terl"),
            include_str!("../../../../../std/http/Sse.terl"),
            include_str!("../../../../../std/http/WebSocket.terl"),
        ]);
    };
    for owner in ["std.http.Router", "app.Pipelines"] {
        verify(ROUTER, body, owner);
        let changed = ROUTER.replace(
            "callback(request, run_response_pipeline(rest, request, response))",
            "run_response_pipeline(rest, request, callback(request, response))",
        );
        assert_ne!(changed, ROUTER);
        verify(
            &changed,
            &body.replace("== Response.text(\"done\")", "== Response.text(\"wrong\")"),
            owner,
        );
    }
}

fn check_router(body: &str, owner: &str) {
    check_router_provider(ROUTER, body, owner);
}

fn check_router_provider(provider: &str, body: &str, owner: &str) {
    let source = format!("{provider}\nimport std.http.{{Response, Sse, WebSocket}}.\n{body}")
        .replace("std.http.Router", owner);
    check_sources(&[
        &source,
        include_str!("../../../../../std/http/Response.terl"),
        include_str!("../../../../../std/http/Request.terl"),
        include_str!("../../../../../std/http/Sse.terl"),
        include_str!("../../../../../std/http/WebSocket.terl"),
        include_str!("../../../../../std/http/Error.terl"),
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
    let first_unchanged = case first.#entries {
        [RouteEntry("GET", "/one", _, before, post)] -> before.#execute == [] and post.#execute == [];
        _ -> false
    };
    let ordered = case all.#entries {
        [RouteEntry("GET", "/one", _, _, _), RouteEntry("POST", "/two", _, _, _),
         RouteEntry("PUT", "/three", _, _, _), RouteEntry("PATCH", "/four", _, _, _),
         RouteEntry("DELETE", "/five", _, _, _), RouteEntry("HEAD", "/six", _, _, _),
         RouteEntry("OPTIONS", "/seven", _, _, _)] -> true;
        _ -> false
    };
    unchanged and first_unchanged and ordered.
"#;
    check_router(body, "std.http.Router");
    check_router(body, "app.Routes");
    let changed = ROUTER.replace(
        "RouteEntry(\"GET\", pattern, handler,",
        "RouteEntry(\"SOURCE\", pattern, handler,",
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
    let body = r#"
handler(_request: Request): Response -> Response.text("source").
leaf(scoped: Router): Router -> scoped.get("/leaf", handler).
configure(scoped: Router): Router -> scoped.get("/child", handler).group("/inner", leaf).
pub check(): Bool ->
    let router = new().group("/prefix", configure);
    case router.#entries {
        [GroupStartEntry, RouteEntry("GET", "/prefix/child", _, _, _),
         GroupStartEntry, RouteEntry("GET", "/prefix/inner/leaf", _, _, _),
         GroupEndEntry, GroupEndEntry] -> true;
        _ -> false
    }.
"#;
    check_router(body, "std.http.Router");
    check_router(body, "app.Routes");
    let changed = ROUTER.replace(
        "base + \"/\" + trim_group_start(path)",
        "base + \"/changed/\" + trim_group_start(path)",
    );
    assert_ne!(changed, ROUTER);
    check_router_provider(
        &changed,
        &body
            .replace("\"/prefix/child\"", "\"/prefix/changed/child\"")
            .replace(
                "\"/prefix/inner/leaf\"",
                "\"/prefix/changed/inner/changed/leaf\"",
            ),
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
        include_str!("../../../../../std/http/Error.terl"),
        include_str!("../../../../../std/http/Response.terl"),
        include_str!("../../../../../std/http/Request.terl"),
        include_str!("../../../../../std/http/Sse.terl"),
        include_str!("../../../../../std/http/WebSocket.terl"),
    ]);
}

#[test]
fn router_group_middleware_is_composed_in_source_including_late_declarations() {
    let body = r#"
handler(_request: Request): Response -> Response.text("source").
middleware(_request: Request): MiddlewareResult -> Continue.
response_middleware(_request: Request, response: Response): Response -> response.
scope_sizes(entries: List[Entry]): List[{Int, Int}] ->
    case entries {
        [] -> [];
        [RouteEntry(_, _, _, middleware, response) | rest] ->
            [{List.length(middleware.#callbacks), List.length(response.#callbacks)} | scope_sizes(rest)];
        [SseEntry(_, _, middleware, response) | rest] ->
            [{List.length(middleware.#callbacks), List.length(response.#callbacks)} | scope_sizes(rest)];
        [WebSocketEntry(_, _, middleware, response) | rest] ->
            [{List.length(middleware.#callbacks), List.length(response.#callbacks)} | scope_sizes(rest)];
        [FallbackEntry(_, middleware, response) | rest] ->
            [{List.length(middleware.#callbacks), List.length(response.#callbacks)} | scope_sizes(rest)];
        [_ | rest] -> scope_sizes(rest)
    }.
pub check(): Bool ->
    let original = new().use(middleware).map_response(response_middleware).get("/outside", handler);
    let router = original.group("/api", (outer: Router) ->
        outer.use(middleware).map_response(response_middleware)
            .group("/inner", (inner: Router) ->
                inner.get("/one", handler)
                    .sse("/events", std.http.Sse.endpoint(2, 256))
                    .websocket("/socket", std.http.WebSocket.endpoint(2, 256))
                    .use(middleware).map_response(response_middleware))
            .get("/two", handler)
            .use(middleware).map_response(response_middleware))
        .fallback(handler).use(middleware).map_response(response_middleware).get("/after", handler);
    scope_sizes(original.#entries) == [{1, 1}]
        and scope_sizes(router.#entries) == [{2, 2}, {5, 5}, {5, 5}, {5, 5}, {4, 4}, {2, 2}, {2, 2}].
"#;
    check_router(body, "std.http.Router");
    check_router(body, "app.Routes");
    let changed = ROUTER.replace(
        "insert_callbacks(scoped.#callbacks, middleware_offset, middleware)",
        "scoped.#callbacks",
    );
    assert_ne!(changed, ROUTER);
    check_router_provider(
        &changed,
        &body
            .replace("{1, 1}", "{0, 1}")
            .replace("{2, 2}", "{0, 2}")
            .replace("{4, 4}", "{0, 4}")
            .replace("{5, 5}", "{0, 5}"),
        "std.http.Router",
    );
}

#[test]
fn router_group_error_inheritance_is_source_owned() {
    let body = r#"
recover(_error: HttpError): Response -> Response.text("child", 501).
parent_recover(_error: HttpError): Response -> Response.text("parent", 502).
callback_response(callback: (String) -> Response): Response -> callback("test").
pub check(): Bool ->
    let original = new();
    let inherited = original.group("/outer", (outer: Router) ->
        outer.group("/inner", (inner: Router) -> inner.error(recover)));
    let promoted = case inherited.#entries {
        [GroupStartEntry, GroupStartEntry, ErrorEntry(_), GroupEndEntry,
         ErrorEntry(_), GroupEndEntry, ErrorEntry(callback)] -> callback_response(callback) == Response.text("child", 501);
        _ -> false
    };
    let parent = new().error(parent_recover).group("/child", (child: Router) -> child.error(recover));
    let preserved = case parent.#entries {
        [ErrorEntry(callback), GroupStartEntry, ErrorEntry(_), GroupEndEntry] -> callback_response(callback) == Response.text("parent", 502);
        _ -> false
    };
    let siblings = new().group("/first", (child: Router) -> child.error(recover))
        .group("/second", (child: Router) -> child.error(parent_recover));
    let first_wins = case siblings.#entries {
        [GroupStartEntry, ErrorEntry(_), GroupEndEntry, ErrorEntry(callback),
         GroupStartEntry, ErrorEntry(_), GroupEndEntry] -> callback_response(callback) == Response.text("child", 501);
        _ -> false
    };
    let unchanged = case original.#entries { [] -> true; _ -> false };
    promoted and preserved and first_wins and unchanged.
"#;
    check_router(body, "std.http.Router");
    check_router(body, "app.Routes");
    let changed = ROUTER
        .replace("let inherited =", "let _inherited =")
        .replace("List.concat(closed.#entries, inherited)", "closed.#entries");
    assert_ne!(changed, ROUTER);
    check_router_provider(
        &changed,
        r#"
recover(_error: HttpError): Response -> Response.text("child").
pub check(): Bool ->
    let router = new().group("/child", (child: Router) -> child.error(recover));
    case router.#entries {
        [GroupStartEntry, ErrorEntry(_), GroupEndEntry] -> true;
        _ -> false
    }.
"#,
        "std.http.Router",
    );
}

#[test]
fn router_recovery_error_shape_is_source_owned() {
    let body = r#"
recover(error: HttpError): Response ->
    if {
        Error.code(error) == Atom["router_execution_failed"] and Error.status(error) == 500 -> Response.text(Error.message(error), 503);
        true -> Response.text("wrong error", 400)
    }.
pub check(): Bool ->
    let router = new().error(recover);
    case router.#entries {
        [ErrorEntry(callback)] -> callback("host failure") == Response.text("host failure", 503);
        _ -> false
    }.
"#;
    check_router(body, "std.http.Router");
    check_router(body, "app.Recovery");
    let changed = ROUTER.replace(
        "Atom[\"router_execution_failed\"], message, 500",
        "Atom[\"package_failure\"], message, 502",
    );
    assert_ne!(changed, ROUTER);
    check_router_provider(
        &changed,
        &body
            .replace("router_execution_failed", "package_failure")
            .replace("== 500", "== 502"),
        "app.Recovery",
    );
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
        [MiddlewareEntry(_), ResponseMiddlewareEntry(_), FallbackEntry(_, before, post), ErrorEntry(_),
         OverloadEntry(Atom["reject"], 41), LifecycleEntry(_), SseEntry("/events", sse, sb, sa),
         WebSocketEntry("/socket", ws, wb, wa)] ->
            sse.max_pending_events == 7 and sse.max_event_bytes == 256
                and ws.max_pending_frames == 9 and ws.max_frame_bytes == 512
                and List.length(before.#execute) == 1 and List.length(post.#execute) == 1
                and List.length(sb.#execute) == 1 and List.length(sa.#execute) == 1
                and List.length(wb.#execute) == 1 and List.length(wa.#execute) == 1;
        _ -> false
    }.
"#,
        "std.http.Router",
    );
}
