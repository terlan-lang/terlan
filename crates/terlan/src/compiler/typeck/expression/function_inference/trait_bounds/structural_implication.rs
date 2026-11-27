use super::*;

/// Extends call substitutions only for direct generic parameters constrained
/// by structural implications. Dynamic remains a wildcard for ordinary type
/// inference, but cannot masquerade as unresolved implication evidence.
pub(super) fn structural_call_subst(
    scheme: &FunctionScheme,
    args: &[Type],
    subst: &HashMap<TypeVarId, Type>,
) -> HashMap<TypeVarId, Type> {
    let mut structural_subst = subst.clone();

    for bound in &scheme.bounds {
        if bound.trait_name != STRUCTURAL_IMPLICATION_BOUND {
            continue;
        }
        let [Type::Var(subject), Type::Map(_)] = bound.trait_args.as_slice() else {
            continue;
        };
        if !matches!(apply_subst(&Type::Var(*subject), subst), Type::Var(_)) {
            continue;
        }

        if let Some((_, actual)) = scheme
            .params
            .iter()
            .zip(args)
            .find(|(expected, _)| apply_subst(expected, subst) == Type::Var(*subject))
        {
            structural_subst.insert(*subject, apply_subst(actual, subst));
        }
    }

    structural_subst
}

/// Returns a field type supplied by active structural implication evidence.
///
/// `None` means no structural evidence applies to the receiver. A matched
/// implication without the requested field returns a stable fail-closed error.
pub(crate) fn structural_implication_field_type(
    receiver: &Type,
    field: &str,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
) -> Option<Result<Type, String>> {
    let mut matched_subject = false;
    for bound in ctx
        .current_bounds
        .iter()
        .filter(|bound| bound.trait_name == STRUCTURAL_IMPLICATION_BOUND)
    {
        let [subject, Type::Map(fields)] = bound.trait_args.as_slice() else {
            continue;
        };
        if apply_subst(subject, subst) != *receiver {
            continue;
        }
        matched_subject = true;
        if let Some(required) = fields.iter().find(|required| required.key == field) {
            return Some(Ok(apply_subst(&required.value, subst)));
        }
    }

    matched_subject.then(|| {
        Err(format!(
            "implication_scope_error: structural evidence for {} does not include field `{field}`",
            pretty_type(receiver)
        ))
    })
}

/// Validates one instantiated structural implication at a declaration or call
/// boundary.
pub(super) fn check_structural_implication_bound(
    args: &[Type],
    context: &str,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    let [subject, Type::Map(required_fields)] = args else {
        return Err(format!(
            "unproven_implication: malformed structural evidence at `{context}`"
        ));
    };

    if matches!(subject, Type::Var(_)) {
        let entailed = ctx.current_bounds.iter().any(|bound| {
            let [candidate_subject, Type::Map(candidate_fields)] = bound.trait_args.as_slice()
            else {
                return false;
            };
            bound.trait_name == STRUCTURAL_IMPLICATION_BOUND
                && apply_subst(candidate_subject, subst) == *subject
                && structural_fields_entail(candidate_fields, required_fields, ctx)
        });
        return entailed.then_some(()).ok_or_else(|| {
            format!(
                "unproven_implication: `{context}` requires {} => {}",
                pretty_type(subject),
                pretty_type(&Type::Map(required_fields.clone()))
            )
        });
    }

    check_concrete_structural_shape(subject, required_fields, context, ctx)
}

/// Checks whether one evidence shape includes every field required by another.
fn structural_fields_entail(
    available: &[MapFieldType],
    required: &[MapFieldType],
    ctx: &ExprInferContext<'_>,
) -> bool {
    required.iter().all(|required_field| {
        available
            .iter()
            .find(|field| field.key == required_field.key)
            .is_some_and(|available_field| {
                structural_field_type_satisfies(
                    &available_field.value,
                    &required_field.value,
                    "structural evidence",
                    ctx,
                )
                .is_ok()
            })
    })
}

/// Validates a concrete closed struct or typed map against a required shape.
fn check_concrete_structural_shape(
    subject: &Type,
    required_fields: &[MapFieldType],
    context: &str,
    ctx: &ExprInferContext<'_>,
) -> Result<(), String> {
    match subject {
        Type::Named { name, .. } => {
            let Some(available) = ctx.struct_fields.get(name) else {
                return Err(format!(
                    "unproven_implication: `{context}` requires closed structural evidence for {}, but the type has no visible struct shape",
                    pretty_type(subject)
                ));
            };
            for required in required_fields {
                let Some(actual) = available.get(&required.key) else {
                    return Err(format!(
                        "unproven_implication: `{context}` requires field `{}` on {}",
                        required.key,
                        pretty_type(subject)
                    ));
                };
                let is_private = ctx
                    .struct_field_visibility
                    .get(name)
                    .and_then(|fields| fields.get(&required.key))
                    .is_some_and(|visibility| visibility.is_private);
                if is_private {
                    return Err(format!(
                        "unproven_implication: `{context}` cannot use private field `{}` as structural evidence for {}",
                        required.key,
                        pretty_type(subject)
                    ));
                }
                structural_field_type_satisfies(actual, &required.value, context, ctx)?;
            }
            Ok(())
        }
        Type::Map(available) if !available.is_empty() => {
            for required in required_fields {
                let Some(actual) = available.iter().find(|field| field.key == required.key) else {
                    return Err(format!(
                        "unproven_implication: `{context}` requires field `{}` on {}",
                        required.key,
                        pretty_type(subject)
                    ));
                };
                structural_field_type_satisfies(&actual.value, &required.value, context, ctx)?;
            }
            Ok(())
        }
        _ => Err(format!(
            "unproven_implication: `{context}` requires closed structural evidence, found {}",
            pretty_type(subject)
        )),
    }
}

/// Checks one concrete field type, recursively validating nested shapes.
fn structural_field_type_satisfies(
    actual: &Type,
    required: &Type,
    context: &str,
    ctx: &ExprInferContext<'_>,
) -> Result<(), String> {
    if let Type::Map(required_fields) = required {
        return check_concrete_structural_shape(actual, required_fields, context, ctx);
    }

    let actual = expand_type_aliases(actual, ctx.aliases);
    let required = expand_type_aliases(required, ctx.aliases);
    if is_subtype_with_aliases(&actual, &required, ctx.aliases) {
        Ok(())
    } else {
        Err(format!(
            "unproven_implication: `{context}` requires field type {}, found {}",
            pretty_type(&required),
            pretty_type(&actual)
        ))
    }
}
