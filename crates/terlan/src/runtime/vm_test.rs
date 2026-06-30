use super::*;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::{ColorChoice, DiagnosticFormat};

/// Verifies simple arithmetic source evaluates without backend execution.
///
/// Inputs:
/// - Source module with one zero-arity function body `1 + 2`.
///
/// Output:
/// - Test assertion only; no files or target runtimes are touched.
///
/// Transformation:
/// - Compiles through the formal pipeline, then runs the compiler-owned
///   evaluator and asserts Terlan-facing value rendering.
#[test]
fn evaluator_renders_simple_integer_expression() {
    let core = compile_core("module repl_test.\n\npub run(): Dynamic ->\n    1 + 2.\n");

    let value = evaluate_repl_function(&core, "run").expect("evaluate");

    assert_eq!(value.render(), "3");
}

/// Verifies console output returns `Unit` through the evaluator hook.
///
/// Inputs:
/// - Source module importing `println` and calling it.
///
/// Output:
/// - Test assertion for the returned value.
///
/// Transformation:
/// - Compiles through the formal pipeline, executes the selected std effect
///   hook through the compiler-owned evaluator, and checks it still returns
///   the Terlan `Unit` value.
#[test]
fn evaluator_returns_unit_for_console_println() {
    let core = compile_core(
            "module repl_test.\n\nimport std.io.Console.{println}.\n\npub run(): Unit ->\n    println(\"hello\").\n",
        );

    let value = evaluate_repl_function(&core, "run").expect("evaluate");

    assert_eq!(value, ReplValue::Unit);
}

/// Verifies console output can be captured by the caller.
///
/// Inputs:
/// - Source module importing `println` and calling it once.
/// - Caller-owned output buffer.
///
/// Output:
/// - Test assertion for returned `Unit` and captured output payload.
///
/// Transformation:
/// - Executes the same evaluator hook through the output-aware entry point
///   used by JSON REPL mode instead of printing directly from the evaluator.
#[test]
fn evaluator_routes_console_println_through_output_sink() {
    let core = compile_core(
            "module repl_test.\n\nimport std.io.Console.{println}.\n\npub run(): Unit ->\n    println(\"hello\").\n",
        );
    let mut output = Vec::new();
    let mut capture = |value: &str| output.push(value.to_string());

    let value = evaluate_repl_function_with_output(&core, "run", &mut capture).expect("evaluate");

    assert_eq!(value, ReplValue::Unit);
    assert_eq!(output, vec!["hello".to_string()]);
}

/// Verifies source-level `type_of` returns a REPL `Type` value.
///
/// Inputs:
/// - Source module with one zero-arity function body `type_of(1)`.
///
/// Output:
/// - Test assertion for rendered type syntax.
///
/// Transformation:
/// - Compiles the implicit function call through the formal path, then
///   executes the compiler-backed REPL type introspection hook.
#[test]
fn evaluator_supports_type_of_for_integer() {
    let core = compile_core("module repl_test.\n\npub run(): Dynamic ->\n    type_of(1).\n");

    let value = evaluate_repl_function(&core, "run").expect("evaluate");

    assert_eq!(value, ReplValue::Type("Int".to_string()));
    assert_eq!(value.render(), "Int");
}

/// Verifies source-level `is_type` compares against implicit type values.
///
/// Inputs:
/// - Source module with one zero-arity function body `is_type(1, Int)`.
///
/// Output:
/// - Test assertion for a boolean result.
///
/// Transformation:
/// - Treats `Int` as an implicit type value in expression position and
///   compares it to the evaluated first argument's type.
#[test]
fn evaluator_supports_is_type_for_implicit_type_value() {
    let core = compile_core("module repl_test.\n\npub run(): Dynamic ->\n    is_type(1, Int).\n");

    let value = evaluate_repl_function(&core, "run").expect("evaluate");

    assert_eq!(value, ReplValue::Bool(true));
    assert_eq!(value.render(), "true");
}

