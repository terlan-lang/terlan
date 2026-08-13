//! Direct-AOT normalization for the portable HTTP session surface.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_field_operation, encode_session_current_operation,
    encode_session_expire_operation, encode_session_get_operation,
    encode_session_mutation_operation, encode_session_option_is_none_operation,
    encode_session_rotate_operation, encode_session_with_response_operation,
    ManagedSessionMutation,
};
use crate::terlan_typeck::{CoreCaseClause, CoreEffectSet, CoreExpr, CoreType};

use super::option_string::{lower_managed_option_case, ManagedOptionCase};
use super::{
    semantic, NativeType, MANAGED_HTTP_MODULE, REQUEST_STRING_MAP, RESPONSE_HEADER,
    RESPONSE_HEADERS, STRING_OPTION,
};

/// Public standard-library module that owns portable session calls.
pub(super) const SESSION_MODULE: &str = "std.http.Session";
const SESSION_SEMANTIC: &str = "Named(Session)";
const SESSION_OPTION_TEMPORARY: &str = "$native_http_session_option";

/// Rewrites one checked session call into the compiler-private managed family.
pub(super) fn rewrite_session_call(expr: &CoreExpr) -> Result<Option<CoreExpr>, String> {
    let rewritten = match expr {
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == SESSION_MODULE => session_call(function, args.clone())?,
        CoreExpr::Call { function, args }
            if matches!(
                function.as_str(),
                "current" | "get" | "set" | "delete" | "rotate" | "expire" | "with_response"
            ) || function.starts_with(&format!("{SESSION_MODULE}.")) =>
        {
            let name = function.rsplit('.').next().unwrap_or(function);
            session_call(name, args.clone())?
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == "__receiver__"
            && matches!(
                function.as_str(),
                "get" | "set" | "delete" | "rotate" | "expire"
            ) =>
        {
            receiver_call(function, args.clone())?
        }
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            effects,
        } if matches!(method.as_str(), "set" | "delete" | "rotate" | "expire") => {
            mutation_call((**receiver).clone(), method, args.clone(), effects.clone())?
        }
        _ => return Ok(None),
    };
    Ok(Some(rewritten))
}

/// Returns the exact result type of one compiler-private session operation.
pub(super) fn operation_type(function: &str, arity: usize) -> Option<NativeType> {
    match (function, arity) {
        ("session_current", 1) | ("session_rotate", 1) => {
            semantic(SESSION_SEMANTIC).ok().map(NativeType::ManagedRef)
        }
        ("session_get", 2) => semantic(STRING_OPTION).ok().map(NativeType::ManagedRef),
        ("session_get_is_none", 1) => Some(NativeType::Bool),
        ("session_get_some", 1) => Some(NativeType::StringRef),
        ("session_set", 3) | ("session_delete", 2) | ("session_expire", 1) => {
            Some(NativeType::Unit)
        }
        ("session_with_response", 2) => {
            semantic(&CoreType::Named("Response".to_string()).contract_text())
                .ok()
                .map(NativeType::ManagedRef)
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
    let session = semantic(SESSION_SEMANTIC)?;
    let encoded = match (function, args.len()) {
        ("session_get_is_none", 1) => {
            encode_session_option_is_none_operation(semantic(STRING_OPTION)?)
        }
        ("session_get_some", 1) => encode_aggregate_field_operation(semantic(STRING_OPTION)?, 0)
            .map_err(|error| format!("error[native_ir.http_session_option]: {error}"))?,
        ("session_current", 1) => encode_session_current_operation(
            semantic(&CoreType::Named("Request".to_string()).contract_text())?,
            semantic(REQUEST_STRING_MAP)?,
            session,
            8,
        )
        .map_err(|error| format!("error[native_ir.http_session_current]: {error}"))?,
        ("session_get", 2) => encode_session_get_operation(session, semantic(STRING_OPTION)?),
        ("session_set", 3) => {
            encode_session_mutation_operation(ManagedSessionMutation::Set, session)
        }
        ("session_delete", 2) => {
            encode_session_mutation_operation(ManagedSessionMutation::Delete, session)
        }
        ("session_rotate", 1) => encode_session_rotate_operation(session),
        ("session_expire", 1) => encode_session_expire_operation(session),
        ("session_with_response", 2) => encode_session_with_response_operation(
            session,
            semantic(&CoreType::Named("Response".to_string()).contract_text())?,
            semantic(RESPONSE_HEADERS)?,
            semantic(RESPONSE_HEADER)?,
            5,
        )
        .map_err(|error| format!("error[native_ir.http_session_response]: {error}"))?,
        _ => return Ok(None),
    };
    Ok(Some(super::super::NativeExpr::ManagedOperation {
        encoded: Arc::from(encoded),
        args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
    }))
}

/// Eliminates one immediate `None`/`Some` match over a managed session read.
pub(super) fn lower_session_case(
    scrutinee: &CoreExpr,
    clauses: &[CoreCaseClause],
) -> Result<Option<CoreExpr>, String> {
    let scrutinee = match scrutinee {
        CoreExpr::Cast { expr, .. }
            if matches!(
                &**expr,
                CoreExpr::RemoteCall { module, function, args }
                    if module == MANAGED_HTTP_MODULE && function == "session_get" && args.len() == 2
            ) =>
        {
            expr.as_ref()
        }
        _ => scrutinee,
    };
    if !matches!(
        scrutinee,
        CoreExpr::RemoteCall { module, function, args }
            if module == MANAGED_HTTP_MODULE && function == "session_get" && args.len() == 2
    ) {
        return Ok(None);
    }
    lower_managed_option_case(
        scrutinee,
        clauses,
        ManagedOptionCase {
            temporary: SESSION_OPTION_TEMPORARY,
            none_operation: "session_get_is_none",
            some_operation: "session_get_some",
            diagnostic: "native_ir.http_session_case",
        },
    )
    .map(Some)
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
        ("with_response", 2) => "session_with_response",
        _ => return session_arity_error(function, args.len()),
    };
    Ok(managed_call(name, args))
}

/// Separates the receiver carried by one resolved receiver call.
fn receiver_call(function: &str, mut args: Vec<CoreExpr>) -> Result<CoreExpr, String> {
    if args.is_empty() {
        return session_arity_error(function, 0);
    }
    let receiver = args.remove(0);
    mutation_call(
        receiver,
        function,
        args,
        CoreEffectSet {
            effects: Vec::new(),
        },
    )
}

/// Maps one session receiver method onto its managed operation identity.
fn mutation_call(
    receiver: CoreExpr,
    method: &str,
    args: Vec<CoreExpr>,
    effects: CoreEffectSet,
) -> Result<CoreExpr, String> {
    let name = match (method, args.len()) {
        ("get", 1) => "session_get",
        ("set", 2) => "session_set",
        ("delete", 1) => "session_delete",
        ("rotate", 0) => "session_rotate",
        ("expire", 0) => "session_expire",
        _ => {
            return Ok(CoreExpr::MutableReceiverCall {
                receiver: Box::new(receiver),
                method: method.to_string(),
                args,
                effects,
            })
        }
    };
    Ok(managed_call(name, [vec![receiver], args].concat()))
}

/// Builds one compiler-private managed session call.
fn managed_call(function: &str, args: Vec<CoreExpr>) -> CoreExpr {
    CoreExpr::RemoteCall {
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
