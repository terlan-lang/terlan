use std::collections::BTreeMap;
use std::path::Path;

use crate::terlan_syntax::{
    SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput, SyntaxParamOutput,
};

use super::super::super::diagnostic::{LintDiagnostic, Severity};
use super::super::parse_lint_source;

const RULE_ID: &str = "TL0506";
const RULE_NAME: &str = "consistency.inline-named-atom";

/// Rejects inline atom literals that duplicate a local singleton type.
pub(crate) fn named_atom_literal_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let Ok(module) = parse_lint_source(path, source) else {
        return Vec::new();
    };
    let named_atoms = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Type {
                name,
                params,
                variants,
                ..
            } if params.is_empty() && variants.len() == 1 => {
                atom_literal_name(&variants[0].text).map(|atom| (atom.to_string(), name.clone()))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    if named_atoms.is_empty() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        collect_declaration_diagnostics(
            path,
            source,
            &declaration.payload,
            &named_atoms,
            &mut diagnostics,
        );
    }
    diagnostics
}

fn atom_literal_name(text: &str) -> Option<&str> {
    text.strip_prefix("Atom[\"")?.strip_suffix("\"]")
}

fn collect_declaration_diagnostics(
    path: &Path,
    source: &str,
    declaration: &SyntaxDeclarationPayload,
    named_atoms: &BTreeMap<String, String>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    match declaration {
        SyntaxDeclarationPayload::Constant { value, .. } => {
            collect_expr_diagnostics(path, source, value, named_atoms, diagnostics);
        }
        SyntaxDeclarationPayload::ConstFunction { params, body, .. } => {
            collect_param_diagnostics(path, source, params, named_atoms, diagnostics);
            collect_expr_diagnostics(path, source, body, named_atoms, diagnostics);
        }
        SyntaxDeclarationPayload::Type { valued_arms, .. } => {
            for arm in valued_arms {
                collect_expr_diagnostics(path, source, &arm.value, named_atoms, diagnostics);
            }
        }
        SyntaxDeclarationPayload::Struct { fields, .. } => {
            for field in fields {
                if let Some(default) = &field.default {
                    collect_expr_diagnostics(path, source, default, named_atoms, diagnostics);
                }
            }
        }
        SyntaxDeclarationPayload::Constructor { clauses, .. } => {
            for clause in clauses {
                for param in &clause.params {
                    if let Some(default) = &param.default {
                        collect_expr_diagnostics(path, source, default, named_atoms, diagnostics);
                    }
                }
                collect_expr_diagnostics(path, source, &clause.body, named_atoms, diagnostics);
            }
        }
        SyntaxDeclarationPayload::Function {
            params, clauses, ..
        } => {
            collect_param_diagnostics(path, source, params, named_atoms, diagnostics);
            for clause in clauses {
                if let Some(guard) = &clause.guard {
                    collect_expr_diagnostics(path, source, guard, named_atoms, diagnostics);
                }
                collect_expr_diagnostics(path, source, &clause.body, named_atoms, diagnostics);
            }
        }
        SyntaxDeclarationPayload::Method {
            receiver,
            params,
            clauses,
            ..
        } => {
            collect_param_diagnostics(
                path,
                source,
                std::slice::from_ref(receiver.as_ref()),
                named_atoms,
                diagnostics,
            );
            collect_param_diagnostics(path, source, params, named_atoms, diagnostics);
            for clause in clauses {
                if let Some(guard) = &clause.guard {
                    collect_expr_diagnostics(path, source, guard, named_atoms, diagnostics);
                }
                collect_expr_diagnostics(path, source, &clause.body, named_atoms, diagnostics);
            }
        }
        SyntaxDeclarationPayload::Trait {
            methods, constants, ..
        } => {
            for method in methods {
                collect_param_diagnostics(path, source, &method.params, named_atoms, diagnostics);
                if let Some(default) = &method.default_body {
                    collect_expr_diagnostics(path, source, default, named_atoms, diagnostics);
                }
            }
            for constant in constants {
                if let Some(default) = &constant.default {
                    collect_expr_diagnostics(path, source, default, named_atoms, diagnostics);
                }
            }
        }
        SyntaxDeclarationPayload::TraitImpl {
            methods, constants, ..
        } => {
            for method in methods {
                collect_param_diagnostics(path, source, &method.params, named_atoms, diagnostics);
                for clause in &method.clauses {
                    if let Some(guard) = &clause.guard {
                        collect_expr_diagnostics(path, source, guard, named_atoms, diagnostics);
                    }
                    collect_expr_diagnostics(path, source, &clause.body, named_atoms, diagnostics);
                }
            }
            for constant in constants {
                collect_expr_diagnostics(path, source, &constant.value, named_atoms, diagnostics);
            }
        }
        SyntaxDeclarationPayload::Template { props, .. } => {
            for prop in props {
                if let Some(default) = &prop.default {
                    collect_expr_diagnostics(path, source, default, named_atoms, diagnostics);
                }
            }
        }
        SyntaxDeclarationPayload::Import { .. }
        | SyntaxDeclarationPayload::Export { .. }
        | SyntaxDeclarationPayload::AnnotationSchema { .. }
        | SyntaxDeclarationPayload::Config { .. }
        | SyntaxDeclarationPayload::Raw { .. } => {}
    }
}

fn collect_param_diagnostics(
    path: &Path,
    source: &str,
    params: &[SyntaxParamOutput],
    named_atoms: &BTreeMap<String, String>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    for param in params {
        if let Some(default) = &param.default {
            collect_expr_diagnostics(path, source, default, named_atoms, diagnostics);
        }
    }
}

fn collect_expr_diagnostics(
    path: &Path,
    source: &str,
    expr: &SyntaxExprOutput,
    named_atoms: &BTreeMap<String, String>,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if expr.kind == SyntaxExprKind::Atom
        && expr.raw.is_some()
        && expr
            .text
            .as_ref()
            .is_some_and(|atom| named_atoms.contains_key(atom))
    {
        let (line, column) = source_line_column_at(source, expr.span.start);
        diagnostics.push(LintDiagnostic {
            path: path.to_path_buf(),
            line,
            column,
            rule_id: RULE_ID,
            rule_name: RULE_NAME,
            severity: Severity::Error,
            message:
                "inline atom literal duplicates a local singleton type; use its named constructor",
            fix_available: false,
        });
    }

    for child in &expr.children {
        collect_expr_diagnostics(path, source, child, named_atoms, diagnostics);
    }
    for guard in expr.let_guards.iter().filter_map(|guard| guard.as_deref()) {
        collect_expr_diagnostics(path, source, guard, named_atoms, diagnostics);
    }
    for field in &expr.fields {
        collect_expr_diagnostics(path, source, &field.value, named_atoms, diagnostics);
    }
    for clause in expr.clauses.iter().chain(&expr.catch_clauses) {
        if let Some(guard) = &clause.guard {
            collect_expr_diagnostics(path, source, guard, named_atoms, diagnostics);
        }
        collect_expr_diagnostics(path, source, &clause.body, named_atoms, diagnostics);
    }
    if let Some(after) = &expr.try_after {
        collect_expr_diagnostics(path, source, &after.trigger, named_atoms, diagnostics);
        collect_expr_diagnostics(path, source, &after.body, named_atoms, diagnostics);
    }
}

fn source_line_column_at(source: &str, index: usize) -> (usize, usize) {
    let prefix = &source[..index];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len(), |(_, tail)| tail.len())
        + 1;
    (line, column)
}
