use super::*;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use std::cell::Cell;
use std::future::pending;
use std::io;
use std::task::{Context, Poll, Waker};

use crate::http2::StreamTask;
use crate::http_test_io::complete;
use crate::tls_test_support::{connected, exchange, Transport};
use crate::websocket::upgrade_test_support::Source;

fn reply(text: &'static str) -> Response<ResponseBody> {
    Response::new(ResponseBody::Buffered(Full::new(Bytes::from_static(
        text.as_bytes(),
    ))))
}

#[test]
fn tls_dispatches_http1_and_default_alpn_with_upgrade_authority() {
    for alpn in [None, Some(b"http/1.1".as_slice())] {
        let (io, mut client, transport) = connected(alpn);
        client.writer().write_all(b"GET /first HTTP/1.1\r\nHost: local\r\n\r\nGET /second HTTP/1.1\r\nHost: local\r\nConnection: close\r\n\r\n").unwrap();
        exchange(&mut client, &transport);
        let calls = Rc::new(RefCell::new(Vec::new()));
        let observed = calls.clone();
        complete(serve_tls::<_, Source, _, _, _, _, _>(
            io,
            move |request, slot| {
                assert!(slot.is_some());
                observed.borrow_mut().push(request.uri().path().to_owned());
                std::future::ready(reply("answer"))
            },
            &Arc::default(),
            || async { panic!("ordinary HTTP must not wait on a WebSocket") },
            |_| panic!("HTTP/1 must not submit HTTP/2 tasks"),
        ))
        .unwrap();
        assert_eq!(*calls.borrow(), ["/first", "/second"]);
        exchange(&mut client, &transport);
        let mut response = String::new();
        client.reader().read_to_string(&mut response).unwrap();
        assert_eq!(response.matches("HTTP/1.1 200").count(), 2);
        assert_eq!(response.matches("answer").count(), 2);
        assert!(transport.0.lock().unwrap().shutdown);
        assert_eq!(Arc::strong_count(&transport.0), 1);
    }
}

#[test]
fn tls_rejects_unknown_alpn_and_reports_protocol_specific_errors() {
    for (alpn, prefix) in [
        (b"unknown".as_slice(), "error[serve_tls.alpn]"),
        (b"http/1.1", "Hyper HTTP/1.1 TLS connection failed:"),
        (b"h2", "Hyper HTTP/2 TLS connection failed:"),
    ] {
        let (io, mut client, transport) = connected(Some(alpn));
        client
            .writer()
            .write_all(b"invalid protocol bytes\r\n\r\n")
            .unwrap();
        client.send_close_notify();
        exchange(&mut client, &transport);
        let error = complete(serve_tls::<_, Source, _, _, _, _, _>(
            io,
            |_, _| async { panic!("invalid protocol must not enter application code") },
            &Arc::default(),
            pending::<()>,
            |_| panic!("invalid connection must not submit stream tasks"),
        ))
        .unwrap_err()
        .to_string();
        assert!(error.starts_with(prefix), "{error}");
        assert_eq!(Arc::strong_count(&transport.0), 1);
    }
}

#[test]
fn dropping_tls_dispatch_cancels_a_parked_http1_handler_and_its_transport() {
    struct Lease(Rc<Cell<usize>>);
    impl Drop for Lease {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let (io, mut client, transport) = connected(None);
    client
        .writer()
        .write_all(b"GET / HTTP/1.1\r\nHost: local\r\n\r\n")
        .unwrap();
    exchange(&mut client, &transport);
    let drops = Rc::new(Cell::new(0));
    let retained = drops.clone();
    let hub = Arc::default();
    let mut connection = Box::pin(serve_tls::<_, Source, _, _, _, _, _>(
        io,
        move |_, slot| {
            assert!(slot.is_some());
            let lease = Lease(retained.clone());
            async move {
                let _lease = lease;
                pending::<Response<ResponseBody>>().await
            }
        },
        &hub,
        pending::<()>,
        |_| panic!("HTTP/1 must not submit HTTP/2 tasks"),
    ));
    assert!(connection
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
        .is_pending());
    assert_eq!(drops.get(), 0);
    drop(connection);
    assert_eq!(drops.get(), 1);
    assert_eq!(Arc::strong_count(&transport.0), 1);
}

struct ClientIo {
    client: rustls::ClientConnection,
    transport: Transport,
}

impl Read for ClientIo {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        exchange(&mut self.client, &self.transport);
        self.client.reader().read(bytes)
    }
}

