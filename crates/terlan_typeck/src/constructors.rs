use std::collections::HashMap;

use terlan_syntax::{SyntaxExprKind, SyntaxExprOutput};

use crate::{
    apply_subst, call_has_named_args, complete_defaulted_call_arg_types, expand_type_aliases,
    infer_syntax_expr, instantiate_constructor_scheme, instantiate_type, is_constructor_name,
    next_constructor_type_var, normalize_union, reorder_named_call_arg_types, substitute_type_vars,
    supplied_named_parameter_slots, syntax_callee_name, unify, validate_named_call_args,
    ConstructorScheme, ExprInferContext, Type, TypeAlias, TypeVarId,
};

/// Infers a constructor call from explicit constructors or alias constructors.
///
/// Inputs:
/// - `name`: constructor name from source call syntax.
/// - `args`: inferred argument types.
/// - `arg_names`: optional source argument names parallel to `args`.
/// - `ctx`: expression inference context containing constructor and alias maps.
/// - `subst`: active type-variable substitution.
/// - `errors`: mutable diagnostic text sink.
///
/// Output:
/// - `Some(Type)` when a constructor-like shape exists for `name`.
/// - `None` when `name` is not constructor-like.
///
/// Transformation:
/// - Prefers explicit constructor declarations and falls back to eligible
///   single-shape alias constructors before delegating arity and unification to
///   scheme inference.
pub(super) fn infer_constructor_call(
    name: &str,
    args: &[Type],
    arg_names: &[Option<String>],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    if let Some(schemes) = ctx.constructors.get(name) {
        return infer_constructor_schemes(name, schemes, args, arg_names, subst, errors);
    }

    let schemes = alias_constructor_call_schemes(name, ctx.aliases)?;
    infer_constructor_schemes(name, &schemes, args, arg_names, subst, errors)
}

/// Infers a constructor call against a candidate scheme set.
///
/// Inputs:
/// - `name`: constructor name used for diagnostics.
/// - `schemes`: explicit or alias-derived constructor schemes.
/// - `args`: inferred argument types.
/// - `arg_names`: optional source argument names parallel to `args`.
/// - `subst`: active type-variable substitution.
/// - `errors`: mutable diagnostic text sink.
///
/// Output:
/// - `Some(Type)` for a resolved constructor call or dynamic error result.
///
/// Transformation:
/// - Instantiates each candidate with fresh type variables, checks fixed or
///   vararg arity, unifies parameters, and commits only the successful trial
///   substitution.
pub(super) fn infer_constructor_schemes(
    name: &str,
    schemes: &[ConstructorScheme],
    args: &[Type],
    arg_names: &[Option<String>],
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let mut last_error = None;
    let mut last_named_errors = Vec::new();

    for scheme in schemes {
        let instantiated =
            instantiate_constructor_scheme(scheme, next_constructor_type_var(args, subst));
        let param_names = instantiated
            .param_names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let effective_args = if call_has_named_args(arg_names) {
            let mut named_errors = Vec::new();
            if !validate_named_call_args(name, arg_names, &param_names, &mut named_errors) {
                last_named_errors = named_errors;
                continue;
            }

            if instantiated.vararg.is_none() {
                if !validate_required_defaulted_constructor_call_args(
                    name,
                    arg_names,
                    &instantiated,
                    &mut named_errors,
                ) {
                    last_named_errors = named_errors;
                    continue;
                }
                complete_defaulted_call_arg_types(
                    args,
                    arg_names,
                    &param_names,
                    &instantiated.fixed_params,
                )
            } else {
                reorder_named_call_arg_types(args, arg_names, &param_names)
            }
        } else if instantiated.vararg.is_none() && args.len() >= instantiated.min_arity {
            complete_defaulted_call_arg_types(
                args,
                arg_names,
                &param_names,
                &instantiated.fixed_params,
            )
        } else {
            args.to_vec()
        };
        let mut trial_subst = subst.clone();
        let result = if let Some(vararg) = &instantiated.vararg {
            infer_varargs_constructor_call(
                name,
                &instantiated,
                vararg,
                &effective_args,
                &mut trial_subst,
            )
        } else if effective_args.len() >= instantiated.min_arity
            && effective_args.len() <= instantiated.fixed_params.len()
        {
            infer_fixed_constructor_call(&instantiated, &effective_args, &mut trial_subst)
        } else {
            Err(format!(
                "constructor {} has arity mismatch: expected {}..{} args, found {}",
                name,
                instantiated.min_arity,
                instantiated.fixed_params.len(),
                effective_args.len()
            ))
        };

        match result {
            Ok(ty) => {
                *subst = trial_subst;
                return Some(ty);
            }
            Err(message) => last_error = Some(message),
        }
    }

    if last_error.is_none() && !last_named_errors.is_empty() {
        errors.extend(last_named_errors);
    } else {
        errors.push(
            last_error
                .unwrap_or_else(|| format!("no matching constructor {} / {}", name, args.len())),
        );
    }
    Some(Type::Dynamic)
}

