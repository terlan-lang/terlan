
#[test]
fn vm_tls_runtime_reports_missing_manual_private_key_file() {
    let (dir, cert_path, _key_path) = write_self_signed_cert_pair("missing_key_file");
    let missing_key_path = dir.join("missing-key.pem");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("missing-key.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(
            listener,
            manual_plan_with_paths(cert_path, missing_key_path.to_string_lossy().to_string()),
        )
        .expect("manual plan");

    let error = expect_server_config_error(
        runtime.build_listener_server_config(listener),
        "missing key should fail",
    );

    assert!(error.starts_with("VM TLS failed to open private key `"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_tls_runtime_reports_malformed_manual_private_key_file() {
    let (dir, cert_path, key_path) = write_self_signed_cert_pair("malformed_key_file");
    fs::write(
        &key_path,
        "-----BEGIN PRIVATE KEY-----\n!\n-----END PRIVATE KEY-----\n",
    )
    .expect("write malformed key");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("malformed-key.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, manual_plan_with_paths(cert_path, key_path))
        .expect("manual plan");

    let error = expect_server_config_error(
        runtime.build_listener_server_config(listener),
        "malformed key should fail",
    );

    assert!(error.starts_with("VM TLS failed to parse private key `"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_tls_runtime_reports_manual_private_key_without_supported_key() {
    let (dir, cert_path, key_path) = write_self_signed_cert_pair("unsupported_key_file");
    fs::write(&key_path, "not a key").expect("write unsupported key");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("unsupported-key.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, manual_plan_with_paths(cert_path, key_path))
        .expect("manual plan");

    let error = expect_server_config_error(
        runtime.build_listener_server_config(listener),
        "unsupported key should fail",
    );

    assert!(error.contains("did not contain a supported unencrypted PEM key"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_tls_runtime_starts_manual_server_connection_with_readiness_state() {
    let (dir, cert_path, key_path) = write_self_signed_cert_pair("manual_connection");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("manual-connection.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, manual_plan_with_paths(cert_path, key_path))
        .expect("manual plan");

    let connection = runtime
        .start_listener_server_connection(listener)
        .expect("manual server connection should start");
    let info = connection.inspect();

    assert_eq!(info.mode, VmTlsMode::Manual);
    assert!(info.handshaking);
    assert!(info.wants_read);
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_tls_runtime_starts_internal_server_connection_with_readiness_state() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("internal-connection.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, internal_plan())
        .expect("internal plan");

    let connection = runtime
        .start_listener_server_connection(listener)
        .expect("internal server connection should start");
    let info = connection.inspect();

    assert_eq!(info.mode, VmTlsMode::Internal);
    assert!(info.handshaking);
    assert!(info.wants_read);
}

#[test]
fn vm_tls_runtime_rejects_plaintext_server_connection_start() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("plain-connection.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, plain_plan())
        .expect("plain plan");

    let error = expect_server_connection_error(
        runtime.start_listener_server_connection(listener),
        "plaintext listener should not start TLS connection",
    );

    assert_eq!(
        error,
        "VM TLS plaintext listener does not require a server config"
    );
}

#[test]
fn vm_tls_runtime_rejects_auto_server_connection_without_cache() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("auto-connection.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, auto_plan())
        .expect("auto plan");

    let error = expect_server_connection_error(
        runtime.start_listener_server_connection(listener),
        "auto listener without cache should not start TLS connection",
    );

    assert_eq!(
        error,
        "VM TLS auto mode requires ACME certificate cache before server config"
    );
}

#[test]
fn vm_tls_server_connection_roundtrips_handshake_and_plaintext() {
    let (dir, cert_path, key_path, cert_der) = write_self_signed_cert_pair_with_der("byte_pump");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("byte-pump.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, manual_plan_with_paths(cert_path, key_path))
        .expect("manual plan");

    let mut server = runtime
        .start_listener_server_connection(listener)
        .expect("manual server connection should start");
    let mut client = tls_client_for_cert(cert_der);

    complete_tls_handshake(&mut client, &mut server);
    assert!(!server.inspect().handshaking);
    assert!(!client.is_handshaking());

    client
        .writer()
        .write_all(b"ping")
        .expect("client writes plaintext");
    pump_client_to_server(&mut client, &mut server);
    assert_eq!(
        server.read_plaintext().expect("server reads plaintext"),
        b"ping".to_vec()
    );

    let written = server
        .write_plaintext(b"pong")
        .expect("server writes plaintext");
    assert_eq!(written, 4);
    pump_server_to_client(&mut server, &mut client);
    let mut response = [0; 8];
    let read = client
        .reader()
        .read(&mut response)
        .expect("client reads plaintext");
    assert_eq!(&response[..read], b"pong");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_tls_server_connection_reports_malformed_tls_packets() {
    let (dir, cert_path, key_path) = write_self_signed_cert_pair("malformed_packet");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("malformed-packet.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, manual_plan_with_paths(cert_path, key_path))
        .expect("manual plan");
    let mut server = runtime
        .start_listener_server_connection(listener)
        .expect("manual server connection should start");

    server
        .read_tls_bytes(b"not a tls record")
        .expect("in-memory read accepts bytes");
    let error = server
        .process_new_packets()
        .expect_err("malformed TLS should fail");

    assert!(error.starts_with("VM TLS failed to process TLS packets:"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_tls_plaintext_reader_handles_eof_would_block_and_errors() {
    let mut eof_reader = plaintext_reader(vec![Ok(b"abc"), Ok(b"")]);
    assert_eq!(
        super::read_plaintext_from(&mut eof_reader).expect("read until eof"),
        b"abc".to_vec()
    );

    let mut would_block_reader = plaintext_reader(vec![Ok(b"abc"), Err(ErrorKind::WouldBlock)]);
    assert_eq!(
        super::read_plaintext_from(&mut would_block_reader).expect("read until would block"),
        b"abc".to_vec()
    );

    let mut error_reader = plaintext_reader(vec![Err(ErrorKind::ConnectionReset)]);
    assert_eq!(
        super::read_plaintext_from(&mut error_reader).expect_err("hard read error should fail"),
        "VM TLS failed to read plaintext: scripted plaintext read failure"
    );
}

#[test]
fn vm_tls_plaintext_writer_reports_written_bytes_and_errors() {
    let mut ok_writer = ScriptedPlaintextWriter { result: Ok(4) };
    assert_eq!(
        super::write_plaintext_to(&mut ok_writer, b"pong").expect("plaintext write"),
        4
    );

    let mut error_writer = ScriptedPlaintextWriter {
        result: Err(ErrorKind::BrokenPipe),
    };
    assert_eq!(
        super::write_plaintext_to(&mut error_writer, b"pong")
            .expect_err("hard write error should fail"),
        "VM TLS failed to write plaintext: scripted plaintext write failure"
    );
}

#[test]
fn vm_tls_tcp_server_stream_flushes_pending_tls_records_without_new_tcp_bytes() {
    let (dir, cert_path, key_path, cert_der) =
        write_self_signed_cert_pair_with_der("pending_write");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("pending-write.local").expect("listener");
    let client_stream = tcp
        .connect("pending-write.local", "tls_client")
        .expect("client stream");
    let server_stream = tcp
        .accept(listener, "tls_server")
        .expect("accept")
        .expect("server stream");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, manual_plan_with_paths(cert_path, key_path))
        .expect("manual plan");
    let mut connection = runtime
        .start_listener_server_connection(listener)
        .expect("manual server connection should start");
    let mut client = tls_client_for_cert(cert_der);

    let mut client_hello = Vec::new();
    client
        .write_tls(&mut client_hello)
        .expect("client writes hello");
    connection
        .read_tls_bytes(&client_hello)
        .expect("server reads hello");
    connection
        .process_new_packets()
        .expect("server processes hello");

    let mut server = VmTlsTcpServerStream::new(server_stream, connection);
    assert_eq!(server.poll_state(), VmTlsTcpPoll::Handshaking);
    assert_eq!(
        server.poll(&mut tcp).expect("server flushes pending write"),
        VmTlsTcpPoll::NeedRead
    );
    assert!(
        tcp.inspect_stream(client_stream)
            .expect("client stream info")
            .queued_bytes
            > 0
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_tls_tcp_server_stream_roundtrips_over_vm_tcp_runtime() {
    let (dir, cert_path, key_path, cert_der) =
        write_self_signed_cert_pair_with_der("tcp_byte_pump");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("tcp-byte-pump.local").expect("listener");
    let client_stream = tcp
        .connect("tcp-byte-pump.local", "tls_client")
        .expect("client stream");
    let server_stream = tcp
        .accept(listener, "tls_server")
        .expect("accept")
        .expect("server stream");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, manual_plan_with_paths(cert_path, key_path))
        .expect("manual plan");
    let connection = runtime
        .start_listener_server_connection(listener)
        .expect("manual server connection should start");
    let mut server = VmTlsTcpServerStream::new(server_stream, connection);
    let mut client = tls_client_for_cert(cert_der);

    assert_eq!(server.stream(), server_stream);
    complete_tls_tcp_handshake(&mut client, &mut tcp, client_stream, &mut server);
    assert_eq!(
        server.poll(&mut tcp).expect("post-handshake poll"),
        VmTlsTcpPoll::Ready
    );

    client
        .writer()
        .write_all(b"GET /secure HTTP/1.1\r\nHost: tls.local\r\n\r\n")
        .expect("client writes plaintext request");
    flush_client_tls_to_tcp(&mut client, &mut tcp, client_stream);
    assert_eq!(
        server
            .poll(&mut tcp)
            .expect("server polls encrypted request"),
        VmTlsTcpPoll::Ready
    );
    assert_eq!(
        server
            .read_plaintext()
            .expect("server reads decrypted request"),
        b"GET /secure HTTP/1.1\r\nHost: tls.local\r\n\r\n".to_vec()
    );

    let written = server
        .write_plaintext(&mut tcp, b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
        .expect("server writes plaintext response");
    assert_eq!(written, 40);
    pump_tcp_to_client(&mut tcp, client_stream, &mut client);
    let mut response = [0; 64];
    let read = client
        .reader()
        .read(&mut response)
        .expect("client reads decrypted response");
    assert_eq!(
        &response[..read],
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_tls_runtime_rejects_invalid_mode_specific_plans() {
    let mut runtime = VmTlsRuntime::new();
    let mut plain = plain_plan();
    plain.server_name = Some("localhost".to_string());
    let mut manual_without_cert = manual_plan();
    manual_without_cert.cert_path = Some(" ".to_string());
    let mut manual = manual_plan();
    manual.key_path = None;
    let mut manual_with_acme = manual_plan();
    manual_with_acme.primary_provider = Some(VmTlsProvider::LetsEncrypt);
    let mut internal = internal_plan();
    internal.server_name = None;
    let mut internal_with_acme = internal_plan();
    internal_with_acme.domains = vec!["terlan.local".to_string()];
    let mut auto = auto_plan();
    auto.domains.clear();
    let mut auto_without_provider = auto_plan();
    auto_without_provider.primary_provider = None;
    let mut auto_with_manual_cert = auto_plan();
    auto_with_manual_cert.cert_path = Some("cert.pem".to_string());
    let mut internal_with_manual_cert = internal_plan();
    internal_with_manual_cert.cert_path = Some("cert.pem".to_string());

    assert_eq!(
        runtime
            .install_plan("plain", plain)
            .expect_err("plain fields should fail"),
        "VM TLS plain mode cannot include TLS configuration fields"
    );
    assert_eq!(
        runtime
            .install_plan("manual-without-cert", manual_without_cert)
            .expect_err("manual cert should fail"),
        "VM TLS manual mode requires cert_path"
    );
    assert_eq!(
        runtime
            .install_plan("manual", manual)
            .expect_err("manual key should fail"),
        "VM TLS manual mode requires key_path"
    );
    assert_eq!(
        runtime
            .install_plan("manual-with-acme", manual_with_acme)
            .expect_err("manual acme should fail"),
        "VM TLS manual mode cannot include ACME provider fields"
    );
    assert_eq!(
        runtime
            .install_plan("internal", internal)
            .expect_err("internal server name should fail"),
        "VM TLS internal mode requires server_name"
    );
    assert_eq!(
        runtime
            .install_plan("internal-with-acme", internal_with_acme)
            .expect_err("internal acme should fail"),
        "VM TLS internal mode cannot include ACME provider fields"
    );
    assert_eq!(
        runtime
            .install_plan("auto", auto)
            .expect_err("auto domains should fail"),
        "VM TLS auto mode requires non-empty domains"
    );
    assert_eq!(
        runtime
            .install_plan("auto-without-provider", auto_without_provider)
            .expect_err("auto provider should fail"),
        "VM TLS auto mode requires a primary provider"
    );
    assert_eq!(
        runtime
            .install_plan("auto-with-manual-cert", auto_with_manual_cert)
            .expect_err("auto manual cert should fail"),
        "VM TLS auto mode cannot include manual certificate fields"
    );
    assert_eq!(
        runtime
            .install_plan("internal-with-cert", internal_with_manual_cert)
            .expect_err("internal manual cert should fail"),
        "VM TLS internal mode cannot include manual certificate fields"
    );
}

#[test]
fn vm_tls_runtime_rejects_empty_listener_names() {
    let mut runtime = VmTlsRuntime::new();

    assert_eq!(
        runtime
            .install_plan(" ", plain_plan())
            .expect_err("empty listener should fail"),
        "VM TLS listener name cannot be empty"
    );
}
