use super::*;
use crate::terlan_typeck::{CoreImport, CoreImportKind};

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
/// - Test passes when VM target-profile validation reports a stable
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

    let violations = target_profile_checks(&module, TargetProfile::Vm);

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

/// Verifies browser target-profile validation accepts browser asset imports.
///
/// Inputs:
/// - A source module with a CSS asset import and a simple function body.
///
/// Output:
/// - Test passes when `js.browser` validation accepts the asset import.
///
/// Transformation:
/// - Treats asset imports as browser target evidence while keeping non-browser
///   target profiles strict.
#[test]
fn accepts_asset_import_resolution_for_browser_target_profile() {
    let module = lower(
        "module profile_browser_asset_import.\n\nimport css \"./style.css\" as PageCss.\n\npub main(): Int ->\n    1.\n",
        "profile_browser_asset_import.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::JsBrowser);

    assert!(
        !violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("asset import resolution")
        }),
        "js.browser should accept browser asset imports, got {violations:?}"
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
/// - Test passes when VM target-profile validation reports a stable
///   unsupported Task-operation diagnostic.
///
/// Transformation:
/// - Lowers the parsed module through std-summary-backed resolution and
///   CoreIR, then validates that executable Task calls cannot pass into
///   backend emission before the backend owns Task runtime semantics.
#[test]
fn rejects_std_core_task_operation_for_vm_profile() {
    let module = lower(
        "\
module profile_task_operation.\n\
\n\
import std.core.Task.\n\
\n\
pub complete(): Task[Int] ->\n\
Task.spawn(() -> 1).\n",
        "std/core/Task.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Vm);

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
/// owns NativeBoundary packaging for the selected backend.
///
/// Inputs:
/// - A source module that imports `std.data.Json` and calls `Json.parse`.
///
/// Output:
/// - Test passes when default VM validation and the explicit
///   NativeBoundary-enabled VM option both admit it.
///
/// Transformation:
/// - Resolves the portable JSON std contract from checked-in summaries,
///   lowers the module to CoreIR, and validates that executable JSON use is
///   admitted through the VM-owned in-process Rust/NativeBoundary bridge.
#[test]
fn admits_vm_owned_json_std_module_for_vm_profile() {
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

    let violations = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        !violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("rust-backed std module std.data.Json")
        }),
        "VM-owned JSON dispatch should be accepted, got {violations:?}"
    );

    let allowed = target_profile_checks_with_options(
        &module,
        TargetProfile::Vm,
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
        "NativeBoundary-enabled VM validation should accept JSON, got {allowed:?}"
    );
}

/// Verifies JavaScript std modules are rejected outside JavaScript profiles.
///
/// Inputs:
/// - A lowered CoreIR module with a synthetic `std.js.String` import.
///
/// Output:
/// - Test passes when VM and CoreV0 reject the import, while `js.shared`
///   accepts it.
///
/// Transformation:
/// - Exercises the import-family target gate directly, proving JavaScript std
///   contracts cannot pass into non-JS backend validation by accident.
#[test]
fn rejects_js_std_module_for_non_js_profiles() {
    let module = module_with_module_import("std.js.String");

    let vm = target_profile_checks(&module, TargetProfile::Vm);
    assert!(
        vm.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("JavaScript std module std.js.String")
        }),
        "expected JavaScript std diagnostic for VM, got {vm:?}"
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

/// Verifies JavaScript profiles reject VM std modules.
///
/// Inputs:
/// - A lowered CoreIR module with a synthetic `std.vm.Process` import.
///
/// Output:
/// - Test passes when `js.shared` rejects the import with a stable
///   target-profile diagnostic.
///
/// Transformation:
/// - Exercises the import-family gate directly, proving VM-specific process
///   contracts cannot pass into JS backend validation.
#[test]
fn rejects_vm_std_module_for_js_profile() {
    let module = module_with_module_import("std.vm.Process");

    let js_shared = target_profile_checks(&module, TargetProfile::JsShared);
    assert!(
        js_shared.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("VM std module std.vm.Process")
        }),
        "expected VM std diagnostic for js.shared, got {js_shared:?}"
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

