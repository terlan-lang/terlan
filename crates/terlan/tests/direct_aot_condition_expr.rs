use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use object::{Object, ObjectSection};

#[path = "support/direct_aot.rs"]
pub mod support;
use support::*;
#[path = "support/direct_aot_resume.rs"]
mod resume_support;
use resume_support::resume_transition_success;

#[test]
fn native_aot_composes_non_linear_scalar_conditions_in_evaluation_order() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-condition-expr-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let source = root.join("direct_aot.terl");
    let output_dir = root.join("build");
    fs::create_dir_all(&root).expect("create condition-expression fixture root");
    fs::write(&source, include_str!("fixtures/direct_aot.terl"))
        .expect("write condition-expression fixture");
    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .output()
        .expect("start condition-expression fixture build");
    assert!(
        build.status.success(),
        "condition-expression fixture failed to build:\n{}",
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
        .expect("start condition-expression worker");
    write_control_frame(worker.stdin.as_mut().unwrap(), 1, &descriptor_digest);
    let output = worker.stdout.as_mut().unwrap();
    assert_eq!(read_control_frame(output), (2, descriptor_digest.to_vec()));
    let mut request = 1;

    for (value, expected) in [(11, 51), (10, 52)] {
        assert_yielding_result(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("yield_comparison_condition"),
            &[value],
            &[value],
            expected,
        );
        request += 1;
    }
    for (flag, expected) in [(0, 53), (1, 54)] {
        assert_yielding_result(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("yield_not_condition"),
            &[flag],
            &[flag],
            expected,
        );
        request += 1;
    }

    assert_immediate_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_and_rhs"),
        &[0, 1],
        56,
    );
    request += 1;
    for (right, expected) in [(1, 55), (0, 56)] {
        assert_yielding_result(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("yield_and_rhs"),
            &[1, right],
            &[right],
            expected,
        );
        request += 1;
    }

    assert_immediate_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_or_rhs"),
        &[1, 0],
        57,
    );
    request += 1;
    for (right, expected) in [(1, 57), (0, 58)] {
        assert_yielding_result(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("yield_or_rhs"),
            &[0, right],
            &[right],
            expected,
        );
        request += 1;
    }

    let (short_no_match_kind, short_no_match) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_and_rhs_only"),
        &[0, 1],
    );
    assert_native_error(short_no_match_kind, &short_no_match, 5);
    request += 1;
    let unmatched = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_and_rhs_only"),
        &[1, 0],
        &[0],
    );
    let (resumed_no_match_kind, resumed_no_match) = exchange_worker_resume(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        transition_continuation(&unmatched),
        &[0],
    );
    assert_native_error(resumed_no_match_kind, &resumed_no_match, 5);
    request += 1;

    let condition = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_and_rhs_then_body"),
        &[1, 1],
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
    assert_eq!(
        frame_value(&resume_success(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &body,
        )),
        60
    );
    request += 1;

    let selected_or = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_or_rhs_then_body"),
        &[1, 0],
        &[],
    );
    assert_eq!(
        frame_value(&resume_success(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &selected_or,
        )),
        74
    );
    request += 1;

    assert_yielding_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_or_rhs_then_body"),
        &[0, 0],
        &[0],
        75,
    );
    request += 1;

    let or_condition = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_or_rhs_then_body"),
        &[0, 1],
        &[1],
    );
    let (or_body_kind, or_body) = exchange_worker_resume(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        transition_continuation(&or_condition),
        &[1],
    );
    assert_eq!(or_body_kind, 8);
    assert_eq!(transition_value_count(&or_body), 0);
    assert_eq!(
        frame_value(&resume_success(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &or_body,
        )),
        74
    );
    request += 1;

    for (arguments, transition, expected) in [
        ([0, 1, 1], false, 63),
        ([1, 0, 1], false, 63),
        ([1, 1, 1], true, 62),
        ([1, 1, 0], true, 63),
    ] {
        if transition {
            assert_yielding_result(
                worker.stdin.as_mut().unwrap(),
                output,
                request,
                export_id("yield_nested_and_rhs"),
                &arguments,
                &arguments[2..],
                expected,
            );
        } else {
            assert_immediate_result(
                worker.stdin.as_mut().unwrap(),
                output,
                request,
                export_id("yield_nested_and_rhs"),
                &arguments,
                expected,
            );
        }
        request += 1;
    }

    let (checked_error_kind, checked_error) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("checked_before_yield_rhs"),
        &[0, 1],
    );
    assert_native_error(checked_error_kind, &checked_error, 4);
    request += 1;
    assert_yielding_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("checked_before_yield_rhs"),
        &[1, 1],
        &[1],
        64,
    );
    request += 1;

    let (eager_error_kind, eager_error) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_eager_right_condition"),
        &[0, 1],
    );
    assert_native_error(eager_error_kind, &eager_error, 4);
    request += 1;
    for (arguments, captures, expected) in [([1, 0], [0, 1], 68), ([2, 3], [3, 0], 69)] {
        assert_yielding_result(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("yield_eager_right_condition"),
            &arguments,
            &captures,
            expected,
        );
        request += 1;
    }

    let (value_error_kind, value_error) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_eager_value"),
        &[0, 5],
    );
    assert_native_error(value_error_kind, &value_error, 4);
    request += 1;
    assert_yielding_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_eager_value"),
        &[2, 5],
        &[5, 0],
        5,
    );
    request += 1;
    assert_yielding_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_unary_value"),
        &[3],
        &[3],
        -3,
    );
    request += 1;

    let (argument_error_kind, argument_error) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_second_call_argument"),
        &[0, 5],
    );
    assert_native_error(argument_error_kind, &argument_error, 4);
    request += 1;
    assert_yielding_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_second_call_argument"),
        &[2, 5],
        &[5, 0],
        5,
    );
    request += 1;

    let first_argument = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_first_call_argument"),
        &[5, 0],
        &[5, 0],
    );
    let (post_resume_error_kind, post_resume_error) = exchange_worker_resume(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        transition_continuation(&first_argument),
        &[5, 0],
    );
    assert_native_error(post_resume_error_kind, &post_resume_error, 4);
    request += 1;

    let unit_transition = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_only"),
        &[],
        &[],
    );
    assert_eq!(
        frame_value(&resume_success(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &unit_transition,
        )),
        0
    );
    request += 1;
    assert_immediate_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("unit_identity"),
        &[0],
        0,
    );
    request += 1;
    let first_unit_effect = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("two_unit_effects"),
        &[43],
        &[43],
    );
    let (second_unit_kind, second_unit_effect) = exchange_worker_resume(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        transition_continuation(&first_unit_effect),
        &[43],
    );
    assert_eq!(second_unit_kind, 8);
    assert_eq!(transition_value_count(&second_unit_effect), 1);
    assert_eq!(transition_value(&second_unit_effect, 0), 43);
    assert_eq!(
        frame_value(&resume_success(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &second_unit_effect,
        )),
        43
    );
    request += 1;
    let (mut effect_kind, mut effect_frame) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("eight_unit_effects"),
        &[44],
    );
    for _ in 0..8 {
        assert_eq!(effect_kind, 8);
        assert_eq!(transition_value_count(&effect_frame), 1);
        assert_eq!(transition_value(&effect_frame, 0), 44);
        (effect_kind, effect_frame) = exchange_worker_resume(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            transition_continuation(&effect_frame),
            &[44],
        );
    }
    assert_eq!(effect_kind, 4);
    assert_eq!(frame_value(&effect_frame), 44);
    request += 1;
    assert_yielding_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_unit_then_int"),
        &[41],
        &[41],
        42,
    );
    request += 1;
    assert_yielding_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("tail_yield_only"),
        &[],
        &[],
        0,
    );
    request += 1;
    assert_yielding_result(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("yield_unit_capture"),
        &[0],
        &[0],
        0,
    );

    write_control_frame(worker.stdin.as_mut().unwrap(), 6, &[]);
    assert_eq!(read_control_frame(output), (7, Vec::new()));
    drop(worker.stdin.take());
    let result = worker
        .wait_with_output()
        .expect("wait for condition-expression worker");
    assert!(
        result.status.success(),
        "condition-expression worker failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let rhs_export = export_id("yield_and_rhs");
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        rhs_export,
        &[1, 1],
        SuspendedAction {
            operation: 9,
            request_id: 2,
            continuation_id_xor: 0,
            values: vec![1],
        },
        "error[native_worker.continuation_stale]",
    );
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        rhs_export,
        &[1, 1],
        SuspendedAction {
            operation: 10,
            request_id: 0,
            continuation_id_xor: 0,
            values: vec![1],
        },
        "error[native_worker.continuation_owner]",
    );
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        rhs_export,
        &[1, 1],
        SuspendedAction {
            operation: 9,
            request_id: 1,
            continuation_id_xor: 0,
            values: vec![2],
        },
        "error[native_worker.boundary_type]",
    );
    assert_duplicate_resume_rejected(&image_path, descriptor_digest, rhs_export, &[1, 1]);
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        export_id("yield_unit_capture"),
        &[0],
        SuspendedAction {
            operation: 9,
            request_id: 1,
            continuation_id_xor: 0,
            values: vec![1],
        },
        "error[native_worker.boundary_type]",
    );
    fs::remove_dir_all(root).expect("remove condition-expression fixture root");
}

fn assert_immediate_result(
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

fn assert_yielding_result(
    input: &mut impl std::io::Write,
    output: &mut impl std::io::Read,
    request_id: u64,
    export_id: u64,
    arguments: &[i64],
    captures: &[i64],
    expected: i64,
) {
    let transition = expect_transition(input, output, request_id, export_id, arguments, captures);
    assert_eq!(
        frame_value(&resume_success(input, output, request_id, &transition)),
        expected
    );
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
    assert_eq!(
        usize::from(transition_value_count(&transition)),
        captures.len()
    );
    for (index, capture) in captures.iter().enumerate() {
        assert_eq!(transition_value(&transition, index), *capture);
    }
    transition
}

fn resume_success(
    input: &mut impl std::io::Write,
    output: &mut impl std::io::Read,
    request_id: u64,
    transition: &[u8],
) -> Vec<u8> {
    resume_transition_success(input, output, request_id, transition)
}

fn assert_native_error(kind: u16, frame: &[u8], expected_status: i32) {
    assert_eq!(kind, 5);
    assert_eq!(
        i32::from_le_bytes(frame[16..20].try_into().unwrap()),
        expected_status
    );
}
