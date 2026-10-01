use super::*;

fn call(name: &str) -> CoreExpr {
    CoreExpr::Call {
        function: name.into(),
        type_args: vec![],
        args: vec![],
    }
}

#[test]
fn dynamic_call_retains_callee_and_earlier_arguments_before_a_suspending_argument() {
    let body = CoreExpr::FunctionCall {
        callee: Box::new(CoreExpr::Var("callback".into())),
        args: vec![call("counter"), call("wait"), CoreExpr::Int(9)],
    };
    let region = composed_call_region(
        &body,
        &HashSet::from([("wait".into(), 0)]),
        &|name, _| name == "wait",
        &HashSet::from(["$native_dynamic_arg_0".into()]),
    )
    .unwrap();
    assert!(matches!(&region.target, CallTarget::Direct(name) if name == "wait"));
    assert_eq!(region.prefix.len(), 2);
    assert_eq!(region.prefix[0].value, CoreExpr::Var("callback".into()));
    assert_eq!(region.prefix[1].value, call("counter"));
    let CorePattern::Var(callee) = &region.prefix[0].pattern else {
        panic!("callee capture")
    };
    let CorePattern::Var(earlier) = &region.prefix[1].pattern else {
        panic!("argument capture")
    };
    assert_ne!(callee, "$native_dynamic_arg_0");
    assert_eq!(
        region.resume,
        CoreExpr::FunctionCall {
            callee: Box::new(CoreExpr::Var(callee.clone())),
            args: vec![
                CoreExpr::Var(earlier.clone()),
                CoreExpr::Var(region.result_name.clone()),
                CoreExpr::Int(9)
            ],
        }
    );
}

#[test]
fn suspending_callable_factory_precedes_all_dynamic_arguments() {
    let body = CoreExpr::FunctionCall {
        callee: Box::new(call("factory")),
        args: vec![call("argument")],
    };
    let suspending = HashSet::from([("factory".into(), 0), ("argument".into(), 0)]);
    let region = composed_call_region(&body, &suspending, &|_, _| true, &HashSet::new()).unwrap();
    assert!(matches!(&region.target, CallTarget::Direct(name) if name == "factory"));
    assert!(region.prefix.is_empty());
    assert_eq!(
        region.resume,
        CoreExpr::FunctionCall {
            callee: Box::new(CoreExpr::Var(region.result_name.clone())),
            args: vec![call("argument")],
        }
    );
    assert!(composed_call_region(
        &body,
        &suspending,
        &|name, _| name == "argument",
        &HashSet::new()
    )
    .is_none());
}

#[test]
fn conditional_dynamic_argument_keeps_its_call_gate_and_outer_resume() {
    use crate::terlan_typeck::CoreIfClause;
    let body = CoreExpr::FunctionCall {
        callee: Box::new(CoreExpr::Var("callback".into())),
        args: vec![CoreExpr::If {
            clauses: vec![
                CoreIfClause {
                    condition: CoreExpr::Var("enabled".into()),
                    body: call("wait"),
                },
                CoreIfClause {
                    condition: CoreExpr::Atom("true".into()),
                    body: CoreExpr::Int(9),
                },
            ],
        }],
    };
    let region = composed_call_region(
        &body,
        &HashSet::from([("wait".into(), 0)]),
        &|name, _| name == "wait",
        &HashSet::new(),
    )
    .unwrap();
    assert_eq!(region.gates.len(), 1);
    assert_eq!(region.gates[0].condition, CoreExpr::Var("enabled".into()));
    assert!(region.gates[0].call_when_true);
    let join = region.join.unwrap();
    let CoreExpr::FunctionCall { args, .. } = join.resume else {
        panic!("outer call must follow either branch")
    };
    assert_eq!(args, vec![CoreExpr::Var(join.result_name)]);
}
