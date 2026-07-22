use super::*;

use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

/// Verifies that the JS backend can parse and reprint generated source
/// through Oxc codegen.
///
/// Inputs:
/// - A minimal JavaScript module shaped like `emit-js` output.
///
/// Output:
/// - Assertion over the Oxc-printed JavaScript source.
///
/// Transformation:
/// - Sends JS source through the backend Oxc parser/codegen adapter and
///   checks that the exported function survives the round trip.
#[test]
fn emit_js_with_oxc_codegen_reprints_module_source() {
    let js = "export function add(A, B) {\n  return (A + B);\n}\n";

    let emitted = oxc_backend::emit_js_with_oxc_codegen(js).expect("Oxc codegen emits JS");

    assert!(emitted.contains("export function add(A, B)"));
    assert!(emitted.contains("return A + B;") || emitted.contains("return (A + B);"));
}

/// Verifies that the command-facing Oxc backend facade emits JavaScript
/// directly from a checked CoreIR module.
///
/// Inputs:
/// - A checked Terlan module containing one public arithmetic function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, calls the Oxc backend
///   facade with the resulting `CoreModule`, and checks the public export
///   and lowered return expression survive Oxc codegen.
#[test]
fn emit_core_module_with_oxc_codegen_emits_core_surface() {
    let source = "\
module js_core_oxc_facade.

pub add(A: Int, B: Int): Int ->
    A + B.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_oxc_facade.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("emit CoreIR through Oxc backend facade");

    assert!(js.contains("export function add(A, B)"));
    assert!(js.contains("return A + B;") || js.contains("return (A + B);"));
}

/// Verifies that partial if expressions stay outside the direct JS backend
/// subset until no-match runtime semantics are represented.
///
/// Inputs:
/// - A checked Terlan module with a public function containing an if
///   expression without a final `true` fallback clause.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines the partial `CoreExpr::If`, then checks the
///   command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_partial_if_expr() {
    let source = "\
module js_core_partial_if_fallback.

