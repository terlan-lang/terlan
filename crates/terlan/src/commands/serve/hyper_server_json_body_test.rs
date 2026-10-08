//! Source-owned library calls through the production protocol and actor path.

use super::*;
use http_body_util::Full;
use terlan_http_native::request_ingress::{prepare_request, BodyStorage};

fn with_source_handler(source: &str, check: impl FnOnce(&dyn Fn(&str) -> String)) {
    with_source_requests(source, &["/json"], |send| {
        check(&|body| {
            send(&format!(
                "POST /json HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()
            ))
        });
    });
}

fn with_source_requests(
    source: &str,
    routes: &[&str],
    check: impl FnOnce(&dyn Fn(&str) -> String),
) {
    with_source_project(source, routes, |web| {
        with_source_protocol_server(web.to_path_buf(), check);
    });
}

fn with_source_project(source: &str, routes: &[&str], check: impl FnOnce(&Path)) {
    let handlers: Vec<_> = routes.iter().map(|route| (*route, "handle", 1)).collect();
    with_source_handler_project(source, &handlers, check);
}

#[test]
fn package_upload_lease_survives_source_projection_and_releases_after_dispatch() {
    use terlan_http_native::request_ingress::RequestBodyFile;

    with_source_project(
        r#"module app.Api.
import std.http.Response.
import type std.http.Request.Request.
import type std.http.Response.Response.
pub handle(request: Request): Response -> Response.text(request.body_file_path()).with_status(202).
"#,
        &["/json"],
        |web| {
            assert!(request_requires_file_body(web, "POST", "/json").unwrap());
            let root = tempfile::tempdir().unwrap();
            let request = Request::post("/json")
                .body(Full::new(Bytes::from_static(b"\0\xff")))
                .unwrap();
            let request =
                block_on(prepare_request(request, 2, BodyStorage::File(root.path()))).unwrap();
            let path = request
                .extensions()
                .get::<RequestBodyFile>()
                .unwrap()
                .path()
                .to_owned();
            assert_eq!(std::fs::read(&path).unwrap(), b"\0\xff");
            let mut channel = None;
            let response = handle_vm_stream_request(request, web, &mut channel, false).unwrap();
            assert_eq!(response.status(), 202);
            assert_eq!(response.body().as_ref(), path.as_bytes());
            assert!(channel.is_none());
            assert!(!Path::new(&path).exists());
            assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        },
    );
}

#[test]
fn package_body_admission_preserves_source_handlers_over_socket() {
    with_source_handler(
        r#"module app.Api.
import std.http.Response.
import type std.http.Request.Request.
import type std.http.Response.Response.
pub handle(request: Request): Response -> Response.text("source:" + request.body_text()).with_status(202).
"#,
        |send_body| {
            let response = send_body("hello");
            assert!(response.starts_with("HTTP/1.1 202 "), "{response}");
            assert!(response.ends_with("\r\n\r\nsource:hello"), "{response}");
            let response = send_body(&"x".repeat(4097));
            assert!(response.starts_with("HTTP/1.1 413 "), "{response}");
            assert!(
                response.ends_with("request body exceeds 4096 bytes"),
                "{response}"
            );
            let response = send_body("recovered");
            assert!(response.ends_with("\r\n\r\nsource:recovered"), "{response}");
        },
    );
}

#[test]
fn package_body_admission_counts_chunked_frames_before_source_execution() {
    with_source_requests(
        r#"module app.Api.
import std.http.Response.
import type std.http.Request.Request.
import type std.http.Response.Response.
pub handle(request: Request): Response -> Response.text("source:" + request.body_text()).with_status(202).
"#,
        &["/json"],
        |send| {
            for (chunks, status, suffix) in [
                (
                    "2\r\nhi\r\n1\r\n!\r\n0\r\n\r\n".to_string(),
                    202,
                    "source:hi!",
                ),
                (
                    format!("1000\r\n{}\r\n1\r\nx\r\n0\r\n\r\n", "a".repeat(4096)),
                    413,
                    "request body exceeds 4096 bytes",
                ),
                ("0\r\n\r\n".to_string(), 202, "source:"),
            ] {
                let response = send(&format!("POST /json HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n{chunks}"));
                assert!(
                    response.starts_with(&format!("HTTP/1.1 {status} ")),
                    "{response}"
                );
                assert!(
                    response.ends_with(&format!("\r\n\r\n{suffix}")),
                    "{response}"
                );
            }
        },
    );
}

