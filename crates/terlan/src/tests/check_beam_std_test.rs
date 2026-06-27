use super::*;

/// Verifies executable Task operations fail in target-profile validation before backend emission.
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
///   unsupported Task operations is rejected until backend support exists.
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

/// Verifies BEAM Agent paired state/value operations pass target-profile validation.
///
/// Inputs:
/// - A temporary Terlan module importing `std.beam.Agent` and calling
///   `Agent.get_and_update(...)` in a function body.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must pass with
///   parse, resolve, typecheck, and CoreIR phases complete.
///
/// Transformation:
/// - Runs the public command path and confirms `get_and_update` is part of the
///   admitted Agent runtime surface instead of being rejected as a deferred
///   process-backed operation.
#[test]
fn run_check_single_file_accepts_beam_agent_get_and_update_before_backend_emission() {
    let dir = make_temp_dir("check_single_file_beam_agent_get_and_update_accepted");
    let source = dir.join("beam_agent_operation.terl");
    fs::write(
        &source,
        "\
module beam_agent_operation.\n\
\n\
import std.beam.Agent.\n\
\n\
pub queue_update(agent: Agent[Int]): Int ->\n\
    Agent.get_and_update(agent, (value: Int) -> {value, value}).\n",
    )
    .expect("write BEAM Agent operation source");
    let manifest = dir.join("beam_agent_operation.phase-manifest.json");

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
    assert!(!manifest_text.contains("BEAM Agent operation std.beam.Agent.get_and_update"));
}

