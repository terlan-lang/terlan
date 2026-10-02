use super::*;
use crate::tls_config::{Mode, Settings};
use rustls::{ClientConfig, ClientConnection, RootCertStore, ServerConnection};
use std::io::{Cursor, Read, Write};
use std::sync::Mutex;

#[derive(Debug)]
struct RecordingResolver {
    inner: Arc<dyn rustls::server::ResolvesServerCert>,
    certificate: Arc<Mutex<Option<CertificateDer<'static>>>>,
}

impl rustls::server::ResolvesServerCert for RecordingResolver {
    fn resolve(
        &self,
        hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        let key = self.inner.resolve(hello)?;
        *self.certificate.lock().unwrap() = key.cert.first().cloned();
        Some(key)
    }
}

fn settings(mode: Mode) -> Config {
    Config {
        mode,
        domains: vec![],
        email: None,
        primary_provider: None,
        fallback_provider: None,
        cert: None,
        key: None,
        passphrase_env: None,
        ca: None,
        server_name: None,
        trust_local: None,
    }
}

fn client(roots: RootCertStore, name: &str, protocols: &[&[u8]]) -> ClientConnection {
    let mut config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
    config.alpn_protocols = protocols.iter().map(|value| value.to_vec()).collect();
    ClientConnection::new(Arc::new(config), name.to_owned().try_into().unwrap()).unwrap()
}

fn handshake(
    config: Arc<ServerConfig>,
    client: &mut ClientConnection,
) -> Result<ServerConnection, rustls::Error> {
    let mut server = ServerConnection::new(config)?;
    for _ in 0..10 {
        let mut wire = Vec::new();
        client.write_tls(&mut wire).unwrap();
        server.read_tls(&mut Cursor::new(wire)).unwrap();
        server.process_new_packets()?;
        let mut wire = Vec::new();
        server.write_tls(&mut wire).unwrap();
        client.read_tls(&mut Cursor::new(wire)).unwrap();
        client.process_new_packets()?;
        if !client.is_handshaking() && !server.is_handshaking() {
            return Ok(server);
        }
    }
    panic!("in-memory TLS handshake did not complete");
}

#[test]
fn manual_material_completes_authenticated_handshake_and_http_exchange() {
    let directory = tempfile::tempdir().unwrap();
    let generated = generate_simple_self_signed(vec!["example.test".into()]).unwrap();
    fs::write(directory.path().join("cert.pem"), generated.cert.pem()).unwrap();
    fs::write(
        directory.path().join("key.pem"),
        generated.key_pair.serialize_pem(),
    )
    .unwrap();
    let config = Settings {
        mode: Some(Mode::Manual),
        cert: Some("cert.pem".into()),
        key: Some("key.pem".into()),
        ..Settings::default()
    }
    .validate()
    .unwrap();
    let runtime = manual(directory.path(), &config).unwrap();
    assert_eq!(
        runtime.server_config.alpn_protocols,
        [b"h2".to_vec(), b"http/1.1".to_vec()]
    );
    for (offered, expected) in [
        (vec![b"http/1.1".as_slice(), b"h2"], Some(b"h2".as_slice())),
        (vec![b"http/1.1".as_slice()], Some(b"http/1.1".as_slice())),
        (vec![], None),
    ] {
        let mut roots = RootCertStore::empty();
        roots.add(generated.cert.der().clone()).unwrap();
        let mut peer = client(roots, "example.test", &offered);
        let mut server = handshake(runtime.server_config.clone(), &mut peer).unwrap();
        assert_eq!(peer.alpn_protocol(), expected);
        assert_eq!(server.alpn_protocol(), expected);
        peer.writer()
            .write_all(b"GET / HTTP/1.1\r\nHost: example.test\r\n\r\n")
            .unwrap();
        let mut wire = Vec::new();
        peer.write_tls(&mut wire).unwrap();
        server.read_tls(&mut Cursor::new(wire)).unwrap();
        server.process_new_packets().unwrap();
        let mut plaintext = [0; 128];
        let count = server.reader().read(&mut plaintext).unwrap();
        assert_eq!(
            &plaintext[..count],
            b"GET / HTTP/1.1\r\nHost: example.test\r\n\r\n"
        );
    }
    let mut roots = RootCertStore::empty();
    roots.add(generated.cert.der().clone()).unwrap();
    let mut peer = client(roots, "wrong.test", &[b"h2"]);
    assert!(handshake(runtime.server_config, &mut peer).is_err());
}

