//! Shared authenticated TLS transport for package codec and dispatch tests.

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};
use std::task::Poll;

use rustls::{ClientConfig, ClientConnection, RootCertStore};
use terlan_net_native::tls_stream::{ShutdownWrite, TlsStream};

use crate::tls::TlsIo;

#[derive(Default)]
pub(crate) struct Wire {
    pub incoming: VecDeque<u8>,
    pub outgoing: Vec<u8>,
    pub blocked: bool,
    pub eof: bool,
    pub shutdown: bool,
}

#[derive(Clone, Default)]
pub(crate) struct Transport(pub Arc<Mutex<Wire>>);

impl io::Read for Transport {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let mut wire = self.0.lock().unwrap();
        if wire.incoming.is_empty() && !wire.eof {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = bytes.len().min(wire.incoming.len());
        for byte in &mut bytes[..count] {
            *byte = wire.incoming.pop_front().unwrap();
        }
        Ok(count)
    }
}

impl io::Write for Transport {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut wire = self.0.lock().unwrap();
        if wire.blocked {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        wire.outgoing.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl ShutdownWrite for Transport {
    fn shutdown_write(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().shutdown = true;
        Ok(())
    }
}

pub(crate) fn exchange(client: &mut ClientConnection, transport: &Transport) {
    let wire = std::mem::take(&mut transport.0.lock().unwrap().outgoing);
    if !wire.is_empty() {
        client.read_tls(&mut io::Cursor::new(wire)).unwrap();
        client.process_new_packets().unwrap();
    }
    let mut wire = Vec::new();
    client.write_tls(&mut wire).unwrap();
    transport.0.lock().unwrap().incoming.extend(wire);
}

pub(crate) fn connected(
    protocol: Option<&[u8]>,
) -> (TlsIo<Transport>, ClientConnection, Transport) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let mut server = terlan_net_native::tls::server_config(
        vec![cert.cert.der().clone()],
        terlan_net_native::tls::parse_private_key(cert.key_pair.serialize_pem().as_bytes())
            .unwrap(),
    )
    .unwrap();
    let mut roots = RootCertStore::empty();
    roots.add(cert.cert.der().clone()).unwrap();
    let mut client =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
    server.alpn_protocols = protocol.into_iter().map(<[u8]>::to_vec).collect();
    client.alpn_protocols = server.alpn_protocols.clone();
    let mut client =
        ClientConnection::new(Arc::new(client), "localhost".try_into().unwrap()).unwrap();
    let transport = Transport::default();
    let mut server = TlsStream::new(transport.clone(), Arc::new(server)).unwrap();
    for _ in 0..10 {
        exchange(&mut client, &transport);
        match server.poll_handshake() {
            Poll::Ready(result) => {
                result.unwrap();
                exchange(&mut client, &transport);
                assert!(!client.is_handshaking());
                return (TlsIo::new(server), client, transport);
            }
            Poll::Pending => {}
        }
    }
    panic!("handshake failed to make progress");
}
