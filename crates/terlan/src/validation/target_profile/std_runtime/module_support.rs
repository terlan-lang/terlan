use std::collections::HashSet;

use crate::terlan_typeck::{CoreImportKind, CoreModule};

use super::super::{TargetProfile, TargetProfileCheckOptions, TargetProfileViolation};
use super::http_response;

/// Std module call-head aliases visible to target-profile validation.
///
/// Inputs:
/// - Built from one lowered CoreIR module.
///
/// Output:
/// - Alias sets for std modules whose executable operations are target-gated.
///
/// Transformation:
/// - Converts import declarations into both fully-qualified and short call-head
///   names so operation diagnostics can distinguish std APIs from unrelated
///   local modules with the same final segment.
pub(in crate::validation::target_profile) struct StdCallHeads {
    pub(in crate::validation::target_profile) task: HashSet<String>,
    pub(in crate::validation::target_profile) vm_native_bridge: HashSet<String>,
}

/// Collects target-gated std module call-head aliases.
///
/// Inputs:
/// - `module`: CoreIR module whose imports define visible remote-call heads.
///
/// Output:
/// - `StdCallHeads` containing aliases for currently target-gated std modules.
///
/// Transformation:
/// - Aggregates individual std call-head collectors behind one context object
///   so expression validation can grow without adding a new parameter for
///   every std runtime family.
pub(in crate::validation::target_profile) fn std_call_heads(module: &CoreModule) -> StdCallHeads {
    StdCallHeads {
        task: std_module_call_heads(module, "std.core.Task", "Task"),
        vm_native_bridge: std_module_call_heads(module, "std.vm.NativeBridge", "NativeBridge"),
    }
}

/// Collects module call heads that refer to an imported std runtime contract.
///
/// Inputs:
/// - `module`: CoreIR module whose imports define visible remote-call heads.
/// - `canonical_module`: fully-qualified std module identity.
/// - `short_head`: unqualified source call head made visible by module imports.
///
/// Output:
/// - Set containing the fully-qualified and short module heads when the exact
///   std module is imported.
///
/// Transformation:
/// - Converts the exact CoreIR import identity into source-call names used by
///   summaries. This lets target-profile validation recognize imported short
///   calls without treating unrelated local modules with the same final segment
///   as std runtime APIs.
fn std_module_call_heads(
    module: &CoreModule,
    canonical_module: &str,
    short_head: &str,
) -> HashSet<String> {
    let mut heads = HashSet::new();
    if module.imports.iter().any(|import| {
        import.kind == CoreImportKind::Module && import.module.as_str() == canonical_module
    }) {
        heads.insert(canonical_module.to_string());
        heads.insert(short_head.to_string());
    }
    heads
}

/// Validates module-level CoreIR import summaries against a target profile.
///
/// Inputs:
/// - `profile`: backend profile requested by the command.
/// - `module`: CoreIR module containing source import summaries.
/// - `violations`: output list for rejected profile features.
///
/// Output:
/// - Appends target-profile violations for unsupported import families.
///
/// Transformation:
/// - Rejects asset imports for normal target-profile compilation. Asset imports
///   require a command-owned resolver, such as static-site rendering, and must
///   not pass through generic backend emission silently.
/// - Validates type-only std module imports through the same target-family
///   table because backend-specific contracts, such as VM process types,
///   are still non-portable even when mentioned only in signatures.
pub(in crate::validation::target_profile) fn validate_core_imports(
    profile: TargetProfile,
    module: &CoreModule,
    options: TargetProfileCheckOptions,
    violations: &mut Vec<TargetProfileViolation>,
) {
    for import in &module.imports {
        match import.kind {
            CoreImportKind::Module | CoreImportKind::TypeModule => {
                if let Some(violation) = std_module_import_violation(
                    profile,
                    options,
                    &format!("module {}", module.module),
                    &import.module,
                ) {
                    violations.push(violation);
                }
            }
            CoreImportKind::File | CoreImportKind::Css | CoreImportKind::Markdown => {
                if !options.allow_asset_imports && profile != TargetProfile::JsBrowser {
                    violations.push(TargetProfileViolation::unsupported(
                        "asset import resolution",
                        profile,
                        &format!("module {}", module.module),
                        &format!("{:?} import `{}`", import.kind, import.module),
                    ));
                }
            }
        }
    }
}

