use super::*;
use terlan_typeck::{CoreImport, CoreImportKind};

/// Builds a minimal lowered module with one injected module import.
///
/// Inputs:
/// - `module_name`: fully qualified import module to add to the fixture.
///
/// Output:
/// - Lowered CoreIR module containing a simple `Int` function and the import.
///
/// Transformation:
/// - Reuses the normal parser/resolver/lowering path for the body, then adds a
///   synthetic import so target-profile family gating can be tested before the
///   generated std module summaries exist.
fn module_with_module_import(module_name: &str) -> CoreModule {
    let mut module = lower(
        "\
module profile_target_import.\n\
\n\
pub main(): Int ->\n\
1.\n",
        "src/profile_target_import.terl",
    );
    module.imports.push(CoreImport {
        module: module_name.to_string(),
        kind: CoreImportKind::Module,
    });
    module
}

/// Verifies target profiles reject asset imports that need command-owned
/// filesystem resolution.
///
/// Inputs:
/// - A source module with a CSS asset import and a simple function body.
///
/// Output:
/// - Test passes when Erlang target-profile validation reports a stable
///   unsupported asset-import-resolution diagnostic.
///
/// Transformation:
/// - Lowers the parsed module through CoreIR, preserving the import kind,
///   then validates that generic backend compilation does not silently
///   accept the unresolved asset import.
#[test]
fn rejects_asset_import_resolution_for_generic_target_profile() {
    let module = lower(
        "module profile_asset_import.\n\nimport css \"./style.css\" as PageCss.\n\npub main(): Int ->\n    1.\n",
        "profile_asset_import.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("asset import resolution Css import `PageCss<-./style.css`")
        }),
        "expected asset import target-profile diagnostic, got {violations:?}"
    );
}

/// Verifies unsupported concrete Task operations are blocked until a
/// backend execution contract exists.
///
/// Inputs:
/// - A source module that imports `std.core.Task`, mentions `Task[Int]` in
///   its signature, and calls `Task.spawn(() -> 1)` in the body.
///
/// Output:
/// - Test passes when Erlang target-profile validation reports a stable
///   unsupported Task-operation diagnostic.
///
/// Transformation:
/// - Lowers the parsed module through std-summary-backed resolution and
///   CoreIR, then validates that executable Task calls cannot pass into
///   backend emission before the backend owns Task runtime semantics.
#[test]
fn rejects_std_core_task_operation_for_erlang_profile() {
    let module = lower(
        "\
module profile_task_operation.\n\
\n\
import std.core.Task.\n\
\n\
pub complete(): Task[Int] ->\n\
Task.spawn(() -> 1).\n",
        "std/core/task.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("task operation std.core.Task.spawn")
        }),
        "expected Task operation target-profile diagnostic, got {violations:?}"
    );
}

/// Verifies Rust-backed portable std modules are rejected unless the command
/// owns SafeNative packaging for the selected backend.
///
/// Inputs:
/// - A source module that imports `std.data.Json` and calls `Json.parse`.
///
/// Output:
/// - Test passes when default Erlang validation rejects JSON and the explicit
///   SafeNative-enabled Erlang option admits it.
///
/// Transformation:
/// - Resolves the portable JSON std contract from checked-in summaries,
///   lowers the module to CoreIR, and validates that executable JSON use is
///   blocked until the selected command owns the Rust/SafeNative bridge.
#[test]
fn gates_rust_backed_json_std_module_for_erlang_profile() {
    let module = lower(
        "\
module profile_json_operation.\n\
\n\
import std.data.Json.\n\
import type std.data.Json.Json.\n\
import type std.data.Json.JsonError.\n\
import type std.core.Result.Result.\n\
\n\
pub parse_value(text: String): Result[Json, JsonError] ->\n\
Json.parse(text).\n",
        "src/profile_json_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("rust-backed std module std.data.Json")
        }),
        "expected Rust-backed JSON target-profile diagnostic, got {violations:?}"
    );

    let allowed = target_profile_checks_with_options(
        &module,
        TargetProfile::Erlang,
        TargetProfileCheckOptions {
            allow_asset_imports: false,
            allow_rust_backed_std_modules: true,
        },
    );
    assert!(
        !allowed.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("rust-backed std module std.data.Json")
        }),
        "SafeNative-enabled Erlang validation should accept JSON, got {allowed:?}"
    );
}

