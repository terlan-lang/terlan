//! Bounded application-wide monomorphization of CoreIR generic helpers.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreMapTypeField, CoreModule, CorePattern, CoreStructTypeField,
    CoreTupleTypeElem, CoreType,
};

const MAX_GENERIC_SPECIALIZATIONS: usize = 128;

/// Every callable candidate grouped by its qualified name and arity.
type CallableTemplates = BTreeMap<(String, usize), Vec<CoreFunction>>;

#[path = "generic_specialization/inference.rs"]
mod inference;
#[path = "generic_specialization/pattern_types.rs"]
mod pattern_types;
#[path = "generic_specialization/type_substitution.rs"]
mod type_substitution;
use inference::{common_concrete_parameter_types, infer_type, needs_contextual_type};
use pattern_types::bind_pattern_types;
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
            for (index, arg) in args.iter_mut().enumerate() {
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
            let concrete_target = match base.as_ref() {
                CoreExpr::Call { function, args } => {
                    generic_template(templates, module, function, args.len())
                        .filter(|template| {
                            contains_generic_parameter(target_type, &template.generic_params)
                        })
                        .and_then(|_| infer_type(base, variables, templates, module))
                }
                _ => None,
            };
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
                let binding_type = infer_type(&binding.value, &locals, templates, module);
                rewrite_expr(
                    &mut binding.value,
                    &locals,
                    templates,
                    cache,
                    generated,
                    module,
                    budget,
                )?;
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

fn infer_generic_argument_types(
    template: &CoreFunction,
    arguments: &[CoreExpr],
    variables: &HashMap<String, CoreType>,
    templates: &CallableTemplates,
    module: &str,
) -> Result<Vec<CoreType>, String> {
    let mut substitution = HashMap::new();
    let mut concrete = Vec::with_capacity(arguments.len());
    for (index, (parameter, argument)) in template.params.iter().zip(arguments).enumerate() {
        let expected = parameter.core_ty.as_ref().ok_or_else(|| {
            "error[native_ir.generic_signature]: generic parameter type is absent".to_string()
        })?;
        let contextual_expected = substitute(expected, &template.generic_params, &substitution);
        let inferred = contextual_literal_type(argument, &contextual_expected)
            .or_else(|| infer_type(argument, variables, templates, module))
            .or_else(|| {
                (!contains_generic_parameter(&contextual_expected, &template.generic_params))
                    .then_some(contextual_expected)
            })
            .ok_or_else(|| {
                format!(
                    "error[native_ir.generic_argument]: cannot infer argument {} for `{}/{}` from `{}`",
                    index + 1,
                    template.name,
                    template.arity,
                    argument.contract_text()
                )
            })?;
        unify(
            expected,
            &inferred,
            &template.generic_params,
            &mut substitution,
        )
        .map_err(|error| {
            format!(
                "{error}; while specializing argument {} of `{}/{}` from `{}`",
                index + 1,
                template.name,
                template.arity,
                argument.contract_text()
            )
        })?;
        concrete.push(inferred);
    }
    Ok(concrete)
}

fn contextual_literal_type(argument: &CoreExpr, expected: &CoreType) -> Option<CoreType> {
    match (argument, expected) {
        (CoreExpr::Binary(_), CoreType::Binary | CoreType::String) => Some(expected.clone()),
        _ => None,
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
    let candidates = templates
        .get(&(function.to_string(), arity))
        .or_else(|| templates.get(&(format!("{module}.{function}"), arity)))?;
    Some(candidates.as_slice())
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
        ) if a.rsplit('.').next() == b.rsplit('.').next() && x.len() == y.len() => {
            for (left, right) in x.iter().zip(y) {
                unify(left, right, generic_params, substitution)?;
            }
            Ok(())
        }
        (CoreType::List(left), CoreType::List(right)) => {
            unify(left, right, generic_params, substitution)
        }
        (
            CoreType::Arrow {
                params: left_params,
                return_type: left_return,
            },
            CoreType::Arrow {
                params: right_params,
                return_type: right_return,
            },
        ) if left_params.len() == right_params.len() => {
            for (left, right) in left_params.iter().zip(right_params) {
                unify(left, right, generic_params, substitution)?;
            }
            unify(left_return, right_return, generic_params, substitution)
        }
        (CoreType::Tuple(left), CoreType::Tuple(right)) if left.len() == right.len() => {
            for (left, right) in left.iter().zip(right) {
                let left = match left {
                    CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
                };
                let right = match right {
                    CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
                };
                unify(left, right, generic_params, substitution)?;
            }
            Ok(())
        }
        (CoreType::Union(left), CoreType::Union(right)) if left.len() == right.len() => {
            for (left, right) in left.iter().zip(right) {
                unify(left, right, generic_params, substitution)?;
            }
            Ok(())
        }
        (CoreType::Map(left), CoreType::Map(right)) if left.len() == right.len() => {
            for (left, right) in left.iter().zip(right) {
                if left.key != right.key || left.operator != right.operator {
                    return Err(format!(
                        "error[native_ir.generic_unification]: map field `{}` does not match `{}`",
                        left.key, right.key
                    ));
                }
                unify(&left.value, &right.value, generic_params, substitution)?;
            }
            Ok(())
        }
        (
            CoreType::Struct {
                name: left_name,
                fields: left,
            },
            CoreType::Struct {
                name: right_name,
                fields: right,
            },
        ) if left_name == right_name && left.len() == right.len() => {
            for (left, right) in left.iter().zip(right) {
                if left.name != right.name || left.is_private != right.is_private {
                    return Err(format!(
                        "error[native_ir.generic_unification]: struct field `{}` does not match `{}`",
                        left.name, right.name
                    ));
                }
                unify(&left.ty, &right.ty, generic_params, substitution)?;
            }
            Ok(())
        }
        (CoreType::Struct { name, .. }, CoreType::Named(concrete))
        | (CoreType::Named(concrete), CoreType::Struct { name, .. })
            if name == concrete =>
        {
            Ok(())
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
