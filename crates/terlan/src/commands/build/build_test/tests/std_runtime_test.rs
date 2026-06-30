use super::*;

/// Verifies the first admitted `std.core.Task` operations lower to backend
/// intrinsics.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.core.Task`, `std.core.Result`, and
///   primitive console/int helpers.
/// - A completed task created with `Task.done(7)` and observed with the
///   receiver method `task.result()`.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints `7`.
///
/// Transformation:
/// - Resolves the formal Task contract through embedded std summaries,
///   lowers the completed-task surface through compiler-owned Task
///   intrinsics, converts the result shape through ordinary Result
///   pattern matching, and proves no fake backend `std_core_task` module
///   call is emitted.
#[test]
fn build_command_compiles_imported_task_done_result_call() {
    let dir = make_temp_dir("directory_project_imported_task_done_result_call");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.core.Task.\n\
import std.core.Result.{Err, Ok}.\n\
import type std.core.Result.Result.\n\
import type std.core.Error.Error.\n\
\n\
pub value(task: Task[Int]): Result[Int, Error] ->\n\
task.result().\n\
\n\
pub unwrap(result: Result[Int, Error]): Int ->\n\
case result {\n\
    Ok(x) ->\n\
        x;\n\
\n\
    Err(_reason) ->\n\
        0\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(unwrap(value(Task.done(7))))).\n",
    )
    .expect("failed to write imported task fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("{'task_done', 7}"),
        "Task.done should lower to the completed task backing shape: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_core_task:done") && !erl_source.contains("std_core_task:result"),
        "Task intrinsics must not lower to a backend std module call: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "7\n");
}

/// Verifies the first admitted `std.beam.Agent` operations lower to BEAM runtime code.
///
/// Inputs:
/// - A source file importing `std.beam.Agent`, `std.core.Result`, and
///   primitive console/int helpers.
/// - An Agent started with `Agent.start(1)`, updated through mutable
///   receiver methods, read through `agent.get()`, and transitioned through
///   `agent.get_and_update(...)`.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints `8`.
///
/// Transformation:
/// - Resolves the target-gated Agent contract through embedded std
///   summaries, lowers the admitted Agent surface through compiler-owned
///   BEAM intrinsics, and proves Terlan source does not need BEAM message
///   operators to own a simple process-backed state value.
#[test]
fn build_command_compiles_imported_beam_agent_start_get_update_cast_call() {
    let dir = make_temp_dir("directory_project_imported_beam_agent_start_get_update_cast_call");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.beam.Agent.\n\
import type std.beam.Agent.Agent.\n\
import std.core.Result.{Err, Ok}.\n\
import type std.core.Result.Result.\n\
import type std.core.Error.Error.\n\
\n\
pub close(agent: Agent[Int]): Unit ->\n\
agent.stop().\n\
\n\
pub read_updated(agent: Agent[Int]): Int ->\n\
agent.cast((value: Int) -> value + 1);\n\
agent.update((value: Int) -> value + 1);\n\
agent.get_and_update((value: Int) -> {value, value}).\n\
\n\
pub run(result: Result[Agent[Int], Error]): Int ->\n\
case result {\n\
    Ok(agent) ->\n\
        read_updated(agent);\n\
\n\
    Err(_reason) ->\n\
        0\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(run(Agent.start(1)))).\n",
    )
    .expect("failed to write imported Agent fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("spawn(fun() -> Loop(1) end)"),
        "Agent.start should lower to a BEAM process loop: {}",
        erl_source
    );
    assert!(
        erl_source.contains("{get_and_update, Writer, From, Ref}"),
        "Agent.get_and_update should lower to the shared BEAM process loop: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_beam_agent:start")
            && !erl_source.contains("std_beam_agent:get")
            && !erl_source.contains("std_beam_agent:get_and_update")
            && !erl_source.contains("std_beam_agent:update")
            && !erl_source.contains("std_beam_agent:cast"),
        "Agent intrinsics must not lower to a backend std module call: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated Agent project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "3\n");
}

