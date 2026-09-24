//! Concrete payload inference for the completed Task contract.

use super::super::task_values;
use super::*;

/// Retains payload types across both task creation and result observation.
pub(super) fn specialize_result(call: &mut CoreIntrinsicCall, arguments: &[Option<CoreType>]) {
    let Some(Some(argument)) = arguments.first() else {
        return;
    };
    match call.id {
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TaskDone) => {
            call.return_type = task_values::storage(argument.clone());
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TaskResult) => {
            if let Some(value) = task_values::element(argument) {
                call.return_type = task_values::result(value.clone());
            }
        }
        _ => {}
    }
}

/// Resolves only a typed Task receiver, leaving unrelated result methods intact.
pub(super) fn specialize_receiver(
    expr: &mut CoreExpr,
    variables: &HashMap<String, CoreType>,
    functions: &FunctionTypes,
    module: &str,
) -> Option<CoreType> {
    let args = match expr {
        CoreExpr::RemoteCall {
            module: owner,
            function,
            args,
            ..
        } if matches!(owner.as_str(), "__receiver__" | "std.core.Task") && function == "result" => {
            args
        }
        CoreExpr::Call { function, args, .. } if function == "std.core.Task.result" => args,
        _ => return None,
    };
    if args.len() != 1 {
        return None;
    }
    let receiver = specialize_expr(&mut args[0], variables, functions, module)?;
    let result = task_values::result(task_values::element(&receiver)?.clone());
    *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TaskResult),
        args: std::mem::take(args),
        return_type: result.clone(),
        effects: CoreEffectSet {
            effects: Vec::new(),
        },
        span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
    });
    Some(result)
}
