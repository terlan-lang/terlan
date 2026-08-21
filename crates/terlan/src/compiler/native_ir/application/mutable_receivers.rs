//! Type-directed closure of mutable receiver calls.

use std::collections::HashMap;

use crate::terlan_typeck::{core_type_from_text, CoreExpr, CoreModule, CorePattern, CoreType};

#[derive(Clone)]
struct ReceiverTarget {
    module: String,
    function: String,
    receiver: CoreType,
    public: bool,
}

/// Resolves mutable receiver syntax to one exact application callable.
///
/// Typechecking has already selected a receiver-method signature, but legacy
/// CoreIR stores only the source method name. Application linking can expose
/// several owners of names such as `put/3`; this pass reconstructs the exact
/// owner from the canonical checked receiver type before native admission.
pub(crate) fn resolve_typed_mutable_receiver_calls(
    cores: &mut [CoreModule],
) -> super::super::NativeIrResult<()> {
    let targets = receiver_targets(cores);
    let qualified_results = function_results(cores);
    for core in cores {
        let mut results = qualified_results.clone();
        for function in &core.functions {
            if let Some(result) = function
                .core_return_type
                .clone()
                .or_else(|| core_type_from_text(&function.return_type))
            {
                results.insert((function.name.clone(), function.arity), result);
            }
        }
        let module = core.module.clone();
        for function in &mut core.functions {
            let variables = function
                .params
                .iter()
                .filter_map(|parameter| {
                    parameter
                        .core_ty
                        .clone()
                        .map(|ty| (parameter.name.clone(), ty))
                })
                .collect::<HashMap<_, _>>();
            for clause in &mut function.clauses {
                if let Some(guard) = clause
                    .guard
                    .as_mut()
                    .and_then(|summary| summary.core_expr.as_mut())
                {
                    resolve_expr(guard, &module, &variables, &results, &targets)?;
                }
                if let Some(body) = clause.body.core_expr.as_mut() {
                    resolve_expr(body, &module, &variables, &results, &targets)?;
                }
            }
        }
    }
    Ok(())
}

fn receiver_targets(cores: &[CoreModule]) -> HashMap<(String, usize), Vec<ReceiverTarget>> {
    let mut targets = HashMap::<(String, usize), Vec<ReceiverTarget>>::new();
    for core in cores {
        for function in &core.functions {
            let Some(receiver) = function.params.first().and_then(|parameter| {
                parameter
                    .core_ty
                    .clone()
                    .or_else(|| core_type_from_text(&parameter.ty))
            }) else {
                continue;
            };
            targets
                .entry((function.name.clone(), function.arity))
                .or_default()
                .push(ReceiverTarget {
                    module: core.module.clone(),
                    function: function.name.clone(),
                    receiver,
                    public: function.public,
                });
        }
    }
    targets
}

fn function_results(cores: &[CoreModule]) -> HashMap<(String, usize), CoreType> {
    cores
        .iter()
        .flat_map(|core| {
            core.functions.iter().filter_map(move |function| {
                function
                    .core_return_type
                    .clone()
                    .or_else(|| core_type_from_text(&function.return_type))
                    .map(|result| {
                        (
                            (format!("{}.{}", core.module, function.name), function.arity),
                            result,
                        )
                    })
            })
        })
        .collect()
}

