use super::*;

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
pub(crate) fn infer_syntax_list_comprehension(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let mut item_locals = locals.clone();
    let mut item_subst = subst.clone();
    for (index, (pattern, source)) in expr
        .patterns
        .iter()
        .zip(expr.children.iter().skip(1))
        .enumerate()
    {
        let mut later_bindings = HashSet::new();
        for later_pattern in expr.patterns.iter().skip(index + 1) {
            collect_comprehension_pattern_bindings(later_pattern, &mut later_bindings);
        }
        if let Some(name) = expression_references_later_binding(source, &later_bindings) {
            errors.push(format!(
                "list comprehension source references later generator binding `{name}`"
            ));
        }
        let source_type = infer_syntax_expr(source, &item_locals, ctx, &mut item_subst, errors);
        let element_type = comprehension_element_type(&source_type, ctx, errors);
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
    let mut lift_constructor: Option<Type> = None;
    for guard in expr.children.iter().skip(expr.patterns.len() + 1) {
        refine_by_syntax_guard(guard, &mut item_locals, ctx.aliases, &mut item_subst);
        super::check_clause_guard_purity(
            guard,
            "list comprehension filter",
            &item_locals,
            ctx,
            &item_subst,
            errors,
        );
        let guard_type = infer_syntax_expr(guard, &item_locals, ctx, &mut item_subst, errors);
        let mut bool_subst = item_subst.clone();
        if unify(&Type::Bool, &guard_type, &mut bool_subst).is_ok() {
            item_subst = bool_subst;
        } else if !is_completed_comprehension_guard_result(&guard_type, ctx.aliases) {
            match comprehension_guard_lift_constructor(&guard_type, ctx) {
                Ok(constructor) => {
                    if let Some(existing) = &lift_constructor {
                        if existing != &constructor {
                            errors.push(format!(
                                "list comprehension guards declare conflicting lift containers `{}` and `{}`",
                                pretty_type(existing),
                                pretty_type(&constructor)
                            ));
                        }
                    } else {
                        lift_constructor = Some(constructor);
                    }
                }
                Err(message) => errors.push(message),
            }
        }
    }
    let item_type = expr
        .children
        .first()
        .map(|item| infer_syntax_expr(item, &item_locals, ctx, &mut item_subst, errors))
        .unwrap_or(Type::Dynamic);

    let list_type = Type::List(Box::new(apply_subst(&item_type, &item_subst)));
    match lift_constructor {
        Some(constructor) => apply_comprehension_lift_constructor(&constructor, list_type)
            .unwrap_or_else(|| {
                errors.push(format!(
                    "list comprehension GuardResult declares unregistered lift container `{}`",
                    pretty_type(&constructor)
                ));
                Type::Dynamic
            }),
        None => list_type,
    }
}

/// Resolves the unique `GuardResult[Result, Container]` implementation.
fn comprehension_guard_lift_constructor(
    guard_type: &Type,
    ctx: &ExprInferContext<'_>,
) -> Result<Type, String> {
    let Some(candidates) = ctx.trait_bound_impl_type_args.get("GuardResult") else {
        return Err(format!(
            "list comprehension filter type `{}` does not implement GuardResult",
            pretty_type(guard_type)
        ));
    };
    let mut matches = Vec::new();
    for args in candidates {
        let [result, container] = args.as_slice() else {
            continue;
        };
        let mut candidate_subst = HashMap::new();
        let direct_match = unify(result, guard_type, &mut candidate_subst).is_ok();
        let expanded_match = if direct_match {
            false
        } else {
            candidate_subst.clear();
            unify(
                &expand_type_aliases(result, ctx.aliases),
                &expand_type_aliases(guard_type, ctx.aliases),
                &mut candidate_subst,
            )
            .is_ok()
        };
        if direct_match || expanded_match {
            let container = apply_subst(container, &candidate_subst);
            if !matches.contains(&container) {
                matches.push(container);
            }
        }
    }
    match matches.as_slice() {
        [container] => Ok(container.clone()),
        [] => Err(format!(
            "list comprehension filter type `{}` does not implement GuardResult",
            pretty_type(guard_type)
        )),
        _ => Err(format!(
            "list comprehension filter type `{}` has ambiguous GuardResult lift containers",
            pretty_type(guard_type)
        )),
    }
}

/// Applies a declared unary lift container to the completed list result.
fn apply_comprehension_lift_constructor(constructor: &Type, result: Type) -> Option<Type> {
    match constructor {
        Type::Named { module, name, args } if args.is_empty() => Some(Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: vec![result],
        }),
        Type::Var(constructor) => Some(Type::Apply {
            constructor: *constructor,
            args: vec![result],
        }),
        _ => None,
    }
}