/// Verifies JavaScript std modules are rejected outside JavaScript profiles.
///
/// Inputs:
/// - A lowered CoreIR module with a synthetic `std.js.String` import.
///
/// Output:
/// - Test passes when Erlang and CoreV0 reject the import, while `js.shared`
///   accepts it.
///
/// Transformation:
/// - Exercises the import-family target gate directly, proving JavaScript std
///   contracts cannot pass into non-JS backend validation by accident.
#[test]
fn rejects_js_std_module_for_non_js_profiles() {
    let module = module_with_module_import("std.js.String");

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);
    assert!(
        erlang.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("JavaScript std module std.js.String")
        }),
        "expected JavaScript std diagnostic for Erlang, got {erlang:?}"
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);
    assert!(
        core_v0.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("JavaScript std module std.js.String")
        }),
        "expected JavaScript std diagnostic for CoreV0, got {core_v0:?}"
    );

    let js_shared = target_profile_checks(&module, TargetProfile::JsShared);
    assert!(
        !js_shared.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("JavaScript std module")
        }),
        "js.shared should accept shared JavaScript std imports, got {js_shared:?}"
    );
}

/// Verifies JavaScript profiles reject BEAM std modules.
///
/// Inputs:
/// - A lowered CoreIR module with a synthetic `std.beam.Process` import.
///
/// Output:
/// - Test passes when `js.shared` rejects the import with a stable
///   target-profile diagnostic.
///
/// Transformation:
/// - Exercises the import-family gate directly, proving BEAM-specific process
///   contracts cannot pass into JS backend validation.
#[test]
fn rejects_beam_std_module_for_js_profile() {
    let module = module_with_module_import("std.beam.Process");

    let js_shared = target_profile_checks(&module, TargetProfile::JsShared);
    assert!(
        js_shared.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("BEAM std module std.beam.Process")
        }),
        "expected BEAM std diagnostic for js.shared, got {js_shared:?}"
    );
}

/// Verifies JavaScript profiles reject native std modules.
///
/// Inputs:
/// - A lowered CoreIR module with a synthetic
///   `std.native.collections.Vector` import.
///
/// Output:
/// - Test passes when `js.shared` rejects the import with a stable
///   target-profile diagnostic.
///
/// Transformation:
/// - Exercises the import-family gate directly, proving native-specific std
///   contracts cannot pass into JS backend validation.
#[test]
fn rejects_native_std_module_for_js_profile() {
    let module = module_with_module_import("std.native.collections.Vector");

    let js_shared = target_profile_checks(&module, TargetProfile::JsShared);
    assert!(
        js_shared.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("native std module std.native.collections.Vector")
        }),
        "expected native std diagnostic for js.shared, got {js_shared:?}"
    );
}

/// Verifies browser DOM bindings require the browser JavaScript profile.
///
/// Inputs:
/// - A lowered CoreIR module with a synthetic `std.js.Dom.Document` import.
///
/// Output:
/// - Test passes when `js.shared` rejects the import and `js.browser` accepts
///   it.
///
/// Transformation:
/// - Encodes the first coarse generated-binding profile rule before generated
///   per-module profile metadata exists.
#[test]
fn rejects_browser_dom_js_std_module_for_shared_js_profile() {
    let module = module_with_module_import("std.js.Dom.Document");

    let js_shared = target_profile_checks(&module, TargetProfile::JsShared);
    assert!(
        js_shared.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("JavaScript std module std.js.Dom.Document")
        }),
        "expected DOM JavaScript std diagnostic for js.shared, got {js_shared:?}"
    );

    let js_browser = target_profile_checks(&module, TargetProfile::JsBrowser);
    assert!(
        !js_browser.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("JavaScript std module")
        }),
        "js.browser should accept DOM JavaScript std imports, got {js_browser:?}"
    );
}

