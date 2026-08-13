use super::expression_formatting::format_expr;
use super::{Expr, Pattern};

/// Formats a pure forwarding lambda as its direct function reference.
///
/// Only bare local/selected names and explicit module members are admitted.
/// Receiver expressions, named arguments, explicit type arguments, guards,
/// reordered arguments, and transformed arguments retain their lambda form.
pub(super) fn format_forwarding_lambda(expr: &Expr) -> Option<String> {
    let Expr::Fun { clauses } = expr else {
        return None;
    };
    let [clause] = clauses.as_slice() else {
        return None;
    };
    if clause.guard.is_some() {
        return None;
    }
    let Expr::Call {
        callee,
        type_args,
        args,
        arg_names,
        remote,
        ..
    } = &clause.body
    else {
        return None;
    };
    if !type_args.is_empty()
        || arg_names.iter().any(Option::is_some)
        || clause.patterns.len() != args.len()
        || !forwards_parameters(&clause.patterns, args)
        || !matches!(callee.as_ref(), Expr::Var(_))
    {
        return None;
    }

    let target = format_expr(callee, 0);
    Some(match remote {
        Some(module) => format!("{module}.{target}"),
        None => target,
    })
}

fn forwards_parameters(patterns: &[Pattern], arguments: &[Expr]) -> bool {
    patterns
        .iter()
        .zip(arguments)
        .all(|(pattern, argument)| match (pattern, argument) {
            (Pattern::Var(parameter), Expr::Var(argument)) => parameter == argument,
            _ => false,
        })
}
