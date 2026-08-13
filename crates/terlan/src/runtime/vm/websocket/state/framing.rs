#[cfg(test)]
use super::*;

/// Encodes one server-to-client text frame through tungstenite.
///
/// Inputs:
/// - `text`: UTF-8 text payload to send to a browser/client endpoint.
///
/// Output:
/// - WebSocket frame bytes ready to write to a VM-owned stream.
///
/// Transformation:
/// - Delegates WebSocket frame serialization, opcode handling, and masking
///   rules to maintained tungstenite code while keeping the byte boundary
///   owned by the VM runtime.
#[cfg(test)]
pub(crate) fn encode_server_text_frame(text: &str) -> Result<Vec<u8>, String> {
    let stream = VmWebSocketMemoryStream::new(Vec::new());
    encode_server_text_frame_with_stream(text, stream)
}

/// Encodes one server text frame with an injected stream.
#[cfg(test)]
pub(in crate::runtime::vm::websocket) fn encode_server_text_frame_with_stream(
    text: &str,
    stream: VmWebSocketMemoryStream,
) -> Result<Vec<u8>, String> {
    let mut socket = WebSocket::from_raw_socket(stream, Role::Server, None);
    socket.send(Message::text(text)).map_err(|error| {
        format!("error[vm_websocket_frame]: failed to encode text frame: {error}")
    })?;
    Ok(socket.into_inner().written)
}

/// Decodes one client-to-server text frame through tungstenite.
///
/// Inputs:
/// - `frame`: bytes read from a VM-owned stream after the opening handshake.
///
/// Output:
/// - Decoded UTF-8 text payload, or a stable VM WebSocket diagnostic.
///
/// Transformation:
/// - Delegates WebSocket frame parsing, masking validation, and UTF-8 checks to
///   maintained tungstenite code while exposing a small VM-owned typed helper.
#[cfg(test)]
pub(crate) fn decode_client_text_frame(frame: &[u8]) -> Result<String, String> {
    let stream = VmWebSocketMemoryStream::new(frame.to_vec());
    let mut socket = WebSocket::from_raw_socket(stream, Role::Server, None);
    match socket.read().map_err(|error| {
        format!("error[vm_websocket_frame]: failed to decode text frame: {error}")
    })? {
        Message::Text(text) => Ok(text.to_string()),
        other => Err(format!(
            "error[vm_websocket_frame]: expected text frame, received {}",
            websocket_message_kind(&other)
        )),
    }
}

/// Encodes one server-to-client control frame through tungstenite.
///
/// Inputs:
/// - `frame`: typed VM WebSocket control operation.
///
/// Output:
/// - WebSocket frame bytes ready to write to a VM-owned stream.
///
/// Transformation:
/// - Delegates control-frame opcode, payload limit, and close-frame encoding
///   to maintained tungstenite code.
#[cfg(test)]
pub(crate) fn encode_server_control_frame(
    frame: VmWebSocketControlFrame,
) -> Result<Vec<u8>, String> {
    encode_control_frame(Role::Server, frame)
}

/// Decodes one client-to-server control frame through tungstenite.
///
/// Inputs:
/// - `frame`: bytes read from a VM-owned stream after the opening handshake.
///
/// Output:
/// - Typed VM control operation, or a stable diagnostic when a data frame is
///   received by a control-frame reader.
///
/// Transformation:
/// - Keeps VM session code on typed ping/pong/close events while tungstenite
///   owns frame validation.
#[cfg(test)]
pub(crate) fn decode_client_control_frame(frame: &[u8]) -> Result<VmWebSocketControlFrame, String> {
    let stream = VmWebSocketMemoryStream::new(frame.to_vec());
    let mut socket = WebSocket::from_raw_socket(stream, Role::Server, None);
    decode_control_message(socket.read().map_err(|error| {
        format!("error[vm_websocket_frame]: failed to decode control frame: {error}")
    })?)
}

