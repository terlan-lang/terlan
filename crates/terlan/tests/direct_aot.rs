use std::fs;
use std::io::{Cursor, Write};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use object::{Object, ObjectSection, ObjectSymbol};

#[path = "support/direct_aot.rs"]
pub mod support;
use support::*;
#[path = "support/direct_aot_managed.rs"]
mod managed_suite;
use managed_suite::assert_direct_managed_execution;
#[path = "support/direct_aot_rejection.rs"]
mod rejection_suite;
#[path = "support/direct_aot_transition_suite.rs"]
mod transition_suite;
use transition_suite::{assert_native_transition_suite, NativeTransitionExports};
#[test]
fn terlan_consumer_executes_descriptor_bearing_tvm_image() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create direct-AOT fixture root");
    let source = root.join("direct_aot.terl");
    let output_dir = root.join("build");
    fs::write(&source, include_str!("fixtures/direct_aot.terl"))
        .expect("write Terlan AOT consumer fixture");

    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .output()
        .expect("start terlc");
    assert!(
        build.status.success(),
        "terlc failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let export_id = direct_aot_export_id;
    let yielded_export_id = export_id("yielded");
    let yielded_add_export_id = export_id("yielded_add");
    let yielded_bool_export_id = export_id("yielded_bool");
    let yielded_local_from_export_id = export_id("yielded_local_from");
    let yielded_add_twice_export_id = export_id("yielded_add_twice");
    let yielded_bool_twice_export_id = export_id("yielded_bool_twice");
    let transition_exports = NativeTransitionExports {
        send: [
            export_id("send_capture_to_self"),
            export_id("send_call_capture_to_self"),
        ],
        receive: [
            export_id("receive_capture"),
            export_id("receive_call_capture"),
        ],
        spawn: [export_id("spawn_capture"), export_id("spawn_call_capture")],
        timer: [export_id("timer_capture"), export_id("timer_call_capture")],
        link: [export_id("link_capture"), export_id("link_call_capture")],
        monitor: [
            export_id("monitor_capture"),
            export_id("monitor_call_capture"),
        ],
        resource: [
            export_id("resource_capture"),
            export_id("resource_call_capture"),
        ],
        cancellation: [
            export_id("cancellation_capture"),
            export_id("cancellation_call_capture"),
        ],
        failure: [
            export_id("failure_capture"),
            export_id("failure_call_capture"),
        ],
        scheduling: [
            export_id("scheduling_capture"),
            export_id("scheduling_call_capture"),
        ],
    };
    let branch_yield_export_id = export_id("branch_yield");
    let branch_yield_local_export_id = export_id("branch_yield_local");
    let branch_yield_both_export_id = export_id("branch_yield_both");
    let nested_branch_yield_export_id = export_id("nested_branch_yield");
    let short_circuit_yield_export_id = export_id("short_circuit_yield");
    let yield_then_branch_export_id = export_id("yield_then_branch");
    let branch_capture_pair_export_id = export_id("branch_capture_pair");
    let branch_yield_only_export_id = export_id("branch_yield_only");
    let cached_build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("--incremental")
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output_dir)
        .env("RUSTC", root.join("rustc-must-not-run"))
        .env("TERLAN_NATIVE_LINKER", root.join("linker-must-not-run"))
        .output()
        .expect("start cached terlc build");
    assert!(
        cached_build.status.success(),
        "cached build repeated native work:\n{}",
        String::from_utf8_lossy(&cached_build.stderr)
    );

    let image_path = output_dir.join("vm/direct_aot.tvm");
    assert_eq!(
        image_path.extension().and_then(|value| value.to_str()),
        Some("tvm")
    );
    let image_bytes = fs::read(&image_path).expect("read TVM image");
    let image = object::File::parse(&*image_bytes).expect("parse TVM image");
    assert!(image.symbols().any(|symbol| {
        symbol.name().is_ok_and(|name| {
            name == "terlan_native_dispatch_v1" || name == "_terlan_native_dispatch_v1"
        })
    }));
    assert!(
        image.entry() != 0
            || image.symbols().any(|symbol| {
                symbol.name().is_ok_and(|name| {
                    name == "terlan_tvm_image_entry_v1" || name == "_terlan_tvm_image_entry_v1"
                })
            })
    );
    let descriptor_section = if cfg!(target_os = "windows") {
        ".tvm$D"
    } else if cfg!(target_os = "macos") {
        "__tvm_desc"
    } else {
        ".note.terlan.tvm"
    };
    let descriptor = image
        .section_by_name(descriptor_section)
        .expect("embedded TVM descriptor section")
        .data()
        .expect("read embedded TVM descriptor");
    assert_eq!(&descriptor[..8], b"TVMDSC01");
    assert!(output_dir.join(".terlan/native-aot").is_dir());
    assert!(!output_dir.join("vm/native").exists());
    let mut worker = Command::new(env!("CARGO_BIN_EXE_terlan-native-worker"))
        .arg(&image_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start native worker");
    let input = worker.stdin.as_mut().expect("native worker stdin");
    let descriptor_digest: [u8; 32] = descriptor[descriptor.len() - 32..]
        .try_into()
        .expect("descriptor digest");
    write_control_frame(input, 1, &descriptor_digest);
    for (request_id, export, arguments) in [
        (1, export_id("add"), vec![1, 2]),
        (2, export_id("subtract"), vec![7, 3]),
        (3, export_id("multiply"), vec![6, 7]),
        (4, export_id("divide"), vec![42, 6]),
        (5, export_id("remainder"), vec![43, 6]),
        (6, export_id("negate"), vec![7]),
        (7, export_id("add_twice"), vec![40, 1]),
        (8, export_id("classify"), vec![6]),
        (9, export_id("choose"), vec![0]),
        (10, export_id("choose"), vec![1]),
        (11, export_id("add"), vec![i64::MAX, 1]),
        (12, export_id("divide"), vec![1, 0]),
        (13, export_id("only_true"), vec![0]),
        (14, export_id("bool_not"), vec![0]),
        (15, export_id("bool_not"), vec![1]),
        (16, export_id("bool_and"), vec![1, 1]),
        (17, export_id("bool_and"), vec![1, 0]),
        (18, export_id("bool_or"), vec![0, 1]),
        (19, export_id("short_and"), vec![]),
        (20, export_id("short_or"), vec![]),
        (
            21,
            export_id("float_add"),
            vec![1.5_f64.to_bits() as i64, 2.25_f64.to_bits() as i64],
        ),
        (
            22,
            export_id("float_mixed"),
            vec![1, 2.75_f64.to_bits() as i64],
        ),
        (
            23,
            export_id("float_compare"),
            vec![3.75_f64.to_bits() as i64, 2],
        ),
        (
            24,
            export_id("float_divide"),
            vec![1.0_f64.to_bits() as i64, 0.0_f64.to_bits() as i64],
        ),
    ] {
        write_control_frame(input, 3, &call_payload(request_id, export, &arguments));
    }
    write_control_frame(input, 6, &[]);
    drop(worker.stdin.take());
    let worker_output = worker.wait_with_output().expect("wait for native worker");
    assert!(worker_output.status.success());
    let mut replies = Cursor::new(worker_output.stdout);
    assert_eq!(
        read_control_frame(&mut replies),
        (2, descriptor_digest.to_vec())
    );
    for (request_id, expected) in [
        (1, 3),
        (2, 4),
        (3, 42),
        (4, 7),
        (5, 1),
        (6, -7),
        (7, 42),
        (8, 13),
        (9, 0),
        (10, 1),
    ] {
        let (kind, payload) = read_control_frame(&mut replies);
        assert_eq!(kind, 4);
        assert_eq!(
            u64::from_le_bytes(payload[..8].try_into().unwrap()),
            request_id
        );
        assert_eq!(
            i64::from_le_bytes(payload[16..24].try_into().unwrap()),
            expected
        );
    }
    for (request_id, expected_status) in [(11, 3_i32), (12, 4_i32), (13, 5_i32)] {
        let (kind, payload) = read_control_frame(&mut replies);
        assert_eq!(kind, 5);
        assert_eq!(
            u64::from_le_bytes(payload[..8].try_into().unwrap()),
            request_id
        );
        assert_eq!(
            i32::from_le_bytes(payload[16..20].try_into().unwrap()),
            expected_status
        );
    }
    for (request_id, expected) in [
        (14, 1),
        (15, 0),
        (16, 1),
        (17, 0),
        (18, 1),
        (19, 0),
        (20, 1),
        (21, 3.75_f64.to_bits() as i64),
        (22, 3.75_f64.to_bits() as i64),
        (23, 1),
    ] {
        let (kind, payload) = read_control_frame(&mut replies);
        assert_eq!(kind, 4);
        assert_eq!(
            u64::from_le_bytes(payload[..8].try_into().unwrap()),
            request_id
        );
        assert_eq!(
            i64::from_le_bytes(payload[16..24].try_into().unwrap()),
            expected
        );
    }
    let (kind, payload) = read_control_frame(&mut replies);
    assert_eq!(kind, 5);
    assert_eq!(u64::from_le_bytes(payload[..8].try_into().unwrap()), 24);
    assert_eq!(i32::from_le_bytes(payload[16..20].try_into().unwrap()), 19);
    assert_eq!(read_control_frame(&mut replies), (7, Vec::new()));

    assert_native_transition_suite(&image_path, descriptor_digest, transition_exports);

    let mut replay_worker = Command::new(env!("CARGO_BIN_EXE_terlan-native-worker"))
        .arg(&image_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start request-order worker");
    let replay_input = replay_worker.stdin.as_mut().expect("replay worker stdin");
    write_control_frame(replay_input, 1, &descriptor_digest);
    let replay_call = call_payload(1, export_id("add"), &[1, 2]);
    write_control_frame(replay_input, 3, &replay_call);
    write_control_frame(replay_input, 3, &replay_call);
    drop(replay_worker.stdin.take());
    let replay_output = replay_worker
        .wait_with_output()
        .expect("wait for request-order worker");
    assert!(!replay_output.status.success());
    assert!(String::from_utf8_lossy(&replay_output.stderr)
        .contains("error[native_worker.request_order]"));

    let load = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("load")
        .arg(&image_path)
        .output()
        .expect("load self-describing TVM image");
    assert!(
        load.status.success(),
        "terlan-vm could not load the native image:\n{}",
        String::from_utf8_lossy(&load.stderr)
    );

    let run = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&image_path)
        .arg("--entry")
        .arg("main")
        .arg("--test-eval")
        .env_remove("TERLAN_NATIVE_WORKER")
        .output()
        .expect("run Terlan consumer");
    assert!(
        run.status.success(),
        "terlan-vm failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let yielded = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&image_path)
        .arg("--entry")
        .arg("yielded")
        .arg("--test-eval")
        .env_remove("TERLAN_NATIVE_WORKER")
        .output()
        .expect("run yielded Terlan consumer");
    assert!(
        yielded.status.success(),
        "terlan-vm could not resume native continuation:\n{}",
        String::from_utf8_lossy(&yielded.stderr)
    );

    assert_direct_managed_execution(&image_path, descriptor);

    let yielded_local = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&image_path)
        .arg("--entry")
        .arg("yielded_local")
        .arg("--test-eval")
        .env_remove("TERLAN_NATIVE_WORKER")
        .output()
        .expect("run live-local Terlan consumer");
    assert!(
        yielded_local.status.success(),
        "terlan-vm could not resume live local:\n{}",
        String::from_utf8_lossy(&yielded_local.stderr)
    );

    let yielded_twice = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&image_path)
        .arg("--entry")
        .arg("yielded_twice")
        .arg("--test-eval")
        .env_remove("TERLAN_NATIVE_WORKER")
        .output()
        .expect("run repeated-yield Terlan consumer");
    assert!(
        yielded_twice.status.success(),
        "terlan-vm could not drive repeated native transitions:\n{}",
        String::from_utf8_lossy(&yielded_twice.stderr)
    );

    let sent_to_self = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&image_path)
        .arg("--entry")
        .arg("send_to_self")
        .arg("--test-eval")
        .env_remove("TERLAN_NATIVE_WORKER")
        .output()
        .expect("run native Send Terlan consumer");
    assert!(
        sent_to_self.status.success(),
        "terlan-vm could not service native Send transition:\n{}",
        String::from_utf8_lossy(&sent_to_self.stderr)
    );

    let received_from_self = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(&image_path)
        .arg("--entry")
        .arg("send_then_receive_call")
        .arg("--test-eval")
        .env_remove("TERLAN_NATIVE_WORKER")
        .output()
        .expect("run native Receive Terlan consumer");
    assert!(
        received_from_self.status.success(),
        "terlan-vm could not service native Receive transition:\n{}",
        String::from_utf8_lossy(&received_from_self.stderr)
    );

    let mut captured_worker = Command::new(env!("CARGO_BIN_EXE_terlan-native-worker"))
        .arg(&image_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start captured-continuation worker");
    {
        let input = captured_worker
            .stdin
            .as_mut()
            .expect("captured-continuation worker stdin");
        write_control_frame(input, 1, &descriptor_digest);
        write_control_frame(input, 3, &call_payload(1, yielded_add_export_id, &[41]));
    }
    let captured_output = captured_worker
        .stdout
        .as_mut()
        .expect("captured-continuation worker stdout");
    assert_eq!(
        read_control_frame(captured_output),
        (2, descriptor_digest.to_vec())
    );
    let (transition_kind, transition) = read_control_frame(captured_output);
    assert_eq!(transition_kind, 8);
    assert_eq!(u64::from_le_bytes(transition[..8].try_into().unwrap()), 1);
    let captured_continuation = u64::from_le_bytes(transition[16..24].try_into().unwrap());
    assert_eq!(
        u16::from_le_bytes(transition[24..26].try_into().unwrap()),
        1
    );
    assert_eq!(
        u16::from_le_bytes(transition[26..28].try_into().unwrap()),
        0
    );
    assert_eq!(
        u16::from_le_bytes(transition[28..30].try_into().unwrap()),
        1
    );
    assert_eq!(
        i64::from_le_bytes(transition[30..38].try_into().unwrap()),
        41
    );
    write_control_frame(
        captured_worker.stdin.as_mut().unwrap(),
        9,
        &resume_payload(1, captured_continuation, &[41]),
    );
    let (success_kind, success) = read_control_frame(captured_output);
    assert_eq!(success_kind, 4);
    assert_eq!(u64::from_le_bytes(success[..8].try_into().unwrap()), 1);
    assert_eq!(i64::from_le_bytes(success[16..24].try_into().unwrap()), 42);
    write_control_frame(
        captured_worker.stdin.as_mut().unwrap(),
        3,
        &call_payload(2, yielded_local_from_export_id, &[21]),
    );
    let (local_kind, local_transition) = read_control_frame(captured_output);
    assert_eq!(local_kind, 8);
    assert_eq!(
        u64::from_le_bytes(local_transition[..8].try_into().unwrap()),
        2
    );
    let local_continuation = u64::from_le_bytes(local_transition[16..24].try_into().unwrap());
    assert_eq!(
        u16::from_le_bytes(local_transition[28..30].try_into().unwrap()),
        1,
        "only the live local should cross the yield"
    );
    assert_eq!(
        i64::from_le_bytes(local_transition[30..38].try_into().unwrap()),
        42
    );
    write_control_frame(
        captured_worker.stdin.as_mut().unwrap(),
        9,
        &resume_payload(2, local_continuation, &[42]),
    );
    let (local_success_kind, local_success) = read_control_frame(captured_output);
    assert_eq!(local_success_kind, 4);
    assert_eq!(
        u64::from_le_bytes(local_success[..8].try_into().unwrap()),
        2
    );
    assert_eq!(
        i64::from_le_bytes(local_success[16..24].try_into().unwrap()),
        43
    );
    write_control_frame(
        captured_worker.stdin.as_mut().unwrap(),
        3,
        &call_payload(3, yielded_add_twice_export_id, &[41]),
    );
    let (first_repeat_kind, first_repeat_transition) = read_control_frame(captured_output);
    assert_eq!(first_repeat_kind, 8);
    let first_repeat_continuation =
        u64::from_le_bytes(first_repeat_transition[16..24].try_into().unwrap());
    assert_eq!(
        i64::from_le_bytes(first_repeat_transition[30..38].try_into().unwrap()),
        41
    );
    write_control_frame(
        captured_worker.stdin.as_mut().unwrap(),
        9,
        &resume_payload(3, first_repeat_continuation, &[41]),
    );
    let (second_repeat_kind, second_repeat_transition) = read_control_frame(captured_output);
    assert_eq!(second_repeat_kind, 8);
    let second_repeat_continuation =
        u64::from_le_bytes(second_repeat_transition[16..24].try_into().unwrap());
    assert_ne!(second_repeat_continuation, first_repeat_continuation);
    assert_eq!(
        i64::from_le_bytes(second_repeat_transition[30..38].try_into().unwrap()),
        42
    );
    write_control_frame(
        captured_worker.stdin.as_mut().unwrap(),
        9,
        &resume_payload(3, second_repeat_continuation, &[42]),
    );
    let (repeat_success_kind, repeat_success) = read_control_frame(captured_output);
    assert_eq!(repeat_success_kind, 4);
    assert_eq!(
        i64::from_le_bytes(repeat_success[16..24].try_into().unwrap()),
        43
    );
    write_control_frame(
        captured_worker.stdin.as_mut().unwrap(),
        3,
        &call_payload(4, yielded_bool_twice_export_id, &[1]),
    );
    let (_, first_bool_transition) = read_control_frame(captured_output);
    let first_bool_continuation =
        u64::from_le_bytes(first_bool_transition[16..24].try_into().unwrap());
    assert_eq!(
        i64::from_le_bytes(first_bool_transition[30..38].try_into().unwrap()),
        1
    );
    write_control_frame(
        captured_worker.stdin.as_mut().unwrap(),
        9,
        &resume_payload(4, first_bool_continuation, &[1]),
    );
    let (second_bool_kind, second_bool_transition) = read_control_frame(captured_output);
    assert_eq!(second_bool_kind, 8);
    let second_bool_continuation =
        u64::from_le_bytes(second_bool_transition[16..24].try_into().unwrap());
    write_control_frame(
        captured_worker.stdin.as_mut().unwrap(),
        9,
        &resume_payload(4, second_bool_continuation, &[1]),
    );
    let (bool_success_kind, bool_success) = read_control_frame(captured_output);
    assert_eq!(bool_success_kind, 4);
    assert_eq!(
        i64::from_le_bytes(bool_success[16..24].try_into().unwrap()),
        1
    );

    let (pure_branch_kind, pure_branch) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        5,
        branch_yield_export_id,
        &[0],
    );
    assert_eq!(pure_branch_kind, 4);
    assert_eq!(frame_value(&pure_branch), 7);

    let (branch_kind, branch_transition) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        6,
        branch_yield_export_id,
        &[1],
    );
    assert_eq!(branch_kind, 8);
    assert_eq!(transition_value_count(&branch_transition), 0);
    let branch_continuation = transition_continuation(&branch_transition);
    let (branch_success_kind, branch_success) = exchange_worker_resume(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        6,
        branch_continuation,
        &[],
    );
    assert_eq!(branch_success_kind, 4);
    assert_eq!(frame_value(&branch_success), 41);

    let (local_branch_kind, local_branch_transition) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        7,
        branch_yield_local_export_id,
        &[1, 40],
    );
    assert_eq!(local_branch_kind, 8);
    assert_eq!(transition_value_count(&local_branch_transition), 1);
    assert_eq!(transition_value(&local_branch_transition, 0), 41);
    let (local_branch_success_kind, local_branch_success) = exchange_worker_resume(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        7,
        transition_continuation(&local_branch_transition),
        &[41],
    );
    assert_eq!(local_branch_success_kind, 4);
    assert_eq!(frame_value(&local_branch_success), 42);
    let (local_fallback_kind, local_fallback) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        8,
        branch_yield_local_export_id,
        &[0, 40],
    );
    assert_eq!(local_fallback_kind, 4);
    assert_eq!(frame_value(&local_fallback), 40);

    let (first_branch_kind, first_branch_transition) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        9,
        branch_yield_both_export_id,
        &[1],
    );
    assert_eq!(first_branch_kind, 8);
    let first_branch_continuation = transition_continuation(&first_branch_transition);
    let (first_branch_success_kind, first_branch_success) = exchange_worker_resume(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        9,
        first_branch_continuation,
        &[],
    );
    assert_eq!(first_branch_success_kind, 4);
    assert_eq!(frame_value(&first_branch_success), 1);
    let (second_branch_kind, second_branch_transition) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        10,
        branch_yield_both_export_id,
        &[0],
    );
    assert_eq!(second_branch_kind, 8);
    let second_branch_continuation = transition_continuation(&second_branch_transition);
    assert_ne!(second_branch_continuation, first_branch_continuation);
    let (second_branch_success_kind, second_branch_success) = exchange_worker_resume(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        10,
        second_branch_continuation,
        &[],
    );
    assert_eq!(second_branch_success_kind, 4);
    assert_eq!(frame_value(&second_branch_success), 2);

    let (nested_transition_kind, nested_transition) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        11,
        nested_branch_yield_export_id,
        &[1, 1],
    );
    assert_eq!(nested_transition_kind, 8);
    let (nested_success_kind, nested_success) = exchange_worker_resume(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        11,
        transition_continuation(&nested_transition),
        &[],
    );
    assert_eq!(nested_success_kind, 4);
    assert_eq!(frame_value(&nested_success), 11);
    for (request_id, arguments, expected) in [(12, [1, 0], 12), (13, [0, 1], 13)] {
        let (kind, frame) = exchange_worker_call(
            captured_worker.stdin.as_mut().unwrap(),
            captured_output,
            request_id,
            nested_branch_yield_export_id,
            &arguments,
        );
        assert_eq!(kind, 4);
        assert_eq!(frame_value(&frame), expected);
    }

    let (short_false_kind, short_false) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        14,
        short_circuit_yield_export_id,
        &[0, 1],
    );
    assert_eq!(short_false_kind, 4);
    assert_eq!(frame_value(&short_false), 0);
    let (short_true_kind, short_true_transition) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        15,
        short_circuit_yield_export_id,
        &[1, 1],
    );
    assert_eq!(short_true_kind, 8);
    assert_eq!(transition_value(&short_true_transition, 0), 1);
    let (short_success_kind, short_success) = exchange_worker_resume(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        15,
        transition_continuation(&short_true_transition),
        &[1],
    );
    assert_eq!(short_success_kind, 4);
    assert_eq!(frame_value(&short_success), 1);

    let (outer_transition_kind, outer_transition) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        16,
        yield_then_branch_export_id,
        &[1],
    );
    assert_eq!(outer_transition_kind, 8);
    assert_eq!(transition_value(&outer_transition, 0), 1);
    let (inner_transition_kind, inner_transition) = exchange_worker_resume(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        16,
        transition_continuation(&outer_transition),
        &[1],
    );
    assert_eq!(inner_transition_kind, 8);
    assert_ne!(
        transition_continuation(&inner_transition),
        transition_continuation(&outer_transition)
    );
    let (inner_success_kind, inner_success) = exchange_worker_resume(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        16,
        transition_continuation(&inner_transition),
        &[],
    );
    assert_eq!(inner_success_kind, 4);
    assert_eq!(frame_value(&inner_success), 21);
    let (outer_fallback_kind, outer_fallback) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        17,
        yield_then_branch_export_id,
        &[0],
    );
    assert_eq!(outer_fallback_kind, 8);
    let (outer_fallback_success_kind, outer_fallback_success) = exchange_worker_resume(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        17,
        transition_continuation(&outer_fallback),
        &[0],
    );
    assert_eq!(outer_fallback_success_kind, 4);
    assert_eq!(frame_value(&outer_fallback_success), 22);

    let (pair_transition_kind, pair_transition) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        18,
        branch_capture_pair_export_id,
        &[1, 20, 22],
    );
    assert_eq!(pair_transition_kind, 8);
    assert_eq!(transition_value_count(&pair_transition), 2);
    assert_eq!(transition_value(&pair_transition, 0), 20);
    assert_eq!(transition_value(&pair_transition, 1), 22);
    let (pair_success_kind, pair_success) = exchange_worker_resume(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        18,
        transition_continuation(&pair_transition),
        &[20, 22],
    );
    assert_eq!(pair_success_kind, 4);
    assert_eq!(frame_value(&pair_success), 42);

    let (unmatched_branch_kind, unmatched_branch) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        19,
        branch_yield_only_export_id,
        &[0],
    );
    assert_eq!(unmatched_branch_kind, 5);
    assert_eq!(
        i32::from_le_bytes(unmatched_branch[16..20].try_into().unwrap()),
        5
    );
    let (matched_branch_kind, matched_branch) = exchange_worker_call(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        20,
        branch_yield_only_export_id,
        &[1],
    );
    assert_eq!(matched_branch_kind, 8);
    let (matched_branch_success_kind, matched_branch_success) = exchange_worker_resume(
        captured_worker.stdin.as_mut().unwrap(),
        captured_output,
        20,
        transition_continuation(&matched_branch),
        &[],
    );
    assert_eq!(matched_branch_success_kind, 4);
    assert_eq!(frame_value(&matched_branch_success), 1);

    write_control_frame(captured_worker.stdin.as_mut().unwrap(), 6, &[]);
    assert_eq!(read_control_frame(captured_output), (7, Vec::new()));
    drop(captured_worker.stdin.take());
    let captured_result = captured_worker
        .wait_with_output()
        .expect("wait for captured-continuation worker");
    assert!(
        captured_result.status.success(),
        "captured continuation failed: {}",
        String::from_utf8_lossy(&captured_result.stderr)
    );

    let mut wrong_type_worker = Command::new(env!("CARGO_BIN_EXE_terlan-native-worker"))
        .arg(&image_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start wrong-type continuation worker");
    let wrong_type_input = wrong_type_worker.stdin.as_mut().unwrap();
    write_control_frame(wrong_type_input, 1, &descriptor_digest);
    write_control_frame(
        wrong_type_input,
        3,
        &call_payload(1, yielded_bool_export_id, &[1]),
    );
    let wrong_type_output = wrong_type_worker.stdout.as_mut().unwrap();
    assert_eq!(
        read_control_frame(wrong_type_output),
        (2, descriptor_digest.to_vec())
    );
    let (wrong_type_kind, wrong_type_transition) = read_control_frame(wrong_type_output);
    assert_eq!(wrong_type_kind, 8);
    let wrong_type_continuation =
        u64::from_le_bytes(wrong_type_transition[16..24].try_into().unwrap());
    write_control_frame(
        wrong_type_worker.stdin.as_mut().unwrap(),
        9,
        &resume_payload(1, wrong_type_continuation, &[2]),
    );
    drop(wrong_type_worker.stdin.take());
    let wrong_type_result = wrong_type_worker
        .wait_with_output()
        .expect("wait for wrong-type continuation worker");
    assert!(!wrong_type_result.status.success());
    assert!(String::from_utf8_lossy(&wrong_type_result.stderr)
        .contains("error[native_worker.boundary_type]"));

    let mut stale_worker = Command::new(env!("CARGO_BIN_EXE_terlan-native-worker"))
        .arg(&image_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start stale-continuation worker");
    let stale_input = stale_worker
        .stdin
        .as_mut()
        .expect("stale-continuation worker stdin");
    write_control_frame(stale_input, 1, &descriptor_digest);
    write_control_frame(stale_input, 3, &call_payload(1, yielded_export_id, &[]));
    let stale_output = stale_worker
        .stdout
        .as_mut()
        .expect("stale-continuation worker stdout");
    assert_eq!(
        read_control_frame(stale_output),
        (2, descriptor_digest.to_vec())
    );
    let (transition_kind, transition) = read_control_frame(stale_output);
    assert_eq!(transition_kind, 8);
    assert_eq!(u64::from_le_bytes(transition[..8].try_into().unwrap()), 1);
    let continuation_id = u64::from_le_bytes(transition[16..24].try_into().unwrap());
    assert_eq!(
        u16::from_le_bytes(transition[24..26].try_into().unwrap()),
        1
    );
    write_control_frame(
        stale_worker.stdin.as_mut().unwrap(),
        9,
        &resume_payload(1, continuation_id ^ 1, &[]),
    );
    drop(stale_worker.stdin.take());
    let stale_result = stale_worker
        .wait_with_output()
        .expect("wait for stale-continuation worker");
    assert!(!stale_result.status.success());
    assert!(String::from_utf8_lossy(&stale_result.stderr)
        .contains("error[native_worker.continuation_stale]"));

    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        yielded_export_id,
        &[],
        SuspendedAction {
            operation: 9,
            request_id: 2,
            continuation_id_xor: 0,
            values: vec![],
        },
        "error[native_worker.continuation_stale]",
    );
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        yielded_add_export_id,
        &[41],
        SuspendedAction {
            operation: 9,
            request_id: 1,
            continuation_id_xor: 0,
            values: vec![],
        },
        "error[native_worker.continuation_type]",
    );
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        yielded_export_id,
        &[],
        SuspendedAction {
            operation: 3,
            request_id: 0,
            continuation_id_xor: 0,
            values: Vec::new(),
        },
        "error[native_worker.continuation_pending]",
    );
    assert_suspended_worker_rejects(
        &image_path,
        descriptor_digest,
        yielded_export_id,
        &[],
        SuspendedAction {
            operation: 6,
            request_id: 0,
            continuation_id_xor: 0,
            values: Vec::new(),
        },
        "error[native_worker.continuation_pending]",
    );
    assert_duplicate_resume_rejected(&image_path, descriptor_digest, yielded_add_export_id, &[41]);

    let mut repl = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("repl")
        .env("RUSTC", root.join("rustc-must-not-run"))
        .env_remove("TERLAN_NATIVE_WORKER")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start direct-AOT REPL");
    repl.stdin
        .as_mut()
        .expect("REPL stdin")
        .write_all(b"1 + 2.\n:quit\n")
        .expect("write REPL expression");
    drop(repl.stdin.take());
    let repl_output = repl.wait_with_output().expect("wait for REPL");
    assert!(repl_output.status.success());
    assert!(String::from_utf8_lossy(&repl_output.stdout).contains("repl> 3\n"));

    fs::remove_dir_all(&root).expect("remove direct-AOT fixture root");
}

