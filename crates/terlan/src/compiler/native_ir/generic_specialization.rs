//! Bounded application-wide monomorphization of CoreIR generic helpers.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreModule, CorePattern, CoreTupleTypeElem, CoreType,
};

const MAX_GENERIC_SPECIALIZATIONS: usize = 128;

/// Every callable candidate grouped by its qualified name and arity.
type CallableTemplates = BTreeMap<(String, usize), Vec<CoreFunction>>;

#[path = "generic_specialization/generic_unification.rs"]
mod generic_unification;
#[path = "generic_specialization/inference.rs"]
mod inference;
#[path = "generic_specialization/pattern_types.rs"]
mod pattern_types;
#[path = "generic_specialization/type_substitution.rs"]
mod type_substitution;
use generic_unification::{substitute, unify};
use inference::{
    common_concrete_parameter_types, contains_implicit_generic_type, infer_generic_argument_types,
    infer_type, needs_contextual_type,
};
pub(super) use pattern_types::{bind_pattern_types, structural_tuple_variant};
use type_substitution::substitute_function_types;

pub(super) fn specialize_application_generics_with_budget(
    cores: &mut [CoreModule],
    budget: &mut super::specialization_budget::SpecializationBudget,
) -> Result<(), String> {
    for core in cores.iter_mut() {
        for function in &mut core.functions {
            if function.generic_params.is_empty() {
                function.generic_params = implicit_generic_params(function);
            } else {
                function.generic_params = function
                    .generic_params
                    .iter()
                    .map(|parameter| {
                        parameter
                            .split_once("=>")
                            .map_or(parameter.as_str(), |(name, _)| name)
                            .trim()
                            .trim_start_matches(['-', '+'])
                            .to_string()
                    })
                    .collect();
            }
        }
    }
    let mut templates = BTreeMap::new();
    for core in cores.iter() {
        let local = core
            .functions
            .iter()
            .map(|function| (function.name.clone(), function.arity))
            .collect::<HashSet<_>>();
        for function in &core.functions {
            let mut template = function.clone();
            qualify_local_calls(&mut template, &core.module, &local);
            template.name = format!("{}.{}", core.module, function.name);
            templates
                .entry((format!("{}.{}", core.module, function.name), function.arity))
                .or_insert_with(Vec::new)
                .push(template);
        }
    }
    if templates.is_empty() {
        return Ok(());
    }
    for core in cores.iter_mut() {
        specialize_core(core, &templates, budget)?;
    }
    Ok(())
}