/// Decodes one client-to-server text or control frame through tungstenite.
///
/// Inputs:
/// - `frame`: bytes read from a VM-owned stream after the opening handshake.
///
/// Output:
/// - Typed VM WebSocket frame event, or a stable diagnostic for unsupported
///   data frame kinds.
///
/// Transformation:
/// - Gives VM actors a single receive path while keeping protocol validation
///   in maintained tungstenite code.
#[cfg(test)]
pub(crate) fn decode_client_frame(frame: &[u8]) -> Result<VmWebSocketFrame, String> {
    let stream = VmWebSocketMemoryStream::new(frame.to_vec());
    let mut socket = WebSocket::from_raw_socket(stream, Role::Server, None);
    decode_message(
        socket.read().map_err(|error| {
            format!("error[vm_websocket_frame]: failed to decode frame: {error}")
        })?,
    )
}

/// Sends one server text frame over a VM TCP stream.
///
/// Inputs:
/// - `tcp`: VM TCP runtime that owns the stream resource.
/// - `stream`: accepted server-side stream handle.
/// - `text`: UTF-8 text payload to send.
///
/// Output:
/// - Number of frame bytes queued to the peer stream.
///
/// Transformation:
/// - Encodes the frame through tungstenite, then sends the resulting bytes
///   through VM-owned TCP without exposing host socket state.
#[cfg(test)]
pub(crate) fn send_server_text_frame(
    tcp: &mut VmTcpRuntime,
    stream: VmTcpStream,
    text: &str,
) -> Result<usize, String> {
    let frame = encode_server_text_frame(text)?;
    tcp.send(stream, frame)
        .map_err(|error| format!("error[vm_websocket_tcp]: failed to send text frame: {error}"))
}

/// Receives one client text frame from a VM TCP stream.
///
/// Inputs:
/// - `tcp`: VM TCP runtime that owns the stream resource.
/// - `stream`: accepted server-side stream handle.
/// - `max_bytes`: maximum bytes to read from the stream inbox.
///
/// Output:
/// - `None` when no bytes are queued, otherwise the decoded text payload.
///
/// Transformation:
/// - Reads bytes from VM TCP and delegates WebSocket frame validation to
///   tungstenite before returning a source-visible text payload.
#[cfg(test)]
pub(crate) fn receive_client_text_frame(
    tcp: &mut VmTcpRuntime,
    stream: VmTcpStream,
    max_bytes: usize,
) -> Result<Option<String>, String> {
    let Some(frame) = tcp.receive(stream, max_bytes).map_err(|error| {
        format!("error[vm_websocket_tcp]: failed to receive text frame: {error}")
    })?
    else {
        return Ok(None);
    };
    decode_client_text_frame(&frame).map(Some)
}

/// Sends one server control frame over a VM TCP stream.
///
/// Inputs:
/// - `tcp`: VM TCP runtime that owns the stream resource.
/// - `stream`: accepted server-side stream handle.
/// - `frame`: ping, pong, or close operation to send.
///
/// Output:
/// - Number of frame bytes queued to the peer stream.
///
/// Transformation:
/// - Encodes control frames through tungstenite, then sends the resulting bytes
///   through VM-owned TCP without exposing host socket state.
#[cfg(test)]
pub(crate) fn send_server_control_frame(
    tcp: &mut VmTcpRuntime,
    stream: VmTcpStream,
    frame: VmWebSocketControlFrame,
) -> Result<usize, String> {
    let bytes = encode_server_control_frame(frame)?;
    tcp.send(stream, bytes)
        .map_err(|error| format!("error[vm_websocket_tcp]: failed to send control frame: {error}"))
}

/// Receives one client control frame from a VM TCP stream.
///
/// Inputs:
/// - `tcp`: VM TCP runtime that owns the stream resource.
/// - `stream`: accepted server-side stream handle.
/// - `max_bytes`: maximum bytes to read from the stream inbox.
///
/// Output:
/// - `None` when no bytes are queued, otherwise a typed control frame.
///
/// Transformation:
/// - Reads bytes from VM TCP and delegates WebSocket control-frame validation
///   to tungstenite before returning scheduler-facing control events.
#[cfg(test)]
pub(crate) fn receive_client_control_frame(
    tcp: &mut VmTcpRuntime,
    stream: VmTcpStream,
    max_bytes: usize,
) -> Result<Option<VmWebSocketControlFrame>, String> {
    let Some(frame) = tcp.receive(stream, max_bytes).map_err(|error| {
        format!("error[vm_websocket_tcp]: failed to receive control frame: {error}")
    })?
    else {
        return Ok(None);
    };
    decode_client_control_frame(&frame).map(Some)
}

