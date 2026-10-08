use super::*;
use crate::http_test_io::{complete, MemoryIo, State};
use crate::websocket::Message;
use bytes::Bytes;
use http_body_util::Full;
use hyper::service::service_fn;
use std::convert::Infallible;
use std::future::pending;
use std::io::Cursor;
use std::sync::Mutex;
use std::task::{Context, Waker};
use tungstenite::protocol::{Role, WebSocket};

use super::super::upgrade_test_support::{Events, Source};

fn empty_upgrade() -> hyper::upgrade::OnUpgrade {
    hyper::upgrade::on(http::Request::new(()))
}

#[test]
fn invalid_response_revokes_only_pending_upgrade_and_preserves_cleanup_error() {
    for status in [200, 204, 301, 400, 500] {
        for fail in [false, true] {
            let slot = UpgradeSlot::default();
            let events = Events::default();
            assert!(slot.validate_response(status).is_ok());
            admit_source(Some(&slot), Some(empty_upgrade()), &events, fail).unwrap();
            assert!(slot.validate_response(101).is_ok());
            assert!(events.lock().unwrap().is_empty());
            let error = slot.validate_response(status).unwrap_err();
            assert_eq!(error.status(), 500);
            assert_eq!(error.code(), "serve_http.upgrade_response");
            assert_eq!(error.message().contains("cleanup failed"), fail);
            assert!(slot.take().is_none());
            assert!(slot.validate_response(status).is_ok());
            drop(slot);
            assert_eq!(events.lock().unwrap().len(), 1);
            assert!(events.lock().unwrap()[0].contains("requires HTTP status 101"));
        }
    }
}

fn admit_source(
    slot: Option<&UpgradeSlot<Source>>,
    on_upgrade: Option<hyper::upgrade::OnUpgrade>,
    events: &Events,
    fail_cancel: bool,
) -> Result<(), HttpError> {
    admit(
        slot,
        on_upgrade,
        Source::new(events, fail_cancel),
        "/ws".into(),
        "/ws?key=value".into(),
    )
}

#[test]
fn rejected_upgrades_cancel_once_and_preserve_admitted_session() {
    for fail_cancel in [false, true] {
        let slot = UpgradeSlot::default();
        for (adapter, upgrade, expected_status, expected_message) in [
            (None, None, 501, "adapter is required"),
            (Some(&slot), None, 500, "future was not retained"),
        ] {
            let events = Events::default();
            let error = admit_source(adapter, upgrade, &events, fail_cancel).unwrap_err();
            assert_eq!(error.status(), expected_status);
            assert!(error.message().contains(expected_message));
            assert_eq!(error.message().contains("cleanup failed"), fail_cancel);
            assert_eq!(events.lock().unwrap().len(), 1);
            assert!(events.lock().unwrap()[0].contains(expected_message));
            assert!(slot.take().is_none());
        }
        let first = Events::default();
        admit_source(Some(&slot), Some(empty_upgrade()), &first, false).unwrap();
        let rejected = Events::default();
        let error =
            admit_source(Some(&slot), Some(empty_upgrade()), &rejected, fail_cancel).unwrap_err();
        assert!(error.message().contains("already owns an upgrade"));
        assert_eq!(error.message().contains("cleanup failed"), fail_cancel);
        assert_eq!(rejected.lock().unwrap().len(), 1);
        assert!(first.lock().unwrap().is_empty());
        drop(slot);
        assert_eq!(
            first.lock().unwrap().as_slice(),
            ["cancel:websocket upgrade abandoned before transport handoff"]
        );
    }
}

#[test]
fn dropping_unpolled_handoff_cancels_the_source() {
    let slot = UpgradeSlot::default();
    let events = Events::default();
    admit_source(Some(&slot), Some(empty_upgrade()), &events, false).unwrap();
    let upgrade = slot.take().unwrap();
    assert!(slot.take().is_none());
    let hub = Arc::default();
    let future = upgrade.serve::<MemoryIo<1>, _, _>(&hub, pending::<()>);
    drop(future);
    assert_eq!(
        events.lock().unwrap().as_slice(),
        ["cancel:websocket upgrade abandoned before transport handoff"]
    );
}

