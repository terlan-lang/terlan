//! Canonical CoreIR values used by managed HTTP normalization.

use super::{CoreExpr, MANAGED_HTTP_MODULE};

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
        module: MANAGED_HTTP_MODULE.to_string(),
        function: "empty_headers".to_string(),
        args: Vec::new(),
    }
}
