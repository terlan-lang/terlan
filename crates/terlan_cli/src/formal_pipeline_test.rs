use super::*;

use crate::validation::target_profile::TargetProfile;

#[test]
fn compile_syntax_module_with_erlang_profile_accepts_float() {
    let source = "\
module target_profile_accept.

pub f(): Float ->
  1.0.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/target_profile_accept.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::Erlang,
    );

    assert_eq!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_some());
    assert!(result.core_diagnostics.is_empty());
}

#[test]
fn compile_syntax_module_with_profile_argument_accepts_float() {
    let source = "\
module target_profile_reject.

pub f(): Float ->
  1.0.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/target_profile_reject.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::Erlang,
    );

    assert_eq!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_some());
    assert!(result.core_diagnostics.is_empty());
}

/// Verifies the strict formal compile path accepts the portable CoreIR v0
/// target subset for a Lean-covered body.
///
/// Inputs:
/// - Source text whose function body lowers to typed integer subtraction.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are written.
///
/// Transformation:
/// - Runs the full syntax-output parse/resolve/typecheck/CoreIR path with
///   `TargetProfile::CoreV0` and asserts no profile diagnostics are emitted.
#[test]
fn compile_syntax_module_with_core_v0_profile_accepts_covered_subset() {
    let source = "\
module target_profile_core_v0_accept.

pub f(x: Int, y: Int): Int ->
  x - y.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/target_profile_core_v0_accept.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::CoreV0,
    );

    assert_eq!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_some());
    assert!(result.core_diagnostics.is_empty());
}

/// Verifies the strict formal compile path rejects CoreIR outside the
/// portable CoreIR v0 target subset.
///
/// Inputs:
/// - Source text whose function body lowers to a typed map expression.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are written.
///
/// Transformation:
/// - Runs the full syntax-output parse/resolve/typecheck/CoreIR path with
///   `TargetProfile::CoreV0` and asserts target-profile diagnostics abort
///   compilation before artifacts are returned.
#[test]
fn compile_syntax_module_with_core_v0_profile_rejects_broad_coreir() {
    let source = "\
module target_profile_core_v0_reject.

pub f(): Map ->
  #{a := 1}.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/target_profile_core_v0_reject.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::CoreV0,
    );

    assert_ne!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_none());
    assert!(
        result
            .core_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "target_profile_unsupported"),
        "Core v0 profile should report target-profile violations"
    );
}

/// Verifies native vector interface summaries are embedded with stdlib.
///
/// Inputs:
/// - Empty interface map.
///
/// Output:
/// - Test passes when `std.native.collections.Vector` is loaded from the
///   compiler-embedded std summary list.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so native std modules are
///   available for import resolution before target-capability diagnostics
///   decide whether the active backend may compile them.
#[test]
fn embedded_std_interfaces_include_native_vector_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let interface = interfaces
        .get("std.native.collections.Vector")
        .expect("embedded native vector interface");
    assert!(interface.opaque_types.contains("Vector"));
    assert!(interface.functions.contains_key(&("new".to_string(), 0)));
    let length = interface
        .functions
        .get(&("length".to_string(), 1))
        .expect("Vector.length receiver method");
    assert!(length.receiver_method);
    assert!(!length.receiver_mutable);
    let set_at = interface
        .functions
        .get(&("set_at".to_string(), 3))
        .expect("Vector.set_at mutable receiver method");
    assert!(set_at.receiver_method);
    assert!(set_at.receiver_mutable);
}

/// Verifies embedded std summaries include the portable task contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when `std.core.Task` is loaded from the embedded summary
///   list with its opaque type and receiver composition methods.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so project imports can
///   resolve the typed async contract before target profiles decide whether
///   a backend can execute it.
#[test]
fn embedded_std_interfaces_include_core_task_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let interface = interfaces
        .get("std.core.Task")
        .expect("embedded core task interface");
    assert!(interface.opaque_types.contains("Task"));
    assert!(interface.functions.contains_key(&("done".to_string(), 1)));
    assert!(interface.functions.contains_key(&("spawn".to_string(), 1)));
    let then = interface
        .functions
        .get(&("then".to_string(), 2))
        .expect("Task.then receiver method");
    assert!(then.receiver_method);
    assert!(!then.receiver_mutable);
    let result = interface
        .functions
        .get(&("result".to_string(), 1))
        .expect("Task.result receiver method");
    assert!(result.receiver_method);
}

