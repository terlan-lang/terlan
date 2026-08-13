use super::*;

/// Actor ownership and retained state for one pollable HTTP exchange.
pub(crate) struct VmHttpActorExchange<'a> {
    pub(super) processes: &'a mut VmProcessTable,
    pub(super) process: VmProcessId,
    pub(super) buffer: &'a mut VmHttpTcpRequestBuffer,
    pub(super) close_connection: bool,
    pub(super) response_memory: Option<&'a mut VmHttpResponseMemory>,
}

impl<'a> VmHttpActorExchange<'a> {
    pub(crate) fn without_response_memory(
        processes: &'a mut VmProcessTable,
        process: VmProcessId,
        buffer: &'a mut VmHttpTcpRequestBuffer,
        close_connection: bool,
    ) -> Self {
        Self {
            processes,
            process,
            buffer,
            close_connection,
            response_memory: None,
        }
    }
}

/// Parsed request and caller-owned buffers needed to finish one exchange.
struct VmHttpExchangeCompletion<'a> {
    buffer: &'a mut VmHttpTcpRequestBuffer,
    request: ::http::Request<String>,
    consumed: usize,
    close_connection: bool,
    response_memory: Option<(
        &'a mut VmHttpResponseMemory,
        &'a mut VmProcessTable,
        VmProcessId,
    )>,
}

