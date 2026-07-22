use super::*;

#[test]
fn core_typing_spec_index_accepts_seed_rows() {
    let index = parse_spec_index(&seed_index_text()).expect("parse seed index");
    let ebnf = seed_ebnf_text();
    let lean = "Expr.int\n";
    let make_targets = BTreeSet::from(["formal-cli-phase-contract-gate".to_string()]);

    let diagnostics = validate_spec_index(&index, &ebnf, lean, &make_targets);

    assert!(diagnostics.is_empty(), "diagnostics = {diagnostics:?}");
}

#[test]
fn core_typing_spec_index_rejects_missing_required_rows() {
    let index = parse_spec_index(
        r#"
schema = 1
forms = []
"#,
    )
    .expect("parse index");

    let diagnostics = validate_spec_index(&index, "", "", &BTreeSet::new());

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("IntLiteral")),
        "expected required-row diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn core_typing_spec_index_rejects_missing_lean_anchor() {
    let index = parse_spec_index(
        r#"
schema = 1

[[forms]]
name = "IntLiteral"
ebnf = "Int"
type_rule = "T-IntLiteral"
core_ir = "Expr.int"
status = "lean-covered"
gate = "formal-cli-phase-contract-gate"
"#,
    )
    .expect("parse index");
    let make_targets = BTreeSet::from(["formal-cli-phase-contract-gate".to_string()]);

    let diagnostics = validate_spec_index(&index, "Int ::= DecimalInt .", "", &make_targets);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("has no Lean anchor")),
        "expected Lean-anchor diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn core_typing_spec_index_rejects_missing_gate_and_ebnf_rule() {
    let index = parse_spec_index(&seed_index_text()).expect("parse seed index");

    let diagnostics = validate_spec_index(&index, "", "Expr.int", &BTreeSet::new());

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("missing EBNF rule `Int`")),
        "expected EBNF diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("missing gate")),
        "expected gate diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn core_typing_spec_doc_requires_contract_terms() {
    let diagnostics = validate_spec_doc("CoreIR Preservation");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("Gamma; Delta; Kappa")),
        "expected missing judgment diagnostic, got {diagnostics:?}"
    );
}

fn seed_index_text() -> String {
    let forms = REQUIRED_FORM_NAMES
        .iter()
        .map(|name| {
            format!(
                r#"
[[forms]]
name = "{name}"
ebnf = "Int"
type_rule = "T-{name}"
core_ir = "Expr.int"
status = "lean-covered"
lean_anchor = "Expr.int"
gate = "formal-cli-phase-contract-gate"
"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("schema = 1\n{forms}")
}

fn seed_ebnf_text() -> String {
    "Int ::= DecimalInt .\n".to_string()
}
