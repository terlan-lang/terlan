//! Private managed storage for the admitted completed Task operations.

use crate::terlan_typeck::{
    visit_core_expr_mut, CoreCaseClause, CoreExpr, CoreIntrinsicId, CoreModule, CorePattern,
    CorePrimitiveIntrinsic, CoreTupleTypeElem, CoreType,
};

const TASK: &str = "std.core.Task.Task";
const COMPLETED: &str = "$aot.completed_task";

/// Supplies a private layout only for the canonical opaque Task declaration.
pub(super) fn declaration_storage(canonical: &str, parameters: &[String]) -> Option<CoreType> {
    let [parameter] = parameters else { return None };
    (canonical == TASK).then(|| storage(CoreType::Named(parameter.clone())))
}

/// Completed tasks own their payload through ordinary managed aggregate roots.
pub(super) fn storage(value: CoreType) -> CoreType {
    CoreType::Tuple(vec![
        CoreTupleTypeElem::Type(CoreType::AtomLiteral(COMPLETED.to_string())),
        CoreTupleTypeElem::Field {
            name: "completed".to_string(),
            ty: value,
        },
    ])
}

/// Returns the checked payload, without treating user-defined Task types as std.
pub(super) fn element(ty: &CoreType) -> Option<&CoreType> {
    match ty {
        CoreType::Apply { constructor, args } if constructor == TASK && args.len() == 1 => {
            args.first()
        }
        CoreType::Tuple(fields) => match fields.as_slice() {
            [CoreTupleTypeElem::Type(CoreType::AtomLiteral(tag)), CoreTupleTypeElem::Field { name, ty }]
                if tag == COMPLETED && name == "completed" =>
            {
                Some(ty)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Observing a completed task returns the public, recoverable Result contract.
pub(super) fn result(value: CoreType) -> CoreType {
    CoreType::Apply {
        constructor: "std.core.Result.Result".to_string(),
        args: vec![value, CoreType::Named("std.core.Error.Error".to_string())],
    }
}

/// Lowers completed task values, not async scheduling, through existing storage.
pub(super) fn lower(cores: &mut [CoreModule]) -> super::NativeIrResult<()> {
    let mut error = None;
    for core in cores {
        for function in &mut core.functions {
            for clause in &mut function.clauses {
                for expression in clause
                    .guard
                    .iter_mut()
                    .filter_map(|guard| guard.core_expr.as_mut())
                    .chain(clause.body.core_expr.iter_mut())
                {
                    visit_core_expr_mut(expression, &mut |expression| {
                        let CoreExpr::Intrinsic(call) = expression else {
                            return;
                        };
                        let tag = match call.id {
                            CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TaskDone) => {
                                COMPLETED
                            }
                            CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TaskResult) => "ok",
                            _ => return,
                        };
                        let [operand] = call.args.as_slice() else {
                            error = Some("error[native_ir.task_arity]: completed Task operation requires one operand");
                            return;
                        };
                        let value = if tag == COMPLETED {
                            operand.clone()
                        } else {
                            CoreExpr::Var("$task_completed_value".into())
                        };
                        let output = CoreExpr::Cast {
                            expr: Box::new(CoreExpr::Tuple(vec![
                                CoreExpr::Atom(tag.to_string()),
                                value,
                            ])),
                            target_type: call.return_type.clone(),
                        };
                        *expression = if tag == COMPLETED {
                            output
                        } else {
                            CoreExpr::Case {
                                scrutinee: Box::new(operand.clone()),
                                clauses: vec![CoreCaseClause {
                                    pattern: CorePattern::Tuple(vec![
                                        CorePattern::Atom(COMPLETED.into()),
                                        CorePattern::Var("$task_completed_value".into()),
                                    ]),
                                    guard: None,
                                    body: output,
                                }],
                            }
                        };
                    });
                }
            }
        }
    }
    error.map_or(Ok(()), |error| Err(error.into()))
}

#[cfg(test)]
#[path = "task_values_test.rs"]
mod tests;
