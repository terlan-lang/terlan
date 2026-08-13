use std::io::Cursor;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore};

use super::tcp::{VmTcpRuntime, VmTcpStream};
use super::tls::{VmTlsMode, VmTlsPlan};

pub(crate) fn plain_plan() -> VmTlsPlan {
    VmTlsPlan {
        mode: VmTlsMode::Plain,
        domains: Vec::new(),
        email: None,
        primary_provider: None,
        fallback_provider: None,
        cert_path: None,
        key_path: None,
        passphrase_env: None,
        ca_path: None,
        server_name: None,
        trust_local: None,
    }
}

pub(crate) fn manual_plan_with_paths(cert_path: String, key_path: String) -> VmTlsPlan {
    VmTlsPlan {
        mode: VmTlsMode::Manual,
        domains: Vec::new(),
        email: None,
        primary_provider: None,
        fallback_provider: None,
        cert_path: Some(cert_path),
        key_path: Some(key_path),
        passphrase_env: None,
        ca_path: None,
        server_name: None,
        trust_local: None,
    }
}

pub(crate) fn client_for_cert(cert_der: Vec<u8>) -> ClientConnection {
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(cert_der))
        .expect("root cert should install");
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("protocol versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
    ClientConnection::new(
        Arc::new(config),
        ServerName::try_from("localhost").expect("server name"),
    )
    .expect("client connection")
}

pub(crate) fn pump_tcp_to_client(
    tcp: &mut VmTcpRuntime,
    client_stream: VmTcpStream,
    client: &mut ClientConnection,
) {
    while let Some(bytes) = tcp
        .receive(client_stream, 16 * 1024)
        .expect("client receives TLS")
    {
        let consumed = client
            .read_tls(&mut Cursor::new(bytes.as_slice()))
            .expect("client reads server TLS bytes");
        assert_eq!(consumed, bytes.len());
        client
            .process_new_packets()
            .expect("client processes TLS packets");
    }
}
