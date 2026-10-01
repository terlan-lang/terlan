//! TLS adaptation for the shared host-readiness stream contract.

use terlan_runtime_abi::poll_io::ReadinessStream;
pub use terlan_runtime_abi::poll_io::{ReadyStream, WriteInterest};

impl<S: ReadinessStream, I> crate::tls_stream::ShutdownWrite for ReadyStream<S, I> {
    fn shutdown_write(&mut self) -> std::io::Result<()> {
        ReadyStream::shutdown_write(self)
    }
}

#[cfg(test)]
#[path = "transport_test.rs"]
mod tests;
