//! Direct-AOT normalization for the portable HTTP session surface.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_session_current_operation, encode_session_expire_operation,
    encode_session_get_operation, encode_session_is_live_operation,
    encode_session_mutation_operation, encode_session_rotate_operation, ManagedSessionMutation,
};
use crate::terlan_typeck::{CoreExpr, CoreIntrinsicId, CoreModule};

use super::{semantic, NativeType, LOOKUP_RESULT, MANAGED_HTTP_MODULE, STRING_OPTION};

/// Reports whether a provider actually declares a supported session primitive.
pub(super) fn uses_native_declarations(core: &CoreModule) -> bool {
    super::core_helpers::uses_expression(core, |expr| {
        native_operation(expr).is_some()
            || matches!(expr, CoreExpr::RemoteCall { module, function, args, .. }
                        if module == MANAGED_HTTP_MODULE && operation_type(function, args.len()).is_some())
    })
}

fn native_operation(expr: &CoreExpr) -> Option<(&str, &[CoreExpr])> {
    let CoreExpr::Intrinsic(call) = expr else {
        return None;
    };
    let CoreIntrinsicId::NativeOperation { operation, .. } = &call.id else {
        return None;
    };
    let name = operation.strip_prefix("std.http.session.")?;
    matches!(
        name,
        "current" | "get" | "set" | "delete" | "rotate" | "expire" | "is_live"
    )
    .then_some((name, call.args.as_slice()))
}

/// Lowers explicit primitive declarations, never source call names or receivers.
pub(super) fn rewrite_session_call(expr: &CoreExpr) -> Result<Option<CoreExpr>, String> {
    let Some((name, args)) = native_operation(expr) else {
        return Ok(None);
    };
    session_call(name, args.to_vec()).map(Some)
}

/// Returns the exact result type of one compiler-private session operation.
pub(super) fn operation_type(function: &str, arity: usize) -> Option<NativeType> {
    match (function, arity) {
        ("session_current", 1) => semantic(LOOKUP_RESULT).ok().map(NativeType::ManagedRef),
        ("session_rotate", 1) => Some(NativeType::StringRef),
        ("session_is_live", 1) => Some(NativeType::Bool),
        ("session_get", 2) => semantic(STRING_OPTION).ok().map(NativeType::ManagedRef),
        ("session_set", 3) | ("session_delete", 2) | ("session_expire", 1) => {
            Some(NativeType::Unit)
        }
        _ => None,
    }
}

/// Lowers one compiler-private session operation into managed NativeIR.
pub(super) fn lower_operation(
    function: &str,
    args: &[CoreExpr],
    mut lower: impl FnMut(&CoreExpr) -> Result<super::super::NativeExpr, String>,
) -> Result<Option<super::super::NativeExpr>, String> {
    let encoded = match (function, args.len()) {
        ("session_current", 1) => {
            encode_session_current_operation(semantic(STRING_OPTION)?, semantic(LOOKUP_RESULT)?)
        }
        ("session_get", 2) => encode_session_get_operation(semantic(STRING_OPTION)?),
        ("session_set", 3) => encode_session_mutation_operation(ManagedSessionMutation::Set),
        ("session_delete", 2) => encode_session_mutation_operation(ManagedSessionMutation::Delete),
        ("session_rotate", 1) => encode_session_rotate_operation(),
        ("session_expire", 1) => encode_session_expire_operation(),
        ("session_is_live", 1) => encode_session_is_live_operation(),
        _ => return Ok(None),
    };
    Ok(Some(super::super::NativeExpr::ManagedOperation {
        encoded: Arc::from(encoded),
        args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
    }))
}

/// Normalizes one module-shaped session call and validates its arity.
fn session_call(function: &str, args: Vec<CoreExpr>) -> Result<CoreExpr, String> {
    let name = match (function, args.len()) {
        ("current", 1) => "session_current",
        ("get", 2) => "session_get",
        ("set", 3) => "session_set",
        ("delete", 2) => "session_delete",
        ("rotate", 1) => "session_rotate",
        ("expire", 1) => "session_expire",
        ("is_live", 1) => "session_is_live",
        _ => return session_arity_error(function, args.len()),
    };
    Ok(managed_call(name, args))
}

/// Builds one compiler-private managed session call.
fn managed_call(function: &str, args: Vec<CoreExpr>) -> CoreExpr {
    CoreExpr::RemoteCall {
        type_args: Vec::new(),
        module: MANAGED_HTTP_MODULE.to_string(),
        function: function.to_string(),
        args,
    }
}

/// Returns the stable diagnostic for an unsupported session call shape.
fn session_arity_error(function: &str, arity: usize) -> Result<CoreExpr, String> {
    Err(format!(
        "error[native_ir.http_session_arity]: Session.{function} received {arity} arguments"
    ))
}