/// Verifies embedded std summaries include the portable JSON contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when `std.data.Json` is loaded from the embedded summary
///   list with its opaque type, derived error type, and receiver accessors.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so project imports can
///   resolve the JSON API before target profiles decide whether a backend
///   can execute the Rust/SafeNative implementation.
#[test]
fn embedded_std_interfaces_include_data_json_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let interface = interfaces
        .get("std.data.Json")
        .expect("embedded data json interface");
    assert!(interface.opaque_types.contains("Json"));
    assert!(interface.public_types.contains("JsonError"));
    assert!(interface.functions.contains_key(&("parse".to_string(), 1)));
    assert!(interface
        .functions
        .contains_key(&("stringify".to_string(), 1)));
    let get = interface
        .functions
        .get(&("get".to_string(), 2))
        .expect("Json.get receiver method");
    assert!(get.receiver_method);
    assert!(!get.receiver_mutable);
    let is_null = interface
        .functions
        .get(&("is_null".to_string(), 1))
        .expect("Json.is_null receiver method");
    assert!(is_null.receiver_method);
    assert!(!is_null.receiver_mutable);
}

/// Verifies embedded std summaries include Rust-backed web/data utilities.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when `std.encoding.Base64`, `std.io.Path`, and
///   `std.net.Uri` are loaded from the embedded summary list with their
///   public contract surfaces.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so project imports can
///   resolve portable web/data utility APIs before target profiles decide
///   whether a backend can execute their Rust/SafeNative implementations.
#[test]
fn embedded_std_interfaces_include_web_data_utility_contracts() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let base64 = interfaces
        .get("std.encoding.Base64")
        .expect("embedded Base64 interface");
    assert!(base64.public_types.contains("Base64Error"));
    assert!(base64.functions.contains_key(&("encode".to_string(), 1)));
    assert!(base64.functions.contains_key(&("decode".to_string(), 1)));

    let path = interfaces
        .get("std.io.Path")
        .expect("embedded Path interface");
    assert!(path.opaque_types.contains("Path"));
    assert!(path.public_types.contains("PathError"));
    assert!(path.functions.contains_key(&("from_string".to_string(), 1)));
    let join = path
        .functions
        .get(&("join".to_string(), 2))
        .expect("Path.join receiver method");
    assert!(join.receiver_method);

    let uri = interfaces
        .get("std.net.Uri")
        .expect("embedded Uri interface");
    assert!(uri.opaque_types.contains("Uri"));
    assert!(uri.public_types.contains("UriError"));
    assert!(uri.functions.contains_key(&("parse".to_string(), 1)));
    let host = uri
        .functions
        .get(&("host".to_string(), 1))
        .expect("Uri.host receiver method");
    assert!(host.receiver_method);
}

/// Verifies embedded std summaries include the JavaScript std seed contracts.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when shared JS wrappers and generated DOM seed modules are
///   loaded from the embedded summary list.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so `std.js.*` imports resolve
///   through the same packaged interface path as hand-authored std modules.
#[test]
fn embedded_std_interfaces_include_js_std_contracts() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let string = interfaces
        .get("std.js.String")
        .expect("embedded JS String interface");
    assert!(string.opaque_types.contains("JsString"));
    assert!(string.functions.contains_key(&("from_core".to_string(), 1)));
    let to_core = string
        .functions
        .get(&("to_core".to_string(), 1))
        .expect("JsString.to_core receiver method");
    assert!(to_core.receiver_method);

    let array = interfaces
        .get("std.js.Array")
        .expect("embedded JS Array interface");
    assert!(array.opaque_types.contains("Array"));
    assert!(array.functions.contains_key(&("from_list".to_string(), 1)));
    let length = array
        .functions
        .get(&("length".to_string(), 1))
        .expect("Array.length receiver method");
    assert!(length.receiver_method);

    let promise = interfaces
        .get("std.js.Promise")
        .expect("embedded JS Promise interface");
    assert!(promise.opaque_types.contains("Promise"));
    assert!(promise
        .functions
        .contains_key(&("from_task".to_string(), 1)));
    let to_task = promise
        .functions
        .get(&("to_task".to_string(), 1))
        .expect("Promise.to_task receiver method");
    assert!(to_task.receiver_method);

    let document = interfaces
        .get("std.js.Dom.Document")
        .expect("embedded generated DOM Document interface");
    assert!(document.opaque_types.contains("Document"));
    assert!(document.functions.contains_key(&("title".to_string(), 1)));

    let html_element = interfaces
        .get("std.js.Dom.HTMLElement")
        .expect("embedded generated DOM HTMLElement interface");
    assert!(html_element.opaque_types.contains("HTMLElement"));
    assert!(html_element
        .functions
        .contains_key(&("text_content".to_string(), 1)));
}

