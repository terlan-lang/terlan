use super::*;
use std::future::Future;
use std::io::Read as _;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper::rt::{Executor, Read, ReadBufCursor, Write};
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, ServerConfig, StreamOwned};

use crate::runtime::vm::protocol_task_executor::start_protocol_tasks_with_topology;
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;

fn restorable_pairing(retention_ms: u64, retained_room_capacity: usize) -> VmWebSocketPairingPlan {
    use crate::runtime::vm::native_callable::VmNativeCallableRef;
    use crate::runtime::vm::websocket::VmWebSocketPairRestorationPlan;

    let callback = |function: &str, arity| VmNativeCallableRef {
        module: "app.Socket".into(),
        function: function.into(),
        arity,
    };
    VmWebSocketPairingPlan {
        waiting: "waiting".into(),
        first_matched: String::new(),
        second_matched: String::new(),
        peer_left: "left".into(),
        stateful: true,
        restoration: Some(VmWebSocketPairRestorationPlan {
            waiting: callback("waiting", 0),
            peer_left: callback("peer_left", 0),
            room_query: "room_id".into(),
            player_query: "player_id".into(),
            room_prefix: "room-".into(),
            first_player: "player-1".into(),
            second_player: "player-2".into(),
            retention_ms,
            retained_room_capacity,
            matched: callback("matched", 4),
            restored: callback("restored", 5),
        }),
        inbound: callback("inbound", 5),
        cancellation: callback("cancelled", 1),
    }
}

#[test]
fn websocket_hub_pairs_broadcasts_and_notifies_disconnect() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = VmWebSocketPairingPlan {
        waiting: "waiting".into(),
        first_matched: "first".into(),
        second_matched: "second".into(),
        peer_left: "left".into(),
        stateful: false,
        restoration: None,
        inbound: crate::runtime::vm::native_callable::VmNativeCallableRef {
            module: "app.Socket".into(),
            function: "inbound".into(),
            arity: 1,
        },
        cancellation: crate::runtime::vm::native_callable::VmNativeCallableRef {
            module: "app.Socket".into(),
            function: "cancelled".into(),
            arity: 1,
        },
    };
    let first = hub
        .join("/ws".into(), "/ws?player=first".into(), 4, &pairing)
        .expect("join first peer");
    assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
    let second = hub
        .join("/ws".into(), "/ws?player=second".into(), 4, &pairing)
        .expect("join second peer");
    assert_eq!(first.outbound.try_recv().unwrap(), "first");
    assert_eq!(second.outbound.try_recv().unwrap(), "second");

    second.broadcast("update".into()).expect("broadcast update");
    assert_eq!(first.outbound.try_recv().unwrap(), "update");
    assert_eq!(second.outbound.try_recv().unwrap(), "update");
    drop(second);
    assert_eq!(first.outbound.try_recv().unwrap(), "left");
}

#[test]
fn websocket_hub_serializes_stateful_pair_transitions_and_addresses_peers() {
    let hub = Arc::new(WebSocketHub::default());
    let pairing = VmWebSocketPairingPlan {
        waiting: "waiting".into(),
        first_matched: "first".into(),
        second_matched: "second".into(),
        peer_left: "left".into(),
        stateful: true,
        restoration: None,
        inbound: crate::runtime::vm::native_callable::VmNativeCallableRef {
            module: "app.Socket".into(),
            function: "inbound".into(),
            arity: 5,
        },
        cancellation: crate::runtime::vm::native_callable::VmNativeCallableRef {
            module: "app.Socket".into(),
            function: "cancelled".into(),
            arity: 1,
        },
    };
    let first = hub
        .join("/ws".into(), "/ws?player=Ada".into(), 4, &pairing)
        .expect("join first peer");
    assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
    let second = hub
        .join("/ws".into(), "/ws?player=Grace".into(), 4, &pairing)
        .expect("join second peer");
    assert_eq!(first.outbound.try_recv().unwrap(), "first");
    assert_eq!(second.outbound.try_recv().unwrap(), "second");

    second
        .transition(|state, role, first_request, second_request| {
            assert_eq!(state, "");
            assert_eq!(role, 2);
            assert_eq!(first_request, "/ws?player=Ada");
            assert_eq!(second_request, "/ws?player=Grace");
            Ok(("move-1".into(), "view-1a".into(), "view-1b".into()))
        })
        .expect("first transition");
    assert_eq!(first.outbound.try_recv().unwrap(), "view-1a");
    assert_eq!(second.outbound.try_recv().unwrap(), "view-1b");

    first
        .transition(|state, role, _, _| {
            assert_eq!(state, "move-1");
            assert_eq!(role, 1);
            Ok(("move-2".into(), "view-2a".into(), "view-2b".into()))
        })
        .expect("second transition");
    assert_eq!(first.outbound.try_recv().unwrap(), "view-2a");
    assert_eq!(second.outbound.try_recv().unwrap(), "view-2b");
}

