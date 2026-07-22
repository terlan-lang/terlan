use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use object::{Object, ObjectSection};

#[path = "support/direct_aot.rs"]
#[allow(dead_code)]
mod support;
use support::*;

#[test]
fn native_aot_wraps_bounded_linear_multi_stage_callees() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-multi-stage-call-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let source = root.join("direct_aot.terl");
    let output_dir = root.join("build");
    fs::create_dir_all(&root).expect("create multi-stage fixture root");
    fs::write(&source, include_str!("fixtures/direct_aot.terl"))
        .expect("write multi-stage fixture");
    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .output()
        .expect("start multi-stage fixture build");
    assert!(
        build.status.success(),
        "multi-stage fixture failed to build:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let export_id = direct_aot_export_id;
    let image_path = output_dir.join("vm/direct_aot.tvm");
    let image_bytes = fs::read(&image_path).expect("read multi-stage image");
    let image = object::File::parse(&*image_bytes).expect("parse multi-stage image");
    let descriptor_section = if cfg!(target_os = "windows") {
        ".tvm$D"
    } else if cfg!(target_os = "macos") {
        "__tvm_desc"
    } else {
        ".note.terlan.tvm"
    };
    let descriptor = image
        .section_by_name(descriptor_section)
        .expect("multi-stage descriptor section")
        .data()
        .expect("read multi-stage descriptor");
    let descriptor_digest: [u8; 32] = descriptor[descriptor.len() - 32..]
        .try_into()
        .expect("multi-stage descriptor digest");

    let mut worker = Command::new(env!("CARGO_BIN_EXE_terlan-native-worker"))
        .arg(&image_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start multi-stage worker");
    write_control_frame(worker.stdin.as_mut().unwrap(), 1, &descriptor_digest);
    let output = worker.stdout.as_mut().unwrap();
    assert_eq!(read_control_frame(output), (2, descriptor_digest.to_vec()));
    let mut request = 1;

    let direct_first = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yielded_add_twice"),
        &[1],
        &[1],
    );
    let direct_first_id = transition_continuation(&direct_first);
    let direct_second = resume_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        &direct_first,
        8,
    );
    assert_captures(&direct_second, &[2]);
    let direct_second_id = transition_continuation(&direct_second);
    assert_ne!(direct_first_id, direct_second_id);
    assert_eq!(
        resume_value(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &direct_second
        ),
        3
    );
    request += 1;

    let caller_first = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_multi_stage"),
        &[],
        &[1],
    );
    let caller_first_id = transition_continuation(&caller_first);
    assert_ne!(caller_first_id, direct_first_id);
    let caller_second = resume_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        &caller_first,
        8,
    );
    assert_captures(&caller_second, &[2]);
    assert_ne!(transition_continuation(&caller_second), direct_second_id);
    assert_ne!(transition_continuation(&caller_second), caller_first_id);
    assert_eq!(
        resume_value(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &caller_second
        ),
        4
    );
    request += 1;

    let offset_first = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_multi_stage_offset"),
        &[10, 5],
        &[10, 5],
    );
    let offset_first_id = transition_continuation(&offset_first);
    let offset_second = resume_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        &offset_first,
        8,
    );
    assert_captures(&offset_second, &[11, 5]);
    assert_eq!(
        resume_value(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &offset_second
        ),
        17
    );
    request += 1;

    assert_two_stage_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_multi_stage_nested"),
        &[2],
        &[2],
        &[3],
        10,
    );
    request += 1;
    for (flag, expected) in [(0, 1), (1, 0)] {
        assert_two_stage_result(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("non_tail_multi_stage_bool"),
            &[flag],
            &[flag],
            &[flag],
            expected,
        );
        request += 1;
    }
    for (flag, expected) in [(0, 73), (1, 72)] {
        assert_two_stage_result(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("non_tail_multi_stage_condition"),
            &[flag],
            &[flag],
            &[flag],
            expected,
        );
        request += 1;
    }

    assert_immediate(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_multi_stage_branch"),
        &[0, 3],
        5,
    );
    request += 1;
    assert_two_stage_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_multi_stage_branch"),
        &[1, 3],
        &[3],
        &[4],
        9,
    );
    request += 1;

    let (checked_kind, checked) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_multi_stage_checked"),
        &[0],
    );
    assert_native_error(checked_kind, &checked, 4);
    request += 1;
    assert_two_stage_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_multi_stage_checked"),
        &[1],
        &[1],
        &[2],
        4,
    );
    request += 1;

    let tail_first = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("tail_multi_stage_composed"),
        &[10, 5],
        &[10, 5],
    );
    assert_eq!(transition_continuation(&tail_first), offset_first_id);
    let tail_second = resume_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        &tail_first,
        8,
    );
    assert_captures(&tail_second, &[11, 5]);
    assert_eq!(
        resume_value(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &tail_second
        ),
        17
    );
    request += 1;

    let mut bounded = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_eight"),
        &[9],
        &[9],
    );
    let mut bounded_ids = vec![transition_continuation(&bounded)];
    for _ in 1..8 {
        bounded = resume_transition(worker.stdin.as_mut().unwrap(), output, request, &bounded, 8);
        assert_captures(&bounded, &[9]);
        let id = transition_continuation(&bounded);
        assert!(!bounded_ids.contains(&id));
        bounded_ids.push(id);
    }
    assert_eq!(bounded_ids.len(), 8);
    assert_eq!(
        resume_value(worker.stdin.as_mut().unwrap(), output, request, &bounded),
        10
    );

    write_control_frame(worker.stdin.as_mut().unwrap(), 6, &[]);
    assert_eq!(read_control_frame(output), (7, Vec::new()));
    drop(worker.stdin.take());
    let result = worker
        .wait_with_output()
        .expect("wait for multi-stage worker");
    assert!(
        result.status.success(),
        "multi-stage worker failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bool_export = export_id("non_tail_multi_stage_bool");
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        bool_export,
        &[1],
        SuspendedAction::Resume {
            request_id: 2,
            continuation_id_xor: 0,
            values: vec![1],
        },
        "error[native_worker.continuation_stale]",
    );
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        bool_export,
        &[1],
        SuspendedAction::Resume {
            request_id: 1,
            continuation_id_xor: 0,
            values: vec![2],
        },
        "error[native_worker.boundary_type]",
    );
    assert_multi_stage_duplicate_resume_rejected(&image_path, descriptor_digest, bool_export, &[1]);
    fs::remove_dir_all(root).expect("remove multi-stage fixture root");
}

