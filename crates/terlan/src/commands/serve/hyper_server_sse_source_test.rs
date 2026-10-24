//! Source producers on the production HTTP body and TLS socket paths.
use super::*;

const SOURCE: &str = r#"module app.Api.
import std.core.Option.{Some, None}.
import std.http.{Request, Response, Router, Sse}.
import std.http.Router.{Continue, Respond, MiddlewareResult}.
import std.vm.Process.
import type std.core.Option.
import type std.http.Router.Router.
import type std.http.Sse.Event.
import type std.http.{Request, Response}.
pub next(): Option[Event] ->
    Process.sleep(Process.timer(1));
    Some(Sse.data("first\r\nsecond\rthird").with_id("7").with_name("update")).
pub end(): Option[Event] -> None.
pub invalid(): Option[Event] -> Some(Sse.data("payload").with_name("bad\nname")).
pub idle(): Unit -> Unit.
pub consume(_text: String): Unit -> Unit.
pub drained(): Unit -> Process.sleep(Process.timer(1)).
pub failed_drain(): Unit ->
    Process.sleep(Process.timer(1));
    let _invalid = Sse.response([Sse.data("never emitted").with_name("bad\nname")]);
    Unit.
pub gate(request: Request): MiddlewareResult ->
    Process.sleep(Process.timer(1));
    case request.param("id") {
        Some("42") -> Respond(Response.text("source denied", 403));
        Some("9") ->
            let _failed = failed_drain();
            Continue;
        Some(_) -> Continue;
        None -> Respond(Response.text("missing capture", 500))
    }.
pub decorate(request: Request, response: Response): Response ->
    Process.sleep(Process.timer(1));
    case request.param("id") {
        Some(id) -> response.with_header("X-Capture", id);
        None -> response.with_status(500)
    }.
pub router(): Router ->
    Router.new()
        .sse("/events", Sse.endpoint(1, 256).producer(next))
        .sse("/empty", Sse.endpoint(1, 256).producer(end))
        .sse("/invalid", Sse.endpoint(1, 256).producer(invalid))
        .sse("/oversized", Sse.endpoint(1, 1).producer(next))
        .sse("/heartbeat", Sse.endpoint_with_keep_alive().producer(next))
        .sse("/drain", Sse.endpoint().producer(end).callbacks(idle, consume, idle, drained, consume))
        .sse("/failed-drain", Sse.endpoint().producer(end).callbacks(idle, consume, idle, failed_drain, consume))
        .sse("/open", Sse.endpoint().producer(end).callbacks(drained, consume, idle, idle, consume))
        .sse("/failed-open", Sse.endpoint().producer(next).callbacks(failed_drain, consume, idle, idle, consume))
        .sse("/__terlan/reload", Sse.endpoint().producer(next).callbacks(failed_drain, consume, idle, idle, consume))
        .group("/guarded", (child: Router) ->
            child.use(gate).map_response(decorate).sse("/{id:Int}", Sse.endpoint().producer(end)))
        .group("/unsupported", (child: Router) ->
            child.use(gate).map_response(decorate).sse("/{id:Int}", Sse.endpoint_with_keep_alive().producer(end))).
"#;

fn with_project(check: impl FnOnce(&Path)) {
    let routes: Vec<_> = [
        "/events",
        "/empty",
        "/invalid",
        "/oversized",
        "/heartbeat",
        "/drain",
        "/failed-drain",
        "/open",
        "/failed-open",
        "/__terlan/reload",
        "/guarded/{id:Int}",
        "/unsupported/{id:Int}",
    ]
    .iter()
    .map(|route| {
        serde_json::json!({
            "route": route, "module": "app.Api",
            "source": {"path": "src/app/Api.terl", "line": 13, "column": 1}
        })
    })
    .collect();
    source_protocol::with_source_manifest_project(
        SOURCE,
        serde_json::json!({"sse": routes}),
        check,
    );
}

#[test]
fn source_sse_body_requires_protocol_owner_and_head_skips_production() {
    with_project(|web| {
        for route in ["/events", "/empty"] {
            let request = Request::get(route).body(Empty::<Bytes>::new()).unwrap();
            let response = block_on(handle_request(request, web, 4096, None));
            assert_eq!(response.status(), 200, "{route}");
            assert_eq!(response.headers()["cache-control"], "no-cache");
            assert_eq!(response.headers()["x-content-type-options"], "nosniff");
            assert!(!response.headers().contains_key("content-length"));
            assert!(!response.headers().contains_key("transfer-encoding"));
            let mut body = response.into_body();
            let frame = block_on(body.frame());
            if route == "/empty" {
                assert!(frame.is_none());
            } else {
                let error = frame.unwrap().unwrap_err().to_string();
                assert!(error.contains("capability_owner"), "{error}");
                assert!(block_on(body.frame()).is_none());
            }
        }
        for (method, path, status) in [
            ("HEAD", "/invalid", 200),
            ("HEAD", "/failed-open", 200),
            ("GET", "/heartbeat", 501),
        ] {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .body(Empty::<Bytes>::new())
                .unwrap();
            let response = block_on(handle_request(request, web, 4096, None));
            assert_eq!(response.status(), status);
            if method == "HEAD" {
                assert_eq!(response.headers()["cache-control"], "no-cache");
                assert_eq!(response.headers()["x-content-type-options"], "nosniff");
                assert!(block_on(response.into_body().collect())
                    .unwrap()
                    .to_bytes()
                    .is_empty());
            }
        }
    });
}