#[test]
fn websocket_hub_restores_disconnected_seat_with_retained_state() {
    let pairing = restorable_pairing(300_000, 1_024);
    let hub = Arc::new(WebSocketHub::default());
    let first = hub
        .join("/ws".into(), "/ws?board=first".into(), 4, &pairing)
        .expect("join first peer");
    assert_eq!(first.outbound.try_recv().unwrap(), "waiting");
    let second = hub
        .join("/ws".into(), "/ws?board=second".into(), 4, &pairing)
        .expect("join second peer");
    first
        .transition(|state, role, first_request, second_request| {
            assert_eq!(state, "");
            assert_eq!(role, 1);
            assert_eq!(first_request, "/ws?board=first");
            assert_eq!(second_request, "/ws?board=second");
            Ok(("1,5,5".into(), "first-view".into(), "second-view".into()))
        })
        .expect("retain first move");
    assert_eq!(first.outbound.try_recv().unwrap(), "first-view");
    assert_eq!(second.outbound.try_recv().unwrap(), "second-view");

    drop(second);
    assert_eq!(first.outbound.try_recv().unwrap(), "left");
    let restored = hub
        .join(
            "/ws".into(),
            "/ws?room_id=room-1&player_id=player-2".into(),
            4,
            &pairing,
        )
        .expect("restore second seat");
    restored
        .transition(|state, role, first_request, second_request| {
            assert_eq!(state, "1,5,5");
            assert_eq!(role, 2);
            assert_eq!(first_request, "/ws?board=first");
            assert_eq!(second_request, "/ws?board=second");
            Ok((
                "1,5,5;2,4,4".into(),
                "next-first".into(),
                "next-second".into(),
            ))
        })
        .expect("transition restored seat");
    assert_eq!(first.outbound.try_recv().unwrap(), "next-first");
    assert_eq!(restored.outbound.try_recv().unwrap(), "next-second");
}

#[test]
fn websocket_hub_retains_room_after_both_peers_disconnect() {
    let pairing = restorable_pairing(300_000, 8);
    let hub = Arc::new(WebSocketHub::default());
    let first = hub
        .join("/ws".into(), "/ws?board=first".into(), 4, &pairing)
        .expect("join first peer");
    let second = hub
        .join("/ws".into(), "/ws?board=second".into(), 4, &pairing)
        .expect("join second peer");
    drop(first);
    drop(second);

    let restored_first = hub
        .join(
            "/ws".into(),
            "/ws?room_id=room-1&player_id=player-1".into(),
            4,
            &pairing,
        )
        .expect("restore first seat");
    let restored_second = hub
        .join(
            "/ws".into(),
            "/ws?room_id=room-1&player_id=player-2".into(),
            4,
            &pairing,
        )
        .expect("restore second seat");
    restored_second
        .transition(|state, role, first_request, second_request| {
            assert_eq!(state, "");
            assert_eq!(role, 2);
            assert_eq!(first_request, "/ws?board=first");
            assert_eq!(second_request, "/ws?board=second");
            Ok(("retained".into(), "first".into(), "second".into()))
        })
        .expect("transition retained room");
    assert_eq!(restored_first.outbound.try_recv().unwrap(), "first");
    assert_eq!(restored_second.outbound.try_recv().unwrap(), "second");
}

