use super::VmHttpTcpServer;
use crate::runtime::vm::{
    memory::VmMemoryLimits,
    process::{VmProcessSource, VmProcessTable},
    tcp::VmTcpRuntime,
};

fn source() -> VmProcessSource {
    VmProcessSource::new("app.Http", "handle", 1)
}

fn request() -> Vec<u8> {
    b"GET / HTTP/1.1\r\nHost: vm.local\r\nContent-Length: 0\r\n\r\n".to_vec()
}

fn response() -> Result<http::Response<String>, String> {
    http::Response::builder()
        .status(200)
        .body("ok".to_string())
        .map_err(|error| error.to_string())
}

#[test]
fn http_server_accounts_generic_response_until_tcp_send_completes() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("response-memory.local").expect("listen");
    let client = tcp
        .connect("response-memory.local", "client")
        .expect("connect");
    tcp.send(client, request()).expect("request");
    let mut server = VmHttpTcpServer::with_response_memory_limits(
        listener,
        source(),
        VmMemoryLimits::new(1024, 2048).expect("limits"),
    );

    let poll = server
        .poll_keep_alive(&mut processes, &mut tcp, |_request| response())
        .expect("poll");
    assert_eq!(poll.completed, 1);
    let handler = server.handlers[0].process;
    let wire = tcp
        .receive(client, 4096)
        .expect("receive")
        .expect("response");
    let metrics = server
        .response_memory_metrics(handler)
        .expect("response memory metrics");
    assert_eq!(metrics.current_bytes, 0);
    assert_eq!(metrics.high_water_bytes, wire.len());
    assert_eq!(metrics.released_bytes, wire.len());
    assert_eq!(server.response_memory_reductions(handler), 4);
    assert_eq!(processes.get(handler).expect("handler").reductions, 5);
}

#[test]
fn http_server_rejects_generic_response_pressure_before_tcp_send() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("response-pressure.local").expect("listen");
    let client = tcp
        .connect("response-pressure.local", "client")
        .expect("connect");
    tcp.send(client, request()).expect("request");
    let mut server = VmHttpTcpServer::with_response_memory_limits(
        listener,
        source(),
        VmMemoryLimits::new(4, 8).expect("limits"),
    );

    let error = server
        .poll_keep_alive(&mut processes, &mut tcp, |_request| response())
        .expect_err("hard pressure");
    let handler = server.handlers[0].process;
    assert_eq!(
        error,
        format!(
            "VM HTTP handler process {} exceeded its response memory hard limit",
            handler.as_u64()
        )
    );
    assert_eq!(tcp.receive(client, 4096).expect("receive"), None);
    let metrics = server
        .response_memory_metrics(handler)
        .expect("response memory metrics");
    assert_eq!(metrics.current_bytes, 0);
    assert_eq!(metrics.high_water_bytes, 0);
    assert_eq!(server.response_memory_reductions(handler), 2);
    assert_eq!(processes.get(handler).expect("handler").reductions, 2);
}

#[test]
fn http_server_releases_generic_response_when_tcp_send_fails() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("response-failure.local").expect("listen");
    let client = tcp
        .connect("response-failure.local", "client")
        .expect("connect");
    tcp.send(client, request()).expect("request");
    tcp.close_stream(client).expect("close client");
    let mut server = VmHttpTcpServer::with_response_memory_limits(
        listener,
        source(),
        VmMemoryLimits::new(1024, 2048).expect("limits"),
    );

    assert_eq!(
        server
            .poll_keep_alive(&mut processes, &mut tcp, |_request| response())
            .expect_err("send failure"),
        "VM TCP peer stream is closed"
    );
    let handler = server.handlers[0].process;
    let metrics = server
        .response_memory_metrics(handler)
        .expect("response memory metrics");
    assert_eq!(metrics.current_bytes, 0);
    assert!(metrics.high_water_bytes > 0);
    assert_eq!(metrics.high_water_bytes, metrics.released_bytes);
    assert_eq!(server.response_memory_reductions(handler), 4);
    assert_eq!(processes.get(handler).expect("handler").reductions, 4);
}
