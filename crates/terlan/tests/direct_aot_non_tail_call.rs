use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use object::{Object, ObjectSection};

#[path = "support/direct_aot.rs"]
#[allow(dead_code)]
mod support;
use support::*;

#[test]
fn native_aot_wraps_terminal_non_tail_calls_in_caller_continuations() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-non-tail-call-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let source = root.join("direct_aot.terl");
    let output_dir = root.join("build");
    fs::create_dir_all(&root).expect("create non-tail fixture root");
    fs::write(&source, include_str!("fixtures/direct_aot.terl")).expect("write non-tail fixture");
    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .output()
        .expect("start non-tail fixture build");
    assert!(
        build.status.success(),
        "non-tail fixture failed to build:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let export_id = direct_aot_export_id;
    let image_path = output_dir.join("vm/direct_aot.tvm");
    let image_bytes = fs::read(&image_path).expect("read non-tail image");
    let image = object::File::parse(&*image_bytes).expect("parse non-tail image");
    let descriptor_section = if cfg!(target_os = "windows") {
        ".tvm$D"
    } else if cfg!(target_os = "macos") {
        "__tvm_desc"
    } else {
        ".note.terlan.tvm"
    };
    let descriptor = image
        .section_by_name(descriptor_section)
        .expect("non-tail descriptor section")
        .data()
        .expect("read non-tail descriptor");
    let descriptor_digest: [u8; 32] = descriptor[descriptor.len() - 32..]
        .try_into()
        .expect("non-tail descriptor digest");

    let mut worker = Command::new(env!("CARGO_BIN_EXE_terlan-native-worker"))
        .arg(&image_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start non-tail worker");
    write_control_frame(worker.stdin.as_mut().unwrap(), 1, &descriptor_digest);
    let output = worker.stdout.as_mut().unwrap();
    assert_eq!(read_control_frame(output), (2, descriptor_digest.to_vec()));
    let mut request = 1;

    let callee = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("branch_yield"),
        &[1],
        &[],
    );
    let callee_continuation = transition_continuation(&callee);
    assert_eq!(
        resume_value(worker.stdin.as_mut().unwrap(), output, request, &callee),
        41
    );
    request += 1;

    assert_immediate(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_yielding"),
        &[0],
        8,
    );
    request += 1;
    let wrapped = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_yielding"),
        &[1],
        &[],
    );
    let wrapper_continuation = transition_continuation(&wrapped);
    assert_ne!(wrapper_continuation, callee_continuation);
    assert_eq!(
        resume_value(worker.stdin.as_mut().unwrap(), output, request, &wrapped),
        42
    );
    request += 1;

    assert_immediate(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_yielding_offset"),
        &[0, 9],
        16,
    );
    request += 1;
    assert_yielding(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_yielding_offset"),
        &[1, 9],
        &[9],
        50,
    );
    request += 1;

    for (name, flag, immediate, resumed) in [
        ("non_tail_nested", 0, 16, 84),
        ("non_tail_negated", 0, -7, -41),
        ("non_tail_call_argument", 0, 8, 42),
        ("non_tail_let", 0, 9, 43),
    ] {
        assert_immediate(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id(name),
            &[flag],
            immediate,
        );
        request += 1;
        assert_yielding(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id(name),
            &[1],
            &[],
            resumed,
        );
        request += 1;
    }

    let (let_error_kind, let_error) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_later_let"),
        &[0, 1],
    );
    assert_native_error(let_error_kind, &let_error, 4);
    request += 1;
    assert_immediate(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_later_let"),
        &[1, 0],
        8,
    );
    request += 1;
    assert_yielding(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_later_let"),
        &[2, 1],
        &[0],
        41,
    );
    request += 1;

    let (argument_error_kind, argument_error) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_second_call_argument"),
        &[0, 1],
    );
    assert_native_error(argument_error_kind, &argument_error, 4);
    request += 1;
    assert_immediate(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_second_call_argument"),
        &[1, 0],
        8,
    );
    request += 1;
    assert_yielding(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_second_call_argument"),
        &[2, 1],
        &[0],
        41,
    );
    request += 1;

    assert_immediate(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_local_capture"),
        &[0, 40, 2],
        42,
    );
    request += 1;
    assert_yielding(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_local_capture"),
        &[1, 40, 2],
        &[41, 2],
        44,
    );
    request += 1;

    for (flag, expected) in [(0, 1), (1, 0)] {
        assert_yielding(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("non_tail_bool"),
            &[flag],
            &[flag],
            expected,
        );
        request += 1;
    }
    for (flag, expected) in [(0, 71), (1, 70)] {
        assert_yielding(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("non_tail_condition"),
            &[flag],
            &[flag],
            expected,
        );
        request += 1;
    }

    let (checked_kind, checked) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_checked_argument"),
        &[0],
    );
    assert_native_error(checked_kind, &checked, 4);
    request += 1;
    assert_yielding(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_checked_argument"),
        &[1],
        &[],
        44,
    );
    request += 1;

    let (eager_error_kind, eager_error) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_eager_right"),
        &[0, 1],
    );
    assert_native_error(eager_error_kind, &eager_error, 4);
    request += 1;
    assert_immediate(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_eager_right"),
        &[1, 0],
        8,
    );
    request += 1;
    for (arguments, captures, expected) in [([1, 1], [1], 42), ([2, 1], [0], 41)] {
        assert_yielding(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("non_tail_eager_right"),
            &arguments,
            &captures,
            expected,
        );
        request += 1;
    }

    assert_immediate(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_two_calls"),
        &[0, 0],
        14,
    );
    request += 1;
    for (arguments, captures) in [([0, 1], [7]), ([1, 0], [0])] {
        assert_yielding(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("non_tail_two_calls"),
            &arguments,
            &captures,
            48,
        );
        request += 1;
    }
    let first_call = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_two_calls"),
        &[1, 1],
        &[1],
    );
    let (second_kind, second_call) = exchange_worker_resume(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        transition_continuation(&first_call),
        &[1],
    );
    assert_eq!(second_kind, 8);
    assert_eq!(transition_value_count(&second_call), 1);
    assert_eq!(transition_value(&second_call, 0), 41);
    assert_eq!(
        resume_value(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &second_call,
        ),
        82
    );
    request += 1;

    assert_immediate(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_eight_calls"),
        &[0, 0, 0, 0, 0, 0, 0, 0],
        56,
    );
    request += 1;
    let (mut chain_kind, mut chain_frame) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_eight_calls"),
        &[1, 1, 1, 1, 1, 1, 1, 1],
    );
    for _ in 0..8 {
        assert_eq!(chain_kind, 8);
        let captures = (0..usize::from(transition_value_count(&chain_frame)))
            .map(|index| transition_value(&chain_frame, index))
            .collect::<Vec<_>>();
        (chain_kind, chain_frame) = exchange_worker_resume(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            transition_continuation(&chain_frame),
            &captures,
        );
    }
    assert_eq!(chain_kind, 4);
    assert_eq!(frame_value(&chain_frame), 328);
    request += 1;

    assert_yielding(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("call_then_direct_yield"),
        &[0, 3],
        &[3, 7],
        10,
    );
    request += 1;
    let call_first = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("call_then_direct_yield"),
        &[1, 3],
        &[3],
    );
    let (direct_kind, direct_yield) = exchange_worker_resume(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        transition_continuation(&call_first),
        &[3],
    );
    assert_eq!(direct_kind, 8);
    assert_eq!(transition_value_count(&direct_yield), 2);
    assert_eq!(transition_value(&direct_yield, 0), 3);
    assert_eq!(transition_value(&direct_yield, 1), 41);
    assert_eq!(
        resume_value(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &direct_yield,
        ),
        44
    );
    request += 1;

    let direct_first = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("direct_yield_then_call"),
        &[3, 0],
        &[3, 0],
    );
    assert_eq!(
        resume_value(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &direct_first,
        ),
        10
    );
    request += 1;
    let direct_first = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("direct_yield_then_call"),
        &[3, 1],
        &[3, 1],
    );
    let (call_kind, call_second) = exchange_worker_resume(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        transition_continuation(&direct_first),
        &[3, 1],
    );
    assert_eq!(call_kind, 8);
    assert_eq!(transition_value_count(&call_second), 1);
    assert_eq!(transition_value(&call_second, 0), 3);
    assert_eq!(
        resume_value(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &call_second,
        ),
        44
    );
    request += 1;

    for (arguments, expected) in [([0, 1], 5), ([1, 0], 11)] {
        assert_immediate(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("non_tail_branch"),
            &arguments,
            expected,
        );
        request += 1;
    }
    assert_yielding(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("non_tail_branch"),
        &[1, 1],
        &[],
        45,
    );
    request += 1;
    let tail_wrapped = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        request,
        export_id("tail_composed_call"),
        &[1],
        &[],
    );
    assert_eq!(transition_continuation(&tail_wrapped), wrapper_continuation);
    assert_eq!(
        resume_value(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            &tail_wrapped
        ),
        42
    );

    write_control_frame(worker.stdin.as_mut().unwrap(), 6, &[]);
    assert_eq!(read_control_frame(output), (7, Vec::new()));
    drop(worker.stdin.take());
    let result = worker.wait_with_output().expect("wait for non-tail worker");
    assert!(
        result.status.success(),
        "non-tail worker failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bool_export = export_id("non_tail_bool");
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
    assert_duplicate_resume_rejected(&image_path, descriptor_digest, bool_export, &[1]);
    fs::remove_dir_all(root).expect("remove non-tail fixture root");
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

fn assert_yielding(
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
        resume_value(input, output, request_id, &transition),
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

fn resume_value(
    input: &mut impl std::io::Write,
    output: &mut impl std::io::Read,
    request_id: u64,
    transition: &[u8],
) -> i64 {
    let values = (0..usize::from(transition_value_count(transition)))
        .map(|index| transition_value(transition, index))
        .collect::<Vec<_>>();
    let (kind, frame) = exchange_worker_resume(
        input,
        output,
        request_id,
        transition_continuation(transition),
        &values,
    );
    assert_eq!(kind, 4);
    frame_value(&frame)
}

fn assert_native_error(kind: u16, frame: &[u8], status: i32) {
    assert_eq!(kind, 5);
    assert_eq!(
        i32::from_le_bytes(frame[16..20].try_into().unwrap()),
        status
    );
}
