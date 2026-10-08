use super::*;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use std::cell::RefCell;
use std::future::pending;
use std::io::{self, Cursor};
use std::sync::Mutex;
use std::task::{Context, Waker};
use tungstenite::protocol::{Role, WebSocket};

use crate::http_test_io::{complete, MemoryIo, State};
use crate::websocket::upgrade::admit;
use crate::websocket::upgrade_test_support::{Events, Source};
use crate::websocket::Message;

fn response(status: u16, text: &'static str) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .body(ResponseBody::Buffered(Full::new(Bytes::from_static(
            text.as_bytes(),
        ))))
        .unwrap()
}

fn upgrade_request(frames: Vec<Message>) -> Arc<Mutex<State>> {
    let mut client = WebSocket::from_raw_socket(Cursor::new(Vec::new()), Role::Client, None);
    for frame in frames {
        client.send(frame).unwrap();
    }
    let mut bytes = b"GET /ws?token=value HTTP/1.1\r\nHost: local\r\nConnection: upgrade\r\nUpgrade: websocket\r\n\r\n".to_vec();
    bytes.extend(client.into_inner().into_inner());
    Arc::new(Mutex::new(State {
        incoming: bytes.into(),
        ..State::default()
    }))
}

type Calls = Rc<RefCell<Vec<(String, Bytes)>>>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Handler {
    Reply,
    Upgrade,
    ParkBeforeUpgrade,
    WrongResponse,
    WrongUpgrade,
}

fn connection<const KIND: u8>(
    state: Arc<Mutex<State>>,
    handler: Handler,
    events: Events,
    calls: Calls,
    hub: &Arc<WebSocketHub>,
) -> impl Future<Output = Result<(), crate::ServiceError>> + '_ {
    serve_http1(
        MemoryIo::<KIND>(state),
        move |mut request, slot| {
            let events = events.clone();
            let calls = calls.clone();
            async move {
                if handler == Handler::Reply {
                    let path = request.uri().path().to_owned();
                    let body = request.into_body().collect().await.unwrap().to_bytes();
                    calls.borrow_mut().push((path, body));
                    return response(200, "answer");
                }
                assert_eq!(request.uri().path_and_query().unwrap(), "/ws?token=value");
                let upgrade = if handler == Handler::WrongUpgrade {
                    hyper::upgrade::on(Request::new(()))
                } else {
                    hyper::upgrade::on(&mut request)
                };
                admit(
                    Some(&slot),
                    Some(upgrade),
                    Source::new(&events, false),
                    "/ws".into(),
                    "/ws?token=value".into(),
                )
                .unwrap();
                if handler == Handler::ParkBeforeUpgrade {
                    pending::<()>().await;
                }
                if handler == Handler::WrongResponse {
                    return response(200, "not an upgrade");
                }
                let mut response = response(101, "");
                response
                    .headers_mut()
                    .insert("connection", "upgrade".parse().unwrap());
                response
                    .headers_mut()
                    .insert("upgrade", "websocket".parse().unwrap());
                response
            }
        },
        hub,
        move || async move {
            assert!(
                handler != Handler::Reply,
                "HTTP dispatch must not use the WebSocket wait"
            );
            pending::<()>().await;
        },
        |error| format!("owner context: {error}"),
    )
}

#[test]
fn keep_alive_dispatches_each_complete_request_once_without_transport_waiting() {
    let state = Arc::new(Mutex::new(State {
        incoming: b"POST /first HTTP/1.1\r\nHost: local\r\nContent-Length: 3\r\n\r\nonePOST /second HTTP/1.1\r\nHost: local\r\nConnection: close\r\nContent-Length: 3\r\n\r\ntwo".to_vec().into(),
        ..State::default()
    }));
    let calls = Rc::new(RefCell::new(Vec::new()));
    complete(connection::<0>(
        state.clone(),
        Handler::Reply,
        Events::default(),
        calls.clone(),
        &Arc::default(),
    ))
    .unwrap();
    assert_eq!(
        *calls.borrow(),
        [
            ("/first".into(), Bytes::from_static(b"one")),
            ("/second".into(), Bytes::from_static(b"two")),
        ]
    );
    let state = state.lock().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&state.outgoing)
            .matches("HTTP/1.1 200")
            .count(),
        2
    );
    assert_eq!(
        String::from_utf8_lossy(&state.outgoing)
            .matches("answer")
            .count(),
        2
    );
    assert_eq!(state.drops, 1);
}

