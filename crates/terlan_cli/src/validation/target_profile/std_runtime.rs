use std::collections::HashSet;

use terlan_typeck::{CoreExpr, CoreExprSummary, CoreImportKind, CoreModule};

use super::{TargetProfile, TargetProfileCheckOptions, TargetProfileViolation};

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
pub(super) struct StdCallHeads {
    pub(super) task: HashSet<String>,
    pub(super) beam_agent: HashSet<String>,
    pub(super) beam_gen_server: HashSet<String>,
    pub(super) beam_native_bridge: HashSet<String>,
    pub(super) beam_supervisor: HashSet<String>,
    pub(super) beam_task: HashSet<String>,
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
pub(super) fn std_call_heads(module: &CoreModule) -> StdCallHeads {
    StdCallHeads {
        task: std_module_call_heads(module, "std.core.Task", "Task"),
        beam_agent: std_module_call_heads(module, "std.beam.Agent", "Agent"),
        beam_gen_server: std_module_call_heads(module, "std.beam.GenServer", "GenServer"),
        beam_native_bridge: std_module_call_heads(module, "std.beam.NativeBridge", "NativeBridge"),
        beam_supervisor: std_module_call_heads(module, "std.beam.Supervisor", "Supervisor"),
        beam_task: std_module_call_heads(module, "std.beam.Task", "Task"),
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
///   table because backend-specific contracts, such as BEAM process types,
///   are still non-portable even when mentioned only in signatures.
pub(super) fn validate_core_imports(
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
                if !options.allow_asset_imports {
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
pub(super) fn std_module_import_violation(
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
    } else if is_beam_std_module(import_module)
        && !target_profile_supports_beam_std_module(profile, import_module)
    {
        Some(TargetProfileViolation::unsupported(
            "BEAM std module",
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
///   implementation depends on the Rust/SafeNative bridge.
///
/// Transformation:
/// - Centralizes the small Rust-backed std allowlist so these modules can be
///   visible to docs and summary generation while unsupported target profiles
///   reject them before backend emission.
fn is_rust_backed_std_module(module: &str) -> bool {
    matches!(
        module,
        "std.data.Json"
            | "std.encoding.Base64"
            | "std.io.Path"
            | "std.net.Uri"
            | "std.db.Postgres"
            | "std.http.Request"
            | "std.http.Cookies"
            | "std.http.Response"
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
///   module's Rust/SafeNative implementation or the command owns SafeNative
///   packaging for Erlang output.
///
/// Transformation:
/// - Keeps generic/pure validation conservative while allowing normal
///   BEAM build/test paths to compile portable Rust-backed std APIs through
///   the SafeNative bridge.
fn target_profile_supports_rust_backed_std_module_with_options(
    profile: TargetProfile,
    options: TargetProfileCheckOptions,
    module: &str,
) -> bool {
    if options.allow_rust_backed_std_modules && matches!(profile, TargetProfile::Erlang) {
        let _ = module;
        return true;
    }

    if is_http_rust_backed_std_module(module) {
        return matches!(
            profile,
            TargetProfile::Erlang
                | TargetProfile::JsBrowser
                | TargetProfile::JsShared
                | TargetProfile::JsWorker
        );
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
        "std.http.Request" | "std.http.Cookies" | "std.http.Response"
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
///   BEAM and JS profiles cannot accidentally accept native-only imports.
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
///   rejecting them from unsupported artifact paths. The current BEAM profile
///   admits `std.native.collections.Vector` through the SafeNative boundary
///   module instead of lowering it to BEAM data.
fn target_profile_supports_native_std_module(profile: TargetProfile, module: &str) -> bool {
    matches!(profile, TargetProfile::Erlang) && matches!(module, "std.native.collections.Vector")
}

/// Returns whether a std module is explicitly tied to BEAM runtime semantics.
///
/// Inputs:
/// - `module`: fully qualified module path from CoreIR import metadata.
///
/// Output:
/// - `true` when the module belongs to the BEAM target-specific std family.
///
/// Transformation:
/// - Classifies BEAM runtime contracts at the module-family boundary so they
///   can be resolved as normal std interfaces while remaining unavailable to
///   portable CoreV0 and future non-BEAM backend profiles.
fn is_beam_std_module(module: &str) -> bool {
    module == "std.beam" || module.starts_with("std.beam.")
}

/// Returns whether a target profile can execute BEAM runtime std modules.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `module`: fully qualified BEAM std module path.
///
/// Output:
/// - `true` only for the current full Erlang/BEAM backend profile.
///
/// Transformation:
/// - Keeps BEAM process, supervision, and bridge contracts out of portable
///   profile subsets while allowing the active Erlang backend to typecheck and
///   validate BEAM-specific source modules.
fn target_profile_supports_beam_std_module(profile: TargetProfile, module: &str) -> bool {
    let _ = module;
    matches!(profile, TargetProfile::Erlang)
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
///   prevents those imports from leaking into BEAM, native, or portable
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
fn is_std_runtime_module(
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
pub(super) fn target_profile_supports_task_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Erlang) && matches!(function, "done" | "result")
}

/// Returns whether the current target profile owns a concrete Agent operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.beam.Agent` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Keeps the Agent type contract available while forcing executable runtime
///   operations to wait for an explicit BEAM backend implementation.
pub(super) fn target_profile_supports_beam_agent_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Erlang)
        && matches!(
            function,
            "start" | "get" | "get_and_update" | "update" | "cast" | "stop"
        )
}

/// Returns whether the current target profile owns a concrete GenServer operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.beam.GenServer` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Admits only the first BEAM callback-process operations implemented
///   through the shared BEAM process intrinsic layer.
pub(super) fn target_profile_supports_beam_gen_server_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Erlang)
        && matches!(function, "start" | "call" | "cast" | "stop")
}

/// Returns whether the current target profile owns a concrete NativeBridge operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.beam.NativeBridge` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Keeps the NativeBridge callable contract visible to source and summaries
///   while admitting only Erlang-profile operations that have compiler-owned
///   bridge proof lowering.
pub(super) fn target_profile_supports_beam_native_bridge_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Erlang)
        && matches!(function, "start" | "call" | "dispose" | "stop")
}

/// Returns whether the current target profile owns a concrete Supervisor operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.beam.Supervisor` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Keeps the Supervisor callable contract visible to source and summaries
///   while admitting only the Erlang profile operations that have local
///   compiler-owned lowering.
pub(super) fn target_profile_supports_beam_supervisor_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Erlang) && matches!(function, "child_spec" | "start" | "stop")
}

/// Returns whether the current target profile owns a concrete BEAM Task operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.beam.Task` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Keeps the BEAM Task type contract available while rejecting executable
///   task process operations until they are implemented through the shared
///   BEAM process intrinsic layer.
pub(super) fn target_profile_supports_beam_task_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Erlang) && matches!(function, "start" | "result" | "cancel")
}

/// Appends a target-profile violation for one std runtime operation family.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
/// - `function_scope`: enclosing function/clause label.
/// - `location`: expression location relative to the function scope.
/// - `module`: remote-call module identity.
/// - `function`: remote-call function identity.
/// - `call_heads`: module names proven to refer to the std runtime module.
/// - `diagnostic_label`: stable feature label used in diagnostics.
/// - `canonical_module`: fully-qualified std module identity used in messages.
/// - `supports_operation`: policy function for profile/function support.
/// - `violations`: mutable output collection for profile violations.
///
/// Output:
/// - No direct return value; a violation is appended for matching runtime calls
///   that do not have backend support.
///
/// Transformation:
/// - Converts concrete runtime API calls into stable unsupported-target
///   diagnostics while leaving the type-level std contracts available for
///   parsing, resolution, and typechecking. This is the shared target-profile
///   validation path for Task, Agent, and future BEAM process abstractions.
pub(super) fn validate_std_runtime_operation_support(
    profile: TargetProfile,
    function_scope: &str,
    location: &str,
    module: &str,
    function: &str,
    call_heads: &HashSet<String>,
    diagnostic_label: &str,
    canonical_module: &str,
    supports_operation: fn(TargetProfile, &str) -> bool,
    violations: &mut Vec<TargetProfileViolation>,
) {
    if is_std_runtime_module(module, canonical_module, call_heads)
        && !supports_operation(profile, function)
    {
        violations.push(TargetProfileViolation::unsupported(
            diagnostic_label,
            profile,
            &format!("{function_scope} {location}"),
            &format!("{canonical_module}.{function}"),
        ));
    }
}

/// Appends a target-profile violation for summary-only std runtime operations.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
/// - `function_scope`: enclosing function/clause label.
/// - `location`: expression location relative to the function scope.
/// - `summary`: expression summary that may describe a remote std runtime call.
/// - `call_heads`: module names proven to refer to the std runtime module.
/// - `diagnostic_label`: stable feature label used in diagnostics.
/// - `canonical_module`: fully-qualified std module identity used in messages.
/// - `supports_operation`: policy function for profile/function support.
/// - `violations`: mutable output collection for profile violations.
///
/// Output:
/// - No direct return value; a violation is appended for matching remote call
///   summaries without backend support.
///
/// Transformation:
/// - Reads the summary's remote module and first child callee text to detect
///   runtime operations before full typed Core payload lowering exists for that
///   call family. This keeps summary-only and typed payload validation aligned.
pub(super) fn validate_std_runtime_operation_summary_support(
    profile: TargetProfile,
    function_scope: &str,
    location: &str,
    summary: &CoreExprSummary,
    call_heads: &HashSet<String>,
    diagnostic_label: &str,
    canonical_module: &str,
    supports_operation: fn(TargetProfile, &str) -> bool,
    violations: &mut Vec<TargetProfileViolation>,
) {
    if matches!(
        summary.core_expr.as_ref(),
        Some(CoreExpr::RemoteCall { .. } | CoreExpr::Intrinsic(_))
    ) {
        return;
    }

    let Some(module) = summary.remote.as_deref() else {
        return;
    };
    if !is_std_runtime_module(module, canonical_module, call_heads) {
        return;
    }

    let function = summary
        .children
        .first()
        .and_then(|child| child.text.as_deref())
        .unwrap_or("<unknown>");
    if supports_operation(profile, function) {
        return;
    }
    violations.push(TargetProfileViolation::unsupported(
        diagnostic_label,
        profile,
        &format!("{function_scope} {location}"),
        &format!("{canonical_module}.{function}"),
    ));
}