fn resolve_expr(
    expr: &mut CoreExpr,
    module: &str,
    variables: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
    targets: &HashMap<(String, usize), Vec<ReceiverTarget>>,
) -> super::super::NativeIrResult<()> {
    match expr {
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            ..
        } => {
            resolve_expr(receiver, module, variables, functions, targets)?;
            for argument in args.iter_mut() {
                resolve_expr(argument, module, variables, functions, targets)?;
            }
            let Some(receiver_type) = infer_core_type(receiver, variables, functions) else {
                return Ok(());
            };
            if let Some(target) =
                receiver_target(method, args.len() + 1, &receiver_type, module, targets)
            {
                let receiver =
                    std::mem::replace(receiver.as_mut(), CoreExpr::Atom("Unit".to_string()));
                let mut call_args = vec![receiver];
                call_args.append(args);
                *expr = CoreExpr::Call {
                    function: if target.module == module {
                        target.function.clone()
                    } else {
                        format!("{}.{}", target.module, target.function)
                    },
                    args: call_args,
                };
            }
        }
        CoreExpr::RemoteCall {
            module: receiver_module,
            function,
            args,
        } if receiver_module == "__receiver__" => {
            for argument in args.iter_mut() {
                resolve_expr(argument, module, variables, functions, targets)?;
            }
            let target = args
                .first()
                .and_then(|receiver| infer_core_type(receiver, variables, functions))
                .and_then(|receiver_type| {
                    receiver_target(function, args.len(), &receiver_type, module, targets)
                });
            if let Some(target) = target {
                *expr = CoreExpr::Call {
                    function: callable_identity(target, module),
                    args: std::mem::take(args),
                };
            }
        }
        CoreExpr::Let { bindings, body } => {
            let mut locals = variables.clone();
            for binding in bindings {
                resolve_expr(&mut binding.value, module, &locals, functions, targets)?;
                if let CorePattern::Var(name) = &binding.pattern {
                    if let Some(ty) = infer_core_type(&binding.value, &locals, functions) {
                        locals.insert(name.clone(), ty);
                    }
                }
            }
            resolve_expr(body, module, &locals, functions, targets)?;
        }
        CoreExpr::Call { function, args } => {
            for argument in args.iter_mut() {
                resolve_expr(argument, module, variables, functions, targets)?;
            }
            if !function.contains('.') {
                let target = args
                    .first()
                    .and_then(|receiver| infer_core_type(receiver, variables, functions))
                    .and_then(|receiver_type| {
                        receiver_target(function, args.len(), &receiver_type, module, targets)
                    });
                if let Some(target) = target {
                    *function = callable_identity(target, module);
                }
            }
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            for argument in args.iter_mut() {
                resolve_expr(argument, module, variables, functions, targets)?;
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            resolve_expr(callee, module, variables, functions, targets)?;
            for argument in args {
                resolve_expr(argument, module, variables, functions, targets)?;
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                resolve_expr(item, module, variables, functions, targets)?;
            }
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::BinaryOp {
            left: head,
            right: tail,
            ..
        } => {
            resolve_expr(head, module, variables, functions, targets)?;
            resolve_expr(tail, module, variables, functions, targets)?;
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                resolve_expr(&mut field.value, module, variables, functions, targets)?;
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                resolve_expr(&mut field.value, module, variables, functions, targets)?;
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            resolve_expr(base, module, variables, functions, targets)?;
            for field in fields {
                resolve_expr(&mut field.value, module, variables, functions, targets)?;
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::Lam { body: base, .. } => {
            resolve_expr(base, module, variables, functions, targets)?;
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                resolve_expr(&mut clause.condition, module, variables, functions, targets)?;
                resolve_expr(&mut clause.body, module, variables, functions, targets)?;
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            resolve_expr(scrutinee, module, variables, functions, targets)?;
            let scrutinee_type = infer_core_type(scrutinee, variables, functions);
            for clause in clauses {
                let mut locals = variables.clone();
                if let Some(scrutinee_type) = &scrutinee_type {
                    super::super::generic_specialization::bind_pattern_types(
                        &clause.pattern,
                        scrutinee_type,
                        &mut locals,
                    );
                }
                if let Some(guard) = &mut clause.guard {
                    resolve_expr(guard, module, &locals, functions, targets)?;
                }
                resolve_expr(&mut clause.body, module, &locals, functions, targets)?;
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            resolve_expr(body, module, variables, functions, targets)?;
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    resolve_expr(guard, module, variables, functions, targets)?;
                }
                resolve_expr(&mut clause.body, module, variables, functions, targets)?;
            }
            if let Some(after) = after_clause {
                resolve_expr(&mut after.trigger, module, variables, functions, targets)?;
                resolve_expr(&mut after.body, module, variables, functions, targets)?;
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for argument in args {
                resolve_expr(argument, module, variables, functions, targets)?;
            }
            resolve_expr(record, module, variables, functions, targets)?;
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            resolve_expr(expr, module, variables, functions, targets)?;
            for generator in generators {
                resolve_expr(&mut generator.source, module, variables, functions, targets)?;
            }
            for guard in guards {
                resolve_expr(guard, module, variables, functions, targets)?;
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            for parameter in parameters {
                resolve_expr(parameter, module, variables, functions, targets)?;
            }
        }
        CoreExpr::Index { base, index } => {
            resolve_expr(base, module, variables, functions, targets)?;
            resolve_expr(index, module, variables, functions, targets)?;
        }
        CoreExpr::Atom(_)
        | CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
    Ok(())
}

fn receiver_target<'a>(
    method: &str,
    arity: usize,
    receiver_type: &CoreType,
    caller_module: &str,
    targets: &'a HashMap<(String, usize), Vec<ReceiverTarget>>,
) -> Option<&'a ReceiverTarget> {
    let identity = (method.to_string(), arity);
    let mut matches = targets
        .get(&identity)
        .into_iter()
        .flatten()
        .filter(|target| {
            receiver_types_match(&target.receiver, receiver_type)
                && (target.public || target.module == caller_module)
        });
    if std::env::var_os("TERLAN_NATIVE_AOT_TRACE").is_some() {
        eprintln!(
            "[native-aot receiver] module={caller_module} method={method}/{arity} receiver={} candidates={:?}",
            receiver_type.contract_text(),
            targets
                .get(&identity)
                .into_iter()
                .flatten()
                .map(|target| (
                    target.module.as_str(),
                    target.receiver.contract_text(),
                    target.public,
                ))
                .collect::<Vec<_>>(),
        );
    }
    let target = matches.next()?;
    matches.next().is_none().then_some(target)
}

fn callable_identity(target: &ReceiverTarget, caller_module: &str) -> String {
    if target.module == caller_module {
        target.function.clone()
    } else {
        format!("{}.{}", target.module, target.function)
    }
}

/// Compares receiver types after nominal qualification and opaque-type
/// expansion have exposed equivalent short and structural names.
fn receiver_types_match(expected: &CoreType, actual: &CoreType) -> bool {
    if expected == actual {
        return true;
    }
    let (Some(expected), Some(actual)) = (nominal_name(expected), nominal_name(actual)) else {
        return false;
    };
    expected == actual
        || ((!expected.contains('.') || !actual.contains('.'))
            && expected.rsplit('.').next() == actual.rsplit('.').next())
}

/// Returns the declared name carried by a nominal or expanded structural type.
fn nominal_name(ty: &CoreType) -> Option<&str> {
    match ty {
        CoreType::Named(name) | CoreType::Struct { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

fn infer_core_type(
    expr: &CoreExpr,
    variables: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
) -> Option<CoreType> {
    match expr {
        CoreExpr::Var(name) => variables.get(name).cloned(),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(_) => Some(CoreType::Atom),
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Intrinsic(call) => Some(call.return_type.clone()),
        CoreExpr::Call { function, args } => {
            functions.get(&(function.clone(), args.len())).cloned()
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => functions
            .get(&(format!("{module}.{function}"), args.len()))
            .cloned(),
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        CoreExpr::List(items) => items
            .first()
            .and_then(|item| infer_core_type(item, variables, functions))
            .map(|item| CoreType::List(Box::new(item))),
        CoreExpr::ListCons { tail, .. } => infer_core_type(tail, variables, functions),
        CoreExpr::Tuple(items) => Some(CoreType::Tuple(
            items
                .iter()
                .map(|item| {
                    infer_core_type(item, variables, functions)
                        .map(crate::terlan_typeck::CoreTupleTypeElem::Type)
                })
                .collect::<Option<Vec<_>>>()?,
        )),
        CoreExpr::Let { bindings, body } => {
            let mut locals = variables.clone();
            for binding in bindings {
                let CorePattern::Var(name) = &binding.pattern else {
                    return None;
                };
                let ty = infer_core_type(&binding.value, &locals, functions)?;
                locals.insert(name.clone(), ty);
            }
            infer_core_type(body, &locals, functions)
        }
        CoreExpr::If { clauses } => common_branch_type(
            clauses
                .iter()
                .map(|clause| infer_core_type(&clause.body, variables, functions)),
        ),
        CoreExpr::Case { clauses, .. } => common_branch_type(
            clauses
                .iter()
                .map(|clause| infer_core_type(&clause.body, variables, functions)),
        ),
        _ => None,
    }
}

fn common_branch_type(types: impl Iterator<Item = Option<CoreType>>) -> Option<CoreType> {
    let mut types = types;
    let first = types.next()??;
    types
        .all(|candidate| candidate.as_ref() == Some(&first))
        .then_some(first)
}

#[cfg(test)]
#[path = "mutable_receivers_test.rs"]
mod tests;
