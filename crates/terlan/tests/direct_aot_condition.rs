use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use object::{Object, ObjectSection};

#[path = "support/direct_aot.rs"]
#[allow(dead_code)]
mod support;
use support::*;

#[test]
fn native_aot_composes_suspending_conditions_with_enclosing_control_flow() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-condition-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let source = root.join("direct_aot.terl");
    let output_dir = root.join("build");
    fs::create_dir_all(&root).expect("create condition fixture root");
    fs::write(&source, include_str!("fixtures/direct_aot.terl")).expect("write condition fixture");

    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .output()
        .expect("start condition fixture build");
    assert!(
        build.status.success(),
        "condition fixture failed to build:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let export_id = direct_aot_export_id;
    let image_path = output_dir.join("vm/direct_aot.tvm");
    let image_bytes = fs::read(&image_path).expect("read condition image");
    let image = object::File::parse(&*image_bytes).expect("parse condition image");
    let descriptor_section = if cfg!(target_os = "windows") {
        ".tvm$D"
    } else if cfg!(target_os = "macos") {
        "__tvm_desc"
    } else {
        ".note.terlan.tvm"
    };
    let descriptor = image
        .section_by_name(descriptor_section)
        .expect("condition descriptor section")
        .data()
        .expect("read condition descriptor");
    let descriptor_digest: [u8; 32] = descriptor[descriptor.len() - 32..]
        .try_into()
        .expect("condition descriptor digest");

    let mut worker = Command::new(env!("CARGO_BIN_EXE_terlan-native-worker"))
        .arg(&image_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start condition worker");
    write_control_frame(worker.stdin.as_mut().unwrap(), 1, &descriptor_digest);
    let output = worker.stdout.as_mut().unwrap();
    assert_eq!(read_control_frame(output), (2, descriptor_digest.to_vec()));

    let mut request = 1;
    for (arguments, expected) in [([1], 31), ([0], 32)] {
        let transition = expect_transition(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("yield_condition"),
            &arguments,
            &arguments,
        );
        let success = resume_once(worker.stdin.as_mut().unwrap(), output, request, &transition);
        assert_eq!(frame_value(&success), expected);
        request += 1;
    }

    let (early_kind, early) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_later_condition"),
        &[1, 0],
    );
    assert_eq!(early_kind, 4);
    assert_eq!(frame_value(&early), 33);
    request += 1;
    for (arguments, expected) in [([0, 1], 34), ([0, 0], 35)] {
        let transition = expect_transition(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("yield_later_condition"),
            &arguments,
            &arguments[1..],
        );
        let success = resume_once(worker.stdin.as_mut().unwrap(), output, request, &transition);
        assert_eq!(frame_value(&success), expected);
        request += 1;
    }

    for (arguments, expected) in [([1, 20, 22], 42), ([0, 20, 22], -2)] {
        let transition = expect_transition(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("yield_condition_capture"),
            &arguments,
            &arguments,
        );
        let success = resume_once(worker.stdin.as_mut().unwrap(), output, request, &transition);
        assert_eq!(frame_value(&success), expected);
        request += 1;
    }

    for (value, captures, expected) in [(10, [10, 11], 10), (5, [5, 6], 0)] {
        let transition = expect_transition(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("yield_condition_local"),
            &[value],
            &captures,
        );
        let success = resume_once(worker.stdin.as_mut().unwrap(), output, request, &transition);
        assert_eq!(frame_value(&success), expected);
        request += 1;
    }

    let (prefix_error_kind, prefix_error) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_condition_prefix_error"),
        &[0],
    );
    assert_eq!(prefix_error_kind, 5);
    assert_eq!(
        i32::from_le_bytes(prefix_error[16..20].try_into().unwrap()),
        4
    );
    request += 1;
    let checked_prefix = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_condition_prefix_error"),
        &[1],
        &[1],
    );
    let checked_success = resume_once(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        &checked_prefix,
    );
    assert_eq!(frame_value(&checked_success), 1);
    request += 1;

    for (flag, expected) in [(1, 36), (0, 37)] {
        let first = expect_transition(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("yield_condition_twice"),
            &[flag],
            &[flag],
        );
        let (second_kind, second) = exchange_worker_resume(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            transition_continuation(&first),
            &[flag],
        );
        assert_eq!(second_kind, 8);
        assert_eq!(transition_value_count(&second), 1);
        assert_eq!(transition_value(&second, 0), flag);
        assert_ne!(
            transition_continuation(&first),
            transition_continuation(&second)
        );
        let success = resume_once(worker.stdin.as_mut().unwrap(), output, request, &second);
        assert_eq!(frame_value(&success), expected);
        request += 1;
    }

    let condition = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_condition_then_body"),
        &[1],
        &[1],
    );
    let (body_kind, body) = exchange_worker_resume(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        transition_continuation(&condition),
        &[1],
    );
    assert_eq!(body_kind, 8);
    assert_eq!(transition_value_count(&body), 0);
    let success = resume_once(worker.stdin.as_mut().unwrap(), output, request, &body);
    assert_eq!(frame_value(&success), 38);
    request += 1;

    let (nested_fallback_kind, nested_fallback) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("nested_yield_condition"),
        &[0, 1],
    );
    assert_eq!(nested_fallback_kind, 4);
    assert_eq!(frame_value(&nested_fallback), 42);
    request += 1;
    for (arguments, expected) in [([1, 1], 40), ([1, 0], 41)] {
        let transition = expect_transition(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("nested_yield_condition"),
            &arguments,
            &arguments[1..],
        );
        let success = resume_once(worker.stdin.as_mut().unwrap(), output, request, &transition);
        assert_eq!(frame_value(&success), expected);
        request += 1;
    }

    for (arguments, expected) in [([0, 1], 0), ([1, 1], 1)] {
        let transition = expect_transition(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("yield_short_circuit_left"),
            &arguments,
            &arguments,
        );
        let success = resume_once(worker.stdin.as_mut().unwrap(), output, request, &transition);
        assert_eq!(frame_value(&success), expected);
        request += 1;
    }

    for arguments in [[0, 1], [1, 0]] {
        let transition = expect_transition(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("yield_short_circuit_or_left"),
            &arguments,
            &arguments,
        );
        let success = resume_once(worker.stdin.as_mut().unwrap(), output, request, &transition);
        assert_eq!(frame_value(&success), 1);
        request += 1;
    }

    let guarded = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_short_circuit_left_guard"),
        &[0],
        &[0],
    );
    let guarded_success = resume_once(worker.stdin.as_mut().unwrap(), output, request, &guarded);
    assert_eq!(frame_value(&guarded_success), 0);
    request += 1;
    let guarded_selected = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_short_circuit_left_guard"),
        &[1],
        &[1],
    );
    let (guarded_error_kind, guarded_error) = exchange_worker_resume(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        transition_continuation(&guarded_selected),
        &[1],
    );
    assert_eq!(guarded_error_kind, 5);
    assert_eq!(
        i32::from_le_bytes(guarded_error[16..20].try_into().unwrap()),
        4
    );
    request += 1;

    let or_guarded = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_short_circuit_or_guard"),
        &[1],
        &[1],
    );
    let or_guarded_success =
        resume_once(worker.stdin.as_mut().unwrap(), output, request, &or_guarded);
    assert_eq!(frame_value(&or_guarded_success), 1);
    request += 1;
    let or_selected = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_short_circuit_or_guard"),
        &[0],
        &[0],
    );
    let (or_error_kind, or_error) = exchange_worker_resume(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        transition_continuation(&or_selected),
        &[0],
    );
    assert_eq!(or_error_kind, 5);
    assert_eq!(i32::from_le_bytes(or_error[16..20].try_into().unwrap()), 4);
    request += 1;

    let unmatched = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_condition_only"),
        &[0],
        &[0],
    );
    let (unmatched_kind, unmatched_error) = exchange_worker_resume(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        transition_continuation(&unmatched),
        &[0],
    );
    assert_eq!(unmatched_kind, 5);
    assert_eq!(
        i32::from_le_bytes(unmatched_error[16..20].try_into().unwrap()),
        5
    );

    write_control_frame(worker.stdin.as_mut().unwrap(), 6, &[]);
    assert_eq!(read_control_frame(output), (7, Vec::new()));
    drop(worker.stdin.take());
    let result = worker
        .wait_with_output()
        .expect("wait for condition worker");
    assert!(
        result.status.success(),
        "condition worker failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let captured_condition = export_id("yield_condition_capture");
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        captured_condition,
        &[1, 20, 22],
        SuspendedAction::Resume {
            request_id: 2,
            continuation_id_xor: 0,
            values: vec![1, 20, 22],
        },
        "error[native_worker.continuation_stale]",
    );
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        captured_condition,
        &[1, 20, 22],
        SuspendedAction::Resume {
            request_id: 1,
            continuation_id_xor: 0,
            values: vec![1, 20],
        },
        "error[native_worker.continuation_type]",
    );
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        captured_condition,
        &[1, 20, 22],
        SuspendedAction::Resume {
            request_id: 1,
            continuation_id_xor: 0,
            values: vec![2, 20, 22],
        },
        "error[native_worker.boundary_type]",
    );
    assert_duplicate_resume_rejected(
        &image_path,
        descriptor_digest,
        export_id("yield_condition"),
        &[1],
    );
    fs::remove_dir_all(root).expect("remove condition fixture root");
}

fn expect_transition(
    input: &mut impl std::io::Write,
    output: &mut impl std::io::Read,
    request_id: u64,
    export_id: u64,
    arguments: &[i64],
    expected_values: &[i64],
) -> Vec<u8> {
    let (kind, transition) = exchange_worker_call(input, output, request_id, export_id, arguments);
    assert_eq!(kind, 8);
    assert_eq!(
        usize::from(transition_value_count(&transition)),
        expected_values.len()
    );
    for (index, expected) in expected_values.iter().enumerate() {
        assert_eq!(transition_value(&transition, index), *expected);
    }
    transition
}

fn resume_once(
    input: &mut impl std::io::Write,
    output: &mut impl std::io::Read,
    request_id: u64,
    transition: &[u8],
) -> Vec<u8> {
    let values = (0..usize::from(transition_value_count(transition)))
        .map(|index| transition_value(transition, index))
        .collect::<Vec<_>>();
    let (kind, success) = exchange_worker_resume(
        input,
        output,
        request_id,
        transition_continuation(transition),
        &values,
    );
    assert_eq!(kind, 4);
    success
}
