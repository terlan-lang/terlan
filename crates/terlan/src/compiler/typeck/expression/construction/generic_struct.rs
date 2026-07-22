use super::*;

/// Infers one implication-constrained generic struct construction.
///
/// The struct is adapted to the existing bounded function scheme so generic
/// unification and structural implication evidence use the same fail-closed
/// checker as ordinary function calls.
pub(in super::super) fn infer_generic_struct_construction(
    name: &str,
    arguments: &[(String, Type)],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let scheme = ctx.struct_schemes.get(name)?;
    if scheme.generic_params.is_empty() {
        return None;
    }

    let field_types = scheme.fields.iter().cloned().collect::<HashMap<_, _>>();
    let mut supplied = HashSet::new();
    let mut params = Vec::with_capacity(arguments.len());
    let mut args = Vec::with_capacity(arguments.len());
    for (source_name, actual) in arguments {
        let (field_name, requested_private) = split_private_field_spelling(source_name);
        if !supplied.insert(field_name.to_string()) {
            errors.push(format!(
                "duplicate field `{field_name}` in struct constructor `{name}`"
            ));
            continue;
        }
        let Some(expected) = field_types.get(field_name) else {
            errors.push(format!("unknown field `{field_name}` on struct `{name}`"));
            continue;
        };
        if let Some(message) = struct_field_visibility_error(
            name,
            field_name,
            requested_private,
            ctx.struct_field_visibility,
            ctx.imported_type_names,
        ) {
            errors.push(message);
        }
        params.push(expected.clone());
        args.push(actual.clone());
    }
    for (field_name, _) in &scheme.fields {
        if !supplied.contains(field_name) {
            errors.push(format!(
                "missing field `{field_name}` in struct constructor `{name}`"
            ));
        }
    }
    if params.len() != arguments.len() || supplied.len() != scheme.fields.len() {
        return Some(Type::Dynamic);
    }

    let callable = FunctionScheme {
        params,
        ret: Type::Named {
            module: None,
            name: name.to_string(),
            args: scheme.params.iter().copied().map(Type::Var).collect(),
        },
        generic_params: scheme.generic_params.clone(),
        bounds: scheme.bounds.clone(),
    };
    Some(
        infer_function_with_bounds(&callable, Some(name), &args, ctx, subst).unwrap_or_else(
            |message| {
                errors.push(message);
                Type::Dynamic
            },
        ),
    )
}
