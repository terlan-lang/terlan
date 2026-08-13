//! Typed VM process and message intrinsic lowering.

use super::*;

pub(super) fn core_typed_process_intrinsic_expr_from_parts(
    module: &str,
    function: &str,
    type_args: &[crate::terlan_syntax::SyntaxTypeOutput],
    args: Vec<CoreExpr>,
    span: Span,
) -> Option<CoreExpr> {
    if module != "std.vm.Process" {
        return None;
    }
    if type_args.is_empty() {
        return core_process_lifecycle_value(function, args);
    }
    if type_args.len() != 1 {
        return None;
    }
    let value_type = core_type_from_text(&type_args[0].text)?;
    let (id, return_type) = match (function, args.len()) {
        ("entry", 1) => (
            CoreIntrinsicId::VmProcessEntry(value_type.clone()),
            CoreType::Apply {
                constructor: "Entry".to_string(),
                args: vec![value_type],
            },
        ),
        ("current", 0) => (
            CoreIntrinsicId::VmProcessCurrent(value_type.clone()),
            CoreType::Apply {
                constructor: "Process".to_string(),
                args: vec![value_type],
            },
        ),
        ("resource_kind", 1) => return args.into_iter().next(),
        ("send", 2) => (
            CoreIntrinsicId::VmProcessSendMessage(value_type),
            CoreType::Named("Unit".to_string()),
        ),
        ("receive", 0) => (
            CoreIntrinsicId::VmProcessReceiveMessage(value_type.clone()),
            CoreType::Apply {
                constructor: "Message".to_string(),
                args: vec![value_type],
            },
        ),
        ("spawn", 1) => (
            CoreIntrinsicId::VmProcessSpawn(value_type.clone()),
            CoreType::Apply {
                constructor: "Process".to_string(),
                args: vec![value_type],
            },
        ),
        ("link", 1) => (
            CoreIntrinsicId::VmProcessLink(value_type),
            CoreType::Named("Unit".to_string()),
        ),
        ("monitor", 1) => (
            CoreIntrinsicId::VmProcessMonitor(value_type.clone()),
            CoreType::Apply {
                constructor: "Monitor".to_string(),
                args: vec![value_type],
            },
        ),
        ("acquire", 1) => (
            CoreIntrinsicId::VmProcessAcquireResource(value_type.clone()),
            CoreType::Apply {
                constructor: "Resource".to_string(),
                args: vec![value_type],
            },
        ),
        ("cancel", 1) => (
            CoreIntrinsicId::VmProcessCancel(value_type),
            CoreType::Named("Unit".to_string()),
        ),
        _ => return None,
    };
    Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
        id,
        args,
        return_type,
        effects: core_vm_effect_execution_set(),
        span,
    }))
}

fn core_process_lifecycle_value(function: &str, args: Vec<CoreExpr>) -> Option<CoreExpr> {
    match (function, args.len()) {
        ("timer", 1) | ("exit_reason", 1) => args.into_iter().next(),
        ("priority", 0) => Some(CoreExpr::Int(1)),
        ("normal", 0) => Some(CoreExpr::Int(2)),
        ("background", 0) => Some(CoreExpr::Int(3)),
        _ => None,
    }
}

pub(super) fn core_typed_message_expr_from_parts(
    module: &str,
    function: &str,
    type_args: &[crate::terlan_syntax::SyntaxTypeOutput],
    args: Vec<CoreExpr>,
) -> Option<CoreExpr> {
    if module != "std.vm.Message" || type_args.len() != 1 || args.len() != 1 {
        return None;
    }
    core_type_from_text(&type_args[0].text)?;
    matches!(function, "wrap" | "unwrap")
        .then(|| args.into_iter().next())
        .flatten()
}
