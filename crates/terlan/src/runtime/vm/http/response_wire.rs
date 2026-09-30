//! Compatibility with the VM's text diagnostic boundary; codecs live in std.http.

use std::io::Write;
use terlan_http_native::http1;

/// Writes buffered or finite-stream output through the package-owned codec.
pub(crate) fn write_http1_response<B: AsRef<[u8]>>(
    writer: &mut dyn Write,
    response: &http::Response<B>,
    close_connection: bool,
) -> Result<(), String> {
    http1::write_http1_response(writer, response, close_connection)
        .map_err(|failure| failure.message)
}

#[cfg(test)]
pub(crate) use write_http1_response as write_http1_bytes_response;
