use std::io::{Read, Write};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

pub(super) const TEST_OWNER_ID: u64 = 1;

/// Reproduces the public format-1 export identity without consulting the
/// transitional serialized VMIR projection.
pub(super) fn native_export_id(module: &str, function: &str, arity: usize) -> u64 {
    let mut digest = Sha256::new();
    digest.update(b"terlan-tvm-export-v1\0");
    digest.update(module.as_bytes());
    digest.update(b"\0");
    digest.update(function.as_bytes());
    digest.update(b"\0");
    digest.update(arity.to_le_bytes());
    let bytes = digest.finalize();
    u64::from_le_bytes(bytes[..8].try_into().expect("SHA-256 export prefix")).max(1)
}

/// Resolves an export identity from the shared direct-AOT source fixture.
pub(super) fn direct_aot_export_id(function: &str) -> u64 {
    let source = include_str!("../fixtures/direct_aot.terl");
    let marker = format!("pub {function}(");
    let signature = source
        .split_once(&marker)
        .unwrap_or_else(|| panic!("missing direct-AOT function {function}"))
        .1;
    let parameters = signature
        .split_once(')')
        .unwrap_or_else(|| panic!("unterminated direct-AOT signature {function}"))
        .0
        .trim();
    let arity = if parameters.is_empty() {
        0
    } else {
        parameters.split(',').count()
    };
    native_export_id("direct_aot", function, arity)
}

pub(super) fn call_payload(request_id: u64, export_id: u64, arguments: &[i64]) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&request_id.to_le_bytes());
    payload.extend_from_slice(&TEST_OWNER_ID.to_le_bytes());
    payload.extend_from_slice(&export_id.to_le_bytes());
    payload.extend_from_slice(&(arguments.len() as u16).to_le_bytes());
    payload.extend_from_slice(&0_u16.to_le_bytes());
    for argument in arguments {
        payload.extend_from_slice(&argument.to_le_bytes());
    }
    payload
}

pub(super) fn exchange_worker_call(
    input: &mut impl Write,
    output: &mut impl Read,
    request_id: u64,
    export_id: u64,
    arguments: &[i64],
) -> (u16, Vec<u8>) {
    write_control_frame(input, 3, &call_payload(request_id, export_id, arguments));
    read_control_frame(output)
}

pub(super) fn exchange_worker_resume(
    input: &mut impl Write,
    output: &mut impl Read,
    request_id: u64,
    continuation_id: u64,
    values: &[i64],
) -> (u16, Vec<u8>) {
    write_control_frame(
        input,
        9,
        &resume_payload(request_id, continuation_id, values),
    );
    read_control_frame(output)
}

pub(super) fn frame_value(frame: &[u8]) -> i64 {
    i64::from_le_bytes(frame[16..24].try_into().expect("native result value"))
}

pub(super) fn transition_continuation(frame: &[u8]) -> u64 {
    u64::from_le_bytes(frame[16..24].try_into().expect("continuation identity"))
}

pub(super) fn transition_value_count(frame: &[u8]) -> u16 {
    u16::from_le_bytes(frame[28..30].try_into().expect("transition value count"))
}

pub(super) fn transition_value(frame: &[u8], index: usize) -> i64 {
    let argument_count = usize::from(u16::from_le_bytes(frame[26..28].try_into().unwrap()));
    let offset = 30 + argument_count * 8 + index * 8;
    i64::from_le_bytes(
        frame[offset..offset + 8]
            .try_into()
            .expect("transition value"),
    )
}

pub(super) struct SuspendedAction {
    pub(super) operation: u8,
    pub(super) request_id: u64,
    pub(super) continuation_id_xor: u64,
    pub(super) values: Vec<i64>,
}