/// Validates required constructor parameters for default-aware calls.
///
/// Inputs:
/// - `name`: constructor name used in diagnostics.
/// - `arg_names`: optional source names parallel to supplied arguments.
/// - `scheme`: selected constructor scheme.
/// - `errors`: output diagnostic sink.
///
/// Output:
/// - `true` when all required constructor parameters are supplied.
///
/// Transformation:
/// - Computes supplied declaration slots from positional and named arguments,
///   then rejects any required slot before the constructor's `min_arity`.
fn validate_required_defaulted_constructor_call_args(
    name: &str,
    arg_names: &[Option<String>],
    scheme: &ConstructorScheme,
    errors: &mut Vec<String>,
) -> bool {
    let param_names = scheme
        .param_names
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let supplied = supplied_named_parameter_slots(arg_names, &param_names);
    for (index, parameter) in scheme.param_names.iter().take(scheme.min_arity).enumerate() {
        if !supplied.contains(&index) {
            errors.push(format!(
                "missing required argument `{}` for constructor `{}`",
                parameter, name
            ));
        }
    }

    errors.is_empty()
}

/// Infers a fixed-arity constructor call.
///
/// Inputs:
/// - `scheme`: instantiated constructor scheme.
/// - `args`: inferred argument types.
/// - `subst`: active type-variable substitution.
///
/// Output:
/// - Resolved return type or unification error text.
///
/// Transformation:
/// - Unifies each fixed parameter against the corresponding argument and
///   applies the resulting substitution to the constructor return type.
fn infer_fixed_constructor_call(
    scheme: &ConstructorScheme,
    args: &[Type],
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    for (expected, actual) in scheme.fixed_params.iter().zip(args.iter()) {
        unify(expected, actual, subst)?;
    }

    Ok(instantiate_type(&scheme.ret, subst))
}

/// Infers a vararg constructor call.
///
/// Inputs:
/// - `name`: constructor name used for diagnostics.
/// - `scheme`: instantiated constructor scheme.
/// - `vararg`: repeated parameter type.
/// - `args`: inferred argument types.
/// - `subst`: active type-variable substitution.
///
/// Output:
/// - Resolved return type or unification error text.
///
/// Transformation:
/// - Checks the fixed prefix, then unifies every remaining argument against the
///   repeated vararg type before applying the return substitution.
fn infer_varargs_constructor_call(
    name: &str,
    scheme: &ConstructorScheme,
    vararg: &Type,
    args: &[Type],
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    if args.len() < scheme.fixed_params.len() {
        return Err(format!(
            "constructor {} expects at least {} args, found {}",
            name,
            scheme.fixed_params.len(),
            args.len()
        ));
    }

    for (expected, actual) in scheme.fixed_params.iter().zip(args.iter()) {
        unify(expected, actual, subst)?;
    }

    for actual in args.iter().skip(scheme.fixed_params.len()) {
        unify(vararg, actual, subst)?;
    }

    Ok(instantiate_type(&scheme.ret, subst))
}

/// Infers an opaque alias constructor call.
///
/// Inputs:
/// - `name`: opaque alias name.
/// - `arg_types`: inferred constructor argument types.
/// - `aliases`: local alias table.
/// - `errors`: mutable diagnostic text sink.
///
/// Output:
/// - `Some(Type)` for an opaque alias constructor result or dynamic error.
/// - `None` when `name` is not an opaque alias.
///
/// Transformation:
/// - Requires one representation argument, verifies it against the expanded
///   alias body, and returns the opaque named type with inferred type
///   parameters.
pub(super) fn infer_opaque_constructor(
    name: &str,
    arg_types: &[Type],
    aliases: &HashMap<String, TypeAlias>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let alias = aliases.get(name)?;
    if !alias.is_opaque {
        return None;
    }
    if arg_types.len() != 1 {
        errors.push(format!(
            "opaque constructor {} expects 1 argument, found {}",
            name,
            arg_types.len()
        ));
        return Some(Type::Dynamic);
    }

    if name == "FixedArray" {
        if let Type::Tuple(items) = &arg_types[0] {
            let elem = if items.is_empty() {
                Type::Never
            } else {
                normalize_union(items.clone())
            };
            return Some(Type::FixedArray {
                size: items.len(),
                elem: Box::new(elem),
            });
        }
    }

    let expected = expand_type_aliases(&alias.body, aliases);
    let mut alias_subst = HashMap::new();
    if let Err(message) = unify(&expected, &arg_types[0], &mut alias_subst) {
        errors.push(message);
        return Some(Type::Dynamic);
    }

    Some(Type::Named {
        module: None,
        name: name.to_string(),
        args: alias
            .params
            .iter()
            .map(|param| apply_subst(&Type::Var(*param), &alias_subst))
            .collect(),
    })
}

