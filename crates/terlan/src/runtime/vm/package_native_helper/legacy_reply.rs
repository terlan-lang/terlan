//! Streams bounded Bytes replies from helpers predating chunked transport.

use std::io::{self, BufRead, Cursor, Read};

use base64::engine::general_purpose::STANDARD;
use base64::read::DecoderReader;
use base64::Engine;

use super::transport::{self, Direction, Limits};
use super::{decode_reply, support::result_ok, ReplValue, VmRuntimeResult};

/// Enforces the decoded-buffer bound on chunked and direct codec callers.
pub(super) fn decode_bytes(encoded: &str) -> VmRuntimeResult<ReplValue> {
    let limit = crate::runtime::native_boundary::adapter_abi::PUBLIC_ADAPTER_MAX_TRANSFER_BYTES;
    if encoded.len() > limit.div_ceil(3) * 4 {
        return Err(protocol("native adapter binary transfer exceeds its bound"));
    }
    let bytes = STANDARD.decode(encoded).map_err(protocol)?;
    if bytes.len() > limit {
        return Err(protocol("native adapter binary transfer exceeds its bound"));
    }
    Ok(ReplValue::Bytes(bytes.into()))
}

/// Reads a reply while retaining compatibility with legacy binary exporters.
pub(super) fn read_reply(
    input: &mut impl BufRead,
    request_id: u64,
    limits: Limits,
) -> VmRuntimeResult<ReplValue> {
    let mut header = Vec::new();
    let mut fields = 0;
    let mut in_field = false;
    while fields < 4 {
        let mut byte = [0];
        input.read_exact(&mut byte).map_err(protocol)?;
        header.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
        if byte[0].is_ascii_whitespace() {
            fields += usize::from(in_field);
            in_field = false;
        } else {
            in_field = true;
        }
        if header.len() > 256 {
            return Err(protocol("native adapter reply header exceeds its bound"));
        }
    }
    let text = std::str::from_utf8(&header).map_err(protocol)?;
    let fields = text.split_whitespace().collect::<Vec<_>>();
    if let ["reply", id, "1", kind @ ("ok_bytes" | "result_ok_bytes")] = fields.as_slice() {
        if id.parse::<u64>().ok() != Some(request_id) {
            return Err(protocol("native adapter reply identity mismatch"));
        }
        let mut bytes = Vec::new();
        if header.last() != Some(&b'\n') {
            let mut line = EncodedLine {
                input,
                remaining: limits.transfer.div_ceil(3) * 4,
                finished: false,
            };
            DecoderReader::new(&mut line, &STANDARD)
                .take((limits.transfer + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(protocol)?;
            if bytes.len() > limits.transfer || !line.finished {
                return Err(protocol("native adapter binary transfer exceeds its bound"));
            }
        }
        let value = ReplValue::Bytes(bytes.into());
        return Ok(if *kind == "result_ok_bytes" {
            result_ok(value)
        } else {
            value
        });
    }
    let mut prefixed = Cursor::new(header).chain(input);
    let message =
        transport::read_message(&mut prefixed, Direction::Reply, Some(request_id), limits)
            .map_err(protocol)?
            .ok_or_else(|| protocol("helper exited without replying"))?;
    decode_reply(&message, request_id)
}

fn protocol(error: impl std::fmt::Display) -> super::VmRuntimeError {
    format!("error[native_helper_protocol]: {error}").into()
}

/// Exposes one encoded line to the streaming decoder without buffering it.
struct EncodedLine<'a, R> {
    input: &'a mut R,
    remaining: usize,
    finished: bool,
}

impl<R: BufRead> Read for EncodedLine<'_, R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.finished || output.is_empty() {
            return Ok(0);
        }
        let available = self.input.fill_buf()?;
        if available.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "native adapter binary reply is incomplete",
            ));
        }
        let end = available
            .iter()
            .position(|byte| matches!(byte, b'\r' | b'\n'))
            .unwrap_or(available.len());
        if end == 0 {
            let crlf = available[0] == b'\r';
            self.input.consume(1);
            if crlf {
                let mut newline = [0];
                self.input.read_exact(&mut newline)?;
                if newline[0] != b'\n' {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid CRLF"));
                }
            }
            self.finished = true;
            return Ok(0);
        }
        let count = end.min(output.len());
        self.remaining = self.remaining.checked_sub(count).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "binary reply exceeds its bound")
        })?;
        output[..count].copy_from_slice(&available[..count]);
        self.input.consume(count);
        Ok(count)
    }
}