/// Verifies the first admitted `std.beam.Task` operations lower to BEAM
/// runtime code.
///
/// Inputs:
/// - A source file importing `std.beam.Task`, `std.core.Result`, and
///   primitive console/int helpers.
/// - A BEAM task started with `Task.start(() -> 9)`, observed through
///   `Task.result(task)`, and a separate `Task.cancel(task)` helper to force
///   cancellation lowering into the emitted module.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints `9`.
///
/// Transformation:
/// - Resolves the target-gated BEAM Task contract through embedded std
///   summaries, lowers the admitted Task surface through compiler-owned
///   BEAM intrinsics, and proves Terlan source can start and observe a
///   process-backed task without BEAM message syntax.
#[test]
fn build_command_compiles_imported_beam_task_start_result_cancel_call() {
    let dir = make_temp_dir("directory_project_imported_beam_task_start_result_cancel_call");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.beam.Task.\n\
import type std.beam.Task.Task.\n\
import std.core.Result.{Err, Ok}.\n\
import type std.core.Result.Result.\n\
import type std.core.Error.Error.\n\
\n\
pub stop(task: Task[Int]): Unit ->\n\
task.cancel().\n\
\n\
pub value(task: Task[Int]): Result[Int, Error] ->\n\
task.result().\n\
\n\
pub unwrap(result: Result[Int, Error]): Int ->\n\
case result {\n\
    Ok(x) ->\n\
        x;\n\
\n\
    Err(_reason) ->\n\
        0\n\
}.\n\
\n\
pub run(result: Result[Task[Int], Error]): Int ->\n\
case result {\n\
    Ok(task) ->\n\
        unwrap(value(task));\n\
\n\
    Err(_reason) ->\n\
        0\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(run(Task.start(() -> 9)))).\n",
    )
    .expect("failed to write imported BEAM Task fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("{result, From, Ref}"),
        "Task.result should lower to the shared BEAM process loop: {}",
        erl_source
    );
    assert!(
        erl_source.contains("cancel ->"),
        "Task.cancel should lower to the shared BEAM process loop: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_beam_task:start")
            && !erl_source.contains("std_beam_task:result")
            && !erl_source.contains("std_beam_task:cancel"),
        "BEAM Task intrinsics must not lower to a backend std module call: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated BEAM Task project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "9\n");
}

/// Verifies the first admitted `std.beam.GenServer` operations lower to
/// executable BEAM runtime code.
///
/// Inputs:
/// - A source file importing `std.beam.GenServer`, `std.core.Result`, and
///   primitive console/int helpers.
/// - A server implementation value started with `GenServer.start(...)`,
///   updated through `server_ref.cast(...)`, observed through
///   `server_ref.call(...)`, and stopped through `server_ref.stop()`.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints `8`.
///
/// Transformation:
/// - Resolves the target-gated GenServer contract through embedded std
///   summaries, lowers the admitted GenServer surface through compiler-owned
///   BEAM intrinsics, and proves callback receiver methods can back a
///   process-owned state loop without BEAM message syntax in source.
#[test]
fn build_command_compiles_imported_beam_gen_server_start_call_cast_stop() {
    let dir = make_temp_dir("directory_project_imported_beam_gen_server_start_call_cast_stop");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.beam.GenServer.\n\
import type std.beam.GenServer.{CallReply, GenServer, ServerRef}.\n\
import std.core.Result.{Err, Ok}.\n\
import type std.core.Result.Result.\n\
import type std.core.Error.Error.\n\
import std.core.Unit.{Unit}.\n\
\n\
pub struct CounterServer implements GenServer[CounterServer, Int, Int, Int, Int] {\n\
seed: Int\n\
}.\n\
\n\
pub constructor CounterServer {\n\
(seed: Int): CounterServer -> CounterServer(seed = seed)\n\
}.\n\
\n\
pub (server: CounterServer) init(): Result[Int, Error] ->\n\
Ok(server.seed).\n\
\n\
pub (server: CounterServer) handle_call(state: Int, request: Int): Result[CallReply[Int, Int], Error] ->\n\
Ok({state + request, state + request}).\n\
\n\
pub (server: CounterServer) handle_cast(state: Int, event: Int): Result[Int, Error] ->\n\
Ok(state + event).\n\
\n\
pub close(server_ref: ServerRef[Int, Int, Int, Int]): Unit ->\n\
server_ref.stop().\n\
\n\
pub read_after_event(server_ref: ServerRef[Int, Int, Int, Int]): Result[Int, Error] ->\n\
server_ref.cast(3);\n\
server_ref.call(4).\n\
\n\
pub unwrap(result: Result[Int, Error]): Int ->\n\
case result {\n\
    Ok(value) ->\n\
        value;\n\
\n\
    Err(_reason) ->\n\
        0\n\
}.\n\
\n\
pub run(result: Result[ServerRef[Int, Int, Int, Int], Error]): Int ->\n\
case result {\n\
    Ok(server_ref) ->\n\
        unwrap(read_after_event(server_ref));\n\
\n\
    Err(_reason) ->\n\
        0\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(run(GenServer.start(CounterServer(1))))).\n",
    )
    .expect("failed to write imported GenServer fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("handle_call(Server, State, Request)"),
        "GenServer.call should lower through callback dispatch: {}",
        erl_source
    );
    assert!(
        erl_source.contains("{cast, Event}"),
        "GenServer.cast should lower through the shared BEAM process loop: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_beam_gen_server:start")
            && !erl_source.contains("std_beam_gen_server:call")
            && !erl_source.contains("std_beam_gen_server:cast"),
        "GenServer intrinsics must not lower to a backend std module call: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated GenServer project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "8\n");
}

