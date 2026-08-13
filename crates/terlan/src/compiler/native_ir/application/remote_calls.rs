// Remote-call normalization for closed native application images.

use super::*;

pub(super) fn normalize_remote_calls(
    core: &mut CoreModule,
    preserve_receivers: bool,
    application_functions: &HashMap<(String, usize), Option<String>>,
) {
    let local_functions = core
        .functions
        .iter()
        .map(|function| (function.name.clone(), function.arity))
        .collect::<HashSet<_>>();
    for function in &mut core.functions {
        for clause in &mut function.clauses {
            if let Some(body) = &mut clause.body.core_expr {
                normalize_remote_expr(
                    body,
                    preserve_receivers,
                    &local_functions,
                    application_functions,
                );
            }
        }
    }
}

fn normalize_remote_expr(
    expr: &mut CoreExpr,
    preserve_receivers: bool,
    local_functions: &HashSet<(String, usize)>,
    application_functions: &HashMap<(String, usize), Option<String>>,
) {
    match expr {
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => {
            for arg in args.iter_mut() {
                normalize_remote_expr(
                    arg,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
            if module == "std.test.Test" {
                if let Some(lowered) = test_assertion_expr(function, args) {
                    *expr = lowered;
                    return;
                }
            }
            if module == "__receiver__" {
                if !preserve_receivers {
                    let identity = (function.clone(), args.len());
                    let target = if local_functions.contains(&identity) {
                        Some(function.clone())
                    } else {
                        application_functions.get(&identity).cloned().flatten()
                    };
                    if let Some(target) = target {
                        *expr = CoreExpr::Call {
                            function: target,
                            args: std::mem::take(args),
                        };
                    }
                }
                return;
            }
            if preserve_receivers
                && matches!(
                    module.as_str(),
                    "std.collections.List" | "std.collections.Map"
                )
            {
                return;
            }
            if crate::compiler::native_ir::http_values::is_managed_http_module(module)
                || crate::compiler::native_ir::template_values::is_managed_template_module(module)
            {
                return;
            }
            *expr = CoreExpr::Call {
                function: format!("{module}.{function}"),
                args: std::mem::take(args),
            };
        }
        CoreExpr::Call { function, args } => {
            for arg in args.iter_mut() {
                normalize_remote_expr(
                    arg,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
            if let Some(function) = function.strip_prefix("std.test.Test.") {
                if let Some(lowered) = test_assertion_expr(function, args) {
                    *expr = lowered;
                }
            }
        }
        CoreExpr::ConstructorCall { args, .. } => {
            for arg in args.iter_mut() {
                normalize_remote_expr(
                    arg,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
        }
        CoreExpr::Intrinsic(call) => {
            for arg in &mut call.args {
                normalize_remote_expr(
                    arg,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                normalize_remote_expr(
                    item,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            normalize_remote_expr(
                head,
                preserve_receivers,
                local_functions,
                application_functions,
            );
            normalize_remote_expr(
                tail,
                preserve_receivers,
                local_functions,
                application_functions,
            );
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            normalize_remote_expr(
                expr,
                preserve_receivers,
                local_functions,
                application_functions,
            );
            for generator in generators {
                normalize_remote_expr(
                    &mut generator.source,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
            for guard in guards {
                normalize_remote_expr(
                    guard,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                normalize_remote_expr(
                    &mut field.value,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                normalize_remote_expr(
                    &mut field.value,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            normalize_remote_expr(
                base,
                preserve_receivers,
                local_functions,
                application_functions,
            );
            for field in fields {
                normalize_remote_expr(
                    &mut field.value,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. } => normalize_remote_expr(
            base,
            preserve_receivers,
            local_functions,
            application_functions,
        ),
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args.iter_mut() {
                normalize_remote_expr(
                    arg,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
            normalize_remote_expr(
                record,
                preserve_receivers,
                local_functions,
                application_functions,
            );
        }
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            ..
        } => {
            normalize_remote_expr(
                receiver,
                preserve_receivers,
                local_functions,
                application_functions,
            );
            for arg in args.iter_mut() {
                normalize_remote_expr(
                    arg,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
            if !preserve_receivers {
                let identity = (method.clone(), args.len().saturating_add(1));
                let target = if local_functions.contains(&identity) {
                    Some(method.clone())
                } else {
                    application_functions.get(&identity).cloned().flatten()
                };
                if let Some(target) = target {
                    let receiver =
                        std::mem::replace(receiver.as_mut(), CoreExpr::Atom("Unit".to_string()));
                    let mut call_args = vec![receiver];
                    call_args.append(args);
                    *expr = CoreExpr::Call {
                        function: target,
                        args: call_args,
                    };
                }
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            normalize_remote_expr(
                callee,
                preserve_receivers,
                local_functions,
                application_functions,
            );
            for arg in args {
                normalize_remote_expr(
                    arg,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            for parameter in parameters {
                normalize_remote_expr(
                    parameter,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
        }
        CoreExpr::UnaryOp { operand, .. } => normalize_remote_expr(
            operand,
            preserve_receivers,
            local_functions,
            application_functions,
        ),
        CoreExpr::BinaryOp { left, right, .. } => {
            normalize_remote_expr(
                left,
                preserve_receivers,
                local_functions,
                application_functions,
            );
            normalize_remote_expr(
                right,
                preserve_receivers,
                local_functions,
                application_functions,
            );
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                normalize_remote_expr(
                    &mut binding.value,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
            normalize_remote_expr(
                body,
                preserve_receivers,
                local_functions,
                application_functions,
            );
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                normalize_remote_expr(
                    &mut clause.condition,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
                normalize_remote_expr(
                    &mut clause.body,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            normalize_remote_expr(
                scrutinee,
                preserve_receivers,
                local_functions,
                application_functions,
            );
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    normalize_remote_expr(
                        guard,
                        preserve_receivers,
                        local_functions,
                        application_functions,
                    );
                }
                normalize_remote_expr(
                    &mut clause.body,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            normalize_remote_expr(
                body,
                preserve_receivers,
                local_functions,
                application_functions,
            );
            for clause in of_clauses.iter_mut().chain(catch_clauses.iter_mut()) {
                if let Some(guard) = &mut clause.guard {
                    normalize_remote_expr(
                        guard,
                        preserve_receivers,
                        local_functions,
                        application_functions,
                    );
                }
                normalize_remote_expr(
                    &mut clause.body,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
            if let Some(after) = after_clause {
                normalize_remote_expr(
                    &mut after.trigger,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
                normalize_remote_expr(
                    &mut after.body,
                    preserve_receivers,
                    local_functions,
                    application_functions,
                );
            }
        }
        CoreExpr::Lam { body, .. } => normalize_remote_expr(
            body,
            preserve_receivers,
            local_functions,
            application_functions,
        ),
        _ => {}
    }
}

pub(super) fn test_assertion_expr(function: &str, args: &mut Vec<CoreExpr>) -> Option<CoreExpr> {
    match (function, args.len()) {
        ("assert" | "assert_true", 1) => std::mem::take(args).pop(),
        ("assert_false", 1) => Some(CoreExpr::UnaryOp {
            operator: "not".to_string(),
            operand: Box::new(std::mem::take(args).pop()?),
        }),
        ("assert_equal" | "assert_not_equal", 2) => {
            let mut values = std::mem::take(args);
            let right = values.pop()?;
            let left = values.pop()?;
            Some(CoreExpr::BinaryOp {
                operator: if function == "assert_equal" {
                    "==".to_string()
                } else {
                    "!=".to_string()
                },
                left: Box::new(left),
                right: Box::new(right),
            })
        }
        ("fail", 0) => Some(CoreExpr::Atom("false".to_string())),
        _ => None,
    }
}

#[cfg(test)]
#[path = "remote_calls_test.rs"]
mod tests;
