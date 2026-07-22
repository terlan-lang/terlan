use super::*;

/// Validates implication-constrained aliases before transparent expansion.
pub(crate) fn check_type_alias_implication_bounds(
    ty: &Type,
    ctx: &ExprInferContext<'_>,
) -> Vec<String> {
    let mut errors = Vec::new();
    check_type_alias_implication_bounds_inner(ty, ctx, &mut errors);
    errors
}

fn check_type_alias_implication_bounds_inner(
    ty: &Type,
    ctx: &ExprInferContext<'_>,
    errors: &mut Vec<String>,
) {
    match ty {
        Type::Named { module, name, args } => {
            for arg in args {
                check_type_alias_implication_bounds_inner(arg, ctx, errors);
            }
            let lookup = module
                .as_ref()
                .map_or_else(|| name.clone(), |module| format!("{module}.{name}"));
            let Some(alias) = ctx.aliases.get(&lookup) else {
                return;
            };
            if alias.bounds.is_empty() || alias.params.len() != args.len() {
                return;
            }
            let scheme = FunctionScheme {
                params: alias.params.iter().copied().map(Type::Var).collect(),
                ret: Type::Never,
                generic_params: Vec::new(),
                bounds: alias.bounds.clone(),
            };
            let mut subst = HashMap::new();
            if let Err(message) =
                infer_function_with_bounds(&scheme, Some(name), args, ctx, &mut subst)
            {
                errors.push(message);
            }
        }
        Type::Apply { args, .. } | Type::Tuple(args) | Type::Union(args) => {
            for arg in args {
                check_type_alias_implication_bounds_inner(arg, ctx, errors);
            }
        }
        Type::Existential { body, .. } | Type::List(body) => {
            check_type_alias_implication_bounds_inner(body, ctx, errors);
        }
        Type::Map(fields) => {
            for field in fields {
                check_type_alias_implication_bounds_inner(&field.value, ctx, errors);
            }
        }
        Type::FixedArray { elem, .. } => {
            check_type_alias_implication_bounds_inner(elem, ctx, errors);
        }
        Type::Function { params, ret } => {
            for param in params {
                check_type_alias_implication_bounds_inner(param, ctx, errors);
            }
            check_type_alias_implication_bounds_inner(ret, ctx, errors);
        }
        Type::Int
        | Type::Float
        | Type::Number
        | Type::Binary
        | Type::Atom
        | Type::Bool
        | Type::Term
        | Type::Dynamic
        | Type::Never
        | Type::LiteralAtom(_)
        | Type::LiteralInt(_)
        | Type::LiteralBool(_)
        | Type::Var(_)
        | Type::Placeholder => {}
    }
}
