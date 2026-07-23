//! Managed response construction and persistent response updates.

use crate::terlan_typeck::{CoreEffectSet, CoreExpr};

use super::cookies::{cookie_delete_args, cookie_option_args, cookie_set_args};
use super::core_helpers::{
    empty_response_headers, managed_http_call, response_cookie_header, string_expr,
};
use super::security::security_headers_constructor;
use super::{COOKIES_MODULE, MANAGED_HTTP_MODULE, RESPONSE_CONSTRUCTOR_PREFIX};

/// Creates one managed response constructor call with an explicit status.
pub(super) fn response_call(name: &str, args: Vec<CoreExpr>) -> Result<CoreExpr, String> {
    match (name, args.len()) {
        ("default_security_headers", 0) => Ok(security_headers_constructor(0, false)),
        ("production_security_headers", 0) => Ok(security_headers_constructor(31_536_000, true)),
        _ => response_builder(name, args),
    }
}

/// Rewrites maintained cookie serializers into bounded managed HTTP operations.
pub(super) fn cookie_call(name: &str, args: Vec<CoreExpr>) -> Result<CoreExpr, String> {
    let (function, args) = match name {
        "set_header" => ("cookie_set_header", cookie_set_args(args)?),
        "set_header_with_options" => ("cookie_set_options_header", cookie_option_args(args)?),
        "delete_header" => ("cookie_delete_header", cookie_delete_args(args)?),
        _ => {
            return Ok(CoreExpr::RemoteCall {
                module: COOKIES_MODULE.to_string(),
                function: name.to_string(),
                args,
            })
        }
    };
    Ok(managed_http_call(function, args))
}

/// Creates one fixed managed response constructor call with an explicit status.
fn response_builder(name: &str, mut args: Vec<CoreExpr>) -> Result<CoreExpr, String> {
    let (kind, default_status) = match name {
        "text" => (0, 200),
        "html" => (1, 200),
        "json_text" => (2, 200),
        "redirect" => (3, 302),
        "file" => return file_response(args),
        _ => {
            return Err(format!(
                "error[native_ir.http_response_builder]: Response.{name} is not in the native managed HTTP profile"
            ))
        }
    };
    let (body, status) = match args.len() {
        1 => (args.remove(0), CoreExpr::Int(default_status)),
        2 => (args.remove(0), args.remove(0)),
        count => return response_arity_error(name, count),
    };
    Ok(CoreExpr::ConstructorCall {
        constructor: response_constructor(name),
        constructor_identity: Some(response_constructor(name)),
        args: vec![
            CoreExpr::Int(0),
            CoreExpr::Int(kind),
            body,
            status,
            CoreExpr::Binary("\"\"".to_string()),
            empty_response_headers(),
        ],
    })
}

/// Creates one managed file-response constructor call with explicit defaults.
fn file_response(mut args: Vec<CoreExpr>) -> Result<CoreExpr, String> {
    if args.is_empty() || args.len() > 3 {
        return response_arity_error("file", args.len());
    }
    let path = args.remove(0);
    let status = if args.is_empty() {
        CoreExpr::Int(200)
    } else {
        args.remove(0)
    };
    let content_type = if args.is_empty() {
        CoreExpr::Binary("\"\"".to_string())
    } else {
        args.remove(0)
    };
    Ok(CoreExpr::ConstructorCall {
        constructor: response_constructor("file"),
        constructor_identity: Some(response_constructor("file")),
        args: vec![
            CoreExpr::Int(0),
            CoreExpr::Int(4),
            path,
            status,
            content_type,
            empty_response_headers(),
        ],
    })
}

/// Returns one stable response-builder arity diagnostic.
fn response_arity_error(name: &str, count: usize) -> Result<CoreExpr, String> {
    Err(format!(
        "error[native_ir.http_response_arity]: Response.{name} received {count} arguments"
    ))
}

/// Builds one compiler-private response constructor identity.
pub(super) fn response_constructor(name: &str) -> String {
    format!("{RESPONSE_CONSTRUCTOR_PREFIX}{name}")
}

/// Rewrites one immutable response update into a compiler-private heap operation.
pub(super) fn response_mutation(
    receiver: CoreExpr,
    method: &str,
    mut args: Vec<CoreExpr>,
    effects: CoreEffectSet,
) -> Result<CoreExpr, String> {
    let (function, mut operation_args) = match method {
        "status" | "with_status" if args.len() == 1 => ("response_status", vec![receiver]),
        "header" | "with_header" if args.len() == 2 => ("response_header", vec![receiver]),
        "set_cookie_header" if args.len() == 1 => {
            ("response_header", vec![receiver, string_expr("Set-Cookie")])
        }
        "cookie" | "with_cookie" => {
            let serialized = managed_http_call("cookie_set_header", cookie_set_args(args)?);
            return Ok(response_cookie_header(receiver, serialized));
        }
        "cookie_with_options" | "with_cookie_options" => {
            let serialized =
                managed_http_call("cookie_set_options_header", cookie_option_args(args)?);
            return Ok(response_cookie_header(receiver, serialized));
        }
        "delete_cookie" | "with_deleted_cookie" => {
            let serialized = managed_http_call("cookie_delete_header", cookie_delete_args(args)?);
            return Ok(response_cookie_header(receiver, serialized));
        }
        "security_headers" | "with_security_headers" if args.len() == 1 => {
            return Ok(managed_http_call(
                "response_security_headers",
                vec![receiver, args.remove(0)],
            ));
        }
        "with_cookies" if args.len() == 1 => {
            return Ok(managed_http_call(
                "response_cookie_jar",
                vec![receiver, args.remove(0)],
            ));
        }
        _ => {
            return Ok(CoreExpr::MutableReceiverCall {
                receiver: Box::new(receiver),
                method: method.to_string(),
                args,
                effects,
            })
        }
    };
    operation_args.append(&mut args);
    Ok(CoreExpr::RemoteCall {
        module: MANAGED_HTTP_MODULE.to_string(),
        function: function.to_string(),
        args: operation_args,
    })
}

/// Rewrites one known cookie-jar mutation into an immutable persistent update.
pub(super) fn jar_mutation(
    receiver: CoreExpr,
    method: &str,
    args: Vec<CoreExpr>,
    effects: CoreEffectSet,
) -> Result<CoreExpr, String> {
    let serialized = match method {
        "set" => managed_http_call("cookie_set_header", cookie_set_args(args)?),
        "delete" => managed_http_call("cookie_delete_header", cookie_delete_args(args)?),
        _ => {
            return Ok(CoreExpr::MutableReceiverCall {
                receiver: Box::new(receiver),
                method: method.to_string(),
                args,
                effects,
            })
        }
    };
    Ok(managed_http_call("jar_append", vec![receiver, serialized]))
}