/// Verifies WebAssembly std modules are accepted only by the Wasm profile.
///
/// Inputs:
/// - A lowered CoreIR module with a synthetic `std.wasm.Abi` import.
///
/// Output:
/// - VM and JS profiles reject the import; `wasm.core` accepts it.
///
/// Transformation:
/// - Keeps WebAssembly ABI contracts target-specific while still allowing them
///   to participate in normal source imports and target inference.
#[test]
fn gates_wasm_std_module_to_wasm_core_profile() {
    let module = module_with_module_import("std.wasm.Abi");

    let vm = target_profile_checks(&module, TargetProfile::Vm);
    assert!(
        vm.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("Wasm std module std.wasm.Abi")
        }),
        "vm should reject Wasm std module imports, got {vm:?}"
    );

    let js_shared = target_profile_checks(&module, TargetProfile::JsShared);
    assert!(
        js_shared.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("Wasm std module std.wasm.Abi")
        }),
        "js.shared should reject Wasm std module imports, got {js_shared:?}"
    );

    let wasm_core = target_profile_checks(&module, TargetProfile::WasmCore);
    assert!(
        !wasm_core.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("Wasm std module")
        }),
        "wasm.core should accept Wasm std module imports, got {wasm_core:?}"
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

/// Verifies VM std modules are target-gated outside VM profiles.
///
/// Inputs:
/// - A source module that imports the `std.vm.Process` type contract and
///   uses it in a function signature.
///
/// Output:
/// - Test passes when the portable CoreV0 target-profile validation reports
///   a stable unsupported VM std module diagnostic, while the full
///   VM profile accepts the same type-level contract.
///
/// Transformation:
/// - Resolves the VM process contract from checked-in summaries, lowers
///   the module to CoreIR, and validates that VM-specific std contracts
///   remain ordinary imports with target-profile gating rather than source
///   grammar special cases.
#[test]
fn rejects_vm_std_module_for_core_v0_profile() {
    let module = lower(
        "\
module profile_vm_process_contract.\n\
\n\
import type std.vm.Process.Process.\n\
import std.core.Unit.{Unit}.\n\
\n\
pub observe(process: Process[String]): Unit ->\n\
Unit.\n",
        "src/profile_vm_process_contract.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);
    assert!(
        !vm.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("VM std module")
        }),
        "VM profile should accept VM std contracts, got {vm:?}"
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);
    assert!(
        core_v0.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("VM std module std.vm.Process")
        }),
        "expected VM std target-profile diagnostic, got {core_v0:?}"
    );
}

/// Verifies NativeBridge contracts are target-gated outside VM profiles.
///
/// Inputs:
/// - A source module that imports the `std.vm.NativeBridge` type contract
///   and uses it in a function signature.
///
/// Output:
/// - Test passes when portable CoreV0 validation reports a stable
///   unsupported VM std module diagnostic for `std.vm.NativeBridge`.
///
/// Transformation:
/// - Resolves the VM native-bridge contract from checked-in summaries,
///   lowers the module to CoreIR, and validates that NativeBoundary/VM bridge
///   types remain target-profile gated before any native attachment path is
///   considered.
#[test]
fn rejects_vm_native_bridge_contract_for_core_v0_profile() {
    let module = lower(
        "\
module profile_vm_native_bridge_contract.\n\
\n\
import type std.vm.NativeBridge.NativeBridge.\n\
import std.core.Unit.{Unit}.\n\
\n\
pub observe(bridge: NativeBridge[String]): Unit ->\n\
Unit.\n",
        "src/profile_vm_native_bridge_contract.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);
    assert!(
        !vm.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("VM std module")
        }),
        "VM profile should accept VM NativeBridge contracts, got {vm:?}"
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);
    assert!(
        core_v0.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("VM std module std.vm.NativeBridge")
        }),
        "expected VM NativeBridge target-profile diagnostic, got {core_v0:?}"
    );
}

