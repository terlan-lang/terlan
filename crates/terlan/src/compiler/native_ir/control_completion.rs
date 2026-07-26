//! Shared completion targets for suspension-aware control lowering.

use std::collections::HashMap;

use super::NativeExpr;

#[derive(Clone)]
pub(super) struct CompletionTarget {
    pub(super) continuation_id: u64,
    pub(super) captures: Vec<String>,
}

pub(super) fn complete(
    value: NativeExpr,
    target: Option<&CompletionTarget>,
    params: &HashMap<String, usize>,
) -> Result<NativeExpr, String> {
    let Some(target) = target else {
        return Ok(value);
    };
    let mut args = target
        .captures
        .iter()
        .map(|name| {
            params
                .get(name)
                .copied()
                .map(NativeExpr::Param)
                .ok_or_else(|| {
                    format!(
                    "error[native_ir.shared_completion_capture]: scalar `{name}` is unavailable"
                )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result_local = super::control::next_local_index(params);
    args.push(NativeExpr::Param(result_local));
    Ok(NativeExpr::Let {
        bindings: vec![value],
        body: Box::new(NativeExpr::ContinuationTailCall {
            continuation_id: target.continuation_id,
            args,
        }),
    })
}
