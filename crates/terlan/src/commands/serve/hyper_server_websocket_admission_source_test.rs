//! Source authorization suspends before production WebSocket resource admission.

use super::*;
use tungstenite::{client, HandshakeError, Message};

const SOURCE: &str = r#"module app.Api.
import std.core.Option.{Some, None}.
import std.http.{Request, Response, Router, Sse, WebSocket}.
import std.http.Router.{Continue, Respond, MiddlewareResult}.
import std.vm.Process.
import std.io.File.
import type std.http.{Request, Response}.
import type std.http.Router.Router.
pub cancelled(_reason: String): Unit -> Unit.
pub opened(): Unit -> Process.sleep(Process.timer(1)).
pub failed_open(): Unit ->
    Process.sleep(Process.timer(1));
    let _invalid = Sse.response([Sse.data("not emitted").with_name("bad\nname")]);
    Unit.
pub received(_frame: String): Unit ->
    Process.sleep(Process.timer(1));
    let _exists = File.exists(".");
    let wake = Process.receive_string();
    Process.sleep(Process.timer(1));
    if {
        wake == "second" -> Unit;
        true -> let _invalid = Sse.response([Sse.data("never").with_name("bad\nname")]); Unit
    }.
pub finished(): Unit -> Process.sleep(Process.timer(1)).
pub closed(): Unit -> Process.sleep(Process.timer(1)).
pub waiting(): String -> Process.sleep(Process.timer(1)); "source waiting".
pub peer_left(): String -> Process.sleep(Process.timer(1)); "source left".
pub gate(request: Request): MiddlewareResult ->
    Process.sleep(Process.timer(1));
    case request.query("mode") {
        Some("deny") -> Respond(Response.text("source denied", 403));
        Some("fail") ->
            let _invalid = Sse.response([Sse.data("never emitted").with_name("bad\nname")]);
            Continue;
        Some("allow") -> Continue;
        _ -> Respond(Response.text("authorization required", 401))
    }.
pub decorate(request: Request, response: Response): Response ->
    Process.sleep(Process.timer(1));
    case request.query("mode") {
        Some(mode) -> response.with_header("X-Source", mode);
        None -> response.with_header("X-Source", "missing")
    }.
pub router(): Router ->
    Router.new().use(gate).map_response(decorate)
        .websocket("/socket", WebSocket.endpoint(8, 1024).paired_callbacks(
            "waiting", "first", "second", "left",
            (frame: String) -> Process.sleep(Process.timer(1)); "source:" + frame, cancelled))
        .websocket("/opening", WebSocket.endpoint(8, 1024).callbacks(
            opened, received, finished, closed, cancelled))
        .websocket("/restorable", WebSocket.endpoint(8, 1024).restorable_stateful_paired_callbacks(
            waiting, peer_left, "room", "player", "room-", "first", "second", 1000, 4,
            (room: String, role: Int, first: String, second: String) ->
                Process.sleep(Process.timer(1)); "source matched:" + room,
            (room: String, state: String, role: Int, first: String, second: String) ->
                Process.sleep(Process.timer(1)); "source restored:" + state,
            (state: String, role: Int, frame: String, first: String, second: String) ->
                Process.sleep(Process.timer(1)); {frame, frame, frame},
            cancelled))
        .websocket("/failed-opening", WebSocket.endpoint(8, 1024).callbacks(
            failed_open, received, finished, closed, cancelled)).
"#;

