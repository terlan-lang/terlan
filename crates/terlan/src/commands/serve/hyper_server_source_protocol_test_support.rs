//! Shared production protocol transport for source-owned package integration tests.

use super::*;

/// Runs raw HTTP/1 requests through TLS, VM protocol actors and default workers.
pub(in crate::commands::serve) fn with_source_protocol_server(
    web: PathBuf,
    check: impl FnOnce(&dyn Fn(&str) -> String),
) {
    assert_ne!(
        std::env::var("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES").as_deref(),
        Ok("1"),
        "source integration must exercise the default external worker"
    );
    let (server_config, client_config) = tls_pair_with_client_alpn(b"http/1.1");
    let listener =
        crate::runtime::vm::protocol_task_executor::bind_protocol_listener("127.0.0.1", 0)
            .expect("bind source integration listener");
    let mut server = start_protocol_tasks_with_topology(
        listener,
        tls_factory(web, server_config, 4096, Arc::new(WebSocketHub::default())),
        VmSchedulerTopology::new(1).unwrap(),
    )
    .expect("start source integration protocol owners");
    let request = |wire: &str| {
        let tcp = std::net::TcpStream::connect(server.local_addr()).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        tcp.set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let connection = ClientConnection::new(
            Arc::clone(&client_config),
            ServerName::try_from("localhost").unwrap(),
        )
        .unwrap();
        let mut stream = StreamOwned::new(connection, tcp);
        std::io::Write::write_all(&mut stream, wire.as_bytes()).unwrap();
        let mut response = String::new();
        std::io::Read::read_to_string(&mut stream, &mut response).unwrap();
        response
    };
    check(&request);
    server
        .stop()
        .expect("stop source integration protocol owners");
}