pub(super) fn assert_suspended_worker_rejects(
    image_path: &std::path::Path,
    descriptor_digest: [u8; 32],
    export_id: u64,
    arguments: &[i64],
    action: SuspendedAction,
    expected_error: &str,
) {
    let mut worker = Command::new(env!("CARGO_BIN_EXE_terlan-native-worker"))
        .arg(image_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start adversarial suspended worker");
    let input = worker.stdin.as_mut().expect("adversarial worker stdin");
    write_control_frame(input, 1, &descriptor_digest);
    write_control_frame(input, 3, &call_payload(1, export_id, arguments));
    let output = worker.stdout.as_mut().expect("adversarial worker stdout");
    assert_eq!(read_control_frame(output), (2, descriptor_digest.to_vec()));
    let (transition_kind, transition) = read_control_frame(output);
    assert_eq!(transition_kind, 8);
    let continuation_id = transition_continuation(&transition);
    match action.operation {
        9 => write_control_frame(
            worker.stdin.as_mut().unwrap(),
            9,
            &resume_payload(
                action.request_id,
                continuation_id ^ action.continuation_id_xor,
                &action.values,
            ),
        ),
        10 => write_control_frame(
            worker.stdin.as_mut().unwrap(),
            9,
            &resume_payload_for_owner(1, TEST_OWNER_ID + 1, continuation_id, &action.values),
        ),
        3 => write_control_frame(
            worker.stdin.as_mut().unwrap(),
            3,
            &call_payload(2, export_id, arguments),
        ),
        6 => write_control_frame(worker.stdin.as_mut().unwrap(), 6, &[]),
        operation => panic!("unsupported suspended action operation {operation}"),
    }
    drop(worker.stdin.take());
    let result = worker
        .wait_with_output()
        .expect("wait for adversarial suspended worker");
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains(expected_error),
        "expected {expected_error}, got {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

pub fn assert_duplicate_resume_rejected(
    image_path: &std::path::Path,
    descriptor_digest: [u8; 32],
    export_id: u64,
    arguments: &[i64],
) {
    let mut worker = Command::new(env!("CARGO_BIN_EXE_terlan-native-worker"))
        .arg(image_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start duplicate-resume worker");
    write_control_frame(worker.stdin.as_mut().unwrap(), 1, &descriptor_digest);
    write_control_frame(
        worker.stdin.as_mut().unwrap(),
        3,
        &call_payload(1, export_id, arguments),
    );
    let output = worker.stdout.as_mut().unwrap();
    assert_eq!(read_control_frame(output), (2, descriptor_digest.to_vec()));
    let (_, transition) = read_control_frame(output);
    let continuation_id = transition_continuation(&transition);
    let values = (0..usize::from(transition_value_count(&transition)))
        .map(|index| transition_value(&transition, index))
        .collect::<Vec<_>>();
    write_control_frame(
        worker.stdin.as_mut().unwrap(),
        9,
        &resume_payload(1, continuation_id, &values),
    );
    assert_eq!(read_control_frame(output).0, 4);
    write_control_frame(
        worker.stdin.as_mut().unwrap(),
        9,
        &resume_payload(1, continuation_id, &values),
    );
    drop(worker.stdin.take());
    let result = worker
        .wait_with_output()
        .expect("wait for duplicate-resume worker");
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("error[native_worker.continuation_stale]")
    );
}

pub(super) fn resume_payload(request_id: u64, continuation_id: u64, values: &[i64]) -> Vec<u8> {
    resume_payload_for_owner(request_id, TEST_OWNER_ID, continuation_id, values)
}

fn resume_payload_for_owner(
    request_id: u64,
    owner_id: u64,
    continuation_id: u64,
    values: &[i64],
) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&request_id.to_le_bytes());
    payload.extend_from_slice(&owner_id.to_le_bytes());
    payload.extend_from_slice(&continuation_id.to_le_bytes());
    payload.extend_from_slice(&0_u16.to_le_bytes());
    payload.extend_from_slice(&(values.len() as u16).to_le_bytes());
    for value in values {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    payload
}

pub(super) fn write_control_frame(writer: &mut impl Write, kind: u16, payload: &[u8]) {
    writer.write_all(b"TVMC").expect("write control magic");
    writer
        .write_all(&1_u16.to_le_bytes())
        .expect("write control version");
    writer
        .write_all(&kind.to_le_bytes())
        .expect("write control kind");
    writer
        .write_all(&(payload.len() as u32).to_le_bytes())
        .expect("write control length");
    writer.write_all(payload).expect("write control payload");
    writer.flush().expect("flush control frame");
}

pub(super) fn read_control_frame(reader: &mut impl Read) -> (u16, Vec<u8>) {
    let mut header = [0_u8; 12];
    reader.read_exact(&mut header).expect("read control header");
    assert_eq!(&header[..4], b"TVMC");
    assert_eq!(u16::from_le_bytes(header[4..6].try_into().unwrap()), 1);
    let kind = u16::from_le_bytes(header[6..8].try_into().unwrap());
    let len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    let mut payload = vec![0; len];
    reader
        .read_exact(&mut payload)
        .expect("read control payload");
    (kind, payload)
}