/// Verifies anonymous functions evaluate to opaque REPL function values.
///
/// Inputs:
/// - Source module returning one lambda expression.
///
/// Output:
/// - Test assertion for the REPL-facing rendered function value.
///
/// Transformation:
/// - Compiles the lambda through CoreIR and evaluates it as a captured closure
///   without invoking a target runtime.
#[test]
fn evaluator_renders_lambda_as_function_value() {
    let core = compile_core("module repl_test.\n\npub run(): Dynamic ->\n    (x) -> x + x.\n");

    let value = evaluate_repl_function(&core, "run").expect("evaluate");

    assert_eq!(value.render(), "<function>");
    assert_eq!(type_of_value(&value), "Function");
}

/// Verifies function-value invocation applies captured lambda values.
///
/// Inputs:
/// - Source module invoking an inline lambda through `f.(10)`-style function
///   value syntax.
///
/// Output:
/// - Test assertion for the evaluated function-call result.
///
/// Transformation:
/// - Exercises CoreIR `Lam` and `FunctionCall` together so the REPL can
///   evaluate first-class functions without BEAM execution.
#[test]
fn evaluator_applies_lambda_function_value_call() {
    let core =
        compile_core("module repl_test.\n\npub run(): Dynamic ->\n    ((x) -> x + x).(10).\n");

    let value = evaluate_repl_function(&core, "run").expect("evaluate");

    assert_eq!(value, ReplValue::Int(20));
}

/// Verifies CoreIR remote calls dispatch through VM-owned std helpers.
#[test]
fn evaluator_supports_remote_std_assertion_call() {
    let core = compile_core(
        "module repl_test.\n\npub run(): Bool ->\n    std.test.Test.assert_equal(3, 1 + 2).\n",
    );

    let value = evaluate_repl_function(&core, "run").expect("evaluate");

    assert_eq!(value, ReplValue::Bool(true));
}

/// Verifies CoreIR case expressions match structural constructor patterns.
#[test]
fn evaluator_supports_case_constructor_pattern() {
    let core = compile_core(
        "module repl_test.\n\nimport std.core.Option.{None, Some}.\n\npub run(): Int ->\n    case Some(42) {\n        Some(value) -> value;\n        None -> 0\n    }.\n",
    );

    let value = evaluate_repl_function(&core, "run").expect("evaluate");

    assert_eq!(value, ReplValue::Int(42));
}

/// Verifies collection intrinsics and mutable receiver rebinding execute.
#[test]
fn evaluator_supports_list_mutation_intrinsics() {
    let core = compile_core(
        "module repl_test.\n\nimport std.collections.List.\n\npub run(): Bool ->\n    let values = List.new();\n    values.push(1);\n    values.push(2);\n    values.length() == 2.\n",
    );

    let value = evaluate_repl_function(&core, "run").expect("evaluate");

    assert_eq!(value, ReplValue::Bool(true));
}

/// Verifies runtime file intrinsics return portable Result values.
#[test]
fn evaluator_supports_runtime_file_exists_intrinsic() {
    let core = compile_core(
        "module repl_test.\n\npub run(): Bool ->\n    std.io.File.exists(\"/definitely/missing/terlan-vm-test-file\").\n",
    );

    let value = evaluate_repl_function(&core, "run").expect("evaluate");

    assert_eq!(value, ReplValue::Bool(false));
}

/// Compiles a test module into CoreIR for evaluator assertions.
///
/// Inputs:
/// - `source`: complete Terlan source module.
///
/// Output:
/// - CoreIR module produced by the formal compiler pipeline.
///
/// Transformation:
/// - Reuses the production formal pipeline so evaluator tests exercise the
///   same CoreIR payloads the REPL receives.
fn compile_core(source: &str) -> CoreModule {
    crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "<repl-evaluator-test>.terl",
        source,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        None,
        NativePolicy::SafeNativeOptional,
        TargetProfile::Erlang,
    )
    .expect("compile evaluator source")
    .core
}