#[test]
fn vm_stream_source_router_matches_typed_captures_and_group_fallbacks_over_socket() {
    with_source_requests(
        r#"module app.Api.
import std.core.Option.{Some, None}.
import std.http.{Router, Response}.
import std.http.Router.{Continue, Respond, MiddlewareResult}.
import type std.http.Router.Router.
import type std.http.Request.Request.
import type std.http.Response.Response.
pub handle(_request: Request): Response -> Response.text("manifest handler must not run").
pub inner_gate(request: Request): MiddlewareResult ->
    if {
        request.path().ends_with("/denied") -> Respond(Response.text("inner-denied").with_status(401));
        true -> Continue
    }.
pub outer_gate(request: Request): MiddlewareResult ->
    if {
        request.path().ends_with("/blocked") -> Respond(Response.text("outer-denied").with_status(403));
        true -> Continue
    }.
pub root_before(request: Request): MiddlewareResult ->
    if {
        request.path().ends_with("/root-blocked") -> Respond(Response.text("root-before-denied").with_status(402));
        true -> Continue
    }.
pub root_after(request: Request): MiddlewareResult ->
    if {
        request.path() == "/api/nested/blocked" or request.path().ends_with("/root-blocked") ->
            Respond(Response.text("root-after-denied").with_status(429));
        true -> Continue
    }.
pub captured(request: Request): Response ->
    case request.param("id") {
        Some(id) -> Response.text("typed:" + id);
        None -> Response.text("missing capture").with_status(500)
    }.
pub router(): Router ->
    Router.new()
        .use(root_before)
        .map_response((_request: Request, response: Response) -> response.with_header("X-Scope", "root-before"))
        .fallback((_request: Request) -> Response.text("root").with_status(404))
        .group("/api", (child: Router) ->
            child.use((_request: Request) -> Continue)
                .map_response((_request: Request, response: Response) -> response.with_header("X-Scope", "outer-before"))
                .fallback((_request: Request) -> Response.text("group").with_status(404))
                .post("/{id:Int}", captured)
                .post("/status", (_request: Request) -> Response.text("exact"))
                .group("/nested", (nested: Router) ->
                    nested.fallback((_request: Request) -> Response.text("nested").with_status(404))
                        .use(inner_gate)
                        .map_response((_request: Request, response: Response) -> response.with_header("X-Scope", "inner")))
                .use(outer_gate)
                .map_response((_request: Request, response: Response) -> response.with_header("X-Scope", "outer-after")))
        .use(root_after)
        .map_response((_request: Request, response: Response) ->
            response.with_header("X-Router", "source").with_header("X-Scope", "root")).
"#,
        &[
            "/api/{id:Int}",
            "/api/status",
            "/api/*",
            "/api/nested/*",
            "*",
        ],
        |send| {
            for (path, status, body) in [
                ("/api/%34%32", 200, "typed:42"),
                ("/api/-7", 200, "typed:-7"),
                ("/api/status", 200, "exact"),
                ("/api/not-an-int", 404, "group"),
                ("/api/9223372036854775808", 404, "group"),
                ("/api", 404, "group"),
                ("/api/deeper/missing", 404, "group"),
                ("/api/nested/missing", 404, "nested"),
                ("/api/nested/denied", 401, "inner-denied"),
                ("/api/blocked", 403, "outer-denied"),
                ("/api/nested/blocked", 429, "root-after-denied"),
                ("/api/nested/root-blocked", 402, "root-before-denied"),
                ("/root-blocked", 402, "root-before-denied"),
                ("/api2/missing", 404, "root"),
                ("/api/42", 200, "typed:42"),
            ] {
                let response = send(&format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"));
                assert!(
                    response.starts_with(&format!("HTTP/1.1 {status} ")),
                    "{path}: {response}"
                );
                assert!(
                    response.contains("x-router: source\r\n"),
                    "{path}: {response}"
                );
                let scope_headers: Vec<_> = response
                    .split("\r\n")
                    .filter_map(|line| line.strip_prefix("x-scope: "))
                    .collect();
                let expected = if path.starts_with("/api/nested/") {
                    vec![
                        "inner",
                        "outer-after",
                        "outer-before",
                        "root",
                        "root-before",
                    ]
                } else if path == "/api" || path.starts_with("/api/") {
                    vec!["outer-after", "outer-before", "root", "root-before"]
                } else {
                    vec!["root", "root-before"]
                };
                assert_eq!(scope_headers, expected, "{path}: {response}");
                assert!(
                    response.ends_with(&format!("\r\n\r\n{body}")),
                    "{path}: {response}"
                );
            }
        },
    );
}

#[test]
fn vm_stream_source_router_executes_captured_handler_over_socket() {
    with_source_handler(
        r#"module app.Api.
import std.http.{Router, Response}.
import std.http.Router.{Continue}.
import type std.http.Router.Router.
import type std.http.Request.Request.
import type std.http.Response.Response.
pub handle(_request: Request): Response -> Response.text("manifest fallback must not run").
route_name(): String -> "json".
pub router(): Router ->
    let prefix = "source:";
    Router.new()
        .use((_request: Request) -> Continue)
        .post("/" + route_name(), (request: Request) -> Response.text(prefix + request.body_text()))
        .map_response((_request: Request, response: Response) -> response.with_status(202)).
"#,
        |request| {
            for body in ["first", "second"] {
                let response = request(body);
                assert!(response.starts_with("HTTP/1.1 202 "), "{response}");
                assert!(
                    response.ends_with(&format!("\r\n\r\nsource:{body}")),
                    "{response}"
                );
            }
        },
    );
}

#[test]
fn vm_stream_source_router_middleware_awaits_package_worker_and_short_circuits() {
    with_source_handler(
        r#"module app.Api.
import std.core.Result.{Ok, Err}.
import std.net.Uri.
import std.http.{Router, Response, Error}.
import std.http.Router.{Continue, Respond, MiddlewareResult}.
import type std.http.Error.HttpError.
import type std.http.Router.Router.
import type std.http.Request.Request.
import type std.http.Response.Response.
pub handle(_request: Request): Response -> Response.text("manifest fallback must not run").
pub gate(request: Request): MiddlewareResult ->
    case Uri.parse(request.body_text()) {
        Ok(_) -> Continue;
        Err(_) -> Respond(Response.text("invalid URI").with_status(400))
    }.
pub router(): Router ->
    Router.new()
        .use(gate)
        .post("/json", (request: Request) -> Response.text("accepted:" + request.body_text()))
        .map_response((request: Request, response: Response) ->
            if {
                request.body_text() == "bad status" -> response.with_status(0);
                true -> response.with_header("X-Router", "source")
            })
        .error((reason: HttpError) ->
            if {
                Error.code(reason) == Atom["router_execution_failed"] and Error.status(reason) == 500 -> Response.text("recovered").with_status(503);
                true -> Response.text("invalid recovery").with_status(500)
            }).
"#,
        |request| {
            for (body, status, expected) in [
                (
                    "https://example.com/first",
                    200,
                    "accepted:https://example.com/first",
                ),
                ("not a URI", 400, "invalid URI"),
                ("bad status", 503, "recovered"),
                (
                    "https://example.com/second",
                    200,
                    "accepted:https://example.com/second",
                ),
            ] {
                let response = request(body);
                assert!(
                    response.starts_with(&format!("HTTP/1.1 {status} ")),
                    "{response}"
                );
                assert_eq!(
                    response.contains("x-router: source\r\n"),
                    status != 503,
                    "{response}"
                );
                assert!(
                    response.ends_with(&format!("\r\n\r\n{expected}")),
                    "{response}"
                );
            }
        },
    );
}

#[test]
fn vm_stream_package_value_binding_uses_default_worker() {
    with_source_handler(
        r#"module app.Api.
import std.core.Atom.
import std.core.Result.{Ok, Err}.
import std.net.Uri.
import std.http.Response.
import type std.http.Request.{Request}.
import type std.http.Response.{Response}.
pub handle(request: Request): Response ->
    case Uri.parse(request.body_text()) {
        Ok(uri) -> Response.text(uri.scheme() + "|" + uri.path());
        Err(reason) -> Response.text(Atom.to_string(reason.code)).with_status(400)
    }.
"#,
        |request| {
            for (body, status, expected) in [
                ("https://example.com/a?x=1", 200, "https|/a"),
                ("mailto:user@example.com", 200, "mailto|user@example.com"),
                ("not a URL", 400, "uri.parse"),
                ("https://example.com/recovered", 200, "https|/recovered"),
            ] {
                let response = request(body);
                assert!(
                    response.starts_with(&format!("HTTP/1.1 {status}")),
                    "{response}"
                );
                assert!(
                    response.ends_with(&format!("\r\n\r\n{expected}")),
                    "{response}"
                );
            }
        },
    );
}

#[test]
fn vm_stream_request_decodes_managed_json_body_result() {
    with_source_handler(
        r#"module app.Api.
import std.core.{Atom, Int}.
import std.core.Result.{Ok, Err}.
import std.data.Json.
import std.http.Response.
import type std.http.Request.{Request}.
import type std.http.Response.{Response}.
pub handle(request: Request): Response ->
    case request.body_json() {
        Ok(json) -> case json.get("count") {
            Ok(value) -> case value.as_int() {
                Ok(count) -> Response.text(Int.to_string(count + 1)).with_status(201);
                Err(_) -> Response.text("not an integer").with_status(422)
            };
            Err(_) -> Response.text("no count").with_status(422)
        };
        Err(reason) -> Response.text(Atom.to_string(reason.code) + ":" + reason.message).with_status(400)
    }.
"#,
        |request| {
            for (body, status, expected) in [
                (r#"{"count":2}"#, 201, "3"),
                (r#"{"count":-1}"#, 201, "0"),
                (r#"{"count":"2"}"#, 422, "not an integer"),
                ("null", 422, "no count"),
                ("[]", 422, "no count"),
            ] {
                let response = request(body);
                assert!(
                    response.starts_with(&format!("HTTP/1.1 {status}")),
                    "{response}"
                );
                assert!(
                    response.ends_with(&format!("\r\n\r\n{expected}")),
                    "{response}"
                );
            }
            for body in ["", "{", "{\"count\":1,}", "{} trailing", "\0", "[1,]"] {
                let response = request(body);
                assert!(response.starts_with("HTTP/1.1 400"), "{body:?}: {response}");
                let (_, error) = response.split_once("\r\n\r\n").expect("HTTP response body");
                assert!(error.starts_with("http.body_json:"), "{error}");
                assert!(
                    error.len() > "http.body_json:".len(),
                    "parser message retained"
                );
            }
            let recovered = request(r#"{"count":41}"#);
            assert!(recovered.starts_with("HTTP/1.1 201"), "{recovered}");
            assert!(recovered.ends_with("\r\n\r\n42"), "{recovered}");
            // More than the worker-owner limit proves completed requests retire resources.
            for count in 0..20 {
                let response = request(&format!("{{\"count\":{count}}}"));
                assert!(response.starts_with("HTTP/1.1 201"), "{response}");
                assert!(
                    response.ends_with(&format!("\r\n\r\n{}", count + 1)),
                    "{response}"
                );
            }
        },
    );
}

#[test]
fn vm_stream_resource_mutations_keep_alias_identity_in_default_worker() {
    with_source_handler(
        r#"module app.Api.
import std.data.Json.
import std.http.Response.
import type std.http.Request.{Request}.
import type std.http.Response.{Response}.
pub handle(request: Request): Response ->
    let values = Json.array();
    let alias = values;
    values.push(Json.int(1));
    values.push(values);
    Response.text(Json.to_string(alias)).with_status(201).
"#,
        |request| {
            for _ in 0..3 {
                let response = request("");
                assert!(response.starts_with("HTTP/1.1 201"), "{response}");
                assert!(response.ends_with("\r\n\r\n[1,[1]]"), "{response}");
            }
        },
    );
}
