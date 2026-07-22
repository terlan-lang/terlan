use super::{core_lowering, oxc_backend};
use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

/// Verifies direct JS lowering consumes a completed GuardResult filter.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_completed_guard_result_comprehension() {
    let artifacts = completed_guard_result_artifacts();

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits completed GuardResult comprehension");

    assert!(js.contains("export function positives(items)"));
    assert!(js.contains("__terlan_comprehension_guard_result[1] === true"));
    assert!(js.contains("[\"guard_result\", value > 0]"), "{js}");
}

/// Verifies fallback JS lowering uses the same completed-result decoder.
#[test]
fn emit_core_module_to_js_handles_completed_guard_result_comprehension() {
    let artifacts = completed_guard_result_artifacts();
    let js = core_lowering::emit_core_module_to_js(&artifacts.core);

    assert!(js.contains("__terlan_comprehension_guard_result === true"));
    assert!(js.contains("__terlan_comprehension_guard_result[0] === \"guard_result\""));
    assert!(js.contains("[\"guard_result\", (value > 0)]"), "{js}");
}

/// Verifies a non-special-cased `GuardResult` lift rejects unsupported JS.
#[test]
fn effectful_guard_result_comprehension_reports_unsupported_js_lowering() {
    let source = r#"
module js_effectful_guard_result.

pub type Deferred[T] = {Atom["deferred"], value: T}.
pub trait GuardResult[R, F[_]] { into_guard(result: R): F[Bool]. }.
pub impl GuardResult[Deferred[Bool], Deferred] for Deferred[Bool] {
    into_guard(result: Deferred[Bool]): Deferred[Bool] -> result.
}.
defer(value: Bool): Deferred[Bool] -> Deferred(value).
pub positives(items: List[Int]): Deferred[List[Int]] ->
    [value | value <- items, defer(value > 0)].
"#;
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_effectful_guard_result.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile trait-declared lifted comprehension");

    assert!(
        oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none(),
        "effectful comprehension must report the stable JS unsupported path"
    );
}

fn completed_guard_result_artifacts() -> crate::formal_pipeline::CheckedSyntaxModuleArtifacts {
    let source = r#"
module js_core_direct_completed_guard_result.

pub positives(items: List[Int]): List[Int] ->
    [value | value <- items, {Atom["guard_result"], value > 0}].
"#;
    compile_syntax_module_through_phases_with_profile(
        "js_core_direct_completed_guard_result.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile completed GuardResult comprehension to CoreIR")
}