#[test]
fn websocket_hub_expires_fully_disconnected_room() {
    let pairing = restorable_pairing(10, 8);
    let hub = Arc::new(WebSocketHub::default());
    let first = hub
        .join("/ws".into(), "/ws?board=first".into(), 4, &pairing)
        .expect("join first peer");
    let second = hub
        .join("/ws".into(), "/ws?board=second".into(), 4, &pairing)
        .expect("join second peer");
    drop(first);
    drop(second);

    let error = hub
        .join_at(
            "/ws".into(),
            "/ws?room_id=room-1&player_id=player-1".into(),
            4,
            &pairing,
            std::time::Instant::now() + Duration::from_secs(1),
        )
        .err()
        .expect("expired room must reject restoration");
    assert!(error.contains("room not found"));
}

#[test]
fn websocket_hub_evicts_oldest_fully_disconnected_room_at_capacity() {
    let pairing = restorable_pairing(300_000, 1);
    let hub = Arc::new(WebSocketHub::default());
    for suffix in ["first", "second"] {
        let first = hub
            .join("/ws".into(), format!("/ws?board={suffix}-1"), 4, &pairing)
            .expect("join first peer");
        let second = hub
            .join("/ws".into(), format!("/ws?board={suffix}-2"), 4, &pairing)
            .expect("join second peer");
        drop(first);
        drop(second);
    }

    let oldest_error = hub
        .join(
            "/ws".into(),
            "/ws?room_id=room-1&player_id=player-1".into(),
            4,
            &pairing,
        )
        .err()
        .expect("oldest retained room must be evicted");
    assert!(oldest_error.contains("room not found"));
    hub.join(
        "/ws".into(),
        "/ws?room_id=room-2&player_id=player-1".into(),
        4,
        &pairing,
    )
    .expect("newest retained room remains restorable");
}
#[path = "hyper_server_mtls_test.rs"]
mod mtls;

#[path = "hyper_server_tls_transport_test.rs"]
mod tls_transport;

#[test]
fn protocol_errors_are_hyper_responses() {
    let response = error_response(400, "bad request".to_string());
    assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
    assert_eq!(
        response.headers().get(http::header::CONTENT_TYPE),
        Some(&http::HeaderValue::from_static("text/plain; charset=utf-8"))
    );
}

#[test]
fn declared_and_chunked_bodies_are_bounded() {
    let mut headers = http::HeaderMap::new();
    headers.insert(http::header::CONTENT_LENGTH, "5".parse().unwrap());
    assert!(declared_body_exceeds_limit(&headers, 4));
    assert!(!declared_body_exceeds_limit(&headers, 5));

    let accepted = block_on(collect_bounded_body(
        http_body_util::Full::new(Bytes::from_static(b"1234")),
        4,
    ))
    .expect("body at limit");
    assert_eq!(accepted, b"1234");
    assert_eq!(
        block_on(collect_bounded_body(
            http_body_util::Full::new(Bytes::from_static(b"12345")),
            4,
        )),
        Err(BodyReadError::TooLarge)
    );
}

#[test]
fn binary_body_spool_is_create_new_bounded_and_removed_on_drop() {
    let root = temp_web_root();
    let temporary = block_on(spool_bounded_body_to_root(
        http_body_util::Full::new(Bytes::from_static(&[0, 159, 146, 150, 255])),
        5,
        &root,
    ))
    .expect("spool binary body");
    assert_eq!(
        std::fs::read(&temporary.path).expect("read spooled body"),
        [0, 159, 146, 150, 255]
    );
    let path = temporary.path.clone();
    drop(temporary);
    assert!(!path.exists());

    assert!(matches!(
        block_on(spool_bounded_body_to_root(
            http_body_util::Full::new(Bytes::from_static(b"123456")),
            5,
            &root,
        )),
        Err(BodyReadError::TooLarge)
    ));
    assert!(std::fs::read_dir(&root)
        .expect("read upload root")
        .next()
        .is_none());
    std::fs::remove_dir_all(root).expect("remove upload root");
}

