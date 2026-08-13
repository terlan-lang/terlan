//! Compile-time normalization for completed Effect and GuardResult values.

use std::collections::HashMap;

use crate::terlan_typeck::{
    CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CorePattern, CorePrimitiveIntrinsic,
    CoreTupleTypeElem, CoreType,
};

use super::super::NativeIrResult;

const EFFECT_CONTAINER: &str = "std.core.Effect.Effect";
const EFFECT_SUCCEED: &str = "std.core.Effect.succeed";

/// Eliminates scheduler crossings for values proven to be completed effects.
pub(super) fn fold_completed_effect_runs(
    expr: &mut CoreExpr,
    completed: &HashMap<String, CoreExpr>,
) {
    if let CoreExpr::Let { bindings, body } = expr {
        let mut scoped = completed.clone();
        let mut rebuilt = Vec::with_capacity(bindings.len());
        let mut completed_bindings = Vec::new();
        for mut binding in std::mem::take(bindings) {
            fold_completed_effect_runs(&mut binding.value, &scoped);
            if let CorePattern::Var(name) = &binding.pattern {
                if let Some(value) = completed_effect_value(&binding.value) {
                    let alias = format!("$completed_effect_{name}");
                    rebuilt.push(crate::terlan_typeck::CoreLetBinding {
                        pattern: CorePattern::Var(alias.clone()),
                        value,
                    });
                    replace_completed_effect_value(
                        &mut binding.value,
                        CoreExpr::Var(alias.clone()),
                    );
                    scoped.insert(name.clone(), CoreExpr::Var(alias));
                    completed_bindings.push((name.clone(), rebuilt.len()));
                } else {
                    scoped.remove(name);
                }
            }
            rebuilt.push(binding);
        }
        fold_completed_effect_runs(body, &scoped);
        for (name, index) in completed_bindings {
            let referenced_later = rebuilt[index + 1..].iter().any(|binding| {
                super::super::expression::free_variables(&binding.value).contains(&name)
            }) || super::super::expression::free_variables(body)
                .contains(&name);
            if !referenced_later {
                rebuilt[index].value = CoreExpr::Atom("Unit".to_string());
            }
        }
        *bindings = rebuilt;
        return;
    }
    match expr {
        CoreExpr::Intrinsic(call)
            if matches!(
                call.id,
                CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmEffectRun)
            ) && call.args.len() == 1 =>
        {
            fold_completed_effect_runs(&mut call.args[0], completed);
            let replacement = match &call.args[0] {
                CoreExpr::Var(name) => completed.get(name).cloned(),
                argument => completed_effect_value(argument),
            };
            if let Some(replacement) = replacement {
                *expr = replacement;
            }
        }
        CoreExpr::Call { function, args }
            if function == "std.core.Effect.run" && args.len() == 1 =>
        {
            fold_completed_effect_runs(&mut args[0], completed);
            let replacement = match &args[0] {
                CoreExpr::Var(name) => completed.get(name).cloned(),
                argument => completed_effect_value(argument),
            };
            if let Some(replacement) = replacement {
                *expr = replacement;
            }
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == "std.core.Effect" && function == "run" && args.len() == 1 => {
            fold_completed_effect_runs(&mut args[0], completed);
            let replacement = match &args[0] {
                CoreExpr::Var(name) => completed.get(name).cloned(),
                argument => completed_effect_value(argument),
            };
            if let Some(replacement) = replacement {
                *expr = replacement;
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                fold_completed_effect_runs(item, completed);
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
            fold_completed_effect_runs(head, completed);
            fold_completed_effect_runs(tail, completed);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                fold_completed_effect_runs(&mut field.value, completed);
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                fold_completed_effect_runs(&mut field.value, completed);
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            fold_completed_effect_runs(base, completed);
            for field in fields {
                fold_completed_effect_runs(&mut field.value, completed);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => fold_completed_effect_runs(base, completed),
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                fold_completed_effect_runs(arg, completed);
            }
            fold_completed_effect_runs(record, completed);
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. }
        | CoreExpr::Intrinsic(CoreIntrinsicCall { args, .. }) => {
            for arg in args {
                fold_completed_effect_runs(arg, completed);
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. }
        | CoreExpr::FunctionCall {
            callee: receiver,
            args,
        } => {
            fold_completed_effect_runs(receiver, completed);
            for arg in args {
                fold_completed_effect_runs(arg, completed);
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            for parameter in parameters {
                fold_completed_effect_runs(parameter, completed);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            fold_completed_effect_runs(scrutinee, completed);
            for clause in clauses {
                if let Some(guard) = clause.guard.as_mut() {
                    fold_completed_effect_runs(guard, completed);
                }
                fold_completed_effect_runs(&mut clause.body, completed);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            fold_completed_effect_runs(body, completed);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = clause.guard.as_mut() {
                    fold_completed_effect_runs(guard, completed);
                }
                fold_completed_effect_runs(&mut clause.body, completed);
            }
            if let Some(after) = after_clause {
                fold_completed_effect_runs(&mut after.trigger, completed);
                fold_completed_effect_runs(&mut after.body, completed);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                fold_completed_effect_runs(&mut clause.condition, completed);
                fold_completed_effect_runs(&mut clause.body, completed);
            }
        }
        CoreExpr::Let { .. } => unreachable!("let expressions return after scoped folding"),
        CoreExpr::ListComprehension { .. }
        | CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}

fn completed_effect_value(expr: &CoreExpr) -> Option<CoreExpr> {
    match expr {
        CoreExpr::Call { function, args } if function == EFFECT_SUCCEED && args.len() == 1 => {
            Some(args[0].clone())
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == "std.core.Effect" && function == "succeed" && args.len() == 1 => {
            Some(args[0].clone())
        }
        CoreExpr::Cast { expr, .. } => completed_effect_value(expr),
        _ => None,
    }
}

fn replace_completed_effect_value(expr: &mut CoreExpr, replacement: CoreExpr) {
    match expr {
        CoreExpr::Call { function, args } if function == EFFECT_SUCCEED && args.len() == 1 => {
            args[0] = replacement;
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == "std.core.Effect" && function == "succeed" && args.len() == 1 => {
            args[0] = replacement;
        }
        CoreExpr::Cast { expr, .. } => replace_completed_effect_value(expr, replacement),
        _ => unreachable!("completed effect value was recognized before replacement"),
    }
}

/// Converts completed `Effect.succeed(Bool)` filters back to pure decisions.
pub(crate) fn lower_completed_effect_guards(guards: &mut [CoreExpr]) -> NativeIrResult<()> {
    for guard in guards {
        let completed = match guard {
            CoreExpr::Call { function, args } if function == EFFECT_SUCCEED && args.len() == 1 => {
                Some(args[0].clone())
            }
            CoreExpr::RemoteCall {
                module,
                function,
                args,
            } if module == "std.core.Effect" && function == "succeed" && args.len() == 1 => {
                Some(args[0].clone())
            }
            CoreExpr::Call { function, .. } if function == "std.core.Effect.fail" => {
                return Err(
                    "error[vm_comprehension_guard_failed]: a failed deferred guard cannot cross the direct-AOT scheduler boundary without continuation lowering"
                        .into(),
                );
            }
            CoreExpr::RemoteCall {
                module, function, ..
            } if module == "std.core.Effect" && function == "fail" => {
                return Err(
                    "error[vm_comprehension_guard_failed]: a failed deferred guard cannot cross the direct-AOT scheduler boundary without continuation lowering"
                        .into(),
                );
            }
            CoreExpr::Call { function, .. } if function == "std.core.Effect.cancelled" => {
                return Err(
                    "error[vm_comprehension_guard_cancelled]: a cancelled deferred guard cannot cross the direct-AOT scheduler boundary without continuation lowering"
                        .into(),
                );
            }
            CoreExpr::RemoteCall {
                module, function, ..
            } if module == "std.core.Effect" && function == "cancelled" => {
                return Err(
                    "error[vm_comprehension_guard_cancelled]: a cancelled deferred guard cannot cross the direct-AOT scheduler boundary without continuation lowering"
                        .into(),
                );
            }
            CoreExpr::Call { function, .. } if function.starts_with("std.core.Effect.") => {
                return Err(format!(
                    "error[native_ir.comprehension_effect]: deferred effect guard `{function}` requires scheduler continuation lowering"
                )
                .into());
            }
            CoreExpr::RemoteCall {
                module, function, ..
            } if module == "std.core.Effect" => {
                return Err(format!(
                    "error[native_ir.comprehension_effect]: deferred effect guard `{module}.{function}` requires scheduler continuation lowering"
                )
                .into());
            }
            _ => None,
        };
        if let Some(completed) = completed {
            *guard = completed;
        }
    }
    Ok(())
}

/// Erases the zero-work `GuardResult.Completed` wrapper before a native branch.
pub(crate) fn lower_completed_guard_results(guards: &mut [CoreExpr]) {
    for guard in guards {
        if let Some(decision) = completed_guard_decision(guard) {
            *guard = decision;
        }
    }
}

fn completed_guard_decision(expr: &CoreExpr) -> Option<CoreExpr> {
    match expr {
        CoreExpr::Cast { expr, .. } => completed_guard_decision(expr),
        CoreExpr::Call { function, args } => guard_result_call(function, args),
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == "std.core.GuardResult" => {
            guard_result_call(&format!("{module}.{function}"), args)
        }
        _ => None,
    }
}

fn guard_result_call(function: &str, args: &[CoreExpr]) -> Option<CoreExpr> {
    let function = function.rsplit('.').next()?;
    match (function, args) {
        ("from_bool" | "value", [decision]) => completed_guard_decision(decision)
            .or_else(|| (function == "from_bool").then(|| decision.clone())),
        ("accept", []) => Some(CoreExpr::Atom("true".to_string())),
        ("reject", []) => Some(CoreExpr::Atom("false".to_string())),
        ("both" | "either", [left, right]) => Some(CoreExpr::BinaryOp {
            operator: if function == "both" { "and" } else { "or" }.to_string(),
            left: Box::new(completed_guard_decision(left)?),
            right: Box::new(completed_guard_decision(right)?),
        }),
        _ => None,
    }
}

/// Finds the completed list payload inside the nominal Effect or its expansion.
pub(crate) fn completed_effect_list_type(output: &CoreType) -> NativeIrResult<CoreType> {
    if let CoreType::Apply { constructor, args } = output {
        if constructor == EFFECT_CONTAINER && args.len() == 1 && list_element(&args[0]).is_some() {
            return Ok(args[0].clone());
        }
    }
    find_list_payload(output).ok_or_else(|| {
        format!(
            "error[native_ir.comprehension_effect]: `{}` has no completed List payload",
            output.contract_text()
        )
        .into()
    })
}

fn find_list_payload(ty: &CoreType) -> Option<CoreType> {
    if list_element(ty).is_some() {
        return Some(ty.clone());
    }
    match ty {
        CoreType::Apply { args, .. } | CoreType::Union(args) => {
            args.iter().find_map(find_list_payload)
        }
        CoreType::Tuple(items) => items.iter().find_map(|item| match item {
            CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => {
                find_list_payload(ty)
            }
        }),
        CoreType::Arrow {
            params,
            return_type,
        } => params
            .iter()
            .find_map(find_list_payload)
            .or_else(|| find_list_payload(return_type)),
        CoreType::Struct { fields, .. } => {
            fields.iter().find_map(|field| find_list_payload(&field.ty))
        }
        CoreType::Map(fields) => fields
            .iter()
            .find_map(|field| find_list_payload(&field.value)),
        _ => None,
    }
}

fn list_element(ty: &CoreType) -> Option<&CoreType> {
    match ty {
        CoreType::List(element) => Some(element),
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            Some(&args[0])
        }
        _ => None,
    }
}