/// Derives constructor schemes from an eligible type alias.
///
/// Inputs:
/// - `name`: alias name, optionally qualified.
/// - `aliases`: alias table containing the candidate alias body.
///
/// Output:
/// - Constructor schemes for zero-payload atom aliases and tuple aliases.
/// - `None` for opaque, lowercase, or non-single-shape aliases.
///
/// Transformation:
/// - Expands aliases, extracts constructor payload parameters from the runtime
///   representation, attaches source tuple-field labels when present, and
///   returns a scheme whose return type is the alias body.
pub(crate) fn alias_constructor_schemes(
    name: &str,
    aliases: &HashMap<String, TypeAlias>,
) -> Option<Vec<ConstructorScheme>> {
    let constructor_name = name.rsplit('.').next().unwrap_or(name);
    if !is_constructor_name(constructor_name) {
        return None;
    }

    let alias = aliases.get(name)?;
    if alias.is_opaque {
        return None;
    }

    let body = expand_type_aliases(&alias.body, aliases);
    let fixed_params = alias_constructor_params(&body)?;
    let param_names = if alias.constructor_param_names.len() == fixed_params.len() {
        alias.constructor_param_names.clone()
    } else {
        Vec::new()
    };
    Some(vec![ConstructorScheme {
        param_names,
        min_arity: fixed_params.len(),
        fixed_params,
        vararg: None,
        ret: body,
    }])
}

/// Derives callable constructor schemes from an eligible type alias.
///
/// Inputs:
/// - `name`: alias name, optionally qualified.
/// - `aliases`: alias table containing the candidate alias body.
///
/// Output:
/// - Non-nullary alias constructor schemes.
/// - `None` when the alias has no callable payload.
///
/// Transformation:
/// - Reuses alias constructor extraction and removes zero-payload aliases so
///   names such as `None()` remain invalid.
pub(super) fn alias_constructor_call_schemes(
    name: &str,
    aliases: &HashMap<String, TypeAlias>,
) -> Option<Vec<ConstructorScheme>> {
    alias_constructor_schemes(name, aliases).and_then(|schemes| {
        let callable = schemes
            .into_iter()
            .filter(|scheme| !scheme.fixed_params.is_empty() || scheme.vararg.is_some())
            .collect::<Vec<_>>();
        (!callable.is_empty()).then_some(callable)
    })
}

/// Extracts alias constructor parameters from a runtime representation.
///
/// Inputs:
/// - `body`: expanded alias body.
///
/// Output:
/// - Empty parameters for atom literal aliases.
/// - Tuple payload parameters for tagged tuple aliases.
/// - `None` for shapes that do not define constructor syntax.
///
/// Transformation:
/// - Treats `{Atom["tag"], value...}` as constructor-like and removes the tag
///   from the callable parameter list.
fn alias_constructor_params(body: &Type) -> Option<Vec<Type>> {
    match body {
        Type::LiteralAtom(_) => Some(Vec::new()),
        Type::Tuple(items) => match items.first() {
            Some(Type::LiteralAtom(_)) => Some(items.iter().skip(1).cloned().collect()),
            _ => None,
        },
        _ => None,
    }
}

/// Checks whether an expected opaque constructor return matches a call body.
///
/// Inputs:
/// - `expr`: source expression expected to be an opaque constructor call.
/// - `expected`: expected opaque named return type.
/// - `locals`: local variable type environment.
/// - `ctx`: expression inference context.
/// - `subst`: active type-variable substitution.
///
/// Output:
/// - `true` when the call constructs the expected opaque type.
/// - `false` otherwise.
///
/// Transformation:
/// - Reconstructs the expected representation from the opaque alias body,
///   infers the argument representation, and commits the trial substitution
///   only when unification succeeds.
pub(super) fn expected_syntax_opaque_constructor_return_matches(
    expr: &SyntaxExprOutput,
    expected: &Type,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> bool {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() || expr.children.len() != 2 {
        return false;
    }

    let Some(constructor_name) = syntax_callee_name(expr) else {
        return false;
    };
    let Some(representation_expr) = expr.children.get(1) else {
        return false;
    };

    let Type::Named {
        module: None,
        name: expected_name,
        args: expected_args,
    } = expected
    else {
        return false;
    };

    if constructor_name != expected_name {
        return false;
    }

    let Some(alias) = ctx.aliases.get(constructor_name) else {
        return false;
    };
    if !alias.is_opaque || alias.params.len() != expected_args.len() {
        return false;
    }

    let mapping = alias
        .params
        .iter()
        .cloned()
        .zip(expected_args.iter().cloned())
        .collect::<HashMap<_, _>>();
    let expected_representation =
        expand_type_aliases(&substitute_type_vars(&alias.body, &mapping), ctx.aliases);

    let mut trial_subst = subst.clone();
    let mut errors = Vec::new();
    let actual_representation = infer_syntax_expr(
        representation_expr,
        locals,
        ctx,
        &mut trial_subst,
        &mut errors,
    );
    if !errors.is_empty() {
        return false;
    }

    if unify(
        &expected_representation,
        &actual_representation,
        &mut trial_subst,
    )
    .is_err()
    {
        return false;
    }

    *subst = trial_subst;
    true
}
