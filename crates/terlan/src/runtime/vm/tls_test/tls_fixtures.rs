use std::fs;
use std::io::{Cursor, Error, ErrorKind, Read, Write};
use std::sync::Arc;

use rcgen::generate_simple_self_signed;
use rustls::ClientConnection;

use crate::runtime::vm::tcp::{VmTcpRuntime, VmTcpStream};
use crate::support::test_fs;

use super::super::{
    VmTlsMode, VmTlsPlan, VmTlsProvider, VmTlsRuntime, VmTlsTcpServerStream, VmTlsTransportMode,
};

pub(super) fn plain_plan() -> VmTlsPlan {
    crate::runtime::vm::tls_test_support::plain_plan()
}

pub(super) fn manual_plan() -> VmTlsPlan {
    VmTlsPlan {
        mode: VmTlsMode::Manual,
        domains: Vec::new(),
        email: None,
        primary_provider: None,
        fallback_provider: None,
        cert_path: Some("cert.pem".to_string()),
        key_path: Some("key.pem".to_string()),
        passphrase_env: None,
        ca_path: Some("ca.pem".to_string()),
        server_name: None,
        trust_local: None,
    }
}

pub(super) fn manual_plan_with_paths(cert_path: String, key_path: String) -> VmTlsPlan {
    crate::runtime::vm::tls_test_support::manual_plan_with_paths(cert_path, key_path)
}

pub(super) fn internal_plan() -> VmTlsPlan {
    VmTlsPlan {
        mode: VmTlsMode::Internal,
        domains: Vec::new(),
        email: None,
        primary_provider: None,
        fallback_provider: None,
        cert_path: None,
        key_path: None,
        passphrase_env: None,
        ca_path: None,
        server_name: Some("localhost".to_string()),
        trust_local: Some(true),
    }
}

pub(super) fn write_self_signed_cert_pair(name: &str) -> (std::path::PathBuf, String, String) {
    let dir = test_fs::temp_path("vm_tls", name);
    fs::create_dir_all(&dir).expect("create TLS fixture dir");
    let generated =
        generate_simple_self_signed(vec!["localhost".to_string()]).expect("generate cert");
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    fs::write(&cert_path, generated.cert.pem()).expect("write cert");
    fs::write(&key_path, generated.key_pair.serialize_pem()).expect("write key");
    (
        dir,
        cert_path.to_string_lossy().to_string(),
        key_path.to_string_lossy().to_string(),
    )
}

pub(super) fn write_self_signed_cert_pair_with_der(
    name: &str,
) -> (std::path::PathBuf, String, String, Vec<u8>) {
    let dir = test_fs::temp_path("vm_tls", name);
    fs::create_dir_all(&dir).expect("create TLS fixture dir");
    let generated =
        generate_simple_self_signed(vec!["localhost".to_string()]).expect("generate cert");
    let cert_der = generated.cert.der().as_ref().to_vec();
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    fs::write(&cert_path, generated.cert.pem()).expect("write cert");
    fs::write(&key_path, generated.key_pair.serialize_pem()).expect("write key");
    (
        dir,
        cert_path.to_string_lossy().to_string(),
        key_path.to_string_lossy().to_string(),
        cert_der,
    )
}

pub(super) fn tls_client_for_cert(cert_der: Vec<u8>) -> ClientConnection {
    crate::runtime::vm::tls_test_support::client_for_cert(cert_der)
}

pub(super) fn pump_client_to_server(
    client: &mut ClientConnection,
    server: &mut super::super::VmTlsServerConnection,
) {
    let mut bytes = Vec::new();
    client.write_tls(&mut bytes).expect("client writes TLS");
    if bytes.is_empty() {
        return;
    }
    let consumed = server
        .read_tls_bytes(&bytes)
        .expect("server reads client TLS bytes");
    assert_eq!(consumed, bytes.len());
    server
        .process_new_packets()
        .expect("server processes TLS packets");
}

