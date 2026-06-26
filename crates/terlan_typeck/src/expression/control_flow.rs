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
    let match_type = widen_case_scrutinee_type_for_patterns(&scrutinee_type, expr, ctx, subst)
        .unwrap_or_else(|| scrutinee_type.clone());
    let branches = expr
        .clauses
        .iter()
        .map(|clause| {
            let mut clause_locals = locals.clone();
            let mut clause_subst = subst.clone();
            if let Some(pattern) = clause.patterns.first() {
                if let Err(message) = check_syntax_pattern(
                    pattern,
                    &match_type,
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

            apply_subst_to_locals(&mut clause_locals, &clause_subst);
            let branch_type =
                infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
            apply_subst(&branch_type, &clause_subst)
        })
        .collect::<Vec<_>>();

    normalize_union(branches)
}

/// Applies inferred type substitutions to local bindings.
///
/// Inputs:
/// - `locals`: local binding table for the current branch.
/// - `subst`: active type-variable substitution map.
///
/// Output:
/// - Mutated local binding table with substituted types.
///
/// Transformation:
/// - Rewrites each local type through the current unification substitution.
fn apply_subst_to_locals(locals: &mut HashMap<String, Type>, subst: &HashMap<TypeVarId, Type>) {
    for value in locals.values_mut() {
        *value = apply_subst(value, subst);
    }
}

/// Widens a concrete constructor scrutinee to a compatible visible union alias.
///
/// Inputs:
/// - `scrutinee_type`: inferred type of the matched expression.
/// - `expr`: case expression containing branch patterns.
/// - `ctx`: active expression inference context with visible aliases and
///   constructor-pattern metadata.
/// - `subst`: active type-variable substitution.
///
/// Output:
/// - A named union-alias type when every branch pattern is valid against that
///   alias and the scrutinee can inhabit one of its variants.
/// - `None` when no visible union alias is a better match or the scrutinee is
///   already a union with established type-variable substitutions.
///
/// Transformation:
/// - Tries visible non-opaque union aliases as supertypes of the scrutinee,
///   infers alias type arguments by unifying the expanded alias body with the
///   concrete non-union scrutinee, then validates all case patterns against the
///   named alias. This lets `case Some(value) { Some(x) -> ...; None -> ... }`
///   typecheck as `Option[T]` without making `Some[T]` itself equal to
///   `Option[T]` in ordinary expression inference. Existing union scrutinees
///   are left unchanged so payload variables such as `Result[A, E]` keep their
///   original `A`/`E` bindings.
fn widen_case_scrutinee_type_for_patterns(
    scrutinee_type: &Type,
    expr: &SyntaxExprOutput,
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> Option<Type> {
    if matches!(scrutinee_type, Type::Dynamic | Type::Term | Type::Union(_)) {
        return None;
    }

    for (alias_name, alias) in ctx.aliases {
        if alias.is_opaque {
            continue;
        }
        let fresh_start = next_constructor_type_var(std::slice::from_ref(scrutinee_type), subst);
        let fresh_params = alias
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| (*param, Type::Var(fresh_start + index as TypeVarId)))
            .collect::<HashMap<_, _>>();
        let fresh_body = substitute_type_vars(&alias.body, &fresh_params);
        let expanded_body = expand_type_aliases(&fresh_body, ctx.aliases);
        if !matches!(expanded_body, Type::Union(_)) {
            continue;
        }

        let mut trial_subst = subst.clone();
        if !type_inhabits_union_alias(&expanded_body, scrutinee_type, &mut trial_subst) {
            continue;
        }

        let candidate = Type::Named {
            module: None,
            name: alias_name.clone(),
            args: alias
                .params
                .iter()
                .filter_map(|param| fresh_params.get(param))
                .map(|param| apply_subst(param, &trial_subst))
                .collect(),
        };

        if case_patterns_accept_type(expr, &candidate, ctx, &trial_subst) {
            return Some(candidate);
        }
    }

    None
}

/// Returns whether a concrete type can inhabit an expanded union alias.
///
/// Inputs:
/// - `expanded_alias_body`: expanded candidate alias body.
/// - `scrutinee_type`: inferred concrete scrutinee type.
/// - `subst`: mutable type-variable substitution.
///
/// Output:
/// - `true` when the scrutinee matches one union variant.
/// - `false` for non-unions or incompatible variants.
///
/// Transformation:
/// - Tries each union variant independently and commits only the substitution
///   from the successful variant. This is intentionally narrower than general
///   `unify(Union, T)`, which checks whole-union equality in some paths.
fn type_inhabits_union_alias(
    expanded_alias_body: &Type,
    scrutinee_type: &Type,
    subst: &mut HashMap<TypeVarId, Type>,
) -> bool {
    let Type::Union(variants) = expanded_alias_body else {
        return false;
    };

    for variant in variants {
        let mut trial_subst = subst.clone();
        match unify(variant, scrutinee_type, &mut trial_subst) {
            Ok(()) => {
                *subst = trial_subst;
                return true;
            }
            Err(_) => {}
        }
    }

    false
}

/// Returns whether every branch pattern can match an expected type.
///
/// Inputs:
/// - `expr`: case expression carrying branch patterns.
/// - `expected`: candidate match type.
/// - `ctx`: expression inference context used for constructor-pattern lookup.
/// - `subst`: substitution inferred while selecting the candidate type.
///
/// Output:
/// - `true` when all branch patterns validate against `expected`.
/// - `false` when any branch pattern is incompatible.
///
/// Transformation:
/// - Runs pattern checking in cloned locals/substitution state so candidate
///   alias probing cannot leak bindings or substitutions into the actual case
///   branch inference pass.
fn case_patterns_accept_type(
    expr: &SyntaxExprOutput,
    expected: &Type,
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    expr.clauses.iter().all(|clause| {
        let Some(pattern) = clause.patterns.first() else {
            return true;
        };
        let mut locals = HashMap::new();
        let mut trial_subst = subst.clone();
        check_syntax_pattern(
            pattern,
            expected,
            ctx.aliases,
            Some(ctx),
            &mut locals,
            &mut trial_subst,
        )
        .is_ok()
    })
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
        if is_unconstrained_empty_constructor_binding(value, &binding_type) {
            let constructor = syntax_callee_name(value).unwrap_or("constructor");
            errors.push(format!(
                "empty constructor `{}()` requires an expected type; use an explicit typed helper such as `{}.new[T]()`",
                constructor, constructor
            ));
        }
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
    }

    infer_syntax_expr(
        &expr.children[expr.patterns.len()],
        &scoped,
        ctx,
        subst,
        errors,
    )
}

