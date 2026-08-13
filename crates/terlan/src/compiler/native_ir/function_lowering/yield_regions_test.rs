use crate::{
    terlan_syntax::span::Span,
    terlan_typeck::{
        CoreCaseClause, CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CorePattern,
        CoreType,
    },
};

use super::yield_regions::{
    condition_yield_region, freshen_generated_prefix_names, process_transition_span,
    yield_capture_set, YieldRegion,
};
use super::NativeTransitionOperation;

/// Constructs one native operation intrinsic with the requested source span.
fn native_operation(span: Span) -> CoreExpr {
    CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::NativeOperation {
            operation: "fixture.native.call".to_string(),
            parameter_types: Vec::new(),
        },
        args: Vec::new(),
        return_type: CoreType::AtomLiteral("Unit".to_string()),
        effects: CoreEffectSet {
            effects: vec!["native-package".to_string()],
        },
        span,
    })
}

/// Sentinel spans from generated native package calls must not enter debug metadata.
#[test]
fn generated_native_operation_omits_empty_source_span() {
    assert_eq!(
        process_transition_span(&native_operation(Span::new(0, 0))),
        None
    );
}

/// Source-backed native transitions retain their exact expression span.
#[test]
fn source_native_operation_retains_nonempty_span() {
    assert_eq!(
        process_transition_span(&native_operation(Span::new(7, 19))),
        Some(Span::new(7, 19))
    );
}

/// A capability used as a structured-case scrutinee resumes into that exact case.
#[test]
fn case_scrutinee_transition_becomes_one_ordered_yield_region() {
    let clauses = vec![CoreCaseClause {
        pattern: CorePattern::Wildcard,
        guard: None,
        body: CoreExpr::Atom("true".to_string()),
    }];
    let expression = CoreExpr::Case {
        scrutinee: Box::new(native_operation(Span::new(3, 9))),
        clauses: clauses.clone(),
    };

    let region = condition_yield_region(&expression).expect("case yield region");
    assert_eq!(region.source_span, Some(Span::new(3, 9)));
    assert_eq!(
        region.resume,
        CoreExpr::Case {
            scrutinee: Box::new(CoreExpr::Var("$native_transition_result".to_string())),
            clauses,
        }
    );
}

/// A contextual result cast around a call argument cannot hide an earlier
/// capability suspension from eager-order lowering.
#[test]
fn cast_wrapped_call_argument_transition_becomes_one_ordered_yield_region() {
    let target_type = CoreType::Union(vec![CoreType::Int, CoreType::Atom]);
    let expression = CoreExpr::Cast {
        expr: Box::new(CoreExpr::Call {
            function: "fixture.parse".to_string(),
            args: vec![native_operation(Span::new(31, 47))],
        }),
        target_type: target_type.clone(),
    };

    let region = condition_yield_region(&expression).expect("cast-wrapped call yield region");
    assert_eq!(region.source_span, Some(Span::new(31, 47)));
    assert_eq!(
        region.resume,
        CoreExpr::Cast {
            expr: Box::new(CoreExpr::Call {
                function: "fixture.parse".to_string(),
                args: vec![CoreExpr::Var("$native_transition_result".to_string())],
            }),
            target_type,
        }
    );
}

/// A later suspension in one resumed condition must not reuse an eager
/// temporary captured by an earlier suspension.
#[test]
fn eager_operand_binding_avoids_an_existing_generated_capture() {
    let expression = CoreExpr::BinaryOp {
        operator: "==".to_string(),
        left: Box::new(CoreExpr::Var("$native_eager_left_0".to_string())),
        right: Box::new(native_operation(Span::new(11, 17))),
    };

    let region = condition_yield_region(&expression).expect("binary yield region");
    assert!(matches!(
        region.prefix.as_slice(),
        [crate::terlan_typeck::CoreLetBinding {
            pattern: CorePattern::Var(name),
            ..
        }] if name == "$native_eager_left_0_11_17"
    ));
}

/// Sequential suspension lowering may carry a generated eager local into the
/// next continuation even when that local is no longer free in its expression.
#[test]
fn generated_yield_prefix_is_alpha_renamed_around_prior_captures() {
    let generated = "$native_eager_left_0_11_17".to_string();
    let region = YieldRegion {
        prefix: vec![crate::terlan_typeck::CoreLetBinding {
            pattern: CorePattern::Var(generated.clone()),
            value: CoreExpr::Int(7),
        }],
        operation: NativeTransitionOperation::Yield,
        arguments: vec![CoreExpr::Var(generated.clone())],
        result: None,
        result_core_type: None,
        resume: CoreExpr::Var(generated.clone()),
        source_span: Some(Span::new(11, 17)),
    };

    let freshened = freshen_generated_prefix_names(&region, std::slice::from_ref(&generated));
    let CorePattern::Var(freshened_name) = &freshened.prefix[0].pattern else {
        panic!("expected generated variable prefix")
    };
    assert_eq!(freshened_name, "$native_eager_left_0_11_17_1");
    assert_eq!(
        freshened.arguments,
        vec![CoreExpr::Var(freshened_name.clone())]
    );
    assert_eq!(freshened.resume, CoreExpr::Var(freshened_name.clone()));
}

/// A nested process transition must preserve values required by the enclosing
/// composed-call completion, even when its immediate resume only returns the
/// transition result.
#[test]
fn process_yield_retains_downstream_completion_captures() {
    let region = YieldRegion {
        prefix: Vec::new(),
        operation: NativeTransitionOperation::Yield,
        arguments: Vec::new(),
        result: Some((
            "$native_transition_result".to_string(),
            super::NativeType::Int,
        )),
        result_core_type: Some(CoreType::Int),
        resume: CoreExpr::Var("$native_transition_result".to_string()),
        source_span: Some(Span::new(23, 31)),
    };

    assert_eq!(
        yield_capture_set(&region, &["options".to_string()]),
        std::collections::HashSet::from(["options".to_string()])
    );
}
