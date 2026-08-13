use super::*;

mod comprehension;
use comprehension::{clause_references_later_binding, collect_comprehension_pattern_bindings};
pub(super) use comprehension::{infer_syntax_let_expr, infer_syntax_list_comprehension};

mod function;
mod let_else;
pub(super) use function::infer_syntax_fun_expr;
use let_else::infer_syntax_let_else;

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
    check_case_exhaustiveness(expr, &match_type, ctx, errors);
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
                super::check_clause_guard_purity(
                    guard,
                    "case guard",
                    &clause_locals,
                    ctx,
                    &clause_subst,
                    errors,
                );
                check_clause_guard_type(
                    guard,
                    "case guard",
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
        })
        .collect::<Vec<_>>();

    normalize_union(branches)
}

/// Requires a clause guard expression to infer as Bool.
///
/// Inputs:
/// - `guard`: syntax-output guard expression.
/// - `label`: diagnostic context such as `case guard`.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - None; appends a diagnostic when the guard is not Boolean.
///
/// Transformation:
/// - Infers the guard under already-bound pattern locals and unifies it with
///   `Bool` so clause selection cannot depend on truthy/non-Boolean values.
fn check_clause_guard_type(
    guard: &SyntaxExprOutput,
    label: &str,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) {
    let guard_type = infer_syntax_expr(guard, locals, ctx, subst, errors);
    if let Err(message) = unify(&Type::Bool, &guard_type, subst) {
        errors.push(format!("{label} {message}"));
    }
}

/// Checks whether a case expression covers every finite union variant.
///
/// Inputs:
/// - `expr`: case expression clauses.
/// - `match_type`: type used for pattern validation.
/// - `ctx`: alias environment used to expand union variants.
/// - `errors`: typecheck error sink.
///
/// Output:
/// - No return value; pushes a hard typecheck error for non-exhaustive cases.
///
/// Transformation:
/// - Treats unguarded wildcard/variable patterns as exhaustive, subtracts
///   unguarded covered variants from finite unions, and ignores guarded
///   clauses for coverage because guards can reject at runtime.
fn check_case_exhaustiveness(
    expr: &SyntaxExprOutput,
    match_type: &Type,
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) {
    check_clauses_exhaustiveness(&expr.clauses, match_type, ctx, errors);
}

fn check_clauses_exhaustiveness(
    clauses: &[crate::terlan_syntax::SyntaxClauseOutput],
    match_type: &Type,
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) {
    let expanded = expand_type_aliases(match_type, ctx.aliases);
    let mut remaining = as_exhaustive_union_variants(&expanded);
    if remaining.len() <= 1 {
        return;
    }

    for clause in clauses {
        let Some(pattern) = clause.patterns.first() else {
            continue;
        };

        if clause.guard.is_some() {
            continue;
        }

        if matches!(
            pattern.kind,
            SyntaxPatternKind::Wildcard
                | SyntaxPatternKind::Ignore
                | SyntaxPatternKind::Placeholder
                | SyntaxPatternKind::Var
        ) {
            return;
        }

        remaining.retain(|variant| !syntax_pattern_subsumes_variant(pattern, variant, ctx.aliases));
        if remaining.is_empty() {
            return;
        }
    }

    if !remaining.is_empty() {
        errors.push(format!(
            "non-exhaustive case expression\nmissing:\n  {}",
            remaining
                .iter()
                .map(|variant| pretty_case_missing_variant(variant, ctx.aliases))
                .collect::<Vec<_>>()
                .join("\n  ")
        ));
    }
}

/// Renders a missing case variant using source-facing alias names when possible.
fn pretty_case_missing_variant(variant: &Type, aliases: &HashMap<String, TypeAlias>) -> String {
    let expanded_variant = expand_type_aliases(variant, aliases);
    let mut matching_aliases = aliases
        .iter()
        .filter_map(|(name, alias)| {
            if !alias.params.is_empty() {
                return None;
            }
            let expanded_alias = expand_type_aliases(&alias.body, aliases);
            if expanded_alias == expanded_variant {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    matching_aliases.sort();
    matching_aliases
        .into_iter()
        .next()
        .unwrap_or_else(|| pretty_type(variant))
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
    if !case_has_constructor_pattern(expr) {
        return None;
    }
    if matches!(scrutinee_type, Type::Dynamic | Type::Term | Type::Union(_)) {
        return None;
    }
    if matches!(
        expand_type_aliases(scrutinee_type, ctx.aliases),
        Type::Union(_)
    ) {
        return None;
    }

    let mut visible_aliases = ctx.aliases.iter().collect::<Vec<_>>();
    visible_aliases.sort_by_key(|(left, _)| *left);

    for (alias_name, alias) in visible_aliases {
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
        if !case_patterns_match_alias_variants(expr, &expanded_body, ctx.aliases) {
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

/// Returns whether a case branch needs constructor union-alias widening.
///
/// Only constructor patterns provide evidence that a concrete constructor
/// scrutinee should be widened to its visible union alias. Lists, tuples, maps,
/// and literals already carry their complete structural type; probing those
/// shapes against an unrelated generic union such as `T0 | T1 | T2` invents
/// phantom variants and false exhaustiveness errors.
fn case_has_constructor_pattern(expr: &SyntaxExprOutput) -> bool {
    expr.clauses.iter().any(|clause| {
        clause
            .patterns
            .first()
            .is_some_and(pattern_contains_constructor)
    })
}

/// Reports whether a pattern or aliased child is a named constructor pattern.
fn pattern_contains_constructor(pattern: &SyntaxPatternOutput) -> bool {
    matches!(pattern.kind, SyntaxPatternKind::Constructor)
        || (matches!(pattern.kind, SyntaxPatternKind::Alias)
            && pattern.children.iter().any(pattern_contains_constructor))
}

/// Rejects union candidates whose structural variants do not represent the
/// constructor patterns written by the case expression.
fn case_patterns_match_alias_variants(
    expr: &SyntaxExprOutput,
    expanded_alias_body: &Type,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    let Type::Union(variants) = expanded_alias_body else {
        return false;
    };
    expr.clauses.iter().all(|clause| {
        let Some(pattern) = clause.patterns.first() else {
            return true;
        };
        !matches!(pattern.kind, SyntaxPatternKind::Constructor)
            || variants
                .iter()
                .any(|variant| syntax_pattern_subsumes_variant(pattern, variant, aliases))
    })
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
        if let Ok(()) = unify(variant, scrutinee_type, &mut trial_subst) {
            *subst = trial_subst;
            return true;
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
                check_clause_guard_type(
                    guard,
                    "try guard",
                    &clause_locals,
                    ctx,
                    &mut clause_subst,
                    errors,
                );
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
            check_clause_guard_type(
                guard,
                "catch guard",
                &clause_locals,
                ctx,
                &mut clause_subst,
                errors,
            );
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
