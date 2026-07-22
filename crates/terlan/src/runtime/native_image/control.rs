//! Bounded binary control protocol for supervised native TVM images.

use std::io::{Read, Write};

const MAGIC: &[u8; 4] = b"TVMC";
const VERSION: u16 = 1;
const HEADER_LEN: usize = 12;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

const HELLO: u16 = 1;
const HELLO_ACK: u16 = 2;
const CALL: u16 = 3;
const SUCCESS: u16 = 4;
const FAILURE: u16 = 5;
const SHUTDOWN: u16 = 6;
const SHUTDOWN_ACK: u16 = 7;
const TRANSITION: u16 = 8;
const RESUME: u16 = 9;

/// VM-owned operation requested by suspended native code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TvmTransitionOperation {
    Yield,
    Send,
    Receive,
    Spawn,
    Timer,
    Link,
    Monitor,
    Resource,
    Cancellation,
    Failure,
    Scheduling,
    Capability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TvmControlFrame {
    Hello {
        descriptor_digest: [u8; 32],
    },
    HelloAck {
        descriptor_digest: [u8; 32],
    },
    Call {
        request_id: u64,
        owner_id: u64,
        export_id: u64,
        arguments: Vec<i64>,
    },
    Success {
        request_id: u64,
        owner_id: u64,
        value: i64,
    },
    Failure {
        request_id: u64,
        owner_id: u64,
        status: i32,
    },
    Transition {
        request_id: u64,
        owner_id: u64,
        continuation_id: u64,
        operation: TvmTransitionOperation,
        arguments: Vec<i64>,
        values: Vec<i64>,
    },
    Resume {
        request_id: u64,
        owner_id: u64,
        continuation_id: u64,
        values: Vec<i64>,
    },
    Shutdown,
    ShutdownAck,
}

pub fn write_control_frame(writer: &mut impl Write, frame: &TvmControlFrame) -> Result<(), String> {
    let (kind, payload) = encode_payload(frame)?;
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| "error[tvm.control.frame_size]: frame is too large".to_string())?;
    writer
        .write_all(MAGIC)
        .and_then(|()| writer.write_all(&VERSION.to_le_bytes()))
        .and_then(|()| writer.write_all(&kind.to_le_bytes()))
        .and_then(|()| writer.write_all(&payload_len.to_le_bytes()))
        .and_then(|()| writer.write_all(&payload))
        .and_then(|()| writer.flush())
        .map_err(|error| format!("error[tvm.control.write]: {error}"))
}

pub fn read_control_frame(reader: &mut impl Read) -> Result<Option<TvmControlFrame>, String> {
    let mut header = [0_u8; HEADER_LEN];
    let read = reader
        .read(&mut header[..1])
        .map_err(|error| format!("error[tvm.control.read]: {error}"))?;
    if read == 0 {
        return Ok(None);
    }
    reader
        .read_exact(&mut header[1..])
        .map_err(|error| format!("error[tvm.control.header]: {error}"))?;
    if &header[..4] != MAGIC {
        return Err("error[tvm.control.magic]: invalid control-frame magic".to_string());
    }
    let version = u16::from_le_bytes([header[4], header[5]]);
    if version != VERSION {
        return Err(format!(
            "error[tvm.control.version]: unsupported control version {version}"
        ));
    }
    let kind = u16::from_le_bytes([header[6], header[7]]);
    let payload_len =
        u32::from_le_bytes(header[8..12].try_into().expect("fixed header range")) as usize;
    if payload_len > MAX_FRAME_BYTES {
        return Err(format!(
            "error[tvm.control.frame_size]: payload exceeds {MAX_FRAME_BYTES} bytes"
        ));
    }
    let mut payload = vec![0; payload_len];
    reader
        .read_exact(&mut payload)
        .map_err(|error| format!("error[tvm.control.payload]: {error}"))?;
    decode_payload(kind, &payload).map(Some)
}

