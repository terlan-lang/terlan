use super::*;

/// Infers a case expression.
///
/// Inputs:
/// - `expr`: syntax-output case expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Union of branch body types.
///
/// Transformation:
/// - Infers the scrutinee, type-checks each pattern against it with scoped
///   locals, applies guards, and normalizes branch body types.
pub(super) fn infer_syntax_case_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let scrutinee_type = expr
        .children
        .first()
        .map(|scrutinee| infer_syntax_expr(scrutinee, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let branches = expr
        .clauses
        .iter()
        .map(|clause| {
            let mut clause_locals = locals.clone();
            let mut clause_subst = subst.clone();
            if let Some(pattern) = clause.patterns.first() {
                if let Err(message) = check_syntax_pattern(
                    pattern,
                    &scrutinee_type,
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
            }

            let branch_type =
                infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
            apply_subst(&branch_type, &clause_subst)
        })
        .collect::<Vec<_>>();

    normalize_union(branches)
}

/// Infers a try expression.
///
/// Inputs:
/// - `expr`: syntax-output try expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Union of try body, catch body, and after body types.
///
/// Transformation:
/// - Infers the body and each catch/after clause in scoped environments while
///   preserving recoverable diagnostics.
pub(super) fn infer_syntax_try_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let protected_type = expr
        .children
        .first()
        .map(|body| infer_syntax_expr(body, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let mut branches = Vec::new();

    if expr.clauses.is_empty() {
        branches.push(protected_type.clone());
    } else {
        branches.extend(expr.clauses.iter().map(|clause| {
            let mut clause_locals = locals.clone();
            let mut clause_subst = subst.clone();
            if let Some(pattern) = clause.patterns.first() {
                if let Err(message) = check_syntax_pattern(
                    pattern,
                    &protected_type,
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
            }

            let branch_type =
                infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
            apply_subst(&branch_type, &clause_subst)
        }));
    }

    branches.extend(expr.catch_clauses.iter().map(|clause| {
        let mut clause_locals = locals.clone();
        let mut clause_subst = subst.clone();
        if let Some(pattern) = clause.patterns.first() {
            if let Err(message) = check_syntax_pattern(
                pattern,
                &Type::Dynamic,
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
        }

        let branch_type =
            infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
        apply_subst(&branch_type, &clause_subst)
    }));

    if let Some(after) = expr.try_after.as_ref() {
        let _ = infer_syntax_expr(&after.trigger, locals, ctx, subst, errors);
        let _ = infer_syntax_expr(&after.body, locals, ctx, subst, errors);
    }

    normalize_union(branches)
}

/// Infers an if expression.
///
/// Inputs:
/// - `expr`: syntax-output if expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Union of branch body types.
///
/// Transformation:
/// - Requires boolean-like conditions, refines branch locals through guards,
///   and normalizes branch result types.
pub(super) fn infer_syntax_if_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let branches = expr
        .clauses
        .iter()
        .map(|clause| {
            let mut clause_subst = subst.clone();
            if let Some(condition) = clause.guard.as_ref() {
                let condition_type =
                    infer_syntax_expr(condition, locals, ctx, &mut clause_subst, errors);
                if let Err(message) = unify(&Type::Bool, &condition_type, &mut clause_subst) {
                    errors.push(message);
                }
            }
            let branch_type =
                infer_syntax_expr(&clause.body, locals, ctx, &mut clause_subst, errors);
            apply_subst(&branch_type, &clause_subst)
        })
        .collect::<Vec<_>>();

    normalize_union(branches)
}

/// Infers a list comprehension expression.
///
/// Inputs:
/// - `expr`: syntax-output list comprehension.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - List type containing the inferred yielded element type.
///
/// Transformation:
/// - Infers the source iterable, binds generator pattern locals, checks the
///   optional guard, and infers the yielded expression in item scope.
pub(super) fn infer_syntax_list_comprehension(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let source_type = expr
        .children
        .get(1)
        .map(|source| infer_syntax_expr(source, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let element_type = match expand_type_aliases(&source_type, ctx.aliases) {
        Type::List(elem) => *elem,
        Type::Dynamic | Type::Term => Type::Dynamic,
        other => {
            if let Some(iterable_item_type) = infer_iterable_comprehension_element_type(&other, ctx)
            {
                iterable_item_type
            } else {
                errors.push(format!(
                    "list comprehension source must be List or Iterable, found {}",
                    pretty_type(&other)
                ));
                Type::Dynamic
            }
        }
    };
    let mut item_locals = locals.clone();
    let mut item_subst = subst.clone();
    if let Some(pattern) = expr.patterns.first() {
        if let Err(message) = check_syntax_pattern(
            pattern,
            &element_type,
            ctx.aliases,
            Some(ctx),
            &mut item_locals,
            &mut item_subst,
        ) {
            errors.push(message);
        }
    }
    if let Some(guard) = expr.children.get(2) {
        refine_by_syntax_guard(guard, &mut item_locals, ctx.aliases, &mut item_subst);
        let guard_type = infer_syntax_expr(guard, &item_locals, ctx, &mut item_subst, errors);
        if let Err(message) = unify(&Type::Bool, &guard_type, &mut item_subst) {
            errors.push(format!("list comprehension filter {}", message));
        }
    }
    let item_type = expr
        .children
        .first()
        .map(|item| infer_syntax_expr(item, &item_locals, ctx, &mut item_subst, errors))
        .unwrap_or(Type::Dynamic);

    Type::List(Box::new(apply_subst(&item_type, &item_subst)))
}

/// Infers the element type produced by an iterable comprehension source.
///
/// Inputs:
/// - `source_type`: inferred source collection type.
/// - `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Element type yielded by the source.
///
/// Transformation:
/// - Handles built-in list-like sources and delegates target-neutral sources to
///   visible `Iterable`/`Iterator` trait information.
fn infer_iterable_comprehension_element_type(
    source_type: &Type,
    ctx: &ExprInferContext,
) -> Option<Type> {
    let source_type = expand_type_aliases(source_type, ctx.aliases);

    if let Some(impl_args_by_type) = ctx.trait_bound_impl_type_args.get("Iterable") {
        for impl_args in impl_args_by_type {
            if impl_args.len() < 2 {
                continue;
            }

            let collection_arg = expand_type_aliases(&impl_args[0], ctx.aliases);
            let item_arg = expand_type_aliases(&impl_args[1], ctx.aliases);
            let mut local_subst = HashMap::new();

            if unify(&collection_arg, &source_type, &mut local_subst).is_ok() {
                return Some(apply_subst(&item_arg, &local_subst));
            }
        }
    }

    for bound in ctx.current_bounds.iter() {
        if bound.trait_name != "Iterable" || bound.trait_args.len() < 2 {
            continue;
        }

        let collection_arg = expand_type_aliases(&bound.trait_args[0], ctx.aliases);
        let item_arg = expand_type_aliases(&bound.trait_args[1], ctx.aliases);
        let mut local_subst = HashMap::new();

        if unify(&collection_arg, &source_type, &mut local_subst).is_ok() {
            return Some(apply_subst(&item_arg, &local_subst));
        }
    }

    None
}

/// Infers an anonymous function expression.
///
/// Inputs:
/// - `expr`: syntax-output function expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Function type or union of compatible clause function types.
///
/// Transformation:
/// - Creates scoped locals for clause patterns, infers each body, and returns a
///   function type preserving parameter count and return type.
pub(super) fn infer_syntax_fun_expr(
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

/// Infers a syntax-output let expression.
///
/// Inputs:
/// - `expr`: syntax-output let node with binding names in `patterns`, binding
///   values in `children`, and a required final body child.
/// - `locals`: local type environment visible before the let expression.
/// - `ctx`, `subst`, `errors`: inference context, substitution state, and
///   diagnostics accumulator.
///
/// Output:
/// - Inferred explicit body type.
///
/// Transformation:
/// - Infers binding values left-to-right, extending a scoped local environment
///   after each binding. The caller's `locals` map is not mutated.
pub(super) fn infer_syntax_let_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if expr.patterns.is_empty() || expr.children.len() != expr.patterns.len() + 1 {
        errors.push("malformed let expression".to_string());
        return Type::Dynamic;
    }

    let mut scoped = locals.clone();
    for (pattern, value) in expr.patterns.iter().zip(expr.children.iter()) {
        let value_type = infer_syntax_expr(value, &scoped, ctx, subst, errors);
        let binding_type = apply_subst(&value_type, subst);
        match pattern.text.as_deref() {
            Some(name) => {
                scoped.insert(name.to_string(), binding_type);
            }
            None => errors.push("malformed let binding name".to_string()),
        }
    }

    infer_syntax_expr(
        &expr.children[expr.patterns.len()],
        &scoped,
        ctx,
        subst,
        errors,
    )
}