/// Returns whether a let binding uses an unconstrained empty constructor.
///
/// Inputs:
/// - `value`: source expression on the right side of one let binding.
/// - `binding_type`: inferred type after applying the current substitution.
///
/// Output:
/// - `true` when `value` is an empty constructor call whose type still
///   contains an unresolved inference variable.
///
/// Transformation:
/// - Recognizes only uppercase constructor-call syntax such as `Vector()` and
///   recursively inspects the inferred type for unresolved generic variables.
///   Contextual uses, such as a function returning `Vector[String]`, are not
///   rejected here because their expected type can resolve the constructor.
fn is_unconstrained_empty_constructor_binding(
    value: &SyntaxExprOutput,
    binding_type: &Type,
) -> bool {
    matches!(value.kind, SyntaxExprKind::Call)
        && value.remote.is_none()
        && value.children.len() == 1
        && syntax_callee_name(value).is_some_and(is_constructor_name)
        && type_contains_unresolved_var(binding_type)
}

/// Returns whether a type contains an unresolved inference variable.
///
/// Inputs:
/// - `ty`: inferred type tree to inspect.
///
/// Output:
/// - `true` when any nested position is `Type::Var`.
///
/// Transformation:
/// - Recursively walks structural, named, function, and fixed-array types
///   without expanding aliases or mutating inference state.
fn type_contains_unresolved_var(ty: &Type) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Apply { args, .. } => args.iter().any(type_contains_unresolved_var),
        Type::Existential { params, body } => type_contains_unresolved_free_var(body, params),
        Type::List(inner) => type_contains_unresolved_var(inner),
        Type::Tuple(items) | Type::Union(items) => items.iter().any(type_contains_unresolved_var),
        Type::Map(fields) => fields
            .iter()
            .any(|field| type_contains_unresolved_var(&field.value)),
        Type::Named { args, .. } => args.iter().any(type_contains_unresolved_var),
        Type::Function { params, ret } => {
            params.iter().any(type_contains_unresolved_var) || type_contains_unresolved_var(ret)
        }
        Type::FixedArray { elem, .. } => type_contains_unresolved_var(elem),
        Type::Int
        | Type::Float
        | Type::Number
        | Type::Binary
        | Type::Atom
        | Type::Bool
        | Type::Term
        | Type::Dynamic
        | Type::Never
        | Type::Placeholder
        | Type::LiteralAtom(_)
        | Type::LiteralInt(_) => false,
    }
}

/// Returns whether a type contains an unresolved variable not bound locally.
///
/// Inputs:
/// - `ty`: type tree to inspect.
/// - `bound`: existential variables that are intentionally scoped by `ty`.
///
/// Output:
/// - `true` when a free inference variable remains unresolved.
///
/// Transformation:
/// - Recursively mirrors `type_contains_unresolved_var` while treating
///   existential binders as local names rather than inference holes.
fn type_contains_unresolved_free_var(ty: &Type, bound: &[TypeVarId]) -> bool {
    match ty {
        Type::Var(id) => !bound.contains(id),
        Type::Apply { constructor, args } => {
            !bound.contains(constructor)
                || args
                    .iter()
                    .any(|arg| type_contains_unresolved_free_var(arg, bound))
        }
        Type::Existential { params, body } => {
            let mut nested_bound = bound.to_vec();
            nested_bound.extend(params);
            type_contains_unresolved_free_var(body, &nested_bound)
        }
        Type::List(inner) => type_contains_unresolved_free_var(inner, bound),
        Type::Tuple(items) | Type::Union(items) => items
            .iter()
            .any(|item| type_contains_unresolved_free_var(item, bound)),
        Type::Map(fields) => fields
            .iter()
            .any(|field| type_contains_unresolved_free_var(&field.value, bound)),
        Type::Named { args, .. } => args
            .iter()
            .any(|arg| type_contains_unresolved_free_var(arg, bound)),
        Type::Function { params, ret } => {
            params
                .iter()
                .any(|param| type_contains_unresolved_free_var(param, bound))
                || type_contains_unresolved_free_var(ret, bound)
        }
        Type::FixedArray { elem, .. } => type_contains_unresolved_free_var(elem, bound),
        Type::Int
        | Type::Float
        | Type::Number
        | Type::Binary
        | Type::Atom
        | Type::Bool
        | Type::Term
        | Type::Dynamic
        | Type::Never
        | Type::Placeholder
        | Type::LiteralAtom(_)
        | Type::LiteralInt(_) => false,
    }
}
