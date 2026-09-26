use super::*;

/// Compiles one generated C adapter and exercises its bounded public protocol.
fn compile_and_exercise_generated_c_adapter(out_dir: &Path, target_dir: &Path) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest = out_dir.join("native/rust/Cargo.toml");
    let test_output = std::process::Command::new(&cargo)
        .args(["test", "--manifest-path"])
        .arg(&manifest)
        .args(["--offline", "--quiet"])
        .env("CARGO_TARGET_DIR", target_dir)
        .output()
        .expect("run generated C ABI tests");
    assert!(
        test_output.status.success(),
        "generated C ABI tests failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&test_output.stdout),
        String::from_utf8_lossy(&test_output.stderr)
    );

    let build_output = std::process::Command::new(cargo)
        .args(["build", "--manifest-path"])
        .arg(&manifest)
        .args(["--offline", "--quiet", "--bin", "native-boundary-helper"])
        .env("CARGO_TARGET_DIR", target_dir)
        .output()
        .expect("build generated helper");
    assert!(
        build_output.status.success(),
        "generated C ABI helper failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );

    let helper = target_dir.join("debug/native-boundary-helper");
    assert!(
        helper.is_file(),
        "missing generated helper {}",
        helper.display()
    );
    let operation = |value: &str| STANDARD.encode(value);
    let handle_type = STANDARD.encode("c_abi_fixture.NativeBoundary.NativeBoundary");
    let wrong_handle_type = STANDARD.encode("other.Type");
    let mut helper_process = std::process::Command::new(&helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn generated helper for stale secondary handle");
    let mut input = helper_process.stdin.take().expect("helper stdin");
    let mut output = BufReader::new(helper_process.stdout.take().expect("helper stdout"));
    let mut replies = Vec::new();
    let mut call = |request_id: u64, operation_name: &str, args: &str| {
        writeln!(
            input,
            "call {request_id} {}{args}",
            operation(operation_name)
        )
        .expect("write helper request");
        input.flush().expect("flush helper request");
        let mut reply = String::new();
        output.read_line(&mut reply).expect("read helper reply");
        let reply = reply.trim_end().to_string();
        replies.push(reply.clone());
        reply
    };
    let first = call(1, "c_abi_fixture.native_boundary.new", " i:40");
    let first_fields = first.split_whitespace().collect::<Vec<_>>();
    let ["reply", "1", "1", "ok_handle", owner, "1", "1", returned_type] = first_fields.as_slice()
    else {
        panic!("unexpected owner-bound handle reply: {first}");
    };
    assert_eq!(*returned_type, handle_type);
    let owner = (*owner).to_string();
    assert_eq!(
        call(2, "c_abi_fixture.native_boundary.new", " i:5"),
        format!("reply 2 1 ok_handle {owner} 2 1 {handle_type}")
    );
    call(3, "c_abi_fixture.native_boundary.dispose", " r:2");
    call(4, "c_abi_fixture.native_boundary.matmul", " r:1 r:2");
    call(
        5,
        "c_abi_fixture.native_boundary.matmul",
        &format!(" r:1 r:1:{wrong_handle_type}"),
    );
    assert_eq!(
        call(6, "c_abi_fixture.native_boundary.new", " i:9"),
        format!("reply 6 1 ok_handle {owner} 2 2 {handle_type}")
    );
    call(7, "c_abi_fixture.native_boundary.dispose", " r:2");
    call(8, "c_abi_fixture.native_boundary.dispose", " r:6");
    call(9, "c_abi_fixture.native_boundary.dispose", " r:6");
    call(9, "c_abi_fixture.native_boundary.dispose", " r:6");
    call(
        10,
        "c_abi_fixture.native_boundary.dispose",
        &format!(" h:{}:1:1:{handle_type}", STANDARD.encode("foreign-worker")),
    );
    drop(call);
    writeln!(
        input,
        "{}",
        "x".repeat(
            crate::runtime::native_boundary::adapter_abi::PUBLIC_ADAPTER_MAX_FRAME_BYTES + 1
        )
    )
    .expect("write oversized helper frame");
    input.flush().expect("flush oversized helper frame");
    let mut oversized = String::new();
    output
        .read_line(&mut oversized)
        .expect("read oversized helper reply");
    replies.push(oversized.trim_end().to_string());
    drop(input);
    assert!(helper_process.wait().expect("wait for helper").success());
    let helper_stdout = replies.join("\n");
    let stale_code = STANDARD.encode("stale_handle");
    assert!(helper_stdout.lines().any(|line| line.contains(&stale_code)));
    let type_code = STANDARD.encode("handle_type_mismatch");
    assert!(helper_stdout.lines().any(|line| line.contains(&type_code)));
    assert!(helper_stdout
        .lines()
        .any(|line| { line == format!("reply 6 1 ok_handle {owner} 2 2 {handle_type}") }));
    assert!(helper_stdout
        .lines()
        .any(|line| line.starts_with("reply 7 1 ") && line.contains(&stale_code)));
    assert!(helper_stdout
        .lines()
        .any(|line| line.starts_with("reply 9 1 ") && line.contains(&stale_code)));
    for code in ["request_not_monotonic", "frame_too_large"] {
        let encoded = STANDARD.encode(code);
        assert!(helper_stdout.lines().any(|line| line.contains(&encoded)));
    }
    let owner_code = STANDARD.encode("cross_owner_handle");
    assert!(helper_stdout
        .lines()
        .any(|line| line.starts_with("reply 10 1 ") && line.contains(&owner_code)));
    helper
}

#[test]
fn generated_c_adapter_compiles_and_enforces_public_protocol() {
    let _guard = crate::commands::bind::native_helper_env_lock()
        .lock()
        .expect("native helper env lock");
    let out_dir = temp_dir("adapter_protocol");
    let target_dir = temp_dir("adapter_protocol_target");
    generate_c_abi_bindings(&fixture_manifest(), &out_dir).expect("generate C ABI package");
    compile_and_exercise_generated_c_adapter(&out_dir, &target_dir);
    fs::remove_dir_all(out_dir).expect("remove generated outputs");
    fs::remove_dir_all(target_dir).expect("remove generated target");
}

#[test]
fn generated_c_ffi_compiles_links_owns_and_executes_from_terlan() {
    let _guard = crate::commands::bind::native_helper_env_lock()
        .lock()
        .expect("native helper env lock");
    let out_dir = temp_dir("end_to_end");
    let target_dir = temp_dir("end_to_end_target");
    generate_c_abi_bindings(&fixture_manifest(), &out_dir).expect("generate C ABI package");
    let helper = compile_and_exercise_generated_c_adapter(&out_dir, &target_dir);

    let helper_env = "TERLAN_C_ABI_FIXTURE_NATIVE_BOUNDARY_HELPER_PATH";
    let previous_helper = std::env::var_os(helper_env);
    std::env::set_var(helper_env, &helper);
    let exit_code = crate::commands::test::run(
        crate::CliCommand {
            verb: Some("test".to_string()),
            args: vec![out_dir.join("tests").to_string_lossy().into_owned()],
        },
        crate::CliState::default(),
    );
    if let Some(previous_helper) = previous_helper {
        std::env::set_var(helper_env, previous_helper);
    } else {
        std::env::remove_var(helper_env);
    }
    assert_eq!(exit_code, ExitCode::SUCCESS);
    fs::remove_dir_all(out_dir).expect("remove generated outputs");
    fs::remove_dir_all(target_dir).expect("remove generated target");
}