/// Builds a target-profile violation for an unsupported std module import.
///
/// Inputs:
/// - `profile`: backend profile requested by the caller.
/// - `context`: source, module, or function context used in diagnostics.
/// - `import_module`: fully qualified module path from source or CoreIR.
///
/// Output:
/// - `Some(TargetProfileViolation)` when the std module family is unsupported.
/// - `None` when the import is not a target-gated std module or the profile
///   supports it.
///
/// Transformation:
/// - Applies the same std-family support table to both pre-lowering source
///   gates and lowered CoreIR import validation so diagnostics cannot drift
///   between build and formal validation paths.
pub(in crate::validation::target_profile) fn std_module_import_violation(
    profile: TargetProfile,
    options: TargetProfileCheckOptions,
    context: &str,
    import_module: &str,
) -> Option<TargetProfileViolation> {
    if context.starts_with("module std.") {
        return None;
    }

    if is_rust_backed_std_module(import_module)
        && !target_profile_supports_rust_backed_std_module_with_options(
            profile,
            options,
            import_module,
        )
    {
        Some(TargetProfileViolation::unsupported(
            "rust-backed std module",
            profile,
            context,
            import_module,
        ))
    } else if is_native_std_module(import_module)
        && !target_profile_supports_native_std_module(profile, import_module)
    {
        Some(TargetProfileViolation::unsupported(
            "native std module",
            profile,
            context,
            import_module,
        ))
    } else if is_js_std_module(import_module)
        && !target_profile_supports_js_std_module(profile, import_module)
    {
        Some(TargetProfileViolation::unsupported(
            "JavaScript std module",
            profile,
            context,
            import_module,
        ))
    } else if is_wasm_std_module(import_module)
        && !target_profile_supports_wasm_std_module(profile, import_module)
    {
        Some(TargetProfileViolation::unsupported(
            "Wasm std module",
            profile,
            context,
            import_module,
        ))
    } else if is_vm_std_module(import_module)
        && !target_profile_supports_vm_std_module(profile, import_module)
    {
        Some(TargetProfileViolation::unsupported(
            "VM std module",
            profile,
            context,
            import_module,
        ))
    } else {
        None
    }
}

/// Returns whether a std module is portable source API backed first by Rust.
///
/// Inputs:
/// - `module`: fully qualified module path from CoreIR import metadata.
///
/// Output:
/// - `true` when the module is a portable std surface whose current executable
///   implementation depends on the Rust/NativeBoundary bridge.
///
/// Transformation:
/// - Centralizes the small Rust-backed std allowlist so these modules can be
///   visible to docs and summary generation while unsupported target profiles
///   reject them before backend emission.
fn is_rust_backed_std_module(module: &str) -> bool {
    matches!(
        module,
        "std.data.Json"
            | "std.data.Toml"
            | "std.crypto.Hash"
            | "std.crypto.Ed25519"
            | "std.regex.Regex"
            | "std.package.Registry"
            | "std.encoding.Base64"
            | "std.encoding.Md5"
            | "std.io.Directory"
            | "std.io.Archive"
            | "std.io.Path"
            | "std.system.Arguments"
            | "std.system.Environment"
            | "std.system.Platform"
            | "std.net.Uri"
            | "std.db.Postgres"
            | "std.http.Request"
            | "std.http.Cookies"
            | "std.http.Response"
            | "std.http.Session"
            | "std.http.Sse"
            | "std.http.WebSocket"
    )
}

/// Returns whether a target profile and command can execute a Rust-backed std
/// module.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `options`: command-owned validation switches.
/// - `module`: fully qualified Rust-backed std module path.
///
/// Output:
/// - `true` only when the target profile owns executable lowering for the
///   module's Rust/NativeBoundary implementation or the command owns NativeBoundary
///   packaging for VM output.
///
/// Transformation:
/// - Keeps generic/pure validation conservative while allowing normal
///   VM build/test paths to compile portable Rust-backed std APIs through
///   the NativeBoundary bridge.
fn target_profile_supports_rust_backed_std_module_with_options(
    profile: TargetProfile,
    options: TargetProfileCheckOptions,
    module: &str,
) -> bool {
    if options.allow_rust_backed_std_modules && matches!(profile, TargetProfile::Vm) {
        let _ = module;
        return true;
    }

    if is_http_rust_backed_std_module(module) {
        return matches!(
            profile,
            TargetProfile::Vm
                | TargetProfile::JsBrowser
                | TargetProfile::JsShared
                | TargetProfile::JsWorker
        );
    }

    if matches!(
        module,
        "std.data.Json"
            | "std.data.Toml"
            | "std.encoding.Base64"
            | "std.regex.Regex"
            | "std.package.Registry"
    ) {
        return matches!(profile, TargetProfile::Vm);
    }

    if matches!(
        module,
        "std.crypto.Hash"
            | "std.crypto.Ed25519"
            | "std.io.Directory"
            | "std.io.Archive"
            | "std.io.Path"
            | "std.system.Arguments"
            | "std.system.Environment"
            | "std.system.Platform"
    ) {
        return matches!(profile, TargetProfile::Vm);
    }

    if module == "std.db.Postgres" {
        return matches!(profile, TargetProfile::Vm);
    }

    let _ = module;
    false
}