/// In-memory byte stream used to drive tungstenite without host sockets.
#[cfg(test)]
pub(in crate::runtime::vm::websocket) struct VmWebSocketMemoryStream {
    read: Cursor<Vec<u8>>,
    written: Vec<u8>,
    fail_writes: bool,
}

#[cfg(test)]
impl VmWebSocketMemoryStream {
    /// Creates a memory stream with preloaded inbound bytes.
    fn new(read_bytes: Vec<u8>) -> Self {
        Self {
            read: Cursor::new(read_bytes),
            written: Vec::new(),
            fail_writes: false,
        }
    }

    /// Creates a memory stream that fails writes for encode diagnostics.
    pub(in crate::runtime::vm::websocket) fn failing_writer() -> Self {
        Self {
            read: Cursor::new(Vec::new()),
            written: Vec::new(),
            fail_writes: true,
        }
    }
}

#[cfg(test)]
impl Read for VmWebSocketMemoryStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.read.read(buffer)
    }
}

#[cfg(test)]
impl Write for VmWebSocketMemoryStream {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.fail_writes {
            return Err(std::io::Error::other("injected websocket write failure"));
        }
        self.written.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Encodes one control frame for a websocket endpoint role.
#[cfg(test)]
fn encode_control_frame(role: Role, frame: VmWebSocketControlFrame) -> Result<Vec<u8>, String> {
    let stream = VmWebSocketMemoryStream::new(Vec::new());
    encode_control_frame_with_stream(role, frame, stream)
}

/// Encodes one control frame with an injected stream.
#[cfg(test)]
pub(in crate::runtime::vm::websocket) fn encode_control_frame_with_stream(
    role: Role,
    frame: VmWebSocketControlFrame,
    stream: VmWebSocketMemoryStream,
) -> Result<Vec<u8>, String> {
    let mut socket = WebSocket::from_raw_socket(stream, role, None);
    let message = match frame {
        VmWebSocketControlFrame::Ping(payload) => Message::Ping(payload.into()),
        VmWebSocketControlFrame::Pong(payload) => Message::Pong(payload.into()),
        VmWebSocketControlFrame::Close => Message::Close(None),
    };
    socket.send(message).map_err(|error| {
        format!("error[vm_websocket_frame]: failed to encode control frame: {error}")
    })?;
    Ok(socket.into_inner().written)
}

/// Converts a tungstenite message into a typed VM control frame.
#[cfg(test)]
fn decode_control_message(message: Message) -> Result<VmWebSocketControlFrame, String> {
    match message {
        Message::Ping(payload) => Ok(VmWebSocketControlFrame::Ping(payload.to_vec())),
        Message::Pong(payload) => Ok(VmWebSocketControlFrame::Pong(payload.to_vec())),
        Message::Close(_) => Ok(VmWebSocketControlFrame::Close),
        other => Err(format!(
            "error[vm_websocket_frame]: expected control frame, received {}",
            websocket_message_kind(&other)
        )),
    }
}

/// Converts a tungstenite message into a typed VM frame event.
#[cfg(test)]
fn decode_message(message: Message) -> Result<VmWebSocketFrame, String> {
    match message {
        Message::Text(text) => Ok(VmWebSocketFrame::Text(text.to_string())),
        Message::Ping(payload) => Ok(VmWebSocketFrame::Control(VmWebSocketControlFrame::Ping(
            payload.to_vec(),
        ))),
        Message::Pong(payload) => Ok(VmWebSocketFrame::Control(VmWebSocketControlFrame::Pong(
            payload.to_vec(),
        ))),
        Message::Close(_) => Ok(VmWebSocketFrame::Control(VmWebSocketControlFrame::Close)),
        other => Err(format!(
            "error[vm_websocket_frame]: unsupported frame kind {}",
            websocket_message_kind(&other)
        )),
    }
}

/// Returns a stable diagnostic name for a tungstenite message variant.
#[cfg(test)]
pub(in crate::runtime::vm::websocket) fn websocket_message_kind(message: &Message) -> &'static str {
    match message {
        Message::Text(_) => "text",
        Message::Binary(_) => "binary",
        Message::Ping(_) => "ping",
        Message::Pong(_) => "pong",
        Message::Close(_) => "close",
        Message::Frame(_) => "frame",
    }
}
