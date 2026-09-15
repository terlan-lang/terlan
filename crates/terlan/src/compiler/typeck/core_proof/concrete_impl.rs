//! Preserve concrete trait bodies and canonical call identities for static selection.

use super::*;

/// Materializes checked, non-generic implementations as ordinary typed functions.
/// The trait identity is metadata, never inferred from the generated symbol.
pub(crate) fn core_syntax_concrete_impl_functions(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    template_prop_order: &HashMap<String, Vec<String>>,
) -> Vec<CoreFunction> {
    let mut functions = Vec::new();
    for (index, declaration) in module.declarations.iter().enumerate() {
        let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            generic_params,
            is_negative: false,
            is_public,
            methods,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if !generic_params.is_empty() {
            continue;
        }
        let head = trait_ref.text.split('[').next().unwrap_or_default().trim();
        let trait_name = resolved.imported_traits.get(head).map_or_else(
            || {
                if head.contains('.') {
                    head.to_string()
                } else {
                    format!("{}.{}", resolved.name, head)
                }
            },
            |imported| format!("{}.{}", imported.source_module, imported.source_name),
        );
        for method in methods {
            let locals = function_value_parameter_names(&method.params);
            functions.push(CoreFunction {
                name: format!("__terlan_concrete_impl_{index}_{}", method.name),
                source: None,
                trait_method: Some(CoreTraitMethodIdentity {
                    trait_name: trait_name.clone(),
                    method: method.name.clone(),
                }),
                arity: method.params.len(),
                public: *is_public,
                generic_params: Vec::new(),
                native_operation: None,
                params: method
                    .params
                    .iter()
                    .map(|parameter| CoreParam {
                        name: parameter.name.clone(),
                        ty: parameter.annotation.text.clone(),
                        core_ty: core_type_from_text(&parameter.annotation.text),
                    })
                    .collect(),
                return_type: method.return_type.text.clone(),
                core_return_type: core_type_from_text(&method.return_type.text),
                clauses: method
                    .clauses
                    .iter()
                    .map(|clause| {
                        core_function_clause_summary(
                            clause,
                            receiver_methods,
                            template_prop_order,
                            &locals,
                        )
                    })
                    .collect(),
            });
        }
    }
    functions
}

/// Resolves trait aliases without selecting an implementation by name or arity.
/// Typed application lowering selects the concrete parameter vector later.
pub(crate) fn rewrite_concrete_trait_calls(
    functions: &mut [CoreFunction],
    resolved: &ResolvedModule,
) {
    let mut dispatch = HashMap::new();
    let traits = resolved
        .interface
        .traits
        .iter()
        .map(|(name, signature)| {
            (
                name.clone(),
                format!("{}.{}", resolved.name, name),
                signature,
            )
        })
        .chain(resolved.imported_traits.values().filter_map(|imported| {
            let signature = resolved
                .interface_map
                .get(&imported.source_module)?
                .traits
                .get(&imported.source_name)?;
            Some((
                imported.local_name.clone(),
                format!("{}.{}", imported.source_module, imported.source_name),
                signature,
            ))
        }));
    for (local, canonical, signature) in traits {
        for (method, signature) in &signature.methods {
            for name in [&local, &canonical] {
                dispatch.insert(
                    (name.clone(), method.clone(), signature.params.len()),
                    format!("{canonical}.{method}"),
                );
            }
        }
    }
    rewrite_structural_impl_calls(functions, &dispatch);
}
