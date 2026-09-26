//! Preserve concrete trait bodies and canonical call identities for static selection.

use super::*;

#[cfg(test)]
#[path = "concrete_impl_test.rs"]
mod tests;

/// Materializes checked, non-generic implementations as ordinary typed functions.
/// The trait identity is metadata, never inferred from the generated symbol.
pub(crate) fn core_syntax_concrete_impl_functions(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    template_prop_order: &HashMap<String, Vec<String>>,
) -> Vec<CoreFunction> {
    let mut functions = Vec::new();
    let type_refs = trait_type_refs(resolved);
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
        let Some(type_args) = trait_type_arguments(&trait_ref.text, &type_refs) else {
            continue;
        };
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
                receiver_method: false,
                trait_method: Some(CoreTraitMethodIdentity {
                    trait_name: trait_name.clone(),
                    type_args: type_args.clone(),
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
    let mut traits = resolved
        .interface
        .traits
        .keys()
        .map(|name| (name.clone(), format!("{}.{}", resolved.name, name)))
        .chain(resolved.imported_traits.values().map(|imported| {
            (
                imported.local_name.clone(),
                format!("{}.{}", imported.source_module, imported.source_name),
            )
        }))
        .collect::<HashMap<_, _>>();
    for canonical in traits.values().cloned().collect::<Vec<_>>() {
        traits.insert(canonical.clone(), canonical);
    }
    let type_refs = trait_type_refs(resolved);
    for function in functions {
        for clause in &mut function.clauses {
            if let Some(guard) = &mut clause.guard {
                rewrite_trait_summary(guard, &traits, &type_refs);
            }
            rewrite_trait_summary(&mut clause.body, &traits, &type_refs);
        }
    }
}

/// Uses resolver identities for both declaration and call-site trait arguments.
fn trait_type_refs(resolved: &ResolvedModule) -> HashMap<String, String> {
    let mut refs = imported_type_text_refs(&imported_type_names(resolved));
    let primitives = primitive_type_names();
    for name in resolved.local_type_names.keys() {
        if !primitives.contains(name) {
            refs.insert(name.clone(), format!("{}.{}", resolved.name, name));
        }
    }
    refs
}

fn trait_type_arguments(reference: &str, refs: &HashMap<String, String>) -> Option<Vec<CoreType>> {
    match core_type_from_text(&crate::terlan_hir::qualify_syntax_type_text(
        reference, refs,
    ))? {
        CoreType::Apply { args, .. } => Some(args),
        CoreType::Named(_) => Some(Vec::new()),
        _ => None,
    }
}

fn rewrite_trait_summary(
    summary: &mut CoreExprSummary,
    traits: &HashMap<String, String>,
    refs: &HashMap<String, String>,
) {
    if let Some(expr) = &mut summary.core_expr {
        let mut changed = false;
        crate::terlan_typeck::visit_core_expr_mut(expr, &mut |expr| {
            let CoreExpr::RemoteCall {
                module,
                function,
                type_args,
                args,
            } = expr
            else {
                return;
            };
            let head = module.split('[').next().unwrap_or(module).trim();
            let Some(trait_name) = traits.get(head) else {
                return;
            };
            let Some(instance_args) = trait_type_arguments(module, refs) else {
                return;
            };
            let identity = CoreTraitMethodIdentity {
                trait_name: trait_name.clone(),
                type_args: instance_args,
                method: function.clone(),
            };
            *expr = CoreExpr::Call {
                function: format!("{}.{}", identity.dispatch_owner(), identity.method),
                type_args: std::mem::take(type_args),
                args: std::mem::take(args),
            };
            changed = true;
        });
        if changed {
            summary.remote = None;
            summary.checked_preservation_evidence = core_expr_checked_preservation_evidence(expr);
        }
    }
    for child in &mut summary.children {
        rewrite_trait_summary(child, traits, refs);
    }
}
