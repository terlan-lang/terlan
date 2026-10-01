//! Serving-host wiring for package TLS over generic VM socket and timer primitives.

use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::ServerConfig;
use terlan_net_native::tls_stream::TlsStream;

use crate::runtime::vm::protocol_task_executor::{protocol_sleep_until, VmReadyStream};

pub(crate) use terlan_http_native::tls::HttpProtocol;
pub(crate) type HttpTlsIo = terlan_http_native::tls::TlsIo<VmReadyStream>;

pub(crate) async fn handshake(
    stream: VmReadyStream,
    config: Arc<ServerConfig>,
) -> io::Result<HttpTlsIo> {
    handshake_until(stream, config, Instant::now() + Duration::from_secs(30)).await
}

pub(crate) async fn handshake_until(
    stream: VmReadyStream,
    config: Arc<ServerConfig>,
    deadline: Instant,
) -> io::Result<HttpTlsIo> {
    TlsStream::handshake(stream, config, protocol_sleep_until(deadline))
        .await
        .map(HttpTlsIo::new)
}
