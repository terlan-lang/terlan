//! Full-cycle evidence for production WebSocket and SSE transport pumps.

use std::fs;
use std::io::{Cursor, Read, Write};

use tungstenite::protocol::{Message, Role, WebSocket};

use crate::commands::serve::handler_cache::handler_cache_test_support::clear_vm_handler_module_cache_for_test;
use crate::runtime::vm::websocket::VmWebSocketFrame;
use crate::support::test_fs;

use super::*;
use crate::commands::serve::{handle_vm_stream_http1_exchange, prewarm_dynamic_handler_sources};

/// Source module containing parked callbacks for the WebSocket route.
const WEBSOCKET_SOURCE: &str = r#"module app.SocketTransport.

import std.core.Unit.
import std.http.{Router, WebSocket}.
import std.vm.Process.
import type std.http.Router.

pub websocket_opened(): Unit -> Unit.
pub consume(_value: String): Unit -> Unit.
pub websocket_inbound(_frame: String): Unit -> consume(Process.receive_string()).
pub websocket_writable(): Unit -> Unit.
pub websocket_closed(): Unit -> Unit.
pub websocket_cancelled(_reason: String): Unit -> Unit.

pub router(): Router ->
    Router.new().websocket(
        "/socket",
        WebSocket.endpoint(2, 1024).callbacks(
            websocket_opened,
            websocket_inbound,
            websocket_writable,
            websocket_closed,
            websocket_cancelled
        )
    ).
"#;

/// Source module containing parked callbacks for the SSE route.
const SSE_SOURCE: &str = r#"module app.SseTransport.

import std.core.Unit.
import std.http.{Router, Sse}.
import std.vm.Process.
import type std.http.Router.

pub sse_opened(): Unit -> Unit.
pub consume(_value: String): Unit -> Unit.
pub sse_event_ready(_data: String): Unit -> consume(Process.receive_string()).
pub sse_keep_alive(): Unit -> Unit.
pub sse_drained(): Unit -> Unit.
pub sse_cancelled(_reason: String): Unit -> Unit.

pub router(): Router ->
    Router.new().sse(
        "/events",
        Sse.endpoint_with_keep_alive(2, 1024, 1).callbacks(
            sse_opened,
            sse_event_ready,
            sse_keep_alive,
            sse_drained,
            sse_cancelled
        )
    ).
"#;

/// Builds one real web package and prewarms its native channel image.
fn channel_package() -> std::path::PathBuf {
    clear_vm_handler_module_cache_for_test();
    let root = test_fs::temp_path("serve", "aot_channel_transport");
    let web_root = root.join("_build/web");
    let source_dir = root.join("src/app");
    fs::create_dir_all(&web_root).expect("create web output");
    fs::create_dir_all(&source_dir).expect("create source directory");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"0.0.7\"\n",
    )
    .expect("write package manifest");
    fs::write(source_dir.join("SocketTransport.terl"), WEBSOCKET_SOURCE)
        .expect("write WebSocket source");
    fs::write(source_dir.join("SseTransport.terl"), SSE_SOURCE).expect("write SSE source");
    fs::write(web_root.join("index.html"), "<!doctype html>\n").expect("write index");
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "websockets": [
    {
      "module": "app.SocketTransport",
      "route": "/socket",
      "protocol": "app.channel.v1",
      "source": { "path": "src/app/SocketTransport.terl", "line": 14, "column": 5 }
    }
  ],
  "sse": [
    {
      "module": "app.SseTransport",
      "route": "/events",
      "source": { "path": "src/app/SseTransport.terl", "line": 14, "column": 5 }
    }
  ],
  "assets": []
}
"#,
    )
    .expect("write web manifest");
    prewarm_dynamic_handler_sources(&web_root).expect("prewarm channel source");
    root
}

/// In-memory duplex used for already-upgraded WebSocket traffic.
struct MemoryDuplex {
    read: Cursor<Vec<u8>>,
    written: Vec<u8>,
}

impl MemoryDuplex {
    /// Creates a duplex with fixed peer bytes and an empty server output.
    fn new(read: Vec<u8>) -> Self {
        Self {
            read: Cursor::new(read),
            written: Vec::new(),
        }
    }
}

impl Read for MemoryDuplex {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.read.read(buffer)
    }
}

impl Write for MemoryDuplex {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.written.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Writer that disconnects on one selected flush boundary.
struct DisconnectingDuplex {
    written: Vec<u8>,
    flushes: usize,
    fail_on_flush: usize,
}

impl DisconnectingDuplex {
    /// Creates a stream that fails at the requested one-based flush count.
    fn new(fail_on_flush: usize) -> Self {
        Self {
            written: Vec::new(),
            flushes: 0,
            fail_on_flush,
        }
    }
}

impl Read for DisconnectingDuplex {
    fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
        Ok(0)
    }
}

