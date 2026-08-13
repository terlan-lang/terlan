use super::*;

pub(super) fn native_resolver(
    resolver: &HashMap<CallIdentity, usize>,
    candidate_to_native: &HashMap<usize, usize>,
) -> HashMap<CallIdentity, usize> {
    resolver
        .iter()
        .filter_map(|(identity, candidate)| {
            candidate_to_native
                .get(candidate)
                .map(|native| (identity.clone(), *native))
        })
        .collect()
}

pub(super) fn native_function_types(
    resolver: &HashMap<CallIdentity, usize>,
    candidate_to_native: &HashMap<usize, usize>,
    candidates: &[Candidate<'_>],
    constructors: &super::super::constructors::NativeConstructorLayouts,
) -> HashMap<CallIdentity, NativeType> {
    resolver
        .iter()
        .filter(|(_, candidate)| candidate_to_native.contains_key(candidate))
        .filter_map(|(identity, candidate)| {
            native_return_type_with_constructors(candidates[*candidate].function, constructors)
                .map(|native_type| (identity.clone(), native_type))
        })
        .collect()
}

pub(super) fn native_function_core_types(
    resolver: &HashMap<CallIdentity, usize>,
    candidate_to_native: &HashMap<usize, usize>,
    candidates: &[Candidate<'_>],
) -> HashMap<CallIdentity, CoreType> {
    resolver
        .iter()
        .filter(|(_, candidate)| candidate_to_native.contains_key(candidate))
        .filter_map(|(identity, candidate)| {
            let function = candidates[*candidate].function;
            let core_type = super::super::dynamic_return::inferred_dynamic_return_type(function)
                .or_else(|| function.core_return_type.clone())?;
            Some((identity.clone(), core_type))
        })
        .collect()
}

pub(super) fn native_callable_shapes(
    resolver: &HashMap<CallIdentity, usize>,
    candidate_to_native: &HashMap<usize, usize>,
    candidates: &[Candidate<'_>],
    constructors: &super::super::constructors::NativeConstructorLayouts,
) -> HashMap<CallIdentity, NativeCallableShape> {
    resolver
        .iter()
        .filter(|(_, candidate)| candidate_to_native.contains_key(candidate))
        .filter_map(|(identity, candidate)| {
            let candidate = candidates[*candidate];
            let parameters = candidate
                .function
                .params
                .iter()
                .map(|parameter| {
                    native_type_with_constructors(
                        parameter.core_ty.as_ref(),
                        &parameter.ty,
                        constructors,
                    )
                })
                .collect::<Option<Vec<_>>>()?;
            let result = native_return_type_with_constructors(candidate.function, constructors)?;
            Some((
                identity.clone(),
                NativeCallableShape {
                    id: super::super::stable_export_id(
                        &candidate.core.module,
                        &candidate.function.name,
                        candidate.function.arity,
                    ),
                    parameters,
                    result,
                },
            ))
        })
        .collect()
}
