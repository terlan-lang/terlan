use std::collections::{BTreeMap, BTreeSet};

use super::{
    SyntaxDeclarationOutput, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
    SyntaxPatternKind,
};

#[derive(Debug, Clone)]
pub(super) struct GuardPredicateDefinition {
    pub(super) params: Vec<String>,
    pub(super) body: SyntaxExprOutput,
}

pub(super) type GuardPredicateDefinitions = BTreeMap<(String, usize), GuardPredicateDefinition>;

pub(super) fn collect_guard_predicate_definitions(
    declarations: &[SyntaxDeclarationOutput],
) -> GuardPredicateDefinitions {
    let mut definitions = GuardPredicateDefinitions::new();
    let mut ambiguous = BTreeSet::new();
    for declaration in declarations {
        let SyntaxDeclarationPayload::Function {
            name,
            return_type,
            clauses,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let [clause] = clauses.as_slice() else {
            continue;
        };
        if return_type.text != "Bool"
            || clause.guard.is_some()
            || !is_call_free_guard_formula(&clause.body)
        {
            continue;
        }
        let Some(params) = clause
            .patterns
            .iter()
            .map(|pattern| {
                (pattern.kind == SyntaxPatternKind::Var)
                    .then(|| pattern.text.clone())
                    .flatten()
            })
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let key = (name.clone(), params.len());
        if definitions
            .insert(
                key.clone(),
                GuardPredicateDefinition {
                    params,
                    body: clause.body.clone(),
                },
            )
            .is_some()
        {
            ambiguous.insert(key);
        }
    }
    for key in ambiguous {
        definitions.remove(&key);
    }
    definitions
}

fn is_call_free_guard_formula(expression: &SyntaxExprOutput) -> bool {
    match expression.kind {
        SyntaxExprKind::Var | SyntaxExprKind::Int => true,
        SyntaxExprKind::UnaryOp => {
            matches!(expression.operator.as_deref(), Some("not" | "-"))
                && expression.children.len() == 1
                && expression.children.iter().all(is_call_free_guard_formula)
        }
        SyntaxExprKind::BinaryOp => {
            matches!(
                expression.operator.as_deref(),
                Some("and" | "or" | "<" | "<=" | ">" | ">=" | "==" | "!=")
            ) && expression.children.len() == 2
                && expression.children.iter().all(is_call_free_guard_formula)
        }
        _ => false,
    }
}
