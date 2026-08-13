use super::*;

/// Verifies that direct Oxc AST lowering handles string-prefix intrinsics.
///
/// Inputs:
/// - A checked Terlan module whose public function calls
///   `"hello".starts_with("he")`.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies receiver-method
///   syntax lowers to the backend-neutral `core.string.starts_with`
///   intrinsic, and checks direct Oxc lowering emits JavaScript
///   `.startsWith(...)`.
#[test]
pub(super) fn emit_core_module_with_direct_oxc_ast_handles_string_starts_with_intrinsic() {
    let source = "\
module js_core_direct_string_starts_with_intrinsic.

pub has_prefix(): Bool ->
    \"hello\".starts_with(\"he\").
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_string_starts_with_intrinsic.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits string starts_with intrinsic CoreIR");

    assert!(js.contains("export function has_prefix()"));
    assert!(js.contains(r#"return "hello".startsWith("he");"#), "{js}");
}

/// Verifies that direct Oxc AST lowering handles text-length intrinsics.
///
/// Inputs:
/// - A checked Terlan module whose public function calls
///   `"hello".length()`.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies receiver-method
///   syntax lowers to the backend-neutral `core.string.length` intrinsic,
///   and checks direct Oxc lowering emits `Array.from(value).length` rather
///   than JavaScript UTF-16 code-unit `.length`.
#[test]
pub(super) fn emit_core_module_with_direct_oxc_ast_handles_string_length_intrinsic() {
    let source = "\
module js_core_direct_string_length_intrinsic.

pub len(): Int ->
    \"hello\".length().
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_string_length_intrinsic.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits string length intrinsic CoreIR");

    assert!(js.contains("export function len()"));
    assert!(js.contains(r#"return Array.from("hello").length;"#), "{js}");
}

/// Verifies that direct Oxc AST lowering handles focused pipe-forward
/// expressions into local named calls.
///
/// Inputs:
/// - A checked Terlan module with a private binary function and a public
///   function using `1 |> add_one()`.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that `CoreExpr::BinaryOp`
///   with `|>` becomes a local JavaScript call with the piped expression as
///   the first argument.
#[test]
pub(super) fn emit_core_module_with_direct_oxc_ast_handles_pipe_forward_to_named_call() {
    let source = "\
module js_core_direct_pipe_forward.

add_one(x: Int): Int ->
    x + 1.

pub piped(): Int ->
    1 |> add_one().
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_pipe_forward.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits pipe-forward CoreIR");

    assert!(js.contains("export function piped()"));
    assert!(js.contains("function add_one(x)"));
    assert!(js.contains("return add_one(1);"), "{js}");
}

/// Verifies that the command-facing Oxc facade emits direct named-call
/// modules with private helpers.
///
/// Inputs:
/// - A checked Terlan module with a private local function and a public
///   caller.
///
/// Output:
/// - Assertions over facade-emitted JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, calls the same Oxc facade
///   used by `emit-js`, and checks that private helpers are emitted locally
///   while only public functions are exported.
#[test]
pub(super) fn emit_core_module_with_oxc_codegen_emits_named_call_private_helper() {
    let source = "\
module js_core_facade_named_call.

identity(x: Int): Int ->
    x.

pub call_it(): Int ->
    identity(1).
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_facade_named_call.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("facade emits direct named-call CoreIR");

    assert!(js.contains("function identity(x)"), "{js}");
    assert!(!js.contains("export function identity"), "{js}");
    assert!(js.contains("export function call_it()"), "{js}");
    assert!(js.contains("return identity(1);"), "{js}");
}

/// Verifies that direct Oxc AST lowering ignores unreachable private
/// functions.
///
/// Inputs:
/// - A checked Terlan module with a supported public function and an
///   unsupported but unused private function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, calls the direct Oxc AST
///   emitter, and checks that only the public reachability surface is
///   emitted while the unused unsupported private helper is ignored.
#[test]
pub(super) fn emit_core_module_with_direct_oxc_ast_ignores_unreachable_private_function() {
    let source = "\
module js_core_direct_reachable.

unused(status: Atom): Atom ->
    case status {
        value -> value
    }.

pub answer(): Int ->
    42.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_reachable.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST ignores unreachable unsupported private helper");

    assert!(js.contains("export function answer()"), "{js}");
    assert!(js.contains("return 42;"), "{js}");
    assert!(!js.contains("unused"), "{js}");
    assert!(!js.contains("Terlan JS backend stub"), "{js}");
}

/// Verifies that the command-facing Oxc facade uses direct reachability
/// filtering.
///
/// Inputs:
/// - A checked Terlan module with a supported public function and an
///   unsupported but unused private function.
///
/// Output:
/// - Assertions over facade-emitted JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, calls the same Oxc facade
///   used by `emit-js`, and checks that unreachable unsupported private code
///   does not trigger bootstrap stub fallback.
#[test]
pub(super) fn emit_core_module_with_oxc_codegen_uses_direct_reachability_filter() {
    let source = "\
module js_core_facade_reachable.

unused(status: Atom): Atom ->
    case status {
        value -> value
    }.

pub answer(): Int ->
    42.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_facade_reachable.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("facade emits through direct reachability filter");

    assert!(js.contains("export function answer()"), "{js}");
    assert!(js.contains("return 42;"), "{js}");
    assert!(!js.contains("unused"), "{js}");
    assert!(!js.contains("Terlan JS backend stub"), "{js}");
}

/// Verifies that binding-pattern case expressions stay outside the direct
/// JS backend subset until pattern-dispatch semantics are represented.
///
/// Inputs:
/// - A checked Terlan module whose public function body contains a case
///   expression with a binding pattern.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines the binding-pattern `CoreExpr::Case`, then
///   checks the command-facing facade preserves the JS stub fallback.
#[test]
pub(super) fn emit_core_module_with_oxc_codegen_falls_back_for_binding_case_expr() {
    let source = "\
module js_core_binding_case_fallback.

pub choose(status: Atom): Atom ->
    case status {
        value -> value
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_binding_case_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function choose(status)"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

#[cfg(test)]
#[path = "declarations_test.rs"]
#[cfg(test)]
mod declarations_test;