#[test]
fn web_root_is_copied_once_per_permanent_protocol_owner() {
    let first = Arc::new(PathBuf::from("/tmp/terlan-owner-a"));
    let first_local = owner_local_web_root(&first);
    let first_reused = owner_local_web_root(&first);
    assert!(Rc::ptr_eq(&first_local, &first_reused));

    let second = Arc::new(PathBuf::from("/tmp/terlan-owner-b"));
    let second_local = owner_local_web_root(&second);
    assert!(!Rc::ptr_eq(&first_local, &second_local));
    assert_eq!(second_local.as_path(), second.as_path());
}

#[test]
fn vm_owned_tls_serves_http2_selected_by_rustls_alpn() {
    let root = temp_web_root();
    std::fs::write(root.join("index.html"), "terlan-http2-ok").expect("write HTTP/2 fixture");
    let (server_config, client_config) = tls_pair();
    let listener =
        crate::runtime::vm::protocol_task_executor::bind_protocol_listener("127.0.0.1", 0)
            .expect("bind protocol listener");
    let mut server = start_protocol_tasks_with_topology(
        listener,
        tls_factory(
            root.clone(),
            server_config,
            crate::commands::serve::args::DEFAULT_MAX_BODY_BYTES,
            Arc::new(WebSocketHub::default()),
        ),
        VmSchedulerTopology::new(1).expect("single test scheduler"),
    )
    .expect("start VM TLS protocol server");

    let tcp = std::net::TcpStream::connect(server.local_addr()).expect("connect TLS client");
    tcp.set_nonblocking(true).expect("set client nonblocking");
    let connection = ClientConnection::new(
        client_config,
        ServerName::try_from("localhost").expect("server name"),
    )
    .expect("create rustls client");
    let io = BlockingTlsIo(StreamOwned::new(connection, tcp));
    let (mut sender, connection) =
        block_on(hyper::client::conn::http2::handshake(ThreadExecutor, io))
            .expect("HTTP/2 client handshake");
    let connection_thread = std::thread::spawn(move || block_on(connection));
    let request = Request::builder()
        .version(http::Version::HTTP_2)
        .method("GET")
        .uri("https://localhost/")
        .body(Empty::<Bytes>::new())
        .expect("HTTP/2 request");
    let response = block_on(sender.send_request(request)).expect("HTTP/2 response");
    assert_eq!(response.version(), http::Version::HTTP_2);
    assert_eq!(response.status(), http::StatusCode::OK);
    let body = block_on(response.into_body().collect())
        .expect("collect HTTP/2 response")
        .to_bytes();
    assert!(body.starts_with(b"terlan-http2-ok"));

    let second = Request::builder()
        .version(http::Version::HTTP_2)
        .method("GET")
        .uri("https://localhost/")
        .body(Empty::<Bytes>::new())
        .expect("second HTTP/2 request");
    let second = block_on(sender.send_request(second)).expect("second HTTP/2 response");
    assert_eq!(second.version(), http::Version::HTTP_2);
    assert_eq!(second.status(), http::StatusCode::OK);
    drop(sender);
    server.stop().expect("stop VM TLS protocol server");
    let _ = connection_thread.join();
    std::fs::remove_dir_all(root).expect("remove HTTP/2 fixture");
}