/// Returns whether a Rust-backed std module belongs to HTTP server packaging.
///
/// Inputs:
/// - `module`: fully qualified Rust-backed std module path.
///
/// Output:
/// - `true` for HTTP request/response/cookie modules that are valid in the
///   0.0.5 web package/server path.
///
/// Transformation:
/// - Separates web-server Rust support from still-gated Rust-backed APIs such
///   as Postgres, JSON, Base64, paths, and URI helpers.
fn is_http_rust_backed_std_module(module: &str) -> bool {
    matches!(
        module,
        "std.http.Request"
            | "std.http.Cookies"
            | "std.http.Response"
            | "std.http.Session"
            | "std.http.Sse"
            | "std.http.WebSocket"
    )
}

/// Returns whether a std module belongs to the native target family.
///
/// Inputs:
/// - `module`: fully qualified module path from CoreIR import metadata.
///
/// Output:
/// - `true` when the module is `std.native` or one of its descendants.
///
/// Transformation:
/// - Classifies native platform modules as target-specific std contracts so
///   VM and JS profiles cannot accidentally accept native-only imports.
fn is_native_std_module(module: &str) -> bool {
    module == "std.native" || module.starts_with("std.native.")
}

/// Returns whether a target profile can use native std modules.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `module`: fully qualified native std module path.
///
/// Output:
/// - `true` only when the selected profile owns a native std bridge.
///
/// Transformation:
/// - Keeps native std modules available to source/interface generation while
///   rejecting them from unsupported artifact paths. The current VM profile
///   admits `std.native.collections.Vector` through the NativeBoundary boundary
///   module instead of lowering it to VM data.
fn target_profile_supports_native_std_module(profile: TargetProfile, module: &str) -> bool {
    matches!(profile, TargetProfile::Vm) && matches!(module, "std.native.collections.Vector")
}

/// Returns whether a std module is explicitly tied to VM runtime semantics.
///
/// Inputs:
/// - `module`: fully qualified module path from CoreIR import metadata.
///
/// Output:
/// - `true` when the module belongs to the VM target-specific std family.
///
/// Transformation:
/// - Classifies VM runtime contracts at the module-family boundary so they
///   can be resolved as normal std interfaces while remaining unavailable to
///   portable CoreV0 and future non-VM backend profiles.
fn is_vm_std_module(module: &str) -> bool {
    module == "std.vm" || module.starts_with("std.vm.")
}

/// Returns whether a target profile can execute VM runtime std modules.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `module`: fully qualified VM std module path.
///
/// Output:
/// - `true` only for the current full VM/VM backend profile.
///
/// Transformation:
/// - Keeps VM process, supervision, and bridge contracts out of portable
///   profile subsets while allowing the active VM backend to typecheck and
///   validate VM-specific source modules.
fn target_profile_supports_vm_std_module(profile: TargetProfile, module: &str) -> bool {
    let _ = module;
    matches!(profile, TargetProfile::Vm)
}

/// Returns whether a std module belongs to the JavaScript target family.
///
/// Inputs:
/// - `module`: fully qualified module path from CoreIR import metadata.
///
/// Output:
/// - `true` when the module is `std.js` or one of its generated descendants.
///
/// Transformation:
/// - Classifies JavaScript platform bindings at the std-family boundary so
///   Terlan source can keep explicit imports while target-profile validation
///   prevents those imports from leaking into VM, native, or portable
///   profiles.
fn is_js_std_module(module: &str) -> bool {
    module == "std.js" || module.starts_with("std.js.")
}

