//! Builds checked plan dispatch and ordinary native callback invocations.

use crate::terlan_typeck::{
    CoreCaseClause, CoreEffectSet, CoreIfClause, CoreIntrinsicCall, CoreLetBinding, CoreParam,
    CorePattern, CoreProofCoverage,
};

use super::*;

pub(super) fn build(
    owner: &CoreFunction,
    schema: &Schema,
    result: &CoreType,
    callbacks: &[Callback],
    name: String,
) -> NativeIrResult<CoreFunction> {
    let mut helper = owner.clone();
    helper.name = name;
    // Retain the real source call owner for debug metadata. A generated runner
    // has no source declaration of its own, just like other lowering helpers.
    helper.receiver_method = false;
    helper.trait_method = None;
    // Link-visible to checked callers in other modules, but not added to the
    // source module's export declarations. The generated name is not source syntax.
    helper.public = true;
    helper.native_operation = None;
    helper.generic_params.clear();
    helper.arity = 1;
    let plan = schema.plan(result);
    helper.params = vec![CoreParam {
        name: "$plan".into(),
        ty: plan.contract_text(),
        core_ty: Some(plan),
    }];
    helper.return_type = result.contract_text();
    helper.core_return_type = Some(result.clone());
    helper.clauses.truncate(1);
    let clause = helper
        .clauses
        .first_mut()
        .ok_or("error[native_ir.effect_runner]: Effect.run owner has no checked clause")?;
    clause.patterns = vec!["$plan".into()];
    clause.core_patterns = vec![Some(CorePattern::Var("$plan".into()))];
    clause.pattern_proof_coverage = vec![CoreProofCoverage::RuntimeBoundary];
    clause.pattern_checked_preservation_evidence = vec![None];
    clause.guard = None;
    let failure = intrinsic(CoreIntrinsicId::VmEffectFail, vec![var("$error")], unit());
    let actor = intrinsic(
        CoreIntrinsicId::VmProcessCurrent(unit()),
        vec![],
        CoreType::Int,
    );
    let cancel = intrinsic(
        CoreIntrinsicId::VmProcessCancel(unit()),
        vec![var("$actor")],
        unit(),
    );
    let cancel = CoreExpr::Let {
        bindings: vec![binding("$actor", actor)],
        body: Box::new(terminal(cancel)),
    };
    clause.body.kind = "case".into();
    clause.body.checked_preservation_evidence = None;
    clause.body.proof_coverage = CoreProofCoverage::RuntimeBoundary;
    clause.body.text = None;
    clause.body.remote = None;
    clause.body.operator = None;
    clause.body.arity = 0;
    clause.body.children.clear();
    clause.body.core_expr = Some(CoreExpr::Case {
        scrutinee: Box::new(var("$plan")),
        clauses: vec![
            variant("Pure", &["$value"], var("$value")),
            variant(
                "Mapped",
                &["$child", "$callback"],
                dispatch(schema, result, callbacks, false),
            ),
            variant(
                "FlatMap",
                &["$child", "$callback"],
                dispatch(schema, result, callbacks, true),
            ),
            variant("Failed", &["$error"], terminal(failure)),
            variant("Cancelled", &["$marker"], cancel),
        ],
    });
    super::visit_function(&mut helper, &mut |expr| {
        if let CoreExpr::Call { function, .. } = expr {
            if let Some(local) = function.strip_prefix("std.core.Effect.") {
                *function = local.to_string();
            }
        }
    });
    Ok(helper)
}

fn dispatch(schema: &Schema, result: &CoreType, callbacks: &[Callback], flat: bool) -> CoreExpr {
    let mut clauses = callbacks
        .iter()
        .filter(|cb| cb.output == *result && cb.flat == flat)
        .map(|cb| {
            let output = if flat {
                schema.plan(result)
            } else {
                result.clone()
            };
            let signature = CoreType::Arrow {
                params: vec![cb.input.clone()],
                return_type: Box::new(output),
            };
            let input = run_call(&cb.input, cast(var("$child"), schema.plan(&cb.input)));
            let invoke = CoreExpr::FunctionCall {
                callee: Box::new(var("$invoke")),
                args: vec![var("$input")],
            };
            let output = if flat {
                run_call(result, invoke)
            } else {
                invoke
            };
            CoreIfClause {
                condition: intrinsic(
                    CoreIntrinsicId::ErasedValueIs(signature.clone()),
                    vec![var("$callback")],
                    CoreType::Bool,
                ),
                body: CoreExpr::Let {
                    bindings: vec![
                        binding("$input", input),
                        binding("$invoke", cast(var("$callback"), signature)),
                    ],
                    body: Box::new(output),
                },
            }
        })
        .collect::<Vec<_>>();
    if clauses.is_empty() {
        // No admitted callback can inhabit this variant in this image. A
        // nonmatching case is a real runtime match error, never a success value.
        unreachable_result()
    } else {
        // A valid box with an unadmitted signature also fails the match. Invalid
        // or foreign envelopes fail the checked query before branch selection.
        clauses.push(CoreIfClause {
            condition: CoreExpr::Atom("true".into()),
            body: unreachable_result(),
        });
        CoreExpr::If { clauses }
    }
}

fn terminal(operation: CoreExpr) -> CoreExpr {
    CoreExpr::Let {
        bindings: vec![binding("$terminated", operation)],
        body: Box::new(unreachable_result()),
    }
}

/// This typed continuation is unreachable after failure/cancellation; if a VM
/// resumes it incorrectly, its known non-Pure plan must fail matching, not return.
fn unreachable_result() -> CoreExpr {
    CoreExpr::Case {
        scrutinee: Box::new(var("$plan")),
        clauses: vec![variant("Pure", &["$unreachable"], var("$unreachable"))],
    }
}

fn variant(name: &str, fields: &[&str], body: CoreExpr) -> CoreCaseClause {
    CoreCaseClause {
        pattern: CorePattern::Constructor {
            name: name.into(),
            constructor_identity: Some(format!("{MODULE}.{name}")),
            args: fields
                .iter()
                .map(|name| CorePattern::Var((*name).into()))
                .collect(),
        },
        guard: None,
        body,
    }
}

fn intrinsic(id: CoreIntrinsicId, args: Vec<CoreExpr>, return_type: CoreType) -> CoreExpr {
    CoreExpr::Intrinsic(CoreIntrinsicCall {
        id,
        args,
        return_type,
        effects: CoreEffectSet {
            effects: vec!["vm_effect_execution".into()],
        },
        span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
    })
}

fn var(name: &str) -> CoreExpr {
    CoreExpr::Var(name.into())
}

fn unit() -> CoreType {
    CoreType::Named("Unit".into())
}

fn cast(expr: CoreExpr, target_type: CoreType) -> CoreExpr {
    CoreExpr::Cast {
        expr: Box::new(expr),
        target_type,
    }
}

fn binding(name: &str, value: CoreExpr) -> CoreLetBinding {
    CoreLetBinding {
        pattern: CorePattern::Var(name.into()),
        value,
    }
}
