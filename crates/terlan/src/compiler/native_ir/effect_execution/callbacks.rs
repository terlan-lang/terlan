//! Discovers callbacks from checked descriptor construction, not helper names.

use crate::terlan_typeck::{visit_core_expr_children_mut, CoreCaseClause};

use super::*;

type Templates = BTreeMap<(String, usize), Vec<CoreFunction>>;
type Callbacks = BTreeMap<(String, String, bool), Callback>;

/// Retains each actually constructed callback's signature before field erasure.
pub(super) fn collect(cores: &mut [CoreModule], schema: &Schema) -> NativeIrResult<Vec<Callback>> {
    let mut templates = Templates::new();
    for core in cores.iter() {
        for function in &core.functions {
            templates
                .entry((format!("{}.{}", core.module, function.name), function.arity))
                .or_default()
                .push(function.clone());
        }
    }
    let mut callbacks = Callbacks::new();
    for core in cores {
        for function in &mut core.functions {
            if !function.generic_params.is_empty() {
                continue;
            }
            let variables = function
                .params
                .iter()
                .filter_map(|param| param.core_ty.clone().map(|ty| (param.name.clone(), ty)))
                .collect::<HashMap<_, _>>();
            for clause in &mut function.clauses {
                let mut local = variables.clone();
                for (pattern, param) in clause.core_patterns.iter().zip(&function.params) {
                    if let (Some(pattern), Some(ty)) = (pattern, &param.core_ty) {
                        super::super::generic_specialization::bind_pattern_types(
                            pattern, ty, &mut local,
                        );
                    }
                }
                for summary in clause
                    .guard
                    .iter_mut()
                    .chain(std::iter::once(&mut clause.body))
                {
                    if let Some(expr) = &mut summary.core_expr {
                        scan(
                            expr,
                            &local,
                            &core.module,
                            schema,
                            &templates,
                            &mut callbacks,
                        )?;
                    }
                }
            }
        }
    }
    Ok(callbacks.into_values().collect())
}

fn scan(
    expr: &mut CoreExpr,
    variables: &HashMap<String, CoreType>,
    module: &str,
    schema: &Schema,
    templates: &Templates,
    callbacks: &mut Callbacks,
) -> NativeIrResult<()> {
    use super::super::generic_specialization::{bind_pattern_types, infer_type};
    match expr {
        CoreExpr::Let { bindings, body } => {
            let mut local = variables.clone();
            for binding in bindings {
                scan(
                    &mut binding.value,
                    &local,
                    module,
                    schema,
                    templates,
                    callbacks,
                )?;
                let ty = infer_type(&binding.value, &local, templates, module);
                for name in super::super::expression::free_variable_analysis::pattern_bound_names(
                    &binding.pattern,
                ) {
                    local.remove(&name);
                }
                if let Some(ty) = ty {
                    bind_pattern_types(&binding.pattern, &ty, &mut local);
                }
            }
            scan(body, &local, module, schema, templates, callbacks)?;
        }
        CoreExpr::Lam {
            params,
            parameter_types,
            body,
        } => {
            let mut local = variables.clone();
            for (pattern, ty) in params.iter().zip(parameter_types) {
                for name in
                    super::super::expression::free_variable_analysis::pattern_bound_names(pattern)
                {
                    local.remove(&name);
                }
                if let Some(ty) = ty {
                    bind_pattern_types(pattern, ty, &mut local);
                }
            }
            scan(body, &local, module, schema, templates, callbacks)?;
        }
        CoreExpr::Case { scrutinee, clauses } => {
            scan(scrutinee, variables, module, schema, templates, callbacks)?;
            let ty = infer_type(scrutinee, variables, templates, module);
            scan_clauses(
                clauses,
                ty.as_ref(),
                variables,
                module,
                schema,
                templates,
                callbacks,
            )?;
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            scan(body, variables, module, schema, templates, callbacks)?;
            let ty = infer_type(body, variables, templates, module);
            scan_clauses(
                of_clauses,
                ty.as_ref(),
                variables,
                module,
                schema,
                templates,
                callbacks,
            )?;
            scan_clauses(
                catch_clauses,
                None,
                variables,
                module,
                schema,
                templates,
                callbacks,
            )?;
            if let Some(after) = after_clause {
                scan(
                    &mut after.trigger,
                    variables,
                    module,
                    schema,
                    templates,
                    callbacks,
                )?;
                scan(
                    &mut after.body,
                    variables,
                    module,
                    schema,
                    templates,
                    callbacks,
                )?;
            }
        }
        _ => {
            let mut result = Ok(());
            visit_core_expr_children_mut(expr, &mut |child| {
                if result.is_ok() {
                    result = scan(child, variables, module, schema, templates, callbacks);
                }
            });
            result?;
        }
    }
    let Some((flat, callback)) = descriptor_callback(expr) else {
        return Ok(());
    };
    let signature = infer_type(callback, variables, templates, module)
        .ok_or("error[native_ir.effect_callback]: descriptor callback has no checked type")?;
    let admitted = from_signature(&signature, flat, schema)?;
    callbacks.insert(
        (
            admitted.input.contract_text(),
            admitted.output.contract_text(),
            flat,
        ),
        admitted,
    );
    // Preserve this checked witness until closure lifting has converted a bare
    // function reference or lambda into its ordinary owned native closure.
    if !matches!(callback, CoreExpr::Cast { target_type, .. } if *target_type == signature) {
        *callback = CoreExpr::Cast {
            expr: Box::new(callback.clone()),
            target_type: signature,
        };
    }
    Ok(())
}