/// Returns whether a target profile can use JavaScript std modules.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `module`: fully qualified JavaScript std module path.
///
/// Output:
/// - `true` for the initial JavaScript target-profile family.
///
/// Transformation:
/// - Keeps generated `std.js.*` bindings ordinary source imports while making
///   the selected backend profile responsible for admitting JavaScript-only
///   contracts before artifact emission.
fn target_profile_supports_js_std_module(profile: TargetProfile, module: &str) -> bool {
    if !profile.is_js() {
        return false;
    }
    match profile {
        TargetProfile::JsBrowser => true,
        TargetProfile::JsShared | TargetProfile::JsWorker => !is_js_browser_only_std_module(module),
        _ => false,
    }
}

/// Returns whether a JavaScript std module requires a browser profile.
///
/// Inputs:
/// - `module`: fully qualified JavaScript std module path.
///
/// Output:
/// - `true` for the browser/DOM seed namespace.
///
/// Transformation:
/// - Encodes the first coarse generated-binding profile rule before per-module
///   generated metadata exists, keeping `std.js.Dom.*` out of shared and worker
///   profiles while admitting it under `js.browser`.
fn is_js_browser_only_std_module(module: &str) -> bool {
    module == "std.js.Dom" || module.starts_with("std.js.Dom.")
}

/// Returns whether a std module belongs to the WebAssembly target family.
///
/// Inputs:
/// - `module`: fully qualified module path from CoreIR import metadata.
///
/// Output:
/// - `true` when the module is `std.wasm` or one of its descendants.
///
/// Transformation:
/// - Classifies WebAssembly ABI bindings at the std-family boundary so Wasm
///   target inference can stay type-driven while non-Wasm targets reject
///   ABI-only imports before artifact emission.
fn is_wasm_std_module(module: &str) -> bool {
    module == "std.wasm" || module.starts_with("std.wasm.")
}

/// Returns whether a target profile can use WebAssembly std modules.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `module`: fully qualified WebAssembly std module path.
///
/// Output:
/// - `true` only for the core WebAssembly target profile.
///
/// Transformation:
/// - Keeps `std.wasm.*` available as normal source imports without allowing
///   WebAssembly ABI types to leak into VM, JS, or portable profiles.
fn target_profile_supports_wasm_std_module(profile: TargetProfile, module: &str) -> bool {
    let _ = module;
    matches!(profile, TargetProfile::WasmCore)
}

/// Returns whether a remote call targets a proven std runtime contract.
///
/// Inputs:
/// - `module`: CoreIR remote-call module path or imported call head.
/// - `canonical_module`: fully-qualified std module identity.
/// - `call_heads`: module names proven by imports to refer to this std module.
///
/// Output:
/// - `true` only when the module is the canonical std module or one of the
///   proven imported call heads.
///
/// Transformation:
/// - Centralizes import-derived std runtime identity checks so target-profile
///   validation can grow new runtime std modules without duplicating alias
///   logic for every module family.
pub(super) fn is_std_runtime_module(
    module: &str,
    canonical_module: &str,
    call_heads: &HashSet<String>,
) -> bool {
    module == canonical_module || call_heads.contains(module)
}

/// Returns whether the current target profile owns a concrete Task operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.core.Task` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Encodes the currently admitted Task subset separately from the type-level
///   Task contract so future runtime operations must be deliberately promoted.
pub(in crate::validation::target_profile) fn target_profile_supports_task_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Vm | TargetProfile::CoreV0)
        && matches!(function, "done" | "result")
}