/// Verifies generated JS std summaries resolve during normal JS compilation.
///
/// Inputs:
/// - A source module importing `std.js.String.JsString`.
///
/// Output:
/// - Test assertion only; compilation succeeds under `js.shared`.
///
/// Transformation:
/// - Runs the full parser, embedded-interface resolution, typecheck, CoreIR,
///   and target-profile path without a cache directory so the test proves the
///   compiler binary carries the generated JS std summary.
#[test]
fn compile_syntax_module_with_js_profile_resolves_js_string_summary() {
    let source = "\
module js_summary_accept.

import type std.js.String.JsString.

pub accepts(value: JsString): JsString ->
  value.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/js_summary_accept.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    );

    assert_eq!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_some());
    assert!(result.core_diagnostics.is_empty());
}

/// Verifies generated DOM summaries resolve for the browser JS profile.
///
/// Inputs:
/// - A source module importing generated `std.js.Dom.Document.Document`.
///
/// Output:
/// - Test assertion only; compilation succeeds under `js.browser`.
///
/// Transformation:
/// - Runs the full formal compilation path without a cache directory, proving
///   generated DOM summaries participate in import/typecheck like hand-authored
///   std modules once the selected target profile admits browser APIs.
#[test]
fn compile_syntax_module_with_browser_profile_resolves_generated_dom_summary() {
    let source = "\
module js_dom_summary_accept.

import type std.js.Dom.Document.Document.

pub accepts(value: Document): Document ->
  value.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/js_dom_summary_accept.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsBrowser,
    );

    assert_eq!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_some());
    assert!(result.core_diagnostics.is_empty());
}