fn encode_payload(frame: &TvmControlFrame) -> Result<(u16, Vec<u8>), String> {
    let mut payload = Vec::new();
    let kind = match frame {
        TvmControlFrame::Hello { descriptor_digest } => {
            payload.extend_from_slice(descriptor_digest);
            HELLO
        }
        TvmControlFrame::HelloAck { descriptor_digest } => {
            payload.extend_from_slice(descriptor_digest);
            HELLO_ACK
        }
        TvmControlFrame::Call {
            request_id,
            owner_id,
            export_id,
            arguments,
        } => {
            let arity = u16::try_from(arguments.len()).map_err(|_| {
                "error[tvm.control.arity]: native call has too many arguments".to_string()
            })?;
            payload.extend_from_slice(&request_id.to_le_bytes());
            payload.extend_from_slice(&owner_id.to_le_bytes());
            payload.extend_from_slice(&export_id.to_le_bytes());
            payload.extend_from_slice(&arity.to_le_bytes());
            payload.extend_from_slice(&0_u16.to_le_bytes());
            for argument in arguments {
                payload.extend_from_slice(&argument.to_le_bytes());
            }
            CALL
        }
        TvmControlFrame::Success {
            request_id,
            owner_id,
            value,
        } => {
            payload.extend_from_slice(&request_id.to_le_bytes());
            payload.extend_from_slice(&owner_id.to_le_bytes());
            payload.extend_from_slice(&value.to_le_bytes());
            SUCCESS
        }
        TvmControlFrame::Failure {
            request_id,
            owner_id,
            status,
        } => {
            payload.extend_from_slice(&request_id.to_le_bytes());
            payload.extend_from_slice(&owner_id.to_le_bytes());
            payload.extend_from_slice(&status.to_le_bytes());
            FAILURE
        }
        TvmControlFrame::Transition {
            request_id,
            owner_id,
            continuation_id,
            operation,
            arguments,
            values,
        } => {
            payload.extend_from_slice(&request_id.to_le_bytes());
            payload.extend_from_slice(&owner_id.to_le_bytes());
            payload.extend_from_slice(&continuation_id.to_le_bytes());
            payload.extend_from_slice(&transition_operation_tag(operation).to_le_bytes());
            push_transition_values(&mut payload, arguments, values)?;
            TRANSITION
        }
        TvmControlFrame::Resume {
            request_id,
            owner_id,
            continuation_id,
            values,
        } => {
            payload.extend_from_slice(&request_id.to_le_bytes());
            payload.extend_from_slice(&owner_id.to_le_bytes());
            payload.extend_from_slice(&continuation_id.to_le_bytes());
            payload.extend_from_slice(&0_u16.to_le_bytes());
            push_values(&mut payload, values)?;
            RESUME
        }
        TvmControlFrame::Shutdown => SHUTDOWN,
        TvmControlFrame::ShutdownAck => SHUTDOWN_ACK,
    };
    if payload.len() > MAX_FRAME_BYTES {
        return Err(format!(
            "error[tvm.control.frame_size]: payload exceeds {MAX_FRAME_BYTES} bytes"
        ));
    }
    Ok((kind, payload))
}

fn decode_payload(kind: u16, payload: &[u8]) -> Result<TvmControlFrame, String> {
    match kind {
        HELLO | HELLO_ACK if payload.len() == 32 => {
            let descriptor_digest = payload.try_into().expect("length checked above");
            Ok(if kind == HELLO {
                TvmControlFrame::Hello { descriptor_digest }
            } else {
                TvmControlFrame::HelloAck { descriptor_digest }
            })
        }
        CALL if payload.len() >= 28 => {
            let request_id = read_u64(payload, 0);
            let owner_id = read_u64(payload, 8);
            let export_id = read_u64(payload, 16);
            let arity = read_u16(payload, 24) as usize;
            if owner_id == 0
                || read_u16(payload, 26) != 0
                || payload.len() != 28 + arity.saturating_mul(8)
            {
                return Err("error[tvm.control.call]: malformed call payload".to_string());
            }
            let arguments = (0..arity)
                .map(|index| read_i64(payload, 28 + index * 8))
                .collect();
            Ok(TvmControlFrame::Call {
                request_id,
                owner_id,
                export_id,
                arguments,
            })
        }
        SUCCESS if payload.len() == 24 && read_u64(payload, 8) != 0 => {
            Ok(TvmControlFrame::Success {
                request_id: read_u64(payload, 0),
                owner_id: read_u64(payload, 8),
                value: read_i64(payload, 16),
            })
        }
        FAILURE if payload.len() == 20 && read_u64(payload, 8) != 0 => {
            Ok(TvmControlFrame::Failure {
                request_id: read_u64(payload, 0),
                owner_id: read_u64(payload, 8),
                status: read_i32(payload, 16),
            })
        }
        TRANSITION if payload.len() >= 30 => {
            if read_u64(payload, 8) == 0 {
                return Err(
                    "error[tvm.control.transition]: transition owner must be nonzero".to_string(),
                );
            }
            let operation = match read_u16(payload, 24) {
                1 => TvmTransitionOperation::Yield,
                2 => TvmTransitionOperation::Send,
                3 => TvmTransitionOperation::Receive,
                4 => TvmTransitionOperation::Spawn,
                5 => TvmTransitionOperation::Timer,
                6 => TvmTransitionOperation::Link,
                7 => TvmTransitionOperation::Monitor,
                8 => TvmTransitionOperation::Resource,
                9 => TvmTransitionOperation::Cancellation,
                10 => TvmTransitionOperation::Failure,
                11 => TvmTransitionOperation::Scheduling,
                12 => TvmTransitionOperation::Capability,
                tag => {
                    return Err(format!(
                        "error[tvm.control.transition]: unsupported transition operation {tag}"
                    ));
                }
            };
            let (arguments, values) = read_transition_values(payload, 26)?;
            Ok(TvmControlFrame::Transition {
                request_id: read_u64(payload, 0),
                owner_id: read_u64(payload, 8),
                continuation_id: read_u64(payload, 16),
                operation,
                arguments,
                values,
            })
        }
        RESUME if payload.len() >= 28 => {
            if read_u64(payload, 8) == 0 || read_u16(payload, 24) != 0 {
                return Err("error[tvm.control.resume]: reserved bits must be zero".to_string());
            }
            Ok(TvmControlFrame::Resume {
                request_id: read_u64(payload, 0),
                owner_id: read_u64(payload, 8),
                continuation_id: read_u64(payload, 16),
                values: read_values(payload, 26)?,
            })
        }
        SHUTDOWN if payload.is_empty() => Ok(TvmControlFrame::Shutdown),
        SHUTDOWN_ACK if payload.is_empty() => Ok(TvmControlFrame::ShutdownAck),
        HELLO | HELLO_ACK | CALL | SUCCESS | FAILURE | TRANSITION | RESUME | SHUTDOWN
        | SHUTDOWN_ACK => {
            Err("error[tvm.control.payload]: malformed control-frame payload".to_string())
        }
        _ => Err(format!(
            "error[tvm.control.kind]: unsupported control-frame kind {kind}"
        )),
    }
}

