//! Keeps return-only generic witnesses at the checked consumer boundary.

use super::*;

/// Solves a result-position call using both its arguments and expected result.
/// This only annotates fully determined calls. The ordinary monomorphizer still
/// checks explicit arguments, rejects contradictions and owns the work budget.
pub(super) fn seed(
    value: &mut CoreExpr,
    expected: &CoreType,
    variables: &HashMap<String, CoreType>,
    templates: &CallableTemplates,
    module: &str,
) {
    match value {
        CoreExpr::Call {
            function,
            args,
            type_args,
        } => {
            if type_args.is_empty() {
                if let Some(inferred) =
                    infer(function, args, expected, variables, templates, module)
                {
                    *type_args = inferred;
                }
            }
        }
        CoreExpr::Cast { expr, target_type } => {
            seed(expr, target_type, variables, templates, module)
        }
        CoreExpr::Let { bindings, body } => {
            let mut locals = variables.clone();
            for binding in bindings {
                let ty = infer_type(&binding.value, &locals, templates, module);
                clear_bindings(&binding.pattern, &mut locals);
                if let Some(ty) = ty {
                    bind_pattern_types(&binding.pattern, &ty, &mut locals);
                }
            }
            seed(body, expected, &locals, templates, module);
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                seed(&mut clause.body, expected, variables, templates, module);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            let scrutinee_type = infer_type(scrutinee, variables, templates, module);
            for clause in clauses {
                let mut locals = variables.clone();
                clear_bindings(&clause.pattern, &mut locals);
                if let Some(ty) = &scrutinee_type {
                    bind_pattern_types(&clause.pattern, ty, &mut locals);
                }
                seed(&mut clause.body, expected, &locals, templates, module);
            }
        }
        CoreExpr::Lam {
            params,
            parameter_types,
            body,
        } => {
            if let CoreType::Arrow {
                params: expected_params,
                return_type,
            } = expected
            {
                if params.len() == expected_params.len() {
                    let mut locals = lambda_type_scope(params, parameter_types, variables);
                    for (pattern, ty) in params.iter().zip(expected_params) {
                        bind_pattern_types(pattern, ty, &mut locals);
                    }
                    seed(body, return_type, &locals, templates, module);
                }
            }
        }
        _ => {}
    }
}

fn clear_bindings(pattern: &CorePattern, variables: &mut HashMap<String, CoreType>) {
    for name in super::super::expression::free_variable_analysis::pattern_bound_names(pattern) {
        variables.remove(&name);
    }
}

fn infer(
    function: &str,
    arguments: &[CoreExpr],
    expected: &CoreType,
    variables: &HashMap<String, CoreType>,
    templates: &CallableTemplates,
    module: &str,
) -> Option<Vec<CoreType>> {
    let candidates = callable_templates(templates, module, function, arguments.len())?;
    let mut matching = candidates.iter().filter_map(|template| {
        if template.generic_params.is_empty() || template.params.len() != arguments.len() {
            return None;
        }
        // Parameters already inferable from arguments keep the established path.
        if !template.generic_params.iter().any(|name| {
            !template
                .params
                .iter()
                .filter_map(|parameter| parameter.core_ty.as_ref())
                .any(|ty| contains_generic_parameter(ty, std::slice::from_ref(name)))
        }) {
            return None;
        }
        let mut bindings = HashMap::new();
        unify(
            template.core_return_type.as_ref()?,
            expected,
            &template.generic_params,
            &mut bindings,
        )
        .ok()?;
        let argument_types =
            infer_generic_argument_types(template, arguments, &[], variables, templates, module)
                .ok()?;
        for (parameter, actual) in template.params.iter().zip(argument_types) {
            unify(
                parameter.core_ty.as_ref()?,
                &actual,
                &template.generic_params,
                &mut bindings,
            )
            .ok()?;
        }
        template
            .generic_params
            .iter()
            .map(|name| {
                let ty = bindings.get(name)?;
                (!contains_implicit_generic_type(ty)).then(|| ty.clone())
            })
            .collect::<Option<Vec<_>>>()
    });
    let inferred = matching.next()?;
    matching.next().is_none().then_some(inferred)
}