#[test]
fn source_websocket_async_authorization_controls_upgrade_and_preserves_later_connections() {
    source_protocol::with_source_manifest_project(
        SOURCE,
        serde_json::json!({"websockets": [{
            "module": "app.Api", "route": "/socket", "protocol": "source.v1",
            "source": {"path": "src/app/Api.terl", "line": 27, "column": 1}
        }, {
            "module": "app.Api", "route": "/opening", "protocol": "source.v1",
            "source": {"path": "src/app/Api.terl", "line": 1, "column": 1}
        }, {
            "module": "app.Api", "route": "/restorable", "protocol": "source.v1",
            "source": {"path": "src/app/Api.terl", "line": 1, "column": 1}
        }, {
            "module": "app.Api", "route": "/failed-opening", "protocol": "source.v1",
            "source": {"path": "src/app/Api.terl", "line": 1, "column": 1}
        }]}),
        |web| {
            source_protocol::with_source_protocol_connections(web.to_path_buf(), |connect| {
                for (query, status, message) in [
                    ("?mode=deny", 403, "source denied"),
                    ("", 401, "authorization required"),
                    ("?mode=fail", 502, ""),
                    ("?mode=deny", 403, "source denied"),
                ] {
                    let Err(HandshakeError::Failure(tungstenite::Error::Http(response))) =
                        client(format!("wss://localhost/socket{query}"), connect())
                    else {
                        panic!("source denial must not upgrade");
                    };
                    assert_eq!(response.status(), status, "{response:?}");
                    let body =
                        String::from_utf8(response.body().as_ref().unwrap().clone()).unwrap();
                    assert!(body.contains(message), "{body}");
                    assert!(!body.contains("waiting"), "{body}");
                    if status != 502 {
                        assert_eq!(
                            response.headers()["x-source"],
                            if query.is_empty() { "missing" } else { "deny" }
                        );
                    }
                }
                for _ in 0..2 {
                    let (mut socket, response) =
                        client("wss://localhost/socket?mode=allow", connect()).unwrap();
                    assert_eq!(response.status(), 101);
                    assert_eq!(socket.read().unwrap(), Message::text("waiting"));
                    socket.send(Message::text("hello")).unwrap();
                    assert_eq!(socket.read().unwrap(), Message::text("source:hello"));
                    socket.close(None).unwrap();
                    assert_eq!(socket.read().unwrap(), Message::Close(None));
                }
                for _ in 0..2 {
                    let Err(HandshakeError::Failure(tungstenite::Error::Http(response))) =
                        client("wss://localhost/failed-opening?mode=allow", connect())
                    else {
                        panic!("failed source initialization must not upgrade");
                    };
                    assert_eq!(response.status(), 502);
                    let body =
                        String::from_utf8(response.body().as_ref().unwrap().clone()).unwrap();
                    assert!(body.contains("Open callback failed"), "{body}");
                    let (mut socket, response) =
                        client("wss://localhost/opening?mode=allow", connect()).unwrap();
                    assert_eq!(response.status(), 101);
                    socket.send(Message::text("first")).unwrap();
                    socket.send(Message::text("second")).unwrap();
                    socket.send(Message::Ping(vec![1, 2].into())).unwrap();
                    assert_eq!(socket.read().unwrap(), Message::Pong(vec![1, 2].into()));
                    socket.close(None).unwrap();
                    assert_eq!(socket.read().unwrap(), Message::Close(None));
                }
                let (mut first, response) =
                    client("wss://localhost/restorable?mode=allow", connect()).unwrap();
                assert_eq!(response.status(), 101);
                assert_eq!(first.read().unwrap(), Message::text("source waiting"));
                let (mut second, response) =
                    client("wss://localhost/restorable?mode=allow", connect()).unwrap();
                assert_eq!(response.status(), 101);
                assert_eq!(
                    first.read().unwrap(),
                    Message::text("source matched:room-1")
                );
                assert_eq!(
                    second.read().unwrap(),
                    Message::text("source matched:room-1")
                );
                second.send(Message::text("source frame")).unwrap();
                assert_eq!(first.read().unwrap(), Message::text("source frame"));
                assert_eq!(second.read().unwrap(), Message::text("source frame"));
                second.close(None).unwrap();
                assert_eq!(second.read().unwrap(), Message::Close(None));
                assert_eq!(first.read().unwrap(), Message::text("source left"));
                let (mut restored, response) = client(
                    "wss://localhost/restorable?mode=allow&room=room-1&player=second",
                    connect(),
                )
                .unwrap();
                assert_eq!(response.status(), 101);
                assert_eq!(
                    restored.read().unwrap(),
                    Message::text("source restored:source frame")
                );
                restored.send(Message::text("after restore")).unwrap();
                assert_eq!(first.read().unwrap(), Message::text("after restore"));
                assert_eq!(restored.read().unwrap(), Message::text("after restore"));
                restored.close(None).unwrap();
                assert_eq!(restored.read().unwrap(), Message::Close(None));
                assert_eq!(first.read().unwrap(), Message::text("source left"));
                first.close(None).unwrap();
                assert_eq!(first.read().unwrap(), Message::Close(None));
                // These requests would return 502 if the failing source gate ran first.
                for (method, extra, status) in [
                    ("POST", "", 405),
                    ("GET", "", 426),
                    (
                        "GET",
                        "Upgrade: websocket\r\nSec-WebSocket-Key: bad\r\n",
                        400,
                    ),
                ] {
                    let mut stream = connect();
                    stream.write_all(format!("{method} /socket?mode=fail HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{extra}\r\n").as_bytes()).unwrap();
                    let mut wire = String::new();
                    let result = stream.read_to_string(&mut wire);
                    assert!(
                        result.is_ok(),
                        "{method} expected {status}: {result:?}; wire: {wire}"
                    );
                    assert!(wire.starts_with(&format!("HTTP/1.1 {status} ")), "{wire}");
                }
            });
        },
    );
}
