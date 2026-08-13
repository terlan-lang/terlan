//! Dependency-free EPMD protocol constants and validation helpers.

use std::vec::Vec;

/// Maximum UTF-8 byte length accepted by OTP epmd for a node name.
pub const MAX_SYMBOL_LEN: usize = 255 * 4;

/// Maximum decoded UTF-8 scalar count accepted by OTP epmd for a node name.
pub const MAX_NODE_NAME_CHARS: usize = 255;

/// Request tag for ALIVE2 registration.
pub const ALIVE2_REQ: u8 = b'x';

/// Request tag for PORT2 lookup.
pub const PORT2_REQ: u8 = b'z';

/// Response tag for pre-version-6 ALIVE2 registrations.
pub const ALIVE2_RESP: u8 = b'y';

/// Response tag for version-6+ ALIVE2 registrations.
pub const ALIVE2_X_RESP: u8 = b'v';

/// Response tag for PORT2 lookups.
pub const PORT2_RESP: u8 = b'w';

/// Request tag for listing registered names.
pub const NAMES_REQ: u8 = b'n';

/// Request tag for dumping registered and unregistered names.
pub const DUMP_REQ: u8 = b'd';

/// Request tag for terminating epmd when no live nodes remain.
pub const KILL_REQ: u8 = b'k';

/// Request tag for stopping a registered node.
pub const STOP_REQ: u8 = b's';

/// EPMD validation failures that do not depend on transport/runtime state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationError {
    /// A node name was empty.
    EmptyName,
    /// A node name exceeded the OTP epmd maximum symbol length.
    NameTooLong,
    /// A node name contained an ASCII NUL byte.
    NameContainsNul,
    /// A node name was not valid UTF-8.
    InvalidNameEncoding,
    /// Extra data exceeded the maximum buffer accepted by OTP epmd.
    ExtraTooLong,
}

/// EPMD frame length validation failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    /// The packet is too short to contain the required length prefix.
    Incomplete,
    /// The packet length prefix does not match the available bytes.
    LengthMismatch,
}

/// EPMD payload-shape failures that do not require allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadError {
    /// The payload has no tag or lacks required bytes.
    Incomplete,
    /// The request tag is not the expected command byte.
    UnknownTag(u8),
    /// The request contains bytes after the expected payload.
    TrailingBytes,
    /// The request name failed validation.
    InvalidName(ValidationError),
}

/// Registration result byte for ALIVE2 responses.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RegistrationResult {
    /// Registration succeeded.
    Ok,
    /// Registration failed.
    Error,
}

/// A decoded epmd request payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Request {
    /// Register an alive node.
    Alive2(Alive2Request),
    /// Look up a registered node by name.
    Port2(NameRequest),
    /// List registered names.
    Names,
    /// Dump registered and unregistered names.
    Dump,
    /// Terminate epmd if allowed by server state.
    Kill,
    /// Stop a registered node by name.
    Stop(NameRequest),
}

/// Decoded fields from an ALIVE2 request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Alive2Request {
    /// TCP port used by the registering Erlang node.
    pub port: u16,
    /// OTP node type byte.
    pub node_type: u8,
    /// OTP distribution protocol byte.
    pub protocol: u8,
    /// Highest distribution version supported by the node.
    pub highest_version: u16,
    /// Lowest distribution version supported by the node.
    pub lowest_version: u16,
    /// UTF-8 encoded node name bytes.
    pub name: Vec<u8>,
    /// Opaque extra data bytes.
    pub extra: Vec<u8>,
}

/// A request that carries a node name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NameRequest {
    /// UTF-8 encoded node name bytes.
    pub name: Vec<u8>,
}

/// Encoded ALIVE2 registration response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Alive2Response {
    /// Whether registration succeeded.
    pub result: RegistrationResult,
    /// Creation value assigned by epmd.
    pub creation: u32,
    /// Whether to encode the version-6+ extended response form.
    pub extended_creation: bool,
}

/// Encoded PORT2 lookup response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Port2Response {
    /// The requested node exists.
    Found(Port2Found),
    /// The requested node does not exist.
    NotFound,
}

/// Fields returned for a successful PORT2 lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Port2Found {
    /// TCP port used by the registered Erlang node.
    pub port: u16,
    /// OTP node type byte.
    pub node_type: u8,
    /// OTP distribution protocol byte.
    pub protocol: u8,
    /// Highest distribution version supported by the node.
    pub highest_version: u16,
    /// Lowest distribution version supported by the node.
    pub lowest_version: u16,
    /// UTF-8 encoded node name bytes.
    pub name: Vec<u8>,
    /// Opaque extra data bytes.
    pub extra: Vec<u8>,
}

