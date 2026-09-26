//! Typechecking of callable declaration clauses.

use super::*;

pub(super) fn check_syntax_callable_clauses(
    declaration: CallableDeclaration<'_>,
    environment: CallableCheckEnvironment<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let CallableDeclaration {
        label: callable_label,
        name: callable_name,
        params,
        clauses,
        scheme,
        fallback_span,
        requires_pure_body,
        requires_trait_purity,
    } = declaration;
    let CallableCheckEnvironment {
        alias_names,
        aliases,
        expr_ctx,
        effectful_local_calls,
        imported_effects,
    } = environment;
    let mut clause_patterns: Vec<(Vec<SyntaxPatternOutput>, Span)> = Vec::new();

    if clauses.is_empty() {
        diagnostics.push(Diagnostic {
            span: fallback_span,
            message: format!("{} has no clauses", callable_label),
            severity: DiagSeverity::Error,
        });
        return;
    }

    for clause in clauses {
        let span = clause.span.into();
        if clause.patterns.len() != params.len() {
            diagnostics.push(Diagnostic {
                span,
                message: format!(
                    "{} has arity mismatch: expected {}, found {}",
                    callable_label,
                    params.len(),
                    clause.patterns.len()
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        }

        let instantiated = instantiate_function_scheme(scheme);
        let mut subst = HashMap::new();
        let mut locals: HashMap<String, Type> = HashMap::new();
        for (pattern, param_type) in clause.patterns.iter().zip(instantiated.params.iter()) {
            if let Err(message) = check_syntax_pattern(
                pattern,
                &expand_type_aliases(param_type, aliases),
                aliases,
                Some(expr_ctx),
                &mut locals,
                &mut subst,
            ) {
                diagnostics.push(Diagnostic {
                    span,
                    message,
                    severity: DiagSeverity::Error,
                });
            }
        }

        let function_values = locals
            .iter()
            .filter(|(_, value_type)| {
                matches!(
                    expand_type_aliases(value_type, aliases),
                    Type::Function { .. }
                )
            })
            .map(|(name, _)| name.clone())
            .collect::<HashSet<_>>();
        let mut facts = effectful_call_facts(effectful_local_calls, imported_effects);
        facts.function_values = Some(&function_values);
        let mut local_expr_ctx = expr_ctx_with_current_bounds(expr_ctx, &instantiated.bounds);
        local_expr_ctx.effectful_calls = facts;
        let mut local_errors = Vec::new();
        if let Some(guard) = clause.guard.as_ref() {
            refine_by_syntax_guard(guard, &mut locals, aliases, &mut subst);
            check_clause_guard_purity(
                guard,
                "function guard",
                &locals,
                &local_expr_ctx,
                &subst,
                &mut local_errors,
            );
            let guard_type = infer_syntax_expr(
                guard,
                &locals,
                &local_expr_ctx,
                &mut subst,
                &mut local_errors,
            );
            if let Err(message) = unify(&Type::Bool, &guard_type, &mut subst) {
                diagnostics.push(Diagnostic {
                    span,
                    message: format!("function guard {message}"),
                    severity: DiagSeverity::Error,
                });
            }
        }
        let bounds_error = if let Err(message) =
            check_function_bounds(&instantiated, Some(callable_name), &local_expr_ctx, &subst)
        {
            diagnostics.push(Diagnostic {
                span,
                message,
                severity: DiagSeverity::Error,
            });
            true
        } else {
            false
        };
        if requires_pure_body {
            let contract = if requires_trait_purity {
                format!("{callable_label} required pure by its trait contract")
            } else {
                format!("{callable_label} annotated @pure")
            };
            check_pure_expression_effects_with_call_facts(
                &clause.body,
                &contract,
                local_expr_ctx.templates,
                &facts,
                &mut local_errors,
            );
        }

        let expected_return = expand_type_aliases(&instantiated.ret, aliases);
        let inferred = if bounds_error {
            Type::Dynamic
        } else {
            infer_syntax_expr_with_expected(
                &clause.body,
                &expected_return,
                &locals,
                &local_expr_ctx,
                &mut subst,
                &mut local_errors,
            )
            .unwrap_or_else(|| {
                infer_syntax_expr(
                    &clause.body,
                    &locals,
                    &local_expr_ctx,
                    &mut subst,
                    &mut local_errors,
                )
            })
        };

        for error in local_errors {
            diagnostics.push(expression_error_to_diagnostic(error, span));
        }

        let inferred_expanded = expand_type_aliases(&inferred, aliases);

        if let Err(message) = unify_return_type(&expected_return, &inferred_expanded, &mut subst) {
            let expected_substituted = apply_subst(&instantiated.ret, &subst);
            let inferred_substituted = apply_subst(&inferred, &subst);
            if is_subtype_with_aliases(&inferred_substituted, &expected_substituted, aliases) {
                clause_patterns.push((clause.patterns.clone(), span));
                continue;
            }
            let revealed_inferred = reveal_opaque_aliases(&inferred_expanded, aliases);
            if unify_return_type(&expected_return, &revealed_inferred, &mut subst).is_ok() {
                clause_patterns.push((clause.patterns.clone(), span));
                continue;
            }
            if expected_syntax_opaque_constructor_return_matches(
                &clause.body,
                &expected_return,
                &locals,
                expr_ctx,
                &mut subst,
            ) {
                clause_patterns.push((clause.patterns.clone(), span));
                continue;
            }
            diagnostics.push(Diagnostic {
                span,
                message,
                severity: DiagSeverity::Error,
            });
        }

        clause_patterns.push((clause.patterns.clone(), span));
    }

    check_syntax_function_clause_exhaustiveness(
        callable_name,
        params.first().map(|param| param.annotation.text.as_str()),
        params.len(),
        alias_names,
        &clause_patterns,
        aliases,
        diagnostics,
    );
}
