use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::terlan_quality::QualityResult;

const SPEC_DOC: &str = "docs/compiler/TERLAN_CORE_TYPING_SPEC.md";
const SPEC_INDEX: &str = "docs/compiler/type_spec/terlan_core_typing_spec.toml";
const EBNF_SPEC: &str = "docs/grammar/TERLAN_SYNTAX_SPEC.ebnf";
const LEAN_CONFORMANCE_DOC: &str = "docs/compiler/CORE_IR_LEAN_CONFORMANCE.md";

const VALID_STATUSES: &[&str] = &[
    "lean-covered",
    "proof-model-required",
    "runtime-boundary",
    "artifact-only",
];

const REQUIRED_DOC_TERMS: &[&str] = &[
    "Gamma; Delta; Kappa; Constraints |- expr : Type",
    "CoreIR Preservation",
    "lean-covered",
    "proof-model-required",
    "runtime-boundary",
    "artifact-only",
    "make core-typing-spec-check",
];

const REQUIRED_FORM_NAMES: &[&str] = &[
    "CaseExpression",
    "ConstructorPattern",
    "IntLiteral",
    "ListExpression",
    "NamedCall",
    "StringLiteral",
    "TraitTargetCall",
    "TupleExpression",
    "TypeParameters",
    "VariableReference",
];

/// Summary produced by the core typing spec gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreTypingSpecSummary {
    pub classified_form_count: usize,
}

#[derive(Debug, Deserialize)]
struct CoreTypingSpecIndex {
    schema: u32,
    forms: Vec<CoreTypingSpecForm>,
}

#[derive(Debug, Deserialize)]
struct CoreTypingSpecForm {
    name: String,
    ebnf: String,
    type_rule: String,
    core_ir: String,
    status: String,
    lean_anchor: Option<String>,
    gate: String,
}

/// Runs the core typing specification gate.
///
/// Inputs:
/// - `root`: release repository root.
///
/// Output:
/// - Success summary when the human spec and TOML index are synchronized.
/// - Stable diagnostics when initial core forms are missing, unclassified, or
///   point at missing EBNF/gate/Lean anchors.
///
/// Transformation:
/// - Treats the type system as a release contract instead of implicit compiler
///   behavior.
pub fn run_core_typing_spec(root: &Path) -> QualityResult<CoreTypingSpecSummary> {
    let doc = read_text(root, SPEC_DOC)?;
    let index_text = read_text(root, SPEC_INDEX)?;
    let ebnf = read_text(root, EBNF_SPEC)?;
    let lean_conformance = read_text(root, LEAN_CONFORMANCE_DOC)?;
    let make_targets = collect_make_targets(root)?;

    let mut diagnostics = validate_spec_doc(&doc);
    let index = parse_spec_index(&index_text)?;
    diagnostics.extend(validate_spec_index(
        &index,
        &ebnf,
        &lean_conformance,
        &make_targets,
    ));

    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    Ok(CoreTypingSpecSummary {
        classified_form_count: index.forms.len(),
    })
}

fn validate_spec_doc(doc: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for term in REQUIRED_DOC_TERMS {
        if !doc.contains(term) {
            diagnostics.push(format!("`{SPEC_DOC}` is missing required term `{term}`"));
        }
    }
    diagnostics
}

fn parse_spec_index(text: &str) -> QualityResult<CoreTypingSpecIndex> {
    basic_toml::from_str(text).map_err(|err| format!("`{SPEC_INDEX}` is invalid TOML: {err}"))
}

fn validate_spec_index(
    index: &CoreTypingSpecIndex,
    ebnf: &str,
    lean_conformance: &str,
    make_targets: &BTreeSet<String>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if index.schema != 1 {
        diagnostics.push(format!(
            "`{SPEC_INDEX}` schema must be 1, found {}",
            index.schema
        ));
    }

    let mut names = BTreeSet::new();
    for form in &index.forms {
        if !names.insert(form.name.clone()) {
            diagnostics.push(format!("duplicate core typing spec row `{}`", form.name));
        }
        diagnostics.extend(validate_form(form, ebnf, lean_conformance, make_targets));
    }

    for required in REQUIRED_FORM_NAMES {
        if !names.contains(*required) {
            diagnostics.push(format!(
                "`{SPEC_INDEX}` is missing required initial core form `{required}`"
            ));
        }
    }
    diagnostics
}

fn validate_form(
    form: &CoreTypingSpecForm,
    ebnf: &str,
    lean_conformance: &str,
    make_targets: &BTreeSet<String>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (field, value) in [
        ("name", form.name.as_str()),
        ("ebnf", form.ebnf.as_str()),
        ("type_rule", form.type_rule.as_str()),
        ("core_ir", form.core_ir.as_str()),
        ("status", form.status.as_str()),
        ("gate", form.gate.as_str()),
    ] {
        if value.trim().is_empty() {
            diagnostics.push(format!(
                "`{SPEC_INDEX}` row `{}` has empty `{field}`",
                form.name
            ));
        }
    }
    if !VALID_STATUSES.contains(&form.status.as_str()) {
        diagnostics.push(format!(
            "`{SPEC_INDEX}` row `{}` has invalid status `{}`",
            form.name, form.status
        ));
    }
    if !ebnf_rule_exists(ebnf, &form.ebnf) {
        diagnostics.push(format!(
            "`{SPEC_INDEX}` row `{}` references missing EBNF rule `{}`",
            form.name, form.ebnf
        ));
    }
    if !make_targets.contains(&form.gate) {
        diagnostics.push(format!(
            "`{SPEC_INDEX}` row `{}` references missing gate `{}`",
            form.name, form.gate
        ));
    }
    match (form.status.as_str(), form.lean_anchor.as_deref()) {
        ("lean-covered", Some(anchor)) if lean_conformance.contains(anchor) => {}
        ("lean-covered", Some(anchor)) => diagnostics.push(format!(
            "`{SPEC_INDEX}` row `{}` references missing Lean anchor `{anchor}`",
            form.name
        )),
        ("lean-covered", None) => diagnostics.push(format!(
            "`{SPEC_INDEX}` row `{}` is lean-covered but has no Lean anchor",
            form.name
        )),
        (_, Some(anchor)) if anchor.trim().is_empty() => diagnostics.push(format!(
            "`{SPEC_INDEX}` row `{}` has an empty Lean anchor",
            form.name
        )),
        _ => {}
    }
    diagnostics
}

fn ebnf_rule_exists(ebnf: &str, rule: &str) -> bool {
    ebnf.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with(rule) && trimmed[rule.len()..].trim_start().starts_with("::=")
    })
}

fn collect_make_targets(root: &Path) -> QualityResult<BTreeSet<String>> {
    let mut targets = BTreeSet::new();
    for path in [
        "Makefile",
        "crates/terlan/cli.mk",
        "std/stdlib.mk",
        "editors/editor.mk",
    ] {
        let text = read_text(root, path)?;
        for line in text.lines() {
            let Some((name, _rest)) = line.split_once(':') else {
                continue;
            };
            let name = name.trim();
            if !name.is_empty()
                && !name.contains(char::is_whitespace)
                && !name.starts_with('.')
                && !name.contains('$')
            {
                targets.insert(name.to_string());
            }
        }
    }
    Ok(targets)
}

fn read_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read file: {err}", path.display()))
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[core-typing-spec] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "core_typing_spec_test.rs"]
mod core_typing_spec_test;
