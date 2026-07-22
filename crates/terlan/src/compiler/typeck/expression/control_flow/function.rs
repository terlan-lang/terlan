use super::*;

/// Infers an anonymous function expression.
pub(crate) fn infer_syntax_fun_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let union = expr
        .clauses
        .iter()
        .map(|clause| {
            let mut clause_locals = locals.clone();
            let mut clause_subst = subst.clone();
            for pattern in &clause.patterns {
                let _ = check_syntax_pattern(
                    pattern,
                    &Type::Dynamic,
                    ctx.aliases,
                    Some(ctx),
                    &mut clause_locals,
                    &mut clause_subst,
                );
            }
            let inferred =
                infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
            Type::Function {
                params: vec![Type::Dynamic; clause.patterns.len()],
                ret: Box::new(apply_subst(&inferred, &clause_subst)),
            }
        })
        .collect::<Vec<_>>();
    normalize_union(union)
}
