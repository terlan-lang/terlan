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
fn native_aot_forwards_suspending_tail_calls_without_a_caller_stack() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-tail-call-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let source = root.join("direct_aot.terl");
    let output_dir = root.join("build");
    fs::create_dir_all(&root).expect("create tail-call fixture root");
    fs::write(&source, include_str!("fixtures/direct_aot.terl")).expect("write tail-call fixture");
    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .output()
        .expect("start tail-call fixture build");
    assert!(
        build.status.success(),
        "tail-call fixture failed to build:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let export_id = direct_aot_export_id;
    let image_path = output_dir.join("vm/direct_aot.tvm");
    let image_bytes = fs::read(&image_path).expect("read tail-call image");
    let image = object::File::parse(&*image_bytes).expect("parse tail-call image");
    let descriptor_section = if cfg!(target_os = "windows") {
        ".tvm$D"
    } else if cfg!(target_os = "macos") {
        "__tvm_desc"
    } else {
        ".note.terlan.tvm"
    };
    let descriptor = image
        .section_by_name(descriptor_section)
        .expect("tail-call descriptor section")
        .data()
        .expect("read tail-call descriptor");
    let descriptor_digest: [u8; 32] = descriptor[descriptor.len() - 32..]
        .try_into()
        .expect("tail-call descriptor digest");

    let mut worker = Command::new(env!("CARGO_BIN_EXE_terlan-native-worker"))
        .arg(&image_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start tail-call worker");
    write_control_frame(worker.stdin.as_mut().unwrap(), 1, &descriptor_digest);
    let output = worker.stdout.as_mut().unwrap();
    assert_eq!(read_control_frame(output), (2, descriptor_digest.to_vec()));

    let (callee_kind, callee_transition) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        1,
        export_id("branch_yield"),
        &[1],
    );
    assert_eq!(callee_kind, 8);
    let forwarded_continuation = transition_continuation(&callee_transition);
    assert_eq!(transition_value_count(&callee_transition), 0);
    assert_eq!(
        frame_value(&resume_success(
            worker.stdin.as_mut().unwrap(),
            output,
            1,
            &callee_transition,
        )),
        41
    );

    let (pure_kind, pure) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        2,
        export_id("call_yielding"),
        &[0],
    );
    assert_eq!(pure_kind, 4);
    assert_eq!(frame_value(&pure), 7);
    let forwarded = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        3,
        export_id("call_yielding"),
        &[1],
        &[],
    );
    assert_eq!(transition_continuation(&forwarded), forwarded_continuation);
    assert_eq!(
        frame_value(&resume_success(
            worker.stdin.as_mut().unwrap(),
            output,
            3,
            &forwarded,
        )),
        41
    );

    let local = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        4,
        export_id("tail_yielding_local"),
        &[1, 40],
        &[41],
    );
    assert_eq!(
        frame_value(&resume_success(
            worker.stdin.as_mut().unwrap(),
            output,
            4,
            &local,
        )),
        42
    );
    let (local_fallback_kind, local_fallback) = exchange_worker_call(
        worker.stdin.as_mut().unwrap(),
        output,
        5,
        export_id("tail_yielding_local"),
        &[0, 40],
    );
    assert_eq!(local_fallback_kind, 4);
    assert_eq!(frame_value(&local_fallback), 40);

    for (request, arguments, captures, expected) in
        [(6, [1, 1], vec![], 41), (7, [0, 0], vec![41], 42)]
    {
        let transition = expect_transition(
            worker.stdin.as_mut().unwrap(),
            output,
            request,
            export_id("tail_yielding_branch"),
            &arguments,
            &captures,
        );
        assert_eq!(
            frame_value(&resume_success(
                worker.stdin.as_mut().unwrap(),
                output,
                request,
                &transition,
            )),
            expected
        );
    }

    let chain = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        8,
        export_id("tail_yielding_chain"),
        &[1],
        &[],
    );
    assert_eq!(transition_continuation(&chain), forwarded_continuation);
    assert_eq!(
        frame_value(&resume_success(
            worker.stdin.as_mut().unwrap(),
            output,
            8,
            &chain,
        )),
        41
    );

    let boolean = expect_transition(
        worker.stdin.as_mut().unwrap(),
        output,
        9,
        export_id("tail_yielding_bool"),
        &[1],
        &[1],
    );
    assert_eq!(
        frame_value(&resume_success(
            worker.stdin.as_mut().unwrap(),
            output,
            9,
            &boolean,
        )),
        1
    );

    write_control_frame(worker.stdin.as_mut().unwrap(), 6, &[]);
    assert_eq!(read_control_frame(output), (7, Vec::new()));
    drop(worker.stdin.take());
    let result = worker
        .wait_with_output()
        .expect("wait for tail-call worker");
    assert!(
        result.status.success(),
        "tail-call worker failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let tail_bool = export_id("tail_yielding_bool");
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        tail_bool,
        &[1],
        SuspendedAction {
            operation: 9,
            request_id: 1,
            continuation_id_xor: 0,
            values: vec![2],
        },
        "error[native_worker.boundary_type]",
    );
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        export_id("tail_yielding_local"),
        &[1, 40],
        SuspendedAction {
            operation: 9,
            request_id: 2,
            continuation_id_xor: 0,
            values: vec![41],
        },
        "error[native_worker.continuation_stale]",
    );
    assert_duplicate_resume_rejected(
        &image_path,
        descriptor_digest,
        export_id("call_yielding"),
        &[1],
    );
    fs::remove_dir_all(root).expect("remove tail-call fixture root");
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