fn transition_operation_tag(operation: &TvmTransitionOperation) -> u16 {
    match operation {
        TvmTransitionOperation::Yield => 1,
        TvmTransitionOperation::Send => 2,
        TvmTransitionOperation::Receive => 3,
        TvmTransitionOperation::Spawn => 4,
        TvmTransitionOperation::Timer => 5,
        TvmTransitionOperation::Link => 6,
        TvmTransitionOperation::Monitor => 7,
        TvmTransitionOperation::Resource => 8,
        TvmTransitionOperation::Cancellation => 9,
        TvmTransitionOperation::Failure => 10,
        TvmTransitionOperation::Scheduling => 11,
        TvmTransitionOperation::Capability => 12,
    }
}

fn push_values(payload: &mut Vec<u8>, values: &[i64]) -> Result<(), String> {
    let count = u16::try_from(values.len())
        .map_err(|_| "error[tvm.control.values]: too many transition values".to_string())?;
    payload.extend_from_slice(&count.to_le_bytes());
    for value in values {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn push_transition_values(
    payload: &mut Vec<u8>,
    arguments: &[i64],
    values: &[i64],
) -> Result<(), String> {
    let argument_count = u16::try_from(arguments.len())
        .map_err(|_| "error[tvm.control.values]: too many transition arguments".to_string())?;
    let value_count = u16::try_from(values.len())
        .map_err(|_| "error[tvm.control.values]: too many transition values".to_string())?;
    payload.extend_from_slice(&argument_count.to_le_bytes());
    payload.extend_from_slice(&value_count.to_le_bytes());
    for value in arguments.iter().chain(values) {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn read_values(payload: &[u8], count_offset: usize) -> Result<Vec<i64>, String> {
    let count = read_u16(payload, count_offset) as usize;
    let values_offset = count_offset + 2;
    if payload.len() != values_offset + count.saturating_mul(8) {
        return Err("error[tvm.control.values]: malformed transition values".to_string());
    }
    Ok((0..count)
        .map(|index| read_i64(payload, values_offset + index * 8))
        .collect())
}

fn read_transition_values(
    payload: &[u8],
    count_offset: usize,
) -> Result<(Vec<i64>, Vec<i64>), String> {
    let argument_count = read_u16(payload, count_offset) as usize;
    let value_count = read_u16(payload, count_offset + 2) as usize;
    let values_offset = count_offset + 4;
    if payload.len() != values_offset + argument_count.saturating_add(value_count).saturating_mul(8)
    {
        return Err("error[tvm.control.values]: malformed transition values".to_string());
    }
    let read = |index| read_i64(payload, values_offset + index * 8);
    Ok((
        (0..argument_count).map(read).collect(),
        (argument_count..argument_count + value_count)
            .map(read)
            .collect(),
    ))
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("validated frame range"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("validated frame range"),
    )
}

fn read_i32(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated frame range"),
    )
}

fn read_i64(bytes: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("validated frame range"),
    )
}
