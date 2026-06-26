use super::*;

use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

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