#[test]
fn handoff_preserves_read_ahead_and_closes_the_exact_admitted_transport() {
    fn check<const KIND: u8>() {
        let state = upgrade_request(vec![Message::text("payload"), Message::Close(None)]);
        let events = Events::default();
        complete(connection::<KIND>(
            state.clone(),
            Handler::Upgrade,
            events.clone(),
            Calls::default(),
            &Arc::default(),
        ))
        .unwrap();
        assert!(state.lock().unwrap().outgoing.starts_with(b"HTTP/1.1 101"));
        assert_eq!(
            *events.lock().unwrap(),
            ["writable", "text:payload", "close"]
        );
        assert_eq!(state.lock().unwrap().drops, 1);
    }
    check::<0>();
    check::<1>();
    check::<2>();
}

#[test]
fn connection_failure_retains_context_and_cancels_an_admitted_upgrade_once() {
    let state = upgrade_request(vec![]);
    state.lock().unwrap().write_error = Some(io::ErrorKind::BrokenPipe);
    let events = Events::default();
    let error = complete(connection::<0>(
        state.clone(),
        Handler::Upgrade,
        events.clone(),
        Calls::default(),
        &Arc::default(),
    ))
    .unwrap_err()
    .to_string();
    assert!(error.starts_with("owner context:"), "{error}");
    assert_eq!(
        *events.lock().unwrap(),
        ["cancel:websocket upgrade abandoned before transport handoff"]
    );
    assert_eq!(state.lock().unwrap().drops, 1);
}

#[test]
fn dropping_before_and_after_handoff_releases_transport_and_cancels_once() {
    for parked_handler in [false, true] {
        let state = upgrade_request(vec![]);
        let events = Events::default();
        let hub = Arc::default();
        let handler = if parked_handler {
            Handler::ParkBeforeUpgrade
        } else {
            Handler::Upgrade
        };
        let mut future = Box::pin(connection::<0>(
            state.clone(),
            handler,
            events.clone(),
            Calls::default(),
            &hub,
        ));
        assert!(future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        drop(future);
        let events = events.lock().unwrap();
        if parked_handler {
            assert_eq!(
                *events,
                ["cancel:websocket upgrade abandoned before transport handoff"]
            );
        } else {
            assert_eq!(
                *events,
                ["writable", "cancel:websocket connection task dropped"]
            );
        }
        assert_eq!(state.lock().unwrap().drops, 1);
    }
}

#[test]
fn ordinary_response_rejects_admitted_upgrade_without_waiting_for_impossible_handoff() {
    let state = upgrade_request(vec![]);
    let events = Events::default();
    complete(connection::<0>(
        state.clone(),
        Handler::WrongResponse,
        events.clone(),
        Calls::default(),
        &Arc::default(),
    ))
    .unwrap();
    let state = state.lock().unwrap();
    let output = String::from_utf8_lossy(&state.outgoing);
    assert!(output.starts_with("HTTP/1.1 500"), "{output}");
    assert!(output.contains("admitted WebSocket session requires HTTP status 101"));
    assert_eq!(events.lock().unwrap().len(), 1);
    assert!(events.lock().unwrap()[0].starts_with("cancel:"));
    assert_eq!(state.drops, 1);
}

#[test]
fn failed_handoff_reports_upgrade_error_instead_of_protocol_context() {
    let state = upgrade_request(vec![]);
    let events = Events::default();
    let error = complete(connection::<0>(
        state.clone(),
        Handler::WrongUpgrade,
        events.clone(),
        Calls::default(),
        &Arc::default(),
    ))
    .unwrap_err()
    .to_string();
    assert!(error.contains("Hyper upgrade failed"), "{error}");
    assert!(!error.contains("owner context"));
    assert_eq!(events.lock().unwrap().len(), 1);
    assert!(events.lock().unwrap()[0].starts_with("cancel:"));
    assert_eq!(state.lock().unwrap().drops, 1);
}
