use super::*;

/// Verifies `String(value)` is accepted as a VM-safe conversion constructor.
///
/// Inputs:
/// - A temporary source file calling `String(...)` on integer, boolean, and
///   string values.
///
/// Output:
/// - Test assertion only; `terlc check` must succeed instead of reporting
///   `unknown constructor String / 1`.
///
/// Transformation:
/// - Runs the public command path so the typecheck and target-profile phases
///   both admit the compiler-owned `core.value.to_string` intrinsic.
#[test]
fn run_check_single_file_accepts_string_conversion_constructor() {
    let dir = make_temp_dir("check_single_file_string_conversion_constructor");
    let source = dir.join("string_conversion_constructor.terl");
    fs::write(
        &source,
        "\
module string_conversion_constructor.\n\
\n\
pub run(): Bool ->\n\
    let parsed_int = std.core.Option.with_default(Int(\"42\"), 0);\n\
    let parsed_float = std.core.Option.with_default(Float(\"1.5\"), 0.0);\n\
    let parsed_bool = std.core.Option.with_default(Bool(\"true\"), false);\n\
    String(1) == \"1\" and String(true) == \"true\" and String(\"ok\") == \"ok\" and parsed_int == 42 and parsed_float == 1.5 and parsed_bool and std.core.Option.is_none(Int(\"nope\")) and std.core.Option.is_none(Float(\"nope\")) and std.core.Option.is_none(Bool(\"yes\")).\n",
    )
    .expect("write string conversion source");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![source.to_string_lossy().into()],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies executable Task operations fail in target-profile validation before runtime execution.
///
/// Inputs:
/// - A temporary Terlan module importing `std.core.Task` and calling
///   `Task.spawn(() -> 1)` in a function body.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must fail with
///   parse, resolve, and typecheck phases complete and the CoreIR target-profile
///   phase marked as an error.
///
/// Transformation:
/// - Runs the public command path and confirms the formal std Task contract
///   remains importable/typecheckable while runtime Task execution for
///   unsupported Task operations is rejected until runtime support exists.
#[test]
fn run_check_single_file_rejects_task_operation_before_backend_emission() {
    let dir = make_temp_dir("check_single_file_task_operation_rejected");
    let source = dir.join("task_operation.terl");
    fs::write(
        &source,
        "\
module task_operation.\n\
\n\
import std.core.Task.\n\
\n\
pub complete(): Task[Int] ->\n\
    Task.spawn(() -> 1).\n",
    )
    .expect("write task operation source");
    let manifest = dir.join("task_operation.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState::default(),
    );

    assert_ne!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"error""#));
    assert!(manifest_text.contains("task operation std.core.Task.spawn"));
}

/// Verifies VM Agent paired state/value operations pass the check pipeline.
///
/// Inputs:
/// - A temporary Terlan module importing `std.vm.Agent` and calling
///   `Agent.get_and_update(...)` in a function body.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must succeed
///   with parse, resolve, typecheck, and CoreIR phases complete.
///
/// Transformation:
/// - Runs the public command path and confirms `get_and_update` remains a
///   valid source-level VM operation through the check pipeline.
#[test]
fn run_check_single_file_accepts_vm_agent_get_and_update_before_runtime_execution() {
    let dir = make_temp_dir("check_single_file_vm_agent_get_and_update_rejected");
    let source = dir.join("vm_agent_operation.terl");
    fs::write(
        &source,
        "\
module vm_agent_operation.\n\
\n\
import std.vm.Agent.\n\
\n\
pub queue_update(agent: Agent[Int]): Int ->\n\
    Agent.get_and_update(agent, (value: Int) -> {state: value, value: value}).\n",
    )
    .expect("write VM Agent operation source");
    let manifest = dir.join("vm_agent_operation.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
}

/// Verifies VM GenServer contracts pass the check pipeline.
///
/// Inputs:
/// - A temporary Terlan module importing `std.vm.GenServer` and implementing
///   `GenServer[...]` without a `terminate` method.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must succeed
///   with parse, resolve, typecheck, and CoreIR phases complete.
///
/// Transformation:
/// - Runs the public command path and confirms generated `.typi` default
///   method markers remain visible to typechecking under the VM profile.
#[test]
fn run_check_single_file_accepts_vm_gen_server_default_terminate_before_runtime_execution() {
    let dir = make_temp_dir("check_single_file_vm_gen_server_default_terminate_rejected");
    let source = dir.join("vm_gen_server_default_terminate.terl");
    fs::write(
        &source,
        "\
module vm_gen_server_default_terminate.\n\
\n\
import std.vm.GenServer.{GenServer, CallReply}.\n\
import std.core.Result.{Result, Ok}.\n\
import std.core.Error.{Error}.\n\
\n\
pub struct CounterServer implements GenServer[CounterServer, Int, Int, Int, Int] {\n\
    seed: Int\n\
}.\n\
\n\
pub (server: CounterServer) init(): Result[Int, Error] ->\n\
    Ok(server.seed).\n\
\n\
pub (server: CounterServer) handle_call(state: Int, request: Int): Result[CallReply[Int, Int], Error] ->\n\
    Ok({state: state, reply: request}).\n\
\n\
pub (server: CounterServer) handle_cast(state: Int, event: Int): Result[Int, Error] ->\n\
    Ok(state + event).\n",
    )
    .expect("write VM GenServer source");
    let manifest = dir.join("vm_gen_server_default_terminate.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
}

/// Verifies executable GenServer operations pass the check pipeline.
///
/// Inputs:
/// - A temporary Terlan module importing `std.vm.GenServer` and calling
///   `GenServer.start(server)`.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must succeed
///   with parse, resolve, typecheck, and CoreIR phases complete.
///
/// Transformation:
/// - Runs the public command path and confirms GenServer process startup is a
///   valid source-level VM operation before runtime execution.
#[test]
fn run_check_single_file_accepts_vm_gen_server_operation_before_runtime_execution() {
    let dir = make_temp_dir("check_single_file_vm_gen_server_operation_rejected");
    let source = dir.join("vm_gen_server_operation.terl");
    fs::write(
        &source,
        "\
module vm_gen_server_operation.\n\
\n\
import std.vm.GenServer.{CallReply, GenServer, ServerRef, start}.\n\
import std.core.Result.{Ok}.\n\
import type std.core.Result.Result.\n\
import type std.core.Error.Error.\n\
\n\
pub struct CounterServer implements GenServer[CounterServer, Int, Int, Int, Int] {\n\
    seed: Int\n\
}.\n\
\n\
pub (server: CounterServer) init(): Result[Int, Error] ->\n\
    Ok(server.seed).\n\
\n\
pub (server: CounterServer) handle_call(state: Int, request: Int): Result[CallReply[Int, Int], Error] ->\n\
    Ok({state: state, reply: request}).\n\
\n\
pub (server: CounterServer) handle_cast(state: Int, event: Int): Result[Int, Error] ->\n\
    Ok(state + event).\n\
\n\
pub start_server(server: CounterServer): Dynamic ->\n\
    start(server).\n",
    )
    .expect("write VM GenServer operation source");
    let manifest = dir.join("vm_gen_server_operation.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
}

/// Verifies executable VM Task operations pass the check pipeline.
///
/// Inputs:
/// - A temporary Terlan module importing `std.vm.Task` and calling
///   `Task.start(() -> 1)` in a function body.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must succeed
///   with parse, resolve, typecheck, and CoreIR phases complete.
///
/// Transformation:
/// - Runs the public command path and confirms VM Task process-backed
///   execution is admitted through check before runtime execution.
#[test]
fn run_check_single_file_accepts_vm_task_operation_before_runtime_execution() {
    let dir = make_temp_dir("check_single_file_vm_task_operation_rejected");
    let source = dir.join("vm_task_operation.terl");
    fs::write(
        &source,
        "\
module vm_task_operation.\n\
\n\
import std.vm.Task.\n\
\n\
pub start_work(): Dynamic ->\n\
    Task.start(() -> 1).\n",
    )
    .expect("write VM Task operation source");
    let manifest = dir.join("vm_task_operation.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
}

/// Verifies NativeBridge runtime operations pass the check pipeline.
///
/// Inputs:
/// - A temporary Terlan module importing `std.vm.NativeBridge` and calling
///   `NativeBridge.start(resource)`.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must succeed
///   under the VM-default profile after parse, resolve, typecheck, and CoreIR
///   complete.
///
/// Transformation:
/// - Runs the public command path and confirms the callable NativeBridge
///   contract is visible to source before runtime execution.
#[test]
fn run_check_single_file_accepts_vm_native_bridge_operation_before_runtime_execution() {
    let dir = make_temp_dir("check_single_file_vm_native_bridge_operation_rejected");
    let source = dir.join("vm_native_bridge_operation.terl");
    fs::write(
        &source,
        "\
module vm_native_bridge_operation.\n\
\n\
import std.vm.NativeBridge.\n\
import std.vm.NativeBridge.{NativeTransfer}.\n\
import type std.vm.NativeBridge.NativeBridge.\n\
\n\
pub start_bridge(resource: String): Dynamic ->\n\
    NativeBridge.start(resource).\n",
    )
    .expect("write VM NativeBridge operation source");
    let manifest = dir.join("vm_native_bridge_operation.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
}

/// Verifies Supervisor runtime operations pass the check pipeline.
///
/// Inputs:
/// - A temporary Terlan module importing `std.vm.Supervisor` and calling
///   `Supervisor.child_spec(value)`.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must succeed
///   under the VM-default profile after parse, resolve, typecheck, and CoreIR
///   complete.
///
/// Transformation:
/// - Runs the public command path and confirms the callable Supervisor
///   contract is visible to source before runtime execution.
#[test]
fn run_check_single_file_accepts_vm_supervisor_operation_before_runtime_execution() {
    let dir = make_temp_dir("check_single_file_vm_supervisor_operation_rejected");
    let source = dir.join("vm_supervisor_operation.terl");
    fs::write(
        &source,
        "\
module vm_supervisor_operation.\n\
\n\
import std.vm.Supervisor.\n\
\n\
pub make_spec(value: Int): Dynamic ->\n\
    Supervisor.child_spec(value).\n",
    )
    .expect("write VM Supervisor operation source");
    let manifest = dir.join("vm_supervisor_operation.phase-manifest.json");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
}
