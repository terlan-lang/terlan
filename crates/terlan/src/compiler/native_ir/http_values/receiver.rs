//! Recognition of receiver-shaped managed HTTP operations.

use crate::terlan_typeck::CoreExpr;

use super::MANAGED_HTTP_MODULE;

/// Reports whether a rewritten expression has a statically known managed string value.
pub(super) fn is_managed_string_expr(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::Binary(_) => true,
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == MANAGED_HTTP_MODULE => matches!(
            (function.as_str(), args.len()),
            (
                "method"
                    | "path"
                    | "query_string"
                    | "body_text"
                    | "cookie_set_header"
                    | "cookie_set_options_header"
                    | "cookie_delete_header"
                    | "string_append",
                _
            )
        ),
        _ => false,
    }
}

/// Reports whether one receiver-shaped call belongs to the response update surface.
pub(super) fn response_method_arity(method: &str, arity: usize) -> bool {
    matches!(
        (method, arity),
        (
            "status"
                | "with_status"
                | "set_cookie_header"
                | "with_security_headers"
                | "security_headers"
                | "with_cookies",
            2
        ) | ("header" | "with_header", 3)
            | ("cookie" | "with_cookie", 3..=6)
            | ("cookie_with_options" | "with_cookie_options", 3..=11)
            | ("delete_cookie" | "with_deleted_cookie", 2..=3)
    )
}

/// Reports whether one receiver call is rooted in the managed cookie-jar surface.
pub(super) fn jar_method_arity(method: &str, arity: usize, args: &[CoreExpr]) -> bool {
    args.first().is_some_and(is_jar_expr)
        && matches!((method, arity), ("get" | "delete", 2..=3) | ("set", 3..=6))
}

/// Reports whether one expression is known to produce the managed cookie jar.
pub(super) fn is_jar_expr(expr: &CoreExpr) -> bool {
    matches!(
        expr,
        CoreExpr::RemoteCall { module, function, .. }
            if module == MANAGED_HTTP_MODULE
                && matches!(function.as_str(), "cookies" | "jar_set" | "jar_delete")
    )
}