#[test]
fn internal_certificate_names_are_verified_without_installing_trust() {
    for name in [None, Some("local.example"), Some("127.0.0.1")] {
        let mut config = settings(Mode::Internal);
        config.server_name = name.map(str::to_owned);
        let mut runtime = internal(&config).unwrap();
        let observed = Arc::new(Mutex::new(None));
        let server = Arc::make_mut(&mut runtime.server_config);
        server.cert_resolver = Arc::new(RecordingResolver {
            inner: server.cert_resolver.clone(),
            certificate: observed.clone(),
        });
        let name = name.unwrap_or("localhost");
        let mut peer = client(RootCertStore::empty(), name, &[b"h2"]);
        assert!(matches!(
            handshake(runtime.server_config.clone(), &mut peer),
            Err(rustls::Error::InvalidCertificate(_))
        ));
        // Trust only the certificate observed in this failed handshake; do not
        // disable chain or hostname verification and do not change system trust.
        let certificate = observed.lock().unwrap().clone().unwrap();
        let mut roots = RootCertStore::empty();
        roots.add(certificate).unwrap();
        let mut peer = client(roots.clone(), name, &[b"h2"]);
        handshake(runtime.clone().server_config, &mut peer).unwrap();
        let mut peer = client(roots, "wrong.test", &[b"h2"]);
        assert!(handshake(runtime.server_config, &mut peer).is_err());
    }
    let mut config = settings(Mode::Internal);
    config.server_name = Some("\u{e9}.test".into());
    assert!(internal(&config)
        .err()
        .unwrap()
        .to_string()
        .contains("failed to generate internal certificate"));
}

#[test]
fn manual_rejects_missing_fields_and_encrypted_key_options_before_io() {
    let mut config = settings(Mode::Manual);
    let root = Path::new("missing-project");
    assert!(manual(root, &config)
        .err()
        .unwrap()
        .to_string()
        .contains("requires a certificate path"));
    config.cert = Some("cert.pem".into());
    assert!(manual(root, &config)
        .err()
        .unwrap()
        .to_string()
        .contains("requires a key path"));
    config.passphrase_env = Some("NO_SECRET_LOOKUP".into());
    assert!(manual(root, &config)
        .err()
        .unwrap()
        .to_string()
        .contains("encrypted manual TLS keys"));
}

#[test]
fn manual_propagates_file_and_material_errors_without_accepting_partial_configuration() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = settings(Mode::Manual);
    config.cert = Some("cert.pem".into());
    config.key = Some("key.pem".into());
    let error = manual(directory.path(), &config).err().unwrap();
    assert!(error.to_string().contains("failed to open TLS certificate"));
    let generated = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    fs::write(directory.path().join("cert.pem"), generated.cert.pem()).unwrap();
    let error = manual(directory.path(), &config).err().unwrap();
    assert!(error.to_string().contains("failed to open TLS private key"));
    let other = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    fs::write(
        directory.path().join("key.pem"),
        other.key_pair.serialize_pem(),
    )
    .unwrap();
    let error = manual(directory.path(), &config).err().unwrap();
    assert!(error
        .to_string()
        .contains("failed to build TLS server config"));
}

#[test]
fn pem_failures_keep_file_context_and_never_accept_partial_chains() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("material.pem");
    assert!(load_certificate_chain(&path)
        .unwrap_err()
        .to_string()
        .contains("failed to open TLS certificate"));
    assert!(load_private_key(&path)
        .unwrap_err()
        .to_string()
        .contains("failed to open TLS private key"));
    for bytes in [
        "",
        "not PEM",
        "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----\n",
    ] {
        fs::write(&path, bytes).unwrap();
        assert!(load_certificate_chain(&path)
            .unwrap_err()
            .to_string()
            .contains("did not contain any PEM certificates"));
        assert!(load_private_key(&path)
            .unwrap_err()
            .to_string()
            .contains("did not contain a supported unencrypted PEM key"));
    }
    let generated = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let bad_certificate = "-----BEGIN CERTIFICATE-----\n!\n-----END CERTIFICATE-----\n";
    fs::write(
        &path,
        format!("{}{}", generated.cert.pem(), bad_certificate),
    )
    .unwrap();
    let error = load_certificate_chain(&path).unwrap_err();
    assert!(error
        .to_string()
        .contains("failed to parse TLS certificate"));
    assert!(error.to_string().contains(path.to_str().unwrap()));
    fs::write(
        &path,
        "-----BEGIN PRIVATE KEY-----\n!\n-----END PRIVATE KEY-----\n",
    )
    .unwrap();
    let error = load_private_key(&path).unwrap_err();
    assert!(error
        .to_string()
        .contains("failed to parse TLS private key"));
    assert!(error.to_string().contains(path.to_str().unwrap()));
    fs::write(
        &path,
        format!("{}{}", generated.cert.pem(), generated.cert.pem()),
    )
    .unwrap();
    assert_eq!(
        load_certificate_chain(&path).unwrap(),
        vec![generated.cert.der().clone(); 2]
    );
}

#[test]
fn rustls_rejects_mismatched_keys_and_invalid_der_without_plaintext_fallback() {
    let generated = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let other = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    for certificates in [
        vec![],
        vec![CertificateDer::from(vec![0, 1, 2])],
        vec![generated.cert.der().clone()],
    ] {
        let key =
            terlan_net_native::tls::parse_private_key(other.key_pair.serialize_pem().as_bytes())
                .unwrap();
        let error = rustls_server_config(certificates, key).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("failed to build TLS server config"),
            "{error}"
        );
    }
}
