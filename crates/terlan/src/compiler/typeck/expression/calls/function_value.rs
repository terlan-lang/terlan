use super::*;

/// Infers a dedicated function-value invocation expression.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` expression whose first child is the
///   callable expression and remaining children are call arguments.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - The invoked function's return type when the callee has a function type.
/// - `Dynamic` when the callee is malformed, non-callable, or has invalid
///   argument types.
///
/// Transformation:
/// - Infers all non-callee arguments, then delegates to the shared
///   function-value invocation checker so pipe-forward can prepend a synthetic
///   first argument without rebuilding syntax nodes.
pub(crate) fn infer_syntax_function_value_call(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let arg_types = expr
        .children
        .iter()
        .skip(1)
        .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors))
        .collect::<Vec<_>>();
    infer_syntax_function_value_call_with_arg_types(expr, &arg_types, locals, ctx, subst, errors)
}

/// Checks a function-value invocation with already inferred argument types.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` expression with a callable child.
/// - `arg_types`: argument types to check against the callee's function type.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - The callee return type with substitutions applied.
/// - `Dynamic` when the callee is not a function or arguments do not match.
///
/// Transformation:
/// - Infers the callee expression, requires a `Type::Function`, checks each
///   provided argument against the corresponding parameter with alias-aware
///   subtyping before falling back to unification, and returns the substituted
///   result type.
pub(super) fn infer_syntax_function_value_call_with_arg_types(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let Some(callee) = expr.children.first() else {
        errors.push("function-value invocation is missing a callee".to_string());
        return Type::Dynamic;
    };

    let callee_type = apply_subst(
        &infer_syntax_expr(callee, locals, ctx, subst, errors),
        subst,
    );
    match callee_type {
        Type::Function { params, ret } => {
            if params.len() != arg_types.len() {
                errors.push(format!(
                    "function arity mismatch: expected {} args, found {}",
                    params.len(),
                    arg_types.len()
                ));
                return Type::Dynamic;
            }

            for (expected, actual) in params.iter().zip(arg_types.iter()) {
                let expected_substituted = apply_subst(expected, subst);
                let actual_substituted = apply_subst(actual, subst);
                if is_subtype_with_aliases(&actual_substituted, &expected_substituted, ctx.aliases)
                {
                    continue;
                }
                if let Err(original_message) = unify(expected, actual, subst) {
                    let expected_expanded = expand_type_aliases(&expected_substituted, ctx.aliases);
                    let actual_expanded = expand_type_aliases(&actual_substituted, ctx.aliases);
                    if is_subtype_with_aliases(&actual_expanded, &expected_expanded, ctx.aliases) {
                        continue;
                    }
                    if unify(&expected_expanded, &actual_expanded, subst).is_err() {
                        errors.push(original_message);
                    }
                }
            }

            apply_subst(ret.as_ref(), subst)
        }
        Type::Dynamic => Type::Dynamic,
        other => {
            errors.push(format!(
                "function-value invocation requires function value, found {}",
                pretty_type(&other)
            ));
            Type::Dynamic
        }
    }
}
