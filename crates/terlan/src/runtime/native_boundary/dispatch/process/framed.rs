//! Typed request decoding for phased byte-length-framed child protocols.

use crate::terlan_native_boundary::cancellation::NativeBoundaryCancellationToken;

use super::{
    execute_length_framed, field, parse_request, process_error, text_field, DispatchError,
    NativeBoundaryValue, ProcessRequest, MAX_FRAMED_EXCHANGES, MAX_FRAMED_RESPONSES,
    MAX_STDIN_BYTES,
};

pub(super) struct FramedExchangeRequest {
    pub(super) input: Vec<u8>,
    pub(super) expected_frames: usize,
}

pub(super) struct FramedProcessRequest {
    pub(super) command: ProcessRequest,
    pub(super) exchanges: Vec<FramedExchangeRequest>,
    pub(super) length_header: String,
}

pub(super) enum FrameSignal {
    Frame,
    End,
    Invalid(String),
}

/// Executes one phased byte-length-framed child protocol.
///
/// The VM retains ownership of the child and all pipes. Each exchange is
/// written and flushed before its declared number of frames is admitted, so a
/// caller can satisfy handshake protocols without exposing raw OS handles.
pub(crate) fn run_process_length_framed(
    args: &[NativeBoundaryValue],
    cancellation: Option<&NativeBoundaryCancellationToken>,
) -> Result<NativeBoundaryValue, DispatchError> {
    let request = match parse_framed_request(args) {
        Ok(request) => request,
        Err((message, program)) => return Ok(process_error("invalid_request", message, program)),
    };
    execute_length_framed(request, cancellation)
}

fn parse_framed_request(
    args: &[NativeBoundaryValue],
) -> Result<FramedProcessRequest, (String, String)> {
    let Some(NativeBoundaryValue::Record { fields, .. }) = args.first() else {
        return Err((
            "framed process request must be FramedRequest".to_string(),
            String::new(),
        ));
    };
    let command_value = field(fields, "command")?;
    let command = parse_request(std::slice::from_ref(command_value))?;
    let program = command.program.clone();
    let length_header = text_field(fields, "length_header")?.to_string();
    if !valid_header_name(&length_header) {
        return Err((
            "framed process length_header must be a nonempty ASCII token".to_string(),
            program,
        ));
    }
    let NativeBoundaryValue::List(exchange_values) = field(fields, "exchanges")? else {
        return Err((
            "framed process exchanges must be List[FramedExchange]".to_string(),
            program,
        ));
    };
    if exchange_values.len() > MAX_FRAMED_EXCHANGES {
        return Err((
            format!("framed process exceeds {MAX_FRAMED_EXCHANGES} exchanges"),
            program,
        ));
    }
    let mut total_input = command.stdin.len();
    let mut total_responses = 0usize;
    let mut exchanges = Vec::with_capacity(exchange_values.len());
    for exchange in exchange_values {
        let NativeBoundaryValue::Record { fields, .. } = exchange else {
            return Err((
                "framed process exchanges must contain FramedExchange values".to_string(),
                program,
            ));
        };
        let input = text_field(fields, "input")?.as_bytes().to_vec();
        let NativeBoundaryValue::Int(expected_frames) = field(fields, "expected_frames")? else {
            return Err((
                "FramedExchange expected_frames must be Int".to_string(),
                program,
            ));
        };
        let expected_frames = usize::try_from(*expected_frames).map_err(|_| {
            (
                "FramedExchange expected_frames must be nonnegative".to_string(),
                program.clone(),
            )
        })?;
        total_input = total_input.checked_add(input.len()).ok_or_else(|| {
            (
                "framed process input size overflow".to_string(),
                program.clone(),
            )
        })?;
        total_responses = total_responses
            .checked_add(expected_frames)
            .ok_or_else(|| {
                (
                    "framed process response count overflow".to_string(),
                    program.clone(),
                )
            })?;
        if total_input > MAX_STDIN_BYTES {
            return Err((
                format!("framed process input exceeds {MAX_STDIN_BYTES} bytes"),
                program,
            ));
        }
        if total_responses > MAX_FRAMED_RESPONSES {
            return Err((
                format!("framed process exceeds {MAX_FRAMED_RESPONSES} response frames"),
                program,
            ));
        }
        exchanges.push(FramedExchangeRequest {
            input,
            expected_frames,
        });
    }
    Ok(FramedProcessRequest {
        command,
        exchanges,
        length_header,
    })
}

fn valid_header_name(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

#[cfg(test)]
#[path = "framed_test.rs"]
mod tests;