#[test]
fn source_sse_producer_emits_over_tls_and_disconnect_does_not_poison_next_request() {
    with_project(|web| {
        source_protocol::with_source_protocol_connections(web.to_path_buf(), |connect| {
            for _ in 0..3 {
                let mut stream = connect();
                stream
                    .write_all(
                        b"GET /events HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
                let mut wire = Vec::new();
                while !wire
                    .windows(b"data: third\n\n".len())
                    .any(|part| part == b"data: third\n\n")
                {
                    let mut chunk = [0; 512];
                    let n = stream.read(&mut chunk).unwrap();
                    assert_ne!(n, 0, "SSE closed before first event: {wire:?}");
                    wire.extend_from_slice(&chunk[..n]);
                    assert!(wire.len() < 4096, "unbounded event response");
                }
                let wire = String::from_utf8(wire).unwrap();
                assert!(wire.starts_with("HTTP/1.1 200 "), "{wire}");
                assert!(wire.contains("cache-control: no-cache\r\n"), "{wire}");
                assert!(
                    wire.contains("x-content-type-options: nosniff\r\n"),
                    "{wire}"
                );
                assert!(wire.contains("transfer-encoding: chunked\r\n"), "{wire}");
                assert!(!wire.contains("content-length:"), "{wire}");
                assert!(
                    wire.contains(
                        "id: 7\nevent: update\ndata: first\ndata: second\ndata: third\n\n"
                    ),
                    "{wire}"
                );
                drop(stream);
            }
            for path in ["/invalid", "/oversized"] {
                let mut stream = connect();
                stream
                    .write_all(
                        format!(
                            "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                let mut wire = String::new();
                // A body failure aborts the HTTP stream; TLS may surface it as EOF.
                if let Err(error) = stream.read_to_string(&mut wire) {
                    assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof, "{error}");
                }
                assert!(!wire.contains("data:"), "{path}: {wire}");
                assert!(
                    !wire.ends_with("0\r\n\r\n"),
                    "failed streams must not report graceful completion"
                );
            }
            let mut stream = connect();
            stream
                .write_all(b"GET /empty HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .unwrap();
            let mut wire = String::new();
            stream.read_to_string(&mut wire).unwrap();
            assert!(wire.starts_with("HTTP/1.1 200 "), "{wire}");
            assert!(wire.ends_with("0\r\n\r\n"), "{wire}");
        });
    });
}

#[test]
fn source_sse_open_and_drain_await_vm_timers_and_failures_abort_eof() {
    with_project(|web| {
        source_protocol::with_source_protocol_connections(web.to_path_buf(), |connect| {
            for path in [
                "/drain",
                "/failed-drain",
                "/drain",
                "/open",
                "/failed-open",
                "/open",
            ] {
                let mut stream = connect();
                stream
                    .write_all(
                        format!(
                            "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                let mut wire = String::new();
                let result = stream.read_to_string(&mut wire);
                if path.starts_with("/failed-") {
                    if let Err(error) = result {
                        assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof, "{error}");
                    }
                    assert!(!wire.ends_with("0\r\n\r\n"), "{wire}");
                    assert!(!wire.contains("never emitted"), "{wire}");
                    assert!(
                        !wire.contains("data:"),
                        "producer must not run after failed open: {wire}"
                    );
                } else {
                    result.unwrap();
                    assert!(wire.starts_with("HTTP/1.1 200 "), "{wire}");
                    assert!(wire.contains("transfer-encoding: chunked\r\n"), "{wire}");
                    assert!(wire.ends_with("0\r\n\r\n"), "{wire}");
                }
            }
        });
    });
}

#[test]
fn source_sse_async_middleware_preserves_captures_and_precedes_transport_admission() {
    with_project(|web| {
        source_protocol::with_source_protocol_server(web.to_path_buf(), |send| {
            for (method, path, status) in [
                ("GET", "/guarded/%34%32", 403),
                ("HEAD", "/guarded/42", 403),
                ("GET", "/unsupported/42", 403),
                ("GET", "/unsupported/7", 501),
                ("GET", "/guarded/9", 502),
                ("GET", "/guarded/7", 200),
                ("GET", "/guarded/42", 403),
                ("GET", "/__terlan/reload", 200),
                ("HEAD", "/__terlan/reload", 200),
            ] {
                let wire = send(&format!(
                    "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
                ));
                assert!(
                    wire.starts_with(&format!("HTTP/1.1 {status} ")),
                    "{path}: {wire}"
                );
                if status == 403 {
                    assert!(wire.contains("x-capture: 42\r\n"), "{wire}");
                    assert_eq!(wire.contains("source denied"), method == "GET", "{wire}");
                }
                if path == "/guarded/7" {
                    assert!(wire.ends_with("0\r\n\r\n"), "{wire}");
                }
                if path == "/__terlan/reload" {
                    assert_eq!(wire.contains(": connected\n\n"), method == "GET", "{wire}");
                }
            }
        });
    });
}
