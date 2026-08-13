use super::*;
pub(super) fn infer_syntax_let_else(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
    success_type: Type,
    binding_types: Vec<Type>,
) -> Type {
    reject_success_binding_references(expr, locals, errors);
    check_binding_exhaustiveness(expr, &binding_types, ctx, errors);

    let fallback_match_type = normalize_union(binding_types);
    let mut branches = vec![apply_subst(&success_type, subst)];
    branches.extend(expr.clauses.iter().map(|clause| {
        infer_fallback_clause(clause, &fallback_match_type, locals, ctx, subst, errors)
    }));
    normalize_union(branches)
}

fn reject_success_binding_references(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    errors: &mut Vec<String>,
) {
    let mut success_bindings = HashSet::new();
    for pattern in &expr.patterns {
        collect_comprehension_pattern_bindings(pattern, &mut success_bindings);
    }
    let outer_bindings = locals.keys().cloned().collect::<HashSet<_>>();
    for clause in &expr.clauses {
        if let Some(name) =
            clause_references_later_binding(clause, &success_bindings, &outer_bindings)
        {
            errors.push(format!(
                "let else fallback cannot reference success binding `{name}`"
            ));
        }
    }
}

fn check_binding_exhaustiveness(
    expr: &SyntaxExprOutput,
    binding_types: &[Type],
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) {
    for (index, (success_pattern, binding_type)) in
        expr.patterns.iter().zip(binding_types).enumerate()
    {
        let success_clause = crate::terlan_syntax::SyntaxClauseOutput {
            patterns: vec![success_pattern.clone()],
            guard: expr.let_guards.get(index).cloned().flatten(),
            body: Box::new(expr.children[expr.patterns.len()].clone()),
        };
        let mut clauses = Vec::with_capacity(expr.clauses.len() + 1);
        clauses.push(success_clause);
        clauses.extend(expr.clauses.iter().cloned());
        check_clauses_exhaustiveness(&clauses, binding_type, ctx, errors);
    }
}

fn infer_fallback_clause(
    clause: &crate::terlan_syntax::SyntaxClauseOutput,
    match_type: &Type,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let mut clause_locals = locals.clone();
    let mut clause_subst = subst.clone();
    if let Some(pattern) = clause.patterns.first() {
        if let Err(message) = check_syntax_pattern(
            pattern,
            match_type,
            ctx.aliases,
            Some(ctx),
            &mut clause_locals,
            &mut clause_subst,
        ) {
            errors.push(message);
        }
    }
    if let Some(guard) = clause.guard.as_ref() {
        refine_by_syntax_guard(guard, &mut clause_locals, ctx.aliases, &mut clause_subst);
        super::super::check_clause_guard_purity(
            guard,
            "let else guard",
            &clause_locals,
            ctx,
            &clause_subst,
            errors,
        );
        check_clause_guard_type(
            guard,
            "let else guard",
            &clause_locals,
            ctx,
            &mut clause_subst,
            errors,
        );
    }
    apply_subst_to_locals(&mut clause_locals, &clause_subst);
    let branch_type =
        infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
    apply_subst(&branch_type, &clause_subst)
}
