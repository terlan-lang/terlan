
/// Verifies the `check` command rejects remote calls for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed remote-call
///   CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required remote-call CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_remote_call_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_remote_call");
    let path = fixture(
        &dir,
        "\
module core_v0_rejects_remote_call.\n\npub value(): Int ->\n    erlang.abs(1).\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects remote function references for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed remote
///   function reference CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required remote function reference CoreIR is
///   rejected before a successful result is returned.
#[test]
fn run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_remote_fun_ref");
    let path = fixture(
        &dir,
        "\
module core_v0_rejects_remote_fun_ref.\n\npub value(): Dynamic ->\n    fun erlang:abs/1.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects record construction for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed record
///   construction CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required record construction CoreIR is rejected
///   before a successful result is returned.
#[test]
fn run_check_single_file_rejects_record_construct_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_record_construct");
    let path = fixture(
        &dir,
        "\
module core_v0_rejects_record_construct.\n\npub value(): Dynamic ->\n    Point { x: 1 }.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects record access for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed record
///   access CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required record access CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_record_access_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_record_access");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_record_access.\n\npub struct Point {\n    x: Int\n}.\n\npub value(point: Point): Dynamic ->\n    point#Point.x.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects record updates for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed record
///   update CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required record update CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_record_update_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_record_update");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_record_update.\n\npub struct Point {\n    x: Int\n}.\n\npub value(point: Point): Dynamic ->\n    point#Point { x: 1 }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects template instantiation for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed
///   template-instantiation CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required template-instantiation CoreIR is rejected
///   before a successful result is returned.
#[test]
fn run_check_single_file_rejects_template_instantiate_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_template_instantiate");
    let template_dir = dir.join("templates");
    fs::create_dir_all(&template_dir).expect("create template dir");
    fs::write(template_dir.join("user_card.terl.html"), "<p>{name}</p>")
        .expect("write template file");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_template_instantiate.\n\ntemplate UserCard from \"./templates/user_card.terl.html\" {\n    name: Text\n}.\n\npub value(): Dynamic ->\n    UserCard(name = \"Ada\").\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies VM-only std contracts are rejected by non-VM target
/// profiles before backend emission.
///
/// Inputs:
/// - A temporary Terlan module importing `std.vm.Process.Process` as a
///   type-only contract and using it in a function signature.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must fail
///   under `core-v0` during target inference, before the formal phase
///   pipeline runs.
///
/// Transformation:
/// - Runs the public command path with `TargetProfile::CoreV0` and confirms
///   VM package contracts provide typed target evidence that cannot be hidden
///   by an explicit non-VM target override.
#[test]
fn run_check_single_file_rejects_beam_process_contract_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_beam_process_contract_rejected");
    let source = dir.join("beam_process_contract.terl");
    fs::write(
        &source,
        "\
module beam_process_contract.\n\
\n\
import type std.vm.Process.Process.\n\
import std.core.Unit.{Unit}.\n\
\n\
pub observe(process: Process[String]): Unit ->\n\
    Unit.\n",
    )
    .expect("write VM Process contract source");
    let manifest = dir.join("beam_process_contract.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..CliState::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"target_inference","status":"error""#));
    assert!(manifest_text
        .contains("explicit target `core-v0` conflicts with typed target evidence for `vm`"));
}

/// Verifies VM NativeBridge contracts are rejected by non-VM target
/// profiles before backend emission.
///
/// Inputs:
/// - A temporary Terlan module importing `std.vm.NativeBridge.NativeBridge`
///   as a type-only contract and using it in a function signature.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must fail
///   under `core-v0` during target inference, before the formal phase
///   pipeline runs.
///
/// Transformation:
/// - Runs the public command path with `TargetProfile::CoreV0` and confirms
///   the VM/NativeBoundary bridge contract provides typed target evidence that
///   cannot be hidden by an explicit non-VM target override.
#[test]
fn run_check_single_file_rejects_vm_native_bridge_contract_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_vm_native_bridge_contract_rejected");
    let source = dir.join("vm_native_bridge_contract.terl");
    fs::write(
        &source,
        "\
module vm_native_bridge_contract.\n\
\n\
import type std.vm.NativeBridge.NativeBridge.\n\
import std.core.Unit.{Unit}.\n\
\n\
pub observe(bridge: NativeBridge[String]): Unit ->\n\
    Unit.\n",
    )
    .expect("write VM NativeBridge contract source");
    let manifest = dir.join("vm_native_bridge_contract.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..CliState::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"target_inference","status":"error""#));
    assert!(manifest_text
        .contains("explicit target `core-v0` conflicts with typed target evidence for `vm`"));
}