/// Verifies every current VM bridge contract module is gated together.
///
/// Inputs:
/// - A source module importing representative type contracts from Agent,
///   Backpressure, GenServer, Message, NativeBridge, Process, Supervisor,
///   and Task.
///
/// Output:
/// - Test passes when CoreV0 target-profile validation reports stable
///   unsupported VM std module diagnostics for each imported module.
///
/// Transformation:
/// - Resolves the whole VM contract family from checked-in summaries,
///   lowers the module once, and validates that adding new bridge-adjacent
///   std modules does not accidentally make any VM-only contract
///   portable.
#[test]
fn rejects_all_vm_bridge_contract_modules_for_core_v0_profile() {
    let module = lower(
        "\
module profile_vm_bridge_family_contract.\n\
\n\
import type std.vm.Agent.Agent.\n\
import type std.vm.Backpressure.Credit.\n\
import type std.vm.GenServer.CallReply.\n\
import type std.vm.Message.Message.\n\
import type std.vm.NativeBridge.NativeBridge.\n\
import type std.vm.Process.Process.\n\
import type std.vm.Supervisor.ChildSpec.\n\
import type std.vm.Supervisor.Supervisor.\n\
import type std.vm.Task.Task.\n\
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
        "src/profile_vm_bridge_family_contract.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);
    assert!(
        vm.is_empty(),
        "VM profile should accept the VM bridge contract family, got {vm:?}"
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);
    for expected in [
        "std.vm.Agent",
        "std.vm.Backpressure",
        "std.vm.GenServer",
        "std.vm.Message",
        "std.vm.NativeBridge",
        "std.vm.Process",
        "std.vm.Supervisor",
        "std.vm.Task",
    ] {
        assert!(
            core_v0.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation
                        .message
                        .contains(&format!("VM std module {expected}"))
            }),
            "expected VM std target-profile diagnostic for {expected}, got {core_v0:?}"
        );
    }
}

/// Verifies source-owned Agent state policy is admitted by the VM profile.
///
/// Inputs:
/// - A source module that imports `std.vm.Agent` and calls its typed
///   source-level `get_and_update` function.
///
/// Output:
/// - Test passes when the full VM profile accepts the Agent source API.
///
/// Transformation:
/// - Resolves the Agent type contract from checked-in summaries and validates
///   that only its underlying process primitives require VM admission.
#[test]
fn accepts_vm_agent_get_and_update_operation_for_vm_profile() {
    let module = lower(
        "\
module profile_vm_agent_operation.\n\
\n\
import std.vm.Agent.\n\
import type std.vm.Agent.Agent.\n\
pub queue_update(agent: Agent[Int]): Int ->\n\
Agent.get_and_update[Int](agent, 1, 7).\n",
        "src/profile_vm_agent_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        !violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("VM Agent operation")
        }),
        "source-owned Agent policy should not require framework admission, got {violations:?}"
    );
}

/// Verifies source-owned GenServer policy is admitted for the VM profile.
///
/// Inputs:
/// - A source module importing `std.vm.GenServer` and calling its typed
///   source-level request function.
///
/// Output:
/// - Test passes when the full VM profile accepts the GenServer source API.
///
/// Transformation:
/// - Resolves the GenServer contract from checked-in summaries and validates
///   that only its underlying process primitives require VM admission.
#[test]
fn accepts_vm_gen_server_operation_for_vm_profile() {
    let module = lower(
        "\
module profile_vm_gen_server_operation.\n\
\n\
import std.vm.GenServer.\n\
import type std.vm.GenServer.ServerRef.\n\
\n\
pub request(server: ServerRef[Int]): Int ->\n\
GenServer.call[Int](server, 1, 7).\n",
        "src/profile_vm_gen_server_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        !violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("VM GenServer operation")
        }),
        "source-owned GenServer policy should not require framework admission, got {violations:?}"
    );
}

/// Verifies NativeBridge runtime operations are admitted once local lowering exists.
///
/// Inputs:
/// - A source module importing `std.vm.NativeBridge` and calling
///   `NativeBridge.start(resource)`.
///
/// Output:
/// - Test passes when the full VM profile accepts the NativeBridge
///   operation without an unsupported-operation diagnostic.
///
/// Transformation:
/// - Keeps the callable NativeBridge contract visible while proving the
///   VM profile has an explicit compiler-owned lowering decision for
///   the local bridge proof.
#[test]
fn accepts_vm_native_bridge_operation_for_vm_profile() {
    let module = lower(
        "\
module profile_vm_native_bridge_operation.\n\
\n\
import std.vm.NativeBridge.\n\
import std.vm.NativeBridge.{NativeTransfer}.\n\
import type std.vm.NativeBridge.NativeBridge.\n\
import type std.core.Result.Result.\n\
import type std.core.Error.Error.\n\
\n\
pub start_bridge(resource: String): Result[NativeBridge[String], Error] ->\n\
NativeBridge.start(resource).\n",
        "src/profile_vm_native_bridge_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        !violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation
                    .message
                    .contains("VM NativeBridge operation std.vm.NativeBridge.start")
        }),
        "VM NativeBridge.start should be admitted for VM, got {violations:?}"
    );
}

