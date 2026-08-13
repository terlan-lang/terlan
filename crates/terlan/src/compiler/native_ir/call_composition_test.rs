use std::collections::{HashMap, HashSet};

use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{
    CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CoreLetBinding, CorePattern,
    CoreRuntimeCapability, CoreType,
};

use super::call_composition::{CallTarget, ComposedContinuationProfile};
use super::{
    composed_call_region, rebase_callee_locals, ComposedCallProfile, NativeCallResume,
    NativeContinuation, NativeExpr, NativeTransitionOperation, NativeType,
    RecursiveReductionMember,
};

fn suspend(continuation_id: u64) -> NativeExpr {
    NativeExpr::Suspend {
        operation: NativeTransitionOperation::Yield,
        arguments: Vec::new(),
        continuation_id,
        values: Vec::new(),
    }
}

#[test]
fn tuple_contained_suspending_calls_participate_in_admission() {
    let identity = ("app.yielding".to_string(), 1);
    let body = CoreExpr::Tuple(vec![CoreExpr::Call {
        function: identity.0.clone(),
        args: vec![CoreExpr::Int(1)],
    }]);
    let suspending = HashSet::from([identity.clone()]);
    let composable = HashSet::from([identity]);

    assert_eq!(
        super::call_composition::suspending_call_count(&body, &suspending),
        1
    );
    assert!(super::call_composition::is_composable_suspending_body(
        &body,
        &suspending,
        &composable,
    ));
}

#[test]
fn process_transition_arguments_compose_before_the_transition() {
    let identity = ("app.render".to_string(), 1);
    let expression = CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileWriteText),
        args: vec![
            CoreExpr::Binary("artifact.txt".to_string()),
            CoreExpr::Call {
                function: identity.0.clone(),
                args: vec![CoreExpr::Var("rows".to_string())],
            },
        ],
        return_type: CoreType::Dynamic,
        effects: CoreEffectSet {
            effects: vec!["filesystem".to_string()],
        },
        span: Span { start: 0, end: 0 },
    });
    let suspending = HashSet::from([identity.clone()]);
    let region = composed_call_region(
        &expression,
        &suspending,
        &|name, arity| (name.to_string(), arity) == identity,
        &HashSet::new(),
    )
    .expect("nested call must compose before the process transition");

    assert!(matches!(region.target, CallTarget::Direct(ref name) if name == "app.render"));
    let CoreExpr::Intrinsic(resume) = region.resume else {
        panic!("process transition must remain as the resume expression");
    };
    assert!(matches!(
        resume.args.as_slice(),
        [CoreExpr::Var(_), CoreExpr::Var(name)] if name == "$native_call_result"
    ));
}

#[test]
fn generated_model_continuation_width_has_real_world_headroom() {
    let identity = ("package.model_operation".to_string(), 0);
    let body = CoreExpr::Tuple(
        (0..1_826)
            .map(|_| CoreExpr::Call {
                function: identity.0.clone(),
                args: Vec::new(),
            })
            .collect(),
    );
    let suspending = HashSet::from([identity.clone()]);
    let composable = HashSet::from([identity]);

    assert_eq!(
        super::call_composition::suspending_call_count(&body, &suspending),
        1_826
    );
    assert!(super::call_composition::is_composable_suspending_body(
        &body,
        &suspending,
        &composable,
    ));
}

#[test]
fn pathological_continuation_width_remains_bounded() {
    let identity = ("package.model_operation".to_string(), 0);
    let body = CoreExpr::Tuple(
        (0..16_385)
            .map(|_| CoreExpr::Call {
                function: identity.0.clone(),
                args: Vec::new(),
            })
            .collect(),
    );
    let suspending = HashSet::from([identity.clone()]);
    let composable = HashSet::from([identity]);

    assert!(!super::call_composition::is_composable_suspending_body(
        &body,
        &suspending,
        &composable,
    ));
    assert!(super::call_composition::composable_suspension_gap_reason(
        &body,
        &suspending,
        &composable,
    )
    .contains("maximum is 16384"));
}

