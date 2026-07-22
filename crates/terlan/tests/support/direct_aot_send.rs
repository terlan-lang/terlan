use std::path::Path;
use std::process::{Command, Stdio};

use super::support::{
    call_payload, frame_value, read_control_frame, resume_payload, transition_continuation,
    write_control_frame,
};

pub(super) fn assert_native_send_transitions(
    image_path: &Path,
    descriptor_digest: [u8; 32],
    direct_export: u64,
    call_export: u64,
) {
    let mut worker = Command::new(env!("CARGO_BIN_EXE_terlan-native-worker"))
        .arg(image_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start native Send worker");
    write_control_frame(worker.stdin.as_mut().unwrap(), 1, &descriptor_digest);
    assert_eq!(
        read_control_frame(worker.stdout.as_mut().unwrap()),
        (2, descriptor_digest.to_vec())
    );

    for (request_id, export) in [(1, direct_export), (2, call_export)] {
        write_control_frame(
            worker.stdin.as_mut().unwrap(),
            3,
            &call_payload(request_id, export, &[41]),
        );
        let (kind, transition) = read_control_frame(worker.stdout.as_mut().unwrap());
        assert_eq!(kind, 8);
        assert_send_frame(&transition);
        write_control_frame(
            worker.stdin.as_mut().unwrap(),
            9,
            &resume_payload(request_id, transition_continuation(&transition), &[41]),
        );
        let (success_kind, success) = read_control_frame(worker.stdout.as_mut().unwrap());
        assert_eq!(success_kind, 4);
        assert_eq!(frame_value(&success), 42);
    }

    write_control_frame(worker.stdin.as_mut().unwrap(), 6, &[]);
    assert_eq!(
        read_control_frame(worker.stdout.as_mut().unwrap()),
        (7, Vec::new())
    );
    drop(worker.stdin.take());
    assert!(worker.wait().expect("wait for Send worker").success());
}

fn assert_send_frame(frame: &[u8]) {
    assert_eq!(u16::from_le_bytes(frame[24..26].try_into().unwrap()), 2);
    assert_eq!(u16::from_le_bytes(frame[26..28].try_into().unwrap()), 2);
    assert_eq!(u16::from_le_bytes(frame[28..30].try_into().unwrap()), 1);
    assert_eq!(i64::from_le_bytes(frame[30..38].try_into().unwrap()), 1);
    assert_eq!(i64::from_le_bytes(frame[38..46].try_into().unwrap()), 41);
    assert_eq!(i64::from_le_bytes(frame[46..54].try_into().unwrap()), 41);
}