/// Proves memory introspection survives source checking, AOT lowering, and VM execution.
#[test]
fn terlan_memory_intrinsics_execute_in_a_native_image() {
    let root = std::env::temp_dir().join(format!(
        "terlan-direct-aot-memory-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create memory fixture root");
    let source = root.join("memory_aot.terl");
    let output = root.join("build");
    fs::write(&source, include_str!("fixtures/memory_aot.terl")).expect("write memory fixture");

    let build = Command::new(env!("CARGO_BIN_EXE_terlc"))
        .arg("build")
        .arg(&source)
        .arg("--target")
        .arg("terlan-vm")
        .arg("--out-dir")
        .arg(&output)
        .output()
        .expect("start terlc");
    assert!(
        build.status.success(),
        "terlc failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let execution = Command::new(env!("CARGO_BIN_EXE_terlan-vm"))
        .arg("run")
        .arg(output.join("vm/memory_aot.tvm"))
        .arg("--entry")
        .arg("memory_aot.memory_contract")
        .arg("--test-eval")
        .output()
        .expect("start terlan-vm");
    assert!(
        execution.status.success(),
        "memory contract failed:\n{}",
        String::from_utf8_lossy(&execution.stderr)
    );

    fs::remove_dir_all(&root).expect("remove direct-AOT memory fixture root");
}
