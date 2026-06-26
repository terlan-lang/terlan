use super::*;

/// Infers a bracket index read.
///
/// Inputs:
/// - `expr`: syntax-output index expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Element type for fixed arrays or trait-selected `IndexGet` return type.
///
/// Transformation:
/// - Checks fixed-array bounds when possible, otherwise delegates to
///   `IndexGet.get_at` trait dispatch.
pub(super) fn infer_syntax_index(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let target_type = expr
        .children
        .first()
        .map(|value| infer_syntax_expr(value, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let index_type = expr
        .children
        .get(1)
        .map(|index| infer_syntax_expr(index, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);

    match target_type {
        Type::FixedArray { size, elem } => match index_type {
            Type::LiteralInt(index) => {
                if index < 0 || index as usize >= size {
                    errors.push(format!(
                        "index {} is out of bounds for {}\nvalid indices: 0..{}",
                        index,
                        pretty_type(&Type::FixedArray {
                            size,
                            elem: elem.clone(),
                        }),
                        size.saturating_sub(1)
                    ));
                }
                *elem
            }
            Type::Int => *elem,
            Type::Var(_) | Type::Dynamic | Type::Number => *elem,
            _ => {
                errors.push(format!("expected Int found {}", pretty_type(&index_type)));
                Type::Dynamic
            }
        },
        _ => infer_index_get_trait_call(&target_type, &index_type, ctx, subst, errors)
            .unwrap_or(Type::Dynamic),
    }
}

/// Infers bracket assignment through `IndexSet`.
///
/// Inputs:
/// - `expr`: syntax-output `IndexAssign` node with collection, index, and value
///   children.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Unit` when a visible `IndexSet.set_at(collection, index, value)`
///   implementation matches.
/// - `Dynamic` when the node shape is malformed or no matching implementation
///   can be found.
///
/// Transformation:
/// - Treats `collection[index] = value` as assignment syntax backed by the
///   same trait system as explicit calls. Successful assignment expressions
///   have `Unit` type; later lowering decides how mutable receiver rebinding is
///   represented for the selected target.
pub(super) fn infer_syntax_index_assign(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if !matches!(expr.kind, SyntaxExprKind::IndexAssign) || expr.children.len() != 3 {
        errors.push("malformed indexed assignment expression".to_string());
        return Type::Dynamic;
    }

    let collection_type = infer_syntax_expr(&expr.children[0], locals, ctx, subst, errors);
    let index_type = infer_syntax_expr(&expr.children[1], locals, ctx, subst, errors);
    let value_type = infer_syntax_expr(&expr.children[2], locals, ctx, subst, errors);

    match infer_index_set_trait_call(
        &collection_type,
        &index_type,
        &value_type,
        ctx,
        subst,
        errors,
    ) {
        Some(return_type) => {
            if !is_unit_named_type(&return_type) && !is_unit_literal_type(&return_type) {
                errors.push(format!(
                    "IndexSet.set_at must return Unit, found {}",
                    pretty_type(&return_type)
                ));
            }
            Type::Named {
                module: None,
                name: "Unit".to_string(),
                args: Vec::new(),
            }
        }
        None => {
            errors.push(format!(
                "cannot find IndexSet.set_at implementation for [{}, {}, {}]",
                pretty_type(&collection_type),
                pretty_type(&index_type),
                pretty_type(&value_type)
            ));
            Type::Dynamic
        }
    }
}

/// Infers non-fixed-array bracket reads through `IndexGet`.
///
/// Inputs:
/// - `target_type`: inferred type of the expression before `[index]`.
/// - `index_type`: inferred type of the bracket index expression.
/// - `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(return_type)` when exactly one visible `IndexGet.get_at` candidate
///   matches `(target_type, index_type)`.
/// - `None` when no visible `IndexGet` trait or impl applies.
///
/// Transformation:
/// - Treats `collection[index]` as a compiler-owned shorthand for an
///   `IndexGet.get_at(collection, index)` trait call. The parser remains
///   collection-agnostic while typechecking uses the same conformance metadata
///   as explicit trait method calls.
fn infer_index_get_trait_call(
    target_type: &Type,
    index_type: &Type,
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let arg_types = vec![
        apply_subst(target_type, subst),
        apply_subst(index_type, subst),
    ];
    let mut selected = None::<(Type, HashMap<TypeVarId, Type>)>;
    let mut matches = 0usize;

    for ((trait_name, method_name), impls) in ctx.trait_method_calls {
        if method_name != "get_at" || !is_index_get_trait_name(trait_name) {
            continue;
        }

        for impl_candidate in impls {
            if !trait_method_candidate_matches_call(impl_candidate, &arg_types, ctx, subst) {
                continue;
            }

            let mut trial_subst = subst.clone();
            if let Ok(return_type) = infer_function_with_bounds(
                &impl_candidate.scheme,
                None,
                &arg_types,
                ctx,
                &mut trial_subst,
            ) {
                matches += 1;
                if selected.is_none() {
                    selected = Some((return_type, trial_subst));
                } else {
                    break;
                }
            }
        }

        if matches > 1 {
            break;
        }

        if matches == 0 {
            if let Some(return_type) = infer_trait_method_call_from_current_bounds(
                trait_name,
                method_name,
                &arg_types,
                ctx,
                subst,
            ) {
                return Some(return_type);
            }
        }
    }

    match (matches, selected) {
        (0, _) => None,
        (1, Some((return_type, inferred_subst))) => {
            *subst = inferred_subst;
            Some(return_type)
        }
        _ => {
            errors.push(format!(
                "ambiguous IndexGet.get_at implementation for [{}]",
                arg_types
                    .iter()
                    .map(pretty_type)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            Some(Type::Dynamic)
        }
    }
}

/// Infers bracket assignments through `IndexSet`.
///
/// Inputs:
/// - `collection_type`: inferred type before `[index]`.
/// - `index_type`: inferred bracket index type.
/// - `value_type`: inferred assigned value type.
/// - `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(return_type)` when exactly one visible `IndexSet.set_at` candidate
///   matches `(collection_type, index_type, value_type)`.
/// - `None` when no visible `IndexSet` trait or impl applies.
///
/// Transformation:
/// - Reuses the trait-method call resolver shape from `IndexGet`, but includes
///   the assigned value as the third call argument and filters on the canonical
///   `IndexSet` trait name.
fn infer_index_set_trait_call(
    collection_type: &Type,
    index_type: &Type,
    value_type: &Type,
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let arg_types = vec![
        apply_subst(collection_type, subst),
        apply_subst(index_type, subst),
        apply_subst(value_type, subst),
    ];
    let mut selected = None::<(Type, HashMap<TypeVarId, Type>)>;
    let mut matches = 0usize;

    for ((trait_name, method_name), impls) in ctx.trait_method_calls {
        if method_name != "set_at" || !is_index_set_trait_name(trait_name) {
            continue;
        }

        for impl_candidate in impls {
            if !trait_method_candidate_matches_call(impl_candidate, &arg_types, ctx, subst) {
                continue;
            }

            let mut trial_subst = subst.clone();
            if let Ok(return_type) = infer_function_with_bounds(
                &impl_candidate.scheme,
                None,
                &arg_types,
                ctx,
                &mut trial_subst,
            ) {
                matches += 1;
                if selected.is_none() {
                    selected = Some((return_type, trial_subst));
                } else {
                    break;
                }
            }
        }

        if matches > 1 {
            break;
        }

        if matches == 0 {
            if let Some(return_type) = infer_trait_method_call_from_current_bounds(
                trait_name,
                method_name,
                &arg_types,
                ctx,
                subst,
            ) {
                return Some(return_type);
            }
        }
    }

    match (matches, selected) {
        (0, _) => None,
        (1, Some((return_type, inferred_subst))) => {
            *subst = inferred_subst;
            Some(return_type)
        }
        _ => {
            errors.push(format!(
                "ambiguous IndexSet.set_at implementation for [{}]",
                arg_types
                    .iter()
                    .map(pretty_type)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            Some(Type::Dynamic)
        }
    }
}

/// Returns whether a trait name denotes the standard index-read contract.
///
/// Inputs:
/// - `trait_name`: local or qualified trait name from conformance metadata.
///
/// Output:
/// - `true` for `IndexGet` and qualified names ending in `.IndexGet`.
///
/// Transformation:
/// - Keeps bracket syntax independent of import spelling while still requiring
///   the resolved trait to use the canonical `IndexGet` name.
fn is_index_get_trait_name(trait_name: &str) -> bool {
    trait_name == "IndexGet" || trait_name.ends_with(".IndexGet")
}

/// Returns whether a trait name denotes the standard index-write contract.
///
/// Inputs:
/// - `trait_name`: local or qualified trait name from conformance metadata.
///
/// Output:
/// - `true` for `IndexSet` and qualified names ending in `.IndexSet`.
///
/// Transformation:
/// - Keeps assignment syntax independent of import spelling while still
///   requiring the resolved trait to use the canonical `IndexSet` name.
fn is_index_set_trait_name(trait_name: &str) -> bool {
    trait_name == "IndexSet" || trait_name.ends_with(".IndexSet")
}
