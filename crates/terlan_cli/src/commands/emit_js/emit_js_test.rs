use super::*;

use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

/// Verifies that JS emission reads public functions and supported bodies
/// from CoreIR instead of syntax declarations.
///
/// Inputs:
/// - A checked Terlan module containing one public and one private
///   function.
///
/// Output:
/// - Assertions over generated JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, emits JS from
///   `CoreModule`, checks that only public CoreIR functions appear, and
///   verifies the supported arithmetic body lowers to a return statement.
#[test]
fn emit_core_module_to_js_uses_core_function_exports() {
    let source = "\
module js_core_surface.

pub add(A: Int, B: Int): Int ->
    A + B.

hidden(A: Int): Int ->
    A.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_surface.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = core_lowering::emit_core_module_to_js(&artifacts.core);

    assert!(js.contains("export function add(A, B)"));
    assert!(js.contains("return (A + B);"));
    assert!(!js.contains("hidden"));
}

/// Verifies that bootstrap CoreIR-to-JS lowering handles Terlan integer
/// division.
///
/// Inputs:
/// - A checked Terlan module with one public function returning `A div B`.
///
/// Output:
/// - Assertions over bootstrap JavaScript source before Oxc reprinting.
///
/// Transformation:
/// - Compiles source through the formal pipeline, emits JS from
///   `CoreModule` through the bootstrap lowering helper, and checks that
///   `CoreExpr::BinaryOp` with `div` becomes `Math.trunc(A / B)`.
#[test]
fn emit_core_module_to_js_handles_integer_division() {
    let source = "\
module js_core_surface_div.

pub quotient(A: Int, B: Int): Int ->
    A div B.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_surface_div.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = core_lowering::emit_core_module_to_js(&artifacts.core);

    assert!(js.contains("export function quotient(A, B)"));
    assert!(js.contains("return Math.trunc(A / B);"), "{js}");
}

/// Verifies that bootstrap CoreIR-to-JS lowering handles focused
/// pipe-forward expressions into local named calls.
///
/// Inputs:
/// - A checked Terlan module with a public unary function and a public
///   function using `1 |> add_one()`.
///
/// Output:
/// - Assertions over bootstrap JavaScript source before Oxc reprinting.
///
/// Transformation:
/// - Compiles source through the formal pipeline, emits JS from
///   `CoreModule` through the bootstrap lowering helper, and checks that
///   `CoreExpr::BinaryOp` with `|>` becomes `add_one(1)`.
#[test]
fn emit_core_module_to_js_handles_pipe_forward_to_named_call() {
    let source = "\
module js_core_surface_pipe_forward.

pub add_one(x: Int): Int ->
    x + 1.

pub piped(): Int ->
    1 |> add_one().
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_surface_pipe_forward.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = core_lowering::emit_core_module_to_js(&artifacts.core);

    assert!(js.contains("export function add_one(x)"));
    assert!(js.contains("export function piped()"));
    assert!(js.contains("return add_one(1);"), "{js}");
}

