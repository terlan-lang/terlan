//! Source accessor calls retain generic field proofs; escapes fail closed.
use super::*;

fn call(target: usize) -> NativeExpr {
    NativeExpr::Call {
        function: target,
        args: vec![NativeExpr::Param(0)],
    }
}

fn caller_projection(modules: &[NativeModule]) -> AggregateFieldProjection {
    native_aggregate_projections(modules)[0].fields.clone()
}

#[test]
fn cross_module_accessors_and_returned_aliases_preserve_observed_fields() {
    let mut caller = module(NativeExpr::Let {
        bindings: vec![call(1)],
        body: Box::new(project(8, NativeExpr::Param(1))),
    });
    let mut accessor = module(NativeExpr::Param(0));
    accessor.name = "app.Accessors".into();
    accessor.functions[0].public = false;
    accessor.functions[0].name = "identity".into();
    assert_eq!(
        caller_projection(&[caller.clone(), accessor.clone()]),
        AggregateFieldProjection::Fields([8].into())
    );
    caller.functions[0].body = NativeExpr::TailCall {
        function: 1,
        args: vec![NativeExpr::Param(0)],
        yield_continuation_id: None,
    };
    assert_eq!(
        caller_projection(&[caller.clone(), accessor.clone()]),
        AggregateFieldProjection::Complete
    );
    accessor.functions[0].body = project(0, NativeExpr::Param(0));
    assert_eq!(
        caller_projection(&[caller, accessor]),
        AggregateFieldProjection::Fields([0].into())
    );
}

#[test]
fn recursion_missing_targets_wrong_parameter_shapes_and_captures_fail_closed() {
    let caller = module(call(1));
    let accessor = module(project(8, NativeExpr::Param(0)));
    for body in [call(0), call(1), call(usize::MAX), NativeExpr::Param(0)] {
        let mut bad = accessor.clone();
        bad.functions[0].body = body;
        assert_eq!(
            caller_projection(&[caller.clone(), bad]),
            AggregateFieldProjection::Complete
        );
    }
    let mut bad = accessor.clone();
    bad.functions[0].params.clear();
    assert_eq!(
        caller_projection(&[caller.clone(), bad]),
        AggregateFieldProjection::Complete
    );
    let mut bad = accessor.clone();
    bad.functions[0].params[0] = NativeType::Int;
    assert_eq!(
        caller_projection(&[caller.clone(), bad]),
        AggregateFieldProjection::Complete
    );
    let mut bad = accessor;
    bad.functions[0].callable_captures = vec![NativeType::Int];
    assert_eq!(
        caller_projection(&[caller, bad]),
        AggregateFieldProjection::Complete
    );
}

#[test]
fn call_budget_bounds_repeated_accessor_analysis_without_hiding_escape() {
    let accessor = module(project(8, NativeExpr::Param(0)));
    for (count, expected) in [
        (128, AggregateFieldProjection::Fields([8].into())),
        (129, AggregateFieldProjection::Complete),
    ] {
        let caller = module(NativeExpr::Let {
            bindings: vec![call(1); count],
            body: Box::new(NativeExpr::Int(1)),
        });
        assert_eq!(caller_projection(&[caller, accessor.clone()]), expected);
    }
}
