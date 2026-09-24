//! Mutual TLS is enforced before Hyper can dispatch an authenticated peer request.

use super::*;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair,
};
use std::sync::atomic::{AtomicUsize, Ordering};

fn authority(name: &str) -> (Certificate, KeyPair) {
    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::new(vec![name.into()]).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    (params.self_signed(&key).unwrap(), key)
}

fn leaf(
    authority: &(Certificate, KeyPair),
    name: &str,
    purpose: ExtendedKeyUsagePurpose,
) -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::new(vec![name.into()]).unwrap();
    params.extended_key_usages = vec![purpose];
    let cert = params.signed_by(&key, &authority.0, &authority.1).unwrap();
    (
        cert.der().clone(),
        PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
    )
}

fn tls_configurations() -> (Arc<ServerConfig>, Vec<Arc<ClientConfig>>) {
    let ca = authority("replica-ca.test");
    let foreign_ca = authority("foreign-ca.test");
    let (server_cert, server_key) = leaf(&ca, "localhost", ExtendedKeyUsagePurpose::ServerAuth);
    let mut roots = RootCertStore::empty();
    roots.add(ca.0.der().clone()).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
        Arc::new(roots.clone()),
        Arc::clone(&provider),
    )
    .build()
    .unwrap();
    let mut server = ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_client_cert_verifier(verifier)
        .with_single_cert(vec![server_cert], server_key)
        .unwrap();
    server.alpn_protocols = vec![b"http/1.1".to_vec()];
    let mut clients = Vec::new();
    for authority in [None, Some(&foreign_ca), Some(&ca)] {
        let builder = ClientConfig::builder_with_provider(Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots.clone());
        let mut client = match authority {
            None => builder.with_no_client_auth(),
            Some(authority) => {
                let (cert, key) = leaf(
                    authority,
                    "writer.test",
                    ExtendedKeyUsagePurpose::ClientAuth,
                );
                builder.with_client_auth_cert(vec![cert], key).unwrap()
            }
        };
        client.alpn_protocols = vec![b"http/1.1".to_vec()];
        clients.push(Arc::new(client));
    }
    (Arc::new(server), clients)
}

fn request_over_tls(
    address: std::net::SocketAddr,
    client: Arc<ClientConfig>,
) -> Result<http::StatusCode, String> {
    let tcp = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(5)).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    let connection =
        ClientConnection::new(client, ServerName::try_from("localhost").unwrap()).unwrap();
    let io = BlockingTlsIo(StreamOwned::new(connection, tcp));
    let (mut sender, connection) =
        block_on(hyper::client::conn::http1::handshake(io)).map_err(|error| error.to_string())?;
    let runner = std::thread::spawn(move || block_on(connection));
    let request = Request::builder()
        .uri("/")
        .header("host", "localhost")
        .header("connection", "close")
        .body(Empty::<Bytes>::new())
        .unwrap();
    let result = block_on(sender.send_request(request))
        .map(|response| response.status())
        .map_err(|error| error.to_string());
    drop(sender);
    let _ = runner.join().unwrap();
    result
}

#[test]
fn vm_tls_authenticates_the_client_before_http_dispatch() {
    let (server_config, clients) = tls_configurations();
    let dispatched = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&dispatched);
    let factory: VmProtocolTaskFactory = Arc::new(move |stream, _route| {
        let config = Arc::clone(&server_config);
        let observed = Arc::clone(&observed);
        Box::pin(async move {
            let io = tls_io::VmTlsHyperIo::handshake(stream, config)
                .await
                .map_err(|error| error.to_string())?;
            let protocol = io
                .negotiated_protocol()
                .map_err(|error| error.to_string())?;
            assert_eq!(protocol, tls_io::VmTlsHttpProtocol::Http1);
            let service = service_fn(move |_request| {
                observed.fetch_add(1, Ordering::SeqCst);
                async {
                    Ok::<_, Infallible>(Response::new(Full::new(Bytes::from_static(
                        b"authenticated",
                    ))))
                }
            });
            http1::Builder::new()
                .serve_connection(io, service)
                .await
                .map_err(|error| error.to_string())
        })
    });
    let listener =
        crate::runtime::vm::protocol_task_executor::bind_protocol_listener("127.0.0.1", 0).unwrap();
    let mut server =
        start_protocol_tasks_with_topology(listener, factory, VmSchedulerTopology::new(1).unwrap())
            .unwrap();
    for (index, client) in clients.into_iter().enumerate() {
        let result = request_over_tls(server.local_addr(), client);
        if index < 2 {
            assert!(result.is_err(), "untrusted client reached HTTP");
            assert_eq!(dispatched.load(Ordering::SeqCst), 0);
        } else {
            assert_eq!(result.unwrap(), http::StatusCode::OK);
            assert_eq!(dispatched.load(Ordering::SeqCst), 1);
        }
    }
    server.stop().unwrap();
}