/// Verifies BEAM Supervisor operations lower through compiler-owned intrinsics.
///
/// Inputs:
/// - A project importing `std.beam.Supervisor` functions and types.
/// - Helper functions that call `Supervisor.child_spec`, `supervisor.start`,
///   and `supervisor.stop`.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` compiles and
///   emits the local Supervisor proof tuple and shared BEAM process
///   messages instead of backend std module calls.
///
/// Transformation:
/// - Resolves Supervisor calls through summaries, lowers them to
///   compiler-owned CoreIR intrinsics, emits Erlang for the local
///   `ChildSpec` proof, routes `start`/`stop` through the shared BEAM
///   process helpers, and keeps the runtime launcher executable without
///   claiming full supervision-tree behavior yet.
#[test]
fn build_command_compiles_imported_beam_supervisor_child_spec_start_stop() {
    let dir = make_temp_dir("directory_project_imported_beam_supervisor_child_spec_start_stop");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.beam.Supervisor.\n\
import type std.beam.Supervisor.{ChildSpec, Supervisor}.\n\
import type std.core.Result.Result.\n\
import type std.core.Error.Error.\n\
import std.core.Unit.{Unit}.\n\
\n\
pub start_supervisor(): Result[Supervisor, Error] ->\n\
Supervisor.start().\n\
\n\
pub spec(value: Int): ChildSpec[Int] ->\n\
Supervisor.child_spec(value).\n\
\n\
pub start_child(supervisor: Supervisor, value: Int): Result[Int, Error] ->\n\
supervisor.start(spec(value)).\n\
\n\
pub stop_child(supervisor: Supervisor, value: Int): Unit ->\n\
supervisor.stop(value);\n\
Unit.\n\
\n\
pub main(): Unit ->\n\
println(\"supervisor proof\").\n",
    )
    .expect("failed to write imported Supervisor fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("Loop = fun Loop(State) ->")
            && erl_source.contains("{start_child, Child, From, Ref}"),
        "Supervisor.start should lower to a BEAM supervisor process loop: {}",
        erl_source
    );
    assert!(
        erl_source.contains("{terlan_child_spec,"),
        "Supervisor.child_spec should lower to the local child spec proof: {}",
        erl_source
    );
    assert!(
        erl_source.contains("invalid_child_spec"),
        "Supervisor.start should validate the local child spec proof: {}",
        erl_source
    );
    assert!(
        erl_source.contains("{start_child, Child, self(), Ref}"),
        "Supervisor.start should use the shared reference-tagged process request: {}",
        erl_source
    );
    assert!(
        erl_source.contains("{stop_child, Value}"),
        "Supervisor.stop should use the shared process send helper: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_beam_supervisor:start")
            && !erl_source.contains("std_beam_supervisor:stop"),
        "Supervisor operations must not lower to backend std module calls: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated Supervisor project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&launcher_output.stdout),
        "supervisor proof\n"
    );
}