/// EPMD protocol parsing and encoding failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    /// The packet is too short to contain the required fields.
    Incomplete,
    /// The packet length prefix does not match the available bytes.
    LengthMismatch,
    /// The request tag is not part of the epmd protocol.
    UnknownTag(u8),
    /// The request contains bytes after the decoded payload.
    TrailingBytes,
    /// A node name was empty.
    EmptyName,
    /// A node name exceeded the OTP epmd maximum symbol length.
    NameTooLong,
    /// A node name contained an ASCII NUL byte.
    NameContainsNul,
    /// A node name was not valid UTF-8.
    InvalidNameEncoding,
    /// Extra data exceeded the maximum buffer accepted by OTP epmd.
    ExtraTooLong,
}

/// Returns the payload inside a length-prefixed EPMD request frame.
#[cfg(test)]
pub fn frame_payload(input: &[u8]) -> Result<&[u8], FrameError> {
    if input.len() < 2 {
        return Err(FrameError::Incomplete);
    }

    let payload_len = u16::from_be_bytes([input[0], input[1]]) as usize;
    let payload = &input[2..];
    if payload.len() != payload_len {
        return Err(FrameError::LengthMismatch);
    }

    Ok(payload)
}

/// Converts an EPMD request payload length into its two-byte frame prefix.
pub fn frame_payload_len(payload_len: usize) -> Result<u16, FrameError> {
    u16::try_from(payload_len).map_err(|_| FrameError::LengthMismatch)
}

/// Returns the wire byte for an ALIVE2 registration result.
pub fn registration_result_byte(result: RegistrationResult) -> u8 {
    match result {
        RegistrationResult::Ok => 0,
        RegistrationResult::Error => 1,
    }
}

/// Parse a length-prefixed EPMD request frame.
#[cfg(test)]
pub fn parse_frame(input: &[u8]) -> Result<Request, ProtocolError> {
    let payload = frame_payload(input).map_err(map_frame_error)?;
    parse_payload(payload)
}

/// Encode a length-prefixed EPMD request frame.
pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    let payload_len = frame_payload_len(payload.len()).map_err(map_frame_error)?;
    let mut frame = Vec::with_capacity(payload.len() + 2);
    push_u16(&mut frame, payload_len);
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Parse an EPMD request payload without its length prefix.
pub fn parse_payload(input: &[u8]) -> Result<Request, ProtocolError> {
    let tag = payload_tag(input).map_err(map_payload_error)?;
    match tag {
        ALIVE2_REQ => parse_alive2_request(input).map(Request::Alive2),
        PORT2_REQ => parse_name_request(input, PORT2_REQ).map(Request::Port2),
        STOP_REQ => parse_stop_request(input).map(Request::Stop),
        NAMES_REQ => parse_empty_command(input, NAMES_REQ, Request::Names),
        DUMP_REQ => parse_empty_command(input, DUMP_REQ, Request::Dump),
        KILL_REQ => parse_empty_command(input, KILL_REQ, Request::Kill),
        other => Err(map_payload_error(PayloadError::UnknownTag(other))),
    }
}

/// Parse an ALIVE2 payload without validating semantic field constraints.
pub fn parse_alive2_payload_unvalidated(input: &[u8]) -> Result<Alive2Request, ProtocolError> {
    let tag = *input.first().ok_or(ProtocolError::Incomplete)?;
    if tag != ALIVE2_REQ {
        return Err(ProtocolError::UnknownTag(tag));
    }

    let input = &input[1..];
    let (port, input) = read_u16(input)?;
    let (node_type, input) = read_u8(input)?;
    let (protocol, input) = read_u8(input)?;
    let (highest_version, input) = read_u16(input)?;
    let (lowest_version, input) = read_u16(input)?;
    let (name_len, input) = read_u16(input)?;
    let (name, input) = take(input, usize::from(name_len))?;
    let (extra_len, input) = read_u16(input)?;
    let (extra, input) = take(input, usize::from(extra_len))?;

    if !input.is_empty() {
        return Err(ProtocolError::TrailingBytes);
    }

    Ok(Alive2Request {
        port,
        node_type,
        protocol,
        highest_version,
        lowest_version,
        name: name.to_vec(),
        extra: extra.to_vec(),
    })
}

