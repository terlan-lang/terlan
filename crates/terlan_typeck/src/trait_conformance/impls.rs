use super::*;

/// Validates one parsed explicit trait implementation against its trait.
///
/// Inputs:
/// - `impl_decl`: parsed impl target, owner type, and method signatures.
/// - `impl_span`: source span used for impl-level diagnostics.
/// - `trait_map`: known local/imported trait signatures.
/// - `inheritance_cache`: memoized inherited method sets by trait name.
/// - `diagnostics`: output diagnostic buffer.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Resolves inheritance, specializes type parameters, and checks coverage.
pub(super) fn check_parsed_trait_impl_signature(
    impl_decl: &ParsedTraitImpl,
    impl_span: Span,
    trait_map: &HashMap<String, ParsedTraitSignature>,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(trait_signature) = trait_map.get(&impl_decl.target.name) else {
        diagnostics.push(Diagnostic {
            span: impl_span,
            message: format!("unknown trait `{}` in impl", impl_decl.target.name,),
            severity: DiagSeverity::Error,
        });
        return;
    };

    let inherited_methods = collect_trait_methods_with_inheritance(
        trait_map,
        &impl_decl.target.name,
        inheritance_cache,
        &mut HashSet::new(),
    )
    .unwrap_or_default();

    if impl_decl.target.type_args.len() != trait_signature.type_params.len() {
        diagnostics.push(Diagnostic {
            span: impl_span,
            message: format!(
                "trait `{}` expects {} type parameter(s), found {}",
                impl_decl.target.name,
                trait_signature.type_params.len(),
                impl_decl.target.type_args.len()
            ),
            severity: DiagSeverity::Error,
        });
        return;
    };

    if let Some(for_type) = &impl_decl.for_type {
        if for_type.trim().is_empty() {
            diagnostics.push(Diagnostic {
                span: impl_span,
                message: format!(
                    "impl of trait `{}` must declare a non-empty owner type",
                    impl_decl.target.name
                ),
                severity: DiagSeverity::Error,
            });
            return;
        }
    }

    let mut seen_methods = HashSet::new();

    for method in &impl_decl.methods {
        if !seen_methods.insert(method.name.clone()) {
            diagnostics.push(Diagnostic {
                span: method.span,
                message: format!(
                    "duplicate method `{}` in impl {}",
                    method.name, impl_decl.target.name
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        }

        let Some(expected) = inherited_methods.get(&method.name) else {
            diagnostics.push(Diagnostic {
                span: method.span,
                message: format!(
                    "method `{}` is not declared in trait `{}`",
                    method.name, impl_decl.target.name
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        };

        let specialized_params = expected
            .params
            .iter()
            .map(|param| {
                specialize_trait_type_text(
                    &param.ty,
                    &trait_signature.type_params,
                    &impl_decl.target.type_args,
                )
            })
            .collect::<Vec<_>>();
        let specialized_return = specialize_trait_type_text(
            &expected.return_type,
            &trait_signature.type_params,
            &impl_decl.target.type_args,
        );

        if specialized_params.len() != method.params.len() {
            diagnostics.push(Diagnostic {
                span: method.span,
                message: format!(
                    "method `{}` in trait `{}` has arity {}, found {}",
                    method.name,
                    impl_decl.target.name,
                    specialized_params.len(),
                    method.params.len()
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        }

        for (idx, (expected_type, found_type)) in
            specialized_params.iter().zip(&method.params).enumerate()
        {
            if expected.params[idx].is_mutable
                && !method.mutable_params.get(idx).copied().unwrap_or(false)
            {
                diagnostics.push(Diagnostic {
                    span: method.span,
                    message: format!(
                        "method `{}` parameter {} in trait `{}` must be mutable",
                        method.name,
                        idx + 1,
                        impl_decl.target.name
                    ),
                    severity: DiagSeverity::Error,
                });
            }
            if !found_type.trim().is_empty() && !trait_type_text_equal(expected_type, found_type) {
                diagnostics.push(Diagnostic {
                    span: method.span,
                    message: format!(
                        "method `{}` parameter {} in trait `{}` expects {}, found {}",
                        method.name,
                        idx + 1,
                        impl_decl.target.name,
                        expected_type,
                        found_type
                    ),
                    severity: DiagSeverity::Error,
                });
            }
        }

        if !method.return_type.trim().is_empty()
            && !trait_type_text_equal(&specialized_return, &method.return_type)
        {
            diagnostics.push(Diagnostic {
                span: method.span,
                message: format!(
                    "method `{}` return type in trait `{}` expects {}, found {}",
                    method.name, impl_decl.target.name, specialized_return, method.return_type
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    for (expected_method, expected_signature) in &inherited_methods {
        if !impl_decl
            .methods
            .iter()
            .any(|method| &method.name == expected_method)
            && !expected_signature.has_default
        {
            diagnostics.push(Diagnostic {
                span: impl_span,
                message: format!(
                    "missing method `{}` in impl of trait `{}`",
                    expected_method, impl_decl.target.name
                ),
                severity: DiagSeverity::Error,
            });
        }
    }
}