/// Verifies BEAM NativeBridge operations lower through compiler-owned intrinsics.
///
/// Inputs:
/// - A project importing `std.beam.NativeBridge` functions and types.
/// - Helper functions that call `NativeBridge.start`, `bridge.call`,
///   `bridge.dispose`, and `bridge.stop`.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` compiles and
///   emits the shared BEAM process proof shape plus stable not-loaded call
///   result.
///
/// Transformation:
/// - Resolves NativeBridge calls through summaries, lowers them to
///   compiler-owned CoreIR intrinsics, emits Erlang through the shared
///   BEAM process helper, and keeps real SafeNative transport attachment
///   out of this compiler-plumbing slice.
#[test]
fn build_command_compiles_imported_beam_native_bridge_start_call_dispose_stop() {
    let dir =
        make_temp_dir("directory_project_imported_beam_native_bridge_start_call_dispose_stop");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.beam.NativeBridge.\n\
import type std.beam.NativeBridge.NativeBridge.\n\
import type std.core.Result.Result.\n\
import type std.core.Error.Error.\n\
import std.core.Unit.{Unit}.\n\
\n\
pub start(resource: String): Dynamic ->\n\
NativeBridge.start(resource).\n\
\n\
pub send(bridge: NativeBridge[String], command: String): Result[String, Error] ->\n\
bridge.call(command).\n\
\n\
pub dispose(bridge: NativeBridge[String]): Unit ->\n\
bridge.dispose();\n\
Unit.\n\
\n\
pub stop(bridge: NativeBridge[String]): Unit ->\n\
bridge.stop();\n\
Unit.\n\
\n\
pub main(): Unit ->\n\
println(\"native bridge proof\").\n",
    )
    .expect("failed to write imported NativeBridge fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("Loop = fun Loop(State) ->")
            && erl_source.contains("spawn(fun() -> Loop("),
        "NativeBridge.start should lower to the shared BEAM process proof: {}",
        erl_source
    );
    assert!(
        erl_source.contains("native_bridge_not_loaded"),
        "NativeBridge.call should lower to the stable not-loaded result: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_beam_native_bridge:start")
            && !erl_source.contains("std_beam_native_bridge:call")
            && !erl_source.contains("std_beam_native_bridge:dispose")
            && !erl_source.contains("std_beam_native_bridge:stop"),
        "NativeBridge operations must not lower to backend std module calls: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated NativeBridge project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&launcher_output.stdout),
        "native bridge proof\n"
    );
}

/// Verifies same-named core and BEAM Task receiver methods keep type origin.
///
/// Inputs:
/// - A project importing `std.core.Task.Task` as `CoreTask` and
///   `std.beam.Task.Task` as `BeamTask`.
/// - Receiver calls to `task.result()` on both aliases and `task.cancel()`
///   on the BEAM alias.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints `4`.
///
/// Transformation:
/// - Resolves selected function and type aliases through summaries, lowers
///   `CoreTask.result()` through the portable completed-task intrinsic, and
///   lowers `BeamTask.result()/cancel()` through the shared BEAM process
///   intrinsic path.
#[test]
fn build_command_compiles_aliased_core_and_beam_task_receiver_calls() {
    let dir = make_temp_dir("directory_project_aliased_core_and_beam_task_receiver_calls");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.core.Task.{done as done_core}.\n\
import type std.core.Task.{Task as CoreTask}.\n\
import std.beam.Task.{start as start_beam}.\n\
import type std.beam.Task.{Task as BeamTask}.\n\
import std.core.Result.{Err, Ok}.\n\
import type std.core.Result.Result.\n\
import type std.core.Error.Error.\n\
\n\
pub core_value(task: CoreTask[Int]): Result[Int, Error] ->\n\
task.result().\n\
\n\
pub beam_value(task: BeamTask[Int]): Result[Int, Error] ->\n\
task.result().\n\
\n\
pub stop(task: BeamTask[Int]): Unit ->\n\
task.cancel().\n\
\n\
pub unwrap(result: Result[Int, Error]): Int ->\n\
case result {\n\
    Ok(x) ->\n\
        x;\n\
\n\
    Err(_reason) ->\n\
        0\n\
}.\n\
\n\
pub seed(): Int ->\n\
unwrap(core_value(done_core(4))).\n\
\n\
pub finish(task: BeamTask[Int]): Int ->\n\
let value = unwrap(beam_value(task));\n\
stop(task);\n\
value.\n\
\n\
pub run(result: Result[BeamTask[Int], Error]): Int ->\n\
case result {\n\
    Ok(task) ->\n\
        finish(task);\n\
\n\
    Err(_reason) ->\n\
        0\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(run(start_beam(() -> seed())))).\n",
    )
    .expect("failed to write aliased core/BEAM Task fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("{result, From, Ref}"),
        "BeamTask.result should lower to the shared BEAM process loop: {}",
        erl_source
    );
    assert!(
        erl_source.contains("cancel ->"),
        "BeamTask.cancel should lower to the shared BEAM process loop: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_core_task:result")
            && !erl_source.contains("std_beam_task:result")
            && !erl_source.contains("std_beam_task:cancel"),
        "Task aliases must lower through compiler-owned intrinsics: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated aliased Task project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "4\n");
}