/// Verifies BEAM std modules are target-gated outside BEAM profiles.
///
/// Inputs:
/// - A source module that imports the `std.beam.Process` type contract and
///   uses it in a function signature.
///
/// Output:
/// - Test passes when the portable CoreV0 target-profile validation reports
///   a stable unsupported BEAM std module diagnostic, while the full
///   Erlang profile accepts the same type-level contract.
///
/// Transformation:
/// - Resolves the BEAM process contract from checked-in summaries, lowers
///   the module to CoreIR, and validates that BEAM-specific std contracts
///   remain ordinary imports with target-profile gating rather than source
///   grammar special cases.
#[test]
fn rejects_beam_std_module_for_core_v0_profile() {
    let module = lower(
        "\
module profile_beam_process_contract.\n\
\n\
import type std.beam.Process.Process.\n\
import std.core.Unit.{Unit}.\n\
\n\
pub observe(process: Process[String]): Unit ->\n\
Unit.\n",
        "src/profile_beam_process_contract.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);
    assert!(
        !erlang.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("BEAM std module")
        }),
        "Erlang profile should accept BEAM std contracts, got {erlang:?}"
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);
    assert!(
        core_v0.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("BEAM std module std.beam.Process")
        }),
        "expected BEAM std target-profile diagnostic, got {core_v0:?}"
    );
}

/// Verifies NativeBridge contracts are target-gated outside BEAM profiles.
///
/// Inputs:
/// - A source module that imports the `std.beam.NativeBridge` type contract
///   and uses it in a function signature.
///
/// Output:
/// - Test passes when portable CoreV0 validation reports a stable
///   unsupported BEAM std module diagnostic for `std.beam.NativeBridge`.
///
/// Transformation:
/// - Resolves the BEAM native-bridge contract from checked-in summaries,
///   lowers the module to CoreIR, and validates that SafeNative/BEAM bridge
///   types remain target-profile gated before any native attachment path is
///   considered.
#[test]
fn rejects_beam_native_bridge_contract_for_core_v0_profile() {
    let module = lower(
        "\
module profile_beam_native_bridge_contract.\n\
\n\
import type std.beam.NativeBridge.NativeBridge.\n\
import std.core.Unit.{Unit}.\n\
\n\
pub observe(bridge: NativeBridge[String]): Unit ->\n\
Unit.\n",
        "src/profile_beam_native_bridge_contract.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);
    assert!(
        !erlang.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("BEAM std module")
        }),
        "Erlang profile should accept BEAM NativeBridge contracts, got {erlang:?}"
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);
    assert!(
        core_v0.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("BEAM std module std.beam.NativeBridge")
        }),
        "expected BEAM NativeBridge target-profile diagnostic, got {core_v0:?}"
    );
}