/// Return true when an error can be reported as an ALIVE2 registration failure.
pub fn is_alive2_validation_error(error: &ProtocolError) -> bool {
    matches!(
        error,
        ProtocolError::EmptyName
            | ProtocolError::NameTooLong
            | ProtocolError::NameContainsNul
            | ProtocolError::InvalidNameEncoding
            | ProtocolError::ExtraTooLong
    )
}

/// Encode an ALIVE2 response payload.
pub fn encode_alive2_response(response: &Alive2Response) -> Vec<u8> {
    let mut out = Vec::with_capacity(if response.extended_creation { 6 } else { 4 });
    out.push(if response.extended_creation {
        ALIVE2_X_RESP
    } else {
        ALIVE2_RESP
    });
    out.push(registration_result_byte(response.result));
    if response.extended_creation {
        push_u32(&mut out, response.creation);
    } else {
        push_u16(&mut out, (response.creation & u32::from(u16::MAX)) as u16);
    }
    out
}

/// Encode the ALIVE2 failure response used for semantically invalid requests.
pub fn encode_alive2_validation_error_response(extended_creation: bool) -> Vec<u8> {
    if extended_creation {
        encode_alive2_response(&Alive2Response {
            result: RegistrationResult::Error,
            creation: 0,
            extended_creation,
        })
    } else {
        vec![ALIVE2_RESP, 1]
    }
}

/// Encode a PORT2 response payload.
pub fn encode_port2_response(response: &Port2Response) -> Result<Vec<u8>, ProtocolError> {
    match response {
        Port2Response::NotFound => Ok(vec![PORT2_RESP, 1]),
        Port2Response::Found(found) => {
            validate_name(&found.name)?;
            validate_extra(&found.extra)?;
            let mut out = Vec::with_capacity(14 + found.name.len() + found.extra.len());
            out.push(PORT2_RESP);
            out.push(0);
            push_u16(&mut out, found.port);
            out.push(found.node_type);
            out.push(found.protocol);
            push_u16(&mut out, found.highest_version);
            push_u16(&mut out, found.lowest_version);
            push_u16(&mut out, found.name.len() as u16);
            out.extend_from_slice(&found.name);
            push_u16(&mut out, found.extra.len() as u16);
            out.extend_from_slice(&found.extra);
            Ok(out)
        }
    }
}

/// Returns the first request tag byte from an EPMD payload.
pub fn payload_tag(input: &[u8]) -> Result<u8, PayloadError> {
    input.first().copied().ok_or(PayloadError::Incomplete)
}

/// Validates an EPMD payload that must contain exactly one command tag.
pub fn validate_empty_command(input: &[u8], expected_tag: u8) -> Result<(), PayloadError> {
    match input {
        [tag] if *tag == expected_tag => Ok(()),
        [tag, ..] if *tag != expected_tag => Err(PayloadError::UnknownTag(*tag)),
        [_tag, ..] => Err(PayloadError::TrailingBytes),
        [] => Err(PayloadError::Incomplete),
    }
}

/// Returns validated name bytes from a tag-plus-name request payload.
pub fn name_payload(input: &[u8], expected_tag: u8) -> Result<&[u8], PayloadError> {
    let tag = payload_tag(input)?;
    if tag != expected_tag {
        return Err(PayloadError::UnknownTag(tag));
    }
    let name = &input[1..];
    validate_node_name(name).map_err(PayloadError::InvalidName)?;
    Ok(name)
}

/// Returns validated name bytes from a STOP request payload.
///
/// The C client historically sends a trailing NUL terminator for STOP. This
/// helper accepts that terminator and validates the stripped name.
pub fn stop_name_payload(input: &[u8]) -> Result<&[u8], PayloadError> {
    let tag = payload_tag(input)?;
    if tag != STOP_REQ {
        return Err(PayloadError::UnknownTag(tag));
    }
    let raw_name = &input[1..];
    let name = raw_name.strip_suffix(&[0]).unwrap_or(raw_name);
    validate_node_name(name).map_err(PayloadError::InvalidName)?;
    Ok(name)
}

/// Validates a node name according to the EPMD protocol layer rules.
pub fn validate_node_name(name: &[u8]) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::EmptyName);
    }
    if name.len() > MAX_SYMBOL_LEN {
        return Err(ValidationError::NameTooLong);
    }
    if name.contains(&0) {
        return Err(ValidationError::NameContainsNul);
    }
    let decoded = core::str::from_utf8(name).map_err(|_| ValidationError::InvalidNameEncoding)?;
    if decoded.chars().count() > MAX_NODE_NAME_CHARS {
        return Err(ValidationError::NameTooLong);
    }
    Ok(())
}