/// Verifies Supervisor source policy is admitted by the VM profile.
///
/// Inputs:
/// - A source module importing `std.vm.Supervisor` and evaluating restart
///   strategy selection.
///
/// Output:
/// - Test passes when the full VM profile accepts the Supervisor source API.
///
/// Transformation:
/// - Keeps the callable Supervisor contract visible while proving the
///   VM profile has an explicit compiler-owned lowering decision for
///   the local supervision proof.
#[test]
fn accepts_vm_supervisor_operation_for_vm_profile() {
    let module = lower(
        "\
module profile_vm_supervisor_operation.\n\
\n\
import std.vm.Supervisor.{RestForOne, selects_child}.\n\
\n\
pub selected(failed: Int, candidate: Int): Bool ->\n\
selects_child(RestForOne, failed, candidate).\n",
        "src/profile_vm_supervisor_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        !violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("VM Supervisor operation")
        }),
        "source-owned Supervisor policy should not require framework admission, got {violations:?}"
    );
}

/// Verifies source-owned Task policy is admitted once process lowering exists.
///
/// Inputs:
/// - A source module that imports `std.vm.Task` and calls `Task.result`.
///
/// Output:
/// - Test passes when the full VM profile accepts the Task source API.
///
/// Transformation:
/// - Resolves the VM Task type contract from checked-in summaries,
///   lowers the source to CoreIR, and validates that executable
///   task-process calls are admitted after shared VM process lowering is
///   implemented.
#[test]
fn accepts_vm_task_operation_for_vm_profile() {
    let module = lower(
        "\
module profile_vm_task_operation.\n\
\n\
import std.vm.Task.\n\
import type std.vm.Task.Task.\n\
\n\
pub await_work(task: Task[Int]): Int ->\n\
Task.result[Int](task, 1).\n",
        "src/profile_vm_task_operation.terl",
    );

    let violations = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        !violations.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("VM Task operation")
        }),
        "source-owned Task policy should not require framework admission, got {violations:?}"
    );
}

/// Verifies Rust-backed web/data std modules are target-gated together.
///
/// Inputs:
/// - A source module that imports `std.encoding.Base64`, `std.io.Path`, and
///   `std.net.Uri`.
///
/// Output:
/// - Test passes when VM target-profile validation accepts the direct-safe Path
///   adapter and reports stable unsupported diagnostics for Base64 and URI.
///
/// Transformation:
/// - Resolves the portable utility std contracts from checked-in summaries,
///   lowers the module to CoreIR, and validates that executable utility use
///   is blocked until the selected target owns the Rust/NativeBoundary bridge.
#[test]
fn vm_profile_accepts_direct_path_and_rejects_unsupported_web_data_adapters() {
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

    let violations = target_profile_checks(&module, TargetProfile::Vm);
    let messages = violations
        .iter()
        .map(|violation| violation.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        messages.contains("rust-backed std module std.encoding.Base64"),
        "expected Base64 target-profile diagnostic, got {violations:?}"
    );
    assert!(!messages.contains("rust-backed std module std.io.Path"));
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
/// - Test passes when VM and JavaScript web packaging profiles accept the
///   HTTP std modules without target-profile diagnostics.
///
/// Transformation:
/// - Resolves the HTTP std contracts from checked-in summaries, lowers the
///   module to CoreIR, and validates that the VM-owned HTTP server surface is
///   available to the web package path.
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
        TargetProfile::Vm,
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

/// Verifies the VM profile accepts Postgres through its actor-owned worker.
///
/// Inputs:
/// - A source module that imports `std.db.Postgres` and calls its public
///   connection function.
///
/// Output:
/// - Test passes when VM target-profile validation accepts the import.
///
/// Transformation:
/// - Resolves the Postgres std contract from checked-in summaries, lowers the
///   module to CoreIR, and validates the supervised worker bridge capability.
#[test]
fn accepts_postgres_std_module_for_vm_profile() {
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

    let violations = target_profile_checks(&module, TargetProfile::Vm);
    assert!(
        !violations.iter().any(|violation| violation
            .message
            .contains("rust-backed std module std.db.Postgres")),
        "VM should accept Postgres through its worker bridge, got {violations:?}"
    );
}
