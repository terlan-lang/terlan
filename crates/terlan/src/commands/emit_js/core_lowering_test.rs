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

/// Verifies bootstrap JS emission escapes binary string literals portably.
///
/// Inputs:
/// - A checked Terlan module returning a string containing quote, backslash,
///   newline, carriage return, and tab escapes.
///
/// Output:
/// - JavaScript source containing the escaped literal in one return statement.
///
/// Transformation:
/// - Compiles source through CoreIR and emits JS, proving the shared
///   `quoted_string_literal` helper preserves all escape-sensitive characters.
#[test]
fn emit_core_module_to_js_escapes_binary_literals_portably() {
    let source = "\
module js_core_surface_string_escape.

pub escaped(): String ->
    \"quote \\\" slash \\\\ newline \\n carriage \\r tab \\t\".
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_surface_string_escape.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    let js = core_lowering::emit_core_module_to_js(&artifacts.core);

    assert!(js.contains("export function escaped()"));
    assert!(
        js.contains(r#"return "quote \" slash \\ newline \n carriage \r tab \t";"#),
        "{js}"
    );
}
