//! Bounded chunk transport shared verbatim with generated native helpers.

use std::io::{self, BufRead, Read, Write};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// Public adapter bounds, supplied by the caller's ABI contract.
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    /// Maximum physical frame, including its newline.
    pub(crate) frame: usize,
    /// Maximum decoded owned buffer in one call.
    pub(crate) transfer: usize,
}

impl Limits {
    /// Allows base64 expansion plus one frame of command metadata.
    pub(crate) fn message(self) -> usize {
        self.transfer.div_ceil(3) * 4 + self.frame
    }
}

/// Direction determines the chunk marker and assembled correlation header.
#[derive(Clone, Copy)]
pub(crate) enum Direction {
    /// A call from the VM to a helper.
    Call,
    /// A reply from a helper to the VM.
    Reply,
}

impl Direction {
    fn marker(self) -> &'static str {
        match self {
            Self::Call => "chunk",
            Self::Reply => "reply_chunk",
        }
    }

    fn header(self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::Reply => "reply",
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Writes a logical message without exceeding the physical frame bound.
pub(crate) fn write_message(
    output: &mut impl Write,
    message: &str,
    request_id: u64,
    direction: Direction,
    limits: Limits,
) -> io::Result<()> {
    if message.len() > limits.message() {
        return Err(invalid("native adapter transfer exceeds its bound"));
    }
    if message.len() < limits.frame {
        writeln!(output, "{message}")?;
    } else {
        let chunk_bytes = limits
            .frame
            .checked_sub(128)
            .filter(|size| *size >= 4)
            .ok_or_else(|| invalid("native adapter frame bound is too small"))?
            * 3
            / 4;
        let chunks = message.as_bytes().chunks(chunk_bytes);
        let count = chunks.len();
        for (index, chunk) in chunks.enumerate() {
            writeln!(
                output,
                "{} {request_id} {index} {} {}",
                direction.marker(),
                usize::from(index + 1 == count),
                STANDARD.encode(chunk)
            )?;
        }
    }
    output.flush()
}

/// Reads one correlated message, rejecting incomplete or interleaved transfers.
pub(crate) fn read_message(
    input: &mut impl BufRead,
    direction: Direction,
    expected_id: Option<u64>,
    limits: Limits,
) -> io::Result<Option<String>> {
    let mut bytes = Vec::new();
    let mut active = None;
    let mut next_index = 0_u64;
    loop {
        let mut frame = Vec::new();
        let read = Read::by_ref(input)
            .take((limits.frame + 1) as u64)
            .read_until(b'\n', &mut frame)?;
        if read == 0 && active.is_none() {
            return Ok(None);
        }
        if frame.len() > limits.frame {
            return Err(invalid("native adapter frame exceeds its bound"));
        }
        if frame.last() != Some(&b'\n') {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "native adapter transfer is incomplete",
            ));
        }
        let line = std::str::from_utf8(&frame)
            .map_err(|_| invalid("native adapter frame is not UTF-8"))?
            .trim_end_matches(['\r', '\n']);
        let mut fields = line.split_whitespace();
        if fields.next() != Some(direction.marker()) {
            if active.is_some() {
                return Err(invalid("native adapter transfer was interrupted"));
            }
            return Ok(Some(line.to_owned()));
        }
        let id = fields
            .next()
            .and_then(|field| field.parse::<u64>().ok())
            .ok_or_else(|| invalid("invalid native adapter chunk identity"))?;
        let index = fields
            .next()
            .and_then(|field| field.parse::<u64>().ok())
            .ok_or_else(|| invalid("invalid native adapter chunk index"))?;
        let final_chunk = match fields.next() {
            Some("0") => false,
            Some("1") => true,
            _ => return Err(invalid("invalid native adapter final-chunk marker")),
        };
        let encoded = fields
            .next()
            .ok_or_else(|| invalid("missing native adapter chunk data"))?;
        if fields.next().is_some()
            || index != next_index
            || active.is_some_and(|current| current != id)
            || expected_id.is_some_and(|expected| expected != id)
        {
            return Err(invalid(
                "native adapter chunk identity or sequence mismatch",
            ));
        }
        let chunk = STANDARD
            .decode(encoded)
            .map_err(|_| invalid("invalid native adapter chunk encoding"))?;
        if chunk.is_empty() || bytes.len().saturating_add(chunk.len()) > limits.message() {
            return Err(invalid("native adapter transfer exceeds its bound"));
        }
        bytes.extend_from_slice(&chunk);
        if final_chunk {
            let message = String::from_utf8(bytes)
                .map_err(|_| invalid("native adapter message is not UTF-8"))?;
            let mut header = message.split_whitespace();
            if header.next() != Some(direction.header())
                || header.next().and_then(|field| field.parse::<u64>().ok()) != Some(id)
                || message.contains(['\r', '\n'])
            {
                return Err(invalid("assembled native adapter identity mismatch"));
            }
            return Ok(Some(message));
        }
        active = Some(id);
        next_index = next_index
            .checked_add(1)
            .ok_or_else(|| invalid("native adapter chunk index overflow"))?;
    }
}