fn descriptor_callback(expr: &mut CoreExpr) -> Option<(bool, &mut CoreExpr)> {
    use crate::terlan_typeck::CoreTupleTypeElem;
    let CoreExpr::Cast { expr, target_type } = expr else {
        return None;
    };
    let CoreExpr::Tuple(items) = expr.as_mut() else {
        return None;
    };
    let [CoreExpr::Atom(actual), _, value] = items.as_mut_slice() else {
        return None;
    };
    let variants = match target_type {
        CoreType::Union(variants) => variants.as_slice(),
        other => std::slice::from_ref(other),
    };
    let mut matching = variants.iter().filter_map(|variant| {
        let CoreType::Tuple(fields) = variant else { return None };
        matches!(fields.first(), Some(CoreTupleTypeElem::Type(CoreType::AtomLiteral(tag))) if tag == actual)
            .then_some(fields)
    });
    let fields = matching.next()?;
    if matching.next().is_some() {
        return None;
    }
    let [CoreTupleTypeElem::Type(CoreType::AtomLiteral(tag)), CoreTupleTypeElem::Field {
        name: input,
        ty: CoreType::Named(input_type),
    }, CoreTupleTypeElem::Field {
        name: callback,
        ty: CoreType::Named(callback_type),
    }] = fields.as_slice()
    else {
        return None;
    };
    if input != "effect"
        || input_type != super::super::effect_values::ERASED_VALUE_TYPE
        || callback_type != super::super::effect_values::ERASED_VALUE_TYPE
    {
        return None;
    }
    let flat = match (tag.as_str(), callback.as_str()) {
        ("mapped", "mapper") => false,
        ("flat_map", "next") => true,
        _ => return None,
    };
    Some((flat, value))
}

fn scan_clauses(
    clauses: &mut [CoreCaseClause],
    matched: Option<&CoreType>,
    variables: &HashMap<String, CoreType>,
    module: &str,
    schema: &Schema,
    templates: &Templates,
    callbacks: &mut Callbacks,
) -> NativeIrResult<()> {
    for clause in clauses {
        let mut local = variables.clone();
        for name in
            super::super::expression::free_variable_analysis::pattern_bound_names(&clause.pattern)
        {
            local.remove(&name);
        }
        if let Some(ty) = matched {
            super::super::generic_specialization::bind_pattern_types(
                &clause.pattern,
                ty,
                &mut local,
            );
        }
        if let Some(guard) = &mut clause.guard {
            scan(guard, &local, module, schema, templates, callbacks)?;
        }
        scan(
            &mut clause.body,
            &local,
            module,
            schema,
            templates,
            callbacks,
        )?;
    }
    Ok(())
}

fn from_signature(signature: &CoreType, flat: bool, schema: &Schema) -> NativeIrResult<Callback> {
    let CoreType::Arrow {
        params,
        return_type,
    } = signature
    else {
        return Err("error[native_ir.effect_callback]: descriptor value is not a callback".into());
    };
    let [input] = params.as_slice() else {
        return Err("error[native_ir.effect_callback]: callback must take one argument".into());
    };
    let output = if flat {
        let mut bindings = HashMap::new();
        super::super::generic_specialization::unify(
            &schema.body,
            return_type,
            std::slice::from_ref(&schema.parameter),
            &mut bindings,
        )
        .map_err(|_| "error[native_ir.effect_callback]: continuation must return Effect")?;
        bindings
            .remove(&schema.parameter)
            .ok_or("error[native_ir.effect_callback]: continuation result is unconstrained")?
    } else {
        return_type.as_ref().clone()
    };
    require_concrete(input)?;
    require_concrete(&output)?;
    Ok(Callback {
        input: input.clone(),
        output,
        flat,
    })
}