/// Reads one HTTP/1 response and returns the exact wire bytes observed.
///
/// Inputs:
/// - `reader`: blocking byte stream positioned at the start of an HTTP/1
///   response.
/// - `expected_status`: status code required for the response.
///
/// Output:
/// - Header and body bytes truncated to the declared `Content-Length`.
///
/// Transformation:
/// - Parses response headers with `httparse`, validates the expected status,
///   reads the declared body, and preserves the original wire bytes for caller
///   assertions.
#[cfg(test)]
pub(crate) fn read_http1_response(
    reader: &mut dyn Read,
    expected_status: u16,
) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|error| format!("failed to read VM HTTP response: {error}"))?;
        if read == 0 {
            return Err("VM HTTP response closed before headers completed".to_string());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > HTTP_HEADER_LIMIT {
            return Err("VM HTTP response exceeded 64 KiB header limit".to_string());
        }
        if let Some(position) = find_header_end(&buffer) {
            break position;
        }
    };

    let body_start = header_end + 4;
    let content_length =
        parse_http1_response_content_length(&buffer[..body_start], expected_status)?;
    if content_length > HTTP_BODY_LIMIT {
        return Err("VM HTTP response exceeded 1 MiB body limit".to_string());
    }
    while buffer.len() < body_start + content_length {
        let read = reader
            .read(&mut chunk)
            .map_err(|error| format!("failed to read VM HTTP response body: {error}"))?;
        if read == 0 {
            return Err("VM HTTP response body ended early".to_string());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    buffer.truncate(body_start + content_length);
    Ok(buffer)
}

/// Handles one HTTP/1 request/response exchange over caller-owned byte streams.
///
/// Inputs:
/// - `reader`: byte stream positioned at a complete or partial HTTP/1 request.
/// - `writer`: byte sink for the serialized HTTP/1 response.
/// - `close_connection`: response connection policy selected by the caller.
/// - `handler`: typed request-to-response function.
///
/// Output:
/// - Exchange telemetry, or a stable protocol/runtime diagnostic.
///
/// Transformation:
/// - Parses request bytes through the maintained HTTP parser, invokes a typed
///   handler, serializes the response, and writes response bytes without
///   requiring VM TCP, live sockets, or framework callback state.
pub(crate) fn handle_http1_in_memory_exchange<B: AsRef<[u8]>>(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    close_connection: bool,
    handler: impl FnOnce(::http::Request<String>) -> Result<::http::Response<B>, String>,
) -> Result<VmHttpInMemoryExchange, String> {
    let request = read_http1_request(reader)?;
    let request_method = request.method().as_str().to_string();
    let request_path = request.uri().path().to_string();
    let response = handler(request)?;
    let response_status = response.status().as_u16();
    let mut response_wire = Vec::new();

    write_http1_response(&mut response_wire, &response, close_connection)?;
    let response_bytes = response_wire.len();
    writer
        .write_all(&response_wire)
        .map_err(|error| format!("failed to write VM HTTP in-memory response: {error}"))?;
    Ok(VmHttpInMemoryExchange {
        request_method,
        request_path,
        response_status,
        response_bytes,
        close_connection,
    })
}

/// Builds a VM handler dispatch stream for a parsed request body.
///
/// Inputs:
/// - `request`: typed HTTP request produced by the VM HTTP parser.
/// - `max_chunk_bytes`: maximum body bytes delivered per dispatch chunk.
///
/// Output:
/// - `VmHttpRequestBodyStream` with ordered chunks and stable final markers.
///
/// Transformation:
/// - Splits the already-validated UTF-8 request body into bounded chunks so
///   handler dispatch can consume body data incrementally without relying on a
///   host async runtime or retaining raw request bodies in diagnostics.
#[cfg(test)]
pub(crate) fn stream_http_request_body_for_dispatch(
    request: &::http::Request<String>,
    max_chunk_bytes: usize,
) -> Result<VmHttpRequestBodyStream, String> {
    if max_chunk_bytes == 0 {
        return Err("VM HTTP request body stream chunk size must be greater than zero".to_string());
    }
    let body = request.body().as_bytes();
    let total_bytes = body.len();
    let chunks = if body.is_empty() {
        VecDeque::from([VmHttpRequestBodyChunk {
            index: 0,
            bytes: Vec::new(),
            is_final: true,
        }])
    } else {
        body.chunks(max_chunk_bytes)
            .enumerate()
            .map(|(index, bytes)| VmHttpRequestBodyChunk {
                index,
                bytes: bytes.to_vec(),
                is_final: (index + 1) * max_chunk_bytes >= total_bytes,
            })
            .collect()
    };

    Ok(VmHttpRequestBodyStream {
        chunks,
        total_bytes,
        max_chunk_bytes,
    })
}

/// Handles one HTTP/1 request/response exchange on a VM TCP stream.
///
/// Inputs:
/// - `tcp`: VM-owned TCP runtime.
/// - `stream`: accepted VM TCP stream owned by the HTTP handler.
/// - `handler`: request-to-response function owned by the caller.
///
/// Output:
/// - Exchange telemetry, or a stable protocol/runtime diagnostic.
///
/// Transformation:
/// - Reads request bytes through the VM TCP stream abstraction, parses through
///   the maintained HTTP parser, serializes the handler response, and sends
///   response bytes back through the same VM TCP abstraction.
#[cfg(test)]
pub(crate) fn handle_http1_tcp_exchange(
    tcp: &mut VmTcpRuntime,
    stream: VmTcpStream,
    handler: impl FnOnce(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<VmHttpTcpExchange, String> {
    let request = {
        let mut reader = VmTcpReadStream::new(tcp, stream);
        read_http1_request(&mut reader)?
    };
    let request_method = request.method().as_str().to_string();
    let request_path = request.uri().path().to_string();
    let response = handler(request)?;
    let response_status = response.status().as_u16();
    let mut response_wire = Vec::new();

    write_http1_response(&mut response_wire, &response, true)?;
    let response_bytes = response_wire.len();
    tcp.send(stream, response_wire)?;
    Ok(VmHttpTcpExchange {
        request_method,
        request_path,
        response_status,
        response_bytes,
        close_connection: true,
    })
}

/// Polls one HTTP/1 exchange over a VM TCP stream.
///
/// Inputs:
/// - `tcp`: VM TCP runtime.
/// - `stream`: accepted VM TCP stream.
/// - `buffer`: retained request buffer for this stream.
/// - `handler`: request-to-response function owned by the caller.
///
/// Output:
/// - `NeedRead` when the stream has no complete request yet, or `Complete`
///   once a response has been written back through the VM TCP stream.
///
/// Transformation:
/// - Converts stream bytes into an incremental HTTP request parse so VM actors
///   can park on read readiness instead of blocking a host thread.
#[cfg(test)]
pub(crate) fn poll_http1_tcp_exchange(
    tcp: &mut VmTcpRuntime,
    stream: VmTcpStream,
    buffer: &mut VmHttpTcpRequestBuffer,
    handler: impl FnOnce(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<VmHttpTcpPoll, String> {
    poll_http1_tcp_exchange_with_connection(tcp, stream, buffer, true, None, handler)
}

/// Polls one HTTP/1 keep-alive exchange over a VM TCP stream.
///
/// Inputs:
/// - VM TCP runtime, stream, retained request buffer, and request handler.
///
/// Output:
/// - `NeedRead` until a full request is available, or `Complete` after one
///   response is written with `Connection: keep-alive`.
///
/// Transformation:
/// - Reuses the pollable HTTP parser while preserving any pipelined bytes in
///   the request buffer for the next exchange on the same VM stream.
#[cfg(test)]
pub(crate) fn poll_http1_tcp_keep_alive_exchange(
    tcp: &mut VmTcpRuntime,
    stream: VmTcpStream,
    buffer: &mut VmHttpTcpRequestBuffer,
    handler: impl FnOnce(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<VmHttpTcpPoll, String> {
    poll_http1_tcp_exchange_with_connection(tcp, stream, buffer, false, None, handler)
}

/// Polls one HTTP/1 exchange over a VM TLS/TCP stream.
///
/// Inputs:
/// - VM TCP runtime, TLS/TCP stream adapter, retained request buffer, and
///   request handler.
///
/// Output:
/// - `NeedRead` until TLS handshake and HTTP request buffering produce a full
///   request, or `Complete` after an encrypted response has been written back
///   through the same VM TCP stream.
///
/// Transformation:
/// - Keeps HTTP parsing above decrypted plaintext while TLS record processing
///   and encryption stay in the VM TLS adapter backed by rustls.
#[cfg(test)]
pub(crate) fn poll_http1_tls_tcp_exchange(
    tcp: &mut VmTcpRuntime,
    tls_stream: &mut VmTlsTcpServerStream,
    buffer: &mut VmHttpTcpRequestBuffer,
    handler: impl FnOnce(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<VmHttpTcpPoll, String> {
    poll_http1_tls_tcp_exchange_with_connection(tcp, tls_stream, buffer, true, None, handler)
}

/// Polls one HTTP/1 exchange with an explicit connection policy.
#[cfg(test)]
pub(super) fn poll_http1_tcp_exchange_with_connection(
    tcp: &mut VmTcpRuntime,
    stream: VmTcpStream,
    buffer: &mut VmHttpTcpRequestBuffer,
    close_connection: bool,
    response_memory: Option<(&mut VmHttpResponseMemory, &mut VmProcessTable, VmProcessId)>,
    handler: impl FnOnce(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<VmHttpTcpPoll, String> {
    if let Some((request, consumed)) = try_parse_http1_request_buffer(&buffer.bytes)? {
        return complete_polled_http_exchange(
            tcp,
            stream,
            VmHttpExchangeCompletion {
                buffer,
                request,
                consumed,
                close_connection,
                response_memory,
            },
            handler,
        );
    }
    let Some(bytes) = tcp.receive(stream, 4096)? else {
        if tcp.peer_write_closed(stream)? {
            return Err(incomplete_http1_request_error(&buffer.bytes));
        }
        return Ok(VmHttpTcpPoll::NeedRead);
    };
    buffer.bytes.extend(bytes);
    let Some((request, consumed)) = try_parse_http1_request_buffer(&buffer.bytes)? else {
        if tcp.peer_write_closed(stream)? {
            return Err(incomplete_http1_request_error(&buffer.bytes));
        }
        return Ok(VmHttpTcpPoll::NeedRead);
    };
    complete_polled_http_exchange(
        tcp,
        stream,
        VmHttpExchangeCompletion {
            buffer,
            request,
            consumed,
            close_connection,
            response_memory,
        },
        handler,
    )
}

/// Polls one HTTP/1 TLS/TCP exchange with an explicit connection policy.
pub(super) fn poll_http1_tls_tcp_exchange_with_connection(
    tcp: &mut VmTcpRuntime,
    tls_stream: &mut VmTlsTcpServerStream,
    buffer: &mut VmHttpTcpRequestBuffer,
    close_connection: bool,
    response_memory: Option<(&mut VmHttpResponseMemory, &mut VmProcessTable, VmProcessId)>,
    handler: impl FnOnce(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<VmHttpTcpPoll, String> {
    if let Some((request, consumed)) = try_parse_http1_request_buffer(&buffer.bytes)? {
        return complete_polled_tls_http_exchange(
            tcp,
            tls_stream,
            VmHttpExchangeCompletion {
                buffer,
                request,
                consumed,
                close_connection,
                response_memory,
            },
            handler,
        );
    }

    match tls_stream.poll(tcp)? {
        VmTlsTcpPoll::Ready => {}
        VmTlsTcpPoll::NeedRead | VmTlsTcpPoll::Handshaking => return Ok(VmHttpTcpPoll::NeedRead),
    }

    let plaintext = tls_stream.read_plaintext()?;
    if !plaintext.is_empty() {
        buffer.bytes.extend(plaintext);
    }
    let Some((request, consumed)) = try_parse_http1_request_buffer(&buffer.bytes)? else {
        return Ok(VmHttpTcpPoll::NeedRead);
    };
    complete_polled_tls_http_exchange(
        tcp,
        tls_stream,
        VmHttpExchangeCompletion {
            buffer,
            request,
            consumed,
            close_connection,
            response_memory,
        },
        handler,
    )
}

/// Polls an HTTP/1 TCP exchange and parks the handler actor when it needs data.
///
/// Inputs:
/// - VM process table, TCP runtime, handler stream/process, retained request
///   buffer, and handler function.
///
/// Output:
/// - `Complete` after response emission, `Parked` when the process was blocked
///   on stream readiness, or `Ready` when the stream became readable before
///   parking and the caller should poll again.
///
/// Transformation:
/// - Binds HTTP stream parsing to VM actor state without letting HTTP or TCP
///   enqueue scheduler work directly.
#[cfg(test)]
pub(crate) fn poll_or_park_http1_tcp_exchange(
    processes: &mut VmProcessTable,
    tcp: &mut VmTcpRuntime,
    process: VmProcessId,
    stream: VmTcpStream,
    buffer: &mut VmHttpTcpRequestBuffer,
    handler: impl FnOnce(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<VmHttpTcpActorPoll, String> {
    poll_or_park_http1_tcp_exchange_with_connection(
        tcp,
        stream,
        VmHttpActorExchange {
            processes,
            process,
            buffer,
            close_connection: true,
            response_memory: None,
        },
        handler,
    )
}

/// Polls an HTTP/1 TCP exchange with an explicit connection policy and parks
/// the handler actor when it needs data.
#[cfg(test)]
pub(super) fn poll_or_park_http1_tcp_exchange_with_connection(
    tcp: &mut VmTcpRuntime,
    stream: VmTcpStream,
    exchange: VmHttpActorExchange<'_>,
    handler: impl FnOnce(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<VmHttpTcpActorPoll, String> {
    let VmHttpActorExchange {
        processes,
        process,
        buffer,
        close_connection,
        response_memory,
    } = exchange;
    let accounting = response_memory.map(|memory| (memory, &mut *processes, process));
    match poll_http1_tcp_exchange_with_connection(
        tcp,
        stream,
        buffer,
        close_connection,
        accounting,
        handler,
    )? {
        VmHttpTcpPoll::Complete(exchange) => Ok(VmHttpTcpActorPoll::Complete(exchange)),
        VmHttpTcpPoll::NeedRead => {
            block_http_handler(processes, process)?;
            if tcp.park_receive(stream, process)? {
                Ok(VmHttpTcpActorPoll::Parked)
            } else {
                processes.with_process_control_mutator(process, |actor| actor.wake())?;
                Ok(VmHttpTcpActorPoll::Ready)
            }
        }
    }
}

/// Polls an HTTP/1 TLS/TCP exchange with an explicit connection policy and
/// parks the handler actor when encrypted stream input is needed.
pub(in crate::runtime::vm::http) fn poll_or_park_http1_tls_tcp_exchange_with_connection(
    tcp: &mut VmTcpRuntime,
    tls_stream: &mut VmTlsTcpServerStream,
    exchange: VmHttpActorExchange<'_>,
    handler: impl FnOnce(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<VmHttpTcpActorPoll, String> {
    let VmHttpActorExchange {
        processes,
        process,
        buffer,
        close_connection,
        response_memory,
    } = exchange;
    let accounting = response_memory.map(|memory| (memory, &mut *processes, process));
    let poll_result = poll_http1_tls_tcp_exchange_with_connection(
        tcp,
        tls_stream,
        buffer,
        close_connection,
        accounting,
        handler,
    );
    match poll_result? {
        VmHttpTcpPoll::Complete(exchange) => Ok(VmHttpTcpActorPoll::Complete(exchange)),
        VmHttpTcpPoll::NeedRead => {
            block_http_handler(processes, process)?;
            if tcp.park_receive(tls_stream.stream(), process)? {
                Ok(VmHttpTcpActorPoll::Parked)
            } else {
                processes.with_process_control_mutator(process, |actor| actor.wake())?;
                Ok(VmHttpTcpActorPoll::Ready)
            }
        }
    }
}

/// Blocks one live HTTP handler under scoped actor ownership.
pub(crate) fn block_http_handler(
    processes: &mut VmProcessTable,
    process: VmProcessId,
) -> Result<(), String> {
    if processes.get(process).is_none() {
        return Err(format!(
            "VM HTTP handler process {} is missing",
            process.as_u64()
        ));
    }
    processes.with_process_control_mutator(process, |actor| {
        if matches!(actor.state, VmProcessState::Exited(_)) {
            return Err(format!(
                "VM HTTP handler process {} has exited",
                process.as_u64()
            ));
        }
        actor.block();
        Ok(())
    })?
}

/// Accepts one VM TCP stream and creates its HTTP handler process.
///
/// Inputs:
/// - VM process table, TCP runtime, listener, and source identity for the
///   handler entrypoint.
///
/// Output:
/// - Optional handler state when a stream was waiting in the listener backlog.
///
/// Transformation:
/// - Converts accepted VM TCP streams into source-visible handler processes
///   that can be scheduled and parked by the VM runtime.
#[cfg(test)]
pub(crate) fn accept_http1_tcp_handler(
    processes: &mut VmProcessTable,
    tcp: &mut VmTcpRuntime,
    listener: VmTcpListener,
    source: VmProcessSource,
) -> Result<Option<VmHttpTcpHandler>, String> {
    let Some(stream) = tcp.accept(listener, "std.http.handler")? else {
        return Ok(None);
    };
    let process = processes.spawn_root(source);
    Ok(Some(VmHttpTcpHandler {
        process,
        stream,
        buffer: VmHttpTcpRequestBuffer::default(),
        tls_stream: None,
    }))
}

/// Accepts one VM TCP stream and creates a TLS-backed HTTP handler process.
#[cfg(test)]
pub(crate) fn accept_http1_tls_tcp_handler(
    processes: &mut VmProcessTable,
    tcp: &mut VmTcpRuntime,
    tls: &VmTlsRuntime,
    listener: VmTcpListener,
    source: VmProcessSource,
) -> Result<Option<VmHttpTcpHandler>, String> {
    let Some(stream) = tcp.accept(listener, "std.http.handler")? else {
        return Ok(None);
    };
    let connection = tls.start_listener_server_connection(listener)?;
    let process = processes.spawn_root(source);
    Ok(Some(VmHttpTcpHandler {
        process,
        stream,
        buffer: VmHttpTcpRequestBuffer::default(),
        tls_stream: Some(VmTlsTcpServerStream::new(stream, connection)),
    }))
}

/// Finishes one VM HTTP/TCP handler process and closes its stream.
///
/// Inputs:
/// - Process table, TCP runtime, handler state, and exit reason.
///
/// Output:
/// - Resource handles released by the exiting process.
///
/// Transformation:
/// - Centralizes HTTP handler lifecycle cleanup so completed or cancelled
///   handlers do not leave open VM TCP streams or live process state behind.
pub(crate) fn finish_http1_tcp_handler(
    processes: &mut VmProcessTable,
    tcp: &mut VmTcpRuntime,
    handler: &VmHttpTcpHandler,
    reason: VmExitReason,
) -> Result<Vec<String>, String> {
    tcp.close_stream(handler.stream)?;
    processes.exit_process(handler.process, reason)
}

/// Completes a pollable HTTP exchange once a full request is buffered.
#[cfg(test)]
fn complete_polled_http_exchange(
    tcp: &mut VmTcpRuntime,
    stream: VmTcpStream,
    completion: VmHttpExchangeCompletion<'_>,
    handler: impl FnOnce(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<VmHttpTcpPoll, String> {
    let VmHttpExchangeCompletion {
        buffer,
        request,
        consumed,
        close_connection,
        response_memory,
    } = completion;
    let request_method = request.method().as_str().to_string();
    let request_path = request.uri().path().to_string();
    let close_connection = close_connection || request_wants_http1_close(&request);
    let response = handler(request)?;
    let response_status = response.status().as_u16();
    let mut response_wire = Vec::new();

    write_http1_response(&mut response_wire, &response, close_connection)?;
    let response_bytes = response_wire.len();
    let mut response_memory = response_memory;
    let allocation = response_memory
        .as_mut()
        .map(|(memory, processes, owner)| memory.reserve(processes, *owner, response_bytes))
        .transpose()?;
    let send_result = tcp.send(stream, response_wire);
    if let (Some((memory, processes, owner)), Some(allocation)) = (response_memory, allocation) {
        memory.complete_write(
            processes,
            owner,
            allocation,
            response_bytes,
            send_result.is_ok(),
        )?;
    }
    send_result?;
    buffer.bytes.drain(..consumed);
    Ok(VmHttpTcpPoll::Complete(VmHttpTcpExchange {
        request_method,
        request_path,
        response_status,
        response_bytes,
        close_connection,
    }))
}

/// Completes a pollable HTTP exchange over TLS once a full request is buffered.
fn complete_polled_tls_http_exchange(
    tcp: &mut VmTcpRuntime,
    tls_stream: &mut VmTlsTcpServerStream,
    completion: VmHttpExchangeCompletion<'_>,
    handler: impl FnOnce(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<VmHttpTcpPoll, String> {
    let VmHttpExchangeCompletion {
        buffer,
        request,
        consumed,
        close_connection,
        response_memory,
    } = completion;
    let request_method = request.method().as_str().to_string();
    let request_path = request.uri().path().to_string();
    let close_connection = close_connection || request_wants_http1_close(&request);
    let response = handler(request)?;
    let response_status = response.status().as_u16();
    let mut response_wire = Vec::new();

    write_http1_response(&mut response_wire, &response, close_connection)?;
    let response_bytes = response_wire.len();
    let mut response_memory = response_memory;
    let allocation = response_memory
        .as_mut()
        .map(|(memory, processes, owner)| memory.reserve(processes, *owner, response_bytes))
        .transpose()?;
    let write_result = tls_stream.write_plaintext(tcp, &response_wire);
    if let (Some((memory, processes, owner)), Some(allocation)) = (response_memory, allocation) {
        memory.complete_write(
            processes,
            owner,
            allocation,
            response_bytes,
            write_result.is_ok(),
        )?;
    }
    write_result?;
    buffer.bytes.drain(..consumed);
    Ok(VmHttpTcpPoll::Complete(VmHttpTcpExchange {
        request_method,
        request_path,
        response_status,
        response_bytes,
        close_connection,
    }))
}

/// Returns whether an HTTP/1 request asks the server to close the connection.
pub(crate) fn request_wants_http1_close(request: &::http::Request<String>) -> bool {
    request
        .headers()
        .get(::http::header::CONNECTION)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("close"))
        })
        .unwrap_or(false)
}

/// Blocking reader facade over one VM TCP stream.
pub(in crate::runtime::vm::http) struct VmTcpReadStream<'a> {
    tcp: &'a mut VmTcpRuntime,
    stream: VmTcpStream,
    pending: VecDeque<u8>,
}

#[cfg(test)]
impl<'a> VmTcpReadStream<'a> {
    /// Creates a reader over a VM TCP stream.
    pub(in crate::runtime::vm::http) fn new(
        tcp: &'a mut VmTcpRuntime,
        stream: VmTcpStream,
    ) -> Self {
        Self {
            tcp,
            stream,
            pending: VecDeque::new(),
        }
    }
}

impl Read for VmTcpReadStream<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        while self.pending.is_empty() {
            match self
                .tcp
                .receive(self.stream, buffer.len().max(1))
                .map_err(std::io::Error::other)?
            {
                Some(bytes) => self.pending.extend(bytes),
                None => return Ok(0),
            }
        }
        let mut read = 0;
        while read < buffer.len() {
            let Some(byte) = self.pending.pop_front() else {
                break;
            };
            buffer[read] = byte;
            read += 1;
        }
        Ok(read)
    }
}

/// Attempts to parse a complete HTTP/1 request from buffered bytes.
pub(crate) fn try_parse_http1_request_buffer(
    buffer: &[u8],
) -> Result<Option<(::http::Request<String>, usize)>, String> {
    let Some(header_end) = find_header_end(buffer) else {
        if buffer.len() > HTTP_HEADER_LIMIT {
            return Err("VM HTTP request exceeded 64 KiB header limit".to_string());
        }
        return Ok(None);
    };
    let body_start = header_end + 4;
    let (method, uri, headers, content_length) =
        parse_http1_request_headers(&buffer[..body_start])?;
    if content_length > HTTP_BODY_LIMIT {
        return Err("VM HTTP request exceeded 1 MiB body limit".to_string());
    }
    let complete_len = body_start + content_length;
    if buffer.len() < complete_len {
        return Ok(None);
    }
    let body = String::from_utf8(buffer[body_start..complete_len].to_vec())
        .map_err(|error| format!("VM HTTP request body must be UTF-8: {error}"))?;
    let mut builder = ::http::Request::builder()
        .method(method.as_str())
        .uri(uri.as_str());
    for (name, value) in headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    let request = builder
        .body(body)
        .map_err(|error| format!("failed to build parsed VM HTTP request: {error}"))?;
    Ok(Some((request, complete_len)))
}

/// Classifies incomplete HTTP/1 request bytes after stream write EOF.
#[cfg(test)]
pub(crate) fn incomplete_http1_request_error(buffer: &[u8]) -> String {
    if let Some(header_end) = find_header_end(buffer) {
        let body_start = header_end + 4;
        if let Ok((_, _, _, content_length)) = parse_http1_request_headers(&buffer[..body_start]) {
            if buffer.len() < body_start.saturating_add(content_length) {
                return "VM HTTP request body ended early".to_string();
            }
        }
    }
    "VM HTTP request closed before headers completed".to_string()
}

/// Finds the HTTP header terminator in a request or response buffer.
pub(crate) fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

type ParsedHttpHeaders = (String, String, Vec<(String, String)>, usize);

/// Parses HTTP/1 request headers with `httparse`.
pub(crate) fn parse_http1_request_headers(bytes: &[u8]) -> Result<ParsedHttpHeaders, String> {
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut request = httparse::Request::new(&mut headers);
    match request
        .parse(bytes)
        .map_err(|error| format!("failed to parse VM HTTP request: {error}"))?
    {
        httparse::Status::Complete(_) => {}
        httparse::Status::Partial => {
            return Err("VM HTTP parser reported partial headers".to_string());
        }
    }
    let method = request
        .method
        .ok_or_else(|| "VM HTTP request missing method".to_string())?
        .to_string();
    let uri = request
        .path
        .ok_or_else(|| "VM HTTP request missing path".to_string())?
        .to_string();
    let mut content_length = 0usize;
    let mut parsed_headers = Vec::with_capacity(request.headers.len());
    for header in request.headers.iter() {
        let value = std::str::from_utf8(header.value)
            .map_err(|error| format!("VM HTTP header `{}` is not UTF-8: {error}", header.name))?
            .to_string();
        if header.name.eq_ignore_ascii_case("content-length") {
            content_length = value
                .parse::<usize>()
                .map_err(|error| format!("VM HTTP Content-Length `{value}` is invalid: {error}"))?;
        }
        parsed_headers.push((header.name.to_string(), value));
    }
    Ok((method, uri, parsed_headers, content_length))
}

/// Parses HTTP/1 response headers with `httparse`.
#[cfg(test)]
pub(crate) fn parse_http1_response_content_length(
    bytes: &[u8],
    expected_status: u16,
) -> Result<usize, String> {
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut response = httparse::Response::new(&mut headers);
    match response
        .parse(bytes)
        .map_err(|error| format!("failed to parse VM HTTP response: {error}"))?
    {
        httparse::Status::Complete(_) => {}
        httparse::Status::Partial => {
            return Err("VM HTTP response parser reported partial headers".to_string());
        }
    }
    if response.code != Some(expected_status) {
        return Err(format!(
            "VM HTTP wire response returned unexpected status: {:?}",
            response.code
        ));
    }
    let mut content_length = None;
    for header in response.headers.iter() {
        if header.name.eq_ignore_ascii_case("content-length") {
            let value = std::str::from_utf8(header.value).map_err(|error| {
                format!("VM HTTP response Content-Length is not UTF-8: {error}")
            })?;
            content_length = Some(match value.parse::<usize>() {
                Ok(length) => length,
                Err(error) => {
                    return Err(format!(
                        "VM HTTP response Content-Length `{value}` is invalid: {error}"
                    ));
                }
            });
        }
    }
    content_length.ok_or_else(|| "VM HTTP response missing Content-Length".to_string())
}