/// Verifies nested package-native calls become ordered continuation regions.
///
/// Inputs:
/// - One composable suspending call used as the argument of another composable
///   suspending call.
///
/// Output:
/// - The first region targets the inner call and resumes by invoking the outer
///   call with the inner result.
///
/// Transformation:
/// - Preserves ordinary nested call evaluation while allowing both package
///   operations to suspend independently.
#[test]
fn nested_composable_suspending_call_arguments_are_sequenced() {
    let inner = ("package.inner".to_string(), 1);
    let outer = ("package.outer".to_string(), 1);
    let body = CoreExpr::Call {
        function: outer.0.clone(),
        args: vec![CoreExpr::Call {
            function: inner.0.clone(),
            args: vec![CoreExpr::Int(7)],
        }],
    };
    let suspending = HashSet::from([inner.clone(), outer.clone()]);
    let composable = HashSet::from([inner.clone(), outer.clone()]);

    let region = composed_call_region(
        &body,
        &suspending,
        &|function, arity| composable.contains(&(function.to_string(), arity)),
        &HashSet::new(),
    )
    .expect("nested composable calls must produce an inner region");

    assert!(matches!(
        region.target,
        super::call_composition::CallTarget::Direct(ref function) if function == &inner.0
    ));
    assert_eq!(
        region.resume,
        CoreExpr::Call {
            function: outer.0,
            args: vec![CoreExpr::Var("$native_call_result".to_string())],
        }
    );
}

#[test]
fn compiler_remote_operation_sequences_a_nested_suspending_call() {
    let nested = ("app.recurse".to_string(), 1);
    let body = CoreExpr::RemoteCall {
        module: "$terlan.managed.comprehension".to_string(),
        function: "prepend".to_string(),
        args: vec![
            CoreExpr::Int(7),
            CoreExpr::Call {
                function: nested.0.clone(),
                args: vec![CoreExpr::Int(1)],
            },
        ],
    };
    let suspending = HashSet::from([nested.clone()]);
    let region = composed_call_region(
        &body,
        &suspending,
        &|function, arity| (function.to_string(), arity) == nested,
        &HashSet::new(),
    )
    .expect("nested remote-operation argument must compose");

    assert!(matches!(
        region.target,
        super::call_composition::CallTarget::Direct(ref function)
            if function == "app.recurse"
    ));
    assert_eq!(region.prefix.len(), 1);
    assert!(matches!(
        region.resume,
        CoreExpr::RemoteCall { ref module, ref function, ref args }
            if module == "$terlan.managed.comprehension"
                && function == "prepend"
                && args.len() == 2
                && matches!(&args[1], CoreExpr::Var(name) if name == "$native_call_result")
    ));
}

#[test]
fn missing_profile_is_pure_only_when_no_transition_edge_exists() {
    let suspending = HashSet::from([3]);

    assert!(super::call_composition::is_definitely_non_suspending(
        &NativeExpr::Call {
            function: 2,
            args: vec![NativeExpr::Int(1)],
        },
        &suspending,
    ));
    assert!(!super::call_composition::is_definitely_non_suspending(
        &NativeExpr::Call {
            function: 3,
            args: vec![NativeExpr::Int(1)],
        },
        &suspending,
    ));
    assert!(!super::call_composition::is_definitely_non_suspending(
        &NativeExpr::InvokeClosure {
            callee: Box::new(NativeExpr::Param(0)),
            args: vec![NativeExpr::Int(1)],
            parameter_types: vec![NativeType::Int],
            result_type: NativeType::Bool,
        },
        &suspending,
    ));
}

#[test]
fn profile_follows_continuation_identities_after_storage_reordering() {
    let continuations = vec![
        NativeContinuation {
            id: 30,
            source_module: "app.Test".to_string(),
            source_function: "main".to_string(),
            source_arity: 0,
            source_span: None,
            capture_names: Vec::new(),
            params: vec![NativeType::Bool],
            return_type: NativeType::Bool,
            body: NativeExpr::Bool(true),
        },
        NativeContinuation {
            id: 10,
            source_module: "app.Test".to_string(),
            source_function: "main".to_string(),
            source_arity: 0,
            source_span: None,
            capture_names: Vec::new(),
            params: vec![NativeType::Int],
            return_type: NativeType::Bool,
            body: suspend(20),
        },
        NativeContinuation {
            id: 20,
            source_module: "app.Test".to_string(),
            source_function: "main".to_string(),
            source_arity: 0,
            source_span: None,
            capture_names: Vec::new(),
            params: vec![NativeType::Float],
            return_type: NativeType::Bool,
            body: suspend(30),
        },
    ];

    let profile = ComposedCallProfile::new(&suspend(10), &continuations, &HashMap::new())
        .expect("linear ID chain");

    assert_eq!(
        profile
            .continuations
            .iter()
            .map(|continuation| continuation.id)
            .collect::<Vec<_>>(),
        [10, 20, 30]
    );
}