impl Write for ClientIo {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.client.writer().write(bytes)?;
        exchange(&mut self.client, &self.transport);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        exchange(&mut self.client, &self.transport);
        Ok(())
    }
}

impl terlan_net_native::tls_stream::ShutdownWrite for ClientIo {
    fn shutdown_write(&mut self) -> io::Result<()> {
        self.client.send_close_notify();
        self.flush()
    }
}

#[derive(Clone, Default)]
struct Tasks(Rc<RefCell<Vec<StreamTask>>>);

impl Tasks {
    fn poll_once(&self, context: &mut Context<'_>) {
        let ready = std::mem::take(&mut *self.0.borrow_mut());
        for mut task in ready {
            if task.as_mut().poll(context).is_pending() {
                self.0.borrow_mut().push(task);
            }
        }
    }
}

impl<F: Future<Output = ()> + 'static> hyper::rt::Executor<F> for Tasks {
    fn execute(&self, task: F) {
        self.0.borrow_mut().push(Box::pin(task));
    }
}

#[test]
fn tls_http2_dispatch_uses_host_tasks_and_never_grants_http1_upgrade_authority() {
    let (io, client, transport) = connected(Some(b"h2"));
    let tasks = Tasks::default();
    let server_tasks = Rc::downgrade(&tasks.0);
    let client_tasks = Tasks::default();
    let client_io = crate::plain_io::PlainIo::new(ClientIo {
        client,
        transport: transport.clone(),
    });
    let (mut sender, client) = complete(hyper::client::conn::http2::handshake(
        client_tasks.clone(),
        client_io,
    ))
    .unwrap();
    let mut client = Box::pin(client);
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let hub = Arc::default();
    let mut server = Box::pin(serve_tls::<_, Source, _, _, _, _, _>(
        io,
        move |request, slot| {
            let observed = observed.clone();
            async move {
                assert!(slot.is_none());
                assert_eq!(request.version(), http::Version::HTTP_2);
                observed.set(observed.get() + 1);
                let body = request.into_body().collect().await.unwrap().to_bytes();
                Response::new(ResponseBody::Buffered(Full::new(body)))
            }
        },
        &hub,
        || async { panic!("HTTP/2 must not wait on a WebSocket") },
        move |task| server_tasks.upgrade().unwrap().borrow_mut().push(task),
    ));
    let mut responses = Vec::new();
    for text in ["first", "second"] {
        responses.push(
            sender.send_request(
                Request::builder()
                    .uri("https://localhost/echo")
                    .method("POST")
                    .body(Full::new(Bytes::from_static(text.as_bytes())))
                    .unwrap(),
            ),
        );
    }
    let mut results = Box::pin(async move {
        let mut bodies = Vec::new();
        for response in futures_util::future::join_all(responses).await {
            let response = response.unwrap();
            assert_eq!(response.version(), http::Version::HTTP_2);
            bodies.push(response.into_body().collect().await.unwrap().to_bytes());
        }
        bodies
    });
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..64 {
        assert!(server.as_mut().poll(&mut context).is_pending());
        assert!(client.as_mut().poll(&mut context).is_pending());
        client_tasks.poll_once(&mut context);
    }
    assert_eq!(calls.get(), 0, "stream execution belongs to the host");
    assert!(!tasks.0.borrow().is_empty());
    let mut completed = false;
    for _ in 0..1000 {
        assert!(server.as_mut().poll(&mut context).is_pending());
        assert!(client.as_mut().poll(&mut context).is_pending());
        for queue in [&tasks, &client_tasks] {
            queue.poll_once(&mut context);
        }
        if let Poll::Ready(bodies) = results.as_mut().poll(&mut context) {
            assert_eq!(
                bodies,
                [Bytes::from_static(b"first"), Bytes::from_static(b"second")]
            );
            completed = true;
            break;
        }
    }
    assert!(completed, "TLS HTTP/2 dispatch did not complete");
    assert_eq!(calls.get(), 2);
    drop(server);
    tasks.0.borrow_mut().clear();
    drop(client);
    client_tasks.0.borrow_mut().clear();
    assert_eq!(Arc::strong_count(&transport.0), 1);
}
