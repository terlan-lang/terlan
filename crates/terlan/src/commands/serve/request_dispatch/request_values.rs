//! Package metadata imports and in-memory HTTP/1 test exchanges.

pub(in crate::commands::serve) use terlan_http_native::{
    query_pairs, request_cookie_pairs, request_header_pairs,
};

use super::*;

/// Handles one raw HTTP/1 request over VM TCP streams.
///
/// Inputs:
/// - `web_root`: generated browser package root.
/// - `raw_request`: HTTP/1 request bytes to inject into a VM TCP stream.
///
/// Output:
/// - Raw HTTP/1 response bytes produced by the VM HTTP writer.
///
/// Transformation:
/// - Connects an in-memory VM TCP client to a VM HTTP listener, dispatches the
///   request through the serve route graph, and reads the response from the
///   VM-managed stream without binding host sockets or entering Hyper.
pub(in crate::commands::serve) fn handle_vm_stream_http1_request(
    web_root: &Path,
    raw_request: &[u8],
) -> Result<Vec<u8>, String> {
    handle_vm_stream_http1_exchange(web_root, raw_request).map(|exchange| exchange.response)
}

/// Raw response and optional admitted channel retained for socket handoff.
#[derive(Debug)]
pub(in crate::commands::serve) struct VmStreamHttp1Exchange {
    pub(in crate::commands::serve) response: Vec<u8>,
    pub(in crate::commands::serve) channel: Option<VmHttpChannelTransport>,
}

/// Handles one raw request while preserving any admitted long-lived channel.
pub(in crate::commands::serve) fn handle_vm_stream_http1_exchange(
    web_root: &Path,
    raw_request: &[u8],
) -> Result<VmStreamHttp1Exchange, String> {
    let mut reader = std::io::Cursor::new(raw_request);
    let mut response = Vec::new();
    let mut channel = None;
    if let Err(error) =
        handle_http1_in_memory_exchange(&mut reader, &mut response, true, |request| {
            handle_vm_stream_request(request, web_root, &mut channel, true)
        })
    {
        return vm_stream_bad_request_response(&error).map(|response| VmStreamHttp1Exchange {
            response,
            channel: None,
        });
    }
    Ok(VmStreamHttp1Exchange { response, channel })
}

/// Converts a malformed VM-stream HTTP request into a stable wire response.
///
/// Inputs:
/// - `error`: parser/runtime diagnostic reported by the strict VM HTTP layer.
///
/// Output:
/// - Raw HTTP/1 `400 Bad Request` response bytes.
///
/// Transformation:
/// - Keeps protocol diagnostics strict inside `runtime::vm::http` while giving
///   `terlc serve` the same user-facing bad-request response shape as the
///   legacy adapter for malformed input.
pub(in crate::commands::serve) fn vm_stream_bad_request_response(
    error: &str,
) -> Result<Vec<u8>, String> {
    let response = serve_vm_stream_response(
        400,
        "Bad Request",
        "text/plain; charset=utf-8",
        &[],
        format!("bad request: {error}").as_bytes(),
        false,
    )?;
    let mut wire = Vec::new();
    write_http1_response(&mut wire, &response, true)?;
    Ok(wire)
}

/// Reads a Hyper request body into source-visible UTF-8 text.
///
/// Inputs:
/// - `request`: Hyper request whose metadata has already been captured.
///
/// Output:
/// - UTF-8-lossy body text for Terlan request handlers.
/// - Stable runtime diagnostic when Hyper body collection fails.
///
/// Transformation:
/// - Uses Hyper/http-body-util collection and only decodes bytes at the VM
///   request boundary, keeping protocol mechanics out of Terlan source.
pub(in crate::commands::serve) async fn request_body_text<B>(
    request: Request<B>,
) -> Result<String, String>
where
    B: hyper::body::Body<Data = Bytes> + Send + 'static,
    B::Error: std::fmt::Display,
{
    let body = request
        .into_body()
        .collect()
        .await
        .map_err(|err| format!("error[serve_request]: failed to read request body: {err}"))?
        .to_bytes();
    Ok(String::from_utf8_lossy(&body).into_owned())
}