pub choose(flag: Bool): Int ->
    if { flag -> 1 }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_partial_if_fallback.terl",
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

    assert!(js.contains("export function choose(flag)"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that direct Oxc AST lowering handles total literal case expressions.
///
/// Inputs:
/// - A checked Terlan module with a public function containing a case
///   expression over a variable scrutinee, one raw atom pattern, and a final
///   wildcard fallback.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that the selected
///   literal-pattern CoreIR case subset becomes a JavaScript conditional
///   expression.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_literal_case_expr() {
    let source = "\
module js_core_direct_literal_case.

pub choose(status: Atom): Int ->
    case status {
        Atom[\"none\"] -> 0;
        _ -> 1
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_literal_case.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits literal case CoreIR");

    assert!(js.contains("export function choose(status)"));
    assert!(js.contains("return status === \"none\" ? 0 : 1;"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles total integer literal
/// case expressions.
///
/// Inputs:
/// - A checked Terlan module with a public function matching an integer
///   scrutinee against an integer literal and a wildcard fallback.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that `CorePattern::Int`
///   becomes a JavaScript strict-equality test.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_integer_literal_case_expr() {
    let source = "\
module js_core_direct_integer_case.

pub classify(value: Int): Int ->
    case value {
        0 -> 1;
        _ -> 2
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_integer_case.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits integer literal case CoreIR");

    assert!(js.contains("export function classify(value)"));
    assert!(js.contains("return value === 0 ? 1 : 2;"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles total float literal case
/// expressions.
///
/// Inputs:
/// - A checked Terlan module with a public function matching a float
///   scrutinee against a finite float literal and a wildcard fallback.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that `CorePattern::Float`
///   becomes a JavaScript strict-equality test.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_float_literal_case_expr() {
    let source = "\
module js_core_direct_float_case.

pub classify(value: Float): Int ->
    case value {
        1.5 -> 1;
        _ -> 2
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_float_case.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits float literal case CoreIR");

    assert!(js.contains("export function classify(value)"));
    assert!(js.contains("return value === 1.5 ? 1 : 2;"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles total string literal case
/// expressions.
///
/// Inputs:
/// - A checked Terlan module with a public function matching a string
///   scrutinee against a string literal and a wildcard fallback.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that `CorePattern::String`
///   becomes a JavaScript strict-equality test.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_string_literal_case_expr() {
    let source = "\
module js_core_direct_string_case.

pub classify(value: String): Int ->
    case value {
        \"terlan\" -> 1;
        _ -> 2
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_string_case.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits string literal case CoreIR");

    assert!(js.contains("export function classify(value)"));
    assert!(js.contains("return value === \"terlan\" ? 1 : 2;"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles boolean literal case
/// expressions through atom artifact matching.
///
/// Inputs:
/// - A checked Terlan module with a public function matching a boolean
///   scrutinee against the raw atom pattern `:true` and a wildcard
///   fallback.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that the boolean
///   `:true` `CorePattern::Atom` artifact becomes JavaScript strict
///   equality against the JavaScript boolean `true`.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_bool_literal_case_expr() {
    let source = "\
module js_core_direct_bool_case.

pub classify(flag: Bool): Int ->
    case flag {
        Atom[\"true\"] -> 1;
        _ -> 0
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_bool_case.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits bool literal case CoreIR");

    assert!(js.contains("export function classify(flag)"));
    assert!(js.contains("return flag === true ? 1 : 0;"), "{js}");
}

/// Verifies that CoreIR case expressions stay outside the direct JS backend
/// subset until branch emission is represented.
///
/// Inputs:
/// - A checked Terlan module with a public function containing an exhaustive
///   case expression over a custom atom union.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines `CoreExpr::Case`, then checks the command-facing
///   facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_partial_case_expr() {
    let source = "\
module js_core_partial_case_fallback.

pub type Status =
      Atom[\"none\"]
    | Atom[\"other\"].

pub choose(status: Status): Int ->
    case status {
        Atom[\"none\"] -> 0;
        Atom[\"other\"] -> 1
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_partial_case_fallback.terl",
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

/// Verifies that the direct JS backend lowers guarded literal case expressions.
///
/// Inputs:
/// - A checked Terlan module with a public function containing a case
///   expression whose literal-pattern branch has a boolean guard.
///
/// Output:
/// - Assertions over direct-AST and facade JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline and verifies both Oxc entry
///   points preserve guarded case dispatch without a runtime stub.
#[test]
fn emit_core_module_with_oxc_codegen_handles_guarded_case_expr() {
    let source = "\
module js_core_guarded_case_fallback.

pub choose(status: Atom, flag: Bool): Int ->
    case status {
        Atom[\"none\"] where flag -> 0;
        _ -> 1
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_guarded_case_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let direct = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc backend lowers guarded case");
    assert!(direct.contains("export function choose(status, flag)"));

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("Oxc backend emits guarded case JS");

    assert!(js.contains("export function choose(status, flag)"));
    assert!(!js.contains("Terlan JS backend stub"), "{js}");
}

/// Verifies that the direct JS backend lowers destructuring case patterns.
///
/// Inputs:
/// - A checked Terlan module with a public function containing a case
///   expression whose first branch uses a tuple destructuring pattern.
///
/// Output:
/// - Assertions over direct-AST and facade JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline and verifies both Oxc entry
///   points preserve destructuring case dispatch without a runtime stub.
#[test]
fn emit_core_module_with_oxc_codegen_handles_destructuring_case_expr() {
    let source = "\
module js_core_destructuring_case_fallback.

pub first(value: Dynamic): Dynamic ->
    case value {
        {left, _} -> left;
        _ -> 0
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_destructuring_case_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let direct = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc backend lowers destructuring case");
    assert!(direct.contains("export function first(value)"));

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("Oxc backend emits destructuring case JS");

    assert!(js.contains("export function first(value)"));
    assert!(!js.contains("Terlan JS backend stub"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles anonymous function values.
///
/// Inputs:
/// - A checked Terlan module with a public function returning a single
///   anonymous function whose parameter is a direct variable binding.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that `CoreExpr::Lam`
///   becomes a JavaScript arrow-function value.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_lambda_value() {
    let source = "\
module js_core_direct_lambda.

pub id_fun(): Term ->
    (x) -> x.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_lambda.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits lambda CoreIR");

    assert!(js.contains("export function id_fun()"));
    assert!(js.contains("return (x) => x;"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles simple list comprehensions.
///
/// Inputs:
/// - A checked Terlan module with a public function returning a
///   single-generator, variable-pattern, unguarded list comprehension.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that the selected
///   `CoreExpr::ListComprehension` subset becomes a JavaScript `.map(...)`
///   call.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_simple_list_comprehension() {
    let source = "\
module js_core_direct_list_comprehension.

pub values(items: List[Int]): List[Int] ->
    [value | value <- items].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_list_comprehension.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits list comprehension CoreIR");

    assert!(js.contains("export function values(items)"));
    assert!(js.contains("return items.map((value) => value);"), "{js}");
}

/// Verifies direct Oxc lowering preserves ordered generator environments.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_ordered_list_comprehension_generators() {
    let source = "\
module js_core_direct_ordered_list_comprehension.

pub flatten(rows: List[List[Int]]): List[Int] ->
    [value | row <- rows, value <- row].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_ordered_list_comprehension.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile ordered comprehension to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits ordered comprehension");

    assert!(js.contains("export function flatten(rows)"));
    assert!(
        js.contains("return rows.flatMap((row) => row.map((value) => value));"),
        "{js}"
    );
}

/// Verifies direct Oxc AST lowering handles tuple-destructuring
/// list-comprehension generators.
///
/// Inputs:
/// - A checked Terlan module with a public function returning a
///   single-generator list comprehension whose generator pattern
///   destructures tuple elements.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that tuple generator patterns
///   lower to JavaScript array destructuring.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_destructuring_list_comprehension() {
    let source = "\
module js_core_direct_list_comprehension_destructuring.

pub firsts(items: List[{Int, Int}]): List[Int] ->
    [left | {left, _right} <- items].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_list_comprehension_destructuring.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits destructuring list comprehension CoreIR");

    assert!(js.contains("export function firsts(items)"));
    assert!(
        js.contains("Array.isArray(__terlan_comprehension_candidate)"),
        "{js}"
    );
    assert!(
        js.contains("__terlan_comprehension_candidate.length === 2"),
        "{js}"
    );
    assert!(js.contains(".map(([left, _right]) => left);"), "{js}");
}

/// Verifies direct Oxc AST lowering handles guarded list comprehensions.
///
/// Inputs:
/// - A checked Terlan module with a public function returning a guarded
///   single-generator list comprehension.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that the selected guarded
///   `CoreExpr::ListComprehension` subset becomes a JavaScript
///   `.filter(...).map(...)` pipeline.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_guarded_list_comprehension() {
    let source = "\
module js_core_direct_list_comprehension_guard.

pub positives(items: List[Int]): List[Int] ->
    [value | value <- items, value > 0].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_list_comprehension_guard.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits guarded list comprehension CoreIR");

    assert!(js.contains("export function positives(items)"));
    assert!(
        js.contains("__terlan_comprehension_guard_result === true"),
        "{js}"
    );
    assert!(
        js.contains("__terlan_comprehension_guard_result[0] === \"guard_result\""),
        "{js}"
    );
    assert!(js.contains(")(value > 0)).map((value) => value);"), "{js}");
}

/// Verifies direct Oxc AST lowering handles guarded destructuring
/// list-comprehension generators.
///
/// Inputs:
/// - A checked Terlan module with a public function returning a
///   tuple-destructuring list comprehension guarded by one destructured
///   binding.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that guarded destructuring
///   composes into `.filter(...).map(...)` with JavaScript array destructuring
///   on both generated callbacks.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_guarded_destructuring_list_comprehension() {
    let source = "\
module js_core_direct_list_comprehension_guarded_destructuring.

pub first_positive(items: List[{Int, Int}]): List[Int] ->
    [left | {left, right} <- items, right > 0].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_list_comprehension_guarded_destructuring.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits guarded destructuring list comprehension CoreIR");

    assert!(js.contains("export function first_positive(items)"));
    assert!(js.contains("filter(([left, right]) =>"), "{js}");
    assert!(
        js.contains("__terlan_comprehension_guard_result[0] === \"guard_result\""),
        "{js}"
    );
    assert!(
        js.contains("right > 0") && js.contains("map(([left, right]) => left)"),
        "{js}"
    );
}

/// Verifies direct Oxc AST lowering handles stacked list-comprehension guards.
///
/// Inputs:
/// - A checked Terlan module with a public function returning a
///   single-generator list comprehension with two boolean guards.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that stacked pure guards are
///   preserved as source-ordered JavaScript filters.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_stacked_guarded_list_comprehension() {
    let source = "\
module js_core_direct_list_comprehension_stacked_guard.

pub middle(items: List[Int]): List[Int] ->
    [value | value <- items, value > 0, value < 10].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_list_comprehension_stacked_guard.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits stacked guarded list comprehension CoreIR");

    assert!(js.contains("export function middle(items)"));
    assert!(
        js.matches("filter((value) =>").count() == 2
            && js.contains("(value > 0)")
            && js.contains("(value < 10)")
            && js.contains("map((value) => value)"),
        "{js}"
    );
}

/// Verifies that remote calls stay outside the direct JS backend subset.
///
/// Inputs:
/// - A checked Terlan module with a public function returning a remote
///   Vm call expression.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines `CoreExpr::RemoteCall`, then checks the
///   command-facing facade preserves the JS stub fallback until JS interop
///   call semantics are selected explicitly.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_remote_call() {
    let source = "\
module js_core_remote_call_fallback.

pub call_remote(): Int ->
    erlang.abs(1).
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_remote_call_fallback.terl",
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

    assert!(js.contains("export function call_remote()"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that remote function references stay outside the direct JS
/// backend subset.
///
/// Inputs:
/// - A checked Terlan module with a public function returning a remote
///   Vm function reference.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines `CoreExpr::RemoteFunRef`, then checks the
///   command-facing facade preserves the JS stub fallback until JS interop
///   function-reference semantics are selected explicitly.
#[test]
fn emit_core_module_with_oxc_codegen_rejects_remote_fun_ref_source_syntax() {
    let source = "\
module js_core_remote_fun_ref_fallback.

pub reference(): Dynamic ->
    fun erlang:abs/1.
";
    let result = compile_syntax_module_through_phases_with_profile(
        "js_core_remote_fun_ref_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    );
    assert!(
        result.is_err(),
        "remote fun references are backend output syntax, not canonical Terlan source"
    );
}