#[test]
fn profile_admits_a_callee_that_can_complete_or_suspend() {
    let body = NativeExpr::If {
        clauses: vec![
            (NativeExpr::Param(0), suspend(10)),
            (NativeExpr::Bool(true), NativeExpr::Int(7)),
        ],
    };
    let continuations = vec![NativeContinuation {
        id: 10,
        source_module: "app.Test".to_string(),
        source_function: "maybe_yield".to_string(),
        source_arity: 1,
        source_span: None,
        capture_names: Vec::new(),
        params: Vec::new(),
        return_type: NativeType::Int,
        body: NativeExpr::Int(41),
    }];

    let profile = ComposedCallProfile::new(&body, &continuations, &HashMap::new())
        .expect("optional suspension has a closed continuation profile");

    assert_eq!(profile.continuations.len(), 1);
    assert_eq!(profile.continuations[0].id, 10);
}

#[test]
fn recursive_component_contract_includes_every_members_outward_yields() {
    let mut profile = ComposedCallProfile {
        continuations: vec![ComposedContinuationProfile {
            id: 10,
            source_span: None,
            params: vec![NativeType::Int],
            body: NativeExpr::CallThen {
                function: 2,
                args: vec![NativeExpr::Param(0)],
                resumes: vec![NativeCallResume {
                    callee_continuation_id: 10,
                    callee_capture_count: 1,
                    continuation_id: 10,
                    caller_value_start: 0,
                }],
                completion_continuation_id: 90,
                completion_function: None,
                values: Vec::new(),
            },
            completion_result: true,
        }],
        entries: vec![10],
        tail_entries: HashMap::new(),
    };
    let mut outward = profile.continuations[0].clone();
    outward.completion_result = false;
    let sibling = ComposedCallProfile {
        continuations: vec![
            outward,
            ComposedContinuationProfile {
                id: 20,
                source_span: None,
                params: vec![NativeType::Int, NativeType::Bool],
                body: NativeExpr::Param(0),
                completion_result: false,
            },
        ],
        entries: vec![20],
        tail_entries: HashMap::new(),
    };

    profile.merge_recursive_component_profile(&sibling);
    let entries = profile.refresh_recursive_component_contract(&[1, 2]);

    assert_eq!(entries, [(10, 1), (20, 2)]);
    assert_eq!(profile.tail_entries[&1], entries);
    assert_eq!(profile.tail_entries[&2], entries);
    assert!(!profile.continuations[0].completion_result);
    let NativeExpr::CallThen { resumes, .. } = &profile.continuations[0].body else {
        panic!("recursive continuation must retain its call-then body");
    };
    assert!(resumes.iter().any(|resume| {
        resume.callee_continuation_id == 20
            && resume.callee_capture_count == 2
            && resume.continuation_id == 20
            && resume.caller_value_start == 0
    }));
}

#[test]
fn recursive_reduction_identity_forwarding_preserves_wide_capture_shape() {
    let profile = ComposedCallProfile::recursive_component(
        "app.Work",
        "reduce",
        3,
        NativeType::Int,
        vec![RecursiveReductionMember {
            module: "app.Work".to_string(),
            function_name: "reduce".to_string(),
            arity: 3,
            function: 7,
            params: vec![NativeType::Int, NativeType::Int, NativeType::Int],
        }],
    );
    let profiles = HashMap::from([(7, profile.clone())]);
    let labels = HashMap::from([(7, "app.Work.reduce/3".to_string())]);
    let destinations = profile
        .continuations
        .iter()
        .map(|continuation| (continuation.id, continuation.params.len()))
        .collect::<HashMap<_, _>>();

    super::call_composition::validate_call_then_contracts_with_destinations(
        &profile.continuations[0].body,
        &profiles,
        &labels,
        &destinations,
    )
    .expect("tail reduction forwards its three captured parameters");
}

#[test]
fn composed_wrapper_identity_does_not_depend_on_profile_width() {
    let first =
        super::identity::stable_composed_continuation_id("app.Main", "run", 1, 7, None, 101);
    let repeated =
        super::identity::stable_composed_continuation_id("app.Main", "run", 1, 7, None, 101);
    let sibling =
        super::identity::stable_composed_continuation_id("app.Main", "run", 1, 7, None, 202);
    let dynamic =
        super::identity::stable_composed_continuation_id("app.Main", "run", 1, 7, Some(55), 101);

    assert_eq!(first, repeated);
    assert_ne!(first, sibling);
    assert_ne!(first, dynamic);
    assert_ne!(
        first,
        super::identity::stable_composed_completion_id("app.Main", "run", 1, 7)
    );
}

#[test]
fn recursive_contract_requires_a_wrapper_for_non_tail_caller_frames() {
    let mut body = NativeExpr::CallThen {
        function: 4,
        args: vec![NativeExpr::Param(0)],
        resumes: Vec::new(),
        completion_continuation_id: 90,
        completion_function: None,
        values: vec![NativeExpr::Param(1)],
    };

    super::call_composition::refresh_recursive_call_contract(&mut body, 4, &[(20, 2)]);

    let NativeExpr::CallThen { resumes, .. } = body else {
        panic!("call-then shape must be preserved");
    };
    assert!(resumes.is_empty());
}

