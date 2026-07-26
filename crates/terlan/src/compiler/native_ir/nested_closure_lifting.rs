//! Lifts nested lambda arguments into owned-closure factory functions.

use std::collections::HashMap;

use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreModule, CoreParam, CorePattern, CoreProofCoverage,
    CoreTupleTypeElem, CoreType,
};

type Signatures = HashMap<(String, usize), (Vec<CoreType>, CoreType)>;

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
                    if let (
                        CoreExpr::Lam { params, body },
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
                            &signatures,
                            &module,
                            &owner,
                            &mut generated,
                            &mut ordinal,
                        )?;
                    } else {
                        rewrite(
                            body,
                            owner.core_return_type.as_ref(),
                            &variables,
                            &signatures,
                            &module,
                            &owner,
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

#[allow(clippy::too_many_arguments)]
fn rewrite(
    expr: &mut CoreExpr,
    expected: Option<&CoreType>,
    variables: &HashMap<String, CoreType>,
    signatures: &Signatures,
    module: &str,
    owner: &CoreFunction,
    generated: &mut Vec<CoreFunction>,
    ordinal: &mut u64,
) -> Result<(), String> {
    if matches!(expr, CoreExpr::Lam { .. }) && matches!(expected, Some(CoreType::Arrow { .. })) {
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
            expected.expect("lambda expected type"),
            lambda,
        )?;
        generated.push(factory);
        *expr = CoreExpr::Call {
            function: name,
            args: captures.into_iter().map(CoreExpr::Var).collect(),
        };
        return Ok(());
    }

    match expr {
        CoreExpr::Call { function, args } => {
            let expected = signature(signatures, module, function, args.len())
                .map(|signature| signature.0.clone());
            for (index, arg) in args.iter_mut().enumerate() {
                rewrite(
                    arg,
                    expected.as_ref().and_then(|types| types.get(index)),
                    variables,
                    signatures,
                    module,
                    owner,
                    generated,
                    ordinal,
                )?;
            }
        }
        CoreExpr::Let { bindings, body } => {
            let mut variables = variables.clone();
            for binding in bindings {
                rewrite(
                    &mut binding.value,
                    None,
                    &variables,
                    signatures,
                    module,
                    owner,
                    generated,
                    ordinal,
                )?;
                if let CorePattern::Var(name) = &binding.pattern {
                    if let Some(ty) = infer(&binding.value, &variables, signatures, module) {
                        variables.insert(name.clone(), ty);
                    }
                }
            }
            rewrite(
                body, expected, &variables, signatures, module, owner, generated, ordinal,
            )?;
        }
        CoreExpr::Lam { params, body } => {
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
            rewrite(
                body, None, &variables, signatures, module, owner, generated, ordinal,
            )?;
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                rewrite(
                    item, None, variables, signatures, module, owner, generated, ordinal,
                )?;
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
            rewrite(
                head, None, variables, signatures, module, owner, generated, ordinal,
            )?;
            rewrite(
                tail, None, variables, signatures, module, owner, generated, ordinal,
            )?;
        }
        CoreExpr::Intrinsic(call) => {
            for arg in &mut call.args {
                rewrite(
                    arg, None, variables, signatures, module, owner, generated, ordinal,
                )?;
            }
        }
        CoreExpr::RemoteCall { args, .. } | CoreExpr::ConstructorCall { args, .. } => {
            for arg in args {
                rewrite(
                    arg, None, variables, signatures, module, owner, generated, ordinal,
                )?;
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            rewrite(
                callee, None, variables, signatures, module, owner, generated, ordinal,
            )?;
            for arg in args {
                rewrite(
                    arg, None, variables, signatures, module, owner, generated, ordinal,
                )?;
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            rewrite(
                receiver, None, variables, signatures, module, owner, generated, ordinal,
            )?;
            for arg in args {
                rewrite(
                    arg, None, variables, signatures, module, owner, generated, ordinal,
                )?;
            }
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                rewrite(
                    &mut field.value,
                    None,
                    variables,
                    signatures,
                    module,
                    owner,
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
                    signatures,
                    module,
                    owner,
                    generated,
                    ordinal,
                )?;
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            rewrite(
                base, None, variables, signatures, module, owner, generated, ordinal,
            )?;
            for field in fields {
                rewrite(
                    &mut field.value,
                    None,
                    variables,
                    signatures,
                    module,
                    owner,
                    generated,
                    ordinal,
                )?;
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => rewrite(
            base, None, variables, signatures, module, owner, generated, ordinal,
        )?,
        CoreExpr::Case { scrutinee, clauses } => {
            rewrite(
                scrutinee, None, variables, signatures, module, owner, generated, ordinal,
            )?;
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    rewrite(
                        guard, None, variables, signatures, module, owner, generated, ordinal,
                    )?;
                }
                rewrite(
                    &mut clause.body,
                    expected,
                    variables,
                    signatures,
                    module,
                    owner,
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
                    signatures,
                    module,
                    owner,
                    generated,
                    ordinal,
                )?;
                rewrite(
                    &mut clause.body,
                    expected,
                    variables,
                    signatures,
                    module,
                    owner,
                    generated,
                    ordinal,
                )?;
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                rewrite(
                    arg, None, variables, signatures, module, owner, generated, ordinal,
                )?;
            }
            rewrite(
                record, None, variables, signatures, module, owner, generated, ordinal,
            )?;
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            rewrite(
                expr, None, variables, signatures, module, owner, generated, ordinal,
            )?;
            for generator in generators {
                rewrite(
                    &mut generator.source,
                    None,
                    variables,
                    signatures,
                    module,
                    owner,
                    generated,
                    ordinal,
                )?;
            }
            for guard in guards {
                rewrite(
                    guard, None, variables, signatures, module, owner, generated, ordinal,
                )?;
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
        CoreExpr::Call { function, args } => {
            signature(signatures, module, function, args.len()).map(|signature| signature.1.clone())
        }
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
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
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "and" | "&&" | "or" | "||"
            ) =>
        {
            Some(CoreType::Bool)
        }
        CoreExpr::BinaryOp { left, .. } => infer(left, variables, signatures, module),
        _ => None,
    }
}
