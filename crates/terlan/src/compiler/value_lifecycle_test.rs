use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxExprKind,
};

use super::evaluate_and_substitute_module_constants;

#[test]
fn evaluator_substitutes_constants_and_records_stable_fingerprints() {
    let mut module = parse_module_as_syntax_output(
        r#"
module lifecycle.pass.
const BASE: Int = 40.
const plus_two(value: Int): Int -> value + 2.
pub const ANSWER: Int = plus_two(BASE).
pub answer(): Int -> ANSWER.
"#,
    )
    .expect("parse lifecycle fixture");
    let report = evaluate_and_substitute_module_constants(&mut module);
    assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
    assert!(report.fingerprints.contains_key("ANSWER"));
    let body = module
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, clauses, .. } if name == "answer" => {
                clauses.first().map(|clause| &clause.body)
            }
            _ => None,
        })
        .expect("answer body");
    assert_eq!(body.kind, SyntaxExprKind::Int);
    assert_eq!(body.text.as_deref(), Some("42"));
}

#[test]
fn evaluator_rejects_cycles_effects_duplicate_values_and_missing_trait_constants() {
    let cases = [
        ("const A: Int = B.\nconst B: Int = A.", "CONST_CYCLE"),
        (
            "const BAD: Int = runtime_value().",
            "CONST_FORBIDDEN_EFFECT",
        ),
        (
            "type Code: Int = OK = 1 | ALSO_OK = 1.",
            "DUPLICATE_VALUED_UNION_VALUE",
        ),
        (
            "trait Required { const VALUE: Int. }.\nimpl Required for Int { value(): Int -> 1. }.",
            "MISSING_TRAIT_CONSTANT",
        ),
    ];
    for (body, code) in cases {
        let source = format!("module lifecycle.reject.\n{body}\n");
        let mut module = parse_module_as_syntax_output(&source).expect("negative fixture parses");
        let report = evaluate_and_substitute_module_constants(&mut module);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code),
            "expected {code}, got {:#?}",
            report.diagnostics
        );
    }
}

#[test]
fn runtime_const_function_calls_are_rejected_after_compile_time_calls_succeed() {
    let mut module = parse_module_as_syntax_output(
        r#"
module lifecycle.runtime_reject.
const double(value: Int): Int -> value * 2.
pub const FOUR: Int = double(2).
pub invalid(): Int -> double(FOUR).
"#,
    )
    .expect("parse const function fixture");
    let report = evaluate_and_substitute_module_constants(&mut module);
    assert!(report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "CONST_FUNCTION_RUNTIME_USE"));
}

#[test]
fn recursive_const_functions_require_core_termination_evidence() {
    let mut proven = parse_module_as_syntax_output(
        r#"
module lifecycle.total_const.
const countdown(n: Int): Int ->
    case n {
        value where value > 0 -> countdown(value - 1);
        _ -> 0
    }.
pub const RESULT: Int = countdown(100).
"#,
    )
    .expect("parse proven recursive const fixture");
    let report = evaluate_and_substitute_module_constants(&mut proven);
    assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);

    let mut unproven = parse_module_as_syntax_output(
        r#"
module lifecycle.partial_const.
const descend(n: Int): Int -> descend(n - 1).
pub const RESULT: Int = descend(100).
"#,
    )
    .expect("parse unproven recursive const fixture");
    let report = evaluate_and_substitute_module_constants(&mut unproven);
    assert!(report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "CONST_TOTALITY_UNPROVEN"));
    assert!(!report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("depth limit")));
}

#[test]
fn valued_union_parse_lowers_to_a_closed_checked_case() {
    let mut module = parse_module_as_syntax_output(
        r#"
module lifecycle.checked_parse.
pub type Status: Int = OK = 200 | NOT_FOUND = 404.
pub parse_status(code: Int): Status -> Status.parse(code).
"#,
    )
    .expect("parse valued-union conversion fixture");
    let report = evaluate_and_substitute_module_constants(&mut module);
    assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
    let body = module
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, clauses, .. } if name == "parse_status" => {
                clauses.first().map(|clause| &clause.body)
            }
            _ => None,
        })
        .expect("parse_status body");
    assert_eq!(body.kind, SyntaxExprKind::Case);
    assert_eq!(body.clauses.len(), 3);
    assert_eq!(
        body.clauses
            .last()
            .and_then(|clause| clause.body.raw.as_deref()),
        Some("checked_valued_union_parse_failure")
    );
}

#[test]
fn valued_union_parse_rejects_an_invalid_arity() {
    let mut module = parse_module_as_syntax_output(
        r#"
module lifecycle.checked_parse_arity.
pub type Status: Int = OK = 200.
pub invalid(): Status -> Status.parse().
"#,
    )
    .expect("parse invalid valued-union conversion fixture");
    let report = evaluate_and_substitute_module_constants(&mut module);
    assert!(report
        .diagnostics
        .iter()
        .any(|diagnostic| { diagnostic.code == "VALUED_UNION_PARSE_ARITY" }));
}

#[test]
fn const_generic_import_bindings_preserve_provider_arity_and_aliases() {
    use super::evaluate_and_substitute_module_constants_with_interfaces;
    use crate::terlan_hir::syntax_module_output_to_interface;
    use crate::terlan_syntax::parse_interface_module_as_syntax_output;
    let process = parse_interface_module_as_syntax_output(
        "module std.vm.Process.\npub opaque type Entry[T].\npub entry[T](slot: Int): Entry[T].\n",
    )
    .unwrap();
    let other = parse_interface_module_as_syntax_output(
        "module fixture.Other.\npub opaque type Entry.\npub entry(slot: Int): Entry.\n",
    )
    .unwrap();
    for _ in 0..64 {
        let interfaces = std::collections::HashMap::from([
            (
                process.module_name.clone(),
                syntax_module_output_to_interface(&process),
            ),
            (
                other.module_name.clone(),
                syntax_module_output_to_interface(&other),
            ),
        ]);
        for (imports, result, call, valid) in [
            ("import type std.vm.Process.{Entry}.\nimport std.vm.Process.{entry}.", "Entry[Int]", "entry[Int](7)", true),
            ("import type std.vm.Process.{Entry as WorkerEntry}.\nimport std.vm.Process.{entry as worker_entry}.", "WorkerEntry[Int]", "worker_entry[Int](7)", true),
            ("import type std.vm.Process.{Entry}.\nimport std.vm.Process.{entry}.", "Entry[Int, Bool]", "entry[Int](7)", false),
            ("import type fixture.Other.{Entry as PlainEntry}.\nimport fixture.Other.{entry}.", "PlainEntry[Int]", "entry(7)", false),
        ] {
            let mut module = parse_module_as_syntax_output(&format!(
                "module fixture.Client.\n{imports}\npub value(): {result} -> {call}.\n"
            )).unwrap();
            let report = evaluate_and_substitute_module_constants_with_interfaces(&mut module, &interfaces);
            if valid {
                assert!(report.diagnostics.is_empty(), "{imports}: {:?}", report.diagnostics);
            } else {
                assert!(report.diagnostics.iter().any(|d| d.code == "CONST_GENERIC_ARITY"), "{:?}", report.diagnostics);
            }
        }
    }
}
