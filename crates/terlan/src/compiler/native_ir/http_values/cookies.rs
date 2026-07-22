//! Argument normalization for maintained HTTP cookie operations.

use crate::terlan_typeck::CoreExpr;

/// Normalizes the practical cookie serializer defaults.
pub(super) fn cookie_set_args(mut args: Vec<CoreExpr>) -> Result<Vec<CoreExpr>, String> {
    if !(2..=5).contains(&args.len()) {
        return Err(format!(
            "error[native_ir.http_cookie_arity]: cookie set received {} arguments",
            args.len()
        ));
    }
    let defaults = [string_expr("/"), bool_expr(false), bool_expr(false)];
    args.extend(defaults.into_iter().skip(args.len().saturating_sub(2)));
    Ok(args)
}

/// Normalizes the full cookie option serializer defaults.
pub(super) fn cookie_option_args(mut args: Vec<CoreExpr>) -> Result<Vec<CoreExpr>, String> {
    if !(2..=10).contains(&args.len()) {
        return Err(format!(
            "error[native_ir.http_cookie_arity]: cookie options received {} arguments",
            args.len()
        ));
    }
    let defaults = vec![
        string_expr("/"),
        string_expr(""),
        CoreExpr::Int(0),
        bool_expr(false),
        string_expr(""),
        bool_expr(false),
        bool_expr(false),
        string_expr(""),
    ];
    args.extend(defaults.into_iter().skip(args.len().saturating_sub(2)));
    Ok(args)
}

/// Normalizes cookie deletion path defaults.
pub(super) fn cookie_delete_args(mut args: Vec<CoreExpr>) -> Result<Vec<CoreExpr>, String> {
    if !(1..=2).contains(&args.len()) {
        return Err(format!(
            "error[native_ir.http_cookie_arity]: cookie deletion received {} arguments",
            args.len()
        ));
    }
    if args.len() == 1 {
        args.push(string_expr("/"));
    }
    Ok(args)
}

/// Builds one canonical CoreIR string literal.
fn string_expr(value: &str) -> CoreExpr {
    CoreExpr::Binary(format!("\"{value}\""))
}

/// Builds one canonical CoreIR Boolean literal.
fn bool_expr(value: bool) -> CoreExpr {
    CoreExpr::Atom(value.to_string())
}