/// Verifies every current BEAM bridge contract module is gated together.
///
/// Inputs:
/// - A source module importing representative type contracts from Agent,
///   Backpressure, GenServer, Message, NativeBridge, Process, Supervisor,
///   and Task.
///
/// Output:
/// - Test passes when CoreV0 target-profile validation reports stable
///   unsupported BEAM std module diagnostics for each imported module.
///
/// Transformation:
/// - Resolves the whole BEAM contract family from checked-in summaries,
///   lowers the module once, and validates that adding new bridge-adjacent
///   std modules does not accidentally make any BEAM-only contract
///   portable.
#[test]
fn rejects_all_beam_bridge_contract_modules_for_core_v0_profile() {
    let module = lower(
        "\
module profile_beam_bridge_family_contract.\n\
\n\
import type std.beam.Agent.Agent.\n\
import type std.beam.Backpressure.Credit.\n\
import type std.beam.GenServer.CallReply.\n\
import type std.beam.Message.Message.\n\
import type std.beam.NativeBridge.NativeBridge.\n\
import type std.beam.Process.Process.\n\
import type std.beam.Supervisor.ChildSpec.\n\
import type std.beam.Supervisor.Supervisor.\n\
import type std.beam.Task.Task.\n\
import std.core.Unit.{Unit}.\n\
\n\
pub observe(\n\
agent: Agent[Int],\n\
credit: Credit,\n\
reply: CallReply[String, Int],\n\
message: Message[String],\n\
bridge: NativeBridge[String],\n\
process: Process[String],\n\
child: ChildSpec[Process[String]],\n\
supervisor: Supervisor,\n\
task: Task[Int]\n\
): Unit ->\n\
Unit.\n",
        "src/profile_beam_bridge_family_contract.terl",
    );

    let erlang = target_profile_checks(&module, TargetProfile::Erlang);
    assert!(
        erlang.is_empty(),
        "Erlang profile should accept the BEAM bridge contract family, got {erlang:?}"
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);
    for expected in [
        "std.beam.Agent",
        "std.beam.Backpressure",
        "std.beam.GenServer",
        "std.beam.Message",
        "std.beam.NativeBridge",
        "std.beam.Process",
        "std.beam.Supervisor",
        "std.beam.Task",
    ] {
        assert!(
            core_v0.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation
                        .message
                        .contains(&format!("BEAM std module {expected}"))
            }),
            "expected BEAM std target-profile diagnostic for {expected}, got {core_v0:?}"
        );
    }
}

/// Verifies paired BEAM Agent state transitions are admitted after runtime
/// lowering exists.
///
/// Inputs:
/// - A source module that imports `std.beam.Agent` and calls the deferred
///   paired-result `Agent.get_and_update` operation.
///
/// Output:
/// - Test passes when the full Erlang profile accepts `get_and_update`
///   without an unsupported BEAM Agent operation diagnostic.
///
/// Transformation:
/// - Resolves the Agent type contract from checked-in summaries, lowers the
///   source to CoreIR, and validates that the paired state/value operation
///   is part of the admitted BEAM Agent runtime surface.
#[test]
fn accepts_beam_agent_get_and_update_operation_for_erlang_profile() {
    let module = lower(
        "\
module profile_beam_agent_operation.\n\
\n\
import std.beam.Agent.\n\
import type std.beam.Agent.Agent.\n\
import type std.core.Error.Error.\n\
import type std.core.Result.Result.\n\
\n\
pub queue_update(agent: Agent[Int]): Int ->\n\
Agent.get_and_update(agent, (value: Int) -> {value, value}).\n",
        "src/profile_beam_agent_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        !violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("BEAM Agent operation std.beam.Agent.get_and_update")
        }),
        "BEAM Agent get_and_update should be admitted, got {violations:?}"
    );
}

/// Verifies GenServer runtime operations are admitted after callback lowering exists.
///
/// Inputs:
/// - A source module importing `std.beam.GenServer` and calling
///   `GenServer.start(server)`.
///
/// Output:
/// - Test passes when the full Erlang profile accepts `GenServer.start`
///   without an unsupported GenServer operation diagnostic.
///
/// Transformation:
/// - Resolves the GenServer contract from checked-in summaries, lowers the
///   source to CoreIR, and validates that callback-process startup is part
///   of the admitted BEAM GenServer runtime surface.
#[test]
fn accepts_beam_gen_server_operation_for_erlang_profile() {
    let module = lower(
        "\
module profile_beam_gen_server_operation.\n\
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
pub start_server(server: CounterServer): Result[ServerRef[Int, Int, Int, Int], Error] ->\n\
GenServer.start(server).\n",
        "src/profile_beam_gen_server_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        !violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("BEAM GenServer operation std.beam.GenServer.start")
        }),
        "BEAM GenServer.start should be admitted, got {violations:?}"
    );
}