/// Verifies that bootstrap CoreIR-to-JS lowering handles total integer
/// literal case expressions.
///
/// Inputs:
/// - A checked Terlan module with a public function matching an integer
///   scrutinee against an integer literal and a wildcard fallback.
///
/// Output:
/// - Assertions over bootstrap JavaScript source before Oxc reprinting.
///
/// Transformation:
/// - Compiles source through the formal pipeline, emits JS from
///   `CoreModule` through the bootstrap lowering helper, and checks that
///   `CorePattern::Int` becomes a JavaScript strict-equality test.
#[test]
fn emit_core_module_to_js_handles_integer_literal_case_expr() {
    let source = "\
module js_core_surface_integer_case.

pub classify(value: Int): Int ->
    case value {
        0 -> 1;
        _ -> 2
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_surface_integer_case.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = core_lowering::emit_core_module_to_js(&artifacts.core);

    assert!(js.contains("export function classify(value)"));
    assert!(js.contains("return (value === 0 ? 1 : 2);"), "{js}");
}

/// Verifies that bootstrap CoreIR-to-JS lowering handles total float
/// literal case expressions.
///
/// Inputs:
/// - A checked Terlan module with a public function matching a float
///   scrutinee against a finite float literal and a wildcard fallback.
///
/// Output:
/// - Assertions over bootstrap JavaScript source before Oxc reprinting.
///
/// Transformation:
/// - Compiles source through the formal pipeline, emits JS from
///   `CoreModule` through the bootstrap lowering helper, and checks that
///   `CorePattern::Float` becomes a JavaScript strict-equality test.
#[test]
fn emit_core_module_to_js_handles_float_literal_case_expr() {
    let source = "\
module js_core_surface_float_case.

pub classify(value: Float): Int ->
    case value {
        1.5 -> 1;
        _ -> 2
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_surface_float_case.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = core_lowering::emit_core_module_to_js(&artifacts.core);

    assert!(js.contains("export function classify(value)"));
    assert!(js.contains("return (value === 1.5 ? 1 : 2);"), "{js}");
}

/// Verifies that bootstrap CoreIR-to-JS lowering handles boolean literal
/// case expressions through atom artifact matching.
///
/// Inputs:
/// - A checked Terlan module with a public function matching a boolean
///   scrutinee against the raw atom pattern `:true` and a wildcard
///   fallback.
///
/// Output:
/// - Assertions over bootstrap JavaScript source before Oxc reprinting.
///
/// Transformation:
/// - Compiles source through the formal pipeline, emits JS from
///   `CoreModule` through the bootstrap lowering helper, and checks that
///   the `:true` `CorePattern::Atom` artifact becomes JavaScript strict
///   equality against the JavaScript boolean `true`.
#[test]
fn emit_core_module_to_js_handles_bool_literal_case_expr() {
    let source = "\
module js_core_surface_bool_case.

pub classify(flag: Bool): Int ->
    case flag {
        :true -> 1;
        _ -> 0
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_surface_bool_case.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = core_lowering::emit_core_module_to_js(&artifacts.core);

    assert!(js.contains("export function classify(flag)"));
    assert!(js.contains("return (flag === true ? 1 : 0);"), "{js}");
}

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
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("emit CoreIR through Oxc backend facade");

    assert!(js.contains("export function add(A, B)"));
    assert!(js.contains("return A + B;") || js.contains("return (A + B);"));
}

/// Verifies that Oxc AST construction can print a minimal module.
///
/// Inputs:
/// - The fixed backend smoke module constructed inside the Oxc adapter.
///
/// Output:
/// - Assertions over generated JavaScript source.
///
/// Transformation:
/// - Builds a JavaScript AST directly through Oxc, prints it through Oxc
///   codegen, and checks the exported function shape.
#[test]
fn emit_minimal_direct_oxc_ast_module_prints_export() {
    let js = oxc_backend::emit_minimal_direct_oxc_ast_module();

    assert!(js.contains("export function add(A, B)"));
    assert!(js.contains("return A + B;") || js.contains("return (A + B);"));
}

/// Verifies that a real CoreIR module can lower arithmetic through the
/// direct Oxc AST subset.
///
/// Inputs:
/// - A checked Terlan module with one public arithmetic function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks the export plus arithmetic
///   return expression.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_arithmetic_function() {
    let source = "\
module js_core_direct_ast.

pub add(A: Int, B: Int): Int ->
    A + B.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_ast.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST smoke emits supported CoreIR");

    assert!(js.contains("export function add(A, B)"));
    assert!(js.contains("return A + B;") || js.contains("return (A + B);"));
}

/// Verifies that direct Oxc AST lowering handles integer literal returns.
///
/// Inputs:
/// - A checked Terlan module with one public nullary integer function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks the numeric return
///   expression is emitted without bootstrap JavaScript parsing.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_integer_literal() {
    let source = "\
module js_core_direct_int.

pub answer(): Int ->
    42.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_int.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits integer literal CoreIR");

    assert!(js.contains("export function answer()"));
    assert!(js.contains("return 42;"));
}

/// Verifies that direct Oxc AST lowering handles finite float literal
/// returns.
///
/// Inputs:
/// - A checked Terlan module with one public nullary float function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks the float return expression
///   is emitted as a JavaScript numeric literal.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_float_literal() {
    let source = "\
module js_core_direct_float.