/// Verifies embedded std summaries include the BEAM bridge contracts.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when the first BEAM bridge and Agent contract modules are
///   loaded from the embedded summary list with their target-gated types,
///   traits, and receiver methods.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so BEAM supervision,
///   process, message, backpressure, and native-bridge contracts can be
///   resolved without adding BEAM-specific grammar to Terlan source.
#[test]
fn embedded_std_interfaces_include_beam_bridge_contracts() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let agent = interfaces
        .get("std.beam.Agent")
        .expect("embedded BEAM Agent interface");
    assert!(agent.opaque_types.contains("Agent"));
    assert!(agent.functions.contains_key(&("start".to_string(), 1)));
    let get = agent
        .functions
        .get(&("get".to_string(), 1))
        .expect("Agent.get receiver method");
    assert!(get.receiver_method);
    assert!(!get.receiver_mutable);
    let update = agent
        .functions
        .get(&("update".to_string(), 2))
        .expect("Agent.update mutable receiver method");
    assert!(update.receiver_method);
    assert!(update.receiver_mutable);
    let get_and_update = agent
        .functions
        .get(&("get_and_update".to_string(), 2))
        .expect("Agent.get_and_update receiver method");
    assert!(get_and_update.receiver_method);
    assert!(!get_and_update.receiver_mutable);

    let process = interfaces
        .get("std.beam.Process")
        .expect("embedded BEAM process interface");
    assert!(process.opaque_types.contains("Process"));
    let process_like = process
        .traits
        .get("ProcessLike")
        .expect("embedded ProcessLike trait contract");
    assert!(process_like.methods.contains_key("send"));
    assert!(process_like.methods.contains_key("stop"));

    let message = interfaces
        .get("std.beam.Message")
        .expect("embedded BEAM message interface");
    assert!(message.opaque_types.contains("Message"));
    let message_codec = message
        .traits
        .get("MessageCodec")
        .expect("embedded MessageCodec trait contract");
    assert!(message_codec.methods.contains_key("wrap"));
    assert!(message_codec.methods.contains_key("unwrap"));

    let backpressure = interfaces
        .get("std.beam.Backpressure")
        .expect("embedded BEAM backpressure interface");
    assert!(backpressure.public_types.contains("Credit"));
    let backpressure_trait = backpressure
        .traits
        .get("Backpressure")
        .expect("embedded Backpressure trait contract");
    assert!(backpressure_trait.methods.contains_key("available"));
    assert!(backpressure_trait.methods.contains_key("request"));
    assert!(backpressure_trait.methods.contains_key("release"));

    let supervisor = interfaces
        .get("std.beam.Supervisor")
        .expect("embedded BEAM supervisor interface");
    assert!(supervisor.opaque_types.contains("Supervisor"));
    assert!(supervisor.opaque_types.contains("ChildSpec"));
    assert!(supervisor
        .functions
        .contains_key(&("child_spec".to_string(), 1)));
    let supervisor_start = supervisor
        .functions
        .get(&("start".to_string(), 2))
        .expect("Supervisor.start receiver method");
    assert!(supervisor_start.receiver_method);
    assert!(!supervisor_start.receiver_mutable);
    let supervisor_stop = supervisor
        .functions
        .get(&("stop".to_string(), 2))
        .expect("Supervisor.stop mutable receiver method");
    assert!(supervisor_stop.receiver_method);
    assert!(supervisor_stop.receiver_mutable);
    assert!(supervisor.traits.contains_key("Supervised"));

    let gen_server = interfaces
        .get("std.beam.GenServer")
        .expect("embedded BEAM GenServer interface");
    assert!(gen_server.public_types.contains("CallReply"));
    assert!(gen_server.opaque_types.contains("ServerRef"));
    assert!(gen_server.functions.contains_key(&("start".to_string(), 1)));
    let call = gen_server
        .functions
        .get(&("call".to_string(), 2))
        .expect("GenServer.call receiver method");
    assert!(call.receiver_method);
    assert!(!call.receiver_mutable);
    let cast = gen_server
        .functions
        .get(&("cast".to_string(), 2))
        .expect("GenServer.cast mutable receiver method");
    assert!(cast.receiver_method);
    assert!(cast.receiver_mutable);
    let stop = gen_server
        .functions
        .get(&("stop".to_string(), 1))
        .expect("GenServer.stop mutable receiver method");
    assert!(stop.receiver_method);
    assert!(stop.receiver_mutable);
    let gen_server_trait = gen_server
        .traits
        .get("GenServer")
        .expect("embedded GenServer trait contract");
    assert!(gen_server_trait.methods.contains_key("init"));
    assert!(gen_server_trait.methods.contains_key("handle_call"));
    assert!(gen_server_trait.methods.contains_key("handle_cast"));
    assert!(
        gen_server_trait
            .methods
            .get("terminate")
            .expect("GenServer terminate callback")
            .has_default
    );

    let native_bridge = interfaces
        .get("std.beam.NativeBridge")
        .expect("embedded BEAM native bridge interface");
    assert!(native_bridge.opaque_types.contains("NativeBridge"));
    assert!(native_bridge
        .functions
        .contains_key(&("start".to_string(), 1)));
    let native_call = native_bridge
        .functions
        .get(&("call".to_string(), 2))
        .expect("NativeBridge.call receiver method");
    assert!(native_call.receiver_method);
    assert!(!native_call.receiver_mutable);
    let dispose = native_bridge
        .functions
        .get(&("dispose".to_string(), 1))
        .expect("NativeBridge.dispose mutable receiver method");
    assert!(dispose.receiver_method);
    assert!(dispose.receiver_mutable);
    let native_stop = native_bridge
        .functions
        .get(&("stop".to_string(), 1))
        .expect("NativeBridge.stop mutable receiver method");
    assert!(native_stop.receiver_method);
    assert!(native_stop.receiver_mutable);
    let native_bridge_runtime = native_bridge
        .traits
        .get("NativeBridgeRuntime")
        .expect("embedded NativeBridgeRuntime trait contract");
    assert!(native_bridge_runtime
        .super_traits
        .contains(&"Supervised[NativeBridge[Resource]]".to_string()));
    assert!(native_bridge_runtime
        .super_traits
        .contains(&"Backpressure[NativeBridge[Resource]]".to_string()));
    assert!(native_bridge_runtime
        .super_traits
        .contains(&"MessageCodec[Command]".to_string()));
    assert!(native_bridge_runtime
        .super_traits
        .contains(&"MessageCodec[Reply]".to_string()));

    let task = interfaces
        .get("std.beam.Task")
        .expect("embedded BEAM Task interface");
    assert!(task.opaque_types.contains("Task"));
    assert!(task.functions.contains_key(&("start".to_string(), 1)));
    let result = task
        .functions
        .get(&("result".to_string(), 1))
        .expect("Task.result receiver method");
    assert!(result.receiver_method);
    assert!(!result.receiver_mutable);
    let cancel = task
        .functions
        .get(&("cancel".to_string(), 1))
        .expect("Task.cancel mutable receiver method");
    assert!(cancel.receiver_method);
    assert!(cancel.receiver_mutable);
}