#[test]
fn vm_owned_tls_serves_http1_on_upgrade_capable_path() {
    let root = temp_web_root();
    std::fs::write(root.join("index.html"), "terlan-http1-tls-ok")
        .expect("write HTTP/1 TLS fixture");
    let (server_config, client_config) = tls_pair_with_client_alpn(b"http/1.1");
    let listener =
        crate::runtime::vm::protocol_task_executor::bind_protocol_listener("127.0.0.1", 0)
            .expect("bind protocol listener");
    let mut server = start_protocol_tasks_with_topology(
        listener,
        tls_factory(
            root.clone(),
            server_config,
            crate::commands::serve::args::DEFAULT_MAX_BODY_BYTES,
            Arc::new(WebSocketHub::default()),
        ),
        VmSchedulerTopology::new(1).expect("single test scheduler"),
    )
    .expect("start VM TLS protocol server");

    let tcp = std::net::TcpStream::connect(server.local_addr()).expect("connect TLS client");
    tcp.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set TLS client timeout");
    let connection = ClientConnection::new(
        client_config,
        ServerName::try_from("localhost").expect("server name"),
    )
    .expect("create rustls client");
    let mut stream = StreamOwned::new(connection, tcp);
    std::io::Write::write_all(
        &mut stream,
        b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .expect("write HTTPS request");
    let mut response = String::new();
    std::io::Read::read_to_string(&mut stream, &mut response).expect("read HTTPS response");
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("terlan-http1-tls-ok"));

    server.stop().expect("stop VM TLS protocol server");
    std::fs::remove_dir_all(root).expect("remove HTTP/1 TLS fixture");
}

#[derive(Clone, Copy)]
struct ThreadExecutor;

impl<F> Executor<F> for ThreadExecutor
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, future: F) {
        std::thread::spawn(move || {
            let _ = block_on(future);
        });
    }
}

struct ThreadWake(std::thread::Thread);

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park_timeout(Duration::from_millis(50)),
        }
    }
}

struct BlockingTlsIo(StreamOwned<ClientConnection, std::net::TcpStream>);

impl Read for BlockingTlsIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        mut cursor: ReadBufCursor<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut bytes = vec![0_u8; cursor.remaining().min(16 * 1024)];
        let read = match self.0.read(&mut bytes) {
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Poll::Pending,
            Err(error) => return Poll::Ready(Err(error)),
        };
        cursor.put_slice(&bytes[..read]);
        Poll::Ready(Ok(()))
    }
}

impl Write for BlockingTlsIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.0.write(bytes) {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
            outcome => Poll::Ready(outcome),
        }
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.0.flush() {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
            outcome => Poll::Ready(outcome),
        }
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.0.conn.send_close_notify();
        self.poll_flush(_context)
    }
}

fn tls_pair() -> (Arc<ServerConfig>, Arc<ClientConfig>) {
    tls_pair_with_client_alpn(b"h2")
}

fn tls_pair_with_client_alpn(protocol: &[u8]) -> (Arc<ServerConfig>, Arc<ClientConfig>) {
    let generated =
        generate_simple_self_signed(vec!["localhost".to_string()]).expect("generate TLS fixture");
    let certificate = generated.cert.der().clone();
    let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(generated.key_pair.serialize_der()));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut server = ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .expect("server TLS versions")
        .with_no_client_auth()
        .with_single_cert(vec![certificate.clone()], key)
        .expect("server TLS fixture");
    server.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(certificate.as_ref().to_vec()))
        .expect("trust TLS fixture");
    let mut client = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("client TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    client.alpn_protocols = vec![protocol.to_vec()];
    (Arc::new(server), Arc::new(client))
}

fn temp_web_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("terlan-hyper-http2-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create HTTP/2 fixture");
    root
}