#[test]
fn nested_wrapper_appends_only_the_new_caller_value_suffix() {
    let body = NativeExpr::CallThen {
        function: 4,
        args: vec![NativeExpr::Param(0)],
        resumes: vec![NativeCallResume {
            callee_continuation_id: 10,
            callee_capture_count: 2,
            continuation_id: 10,
            caller_value_start: 2,
        }],
        completion_continuation_id: 90,
        completion_function: None,
        values: vec![NativeExpr::Param(0), NativeExpr::Param(1)],
    };
    let wrapper_ids = HashMap::from([(10, 20), (90, 99)]);
    let completion_result_ids = HashSet::new();
    let tail_entries = HashMap::new();
    let wrapped = super::composed_continuation::wrap_composed_continuation(
        &body,
        3,
        &super::composed_continuation::ComposedContinuationContext {
            caller_capture_start: 2,
            caller_capture_count: 1,
            wrapper_ids: &wrapper_ids,
            completion_result_ids: &completion_result_ids,
            tail_entries: &tail_entries,
            completion_id: 99,
        },
    )
    .expect("nested continuation wrapper");

    let NativeExpr::CallThen {
        resumes, values, ..
    } = wrapped
    else {
        panic!("call-then shape must be preserved");
    };
    assert_eq!(values.len(), 3);
    assert_eq!(values[2], NativeExpr::Param(2));
    assert_eq!(resumes[0].continuation_id, 20);
    assert_eq!(resumes[0].caller_value_start, 2);
}

#[test]
fn root_owned_wrapper_is_valid_without_a_reusable_caller_profile() {
    let callee = ComposedCallProfile {
        continuations: vec![ComposedContinuationProfile {
            id: 10,
            source_span: None,
            params: vec![NativeType::Int, NativeType::Bool],
            body: NativeExpr::Param(0),
            completion_result: false,
        }],
        entries: vec![10],
        tail_entries: HashMap::new(),
    };
    let profiles = HashMap::from([(4, callee)]);
    let labels = HashMap::from([(4, "fixture.callee/0".to_string())]);
    let body = NativeExpr::CallThen {
        function: 4,
        args: Vec::new(),
        resumes: vec![NativeCallResume {
            callee_continuation_id: 10,
            callee_capture_count: 2,
            continuation_id: 20,
            caller_value_start: 0,
        }],
        completion_continuation_id: 21,
        completion_function: None,
        values: Vec::new(),
    };

    let profile_only =
        super::call_composition::validate_call_then_contracts(&body, &profiles, &labels)
            .expect_err("root-owned wrapper is not part of a reusable caller profile");
    assert!(profile_only.contains("absent continuation 20"));

    super::call_composition::validate_call_then_contracts_with_destinations(
        &body,
        &profiles,
        &labels,
        &HashMap::from([(20, 1)]),
    )
    .expect("emitted application owns the root wrapper");
}

#[test]
fn rebasing_reaches_managed_operation_arguments() {
    let body = NativeExpr::ManagedOperation {
        encoded: b"test".as_slice().into(),
        args: vec![NativeExpr::Param(0), NativeExpr::Param(2)],
    };

    assert_eq!(
        rebase_callee_locals(&body, 1, 3),
        NativeExpr::ManagedOperation {
            encoded: b"test".as_slice().into(),
            args: vec![NativeExpr::Param(0), NativeExpr::Param(5)],
        }
    );
}

#[test]
fn gated_call_factors_the_surrounding_suffix_into_one_join() {
    let call = CoreExpr::Call {
        function: "package.native_check".to_string(),
        args: Vec::new(),
    };
    let expr = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("valid".to_string()),
            value: CoreExpr::BinaryOp {
                operator: "and".to_string(),
                left: Box::new(CoreExpr::Atom("true".to_string())),
                right: Box::new(call),
            },
        }],
        body: Box::new(CoreExpr::Var("valid".to_string())),
    };
    let suspending = HashSet::from([("package.native_check".to_string(), 0)]);

    let region = composed_call_region(&expr, &suspending, &|_, _| true, &HashSet::new())
        .expect("suspending branch must compose");

    assert_eq!(region.gates.len(), 1);
    assert_eq!(
        region.gates[0].bypass_resume,
        CoreExpr::Atom("false".to_string())
    );
    assert!(matches!(
        region.join.expect("outer suffix must be shared").resume,
        CoreExpr::Let { .. }
    ));
}
