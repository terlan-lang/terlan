//! Managed construction and projection for portable HTTP errors.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_field_operation, encode_aggregate_scalar_field_operation,
};
use crate::terlan_typeck::{CoreExpr, CoreType};

use super::layout::semantic;
use super::{managed_http_call, NativeType, ERROR_MODULE};

/// Compiler-private identity of the managed `HttpError` constructor.
pub(super) const ERROR_CONSTRUCTOR: &str = "$terlan.http.error";

/// Reports whether one receiver-shaped call is a portable HTTP error projection.
pub(super) fn error_method_arity(method: &str, arity: usize) -> bool {
    arity == 1 && matches!(method, "code" | "message" | "status")
}

/// Rewrites one public HTTP error function into its managed representation.
pub(super) fn error_call(function: &str, args: Vec<CoreExpr>) -> Result<CoreExpr, String> {
    match (function, args.len()) {
        ("new", 3) => Ok(CoreExpr::ConstructorCall {
            constructor: ERROR_CONSTRUCTOR.to_string(),
            constructor_identity: Some(ERROR_CONSTRUCTOR.to_string()),
            args,
        }),
        ("code", 1) => Ok(managed_http_call("error_code", args)),
        ("message", 1) => Ok(managed_http_call("error_message", args)),
        ("status", 1) => Ok(managed_http_call("error_status", args)),
        ("new" | "code" | "message" | "status", count) => Err(format!(
            "error[native_ir.http_error_arity]: HttpError.{function} received {count} arguments"
        )),
        _ => Ok(CoreExpr::RemoteCall {
            module: ERROR_MODULE.to_string(),
            function: function.to_string(),
            args,
        }),
    }
}

/// Lowers one compiler-private HTTP error projection into managed NativeIR.
pub(super) fn lower_error_operation(
    function: &str,
    args: &[CoreExpr],
    lower: &mut impl FnMut(&CoreExpr) -> Result<super::super::NativeExpr, String>,
) -> Result<Option<super::super::NativeExpr>, String> {
    let (field, scalar) = match (function, args.len()) {
        ("error_code", 1) => (0, true),
        ("error_message", 1) => (1, false),
        ("error_status", 1) => (2, true),
        _ => return Ok(None),
    };
    let semantic = semantic(&CoreType::Named("HttpError".to_string()).contract_text())?;
    let encoded = if scalar {
        encode_aggregate_scalar_field_operation(semantic, field)
    } else {
        encode_aggregate_field_operation(semantic, field)
    }
    .map_err(|error| format!("error[native_ir.http_error_project]: {error}"))?;
    Ok(Some(super::super::NativeExpr::ManagedOperation {
        encoded: Arc::from(encoded),
        args: vec![lower(&args[0])?],
    }))
}

/// Returns the native result type of one compiler-private HTTP error projection.
pub(super) fn error_operation_type(function: &str, arity: usize) -> Option<NativeType> {
    match (function, arity) {
        ("error_code", 1) => Some(NativeType::Atom),
        ("error_message", 1) => Some(NativeType::StringRef),
        ("error_status", 1) => Some(NativeType::Int),
        _ => None,
    }
}