/// Returns whether CoreV0 may execute a std remote call through Terlan VM.
///
/// Inputs:
/// - `profile`: target profile under validation.
/// - `module`: CoreIR remote-call module path.
/// - `function`: CoreIR remote-call function name.
/// - `arity`: number of supplied arguments.
///
/// Output:
/// - `true` only for std remote calls implemented directly by the Terlan VM
///   evaluator and needed by VM-default stdlib release tests.
///
/// Transformation:
/// - Keeps the CoreV0 remote-call opening deliberately narrower than generic
///   runtime-boundary support. Arbitrary module calls still fail target-profile
///   validation.
pub(in crate::validation::target_profile) fn target_profile_supports_vm_std_remote_call(
    profile: TargetProfile,
    module: &str,
    function: &str,
    arity: usize,
) -> bool {
    if !matches!(profile, TargetProfile::CoreV0) {
        return false;
    }

    match module {
        "std.test.Test" | "Test" => matches!(
            (function, arity),
            ("assert", 1)
                | ("assert_true", 1)
                | ("assert_false", 1)
                | ("assert_equal", 2)
                | ("assert_not_equal", 2)
                | ("fail", 0)
        ),
        "std.core.Bool" | "Bool" => matches!(
            (function, arity),
            ("equal", 2)
                | ("is_true", 1)
                | ("is_false", 1)
                | ("compare", 2)
                | ("to_string", 1)
                | ("from_string", 1)
        ),
        "std.core.Int" | "Int" => matches!(
            (function, arity),
            ("equal", 2)
                | ("min", 2)
                | ("max", 2)
                | ("abs", 1)
                | ("compare", 2)
                | ("to_string", 1)
                | ("from_string", 1)
        ),
        "std.core.Float" | "Float" => matches!(
            (function, arity),
            ("equal", 2)
                | ("min", 2)
                | ("max", 2)
                | ("abs", 1)
                | ("compare", 2)
                | ("to_string", 1)
                | ("from_string", 1)
        ),
        "std.core.String" | "String" => matches!(
            (function, arity),
            ("equal", 2)
                | ("compare", 2)
                | ("to_string", 1)
                | ("from_string", 1)
                | ("is_empty", 1)
                | ("append", 2)
                | ("concat", 1)
                | ("contains", 2)
                | ("starts_with", 2)
                | ("ends_with", 2)
                | ("length", 1)
                | ("byte_size", 1)
                | ("lowercase", 1)
                | ("uppercase", 1)
                | ("characters", 1)
                | ("trim", 1)
                | ("trim_start", 1)
                | ("trim_end", 1)
                | ("replace", 3)
                | ("split", 2)
                | ("split_once", 2)
        ),
        "std.core.Option" | "Option" => matches!(
            (function, arity),
            ("is_some", 1) | ("is_none", 1) | ("with_default", 2)
        ),
        "std.core.Result" | "Result" => matches!(
            (function, arity),
            ("is_ok", 1) | ("is_err", 1) | ("with_default", 2)
        ),
        "std.core.Unit" | "Unit" => matches!(
            (function, arity),
            ("equal", 2) | ("compare", 2) | ("to_string", 1) | ("from_string", 1)
        ),
        "std.core.Ordering" | "Ordering" => matches!(
            (function, arity),
            ("compare", 2) | ("to_string", 1) | ("from_string", 1)
        ),
        "std.core.Atom" | "Atom" => {
            matches!((function, arity), ("equal", 2) | ("to_string", 1))
        }
        "std.core.Error" | "Error" => {
            matches!(
                (function, arity),
                ("new", 2) | ("code", 1) | ("message", 1) | ("to_string", 1)
            )
        }
        "List" | "std.collections.List" => matches!((function, arity), ("new", 0)),
        "Map" | "std.collections.Map" | "Object" | "std.core.Object" => {
            matches!((function, arity), ("new", 0) | ("from_entries", 1))
        }
        "Set" | "std.collections.Set" => {
            matches!((function, arity), ("new", 0) | ("from_list", 1))
        }
        "std.io.File" | "File" => {
            matches!(
                (function, arity),
                ("new", 3) | ("code", 1) | ("message", 1) | ("path", 1)
            )
        }
        "std.sync.Resource" | "Resource" => matches!(
            (function, arity),
            ("inserted", 1) | ("updated", 1) | ("deleted", 1) | ("kind", 1) | ("value", 1)
        ),
        "std.core.Task" | "Task" => matches!((function, arity), ("done" | "failed", 1)),
        "std.http.Error" => {
            matches!(
                (function, arity),
                ("new", 3) | ("code", 1) | ("message", 1) | ("status", 1)
            )
        }
        "std.http.Response" | "Response" => http_response::supports_operation(function, arity),
        "std.http.Session" | "Session" => matches!(
            (function, arity),
            ("current", 1)
                | ("get", 2)
                | ("set", 3)
                | ("delete", 2)
                | ("rotate", 1)
                | ("expire", 1)
                | ("with_response", 2)
        ),
        "std.http.Sse" | "Sse" => matches!(
            (function, arity),
            ("data", 1)
                | ("with_id", 2)
                | ("with_name", 2)
                | ("with_retry_ms", 2)
                | ("response", 1 | 2)
                | ("endpoint", 0..=2)
                | ("endpoint_with_keep_alive", 0..=3)
        ),
        "std.http.WebSocket" | "WebSocket" => matches!(
            (function, arity),
            ("text" | "ping" | "pong", 1) | ("close", 0) | ("endpoint", 0..=2)
        ),
        "std.http.Router" | "Router" => matches!(
            (function, arity),
            ("new", 0)
                | ("get", 3)
                | ("post", 3)
                | ("put", 3)
                | ("patch", 3)
                | ("delete", 3)
                | ("head", 3)
                | ("options", 3)
                | ("sse", 3)
                | ("websocket", 3)
                | ("use", 2)
                | ("map_response", 2)
                | ("fallback", 2)
                | ("error", 2)
                | ("overload", 3)
                | ("lifecycle", 2)
                | ("group", 3)
        ),
        "__receiver__" => target_profile_supports_vm_receiver_call(profile, function, arity),
        _ => false,
    }
}

