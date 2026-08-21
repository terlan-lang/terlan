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
        // SAFETY: the blocking read initialized `read` bytes and the cursor
        // advertised at least that much remaining capacity.
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                cursor.as_mut().as_mut_ptr().cast::<u8>(),
                read,
            );
            cursor.advance(read);
        }
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
    client.alpn_protocols = vec![b"h2".to_vec()];
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
