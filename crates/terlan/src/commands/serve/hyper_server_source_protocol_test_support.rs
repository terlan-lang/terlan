//! Shared production protocol transport for source-owned package integration tests.

use super::*;

/// Compiles one source project shared by immediate and socket transports.
pub(in crate::commands::serve) fn with_source_handler_project(
    source: &str,
    handlers: &[(&str, &str, usize)],
    check: impl FnOnce(&Path),
) {
    with_source_manifest_project(
        source,
        serde_json::json!({
            "handlers": handlers.iter().map(|(route, function, arity)| serde_json::json!({"method":"POST","route":route,"module":"app.Api",
                "function":function,"arity":arity,"source":{"path":"src/app/Api.terl","line":8,"column":5}})).collect::<Vec<_>>()
        }),
        check,
    );
}

pub(super) fn with_source_manifest_project(
    source: &str,
    routes: serde_json::Value,
    check: impl FnOnce(&Path),
) {
    let root = temp_web_root();
    let web = root.join("_build/web");
    std::fs::create_dir_all(root.join("src/app")).unwrap();
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"json_body_test\"\nversion = \"0.0.9\"\nnamespace = \"app\"\n",
    )
    .unwrap();
    std::fs::write(web.join("index.html"), "").unwrap();
    std::fs::write(root.join("src/app/Api.terl"), source).unwrap();
    let mut manifest = serde_json::json!({
        "schema":"terlan-web-build-v1", "target_profile":"js.browser",
        "source_js_manifest":"../js/manifest.json", "index":"index.html", "assets":[],
    });
    manifest
        .as_object_mut()
        .unwrap()
        .extend(routes.as_object().unwrap().clone());
    std::fs::write(
        web.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    crate::commands::serve::prewarm_dynamic_handler_sources(&web)
        .expect("compile source-owned library handler");
    check(&web);
    std::fs::remove_dir_all(root).unwrap();
}

/// Runs raw HTTP/1 requests through TLS, VM protocol actors and default workers.
pub(in crate::commands::serve) fn with_source_protocol_server(
    web: PathBuf,
    check: impl FnOnce(&dyn Fn(&str) -> String),
) {
    with_source_protocol_connections(web, |connect| {
        check(&|wire| {
            let mut stream = connect();
            std::io::Write::write_all(&mut stream, wire.as_bytes()).unwrap();
            let mut response = String::new();
            std::io::Read::read_to_string(&mut stream, &mut response).unwrap();
            response
        });
    });
}

pub(super) fn with_source_protocol_connections(
    web: PathBuf,
    check: impl FnOnce(&dyn Fn() -> StreamOwned<ClientConnection, std::net::TcpStream>),
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
    let connect = || {
        let tcp = std::net::TcpStream::connect(server.local_addr()).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        tcp.set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let connection = ClientConnection::new(
            Arc::clone(&client_config),
            ServerName::try_from("localhost").unwrap(),
        )
        .unwrap();
        StreamOwned::new(connection, tcp)
    };
    check(&connect);
    server
        .stop()
        .expect("stop source integration protocol owners");
}