fn specialize_core(
    core: &mut CoreModule,
    templates: &CallableTemplates,
    budget: &mut super::specialization_budget::SpecializationBudget,
) -> Result<(), String> {
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
                    templates,
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
        .retain(|function| !function_is_generic(function));
    Ok(())
}
fn rewrite_expr(
    expr: &mut CoreExpr,
    variables: &HashMap<String, CoreType>,
    templates: &CallableTemplates,
    cache: &mut BTreeMap<(String, Vec<String>), String>,
    generated: &mut Vec<CoreFunction>,
    module: &str,
    budget: &mut super::specialization_budget::SpecializationBudget,
) -> Result<(), String> {
    if let CoreExpr::RemoteCall {
        module: receiver_module,
        function,
        args,
    } = expr
    {
        if receiver_module == "__receiver__" {
            let function = function.clone();
            let args = std::mem::take(args);
            *expr = CoreExpr::Call { function, args };
            return rewrite_expr(expr, variables, templates, cache, generated, module, budget);
        }
    }
    if let CoreExpr::Call { function, args } = expr {
        if matches!(variables.get(function), Some(CoreType::Arrow { .. })) {
            let callee = function.clone();
            for argument in args.iter_mut() {
                rewrite_expr(
                    argument, variables, templates, cache, generated, module, budget,
                )?;
            }
            let arguments = std::mem::take(args);
            *expr = CoreExpr::FunctionCall {
                callee: Box::new(CoreExpr::Var(callee)),
                args: arguments,
            };
            return Ok(());
        }
    }
    match expr {
        CoreExpr::Call { function, args } => {
            let contextual_parameters = callable_templates(templates, module, function, args.len())
                .and_then(common_concrete_parameter_types);
            if let Some(parameter_types) = contextual_parameters {
                for (argument, expected) in args.iter_mut().zip(parameter_types) {
                    if needs_contextual_type(argument) {
                        *argument = CoreExpr::Cast {
                            expr: Box::new(argument.clone()),
                            target_type: expected,
                        };
                    }
                }
            }
            let template = generic_template(templates, module, function, args.len());
            let argument_types = template
                .map(|template| {
                    infer_generic_argument_types(template, args, variables, templates, module)
                })
                .transpose()?;
            if let Some(argument_types) = &argument_types {
                for (argument, expected) in args.iter_mut().zip(argument_types) {
                    apply_contextual_argument_type(argument, expected);
                }
            }
            for (index, arg) in args.iter_mut().enumerate() {
                if let (
                    CoreExpr::Var(function),
                    Some(CoreType::Arrow {
                        params: parameter_types,
                        ..
                    }),
                ) = (
                    &*arg,
                    argument_types.as_ref().and_then(|types| types.get(index)),
                ) {
                    if !variables.contains_key(function) {
                        let function = function.clone();
                        let parameters = (0..parameter_types.len())
                            .map(|parameter| format!("$native_named_callback_{index}_{parameter}"))
                            .collect::<Vec<_>>();
                        *arg = CoreExpr::Lam {
                            params: parameters.iter().cloned().map(CorePattern::Var).collect(),
                            body: Box::new(CoreExpr::Call {
                                function,
                                args: parameters.into_iter().map(CoreExpr::Var).collect(),
                            }),
                        };
                    }
                }
                if let (
                    CoreExpr::Lam { params, body },
                    Some(CoreType::Arrow {
                        params: parameter_types,
                        ..
                    }),
                ) = (
                    &mut *arg,
                    argument_types.as_ref().and_then(|types| types.get(index)),
                ) {
                    let mut locals = variables.clone();
                    for (pattern, ty) in params.iter().zip(parameter_types) {
                        bind_pattern_types(pattern, ty, &mut locals);
                    }
                    rewrite_expr(body, &locals, templates, cache, generated, module, budget)?;
                } else {
                    rewrite_expr(arg, variables, templates, cache, generated, module, budget)?;
                }
            }
            let Some(template) = template else {
                return Ok(());
            };
            let argument_types = argument_types.expect("generic template has argument types");
            let key = (
                template.name.clone(),
                argument_types.iter().map(CoreType::contract_text).collect(),
            );
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
            if let Some(inlined) = inline_record_forwarder(template, args) {
                *expr = inlined;
                return Ok(());
            }
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
            let name = format!("$aot_generic_{}_{}", template.name, cache.len());
            cache.insert(key, name.clone());
            let mut specialized = template.clone();
            specialized.name = name.clone();
            specialized.public = false;
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
            substitute_function_types(&mut specialized, &template.generic_params, &substitution);
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
        CoreExpr::Cast {
            expr: base,
            target_type,
        } => {
            let concrete_target = contains_implicit_generic_type(target_type)
                .then(|| infer_type(base, variables, templates, module))
                .flatten();
            rewrite_expr(base, variables, templates, cache, generated, module, budget)?;
            if let Some(concrete_target) = concrete_target {
                *target_type = concrete_target;
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => {
            rewrite_expr(base, variables, templates, cache, generated, module, budget)?;
        }
        CoreExpr::Let { bindings, body } => {
            let mut locals = variables.clone();
            for binding in bindings {
                let binding_type_before = infer_type(&binding.value, &locals, templates, module);
                rewrite_expr(
                    &mut binding.value,
                    &locals,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
                let binding_type = binding_type_before
                    .or_else(|| infer_type(&binding.value, &locals, templates, module));
                if let Some(ty) = binding_type {
                    bind_pattern_types(&binding.pattern, &ty, &mut locals);
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
            let scrutinee_type = infer_type(scrutinee, variables, templates, module);
            rewrite_expr(
                scrutinee, variables, templates, cache, generated, module, budget,
            )?;
            for clause in clauses {
                let mut locals = variables.clone();
                if let Some(scrutinee_type) = scrutinee_type.as_ref() {
                    bind_pattern_types(&clause.pattern, scrutinee_type, &mut locals);
                }
                if let Some(guard) = &mut clause.guard {
                    rewrite_expr(guard, &locals, templates, cache, generated, module, budget)?;
                }
                rewrite_expr(
                    &mut clause.body,
                    &locals,
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

fn contextual_literal_type(argument: &CoreExpr, expected: &CoreType) -> Option<CoreType> {
    match (argument, expected) {
        (CoreExpr::Binary(_), CoreType::Binary | CoreType::String) => Some(expected.clone()),
        _ => None,
    }
}

fn apply_contextual_argument_type(argument: &mut CoreExpr, expected: &CoreType) {
    if matches!(argument, CoreExpr::Cast { target_type, .. } if target_type == expected) {
        return;
    }
    if needs_contextual_type(argument) || contextual_literal_type(argument, expected).is_some() {
        let expression = std::mem::replace(argument, CoreExpr::Atom("Unit".to_string()));
        *argument = CoreExpr::Cast {
            expr: Box::new(expression),
            target_type: expected.clone(),
        };
    }
}

fn function_is_generic(function: &CoreFunction) -> bool {
    !function.generic_params.is_empty()
}

fn generic_template<'a>(
    templates: &'a CallableTemplates,
    module: &str,
    function: &str,
    arity: usize,
) -> Option<&'a CoreFunction> {
    callable_templates(templates, module, function, arity)?
        .iter()
        .find(|function| function_is_generic(function))
}

fn callable_templates<'a>(
    templates: &'a CallableTemplates,
    module: &str,
    function: &str,
    arity: usize,
) -> Option<&'a [CoreFunction]> {
    if let Some(candidates) = templates
        .get(&(function.to_string(), arity))
        .or_else(|| templates.get(&(format!("{module}.{function}"), arity)))
    {
        return Some(candidates.as_slice());
    }

    // Checked local function references can survive module normalization in
    // their source spelling while the callable inventory is fully qualified.
    // Accept that spelling only when it identifies one callable unambiguously.
    let suffix = format!(".{function}");
    let mut matches = templates
        .iter()
        .filter(|((candidate, candidate_arity), _)| {
            *candidate_arity == arity && candidate.ends_with(&suffix)
        })
        .map(|(_, candidates)| candidates.as_slice());
    let candidate = matches.next()?;
    matches.next().is_none().then_some(candidate)
}

fn implicit_generic_params(function: &CoreFunction) -> Vec<String> {
    let mut names = HashSet::new();
    for ty in function
        .params
        .iter()
        .filter_map(|parameter| parameter.core_ty.as_ref())
        .chain(function.core_return_type.as_ref())
    {
        collect_implicit_generic_params(ty, &mut names);
    }
    let mut names = names.into_iter().collect::<Vec<_>>();
    names.sort();
    names
}

fn collect_implicit_generic_params(ty: &CoreType, names: &mut HashSet<String>) {
    match ty {
        CoreType::Named(name) if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() => {
            names.insert(name.clone());
        }
        CoreType::Apply { args, .. } | CoreType::Union(args) => {
            for ty in args {
                collect_implicit_generic_params(ty, names);
            }
        }
        CoreType::Tuple(elements) => {
            for element in elements {
                match element {
                    CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => {
                        collect_implicit_generic_params(ty, names)
                    }
                }
            }
        }
        CoreType::List(element) => collect_implicit_generic_params(element, names),
        CoreType::Struct { fields, .. } => {
            for field in fields {
                collect_implicit_generic_params(&field.ty, names);
            }
        }
        CoreType::Map(fields) => {
            for field in fields {
                collect_implicit_generic_params(&field.value, names);
            }
        }
        CoreType::Arrow {
            params,
            return_type,
        } => {
            for ty in params {
                collect_implicit_generic_params(ty, names);
            }
            collect_implicit_generic_params(return_type, names);
        }
        _ => {}
    }
}

fn inline_record_forwarder(template: &CoreFunction, arguments: &[CoreExpr]) -> Option<CoreExpr> {
    let [clause] = template.clauses.as_slice() else {
        return None;
    };
    if clause.guard.is_some()
        || arguments.len() != template.params.len()
        || !clause
            .core_patterns
            .iter()
            .zip(&template.params)
            .all(|(pattern, parameter)| {
                matches!(pattern, Some(CorePattern::Var(name)) if name == &parameter.name)
            })
    {
        return None;
    }
    let CoreExpr::RecordConstruct { name, fields } = clause.body.core_expr.as_ref()? else {
        return None;
    };
    if fields.len() != template.params.len() {
        return None;
    }
    let parameter_indices = template
        .params
        .iter()
        .enumerate()
        .map(|(index, parameter)| (parameter.name.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut used = HashSet::new();
    let fields = fields
        .iter()
        .map(|field| {
            let CoreExpr::Var(parameter) = &field.value else {
                return None;
            };
            let index = *parameter_indices.get(parameter.as_str())?;
            if index != used.len() || !used.insert(index) {
                return None;
            }
            Some(crate::terlan_typeck::CoreRecordExprField {
                key: field.key.clone(),
                required: field.required,
                value: arguments[index].clone(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    (used.len() == arguments.len()).then(|| CoreExpr::RecordConstruct {
        name: name.clone(),
        fields,
    })
}

fn contains_generic_parameter(ty: &CoreType, parameters: &[String]) -> bool {
    match ty {
        CoreType::Named(name) => parameters.contains(name),
        CoreType::Apply { args, .. } | CoreType::Union(args) => args
            .iter()
            .any(|ty| contains_generic_parameter(ty, parameters)),
        CoreType::List(element) => contains_generic_parameter(element, parameters),
        CoreType::Tuple(elements) => elements.iter().any(|element| match element {
            CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => {
                contains_generic_parameter(ty, parameters)
            }
        }),
        CoreType::Struct { fields, .. } => fields
            .iter()
            .any(|field| contains_generic_parameter(&field.ty, parameters)),
        CoreType::Map(fields) => fields
            .iter()
            .any(|field| contains_generic_parameter(&field.value, parameters)),
        CoreType::Arrow {
            params,
            return_type,
        } => {
            params
                .iter()
                .any(|ty| contains_generic_parameter(ty, parameters))
                || contains_generic_parameter(return_type, parameters)
        }
        _ => false,
    }
}

mod call_qualification;

use call_qualification::qualify_local_calls;
