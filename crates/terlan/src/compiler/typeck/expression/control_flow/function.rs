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
            let mut variables = HashMap::new();
            let mut next_variable =
                next_function_type_var(&locals.values().cloned().collect::<Vec<_>>(), subst);
            let parameters = clause
                .patterns
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    let Some(annotation) =
                        clause.parameter_types.get(index).and_then(Option::as_ref)
                    else {
                        return Type::Dynamic;
                    };
                    match parse_type_expr(
                        &annotation.text,
                        ctx.alias_names,
                        &mut variables,
                        &mut next_variable,
                    ) {
                        Some(ty) => qualify_type_names(
                            &expand_type_aliases(&ty, ctx.aliases),
                            ctx.imported_type_names,
                        ),
                        None => {
                            errors.push(format!(
                                "invalid lambda parameter type `{}`",
                                annotation.text
                            ));
                            Type::Dynamic
                        }
                    }
                })
                .collect::<Vec<_>>();
            if !clause.parameter_types.is_empty()
                && clause.parameter_types.len() != clause.patterns.len()
            {
                errors.push(
                    "lambda parameter annotation count does not match its patterns".to_string(),
                );
            }
            for (pattern, parameter) in clause.patterns.iter().zip(&parameters) {
                if let Err(error) = check_syntax_pattern(
                    pattern,
                    parameter,
                    ctx.aliases,
                    Some(ctx),
                    &mut clause_locals,
                    &mut clause_subst,
                ) {
                    errors.push(error);
                }
            }
            let inferred =
                infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
            Type::Function {
                params: parameters
                    .iter()
                    .map(|parameter| apply_subst(parameter, &clause_subst))
                    .collect(),
                ret: Box::new(apply_subst(&inferred, &clause_subst)),
            }
        })
        .collect::<Vec<_>>();
    normalize_union(union)
}