pub(super) fn pump_server_to_client(
    server: &mut super::super::VmTlsServerConnection,
    client: &mut ClientConnection,
) {
    let bytes = server.write_tls_bytes().expect("server writes TLS bytes");
    if bytes.is_empty() {
        return;
    }
    let consumed = client
        .read_tls(&mut Cursor::new(&bytes))
        .expect("client reads server TLS bytes");
    assert_eq!(consumed, bytes.len());
    client
        .process_new_packets()
        .expect("client processes TLS packets");
}

pub(super) fn complete_tls_handshake(
    client: &mut ClientConnection,
    server: &mut super::super::VmTlsServerConnection,
) {
    for _ in 0..10 {
        if !client.is_handshaking() && !server.inspect().handshaking {
            return;
        }
        pump_client_to_server(client, server);
        pump_server_to_client(server, client);
    }
    panic!("TLS handshake did not complete");
}

pub(super) fn flush_client_tls_to_tcp(
    client: &mut ClientConnection,
    tcp: &mut VmTcpRuntime,
    client_stream: VmTcpStream,
) {
    let mut bytes = Vec::new();
    client.write_tls(&mut bytes).expect("client writes TLS");
    if !bytes.is_empty() {
        tcp.send(client_stream, bytes)
            .expect("client sends TLS over VM TCP");
    }
}

pub(super) fn pump_tcp_to_client(
    tcp: &mut VmTcpRuntime,
    client_stream: VmTcpStream,
    client: &mut ClientConnection,
) {
    crate::runtime::vm::tls_test_support::pump_tcp_to_client(tcp, client_stream, client);
}

pub(super) fn complete_tls_tcp_handshake(
    client: &mut ClientConnection,
    tcp: &mut VmTcpRuntime,
    client_stream: VmTcpStream,
    server: &mut VmTlsTcpServerStream,
) {
    flush_client_tls_to_tcp(client, tcp, client_stream);
    for _ in 0..10 {
        let _ = server.poll(tcp).expect("server polls TLS over VM TCP");
        pump_tcp_to_client(tcp, client_stream, client);
        flush_client_tls_to_tcp(client, tcp, client_stream);
        if !client.is_handshaking() && !server.inspect().handshaking {
            return;
        }
    }
    panic!("TLS VM TCP handshake did not complete");
}

pub(super) fn auto_plan() -> VmTlsPlan {
    VmTlsPlan {
        mode: VmTlsMode::Auto,
        domains: vec!["terlan.local".to_string()],
        email: Some("ops@terlan.local".to_string()),
        primary_provider: Some(VmTlsProvider::LetsEncrypt),
        fallback_provider: Some(VmTlsProvider::ZeroSsl),
        cert_path: None,
        key_path: None,
        passphrase_env: None,
        ca_path: None,
        server_name: None,
        trust_local: None,
    }
}

pub(super) fn expect_server_config_error(
    result: Result<super::super::VmTlsServerConfig, String>,
    message: &str,
) -> String {
    result.expect_err(message)
}

pub(super) fn expect_server_connection_error(
    result: Result<super::super::VmTlsServerConnection, String>,
    message: &str,
) -> String {
    result.expect_err(message)
}

pub(super) struct ScriptedPlaintextReader {
    steps: Vec<Result<Vec<u8>, ErrorKind>>,
}

pub(super) struct ScriptedPlaintextWriter {
    pub(super) result: Result<usize, ErrorKind>,
}

impl Read for ScriptedPlaintextReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let Some(step) = self.steps.pop() else {
            return Ok(0);
        };
        match step {
            Ok(bytes) => {
                let len = bytes.len().min(buffer.len());
                buffer[..len].copy_from_slice(&bytes[..len]);
                Ok(len)
            }
            Err(kind) => Err(Error::new(kind, "scripted plaintext read failure")),
        }
    }
}