pub ratio(): Float ->
    1.5.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_float.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits float literal CoreIR");

    assert!(js.contains("export function ratio()"));
    assert!(js.contains("return 1.5;"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles string-like literals.
///
/// Inputs:
/// - A checked Terlan module with public binary and atom literal returns.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR binary and atom
///   literals become JavaScript string literal returns.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_string_like_literals() {
    let source = "\
module js_core_direct_strings.

pub message(): Binary ->
    \"hello\".

pub status(): Atom ->
    :ok.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_strings.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits string-like literal CoreIR");

    assert!(js.contains("export function message()"));
    assert!(js.contains(r#"return "hello";"#), "{js}");
    assert!(js.contains("export function status()"));
    assert!(
        js.contains("return \"ok\";") || js.contains("return 'ok';"),
        "{js}"
    );
}

/// Verifies that direct Oxc AST lowering handles boolean literals.
///
/// Inputs:
/// - A checked Terlan module with public functions returning `true` and
///   `false`.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR atoms
///   representing booleans become JavaScript boolean literals while other
///   atoms remain string-like artifacts.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_bool_literals() {
    let source = "\
module js_core_direct_bools.

pub yes(): Bool ->
    true.

pub no(): Bool ->
    false.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_bools.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits boolean literal CoreIR");

    assert!(js.contains("export function yes()"));
    assert!(js.contains("return true;"), "{js}");
    assert!(js.contains("export function no()"));
    assert!(js.contains("return false;"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles total if expressions.
///
/// Inputs:
/// - A checked Terlan module with a public function containing an if
///   expression whose final clause is `true`.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that the total CoreIR if
///   subset becomes a JavaScript conditional expression.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_total_if_expr() {
    let source = "\
module js_core_direct_total_if.

pub choose(flag: Bool): Int ->
    if { flag -> 1; true -> 0 }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_total_if.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits total if CoreIR");

    assert!(js.contains("export function choose(flag)"));
    assert!(js.contains("return flag ? 1 : 0;"), "{js}");
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
        TargetProfile::default(),
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
        :none -> 0;
        _ -> 1
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_literal_case.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
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
        TargetProfile::default(),
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
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits float literal case CoreIR");

    assert!(js.contains("export function classify(value)"));
    assert!(js.contains("return value === 1.5 ? 1 : 2;"), "{js}");
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
        :true -> 1;
        _ -> 0
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_bool_case.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits bool literal case CoreIR");

    assert!(js.contains("export function classify(flag)"));
    assert!(js.contains("return flag === true ? 1 : 0;"), "{js}");
}

/// Verifies that partial literal case expressions stay outside the direct
/// JS backend subset until no-match runtime semantics are represented.
///
/// Inputs:
/// - A checked Terlan module with a public function containing a case
///   expression over a variable scrutinee and no final wildcard fallback.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines the partial `CoreExpr::Case`, then checks the
///   command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_partial_case_expr() {
    let source = "\
module js_core_partial_case_fallback.

pub choose(status: Atom): Int ->
    case status {
        :none -> 0
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_partial_case_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
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

/// Verifies that guarded literal case expressions stay outside the direct
/// JS backend subset until guard dispatch semantics are represented.
///
/// Inputs:
/// - A checked Terlan module with a public function containing a case
///   expression whose literal-pattern branch has a boolean guard.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines the guarded `CoreExpr::Case`, then checks the
///   command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_guarded_case_expr() {
    let source = "\
module js_core_guarded_case_fallback.

pub choose(status: Atom, flag: Bool): Int ->
    case status {
        :none when flag -> 0;
        _ -> 1
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_guarded_case_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function choose(status, flag)"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that destructuring case patterns stay outside the direct JS
/// backend subset until pattern-dispatch semantics are represented.
///
/// Inputs:
/// - A checked Terlan module with a public function containing a case
///   expression whose first branch uses a tuple destructuring pattern.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines the destructuring `CoreExpr::Case`, then checks
///   the command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_destructuring_case_expr() {
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
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function first(value)"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
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
        TargetProfile::default(),
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
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits list comprehension CoreIR");

    assert!(js.contains("export function values(items)"));
    assert!(js.contains("return items.map((value) => value);"), "{js}");
}

/// Verifies that destructuring list-comprehension generators stay outside
/// the direct JS subset.
///
/// Inputs:
/// - A checked Terlan module with a public function returning a
///   single-generator list comprehension whose generator pattern
///   destructures tuple elements.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies direct Oxc AST
///   lowering declines the destructuring `CoreExpr::ListComprehension`,
///   then checks the command-facing Oxc facade preserves the existing JS
///   stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_destructuring_list_comprehension() {
    let source = "\
module js_core_list_comprehension_destructuring_fallback.

pub firsts(items: List[{Int, Int}]): List[Int] ->
    [left | {left, _right} <- items].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_list_comprehension_destructuring_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function firsts(items)"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that remote calls stay outside the direct JS backend subset.
///
/// Inputs:
/// - A checked Terlan module with a public function returning a remote
///   Erlang call expression.
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
        TargetProfile::default(),
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
///   Erlang function reference.
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
        TargetProfile::default(),
    );
    assert!(
        result.is_err(),
        "remote fun references are backend output syntax, not canonical Terlan source"
    );
}

/// Verifies that constructor calls stay outside the direct JS backend
/// subset until their runtime representation is selected.
///
/// Inputs:
/// - A checked Terlan module with a declared constructor and a public
///   function returning a constructor call.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines `CoreExpr::ConstructorCall`, then checks the
///   command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_constructor_call() {
    let source = "\
module js_core_constructor_call_fallback.

pub constructor Ok {
    (value: Int): Dynamic -> value
}.

pub make(): Dynamic ->
    Ok(1).
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_constructor_call_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function make()"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that constructor chains stay outside the direct JS backend
/// subset until their runtime representation is selected.
///
/// Inputs:
/// - A checked Terlan module with a declared constructor and a public
///   function returning a constructor-chain expression.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines `CoreExpr::ConstructorChain`, then checks the
///   command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_constructor_chain() {
    let source = "\
module js_core_constructor_chain_fallback.

pub constructor User {
    (id: Int, name: Binary): Dynamic -> id
}.

pub make(id: Int, name: Binary): Dynamic ->
    User(id, name) with Admin { id = id, name = name }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_constructor_chain_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function make(id, name)"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that try expressions stay outside the direct JS backend subset
/// until exception and cleanup semantics are selected.
///
/// Inputs:
/// - A checked Terlan module with a public function returning a `try`
///   expression with `of`, `catch`, and `after` clauses.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines `CoreExpr::Try`, then checks the command-facing
///   facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_try_expr() {
    let source = "\
module js_core_try_fallback.

pub run(): Dynamic ->
    try 1 {
        value -> value
    catch
        reason -> reason
    after
        0 -> :done
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_try_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function run()"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that quote expressions stay outside the direct JS backend
/// subset until macro-AST runtime semantics are selected.
///
/// Inputs:
/// - A checked Terlan module with a public function returning `quote 1`.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines the runtime-boundary quote body, then checks the
///   command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_quote_expr() {
    let source = "\
module js_core_quote_fallback.

pub quoted(): Ast[Int] ->
    quote 1.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_quote_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function quoted()"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that unquote expressions stay outside the direct JS backend
/// subset until macro-AST runtime semantics are selected.
///
/// Inputs:
/// - A checked Terlan module with a public function returning
///   `unquote(value)`.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines the runtime-boundary unquote body, then checks
///   the command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_unquote_expr() {
    let source = "\
module js_core_unquote_fallback.

pub unquoted(value: Int): Int ->
    unquote(value).
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_unquote_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function unquoted(value)"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that inline HTML blocks stay outside the direct JS backend
/// subset until HTML rendering semantics are selected for `emit-js`.
///
/// Inputs:
/// - A checked Terlan module with a public function returning an
///   `html { ... }` block.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines the runtime-boundary HTML block body, then
///   checks the command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_html_block_expr() {
    let source = "\
module js_core_html_block_fallback.

pub view(): Html[Dynamic] ->
    html { <main>Hello</main> }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_html_block_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function view()"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that direct Oxc AST lowering handles tuple and list literals.
///
/// Inputs:
/// - A checked Terlan module with public tuple and list literal returns.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR tuple/list
///   values use the JavaScript array representation.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_array_like_literals() {
    let source = "\
module js_core_direct_arrays.

pub pair(): {Int, Int} ->
    {1, 2}.

pub values(): List[Int] ->
    [3, 4].

pub fixed(): FixedArray[2, Int] ->
    #[5, 6].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_arrays.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits tuple/list literal CoreIR");

    assert!(js.contains("export function pair()"));
    assert!(js.contains("return [1, 2];"), "{js}");
    assert!(js.contains("export function values()"));
    assert!(js.contains("return [3, 4];"), "{js}");
    assert!(js.contains("export function fixed()"));
    assert!(js.contains("return [5, 6];"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles unary negation.
///
/// Inputs:
/// - A checked Terlan module with one public unary-minus function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR unary minus
///   becomes a JavaScript unary negation return.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_unary_negation() {
    let source = "\
module js_core_direct_unary.

pub negate(value: Int): Int ->
    -value.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_unary.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits unary negation CoreIR");

    assert!(js.contains("export function negate(value)"));
    assert!(
        js.contains("return -value;") || js.contains("return (-value);"),
        "{js}"
    );
}

/// Verifies that direct Oxc AST lowering handles expression-side list cons.
///
/// Inputs:
/// - A checked Terlan module with one public list-cons function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR list cons
///   becomes a JavaScript array literal with a spread tail.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_list_cons() {
    let source = "\
module js_core_direct_list_cons.

pub prepend(head: Int, tail: List[Int]): List[Int] ->
    [head | tail].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_list_cons.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits list-cons CoreIR");

    assert!(js.contains("export function prepend(head, tail)"));
    assert!(js.contains("return [head, ...tail];"), "{js}");
}

/// Verifies that source index expressions currently fall back in Oxc JS.
///
/// Inputs:
/// - A checked Terlan module with one public fixed-array indexing
///   function.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, which lowers bracket
///   source syntax through `IndexGet.get_at`, verifies direct Oxc emission
///   declines that trait-backed call, then checks the public Oxc facade
///   returns the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_index_trait_call() {
    let source = "\
module js_core_direct_index.

pub first(items: FixedArray[2, Int]): Int ->
    items[0].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_index.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function first(items)"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that direct Oxc AST lowering handles identifier-key map
/// literals.
///
/// Inputs:
/// - A checked Terlan module with one public map literal function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that a CoreIR map literal
///   becomes a JavaScript object literal for the current identifier-key
///   subset.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_map_literal() {
    let source = "\
module js_core_direct_map.

pub point(): Term ->
    #{x => 1, y => 2}.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_map.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits map literal CoreIR");

    assert!(js.contains("export function point()"));
    assert!(js.contains("return {"), "{js}");
    assert!(js.contains("x: 1"), "{js}");
    assert!(js.contains("y: 2"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles struct field-access
/// expressions.
///
/// Inputs:
/// - A checked Terlan module with a public struct and field reader.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR field access
///   becomes JavaScript static member access.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_field_access() {
    let source = "\
module js_core_direct_field.

pub struct Point {
    x: Int
}.

pub read(point: Point): Int ->
    point.x.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_field.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits field-access CoreIR");

    assert!(js.contains("export function read(point)"));
    assert!(js.contains("return point.x;"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles struct construction.
///
/// Inputs:
/// - A checked Terlan module with a public struct constructor function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR record
///   construction becomes a JavaScript object literal for the current
///   struct-value subset.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_record_construct() {
    let source = "\
module js_core_direct_record_construct.

pub struct Point {
    x: Int
}.

pub make(): Point ->
    #Point { x = 1 }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_record_construct.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits record construction CoreIR");

    assert!(js.contains("export function make()"));
    assert!(js.contains("return {"), "{js}");
    assert!(js.contains("x: 1"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles explicit record-access
/// expressions.
///
/// Inputs:
/// - A checked Terlan module with a public struct and explicit record
///   field reader.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR record access
///   becomes JavaScript static member access.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_record_access() {
    let source = "\
module js_core_direct_record_access.

pub struct Point {
    x: Int
}.

pub read(point: Point): Int ->
    point#Point.x.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_record_access.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits record-access CoreIR");

    assert!(js.contains("export function read(point)"));
    assert!(js.contains("return point.x;"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles explicit record-update
/// expressions.
///
/// Inputs:
/// - A checked Terlan module with a public struct and record update
///   function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR record update
///   becomes JavaScript object spread for the current struct-value subset.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_record_update() {
    let source = "\
module js_core_direct_record_update.

pub struct Point {
    x: Int,
    y: Int
}.

pub set_x(point: Point): Point ->
    point#Point { x = 1, y = point.y }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_record_update.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits record-update CoreIR");

    assert!(js.contains("export function set_x(point)"));
    assert!(js.contains("...point"), "{js}");
    assert!(js.contains("x: 1"), "{js}");
    assert!(js.contains("y: point.y"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles template-instantiation
/// values as object-like JS artifacts.
///
/// Inputs:
/// - A checked Terlan module with a template declaration and a public
///   function that instantiates it.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through parse, template validation, typechecking, and
///   CoreIR lowering, then checks that `CoreExpr::TemplateInstantiate`
///   becomes a JavaScript object literal in the current `emit-js` artifact
///   subset. This does not render HTML; static rendering remains owned by
///   the static-site command path.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_template_instantiate() {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "terlan_emit_js_template_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let template_dir = dir.join("templates");
    fs::create_dir_all(&template_dir).expect("create template dir");
    fs::write(template_dir.join("page.terl.html"), "<h1>{title}</h1>").expect("write template");
    let source_path = dir.join("js_core_direct_template_instantiate.terl");
    let source_path = source_path.to_string_lossy().to_string();
    let source = "\
module js_core_direct_template_instantiate.

template Page from \"./templates/page.terl.html\" {
    title: Binary
}.

pub view(title: Binary): Html[Dynamic] ->
    Page{ title = title }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        &source_path,
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits template-instantiation CoreIR");

    assert!(js.contains("export function view(title)"));
    assert!(js.contains("return {"), "{js}");
    assert!(js.contains("return { title };"), "{js}");
}

/// Verifies that direct Oxc AST lowering covers selected binary operators.
///
/// Inputs:
/// - A checked Terlan module with public arithmetic and comparison
///   operator functions.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that selected CoreIR
///   binary operators map to their JavaScript operator forms.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_binary_operator_set() {
    let source = "\
module js_core_direct_binary_ops.

pub subtract(x: Int, y: Int): Int ->
    x - y.

pub multiply(x: Int, y: Int): Int ->
    x * y.

pub divide(x: Float, y: Float): Float ->
    x / y.

pub integer_divide(x: Int, y: Int): Int ->
    x div y.

pub remainder(x: Int, y: Int): Int ->
    x rem y.

pub same(x: Int, y: Int): Bool ->
    x == y.

pub exact_same(x: Int, y: Int): Bool ->
    x == y.

pub not_same(x: Int, y: Int): Bool ->
    x != y.

pub not_exact_same(x: Int, y: Int): Bool ->
    x != y.

pub less_than(x: Int, y: Int): Bool ->
    x < y.

pub less_than_or_equal(x: Int, y: Int): Bool ->
    x <= y.

pub greater_than(x: Int, y: Int): Bool ->
    x > y.

pub greater_than_or_equal(x: Int, y: Int): Bool ->
    x >= y.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_binary_ops.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits selected binary operator CoreIR");

    assert!(js.contains("return x - y;"), "{js}");
    assert!(js.contains("return x * y;"), "{js}");
    assert!(js.contains("return x / y;"), "{js}");
    assert!(js.contains("return Math.trunc(x / y);"), "{js}");
    assert!(js.contains("return x % y;"), "{js}");
    assert!(js.contains("return x === y;"), "{js}");
    assert_eq!(js.matches("return x === y;").count(), 2, "{js}");
    assert_eq!(js.matches("return x !== y;").count(), 2, "{js}");
    assert!(js.contains("return x < y;"), "{js}");
    assert!(js.contains("return x <= y;"), "{js}");
    assert!(js.contains("return x > y;"), "{js}");
    assert!(js.contains("return x >= y;"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles local named calls.
///
/// Inputs:
/// - A checked Terlan module with a private local function and a public
///   caller.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that a CoreIR local call
///   becomes a JavaScript call expression.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_named_call() {
    let source = "\
module js_core_direct_named_call.

identity(x: Int): Int ->
    x.

pub call_it(): Int ->
    identity(1).
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_named_call.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits named-call CoreIR");

    assert!(js.contains("function identity(x)"));
    assert!(!js.contains("export function identity"));
    assert!(js.contains("export function call_it()"));
    assert!(js.contains("return identity(1);"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles function-value invocation.
///
/// Inputs:
/// - A checked Terlan module whose public function invokes a function-typed
///   parameter with `f.(value)`.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that
///   `CoreExpr::FunctionCall` becomes a JavaScript callable-value
///   application.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_function_value_call() {
    let source = "\
module js_core_direct_function_value_call.

pub apply(value: Int, f: (Int) -> Int): Int ->
    f.(value).
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_function_value_call.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits function-value call CoreIR");

    assert!(js.contains("export function apply(value, f)"));
    assert!(js.contains("return f(value);"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles selected CoreIR intrinsics.
///
/// Inputs:
/// - A checked Terlan module whose public function calls
///   `"hello".contains("ell")`.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies receiver-method
///   syntax lowers to the backend-neutral `core.string.contains` intrinsic,
///   and checks direct Oxc lowering emits JavaScript `.includes(...)`.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_string_contains_intrinsic() {
    let source = "\
module js_core_direct_string_contains_intrinsic.

pub has_needle(): Bool ->
    \"hello\".contains(\"ell\").
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_string_contains_intrinsic.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits string contains intrinsic CoreIR");

    assert!(js.contains("export function has_needle()"));
    assert!(js.contains(r#"return "hello".includes("ell");"#), "{js}");
}

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
fn emit_core_module_with_direct_oxc_ast_handles_string_starts_with_intrinsic() {
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
        TargetProfile::default(),
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
fn emit_core_module_with_direct_oxc_ast_handles_string_length_intrinsic() {
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
        TargetProfile::default(),
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
fn emit_core_module_with_direct_oxc_ast_handles_pipe_forward_to_named_call() {
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
        TargetProfile::default(),
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
fn emit_core_module_with_oxc_codegen_emits_named_call_private_helper() {
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
        TargetProfile::default(),
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
fn emit_core_module_with_direct_oxc_ast_ignores_unreachable_private_function() {
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
        TargetProfile::default(),
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
fn emit_core_module_with_oxc_codegen_uses_direct_reachability_filter() {
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
        TargetProfile::default(),
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
fn emit_core_module_with_oxc_codegen_falls_back_for_binding_case_expr() {
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
        TargetProfile::default(),
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

/// Verifies that TypeScript declarations read public signatures from
/// CoreIR metadata.
///
/// Inputs:
/// - A checked Terlan module containing public/private functions and a
///   public result type.
///
/// Output:
/// - Assertions over generated TypeScript declaration source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, emits declarations from
///   `CoreModule`, and checks type/function visibility plus CoreIR type
///   mapping.
#[test]
fn emit_core_module_to_typescript_declarations_uses_core_surface() {
    let source = "\
module js_core_declarations.

pub type Result[T, E] =
      {ok, T}
    | {error, E}.

pub add(A: Int, B: Int): Int ->
    A + B.

hidden(A: Int): Int ->
    A.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_declarations.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let declarations = declarations::emit_core_module_to_typescript_declarations(&artifacts.core);

    assert!(declarations.contains("export type Result<T, E>"));
    assert!(declarations.contains("export function add(A: number, B: number): number;"));
    assert!(!declarations.contains("hidden"));
}
