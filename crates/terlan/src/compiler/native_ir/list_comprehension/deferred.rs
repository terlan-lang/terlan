//! VM-owned execution of deferred comprehension bodies and guard decisions.

use super::*;

/// Wraps the collector in a standard, typed Effect descriptor without running it.
pub(super) fn plan(body: CoreExpr, result: CoreType) -> CoreExpr {
    let unit = CoreType::Named("Unit".to_string());
    let input = CoreExpr::Cast {
        expr: Box::new(CoreExpr::Tuple(vec![
            CoreExpr::Atom("pure".to_string()),
            CoreExpr::Atom("Unit".to_string()),
        ])),
        target_type: effect(unit.clone()),
    };
    let callback = CoreExpr::Cast {
        expr: Box::new(CoreExpr::Lam {
            params: vec![CorePattern::Var("$comprehension_trigger".to_string())],
            parameter_types: vec![Some(unit.clone())],
            body: Box::new(body),
        }),
        target_type: CoreType::Arrow {
            params: vec![unit],
            return_type: Box::new(result.clone()),
        },
    };
    CoreExpr::Cast {
        expr: Box::new(CoreExpr::Tuple(vec![
            CoreExpr::Atom("mapped".to_string()),
            input,
            callback,
        ])),
        target_type: effect(result),
    }
}

/// Executes checked Effect guards only from within the deferred collector.
pub(in crate::compiler::native_ir) fn lower_guards(guards: &mut [CoreExpr]) {
    for guard in guards {
        let CoreExpr::Cast { expr, target_type } = guard else {
            continue;
        };
        if !is_effect(target_type) {
            continue;
        }
        let decision = expr.as_ref().clone();
        *guard = CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmEffectRun),
            args: vec![CoreExpr::Cast {
                expr: Box::new(decision),
                target_type: effect(CoreType::Bool),
            }],
            return_type: CoreType::Bool,
            effects: CoreEffectSet {
                effects: vec!["vm_effect_execution".to_string()],
            },
            span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
        });
    }
}

fn effect(result: CoreType) -> CoreType {
    CoreType::Apply {
        constructor: EFFECT_CONTAINER.to_string(),
        args: vec![result],
    }
}

fn is_effect(ty: &CoreType) -> bool {
    match ty {
        CoreType::Apply { constructor, .. } => constructor == EFFECT_CONTAINER,
        CoreType::Union(variants) => variants.iter().any(|variant| {
            let CoreType::Tuple(fields) = variant else {
                return false;
            };
            fields.iter().any(|field| {
                matches!(field, CoreTupleTypeElem::Field { ty: CoreType::Named(name), .. }
                if name == super::super::effect_values::ERASED_VALUE_TYPE)
            })
        }),
        _ => false,
    }
}
