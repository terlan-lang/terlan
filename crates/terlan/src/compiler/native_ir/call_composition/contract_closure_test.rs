//! Late callback suspension discovery must reach already lowered call sites.

use super::*;
use crate::compiler::native_ir::{
    call_composition::{ComposedContinuationProfile, DynamicTargetProfile},
    NativeDynamicCallResume, NativeType,
};

#[test]
fn synchronous_completion_can_reach_the_first_suspension() {
    use crate::compiler::native_ir::{NativeContinuation, NativeTransitionOperation};
    let continuations = vec![
        NativeContinuation {
            id: 90,
            source_module: "app.Callback".into(),
            source_function: "apply".into(),
            source_arity: 1,
            source_span: None,
            capture_names: Vec::new(),
            params: vec![NativeType::Int, NativeType::Bool],
            return_type: NativeType::Bool,
            body: NativeExpr::Suspend {
                operation: NativeTransitionOperation::Yield,
                arguments: Vec::new(),
                continuation_id: 91,
                values: Vec::new(),
            },
        },
        NativeContinuation {
            id: 91,
            source_module: "app.Callback".into(),
            source_function: "apply".into(),
            source_arity: 1,
            source_span: None,
            capture_names: Vec::new(),
            params: Vec::new(),
            return_type: NativeType::Bool,
            body: NativeExpr::Bool(true),
        },
    ];
    for body in [
        callback(),
        NativeExpr::CallThen {
            function: 0,
            args: vec![NativeExpr::Int(42)],
            resumes: Vec::new(),
            completion_continuation_id: 90,
            completion_function: None,
            values: vec![NativeExpr::Int(7)],
        },
    ] {
        let profile = ComposedCallProfile::new(&body, &continuations, &HashMap::new())
            .expect("synchronous completion reaches a suspension");
        assert_eq!(profile.entries, [91]);
        assert_eq!(profile.continuations.len(), 2);
        assert!(
            profile
                .continuations
                .iter()
                .find(|entry| entry.id == 90)
                .unwrap()
                .completion_result
        );
        assert!(
            !profile
                .continuations
                .iter()
                .find(|entry| entry.id == 91)
                .unwrap()
                .completion_result
        );
    }
}

fn callback() -> NativeExpr {
    NativeExpr::InvokeClosureThen {
        callee: Box::new(NativeExpr::Param(0)),
        args: vec![NativeExpr::Int(42)],
        parameter_types: vec![NativeType::Int],
        result_type: NativeType::Bool,
        resumes: Vec::new(),
        completion_continuation_id: 90,
        completion_function: None,
        values: vec![NativeExpr::Int(7)],
    }
}

fn profile() -> DynamicCallProfiles {
    HashMap::from([(
        DynamicCallSignature {
            parameters: vec![NativeType::Int],
            result: NativeType::Bool,
        },
        vec![DynamicTargetProfile {
            export_id: 20,
            source: "app.Callback.apply/1".into(),
            profile: ComposedCallProfile {
                entries: vec![30],
                continuations: vec![ComposedContinuationProfile {
                    id: 30,
                    source_span: None,
                    params: vec![NativeType::Int, NativeType::Bool],
                    body: NativeExpr::Bool(true),
                    completion_result: false,
                }],
                tail_entries: HashMap::new(),
            },
        }],
    )])
}

#[test]
fn late_callback_entries_close_nested_calls_idempotently() {
    let mut body = NativeExpr::Let {
        bindings: vec![callback()],
        body: Box::new(callback()),
    };
    close_call_contracts(&mut body, &HashMap::new(), &profile());
    let once = body.clone();
    close_call_contracts(&mut body, &HashMap::new(), &profile());
    assert_eq!(body, once);
    let mut calls = 0;
    walk_native_expr(&body, &mut |expr| {
        if let NativeExpr::InvokeClosureThen {
            resumes, values, ..
        } = expr
        {
            calls += 1;
            assert_eq!(values, &[NativeExpr::Int(7)]);
            assert_eq!(
                resumes,
                &[NativeDynamicCallResume {
                    callee_export_id: 20,
                    callee_continuation_id: 30,
                    callee_capture_count: 2,
                    continuation_id: 90,
                }]
            );
        }
    });
    assert_eq!(calls, 2);
}

#[test]
fn incompatible_callback_signatures_do_not_gain_resume_entries() {
    for (parameters, result) in [
        (vec![NativeType::Bool], NativeType::Bool),
        (vec![NativeType::Int], NativeType::Int),
        (vec![], NativeType::Bool),
    ] {
        let mut body = callback();
        let NativeExpr::InvokeClosureThen {
            parameter_types,
            result_type,
            ..
        } = &mut body
        else {
            unreachable!();
        };
        *parameter_types = parameters;
        *result_type = result;
        let original = body.clone();
        close_call_contracts(&mut body, &HashMap::new(), &profile());
        assert_eq!(body, original);
    }
}

#[test]
fn existing_callback_resume_routes_are_preserved() {
    let mut body = callback();
    let NativeExpr::InvokeClosureThen { resumes, .. } = &mut body else {
        unreachable!();
    };
    resumes.push(NativeDynamicCallResume {
        callee_export_id: 20,
        callee_continuation_id: 30,
        callee_capture_count: 2,
        continuation_id: 30,
    });
    let original = body.clone();
    close_call_contracts(&mut body, &HashMap::new(), &profile());
    assert_eq!(body, original);
}