/// Verifies NativeBridge runtime operations are admitted once local lowering exists.
///
/// Inputs:
/// - A source module importing `std.beam.NativeBridge` and calling
///   `NativeBridge.start(resource)`.
///
/// Output:
/// - Test passes when the full Erlang profile accepts the NativeBridge
///   operation without an unsupported-operation diagnostic.
///
/// Transformation:
/// - Keeps the callable NativeBridge contract visible while proving the
///   Erlang profile has an explicit compiler-owned lowering decision for
///   the local bridge proof.
#[test]
fn accepts_beam_native_bridge_operation_for_erlang_profile() {
    let module = lower(
        "\
module profile_beam_native_bridge_operation.\n\
\n\
import std.beam.NativeBridge.\n\
import type std.beam.NativeBridge.NativeBridge.\n\
import type std.core.Result.Result.\n\
import type std.core.Error.Error.\n\
\n\
pub start_bridge(resource: String): Result[NativeBridge[String], Error] ->\n\
NativeBridge.start(resource).\n",
        "src/profile_beam_native_bridge_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        !violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("BEAM NativeBridge operation std.beam.NativeBridge.start")
        }),
        "BEAM NativeBridge.start should be admitted for Erlang, got {violations:?}"
    );
}

/// Verifies Supervisor runtime operations are admitted once local lowering exists.
///
/// Inputs:
/// - A source module importing `std.beam.Supervisor` and calling
///   `Supervisor.child_spec(value)`.
///
/// Output:
/// - Test passes when the full Erlang profile accepts the Supervisor
///   operation without an unsupported-operation diagnostic.
///
/// Transformation:
/// - Keeps the callable Supervisor contract visible while proving the
///   Erlang profile has an explicit compiler-owned lowering decision for
///   the local supervision proof.
#[test]
fn accepts_beam_supervisor_operation_for_erlang_profile() {
    let module = lower(
        "\
module profile_beam_supervisor_operation.\n\
\n\
import std.beam.Supervisor.\n\
\n\
pub make_spec(value: Int): Dynamic ->\n\
Supervisor.child_spec(value).\n",
        "src/profile_beam_supervisor_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        !violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("BEAM Supervisor operation std.beam.Supervisor.child_spec")
        }),
        "BEAM Supervisor.child_spec should be admitted for Erlang, got {violations:?}"
    );
}

/// Verifies BEAM Task operations are admitted once process lowering exists.
///
/// Inputs:
/// - A source module that imports `std.beam.Task` and calls `Task.start`.
///
/// Output:
/// - Test passes when the full Erlang profile accepts `Task.start`
///   without an unsupported BEAM Task operation diagnostic.
///
/// Transformation:
/// - Resolves the BEAM Task type contract from checked-in summaries,
///   lowers the source to CoreIR, and validates that executable
///   task-process calls are admitted after shared BEAM process lowering is
///   implemented.
#[test]
fn accepts_beam_task_operation_for_erlang_profile() {
    let module = lower(
        "\
module profile_beam_task_operation.\n\
\n\
import std.beam.Task.\n\
import type std.beam.Task.Task.\n\
import type std.core.Error.Error.\n\
import type std.core.Result.Result.\n\
\n\
pub start_work(): Result[Task[Int], Error] ->\n\
Task.start(() -> 1).\n",
        "src/profile_beam_task_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Erlang);

    assert!(
        !violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("BEAM Task operation std.beam.Task.start")
        }),
        "BEAM Task.start should be admitted, got {violations:?}"
    );
}

