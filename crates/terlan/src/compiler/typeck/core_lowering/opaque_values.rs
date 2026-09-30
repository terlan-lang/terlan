//! Erase checked, representation-bearing opaque constructors inside their owner.

use super::*;

pub(super) fn lower(
    module: &SyntaxModuleOutput,
    clauses: &mut HashMap<CoreCallableSignature, Vec<CoreFunctionClause>>,
) {
    let explicit = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Constructor { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let aliases = collect_syntax_type_aliases(module)
        .into_iter()
        .filter(|(name, alias)| {
            alias.is_opaque
                && alias.params.is_empty()
                && !explicit.contains(name.as_str())
                && !matches!(alias.body, Type::Dynamic)
        })
        .map(|(name, _)| name)
        .collect::<HashSet<_>>();
    if aliases.is_empty() {
        return;
    }
    for clause in clauses.values_mut().flatten() {
        if let Some(guard) = &mut clause.guard {
            lower_summary(guard, &aliases);
        }
        lower_summary(&mut clause.body, &aliases);
    }
}

fn lower_summary(summary: &mut CoreExprSummary, aliases: &HashSet<String>) {
    for child in &mut summary.children {
        lower_summary(child, aliases);
    }
    if let Some(expr) = &mut summary.core_expr {
        crate::terlan_typeck::core_ir::visit_core_expr_mut(expr, &mut |expr| {
            let CoreExpr::ConstructorCall {
                constructor,
                constructor_identity: None,
                type_args,
                args,
            } = expr
            else {
                return;
            };
            if !aliases.contains(constructor) || !type_args.is_empty() || args.len() != 1 {
                return;
            }
            // Type checking has validated the representation argument. A cast
            // retains the nominal result until ordinary opaque-alias expansion.
            *expr = CoreExpr::Cast {
                target_type: CoreType::Named(constructor.clone()),
                expr: Box::new(args.remove(0)),
            };
        });
    }
}
