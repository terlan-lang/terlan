use super::*;

use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

/// Compiles a Terlan source module and emits direct-Oxc JavaScript.
///
/// Inputs:
/// - `source`: Terlan source text expected to typecheck and fit the direct JS
///   backend subset.
///
/// Output:
/// - JavaScript source printed by Oxc.
///
/// Transformation:
/// - Runs the formal syntax/type/CoreIR pipeline, then emits JavaScript through
///   the release-owned direct CoreIR-to-Oxc-AST backend.
fn compile_and_emit_direct_js(source: &str) -> String {
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_std_core_string_intrinsics.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits selected std.core.String intrinsics")
}

/// Verifies direct JavaScript lowering for selected `std.core.String`
/// intrinsics.
///
/// Inputs:
/// - A checked Terlan module that calls portable `std.core.String` receiver
///   methods and one module-level concat operation.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Proves the JS backend maps backend-neutral CoreIR intrinsic ids to
///   JavaScript string operations without changing the public `std.core.String`
///   source API.
#[test]
fn emits_selected_std_core_string_intrinsics_as_direct_js_operations() {
    let source = "\
module js_std_core_string_intrinsics.

pub has_suffix(): Bool ->
    \"hello\".ends_with(\"lo\").

pub clean(): String ->
    \"  hello  \".trim().

pub clean_start(): String ->
    \"  hello\".trim_start().

pub clean_end(): String ->
    \"hello  \".trim_end().

pub shout(): String ->
    \"hello\".uppercase().

pub whisper(): String ->
    \"HELLO\".lowercase().

pub append_pair(): String ->
    \"a\".append(\"b\").

pub concat_pair(): String ->
    std.core.String.concat([\"a\", \"b\"]).
";

    let js = compile_and_emit_direct_js(source);

    assert!(js.contains(r#"return "hello".endsWith("lo");"#), "{js}");
    assert!(js.contains(r#"return "  hello  ".trim();"#), "{js}");
    assert!(js.contains(r#"return "  hello".trimStart();"#), "{js}");
    assert!(js.contains(r#"return "hello  ".trimEnd();"#), "{js}");
    assert!(js.contains(r#"return "hello".toUpperCase();"#), "{js}");
    assert!(js.contains(r#"return "HELLO".toLowerCase();"#), "{js}");
    assert!(js.contains(r#"return "a" + "b";"#), "{js}");
    assert!(js.contains(r#"return ["a", "b"].join("");"#), "{js}");
}

/// Verifies direct JavaScript lowering for `std.core.String.is_empty`.
///
/// Inputs:
/// - A checked Terlan module that calls the portable string emptiness
///   predicate.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Proves `core.string.is_empty` lowers to a strict empty-string comparison
///   rather than a backend helper call.
#[test]
fn emits_std_core_string_is_empty_as_strict_empty_string_check() {
    let source = "\
module js_std_core_string_is_empty.

pub empty(): Bool ->
    \"\".is_empty().
";

    let js = compile_and_emit_direct_js(source);

    assert!(js.contains(r#"return "" === "";"#), "{js}");
}