/// Verifies Rust-backed web/data std modules are target-gated together.
///
/// Inputs:
/// - A source module that imports `std.encoding.Base64`, `std.io.Path`, and
///   `std.net.Uri`.
///
/// Output:
/// - Test passes when Erlang target-profile validation reports stable
///   unsupported Rust-backed std module diagnostics for all three imports.
///
/// Transformation:
/// - Resolves the portable utility std contracts from checked-in summaries,
///   lowers the module to CoreIR, and validates that executable utility use
///   is blocked until the selected target owns the Rust/SafeNative bridge.
#[test]
fn rejects_rust_backed_web_data_std_modules_for_erlang_profile() {
    let module = lower(
        "\
module profile_web_data_operation.\n\
\n\
import std.encoding.Base64.\n\
import std.io.Path.\n\
import std.net.Uri.\n\
import type std.core.Result.Result.\n\
import type std.encoding.Base64.Base64Error.\n\
import type std.io.Path.Path.\n\
import type std.io.Path.PathError.\n\
import type std.net.Uri.Uri.\n\
import type std.net.Uri.UriError.\n\
\n\
pub encode(text: String): String ->\n\
Base64.encode(text).\n\
\n\
pub parse_path(text: String): Result[Path, PathError] ->\n\
Path.from_string(text).\n\
\n\
pub parse_uri(text: String): Result[Uri, UriError] ->\n\
Uri.parse(text).\n",
        "src/profile_web_data_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Erlang);
    let messages = violations
        .iter()
        .map(|violation| violation.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        messages.contains("rust-backed std module std.encoding.Base64"),
        "expected Base64 target-profile diagnostic, got {violations:?}"
    );
    assert!(
        messages.contains("rust-backed std module std.io.Path"),
        "expected Path target-profile diagnostic, got {violations:?}"
    );
    assert!(
        messages.contains("rust-backed std module std.net.Uri"),
        "expected Uri target-profile diagnostic, got {violations:?}"
    );
}

/// Verifies Rust-backed HTTP std modules are accepted by web-capable profiles.
///
/// Inputs:
/// - A source module that imports `std.http.Request` and `std.http.Response`.
///
/// Output:
/// - Test passes when Erlang and JavaScript web packaging profiles accept the
///   HTTP std modules without target-profile diagnostics.
///
/// Transformation:
/// - Resolves the HTTP std contracts from checked-in summaries, lowers the
///   module to CoreIR, and validates that the Rust/Tokio-owned HTTP server
///   surface is available to the 0.0.5 web package path.
#[test]
fn accepts_rust_backed_http_std_modules_for_web_profiles() {
    let module = lower(
        "\
module profile_http_operation.\n\
\n\
import std.http.Request.\n\
import std.http.Response.\n\
import type std.http.Request.Request.\n\
import type std.http.Response.Response.\n\
\n\
pub handle(_request: Request): Response ->\n\
Response.text(\"ok\").\n",
        "src/profile_http_operation.terl",
    );

    for profile in [
        TargetProfile::Erlang,
        TargetProfile::JsShared,
        TargetProfile::JsBrowser,
        TargetProfile::JsWorker,
    ] {
        let violations = target_profile_checks(&module, profile);
        assert!(
            !violations.iter().any(|violation| {
                violation
                    .message
                    .contains("rust-backed std module std.http.Request")
                    || violation
                        .message
                        .contains("rust-backed std module std.http.Response")
            }),
            "{profile:?} should accept HTTP std modules, got {violations:?}"
        );
    }
}

/// Verifies Postgres std imports are target-gated until the worker adapter can
/// execute them.
///
/// Inputs:
/// - A source module that imports `std.db.Postgres` and calls its public
///   connection function.
///
/// Output:
/// - Test passes when Erlang target-profile validation reports a stable
///   unsupported Rust-backed std module diagnostic for the import.
///
/// Transformation:
/// - Resolves the Postgres std contract from checked-in summaries, lowers the
///   module to CoreIR, and validates that database APIs do not silently pass
///   into a backend profile before the supervised worker bridge exists.
#[test]
fn rejects_postgres_std_module_for_erlang_profile_until_adapter_exists() {
    let module = lower(
        "\
module profile_postgres_operation.\n\
\n\
import std.db.Postgres.\n\
import type std.db.Postgres.Config.\n\
import type std.db.Postgres.Pool.\n\
import type std.core.Error.Error.\n\
import type std.core.Result.Result.\n\
\n\
pub connect(config: Config): Result[Pool, Error] ->\n\
Postgres.connect(config).\n",
        "src/profile_postgres_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Erlang);
    let messages = violations
        .iter()
        .map(|violation| violation.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        messages.contains("rust-backed std module std.db.Postgres"),
        "expected Postgres target-profile diagnostic, got {violations:?}"
    );
}