#[allow(clippy::too_many_arguments)]
fn assert_two_stage_result(
    input: &mut impl std::io::Write,
    output: &mut impl std::io::Read,
    request_id: u64,
    export_id: u64,
    arguments: &[i64],
    first_captures: &[i64],
    second_captures: &[i64],
    expected: i64,
) {
    let first = expect_transition(
        input,
        output,
        request_id,
        export_id,
        arguments,
        first_captures,
    );
    let second = resume_transition(input, output, request_id, &first, 8);
    assert_captures(&second, second_captures);
    assert_eq!(resume_value(input, output, request_id, &second), expected);
}

fn assert_immediate(
    input: &mut impl std::io::Write,
    output: &mut impl std::io::Read,
    request_id: u64,
    export_id: u64,
    arguments: &[i64],
    expected: i64,
) {
    let (kind, frame) = exchange_worker_call(input, output, request_id, export_id, arguments);
    assert_eq!(kind, 4);
    assert_eq!(frame_value(&frame), expected);
}

fn expect_transition(
    input: &mut impl std::io::Write,
    output: &mut impl std::io::Read,
    request_id: u64,
    export_id: u64,
    arguments: &[i64],
    captures: &[i64],
) -> Vec<u8> {
    let (kind, transition) = exchange_worker_call(input, output, request_id, export_id, arguments);
    assert_eq!(kind, 8);
    assert_captures(&transition, captures);
    transition
}

fn resume_transition(
    input: &mut impl std::io::Write,
    output: &mut impl std::io::Read,
    request_id: u64,
    transition: &[u8],
    expected_kind: u16,
) -> Vec<u8> {
    let values = transition_values(transition);
    let (kind, frame) = exchange_worker_resume(
        input,
        output,
        request_id,
        transition_continuation(transition),
        &values,
    );
    assert_eq!(kind, expected_kind);
    frame
}

fn resume_value(
    input: &mut impl std::io::Write,
    output: &mut impl std::io::Read,
    request_id: u64,
    transition: &[u8],
) -> i64 {
    let frame = resume_transition(input, output, request_id, transition, 4);
    frame_value(&frame)
}

fn transition_values(transition: &[u8]) -> Vec<i64> {
    (0..usize::from(transition_value_count(transition)))
        .map(|index| transition_value(transition, index))
        .collect()
}

fn assert_captures(transition: &[u8], captures: &[i64]) {
    assert_eq!(
        usize::from(transition_value_count(transition)),
        captures.len()
    );
    for (index, capture) in captures.iter().enumerate() {
        assert_eq!(transition_value(transition, index), *capture);
    }
}

fn assert_native_error(kind: u16, frame: &[u8], status: i32) {
    assert_eq!(kind, 5);
    assert_eq!(
        i32::from_le_bytes(frame[16..20].try_into().unwrap()),
        status
    );
}

fn assert_multi_stage_duplicate_resume_rejected(
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
        .expect("start multi-stage duplicate worker");
    write_control_frame(worker.stdin.as_mut().unwrap(), 1, &descriptor_digest);
    write_control_frame(
        worker.stdin.as_mut().unwrap(),
        3,
        &call_payload(1, export_id, arguments),
    );
    let output = worker.stdout.as_mut().unwrap();
    assert_eq!(read_control_frame(output), (2, descriptor_digest.to_vec()));
    let (first_kind, first) = read_control_frame(output);
    assert_eq!(first_kind, 8);
    let first_values = transition_values(&first);
    write_control_frame(
        worker.stdin.as_mut().unwrap(),
        9,
        &resume_payload(1, transition_continuation(&first), &first_values),
    );
    assert_eq!(read_control_frame(output).0, 8);
    write_control_frame(
        worker.stdin.as_mut().unwrap(),
        9,
        &resume_payload(1, transition_continuation(&first), &first_values),
    );
    drop(worker.stdin.take());
    let result = worker
        .wait_with_output()
        .expect("wait for multi-stage duplicate worker");
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("error[native_worker.continuation_stale]")
    );
}
