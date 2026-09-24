//! Lifts nested lambda arguments into owned-closure factory functions.

use std::collections::HashMap;

use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreModule, CoreParam, CorePattern, CoreProofCoverage,
    CoreTupleTypeElem, CoreType,
};

type Signatures = HashMap<(String, usize), (Vec<CoreType>, CoreType)>;

/// Stable source owner and callable signatures used while lifting closures.
#[derive(Clone, Copy)]
struct ClosureLiftEnvironment<'a> {
    signatures: &'a Signatures,
    module: &'a str,
    owner: &'a CoreFunction,
}

pub(super) fn lift_nested_closure_arguments(cores: &mut [CoreModule]) -> Result<(), String> {
    let signatures = cores
        .iter()
        .flat_map(|core| {
            core.functions.iter().filter_map(move |function| {
                let params = function
                    .params
                    .iter()
                    .map(|param| param.core_ty.clone())
                    .collect::<Option<Vec<_>>>()?;
                let result = function.core_return_type.clone()?;
                Some((
                    (format!("{}.{}", core.module, function.name), function.arity),
                    (params, result),
                ))
            })
        })
        .collect::<Signatures>();
    for core in cores.iter() {
        for function in &core.functions {
            if function.name.starts_with("$aot_comprehension_")
                && !signatures
                    .contains_key(&(format!("{}.{}", core.module, function.name), function.arity))
            {
                return Err(format!(
                    "error[native_ir.closure_signature]: generated helper `{}.{}/{}` lost its closed signature (parameters={}, result={})",
                    core.module,
                    function.name,
                    function.arity,
                    function.params.iter().all(|param| param.core_ty.is_some()),
                    function.core_return_type.is_some()
                ));
            }
        }
    }

    for core in cores {
        let module = core.module.clone();
        let mut cursor = 0;
        let mut ordinal = 0_u64;
        while cursor < core.functions.len() {
            let owner = core.functions[cursor].clone();
            let mut generated = Vec::new();
            let mut variables = owner
                .params
                .iter()
                .filter_map(|param| param.core_ty.clone().map(|ty| (param.name.clone(), ty)))
                .collect::<HashMap<_, _>>();
            for clause in &owner.clauses {
                for pattern in clause.core_patterns.iter().flatten() {
                    if let CorePattern::Var(name) = pattern {
                        if let Some(ty) = owner
                            .params
                            .iter()
                            .find(|param| param.name == *name)
                            .and_then(|param| param.core_ty.clone())
                        {
                            variables.insert(name.clone(), ty);
                        }
                    }
                }
            }
            for clause in &mut core.functions[cursor].clauses {
                if let Some(body) = clause.body.core_expr.as_mut() {
                    // Whole-result references already use the ordinary closure
                    // lowerer; lifting a generated factory's result again would
                    // manufacture an unbounded chain of identical factories.
                    if matches!(owner.core_return_type, Some(CoreType::Arrow { .. }))
                        && named_function_reference(body, &variables)
                    {
                        continue;
                    }
                    if let (
                        CoreExpr::Lam { params, body, .. },
                        Some(CoreType::Arrow {
                            params: parameter_types,
                            ..
                        }),
                    ) = (&mut *body, owner.core_return_type.as_ref())
                    {
                        let mut lambda_variables = variables.clone();
                        for (pattern, ty) in params.iter().zip(parameter_types) {
                            if let CorePattern::Var(name) = pattern {
                                lambda_variables.insert(name.clone(), ty.clone());
                            }
                        }
                        rewrite(
                            body,
                            None,
                            &lambda_variables,
                            ClosureLiftEnvironment {
                                signatures: &signatures,
                                module: &module,
                                owner: &owner,
                            },
                            &mut generated,
                            &mut ordinal,
                        )?;
                    } else {
                        rewrite(
                            body,
                            owner.core_return_type.as_ref(),
                            &variables,
                            ClosureLiftEnvironment {
                                signatures: &signatures,
                                module: &module,
                                owner: &owner,
                            },
                            &mut generated,
                            &mut ordinal,
                        )?;
                    }
                }
            }
            core.functions.extend(generated);
            cursor += 1;
        }
    }
    Ok(())
}
fn rewrite(
    expr: &mut CoreExpr,
    expected: Option<&CoreType>,
    variables: &HashMap<String, CoreType>,
    environment: ClosureLiftEnvironment<'_>,
    generated: &mut Vec<CoreFunction>,
    ordinal: &mut u64,
) -> Result<(), String> {
    let ClosureLiftEnvironment {
        signatures,
        module,
        owner,
    } = environment;
    if let CoreExpr::Call { function, args, .. } = expr {
        if matches!(variables.get(function), Some(CoreType::Arrow { .. })) {
            *expr = CoreExpr::FunctionCall {
                callee: Box::new(CoreExpr::Var(function.clone())),
                args: std::mem::take(args),
            };
        }
    }
    if (matches!(expr, CoreExpr::Lam { .. }) || named_function_reference(expr, variables))
        && matches!(expected, Some(CoreType::Arrow { .. }))
    {
        let lambda = expr.clone();
        let mut captures = super::free_variables(&lambda)
            .into_iter()
            .filter(|name| variables.contains_key(name))
            .collect::<Vec<_>>();
        captures.sort();
        captures.dedup();
        let name = format!("$aot_closure_factory_{}_{}", owner.name, *ordinal);
        *ordinal = ordinal.saturating_add(1);
        let factory = closure_factory(
            owner,
            name.clone(),
            &captures,
            variables,
            expected.expect("callable expected type"),
            lambda,
        )?;
        generated.push(factory);
        *expr = CoreExpr::Call {
            type_args: Vec::new(),
            function: name,
            args: captures.into_iter().map(CoreExpr::Var).collect(),
        };
        return Ok(());
    }

    match expr {
        CoreExpr::Call { function, args, .. } => {
            let expected = signature(signatures, module, function, args.len())
                .map(|signature| signature.0.clone());
            if args
                .iter()
                .any(|argument| matches!(argument, CoreExpr::Lam { .. }))
                && expected.is_none()
            {
                return Err(format!(
                    "error[native_ir.closure_signature]: `{function}/{}` has a direct lambda argument but no concrete callable signature",
                    args.len()
                ));
            }
            for (index, arg) in args.iter_mut().enumerate() {
                if matches!(arg, CoreExpr::Lam { .. })
                    && !matches!(
                        expected.as_ref().and_then(|types| types.get(index)),
                        Some(CoreType::Arrow { .. })
                    )
                {
                    return Err(format!(
                        "error[native_ir.closure_signature]: argument {} of `{function}/{}` is a lambda without an arrow contract",
                        index + 1,
                        args.len()
                    ));
                }
                rewrite(
                    arg,
                    expected.as_ref().and_then(|types| types.get(index)),
                    variables,
                    environment,
                    generated,
                    ordinal,
                )?;
            }
        }
        CoreExpr::Let { bindings, body } => {
            let mut variables = variables.clone();
            // Calls name their callee separately; a variable occurrence means
            // a callable may be stored or passed onward. Conservatively retain
            // an owned closure for those uses, while immediate calls can still
            // use static beta reduction. This scan does not mutate the tree.
            let mut value_uses = std::collections::HashSet::new();
            let mut aliases = Vec::new();
            for binding in bindings.iter_mut() {
                if let (CorePattern::Var(name), CoreExpr::Var(source)) =
                    (&binding.pattern, &binding.value)
                {
                    aliases.push((name.clone(), source.clone()));
                } else {
                    stored_callable_uses(&mut binding.value, &mut value_uses);
                }
            }
            let terminal_alias = matches!((bindings.last(), body.as_ref()),
                (Some(binding), CoreExpr::Var(name)) if matches!(&binding.pattern, CorePattern::Var(bound) if bound == name));
            if !terminal_alias {
                stored_callable_uses(body, &mut value_uses);
            }
            for (name, source) in aliases.into_iter().rev() {
                if value_uses.contains(&name) {
                    value_uses.insert(source);
                }
            }
            for binding in bindings {
                let binding_type = infer(&binding.value, &variables, signatures, module);
                // A lambda used only as a callee stays eligible for static
                // beta reduction; stored or passed values need an owned factory.
                let expected_binding = if matches!(binding.value, CoreExpr::Lam { .. })
                    && matches!(&binding.pattern, CorePattern::Var(name) if !value_uses.contains(name))
                {
                    None
                } else {
                    binding_type.as_ref()
                };
                rewrite(
                    &mut binding.value,
                    expected_binding,
                    &variables,
                    environment,
                    generated,
                    ordinal,
                )?;
                let ty =
                    binding_type.or_else(|| infer(&binding.value, &variables, signatures, module));
                for name in
                    super::expression::free_variable_analysis::pattern_bound_names(&binding.pattern)
                {
                    variables.remove(&name);
                }
                if let Some(ty) = ty {
                    bind_pattern_variables(&binding.pattern, &ty, &mut variables);
                }
            }
            rewrite(body, expected, &variables, environment, generated, ordinal)?;
        }
        CoreExpr::Lam { params, body, .. } => {
            let mut variables = variables.clone();
            if let Some(CoreType::Arrow {
                params: parameter_types,
                ..
            }) = expected
            {
                for (pattern, ty) in params.iter().zip(parameter_types) {
                    if let CorePattern::Var(name) = pattern {
                        variables.insert(name.clone(), ty.clone());
                    }
                }
            }
            rewrite(body, None, &variables, environment, generated, ordinal)?;
        }
        CoreExpr::Tuple(items) => {
            let types = expected.and_then(|ty| {
                super::collection_intrinsic_specialization::contextual_tuple_elements(items, ty)
            });
            for (index, item) in items.iter_mut().enumerate() {
                rewrite(
                    item,
                    types.as_ref().and_then(|types| types.get(index).copied()),
                    variables,
                    environment,
                    generated,
                    ordinal,
                )?;
            }
        }
        CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            let element =
                expected.and_then(super::collection_intrinsic_specialization::list_element);
            for item in items {
                rewrite(item, element, variables, environment, generated, ordinal)?;
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
            rewrite(head, None, variables, environment, generated, ordinal)?;
            rewrite(tail, None, variables, environment, generated, ordinal)?;
        }
        CoreExpr::Intrinsic(call) => {
            let expected = matches!(
                call.id,
                crate::terlan_typeck::CoreIntrinsicId::Primitive(
                    crate::terlan_typeck::CorePrimitiveIntrinsic::ListNew
                )
            )
            .then_some(&call.return_type);
            for arg in &mut call.args {
                rewrite(arg, expected, variables, environment, generated, ordinal)?;
            }
        }
        CoreExpr::RemoteCall { args, .. } | CoreExpr::ConstructorCall { args, .. } => {
            for arg in args {
                rewrite(arg, None, variables, environment, generated, ordinal)?;
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            rewrite(callee, None, variables, environment, generated, ordinal)?;
            for arg in args {
                rewrite(arg, None, variables, environment, generated, ordinal)?;
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            rewrite(receiver, None, variables, environment, generated, ordinal)?;
            for arg in args {
                rewrite(arg, None, variables, environment, generated, ordinal)?;
            }
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                rewrite(
                    &mut field.value,
                    None,
                    variables,
                    environment,
                    generated,
                    ordinal,
                )?;
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                rewrite(
                    &mut field.value,
                    None,
                    variables,
                    environment,
                    generated,
                    ordinal,
                )?;
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            rewrite(base, None, variables, environment, generated, ordinal)?;
            for field in fields {
                rewrite(
                    &mut field.value,
                    None,
                    variables,
                    environment,
                    generated,
                    ordinal,
                )?;
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => {
            rewrite(base, None, variables, environment, generated, ordinal)?
        }
        CoreExpr::Cast { expr, target_type } => rewrite(
            expr,
            Some(target_type),
            variables,
            environment,
            generated,
            ordinal,
        )?,
        CoreExpr::Case { scrutinee, clauses } => {
            let scrutinee_type = infer(scrutinee, variables, signatures, module);
            rewrite(scrutinee, None, variables, environment, generated, ordinal)?;
            for clause in clauses {
                let mut clause_variables = variables.clone();
                if let Some(scrutinee_type) = scrutinee_type.as_ref() {
                    bind_pattern_variables(&clause.pattern, scrutinee_type, &mut clause_variables);
                }
                if let Some(guard) = &mut clause.guard {
                    rewrite(
                        guard,
                        None,
                        &clause_variables,
                        environment,
                        generated,
                        ordinal,
                    )?;
                }
                rewrite(
                    &mut clause.body,
                    expected,
                    &clause_variables,
                    environment,
                    generated,
                    ordinal,
                )?;
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                rewrite(
                    &mut clause.condition,
                    None,
                    variables,
                    environment,
                    generated,
                    ordinal,
                )?;
                rewrite(
                    &mut clause.body,
                    expected,
                    variables,
                    environment,
                    generated,
                    ordinal,
                )?;
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                rewrite(arg, None, variables, environment, generated, ordinal)?;
            }
            rewrite(record, None, variables, environment, generated, ordinal)?;
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            rewrite(expr, None, variables, environment, generated, ordinal)?;
            for generator in generators {
                rewrite(
                    &mut generator.source,
                    None,
                    variables,
                    environment,
                    generated,
                    ordinal,
                )?;
            }
            for guard in guards {
                rewrite(guard, None, variables, environment, generated, ordinal)?;
            }
        }
        CoreExpr::Try { .. } | CoreExpr::SqlQuery { .. } => {}
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
    Ok(())
}

fn named_function_reference(expr: &CoreExpr, variables: &HashMap<String, CoreType>) -> bool {
    match expr {
        CoreExpr::Var(name) => !variables.contains_key(name),
        CoreExpr::RemoteFunRef { .. } => true,
        _ => false,
    }
}

/// Records value uses while leaving direct calls eligible for static lowering.
fn stored_callable_uses(expr: &mut CoreExpr, names: &mut std::collections::HashSet<String>) {
    match expr {
        CoreExpr::Var(name) => {
            names.insert(name.clone());
        }
        CoreExpr::FunctionCall { callee, args } if matches!(callee.as_ref(), CoreExpr::Var(_)) => {
            for arg in args {
                stored_callable_uses(arg, names);
            }
        }
        _ => crate::terlan_typeck::visit_core_expr_children_mut(expr, &mut |child| {
            stored_callable_uses(child, names);
        }),
    }
}

fn bind_pattern_variables(
    pattern: &CorePattern,
    ty: &CoreType,
    variables: &mut HashMap<String, CoreType>,
) {
    super::generic_specialization::bind_pattern_types(pattern, ty, variables);
}

fn closure_factory(
    owner: &CoreFunction,
    name: String,
    captures: &[String],
    variables: &HashMap<String, CoreType>,
    return_type: &CoreType,
    lambda: CoreExpr,
) -> Result<CoreFunction, String> {
    let mut factory = owner.clone();
    factory.name = name;
    factory.public = false;
    factory.generic_params.clear();
    factory.native_operation = None;
    factory.trait_method = None;
    factory.receiver_method = false;
    factory.params = captures
        .iter()
        .map(|name| {
            let ty = variables.get(name).cloned().ok_or_else(|| {
                format!("error[native_ir.closure_capture_type]: `{name}` has no concrete type")
            })?;
            Ok(CoreParam {
                name: name.clone(),
                ty: ty.contract_text(),
                core_ty: Some(ty),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    factory.arity = factory.params.len();
    factory.return_type = return_type.contract_text();
    factory.core_return_type = Some(return_type.clone());
    factory.clauses.truncate(1);
    let clause = factory
        .clauses
        .first_mut()
        .ok_or_else(|| "error[native_ir.closure_factory]: owner has no clause".to_string())?;
    clause.patterns = captures.to_vec();
    clause.core_patterns = captures
        .iter()
        .cloned()
        .map(CorePattern::Var)
        .map(Some)
        .collect();
    clause.pattern_proof_coverage = vec![CoreProofCoverage::RuntimeBoundary; captures.len()];
    clause.pattern_checked_preservation_evidence = vec![None; captures.len()];
    clause.guard = None;
    clause.body.core_expr = Some(lambda);
    clause.body.proof_coverage = CoreProofCoverage::RuntimeBoundary;
    clause.body.checked_preservation_evidence = None;
    Ok(factory)
}

fn signature<'a>(
    signatures: &'a Signatures,
    module: &str,
    function: &str,
    arity: usize,
) -> Option<&'a (Vec<CoreType>, CoreType)> {
    signatures
        .get(&(function.to_string(), arity))
        .or_else(|| signatures.get(&(format!("{module}.{function}"), arity)))
        .or_else(|| {
            let mut matches = signatures
                .iter()
                .filter_map(|((name, candidate_arity), value)| {
                    (*candidate_arity == arity
                        && name.rsplit('.').next() == function.rsplit('.').next())
                    .then_some(value)
                });
            let first = matches.next()?;
            matches.all(|candidate| candidate == first).then_some(first)
        })
}

fn infer(
    expr: &CoreExpr,
    variables: &HashMap<String, CoreType>,
    signatures: &Signatures,
    module: &str,
) -> Option<CoreType> {
    match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(_) => Some(CoreType::Atom),
        CoreExpr::Var(name) => variables.get(name).cloned(),
        CoreExpr::Intrinsic(call) => Some(call.return_type.clone()),
        CoreExpr::Call { function, args, .. } => {
            signature(signatures, module, function, args.len()).map(|signature| signature.1.clone())
        }
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        CoreExpr::Lam {
            params,
            parameter_types,
            body,
        } => {
            if params.len() != parameter_types.len() {
                return None;
            }
            let parameter_types = parameter_types
                .iter()
                .cloned()
                .collect::<Option<Vec<_>>>()?;
            let mut variables = variables.clone();
            for (pattern, ty) in params.iter().zip(&parameter_types) {
                bind_pattern_variables(pattern, ty, &mut variables);
            }
            Some(CoreType::Arrow {
                params: parameter_types,
                return_type: Box::new(infer(body, &variables, signatures, module)?),
            })
        }
        CoreExpr::If { clauses } => {
            let mut types = clauses
                .iter()
                .map(|clause| infer(&clause.body, variables, signatures, module));
            let first = types.next()??;
            types.all(|ty| ty.as_ref() == Some(&first)).then_some(first)
        }
        CoreExpr::FunctionCall { callee, .. } => {
            match infer(callee, variables, signatures, module)? {
                CoreType::Arrow { return_type, .. } => Some(*return_type),
                _ => None,
            }
        }
        CoreExpr::Let { bindings, body } => {
            let mut variables = variables.clone();
            for binding in bindings {
                let ty = infer(&binding.value, &variables, signatures, module)?;
                bind_pattern_variables(&binding.pattern, &ty, &mut variables);
            }
            infer(body, &variables, signatures, module)
        }
        CoreExpr::List(items) if !items.is_empty() => {
            infer(&items[0], variables, signatures, module).map(|ty| CoreType::List(Box::new(ty)))
        }
        CoreExpr::Tuple(items) => items
            .iter()
            .map(|item| infer(item, variables, signatures, module).map(CoreTupleTypeElem::Type))
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Tuple),
        CoreExpr::UnaryOp { operand, .. } => infer(operand, variables, signatures, module),
        CoreExpr::BinaryOp { operator, left, .. }
            if matches!(
                operator.as_str(),
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "and" | "or"
            ) =>
        {
            Some(CoreType::Bool)
        }
        CoreExpr::BinaryOp { left, .. } => infer(left, variables, signatures, module),
        _ => None,
    }
}
