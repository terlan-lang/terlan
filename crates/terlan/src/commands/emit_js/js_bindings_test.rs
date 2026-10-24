//! Host bindings are checked through the real Terlan frontend and JS backend.

use super::*;

#[test]
fn real_array_program_compiles_with_explicit_type_arguments_and_call_scrutinee() {
    let source = r#"module tests.js.ArrayCompilation.
import std.js.Array.
pub check(): Bool ->
    let values = Array.from_list[Int]([10, 20, 30]);
    values.length() == 3 and values.get_at(1) == 20
        and case values.to_list() { [10, 20, 30] -> true; _ -> false }.
"#;
    let javascript = compiled(source);
    execute(
        &javascript,
        "if (check() !== true) throw new Error('array conversion failed');",
    );
}

#[test]
fn case_call_scrutinee_runs_once_with_lexical_state_preserved() {
    let source = r#"module tests.js.CaseCompilation.
pub check(read: (Int) -> Int, expected: Int): Bool ->
    case read(1) { 1 -> expected == 17; _ -> false }.
"#;
    let javascript = compiled(source);
    execute(&javascript, "let reads = 0; if (!check(value => { reads++; return value; }, 17) || reads !== 1) throw new Error('case scrutinee repeated'); if (check(() => 2, 17) !== false) throw new Error('case fallback failed');");
}

#[test]
fn javascript_reserved_parameter_names_preserve_their_values() {
    let source = r#"module tests.js.ReservedBindings.
pub check(default: Int, arguments: Int): Int ->
    let eval = default + arguments;
    case eval { 42 -> eval; _ -> default }.
"#;
    execute(&compiled(source), "if (check(17, 25) !== 42 || check(3, 4) !== 3) throw new Error('reserved binding was lost');");
}

#[test]
fn foreign_operations_reject_unknown_names_and_incorrect_arities() {
    assert!(adapter("std.js.String.to_core", 2)
        .unwrap_err()
        .to_string()
        .contains("expects 1 arguments"));
    assert!(adapter("std.js.Dom.Element.missing", 1)
        .unwrap_err()
        .to_string()
        .contains("unsupported host operation"));
}

fn compiled(source: &str) -> String {
    let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "tests/js/Compilation.terl",
        source,
        crate::DiagnosticFormat::default(),
        None,
        crate::validation::native_policy::NativePolicy::default(),
        crate::validation::target_profile::TargetProfile::JsBrowser,
    )
    .expect("compile the Terlan application");
    crate::commands::emit_js::emit_core_module_with_template_runtime(
        &artifacts.core,
        &artifacts.syntax_output,
        std::path::Path::new("tests/js/Compilation.terl"),
        crate::validation::target_profile::TargetProfile::JsBrowser,
    )
    .expect("emit checked JavaScript")
    .expect("all application bodies must be supported")
}

fn execute(javascript: &str, assertion: &str) {
    let result = std::process::Command::new("node")
        .args([
            "--input-type=module",
            "--eval",
            &format!("{javascript}\n{assertion}"),
        ])
        .output()
        .expect("execute emitted JavaScript");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn primary_option_arguments_decode_the_same_as_legacy_spelling() {
    for ty in [
        "Option[Int]",
        "std.core.Option[Int]",
        "std.core.Option.Option[Int]",
    ] {
        let expression = decode_nullable_argument("value", ty);
        execute(
            "const value = 'none';",
            &format!("if (({expression}) !== null) throw new Error('missing nullable argument');"),
        );
        execute(
            "const value = ['some', 42];",
            &format!("if (({expression}) !== 42) throw new Error('present nullable argument');"),
        );
    }
}

#[test]
fn primary_option_results_and_unit_results_keep_host_conversion() {
    let (_, option) =
        adapter("std.js.Dom.Document.get_element_by_id", 2).expect("nullable DOM method");
    execute(
        "const a0 = { getElementById() { return null; } }; const a1 = 'div';",
        &format!("if (({option}) !== 'none') throw new Error('missing nullable result');"),
    );
    let (_, unit) = adapter("std.js.Dom.Event.prevent_default", 1).expect("unit DOM method");
    execute(
        "const a0 = { preventDefault() { return 42; } };",
        &format!("if (({unit}) !== 'unit') throw new Error('unit result');"),
    );
}
