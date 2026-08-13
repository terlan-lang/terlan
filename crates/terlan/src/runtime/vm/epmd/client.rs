//! Deterministic EPMD client request and response helpers.

use std::{format, vec::Vec};

use super::protocol::{encode_frame, ProtocolError, NAMES_REQ, STOP_REQ};

/// Result of an EPMD client command.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub struct ClientOutput {
    /// Bytes that should be written to standard output.
    pub stdout: Vec<u8>,
    /// Process exit code requested by the command.
    pub exit_code: i32,
}

#[cfg(test)]
impl ClientOutput {
    /// Return successful command output.
    pub fn success(stdout: Vec<u8>) -> Self {
        Self {
            stdout,
            exit_code: 0,
        }
    }

    /// Return failed command output.
    pub fn failure(stdout: Vec<u8>) -> Self {
        Self {
            stdout,
            exit_code: 1,
        }
    }
}

/// Deterministic client-side command helper failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub enum ClientPlanError {
    /// The requested node name exceeds OTP epmd's stop-command limit.
    NameTooLong,
    /// Protocol frame encoding failed.
    Protocol(ProtocolError),
}

/// Return the length-prefixed request frame for `epmd -names` and `-started`.
#[cfg(test)]
pub fn names_request_frame() -> Result<Vec<u8>, ClientPlanError> {
    encode_frame(&[NAMES_REQ]).map_err(ClientPlanError::Protocol)
}

/// Return the length-prefixed request frame for `epmd -stop NAME`.
#[cfg(test)]
pub fn stop_request_frame(name: &str) -> Result<Vec<u8>, ClientPlanError> {
    if name.len() > 1000 {
        return Err(ClientPlanError::NameTooLong);
    }
    let mut payload = Vec::with_capacity(name.len() + 2);
    payload.push(STOP_REQ);
    payload.extend_from_slice(name.as_bytes());
    payload.push(0);
    encode_frame(&payload).map_err(ClientPlanError::Protocol)
}

/// Format a response from `epmd -names`, `-started`, or `-dump`.
#[cfg(test)]
pub fn names_like_client_output(response: &[u8], silent: bool) -> ClientOutput {
    if response.len() < 4 {
        return if silent {
            ClientOutput::failure(Vec::new())
        } else {
            ClientOutput::failure(b"epmd: no response from local epmd\n".to_vec())
        };
    }
    if silent {
        return ClientOutput::success(Vec::new());
    }
    let reported_port = u32::from_be_bytes([response[0], response[1], response[2], response[3]]);
    let mut stdout =
        format!("epmd: up and running on port {reported_port} with data:\n").into_bytes();
    stdout.extend_from_slice(&response[4..]);
    ClientOutput::success(stdout)
}

/// Format a response from `epmd -kill`.
#[cfg(test)]
pub fn kill_client_output(response: &[u8]) -> ClientOutput {
    if response == b"OK" {
        ClientOutput::success(b"Killed\n".to_vec())
    } else if response == b"NO" {
        ClientOutput::failure(b"Killing not allowed - living nodes in database.\n".to_vec())
    } else {
        local_response_failure(response)
    }
}

/// Return an OTP-compatible unexpected-response failure.
#[cfg(test)]
pub fn local_response_failure(response: &[u8]) -> ClientOutput {
    let mut stdout = b"epmd: local epmd responded with <".to_vec();
    stdout.extend_from_slice(response);
    stdout.extend_from_slice(b">\n");
    ClientOutput::failure(stdout)
}