impl Write for ScriptedPlaintextWriter {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        self.result
            .map_err(|kind| Error::new(kind, "scripted plaintext write failure"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn plaintext_reader(steps: Vec<Result<&[u8], ErrorKind>>) -> ScriptedPlaintextReader {
    ScriptedPlaintextReader {
        steps: steps
            .into_iter()
            .rev()
            .map(|step| step.map(|bytes| bytes.to_vec()))
            .collect(),
    }
}

#[test]
pub(super) fn vm_tls_runtime_installs_and_inspects_plain_plan() {
    let mut runtime = VmTlsRuntime::new();
    let plan = plain_plan();

    runtime
        .install_plan("http.local", plan.clone())
        .expect("plain plan should install");

    assert_eq!(runtime.inspect_plan("http.local"), Some(&plan));
    assert_eq!(runtime.inspect_plan("missing"), None);
}

#[test]
pub(super) fn vm_tls_runtime_accepts_manual_internal_and_auto_modes() {
    let mut runtime = VmTlsRuntime::new();

    runtime
        .install_plan("manual", manual_plan())
        .expect("manual plan should install");
    runtime
        .install_plan("internal", internal_plan())
        .expect("internal plan should install");
    runtime
        .install_plan("auto", auto_plan())
        .expect("auto plan should install");

    assert_eq!(
        runtime.inspect_plan("manual").expect("manual").mode,
        VmTlsMode::Manual
    );
    assert_eq!(
        runtime.inspect_plan("internal").expect("internal").mode,
        VmTlsMode::Internal
    );
    assert_eq!(
        runtime.inspect_plan("auto").expect("auto").primary_provider,
        Some(VmTlsProvider::LetsEncrypt)
    );
}

#[test]
pub(super) fn vm_tls_runtime_binds_plan_to_tcp_listener_handle() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("https.local").expect("listener");
    let other_listener = tcp.listen("http.local").expect("other listener");
    let mut runtime = VmTlsRuntime::new();
    let plan = internal_plan();

    runtime
        .install_listener_plan(listener, plan.clone())
        .expect("listener TLS plan should install");

    assert_eq!(runtime.inspect_listener_plan(listener), Some(&plan));
    assert_eq!(runtime.inspect_listener_plan(other_listener), None);
}

#[test]
pub(super) fn vm_tls_runtime_rejects_invalid_tcp_listener_plan() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("https.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    let mut invalid = auto_plan();
    invalid.primary_provider = None;

    assert_eq!(
        runtime
            .install_listener_plan(listener, invalid)
            .expect_err("invalid listener plan should fail"),
        "VM TLS auto mode requires a primary provider"
    );
    assert_eq!(runtime.inspect_listener_plan(listener), None);
}

#[test]
pub(super) fn vm_tls_runtime_removes_tcp_listener_plan_on_shutdown() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("https.local").expect("listener");
    let other_listener = tcp.listen("https-alt.local").expect("other listener");
    let mut runtime = VmTlsRuntime::new();
    let plan = manual_plan();

    runtime
        .install_listener_plan(listener, plan.clone())
        .expect("listener TLS plan should install");

    assert_eq!(runtime.remove_listener_plan(other_listener), None);
    assert_eq!(runtime.remove_listener_plan(listener), Some(plan));
    assert_eq!(runtime.inspect_listener_plan(listener), None);
    assert_eq!(runtime.remove_listener_plan(listener), None);
}

#[test]
pub(super) fn vm_tls_runtime_enforces_rotation_overlap_window_before_retiring_old_config() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("rotate.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    let (dir, cert_path, key_path) = write_self_signed_cert_pair("rotation_overlap");

    runtime
        .install_listener_plan(listener, internal_plan())
        .expect("initial listener TLS plan should install");
    let window = runtime
        .rotate_listener_plan(
            listener,
            manual_plan_with_paths(cert_path, key_path),
            100,
            50,
        )
        .expect("rotation should install replacement plan");

    assert_eq!(window.listener, listener);
    assert_eq!(window.previous_mode, VmTlsMode::Internal);
    assert_eq!(window.replacement_mode, VmTlsMode::Manual);
    assert_eq!(window.started_at_tick, 100);
    assert_eq!(window.retire_after_tick, 150);
    assert_eq!(
        runtime
            .inspect_listener_plan(listener)
            .expect("active plan")
            .mode,
        VmTlsMode::Manual
    );
    assert_eq!(runtime.inspect_rotation_window(listener), Some(&window));
    assert!(runtime
        .retire_rotation_window(listener, 149)
        .expect_err("overlap should not retire early")
        .contains("cannot retire until 150"));
    assert_eq!(
        runtime
            .retire_rotation_window(listener, 150)
            .expect("overlap can retire"),
        Some(window)
    );
    assert_eq!(runtime.inspect_rotation_window(listener), None);
    assert_eq!(
        runtime
            .retire_rotation_window(listener, 151)
            .expect("already retired"),
        None
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_tls_runtime_rejects_zero_rotation_overlap() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("rotate-zero.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, internal_plan())
        .expect("initial listener TLS plan should install");

    let error = runtime
        .rotate_listener_plan(listener, internal_plan(), 100, 0)
        .expect_err("zero overlap should fail");

    assert_eq!(error, "VM TLS rotation overlap must be positive");
}

#[test]
pub(super) fn vm_tls_runtime_rejects_rotation_without_installed_listener_plan() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("rotate-missing.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();

    assert_eq!(
        runtime
            .rotate_listener_plan(listener, internal_plan(), 100, 10)
            .expect_err("rotation requires an installed plan"),
        "VM TLS listener handle has no installed transport plan"
    );
}

#[test]
pub(super) fn vm_tls_runtime_hot_rotation_keeps_existing_connection_mode_for_old_accepts() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("hot-rotate.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    let (dir, cert_path, key_path) = write_self_signed_cert_pair("hot_rotation");

    runtime
        .install_listener_plan(listener, internal_plan())
        .expect("initial listener TLS plan should install");
    let old_connection = runtime
        .start_listener_server_connection(listener)
        .expect("old accepted connection should start");
    runtime
        .rotate_listener_plan(
            listener,
            manual_plan_with_paths(cert_path, key_path),
            100,
            50,
        )
        .expect("rotation should install replacement plan");
    let new_connection = runtime
        .start_listener_server_connection(listener)
        .expect("new accepted connection should start");

    assert_eq!(old_connection.inspect().mode, VmTlsMode::Internal);
    assert_eq!(new_connection.inspect().mode, VmTlsMode::Manual);
    assert_eq!(
        runtime
            .inspect_listener_plan(listener)
            .expect("active plan")
            .mode,
        VmTlsMode::Manual
    );
    assert!(runtime.inspect_rotation_window(listener).is_some());
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_tls_runtime_reports_listener_transport_mode() {
    let mut tcp = VmTcpRuntime::new();
    let plain_listener = tcp.listen("plain.local").expect("plain listener");
    let manual_listener = tcp.listen("manual.local").expect("manual listener");
    let internal_listener = tcp.listen("internal.local").expect("internal listener");
    let auto_listener = tcp.listen("auto.local").expect("auto listener");
    let mut runtime = VmTlsRuntime::new();

    runtime
        .install_listener_plan(plain_listener, plain_plan())
        .expect("plain plan");
    runtime
        .install_listener_plan(manual_listener, manual_plan())
        .expect("manual plan");
    runtime
        .install_listener_plan(internal_listener, internal_plan())
        .expect("internal plan");
    runtime
        .install_listener_plan(auto_listener, auto_plan())
        .expect("auto plan");

    assert_eq!(
        runtime.listener_transport_mode(plain_listener),
        Ok(VmTlsTransportMode::Plaintext)
    );
    assert_eq!(
        runtime.listener_transport_mode(manual_listener),
        Ok(VmTlsTransportMode::Tls)
    );
    assert_eq!(
        runtime.listener_transport_mode(internal_listener),
        Ok(VmTlsTransportMode::Tls)
    );
    assert_eq!(
        runtime.listener_transport_mode(auto_listener),
        Ok(VmTlsTransportMode::Tls)
    );
}

#[test]
pub(super) fn vm_tls_runtime_reports_missing_listener_transport_plan() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("missing.local").expect("listener");
    let runtime = VmTlsRuntime::new();

    assert_eq!(
        runtime
            .listener_transport_mode(listener)
            .expect_err("missing plan should fail"),
        "VM TLS listener handle has no installed transport plan"
    );
}

#[test]
pub(super) fn vm_tls_runtime_reports_missing_listener_server_config_and_connection() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("missing-config.local").expect("listener");
    let runtime = VmTlsRuntime::new();

    assert_eq!(
        expect_server_config_error(
            runtime.build_listener_server_config(listener),
            "missing config should fail",
        ),
        "VM TLS listener handle has no installed transport plan"
    );
    assert_eq!(
        expect_server_connection_error(
            runtime.start_listener_server_connection(listener),
            "missing connection config should fail",
        ),
        "VM TLS listener handle has no installed transport plan"
    );
}

#[test]
pub(super) fn vm_tls_manual_and_internal_config_builders_report_missing_fields() {
    let mut manual = manual_plan();
    manual.cert_path = None;
    assert_eq!(
        expect_server_config_error(
            super::super::build_manual_server_config(&manual),
            "manual builder should require cert",
        ),
        "VM TLS manual mode requires cert_path"
    );

    let mut manual = manual_plan();
    manual.key_path = None;
    assert_eq!(
        expect_server_config_error(
            super::super::build_manual_server_config(&manual),
            "manual builder should require key",
        ),
        "VM TLS manual mode requires key_path"
    );

    let mut internal = internal_plan();
    internal.server_name = None;
    assert_eq!(
        expect_server_config_error(
            super::super::build_internal_server_config(&internal),
            "internal builder should require server name",
        ),
        "VM TLS internal mode requires server_name"
    );
}

#[test]
pub(super) fn vm_tls_runtime_requires_plaintext_listener_for_raw_protocol_poll() {
    let mut tcp = VmTcpRuntime::new();
    let plain_listener = tcp.listen("plain-required.local").expect("plain listener");
    let tls_listener = tcp.listen("tls-required.local").expect("tls listener");
    let mut runtime = VmTlsRuntime::new();

    runtime
        .install_listener_plan(plain_listener, plain_plan())
        .expect("plain plan");
    runtime
        .install_listener_plan(tls_listener, manual_plan())
        .expect("manual plan");

    assert_eq!(runtime.require_plaintext_listener(plain_listener), Ok(()));
    assert_eq!(
        runtime
            .require_plaintext_listener(tls_listener)
            .expect_err("TLS listener should require TLS stream handling"),
        "VM TLS listener requires TLS stream handling before protocol polling"
    );
}

#[test]
pub(super) fn vm_tls_runtime_requires_plaintext_listener_reports_missing_plan() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("missing-required.local").expect("listener");
    let runtime = VmTlsRuntime::new();

    assert_eq!(
        runtime
            .require_plaintext_listener(listener)
            .expect_err("missing plan should fail"),
        "VM TLS listener handle has no installed transport plan"
    );
}

#[test]
pub(super) fn vm_tls_runtime_builds_manual_rustls_server_config() {
    let (dir, cert_path, key_path) = write_self_signed_cert_pair("manual_config");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("manual-config.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, manual_plan_with_paths(cert_path, key_path))
        .expect("manual plan");

    let config = runtime
        .build_listener_server_config(listener)
        .expect("manual rustls config should build");

    assert_eq!(config.mode, VmTlsMode::Manual);
    assert_eq!(Arc::strong_count(&config.server_config), 1);
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_tls_runtime_builds_internal_rustls_server_config() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("internal-config.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, internal_plan())
        .expect("internal plan");

    let config = runtime
        .build_listener_server_config(listener)
        .expect("internal rustls config should build");

    assert_eq!(config.mode, VmTlsMode::Internal);
    assert_eq!(Arc::strong_count(&config.server_config), 1);
}

#[test]
pub(super) fn vm_tls_runtime_reports_internal_rustls_config_build_failure() {
    let generated =
        generate_simple_self_signed(vec!["localhost".to_string()]).expect("generate cert");
    let cert_der = generated.cert.der().as_ref().to_vec();

    let error = expect_server_config_error(
        super::super::build_internal_server_config_from_der(cert_der, vec![1, 2, 3]),
        "invalid internal key should fail",
    );

    assert!(error.starts_with("VM TLS failed to build server config:"));
}

#[test]
pub(super) fn vm_tls_runtime_rejects_plaintext_listener_server_config() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("plain-config.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, plain_plan())
        .expect("plain plan");

    let error = expect_server_config_error(
        runtime.build_listener_server_config(listener),
        "plaintext listener should not build TLS config",
    );

    assert_eq!(
        error,
        "VM TLS plaintext listener does not require a server config"
    );
}

#[test]
pub(super) fn vm_tls_runtime_rejects_invalid_manual_certificate_files() {
    let dir = test_fs::temp_path("vm_tls", "invalid_manual_config");
    fs::create_dir_all(&dir).expect("create TLS fixture dir");
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    fs::write(&cert_path, "not a cert").expect("write invalid cert");
    fs::write(&key_path, "not a key").expect("write invalid key");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("invalid-manual.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(
            listener,
            manual_plan_with_paths(
                cert_path.to_string_lossy().to_string(),
                key_path.to_string_lossy().to_string(),
            ),
        )
        .expect("manual plan");

    let error = expect_server_config_error(
        runtime.build_listener_server_config(listener),
        "invalid manual files should fail",
    );

    assert!(error.starts_with("VM TLS certificate `"));
    assert!(error.contains("did not contain any PEM certificates"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_tls_runtime_rejects_manual_encrypted_private_key_plan() {
    let (_dir, cert_path, key_path) = write_self_signed_cert_pair("manual_passphrase");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("manual-passphrase.local").expect("listener");
    let mut plan = manual_plan_with_paths(cert_path, key_path);
    plan.passphrase_env = Some("TLS_KEY_PASSPHRASE".to_string());
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(listener, plan)
        .expect("manual plan");

    assert_eq!(
        expect_server_config_error(
            runtime.build_listener_server_config(listener),
            "encrypted key should fail",
        ),
        "VM TLS manual encrypted private keys are not supported by VM runtime yet"
    );
}

#[test]
pub(super) fn vm_tls_runtime_reports_missing_manual_certificate_file() {
    let dir = test_fs::temp_path("vm_tls", "missing_cert_file");
    fs::create_dir_all(&dir).expect("create TLS fixture dir");
    let cert_path = dir.join("missing-cert.pem");
    let key_path = dir.join("key.pem");
    fs::write(&key_path, "not a key").expect("write key placeholder");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("missing-cert.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(
            listener,
            manual_plan_with_paths(
                cert_path.to_string_lossy().to_string(),
                key_path.to_string_lossy().to_string(),
            ),
        )
        .expect("manual plan");

    let error = expect_server_config_error(
        runtime.build_listener_server_config(listener),
        "missing cert should fail",
    );

    assert!(error.starts_with("VM TLS failed to open certificate `"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_tls_runtime_reports_malformed_manual_certificate_file() {
    let dir = test_fs::temp_path("vm_tls", "malformed_cert_file");
    fs::create_dir_all(&dir).expect("create TLS fixture dir");
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    fs::write(
        &cert_path,
        "-----BEGIN CERTIFICATE-----\n!\n-----END CERTIFICATE-----\n",
    )
    .expect("write malformed cert");
    fs::write(&key_path, "not a key").expect("write key placeholder");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("malformed-cert.local").expect("listener");
    let mut runtime = VmTlsRuntime::new();
    runtime
        .install_listener_plan(
            listener,
            manual_plan_with_paths(
                cert_path.to_string_lossy().to_string(),
                key_path.to_string_lossy().to_string(),
            ),
        )
        .expect("manual plan");

    let error = expect_server_config_error(
        runtime.build_listener_server_config(listener),
        "malformed cert should fail",
    );

    assert!(error.starts_with("VM TLS failed to parse certificate `"));
    fs::remove_dir_all(dir).expect("cleanup");
}
