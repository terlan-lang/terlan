//! VM socket ownership and deadlines for the package-owned TLS/HTTP adapter.

use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::ServerConfig;
use terlan_net_native::tls_stream::{ShutdownWrite, TlsStream};

use super::protocol_task_executor::{protocol_sleep_until, VmReadyTcpStream};

pub(crate) use terlan_http_native::tls::HttpProtocol as VmTlsHttpProtocol;
pub(crate) type VmTlsHyperIo = terlan_http_native::tls::TlsIo<VmReadyTcpStream>;

impl ShutdownWrite for VmReadyTcpStream {
    fn shutdown_write(&mut self) -> io::Result<()> {
        VmReadyTcpStream::shutdown_write(self)
    }
}

pub(crate) async fn handshake(
    stream: VmReadyTcpStream,
    config: Arc<ServerConfig>,
) -> io::Result<VmTlsHyperIo> {
    handshake_until(stream, config, Instant::now() + Duration::from_secs(30)).await
}

pub(crate) async fn handshake_until(
    stream: VmReadyTcpStream,
    config: Arc<ServerConfig>,
    deadline: Instant,
) -> io::Result<VmTlsHyperIo> {
    TlsStream::handshake(stream, config, protocol_sleep_until(deadline))
        .await
        .map(VmTlsHyperIo::new)
}
