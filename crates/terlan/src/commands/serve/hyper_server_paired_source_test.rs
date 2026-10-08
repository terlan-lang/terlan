//! Source pairing policy through the production TLS, upgrade, and VM callback path.

use super::*;
use crate::support::test_fs::TestDirectory;
use tungstenite::{client, Error, Message};

const SOURCE: &str = r#"module app.Pairing.
import std.core.Unit.
import std.http.{Router, WebSocket}.
import type std.http.Router.Router.
pub cancelled(_reason: String): Unit -> Unit.
pub router(): Router ->
    let prefix = "source:";
    Router.new()
        .websocket("/pair", WebSocket.endpoint(8, 1024).paired_callbacks(
            "waiting", "first", "second", "left",
            (frame: String) -> case frame { "" -> ""; _ -> prefix + frame }, cancelled))
        .websocket("/stateful", WebSocket.endpoint(8, 1024).stateful_paired_callbacks(
            "waiting", "first", "second", "left",
            (state: String, role: Int, frame: String, first: String, second: String) ->
                {state + frame, state + frame, ""}, cancelled)).
"#;

#[test]
fn source_pairing_policies_deliver_over_tls_and_survive_peer_disconnect() {
    let root = TestDirectory::new("serve", "paired_source_socket");
    let web = root.join("_build/web");
    std::fs::create_dir_all(root.join("src/app")).unwrap();
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"paired_source\"\nversion = \"0.0.9\"\nnamespace = \"app\"\n",
    )
    .unwrap();
    std::fs::write(root.join("src/app/Pairing.terl"), SOURCE).unwrap();
    std::fs::write(web.join("index.html"), "").unwrap();
    std::fs::write(
        web.join("manifest.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": "terlan-web-build-v1", "target_profile": "js.browser",
            "source_js_manifest": "../js/manifest.json", "index": "index.html", "assets": [],
            "websockets": (["/pair", "/stateful"].map(|route| serde_json::json!({
                "module": "app.Pairing", "route": route, "protocol": "pair.v1",
                "source": {"path": "src/app/Pairing.terl", "line": 6, "column": 1}
            })))
        }))
        .unwrap(),
    )
    .unwrap();
    crate::commands::serve::prewarm_dynamic_handler_sources(&web).unwrap();
    let (server_config, client_config) = tls_pair_with_client_alpn(b"http/1.1");
    let listener =
        crate::runtime::vm::protocol_task_executor::bind_protocol_listener("127.0.0.1", 0).unwrap();
    let mut server = start_protocol_tasks_with_topology(
        listener,
        tls_factory(web, server_config, 4096, Arc::new(WebSocketHub::default())),
        VmSchedulerTopology::new(1).unwrap(),
    )
    .unwrap();
    let connect = |path: &str| {
        let tcp = std::net::TcpStream::connect(server.local_addr()).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        tcp.set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let tls = ClientConnection::new(
            Arc::clone(&client_config),
            ServerName::try_from("localhost").unwrap(),
        )
        .unwrap();
        let (socket, response) =
            client(format!("wss://localhost{path}"), StreamOwned::new(tls, tcp)).unwrap();
        assert_eq!(response.status(), 101);
        socket
    };

    let mut first = connect("/pair");
    assert_eq!(first.read().unwrap(), Message::text("waiting"));
    first.send(Message::text("early")).unwrap();
    assert_eq!(first.read().unwrap(), Message::text("source:early"));
    let mut second = connect("/pair");
    assert_eq!(first.read().unwrap(), Message::text("first"));
    assert_eq!(second.read().unwrap(), Message::text("second"));
    for payload in ["", "update"] {
        second.send(Message::text(payload)).unwrap();
        let expected = if payload.is_empty() {
            String::new()
        } else {
            format!("source:{payload}")
        };
        assert_eq!(first.read().unwrap(), Message::text(expected.clone()));
        assert_eq!(second.read().unwrap(), Message::text(expected));
    }
    second.close(None).unwrap();
    assert_eq!(second.read().unwrap(), Message::Close(None));
    assert_eq!(first.read().unwrap(), Message::text("left"));
    first.send(Message::text("alone")).unwrap();
    assert_eq!(first.read().unwrap(), Message::text("source:alone"));
    first.close(None).unwrap();
    assert_eq!(first.read().unwrap(), Message::Close(None));

    let mut early = connect("/stateful");
    assert_eq!(early.read().unwrap(), Message::text("waiting"));
    early.send(Message::text("not yet")).unwrap();
    match early.read().unwrap_err() {
        Error::ConnectionClosed | Error::AlreadyClosed => {}
        Error::Protocol(tungstenite::error::ProtocolError::ResetWithoutClosingHandshake) => {}
        Error::Io(error) => assert!(matches!(
            error.kind(),
            std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::BrokenPipe
        )),
        error => panic!("source rejection must close, not stall: {error}"),
    }
    let mut first = connect("/stateful");
    assert_eq!(first.read().unwrap(), Message::text("waiting"));
    let mut second = connect("/stateful");
    assert_eq!(first.read().unwrap(), Message::text("first"));
    assert_eq!(second.read().unwrap(), Message::text("second"));
    for (input, expected) in [("a", "a"), ("b", "ab")] {
        second.send(Message::text(input)).unwrap();
        assert_eq!(first.read().unwrap(), Message::text(expected));
    }
    second.close(None).unwrap();
    // No stateful frame was sent to the second peer before its close reply.
    assert_eq!(second.read().unwrap(), Message::Close(None));
    assert_eq!(first.read().unwrap(), Message::text("left"));
    first.close(None).unwrap();
    assert_eq!(first.read().unwrap(), Message::Close(None));
    server.stop().unwrap();
}