/// Returns whether a filter result is the completed core GuardResult shape.
fn is_completed_comprehension_guard_result(
    guard_type: &Type,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    matches!(
        expand_type_aliases(guard_type, aliases),
        Type::Tuple(items)
            if matches!(items.as_slice(), [Type::LiteralAtom(tag), Type::Bool] if tag == COMPLETED_GUARD_RESULT_TAG)
    )
}

/// Adds every name introduced by a comprehension pattern to the binding set.
pub(super) fn collect_comprehension_pattern_bindings(
    pattern: &SyntaxPatternOutput,
    bindings: &mut HashSet<String>,
) {
    if matches!(
        pattern.kind,
        SyntaxPatternKind::Var | SyntaxPatternKind::Alias | SyntaxPatternKind::StringCapture
    ) {
        if let Some(name) = &pattern.text {
            bindings.insert(name.clone());
        }
    }
    for child in &pattern.children {
        collect_comprehension_pattern_bindings(child, bindings);
    }
    for field in &pattern.fields {
        collect_comprehension_pattern_bindings(&field.value, bindings);
    }
}

/// Finds a generator binding referenced before its declaration.
fn expression_references_later_binding(
    expr: &SyntaxExprOutput,
    later_bindings: &HashSet<String>,
) -> Option<String> {
    expression_references_later_binding_with_bound(expr, later_bindings, &HashSet::new())
}

/// Finds forward references while respecting bindings introduced by nested forms.
fn expression_references_later_binding_with_bound(
    expr: &SyntaxExprOutput,
    later_bindings: &HashSet<String>,
    bound: &HashSet<String>,
) -> Option<String> {
    if expr.kind == SyntaxExprKind::Let {
        let mut let_bound = bound.clone();
        for (index, (pattern, value)) in expr.patterns.iter().zip(&expr.children).enumerate() {
            if let Some(name) =
                expression_references_later_binding_with_bound(value, later_bindings, &let_bound)
            {
                return Some(name);
            }
            collect_comprehension_pattern_bindings(pattern, &mut let_bound);
            if let Some(name) = expr
                .let_guards
                .get(index)
                .and_then(Option::as_deref)
                .and_then(|guard| {
                    expression_references_later_binding_with_bound(
                        guard,
                        later_bindings,
                        &let_bound,
                    )
                })
            {
                return Some(name);
            }
        }
        return expr
            .clauses
            .iter()
            .find_map(|clause| clause_references_later_binding(clause, later_bindings, bound))
            .or_else(|| {
                expr.children.get(expr.patterns.len()).and_then(|body| {
                    expression_references_later_binding_with_bound(body, later_bindings, &let_bound)
                })
            });
    }
    if expr.kind == SyntaxExprKind::ListComprehension {
        let mut comprehension_bound = bound.clone();
        for (pattern, source) in expr.patterns.iter().zip(expr.children.iter().skip(1)) {
            if let Some(name) = expression_references_later_binding_with_bound(
                source,
                later_bindings,
                &comprehension_bound,
            ) {
                return Some(name);
            }
            collect_comprehension_pattern_bindings(pattern, &mut comprehension_bound);
        }
        return expr
            .children
            .get(expr.patterns.len() + 1)
            .and_then(|guard| {
                expression_references_later_binding_with_bound(
                    guard,
                    later_bindings,
                    &comprehension_bound,
                )
            })
            .or_else(|| {
                expr.children.first().and_then(|yielded| {
                    expression_references_later_binding_with_bound(
                        yielded,
                        later_bindings,
                        &comprehension_bound,
                    )
                })
            });
    }
    if expr.kind == SyntaxExprKind::Var {
        if let Some(name) = expr
            .text
            .as_ref()
            .filter(|name| later_bindings.contains(*name) && !bound.contains(*name))
        {
            return Some(name.clone());
        }
    }
    expr.children
        .iter()
        .find_map(|child| {
            expression_references_later_binding_with_bound(child, later_bindings, bound)
        })
        .or_else(|| {
            expr.let_guards.iter().find_map(|guard| {
                guard.as_deref().and_then(|guard| {
                    expression_references_later_binding_with_bound(guard, later_bindings, bound)
                })
            })
        })
        .or_else(|| {
            expr.fields.iter().find_map(|field| {
                expression_references_later_binding_with_bound(&field.value, later_bindings, bound)
            })
        })
        .or_else(|| {
            expr.clauses
                .iter()
                .find_map(|clause| clause_references_later_binding(clause, later_bindings, bound))
        })
        .or_else(|| {
            expr.catch_clauses
                .iter()
                .find_map(|clause| clause_references_later_binding(clause, later_bindings, bound))
        })
        .or_else(|| {
            expr.try_after.as_ref().and_then(|after| {
                expression_references_later_binding_with_bound(
                    &after.trigger,
                    later_bindings,
                    bound,
                )
                .or_else(|| {
                    expression_references_later_binding_with_bound(
                        &after.body,
                        later_bindings,
                        bound,
                    )
                })
            })
        })
}