impl Write for DisconnectingDuplex {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.written.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flushes += 1;
        if self.flushes == self.fail_on_flush {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "injected client disconnect",
            ));
        }
        Ok(())
    }
}

/// Encodes two client text messages and a graceful close through tungstenite.
fn client_websocket_frames() -> Vec<u8> {
    let stream = MemoryDuplex::new(Vec::new());
    let mut socket = WebSocket::from_raw_socket(stream, Role::Client, None);
    socket
        .send(Message::text("transport-one"))
        .expect("send first client text");
    socket
        .send(Message::text("transport-two"))
        .expect("send second client text");
    socket.close(None).expect("send client close");
    socket.into_inner().written
}

/// Proves route admission, pressure, typed wake, cancellation, and drain.
#[test]
fn production_channel_pumps_preserve_vm_lifecycle_and_pressure_contracts() {
    let root = channel_package();
    let web_root = root.join("_build/web");

    let mut websocket = handle_vm_stream_http1_exchange(
        &web_root,
        b"GET /socket HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
    )
    .expect("admit WebSocket exchange");
    let Some(VmHttpChannelTransport::WebSocket(session)) = websocket.channel.as_mut() else {
        panic!("expected retained WebSocket session")
    };
    session
        .enqueue_inbound(VmWebSocketFrame::Text("queued-one".to_string()))
        .expect("queue first frame");
    session
        .enqueue_inbound(VmWebSocketFrame::Text("queued-two".to_string()))
        .expect("queue second frame");
    let pressure = session
        .enqueue_inbound(VmWebSocketFrame::Text("overflow".to_string()))
        .expect_err("bounded WebSocket queue must reject overflow");
    assert!(
        pressure.contains("pending frame queue is full"),
        "{pressure}"
    );
    drain_websocket_inbound(session).expect("dispatch queued frame and typed wake");
    assert_eq!(session.inspect().pending_frames, 0);
    assert!(!session.is_waiting());

    let mut websocket_stream = MemoryDuplex::new(client_websocket_frames());
    serve_vm_stream_http1_exchange(&mut websocket_stream, websocket)
        .expect("pump WebSocket transport");
    let websocket_wire = String::from_utf8_lossy(&websocket_stream.written);
    assert!(
        websocket_wire.starts_with("HTTP/1.1 101 Switching Protocols\r\n"),
        "{websocket_wire}"
    );
    assert!(
        websocket_stream.written.len() > websocket_wire.find("\r\n\r\n").unwrap() + 4,
        "server close frame must follow the upgrade head"
    );

    let mut sse = handle_vm_stream_http1_exchange(
        &web_root,
        b"GET /events HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("admit SSE exchange");
    let Some(VmHttpChannelTransport::Sse(session)) = sse.channel.as_mut() else {
        panic!("expected retained SSE session")
    };
    session
        .enqueue_event("alpha".to_string())
        .expect("queue first SSE event");
    session
        .enqueue_event("beta".to_string())
        .expect("queue second SSE event and typed wake");
    let pressure = session
        .enqueue_event("overflow".to_string())
        .expect_err("bounded SSE queue must reject overflow");
    assert!(pressure.contains("BackpressureExceeded"), "{pressure}");
    session.drain().expect("begin graceful SSE drain");

    let mut sse_stream = MemoryDuplex::new(Vec::new());
    serve_vm_stream_http1_exchange(&mut sse_stream, sse).expect("drain SSE transport");
    let sse_wire = String::from_utf8(sse_stream.written).expect("SSE wire is UTF-8");
    assert!(
        sse_wire.contains("Transfer-Encoding: chunked\r\n"),
        "{sse_wire}"
    );
    assert!(sse_wire.contains("data: alpha\n\n"), "{sse_wire}");
    assert!(sse_wire.contains("data: beta\n\n"), "{sse_wire}");
    assert!(sse_wire.ends_with("0\r\n\r\n"), "{sse_wire}");

    let disconnect = handle_vm_stream_http1_exchange(
        &web_root,
        b"GET /events HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .expect("admit disconnect SSE exchange");
    let mut disconnect_stream = DisconnectingDuplex::new(2);
    serve_vm_stream_http1_exchange(&mut disconnect_stream, disconnect)
        .expect("disconnect must cancel and release SSE state");
    let disconnect_wire = String::from_utf8(disconnect_stream.written).expect("SSE wire is UTF-8");
    assert!(disconnect_wire.contains(": connected\n\n"));
    assert!(disconnect_wire.contains(": keep-alive\n\n"));

    clear_vm_handler_module_cache_for_test();
    fs::remove_dir_all(root).expect("cleanup channel fixture");
}
