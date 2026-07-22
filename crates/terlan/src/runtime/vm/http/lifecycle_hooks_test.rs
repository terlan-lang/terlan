use std::sync::{Arc, Mutex};

use super::{
    VmHttpLifecycleEvent, VmHttpLifecycleHook, VmHttpRequestOutcome, VmHttpShutdownMode,
    VmHttpTcpServer,
};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessSource, VmProcessTable},
    tcp::VmTcpRuntime,
};

struct RecordingHook {
    events: Arc<Mutex<Vec<VmHttpLifecycleEvent>>>,
    reject_request: bool,
    reject_drain: bool,
    reject_channel: bool,
}

impl VmHttpLifecycleHook for RecordingHook {
    fn authorize(&mut self, event: &VmHttpLifecycleEvent) -> Result<(), String> {
        if self.reject_request && matches!(event, VmHttpLifecycleEvent::RequestStart { .. }) {
            return Err("request rejected by VM HTTP lifecycle policy".to_string());
        }
        if self.reject_drain
            && matches!(
                event,
                VmHttpLifecycleEvent::ShutdownHandoff {
                    mode: VmHttpShutdownMode::Drain,
                    ..
                }
            )
        {
            return Err("drain rejected by VM HTTP lifecycle policy".to_string());
        }
        if self.reject_channel && matches!(event, VmHttpLifecycleEvent::ChannelBind { .. }) {
            return Err("channel rejected by VM HTTP lifecycle policy".to_string());
        }
        Ok(())
    }

    fn observe(&mut self, event: &VmHttpLifecycleEvent) -> Result<(), String> {
        self.events
            .lock()
            .expect("event recorder should lock")
            .push(event.clone());
        Ok(())
    }
}

fn server_with_hook(
    tcp: &mut VmTcpRuntime,
    address: &str,
    events: Arc<Mutex<Vec<VmHttpLifecycleEvent>>>,
    reject_request: bool,
    reject_drain: bool,
    reject_channel: bool,
) -> VmHttpTcpServer {
    let listener = tcp.listen(address).expect("listener should bind");
    let mut server = VmHttpTcpServer::new(listener, VmProcessSource::new("app.Http", "handle", 1));
    server.install_lifecycle_hook(RecordingHook {
        events,
        reject_request,
        reject_drain,
        reject_channel,
    });
    server
}

#[test]
fn vm_http_lifecycle_hook_observes_ordered_worker_request_channel_and_shutdown_events() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let mut server = server_with_hook(&mut tcp, "hooks.local", events.clone(), false, false, false);
    let client = tcp.connect("hooks.local", "client").expect("connect");
    tcp.send(
        client,
        b"GET /sum HTTP/1.1\r\nHost: hooks.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("request should send");

    let report = server
        .poll(&mut processes, &mut tcp, |_request| {
            http::Response::builder()
                .status(201)
                .body("created".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("request should complete");
    assert_eq!(report.completed, 1);
    server
        .shutdown(&mut processes, &mut tcp, VmExitReason::Killed)
        .expect("empty server should shut down");

    let recorded = events.lock().expect("event recorder should lock");
    assert_eq!(recorded.len(), 6);
    let (process, stream) = match (&recorded[0], &recorded[1]) {
        (
            VmHttpLifecycleEvent::WorkerStart { process },
            VmHttpLifecycleEvent::ChannelBind {
                process: bound,
                stream,
            },
        ) => {
            assert_eq!(process, bound);
            (*process, *stream)
        }
        events => panic!("unexpected worker lifecycle prefix: {events:?}"),
    };
    assert_eq!(
        recorded[2],
        VmHttpLifecycleEvent::RequestStart {
            process,
            method: "GET".to_string(),
            path: "/sum".to_string(),
        }
    );
    assert_eq!(
        recorded[3],
        VmHttpLifecycleEvent::RequestEnd {
            process,
            method: "GET".to_string(),
            path: "/sum".to_string(),
            outcome: VmHttpRequestOutcome::Response { status: 201 },
        }
    );
    assert_eq!(
        recorded[4],
        VmHttpLifecycleEvent::ChannelUnbind {
            process,
            stream,
            reason: VmExitReason::Normal,
        }
    );
    assert_eq!(
        recorded[5],
        VmHttpLifecycleEvent::ShutdownHandoff {
            mode: VmHttpShutdownMode::Immediate,
            active_handlers: 0,
        }
    );
}

#[test]
fn vm_http_lifecycle_hook_rejects_request_before_handler_execution() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let mut server = server_with_hook(&mut tcp, "policy.local", events.clone(), true, false, false);
    let client = tcp.connect("policy.local", "client").expect("connect");
    tcp.send(
        client,
        b"GET /private HTTP/1.1\r\nHost: policy.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("request should send");

    let error = server
        .poll(&mut processes, &mut tcp, |_request| {
            panic!("rejected request must not execute its handler")
        })
        .expect_err("request policy should reject dispatch");
    assert_eq!(error, "request rejected by VM HTTP lifecycle policy");
    assert_eq!(events.lock().expect("event recorder should lock").len(), 2);
    server
        .shutdown(&mut processes, &mut tcp, VmExitReason::Killed)
        .expect("rejected handler should clean up");
    assert!(processes.live_process_ids().is_empty());
}

#[test]
fn vm_http_lifecycle_hook_can_reject_drain_without_closing_listener() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut tcp = VmTcpRuntime::new();
    let mut server = server_with_hook(
        &mut tcp,
        "drain-policy.local",
        events.clone(),
        false,
        true,
        false,
    );

    assert_eq!(
        server
            .begin_drain(&mut tcp, 4)
            .expect_err("drain policy should reject handoff"),
        "drain rejected by VM HTTP lifecycle policy"
    );
    let client = tcp
        .connect("drain-policy.local", "still-open")
        .expect("rejected drain must leave listener open");
    assert!(events
        .lock()
        .expect("event recorder should lock")
        .is_empty());

    let mut processes = VmProcessTable::default();
    server
        .shutdown(&mut processes, &mut tcp, VmExitReason::Killed)
        .expect("immediate shutdown must remain non-vetoable");
    tcp.close_stream(client).expect("client should close");
    assert_eq!(tcp.metrics().open_streams, 0);
}

#[test]
fn vm_http_lifecycle_hook_channel_rejection_rolls_back_process_and_stream() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let mut server = server_with_hook(
        &mut tcp,
        "channel-policy.local",
        events.clone(),
        false,
        false,
        true,
    );
    let client = tcp
        .connect("channel-policy.local", "client")
        .expect("connect");

    assert_eq!(
        server
            .poll(&mut processes, &mut tcp, |_request| {
                panic!("rejected channel cannot dispatch a request")
            })
            .expect_err("channel policy should reject admission"),
        "channel rejected by VM HTTP lifecycle policy"
    );
    assert!(events
        .lock()
        .expect("event recorder should lock")
        .is_empty());
    assert!(processes.live_process_ids().is_empty());
    assert_eq!(server.active_handlers(), 0);
    assert_eq!(
        tcp.send(client, b"late".to_vec())
            .expect_err("rolled-back server stream must close its peer"),
        "VM TCP peer stream is closed"
    );
    tcp.close_stream(client).expect("client should close");
    assert_eq!(tcp.metrics().open_streams, 0);
}