/// Finds forward references in one guarded clause with pattern-local bindings.
pub(super) fn clause_references_later_binding(
    clause: &crate::terlan_syntax::SyntaxClauseOutput,
    later_bindings: &HashSet<String>,
    bound: &HashSet<String>,
) -> Option<String> {
    let mut clause_bound = bound.clone();
    for pattern in &clause.patterns {
        collect_comprehension_pattern_bindings(pattern, &mut clause_bound);
    }
    clause
        .guard
        .as_deref()
        .and_then(|guard| {
            expression_references_later_binding_with_bound(guard, later_bindings, &clause_bound)
        })
        .or_else(|| {
            expression_references_later_binding_with_bound(
                &clause.body,
                later_bindings,
                &clause_bound,
            )
        })
}

/// Resolves the item type yielded by one list-comprehension source.
fn comprehension_element_type(
    source_type: &Type,
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) -> Type {
    match expand_type_aliases(source_type, ctx.aliases) {
        Type::List(elem) => *elem,
        Type::Named { module, name, args }
            if module.as_deref() == Some("std.range.Range")
                && name == "Range"
                && args.is_empty() =>
        {
            Type::Int
        }
        Type::Dynamic | Type::Term => Type::Dynamic,
        other => infer_iterable_comprehension_element_type(&other, ctx).unwrap_or_else(|| {
            errors.push(format!(
                "list comprehension source must be List or Iterable, found {}",
                pretty_type(&other)
            ));
            Type::Dynamic
        }),
    }
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

/// Infers a syntax-output let expression.
///
/// Inputs:
/// - `expr`: syntax-output let node with binding patterns in `patterns`,
///   binding values in `children`, and a required final body child.
/// - `locals`: local type environment visible before the let expression.
/// - `ctx`, `subst`, `errors`: inference context, substitution state, and
///   diagnostics accumulator.
///
/// Output:
/// - Inferred explicit body type.
///
/// Transformation:
/// - Infers binding values left-to-right, type-checks each pattern against its
///   value, and extends a scoped local environment after each binding. The
///   caller's `locals` map is not mutated.
pub(crate) fn infer_syntax_let_expr(
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
    let mut binding_types = Vec::with_capacity(expr.patterns.len());
    for (index, (pattern, value)) in expr.patterns.iter().zip(expr.children.iter()).enumerate() {
        let value_type = infer_syntax_expr(value, &scoped, ctx, subst, errors);
        let binding_type = apply_subst(&value_type, subst);
        binding_types.push(binding_type.clone());
        if let Err(message) = check_syntax_pattern(
            pattern,
            &binding_type,
            ctx.aliases,
            Some(ctx),
            &mut scoped,
            subst,
        ) {
            errors.push(message);
        }
        if let Some(guard) = expr.let_guards.get(index).and_then(Option::as_deref) {
            refine_by_syntax_guard(guard, &mut scoped, ctx.aliases, subst);
            check_clause_guard_purity(guard, "let success guard", &scoped, ctx, subst, errors);
            check_clause_guard_type(guard, "let success guard", &scoped, ctx, subst, errors);
            apply_subst_to_locals(&mut scoped, subst);
        }
    }

    let success_type = infer_syntax_expr(
        &expr.children[expr.patterns.len()],
        &scoped,
        ctx,
        subst,
        errors,
    );
    if expr.clauses.is_empty() {
        return success_type;
    }

    infer_syntax_let_else(
        expr,
        locals,
        ctx,
        subst,
        errors,
        success_type,
        binding_types,
    )
}