/// Returns whether CoreV0 may execute or validate a receiver-style std call.
///
/// Inputs:
/// - `profile`: target profile under validation.
/// - `function`: receiver method name after CoreIR lowering.
/// - `arity`: total CoreIR argument count including the receiver value.
///
/// Output:
/// - `true` for the receiver-method subset already represented by the VM
///   evaluator or needed by std declaration tests.
///
/// Transformation:
/// - Keeps receiver calls narrower than a blanket CoreV0 opening while
///   allowing std collections, strings, Task, HTTP, and Postgres row surfaces
///   to move through the VM-default stdlib release gate.
pub(in crate::validation::target_profile) fn target_profile_supports_vm_receiver_call(
    profile: TargetProfile,
    function: &str,
    arity: usize,
) -> bool {
    matches!(profile, TargetProfile::CoreV0)
        && matches!(
            (function, arity),
            ("is_empty", 1)
                | ("length", 1)
                | ("byte_size", 1)
                | ("size", 1)
                | ("first", 1)
                | ("iterator", 1)
                | ("get", 2)
                | ("contains_key", 2)
                | ("contains", 2)
                | ("starts_with", 2)
                | ("ends_with", 2)
                | ("append", 2)
                | ("lowercase", 1)
                | ("uppercase", 1)
                | ("characters", 1)
                | ("trim", 1)
                | ("trim_start", 1)
                | ("trim_end", 1)
                | ("replace", 3)
                | ("split", 2)
                | ("split_once", 2)
                | ("to_string", 1)
                | ("each", 2)
                | ("result", 1)
                | ("code", 1)
                | ("message", 1)
                | ("method", 1)
                | ("path", 1)
                | ("param", 2)
                | ("query", 2)
                | ("query_string", 1)
                | ("header", 2 | 3)
                | ("cookie", 2..=6)
                | ("cookies", 1)
                | ("body_text", 1)
                | ("body_json", 1)
                | ("status", 2)
                | ("with_status", 2)
                | ("with_header", 3)
                | ("set_cookie_header", 2)
                | ("cookie_with_options", 3..=11)
                | ("with_cookie_options", 3..=11)
                | ("delete_cookie", 2 | 3)
                | ("with_deleted_cookie", 2 | 3)
                | ("int", 2)
                | ("bool", 2)
                | ("json", 2)
                | ("string", 2)
        )
}

/// Returns whether CoreV0 may execute a mutable std receiver call.
///
/// Inputs:
/// - `profile`: target profile under validation.
/// - `method`: mutable receiver method name.
/// - `arity`: non-receiver argument count.
///
/// Output:
/// - `true` for collection mutators implemented by the VM evaluator and HTTP
///   response mutator declarations that should validate under the VM lane.
///
/// Transformation:
/// - Preserves the default rejection for arbitrary mutable receiver calls while
///   admitting the std release-test surface.
pub(in crate::validation::target_profile) fn target_profile_supports_vm_mutable_receiver_call(
    profile: TargetProfile,
    method: &str,
    arity: usize,
) -> bool {
    matches!(profile, TargetProfile::CoreV0)
        && matches!(
            (method, arity),
            ("push", 1)
                | ("clear", 0)
                | ("put", 2)
                | ("remove", 1)
                | ("add", 1)
                | ("status", 1)
                | ("header", 2)
                | ("with_status", 1)
                | ("with_header", 2)
                | ("set_cookie_header", 1)
                | ("cookie", 2..=5)
                | ("cookie_with_options", 2..=10)
                | ("with_cookie", 2..=5)
                | ("with_cookie_options", 2..=10)
                | ("delete_cookie", 1 | 2)
                | ("with_deleted_cookie", 1 | 2)
        )
}
