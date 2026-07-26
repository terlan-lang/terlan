// Remote-call normalization for closed native application images.

fn normalize_remote_calls(core: &mut CoreModule, preserve_receivers: bool) {
    for function in &mut core.functions {
        for clause in &mut function.clauses {
            if let Some(body) = &mut clause.body.core_expr {
                normalize_remote_expr(body, preserve_receivers);
            }
        }
    }
}

fn normalize_remote_expr(expr: &mut CoreExpr, preserve_receivers: bool) {
    match expr {
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => {
            for arg in args.iter_mut() {
                normalize_remote_expr(arg, preserve_receivers);
            }
            if module == "std.test.Test" {
                if let Some(lowered) = test_equality_expr(function, args) {
                    *expr = lowered;
                    return;
                }
            }
            if module == "__receiver__"
                || (preserve_receivers
                    && matches!(
                        module.as_str(),
                        "std.collections.List" | "std.collections.Map"
                    ))
            {
                return;
            }
            if super::http_values::is_managed_http_module(module)
                || super::template_values::is_managed_template_module(module)
                || super::list_comprehension::is_managed_comprehension_module(module)
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
                normalize_remote_expr(arg, preserve_receivers);
            }
            if let Some(function) = function.strip_prefix("std.test.Test.") {
                if let Some(lowered) = test_equality_expr(function, args) {
                    *expr = lowered;
                }
            }
        }
        CoreExpr::ConstructorCall { args, .. } => {
            for arg in args {
                normalize_remote_expr(arg, preserve_receivers);
            }
        }
        CoreExpr::Intrinsic(call) => {
            for arg in &mut call.args {
                normalize_remote_expr(arg, preserve_receivers);
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                normalize_remote_expr(item, preserve_receivers);
            }
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            normalize_remote_expr(head, preserve_receivers);
            normalize_remote_expr(tail, preserve_receivers);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            normalize_remote_expr(expr, preserve_receivers);
            for generator in generators {
                normalize_remote_expr(&mut generator.source, preserve_receivers);
            }
            for guard in guards {
                normalize_remote_expr(guard, preserve_receivers);
            }
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                normalize_remote_expr(&mut field.value, preserve_receivers);
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                normalize_remote_expr(&mut field.value, preserve_receivers);
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            normalize_remote_expr(base, preserve_receivers);
            for field in fields {
                normalize_remote_expr(&mut field.value, preserve_receivers);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. } => normalize_remote_expr(base, preserve_receivers),
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                normalize_remote_expr(arg, preserve_receivers);
            }
            normalize_remote_expr(record, preserve_receivers);
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            normalize_remote_expr(receiver, preserve_receivers);
            for arg in args {
                normalize_remote_expr(arg, preserve_receivers);
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            normalize_remote_expr(callee, preserve_receivers);
            for arg in args {
                normalize_remote_expr(arg, preserve_receivers);
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            for parameter in parameters {
                normalize_remote_expr(parameter, preserve_receivers);
            }
        }
        CoreExpr::UnaryOp { operand, .. } => normalize_remote_expr(operand, preserve_receivers),
        CoreExpr::BinaryOp { left, right, .. } => {
            normalize_remote_expr(left, preserve_receivers);
            normalize_remote_expr(right, preserve_receivers);
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                normalize_remote_expr(&mut binding.value, preserve_receivers);
            }
            normalize_remote_expr(body, preserve_receivers);
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                normalize_remote_expr(&mut clause.condition, preserve_receivers);
                normalize_remote_expr(&mut clause.body, preserve_receivers);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            normalize_remote_expr(scrutinee, preserve_receivers);
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    normalize_remote_expr(guard, preserve_receivers);
                }
                normalize_remote_expr(&mut clause.body, preserve_receivers);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            normalize_remote_expr(body, preserve_receivers);
            for clause in of_clauses.iter_mut().chain(catch_clauses.iter_mut()) {
                if let Some(guard) = &mut clause.guard {
                    normalize_remote_expr(guard, preserve_receivers);
                }
                normalize_remote_expr(&mut clause.body, preserve_receivers);
            }
            if let Some(after) = after_clause {
                normalize_remote_expr(&mut after.trigger, preserve_receivers);
                normalize_remote_expr(&mut after.body, preserve_receivers);
            }
        }
        CoreExpr::Lam { body, .. } => normalize_remote_expr(body, preserve_receivers),
        _ => {}
    }
}

fn test_equality_expr(function: &str, args: &mut Vec<CoreExpr>) -> Option<CoreExpr> {
    if args.len() != 2 || !matches!(function, "assert_equal" | "assert_not_equal") {
        return None;
    }
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
