//! Maintained HTTP/1 connection and upgrade lifecycle, without an executor.

use std::future::Future;

use hyper::body::Incoming;
use hyper::rt::{Read, Write};
use hyper::service::Service;
use hyper::{Request, Response};

/// The host polls this future on its existing owner and supplies the handler.
/// Hyper retains framing, keep-alive, and upgrade read-ahead ownership. Dropping
/// the future cancels its in-flight handler and releases the transport; an
/// accepted upgrade transfers transport ownership to Hyper's OnUpgrade result.
pub fn serve_connection<I, S, B>(
    io: I,
    service: S,
) -> impl Future<Output = Result<(), hyper::Error>>
where
    I: Read + Write + Unpin + Send + 'static,
    S: Service<Request<Incoming>, Response = Response<B>> + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: 'static,
    B: hyper::body::Body + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    hyper::server::conn::http1::Builder::new()
        .serve_connection(io, service)
        .with_upgrades()
}

#[cfg(test)]
#[path = "connection_test.rs"]
mod tests;
