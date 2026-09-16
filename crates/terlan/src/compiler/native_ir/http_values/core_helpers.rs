//! Canonical CoreIR values used by managed HTTP normalization.

use super::{CoreExpr, MANAGED_HTTP_MODULE};

/// Flattens checked string concatenations into one managed operation.
pub(super) fn managed_string_concat(left: CoreExpr, right: CoreExpr) -> CoreExpr {
    fn append_args(expr: CoreExpr, output: &mut Vec<CoreExpr>) {
        match expr {
            CoreExpr::RemoteCall {
                module,
                function,
                args,
                ..
            } if module == MANAGED_HTTP_MODULE
                && matches!(function.as_str(), "string_append" | "string_concat") =>
            {
                output.extend(args);
            }
            expr => output.push(expr),
        }
    }
    let mut args = Vec::new();
    append_args(left, &mut args);
    append_args(right, &mut args);
    let function = if args.len() == 2 {
        "string_append"
    } else {
        "string_concat"
    };
    managed_http_call(function, args)
}

/// Wraps one serialized cookie value in a repeated response-header update.
pub(super) fn response_cookie_header(receiver: CoreExpr, value: CoreExpr) -> CoreExpr {
    managed_http_call(
        "response_header",
        vec![receiver, string_expr("Set-Cookie"), value],
    )
}

/// Builds one compiler-private managed HTTP call.
pub(super) fn managed_http_call(function: &str, args: Vec<CoreExpr>) -> CoreExpr {
    CoreExpr::RemoteCall {
        type_args: Vec::new(),
        module: MANAGED_HTTP_MODULE.to_string(),
        function: function.to_string(),
        args,
    }
}

/// Decodes one checked CoreIR string payload into its runtime UTF-8 value.
pub(super) fn core_string_runtime_value(value: &str) -> Result<String, String> {
    if value.starts_with('"') && value.ends_with('"') {
        serde_json::from_str(value)
            .map_err(|error| format!("error[native_ir.http_string_literal]: {error}"))
    } else {
        Ok(value.to_string())
    }
}

/// Builds one canonical CoreIR string literal.
pub(super) fn string_expr(value: &str) -> CoreExpr {
    CoreExpr::Binary(format!("\"{value}\""))
}

/// Builds one canonical CoreIR Boolean literal.
pub(super) fn bool_expr(value: bool) -> CoreExpr {
    CoreExpr::Atom(value.to_string())
}

/// Builds the private zero-argument persistent response-header list operation.
pub(super) fn empty_response_headers() -> CoreExpr {
    CoreExpr::RemoteCall {
        type_args: Vec::new(),
        module: MANAGED_HTTP_MODULE.to_string(),
        function: "empty_headers".to_string(),
        args: Vec::new(),
    }
}
