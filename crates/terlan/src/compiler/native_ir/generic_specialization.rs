//! Bounded monomorphization of private CoreIR generic helpers.

use std::collections::{BTreeMap, HashMap};

use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreMapTypeField, CoreModule, CorePattern, CoreStructTypeField,
    CoreTupleTypeElem, CoreType,
};

const MAX_GENERIC_SPECIALIZATIONS: usize = 128;

pub(super) fn specialize_private_generics_with_budget(
    core: &mut CoreModule,
    budget: &mut super::specialization_budget::SpecializationBudget,
) -> Result<(), String> {
    let templates = core
        .functions
        .iter()
        .filter(|function| function_is_generic(function))
        .cloned()
        .map(|function| ((function.name.clone(), function.arity), function))
        .collect::<BTreeMap<_, _>>();
    if templates.is_empty() {
        return Ok(());
    }
    if let Some(function) = templates.values().find(|function| function.public) {
        return Err(format!(
            "error[native_ir.generic_export]: generic export `{}/{}` requires an explicit concrete wrapper",
            function.name, function.arity
        ));
    }

    let mut cache = BTreeMap::<(String, Vec<String>), String>::new();
    let mut cursor = 0usize;
    while cursor < core.functions.len() {
        if function_is_generic(&core.functions[cursor]) {
            cursor += 1;
            continue;
        }
        let mut generated = Vec::new();
        let parameter_types = core.functions[cursor]
            .params
            .iter()
            .filter_map(|parameter| {
                parameter
                    .core_ty
                    .as_ref()
                    .map(|ty| (parameter.name.clone(), ty.clone()))
            })
            .collect::<HashMap<_, _>>();
        for clause in &mut core.functions[cursor].clauses {
            if let Some(body) = clause.body.core_expr.as_mut() {
                rewrite_expr(
                    body,
                    &parameter_types,
                    &templates,
                    &mut cache,
                    &mut generated,
                    &core.module,
                    budget,
                )?;
            }
        }
        core.functions.extend(generated);
        cursor += 1;
    }
    core.functions
        .retain(|function| !templates.contains_key(&(function.name.clone(), function.arity)));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn rewrite_expr(
    expr: &mut CoreExpr,
    variables: &HashMap<String, CoreType>,
    templates: &BTreeMap<(String, usize), CoreFunction>,
    cache: &mut BTreeMap<(String, Vec<String>), String>,
    generated: &mut Vec<CoreFunction>,
    module: &str,
    budget: &mut super::specialization_budget::SpecializationBudget,
) -> Result<(), String> {
    match expr {
        CoreExpr::Call { function, args } => {
            for arg in args.iter_mut() {
                rewrite_expr(arg, variables, templates, cache, generated, module, budget)?;
            }
            let Some(template) = templates.get(&(function.clone(), args.len())) else {
                return Ok(());
            };
            let argument_types = args
                .iter()
                .map(|argument| infer_type(argument, variables, templates))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.generic_argument]: cannot infer concrete arguments for `{}/{}`",
                        template.name, template.arity
                    )
                })?;
            let key = (
                template.name.clone(),
                argument_types.iter().map(CoreType::contract_text).collect(),
            );
            if let Some(name) = cache.get(&key) {
                *function = name.clone();
                return Ok(());
            }
            if cache.len() >= MAX_GENERIC_SPECIALIZATIONS {
                return Err(format!(
                    "error[native_ir.generic_budget]: module exceeds {MAX_GENERIC_SPECIALIZATIONS} generic specializations"
                ));
            }
            budget.reserve(
                super::specialization_budget::SpecializationKind::Generic,
                module,
                1,
            )?;
            let mut substitution = HashMap::new();
            for (parameter, argument) in template.params.iter().zip(&argument_types) {
                unify(
                    parameter.core_ty.as_ref().ok_or_else(|| {
                        "error[native_ir.generic_signature]: generic parameter type is absent"
                            .to_string()
                    })?,
                    argument,
                    &template.generic_params,
                    &mut substitution,
                )?;
            }
            let name = format!("$aot_generic_{}_{}", template.name, cache.len());
            cache.insert(key, name.clone());
            let mut specialized = template.clone();
            specialized.name = name.clone();
            specialized.generic_params.clear();
            for parameter in &mut specialized.params {
                let ty = substitute(
                    parameter.core_ty.as_ref().ok_or_else(|| {
                        "error[native_ir.generic_signature]: generic parameter type is absent"
                            .to_string()
                    })?,
                    &template.generic_params,
                    &substitution,
                );
                parameter.ty = ty.contract_text();
                parameter.core_ty = Some(ty);
            }
            let result = substitute(
                specialized.core_return_type.as_ref().ok_or_else(|| {
                    "error[native_ir.generic_signature]: generic result type is absent".to_string()
                })?,
                &template.generic_params,
                &substitution,
            );
            specialized.return_type = result.contract_text();
            specialized.core_return_type = Some(result);
            generated.push(specialized);
            *function = name;
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            for arg in args {
                rewrite_expr(arg, variables, templates, cache, generated, module, budget)?;
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. }
        | CoreExpr::FunctionCall {
            callee: receiver,
            args,
        } => {
            rewrite_expr(
                receiver, variables, templates, cache, generated, module, budget,
            )?;
            for arg in args {
                rewrite_expr(arg, variables, templates, cache, generated, module, budget)?;
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                rewrite_expr(item, variables, templates, cache, generated, module, budget)?;
            }
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        }
        | CoreExpr::BinaryOp {
            left: head,
            right: tail,
            ..
        } => {
            rewrite_expr(head, variables, templates, cache, generated, module, budget)?;
            rewrite_expr(tail, variables, templates, cache, generated, module, budget)?;
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                rewrite_expr(
                    &mut field.value,
                    variables,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                rewrite_expr(
                    &mut field.value,
                    variables,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            rewrite_expr(base, variables, templates, cache, generated, module, budget)?;
            for field in fields {
                rewrite_expr(
                    &mut field.value,
                    variables,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => {
            rewrite_expr(base, variables, templates, cache, generated, module, budget)?;
        }
        CoreExpr::Let { bindings, body } => {
            let mut locals = variables.clone();
            for binding in bindings {
                rewrite_expr(
                    &mut binding.value,
                    &locals,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
                if let CorePattern::Var(name) = &binding.pattern {
                    if let Some(ty) = infer_type(&binding.value, &locals, templates) {
                        locals.insert(name.clone(), ty);
                    }
                }
            }
            rewrite_expr(body, &locals, templates, cache, generated, module, budget)?;
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                rewrite_expr(
                    &mut clause.condition,
                    variables,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
                rewrite_expr(
                    &mut clause.body,
                    variables,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            rewrite_expr(
                scrutinee, variables, templates, cache, generated, module, budget,
            )?;
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    rewrite_expr(
                        guard, variables, templates, cache, generated, module, budget,
                    )?;
                }
                rewrite_expr(
                    &mut clause.body,
                    variables,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            rewrite_expr(body, variables, templates, cache, generated, module, budget)?;
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    rewrite_expr(
                        guard, variables, templates, cache, generated, module, budget,
                    )?;
                }
                rewrite_expr(
                    &mut clause.body,
                    variables,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
            }
            if let Some(after) = after_clause {
                rewrite_expr(
                    &mut after.trigger,
                    variables,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
                rewrite_expr(
                    &mut after.body,
                    variables,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
            }
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            rewrite_expr(expr, variables, templates, cache, generated, module, budget)?;
            for generator in generators {
                rewrite_expr(
                    &mut generator.source,
                    variables,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
            }
            for guard in guards {
                rewrite_expr(
                    guard, variables, templates, cache, generated, module, budget,
                )?;
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            for parameter in parameters {
                rewrite_expr(
                    parameter, variables, templates, cache, generated, module, budget,
                )?;
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                rewrite_expr(arg, variables, templates, cache, generated, module, budget)?;
            }
            rewrite_expr(
                record, variables, templates, cache, generated, module, budget,
            )?;
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
    Ok(())
}

fn function_is_generic(function: &CoreFunction) -> bool {
    !function.generic_params.is_empty()
}

fn unify(
    template: &CoreType,
    concrete: &CoreType,
    generic_params: &[String],
    substitution: &mut HashMap<String, CoreType>,
) -> Result<(), String> {
    if let CoreType::Named(name) = template {
        if generic_params.iter().any(|parameter| parameter == name) {
            return match substitution.get(name) {
                Some(previous) if previous != concrete => Err(format!(
                    "error[native_ir.generic_unification]: `{name}` has incompatible concrete types"
                )),
                Some(_) => Ok(()),
                None => {
                    substitution.insert(name.clone(), concrete.clone());
                    Ok(())
                }
            };
        }
    }
    match (template, concrete) {
        (
            CoreType::Apply {
                constructor: a,
                args: x,
            },
            CoreType::Apply {
                constructor: b,
                args: y,
            },
        ) if a == b && x.len() == y.len() => {
            for (left, right) in x.iter().zip(y) {
                unify(left, right, generic_params, substitution)?;
            }
            Ok(())
        }
        (CoreType::List(left), CoreType::List(right)) => {
            unify(left, right, generic_params, substitution)
        }
        _ if template == concrete => Ok(()),
        _ => Err(format!(
            "error[native_ir.generic_unification]: `{}` does not match `{}`",
            template.contract_text(),
            concrete.contract_text()
        )),
    }
}

fn substitute(
    ty: &CoreType,
    generic_params: &[String],
    values: &HashMap<String, CoreType>,
) -> CoreType {
    match ty {
        CoreType::Named(name) if generic_params.iter().any(|parameter| parameter == name) => {
            values.get(name).cloned().unwrap_or_else(|| ty.clone())
        }
        CoreType::Apply { constructor, args } => CoreType::Apply {
            constructor: constructor.clone(),
            args: args
                .iter()
                .map(|ty| substitute(ty, generic_params, values))
                .collect(),
        },
        CoreType::List(element) => {
            CoreType::List(Box::new(substitute(element, generic_params, values)))
        }
        CoreType::Tuple(elements) => CoreType::Tuple(
            elements
                .iter()
                .map(|element| match element {
                    CoreTupleTypeElem::Type(ty) => {
                        CoreTupleTypeElem::Type(substitute(ty, generic_params, values))
                    }
                    CoreTupleTypeElem::Field { name, ty } => CoreTupleTypeElem::Field {
                        name: name.clone(),
                        ty: substitute(ty, generic_params, values),
                    },
                })
                .collect(),
        ),
        CoreType::Struct { name, fields } => CoreType::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|field| CoreStructTypeField {
                    name: field.name.clone(),
                    ty: substitute(&field.ty, generic_params, values),
                    is_private: field.is_private,
                })
                .collect(),
        },
        CoreType::Map(fields) => CoreType::Map(
            fields
                .iter()
                .map(|field| CoreMapTypeField {
                    key: field.key.clone(),
                    operator: field.operator.clone(),
                    value: substitute(&field.value, generic_params, values),
                })
                .collect(),
        ),
        CoreType::Arrow {
            params,
            return_type,
        } => CoreType::Arrow {
            params: params
                .iter()
                .map(|ty| substitute(ty, generic_params, values))
                .collect(),
            return_type: Box::new(substitute(return_type, generic_params, values)),
        },
        CoreType::Union(items) => CoreType::Union(
            items
                .iter()
                .map(|ty| substitute(ty, generic_params, values))
                .collect(),
        ),
        _ => ty.clone(),
    }
}

fn infer_type(
    expr: &CoreExpr,
    variables: &HashMap<String, CoreType>,
    templates: &BTreeMap<(String, usize), CoreFunction>,
) -> Option<CoreType> {
    match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(value) if value == "Unit" => Some(CoreType::Named("Unit".into())),
        CoreExpr::Var(name) => variables.get(name).cloned(),
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        CoreExpr::Call { function, args } => {
            let template = templates.get(&(function.clone(), args.len()))?;
            let mut values = HashMap::new();
            for (parameter, argument) in template.params.iter().zip(args) {
                unify(
                    parameter.core_ty.as_ref()?,
                    &infer_type(argument, variables, templates)?,
                    &template.generic_params,
                    &mut values,
                )
                .ok()?;
            }
            Some(substitute(
                template.core_return_type.as_ref()?,
                &template.generic_params,
                &values,
            ))
        }
        _ => None,
    }
}
