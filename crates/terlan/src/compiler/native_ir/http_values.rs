//! Remaining session primitive normalization for direct AOT handlers.

use std::sync::Arc;

use crate::terlan_typeck::{CoreCaseClause, CoreExpr, CoreModule};

use super::NativeType;

mod core_helpers;
mod layout;
mod session;
mod traversal;

use layout::{encoded_descriptor, lookup_descriptor, option_string_layouts, semantic};
use traversal::rewrite_children;

const STRING_OPTION: &str = "Apply(Option;String)";
const LOOKUP_RESULT: &str = "Tuple(String,Bool)";
const MANAGED_HTTP_MODULE: &str = "$terlan.managed.http";

/// Reports whether a remote-call owner names the compiler-private HTTP operation family.
pub(super) fn is_managed_http_module(module: &str) -> bool {
    module == MANAGED_HTTP_MODULE
}

/// Normalizes only explicitly declared session primitives.
pub(super) fn lower_http_values(core: &mut CoreModule) -> Result<(), String> {
    if !session::uses_native_declarations(core) {
        return Ok(());
    }
    for function in &mut core.functions {
        for clause in &mut function.clauses {
            if let Some(body) = &mut clause.body.core_expr {
                *body = rewrite(body)?;
            }
        }
    }
    Ok(())
}

/// Returns target-owned aggregate layouts required at the HTTP boundary.
pub(super) fn http_managed_layouts(core: &CoreModule) -> Result<Vec<Arc<[u8]>>, String> {
    let mut layouts = Vec::new();
    let session = session::uses_native_declarations(core);
    if session {
        layouts.extend(option_string_layouts()?);
        layouts.push(encoded_descriptor(&lookup_descriptor()?)?);
    }
    Ok(layouts)
}

/// Rewrites one expression after recursively normalizing its children.
fn rewrite(expr: &CoreExpr) -> Result<CoreExpr, String> {
    let mut rewritten = expr.clone();
    rewrite_children(&mut rewritten)?;
    Ok(session::rewrite_session_call(&rewritten)?.unwrap_or(rewritten))
}

/// Returns the exact result type of one compiler-private session operation.
pub(super) fn managed_http_operation_type(expr: &CoreExpr) -> Option<NativeType> {
    let CoreExpr::RemoteCall {
        module,
        function,
        args,
        ..
    } = expr
    else {
        return None;
    };
    if module != MANAGED_HTTP_MODULE {
        return None;
    }
    session::operation_type(function, args.len())
}

/// Lowers one compiler-private session operation into managed NativeIR.
pub(super) fn lower_managed_http_operation(
    expr: &CoreExpr,
    lower: impl FnMut(&CoreExpr) -> Result<super::NativeExpr, String>,
) -> Result<Option<super::NativeExpr>, String> {
    let CoreExpr::RemoteCall {
        module,
        function,
        args,
        ..
    } = expr
    else {
        return Ok(None);
    };
    if module != MANAGED_HTTP_MODULE {
        return Ok(None);
    }
    session::lower_operation(function, args, lower)
}