#[test]
fn failed_hyper_upgrade_transfers_cancellation_exactly_once() {
    for fail_cancel in [false, true] {
        let events = Events::default();
        let slot = UpgradeSlot::default();
        admit_source(Some(&slot), Some(empty_upgrade()), &events, fail_cancel).unwrap();
        let error = complete(
            slot.take()
                .unwrap()
                .serve::<MemoryIo<1>, _, _>(&Arc::default(), pending::<()>),
        )
        .unwrap_err();
        assert!(error.to_string().contains("Hyper upgrade failed"));
        assert_eq!(error.to_string().contains("cleanup failed"), fail_cancel);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].contains("Hyper upgrade failed"));
    }
}

fn upgraded<const KIND: u8>(
    events: &Events,
    frames: Vec<Message>,
) -> (PendingUpgrade<Source>, Arc<Mutex<State>>) {
    let mut client = WebSocket::from_raw_socket(Cursor::new(Vec::new()), Role::Client, None);
    for frame in frames {
        client.send(frame).unwrap();
    }
    let mut incoming = b"GET /ws?key=value HTTP/1.1\r\nHost: localhost\r\nConnection: upgrade\r\nUpgrade: websocket\r\n\r\n".to_vec();
    incoming.extend(client.into_inner().into_inner());
    let state = Arc::new(Mutex::new(State {
        incoming: incoming.into(),
        ..State::default()
    }));
    let slot = std::rc::Rc::new(UpgradeSlot::default());
    let service_slot = slot.clone();
    let events = events.clone();
    let service = service_fn(move |mut request| {
        admit_source(
            Some(&service_slot),
            Some(hyper::upgrade::on(&mut request)),
            &events,
            false,
        )
        .unwrap();
        async {
            Ok::<_, Infallible>(
                http::Response::builder()
                    .status(101)
                    .header("connection", "upgrade")
                    .header("upgrade", "websocket")
                    .body(Full::new(Bytes::new()))
                    .unwrap(),
            )
        }
    });
    complete(crate::http1::serve_connection(
        MemoryIo::<KIND>(state.clone()),
        service,
    ))
    .unwrap();
    assert!(state.lock().unwrap().outgoing.starts_with(b"HTTP/1.1 101"));
    (slot.take().unwrap(), state)
}

#[test]
fn both_transport_types_preserve_read_ahead_and_normal_close() {
    fn check<const KIND: u8>() {
        let events = Events::default();
        let (upgrade, state) =
            upgraded::<KIND>(&events, vec![Message::text("hello"), Message::Close(None)]);
        complete(upgrade.serve::<MemoryIo<KIND>, _, _>(&Arc::default(), pending::<()>)).unwrap();
        assert_eq!(
            events.lock().unwrap().as_slice(),
            ["writable", "text:hello", "close"]
        );
        assert_eq!(state.lock().unwrap().drops, 1);
    }
    check::<1>();
    check::<2>();
}

#[test]
fn foreign_transport_cancels_source_and_releases_io() {
    let events = Events::default();
    let (upgrade, state) = upgraded::<3>(&events, vec![]);
    let error =
        complete(upgrade.serve::<MemoryIo<1>, _, _>(&Arc::default(), pending::<()>)).unwrap_err();
    assert!(error.to_string().contains("unexpected transport type"));
    assert_eq!(events.lock().unwrap().len(), 1);
    assert!(events.lock().unwrap()[0].contains("unexpected transport type"));
    assert_eq!(state.lock().unwrap().drops, 1);
}

#[test]
fn dropping_running_handoff_cancels_without_duplicate_notification() {
    let events = Events::default();
    let (upgrade, state) = upgraded::<1>(&events, vec![]);
    let hub = Arc::default();
    let mut future = Box::pin(upgrade.serve::<MemoryIo<1>, _, _>(&hub, pending::<()>));
    assert!(future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
        .is_pending());
    drop(future);
    assert_eq!(
        events.lock().unwrap().as_slice(),
        ["writable", "cancel:websocket connection task dropped"]
    );
    assert_eq!(state.lock().unwrap().drops, 1);
}
