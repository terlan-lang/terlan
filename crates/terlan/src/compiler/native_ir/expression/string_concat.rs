//! String concatenation optimizations shared by every source module.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_string_append_operation, encode_string_concat_operation,
    encode_string_prepend_literal_operation,
};
use crate::terlan_typeck::CoreExpr;

use super::{core_string_runtime_value, NativeExpr};

pub(super) fn lower(
    left: &CoreExpr,
    right: &CoreExpr,
    mut lower: impl FnMut(&CoreExpr) -> Result<NativeExpr, String>,
) -> Result<NativeExpr, String> {
    if let CoreExpr::Binary(literal) = left {
        let literal = core_string_runtime_value(literal)?;
        return Ok(NativeExpr::ManagedOperation {
            encoded: Arc::from(
                encode_string_prepend_literal_operation(&literal)
                    .map_err(|error| format!("error[native_ir.string_prepend]: {error}"))?,
            ),
            args: vec![lower(right)?],
        });
    }
    let append = encode_string_append_operation();
    let concat = encode_string_concat_operation();
    let mut args = Vec::new();
    // Only flatten concatenation nodes: retain lexical scopes and evaluation order.
    for expression in [lower(left)?, lower(right)?] {
        match expression {
            NativeExpr::ManagedOperation {
                encoded,
                args: nested,
            } if encoded.as_ref() == append || encoded.as_ref() == concat => {
                args.extend(nested);
            }
            expression => args.push(expression),
        }
    }
    Ok(NativeExpr::ManagedOperation {
        encoded: Arc::from(if args.len() == 2 { append } else { concat }),
        args,
    })
}

#[cfg(test)]
#[path = "string_concat/string_concat_test.rs"]
mod tests;