/// Verifies BEAM GenServer implementations can rely on default callbacks.
///
/// Inputs:
/// - A temporary Terlan module importing `std.beam.GenServer` and implementing
///   `GenServer[...]` without a `terminate` method.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must pass with
///   parse, resolve, typecheck, and CoreIR phases complete.
///
/// Transformation:
/// - Runs the public command path and confirms generated `.typi` default method
///   markers make optional BEAM callbacks usable outside the typechecker
///   unit-test path.
#[test]
fn run_check_single_file_accepts_beam_gen_server_default_terminate_before_backend_emission() {
    let dir = make_temp_dir("check_single_file_beam_gen_server_default_terminate_accepted");
    let source = dir.join("beam_gen_server_default_terminate.terl");
    fs::write(
        &source,
        "\
module beam_gen_server_default_terminate.\n\
\n\
import std.beam.GenServer.{GenServer, CallReply}.\n\
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
    Ok({state, request}).\n\
\n\
pub (server: CounterServer) handle_cast(state: Int, event: Int): Result[Int, Error] ->\n\
    Ok(state + event).\n",
    )
    .expect("write BEAM GenServer source");
    let manifest = dir.join("beam_gen_server_default_terminate.phase-manifest.json");

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

/// Verifies executable GenServer operations pass after runtime lowering.
///
/// Inputs:
/// - A temporary Terlan module importing `std.beam.GenServer` and calling
///   `GenServer.start(server)`.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must pass with
///   parse, resolve, typecheck, and CoreIR phases complete.
///
/// Transformation:
/// - Runs the public command path and confirms GenServer process startup is
///   admitted after callback dispatch lowering is implemented by the BEAM
///   backend.
#[test]
fn run_check_single_file_accepts_beam_gen_server_operation_before_backend_emission() {
    let dir = make_temp_dir("check_single_file_beam_gen_server_operation_accepted");
    let source = dir.join("beam_gen_server_operation.terl");
    fs::write(
        &source,
        "\
module beam_gen_server_operation.\n\
\n\
import std.beam.GenServer.\n\
import type std.beam.GenServer.{CallReply, GenServer, ServerRef}.\n\
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
    Ok({state, request}).\n\
\n\
pub (server: CounterServer) handle_cast(state: Int, event: Int): Result[Int, Error] ->\n\
    Ok(state + event).\n\
\n\
pub start_server(server: CounterServer): Dynamic ->\n\
    GenServer.start(server).\n",
    )
    .expect("write BEAM GenServer operation source");
    let manifest = dir.join("beam_gen_server_operation.phase-manifest.json");

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
    assert!(!manifest_text.contains("BEAM GenServer operation std.beam.GenServer.start"));
}

/// Verifies executable BEAM Task operations pass target-profile validation.
///
/// Inputs:
/// - A temporary Terlan module importing `std.beam.Task` and calling
///   `Task.start(() -> 1)` in a function body.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must pass with
///   parse, resolve, typecheck, and CoreIR phases complete.
///
/// Transformation:
/// - Runs the public command path and confirms BEAM Task process-backed
///   execution is admitted after the shared BEAM process intrinsic layer owns
///   lowering.
#[test]
fn run_check_single_file_accepts_beam_task_operation_before_backend_emission() {
    let dir = make_temp_dir("check_single_file_beam_task_operation_accepted");
    let source = dir.join("beam_task_operation.terl");
    fs::write(
        &source,
        "\
module beam_task_operation.\n\
\n\
import std.beam.Task.\n\
\n\
pub start_work(): Dynamic ->\n\
    Task.start(() -> 1).\n",
    )
    .expect("write BEAM Task operation source");
    let manifest = dir.join("beam_task_operation.phase-manifest.json");

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
    assert!(!manifest_text.contains("BEAM Task operation std.beam.Task.start"));
}

/// Verifies NativeBridge runtime operations pass public check after local lowering exists.
///
/// Inputs:
/// - A temporary Terlan module importing `std.beam.NativeBridge` and calling
///   `NativeBridge.start(resource)`.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must pass under
///   the Erlang profile with parse, resolve, typecheck, and CoreIR phases all
///   marked ok.
///
/// Transformation:
/// - Runs the public command path and confirms the callable NativeBridge
///   contract is visible to source and admitted by the Erlang target profile
///   before backend emission.
#[test]
fn run_check_single_file_accepts_beam_native_bridge_operation_before_backend_emission() {
    let dir = make_temp_dir("check_single_file_beam_native_bridge_operation_accepted");
    let source = dir.join("beam_native_bridge_operation.terl");
    fs::write(
        &source,
        "\
module beam_native_bridge_operation.\n\
\n\
import std.beam.NativeBridge.\n\
import type std.beam.NativeBridge.NativeBridge.\n\
\n\
pub start_bridge(resource: String): Dynamic ->\n\
    NativeBridge.start(resource).\n",
    )
    .expect("write BEAM NativeBridge operation source");
    let manifest = dir.join("beam_native_bridge_operation.phase-manifest.json");

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
    assert!(!manifest_text.contains("BEAM NativeBridge operation std.beam.NativeBridge.start"));
}

/// Verifies Supervisor runtime operations pass public check after local lowering exists.
///
/// Inputs:
/// - A temporary Terlan module importing `std.beam.Supervisor` and calling
///   `Supervisor.child_spec(value)`.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must pass under
///   the Erlang profile with parse, resolve, typecheck, and CoreIR phases all
///   marked ok.
///
/// Transformation:
/// - Runs the public command path and confirms the callable Supervisor contract
///   is visible to source and admitted by the Erlang target profile before
///   backend emission.
#[test]
fn run_check_single_file_accepts_beam_supervisor_operation_before_backend_emission() {
    let dir = make_temp_dir("check_single_file_beam_supervisor_operation_accepted");
    let source = dir.join("beam_supervisor_operation.terl");
    fs::write(
        &source,
        "\
module beam_supervisor_operation.\n\
\n\
import std.beam.Supervisor.\n\
\n\
pub make_spec(value: Int): Dynamic ->\n\
    Supervisor.child_spec(value).\n",
    )
    .expect("write BEAM Supervisor operation source");
    let manifest = dir.join("beam_supervisor_operation.phase-manifest.json");

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
    assert!(!manifest_text.contains("BEAM Supervisor operation std.beam.Supervisor.child_spec"));
}