/// Validates opaque EPMD extra data.
pub fn validate_extra_data(extra: &[u8]) -> Result<(), ValidationError> {
    if extra.len() > MAX_SYMBOL_LEN {
        return Err(ValidationError::ExtraTooLong);
    }
    Ok(())
}

/// Validate a node name according to the EPMD protocol layer rules.
pub fn validate_name(name: &[u8]) -> Result<(), ProtocolError> {
    validate_node_name(name).map_err(map_validation_error)
}

/// Validate opaque extra data according to the EPMD protocol layer rules.
pub fn validate_extra(extra: &[u8]) -> Result<(), ProtocolError> {
    validate_extra_data(extra).map_err(map_validation_error)
}

/// Parses and validates one ALIVE2 registration payload.
fn parse_alive2_request(input: &[u8]) -> Result<Alive2Request, ProtocolError> {
    let request = parse_alive2_payload_unvalidated(input)?;
    validate_name(&request.name)?;
    validate_extra(&request.extra)?;
    Ok(request)
}

/// Parses one command whose remaining payload is a validated node name.
fn parse_name_request(input: &[u8], expected_tag: u8) -> Result<NameRequest, ProtocolError> {
    let name = name_payload(input, expected_tag).map_err(map_payload_error)?;
    Ok(NameRequest {
        name: name.to_vec(),
    })
}

/// Parses one STOP request with its optional trailing NUL byte.
fn parse_stop_request(input: &[u8]) -> Result<NameRequest, ProtocolError> {
    let name = stop_name_payload(input).map_err(map_payload_error)?;
    Ok(NameRequest {
        name: name.to_vec(),
    })
}

/// Parses one command that permits no payload after its tag.
fn parse_empty_command(
    input: &[u8],
    expected_tag: u8,
    request: Request,
) -> Result<Request, ProtocolError> {
    validate_empty_command(input, expected_tag).map_err(map_payload_error)?;
    Ok(request)
}

/// Consumes one unsigned byte and returns the remaining input.
fn read_u8(input: &[u8]) -> Result<(u8, &[u8]), ProtocolError> {
    let byte = *input.first().ok_or(ProtocolError::Incomplete)?;
    Ok((byte, &input[1..]))
}

/// Consumes one big-endian unsigned 16-bit value.
fn read_u16(input: &[u8]) -> Result<(u16, &[u8]), ProtocolError> {
    if input.len() < 2 {
        return Err(ProtocolError::Incomplete);
    }
    Ok((u16::from_be_bytes([input[0], input[1]]), &input[2..]))
}

/// Splits one exact bounded field from the remaining payload.
fn take(input: &[u8], len: usize) -> Result<(&[u8], &[u8]), ProtocolError> {
    if input.len() < len {
        return Err(ProtocolError::Incomplete);
    }
    Ok(input.split_at(len))
}

/// Appends one big-endian unsigned 16-bit value.
fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Appends one big-endian unsigned 32-bit value.
fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Promotes a frame boundary failure into the public protocol error.
fn map_frame_error(error: FrameError) -> ProtocolError {
    match error {
        FrameError::Incomplete => ProtocolError::Incomplete,
        FrameError::LengthMismatch => ProtocolError::LengthMismatch,
    }
}

/// Promotes a payload shape failure into the public protocol error.
fn map_payload_error(error: PayloadError) -> ProtocolError {
    match error {
        PayloadError::Incomplete => ProtocolError::Incomplete,
        PayloadError::UnknownTag(tag) => ProtocolError::UnknownTag(tag),
        PayloadError::TrailingBytes => ProtocolError::TrailingBytes,
        PayloadError::InvalidName(error) => map_validation_error(error),
    }
}

/// Promotes field validation failure into the public protocol error.
fn map_validation_error(error: ValidationError) -> ProtocolError {
    match error {
        ValidationError::EmptyName => ProtocolError::EmptyName,
        ValidationError::NameTooLong => ProtocolError::NameTooLong,
        ValidationError::NameContainsNul => ProtocolError::NameContainsNul,
        ValidationError::InvalidNameEncoding => ProtocolError::InvalidNameEncoding,
        ValidationError::ExtraTooLong => ProtocolError::ExtraTooLong,
    }
}