#[test]
fn source_response_stream_reaches_production_hyper_with_bounds_and_metadata() {
    let root = temp_web_root();
    let web = root.join("_build/web");
    std::fs::create_dir_all(root.join("src/app")).unwrap();
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"stream_test\"\nversion = \"0.0.9\"\nnamespace = \"app\"\n",
    )
    .unwrap();
    std::fs::write(web.join("index.html"), "").unwrap();
    std::fs::write(
        root.join("src/app/Api.terl"),
        r#"module app.Api.
import std.http.Response.
import type std.http.Request.{Request}.
import type std.http.Response.{Response}.
pub handle(request: Request): Response ->
    case request.query_string() {
        "plain" -> Response.text("buffered");
        "empty" -> Response.stream(["", ""]);
        "invalid" -> Response.stream(["bad"], 200, "text/plain", 0, 1);
        _ -> Response.stream(["hello", "", request.body_text(), "é🙂"], 201, "text/plain", 3, 1)
            .with_status(202)
            .with_cookie("session", "abc", "/", true, true)
            .with_security_headers(Response.default_security_headers())
    }.
"#,
    )
    .unwrap();
    std::fs::write(
        web.join("manifest.json"),
        r#"{
        "schema":"terlan-web-build-v1", "target_profile":"js.browser",
        "source_js_manifest":"../js/manifest.json", "index":"index.html", "assets":[],
        "handlers":[{"method":"GET","route":"/stream","module":"app.Api",
            "function":"handle","arity":1,"source":{"path":"src/app/Api.terl","line":5,"column":5}}]
    }"#,
    )
    .unwrap();
    crate::commands::serve::prewarm_dynamic_handler_sources(&web)
        .expect("compile streaming source");
    let (server_config, client_config) = tls_pair_with_client_alpn(b"http/1.1");
    let listener =
        crate::runtime::vm::protocol_task_executor::bind_protocol_listener("127.0.0.1", 0).unwrap();
    let mut server = start_protocol_tasks_with_topology(
        listener,
        tls_factory(
            web.clone(),
            server_config,
            1024,
            Arc::new(WebSocketHub::default()),
        ),
        VmSchedulerTopology::new(1).unwrap(),
    )
    .unwrap();
    let request = |method: &str, query: &str| {
        let tcp = std::net::TcpStream::connect(server.local_addr()).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let connection = ClientConnection::new(
            Arc::clone(&client_config),
            ServerName::try_from("localhost").unwrap(),
        )
        .unwrap();
        let mut stream = StreamOwned::new(connection, tcp);
        std::io::Write::write_all(&mut stream, format!("{method} /stream?{query} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 3\r\n\r\nXYZ").as_bytes()).unwrap();
        let mut response = Vec::new();
        std::io::Read::read_to_end(&mut stream, &mut response).unwrap();
        response
    };
    let response = request("GET", "");
    let split = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap()
        + 4;
    let head = std::str::from_utf8(&response[..split]).unwrap();
    assert!(head.starts_with("HTTP/1.1 202"), "{head}");
    assert!(head.contains("transfer-encoding: chunked\r\n"), "{head}");
    assert!(!head.contains("content-length:"), "{head}");
    assert!(
        head.contains("set-cookie: session=abc; HttpOnly; Secure; Path=/\r\n"),
        "{head}"
    );
    assert!(head.contains("x-frame-options: DENY\r\n"), "{head}");
    let expected = [
        b"3\r\nhel\r\n2\r\nlo\r\n3\r\nXYZ\r\n3\r\n".as_slice(),
        &"é🙂".as_bytes()[..3],
        b"\r\n3\r\n",
        &"é🙂".as_bytes()[3..],
        b"\r\n0\r\n\r\n",
    ]
    .concat();
    assert_eq!(&response[split..], expected);
    let vm_wire = crate::commands::serve::request_dispatch::handle_vm_stream_http1_request(
        &web,
        b"GET /stream HTTP/1.1\r\nHost: localhost\r\nContent-Length: 3\r\n\r\nXYZ",
    )
    .unwrap();
    let vm_split = vm_wire
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap()
        + 4;
    assert!(vm_wire.starts_with(b"HTTP/1.1 202"));
    assert_eq!(&vm_wire[vm_split..], expected);
    let head = request("HEAD", "");
    assert!(head.starts_with(b"HTTP/1.1 202"));
    assert!(head.ends_with(b"\r\n\r\n"));
    let plain = request("GET", "plain");
    assert!(plain.starts_with(b"HTTP/1.1 200"));
    assert!(plain.ends_with(b"buffered"));
    let empty = request("GET", "empty");
    assert!(empty.starts_with(b"HTTP/1.1 200"));
    let invalid = request("GET", "invalid");
    assert!(invalid.starts_with(b"HTTP/1.1 500") || invalid.starts_with(b"HTTP/1.1 502"));
    assert!(String::from_utf8_lossy(&invalid).contains("invalid Response.stream limits"));
    server.stop().unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
