//! Tests for the versioned CoreIR-to-NativeIR lowering coverage contract.

use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{
    CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CorePattern,
    CorePrimitiveIntrinsic, CoreRecordExprField, CoreRuntimeCapability, CoreType,
};

use super::lowering_coverage::{
    effect_coverage, expression_coverage, intrinsic_coverage, pattern_coverage,
    LoweringDisposition, LOWERING_COVERAGE_VERSION,
};

/// Verifies the matrix has an explicit, nonzero schema version.
#[test]
fn lowering_coverage_matrix_is_versioned() {
    assert_eq!(LOWERING_COVERAGE_VERSION, 6);
}

/// Verifies direct calls and named values are native while remote calls and
/// unresolved dynamic invocation enter mandatory compiler rewriting.
#[test]
fn call_families_have_explicit_lowering_dispositions() {
    let direct = CoreExpr::Call {
        function: "inc".to_string(),
        args: vec![CoreExpr::Int(1)],
    };
    let remote = CoreExpr::RemoteCall {
        module: "math".to_string(),
        function: "inc".to_string(),
        args: vec![CoreExpr::Int(1)],
    };
    let dynamic = CoreExpr::FunctionCall {
        callee: Box::new(CoreExpr::Var("callback".to_string())),
        args: vec![CoreExpr::Int(1)],
    };
    let named_value = CoreExpr::RemoteFunRef {
        module: "math".to_string(),
        function: "inc".to_string(),
        arity: 1,
    };

    assert_eq!(
        expression_coverage(&direct).disposition,
        LoweringDisposition::NativeLowered
    );
    assert_eq!(
        expression_coverage(&remote).disposition,
        LoweringDisposition::CompilerRewrite
    );
    assert_eq!(
        expression_coverage(&dynamic).disposition,
        LoweringDisposition::NativeLowered
    );
    assert_eq!(
        expression_coverage(&named_value).disposition,
        LoweringDisposition::NativeLowered
    );
}

/// Verifies escaping closure syntax has an owned NativeIR lowering.
#[test]
fn closures_have_explicit_native_lowering() {
    let closure = CoreExpr::Lam {
        params: vec![CorePattern::Var("value".to_string())],
        body: Box::new(CoreExpr::Var("value".to_string())),
    };

    assert_eq!(
        expression_coverage(&closure).disposition,
        LoweringDisposition::NativeLowered
    );
}

/// Verifies scalar case control and its compiler-owned pattern subset enter the
/// mandatory case-elimination pass.
#[test]
fn case_families_have_native_or_mandatory_scalar_lowering() {
    let case = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Int(1)),
        clauses: Vec::new(),
    };
    assert_eq!(
        expression_coverage(&case).disposition,
        LoweringDisposition::NativeLowered
    );
    for pattern in [
        CorePattern::Wildcard,
        CorePattern::Int(1),
        CorePattern::Float("1.5".to_string()),
        CorePattern::Atom("true".to_string()),
        CorePattern::Alias {
            alias: "value".to_string(),
            pattern: Box::new(CorePattern::Var("inner".to_string())),
        },
    ] {
        assert_eq!(
            pattern_coverage(&pattern).disposition,
            LoweringDisposition::CompilerRewrite
        );
    }
    assert_eq!(
        pattern_coverage(&CorePattern::Constructor {
            name: "Unit".to_string(),
            constructor_identity: None,
            args: Vec::new(),
        })
        .disposition,
        LoweringDisposition::NativeLowered
    );
}

/// Verifies checked named field reads use the native managed-operation ABI.
#[test]
fn managed_field_access_is_native_lowered() {
    for access in [
        CoreExpr::FieldAccess {
            base: Box::new(CoreExpr::Var("value".to_string())),
            field: "left".to_string(),
        },
        CoreExpr::RecordAccess {
            base: Box::new(CoreExpr::Var("value".to_string())),
            name: "Pair".to_string(),
            field: "left".to_string(),
        },
    ] {
        assert_eq!(
            expression_coverage(&access).disposition,
            LoweringDisposition::NativeLowered
        );
    }
}

/// Verifies checked named-record construction uses managed aggregate lowering.
#[test]
fn managed_record_construction_is_native_lowered() {
    let construction = CoreExpr::RecordConstruct {
        name: "Pair".to_string(),
        fields: vec![CoreRecordExprField {
            key: "left".to_string(),
            required: true,
            value: CoreExpr::Int(1),
        }],
    };
    assert_eq!(
        expression_coverage(&construction).disposition,
        LoweringDisposition::NativeLowered
    );
    let update = CoreExpr::RecordUpdate {
        base: Box::new(CoreExpr::Var("pair".to_string())),
        name: "Pair".to_string(),
        fields: vec![CoreRecordExprField {
            key: "left".to_string(),
            required: true,
            value: CoreExpr::Int(2),
        }],
    };
    assert_eq!(
        expression_coverage(&update).disposition,
        LoweringDisposition::NativeLowered
    );
}

/// Verifies native transition intrinsics and unavailable runtime capabilities
/// cannot be confused by the backend.
#[test]
fn intrinsic_families_have_explicit_lowering_dispositions() {
    assert_eq!(
        intrinsic_coverage(&CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::VmProcessYield
        ))
        .disposition,
        LoweringDisposition::NativeLowered
    );
    assert_eq!(
        intrinsic_coverage(&CoreIntrinsicId::Runtime(
            CoreRuntimeCapability::FileReadText
        ))
        .disposition,
        LoweringDisposition::NativeLowered
    );

    let expression = CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessReceiveInt),
        args: Vec::new(),
        return_type: CoreType::Int,
        effects: CoreEffectSet {
            effects: vec!["vm_effect_execution".to_string()],
        },
        span: Span::new(0, 0),
    });
    assert_eq!(
        expression_coverage(&expression).disposition,
        LoweringDisposition::NativeLowered
    );
    assert_eq!(
        intrinsic_coverage(&CoreIntrinsicId::VmProcessSendMessage(CoreType::Int)).disposition,
        LoweringDisposition::NativeLowered
    );
    assert_eq!(
        intrinsic_coverage(&CoreIntrinsicId::VmProcessReceiveMessage(CoreType::Int)).disposition,
        LoweringDisposition::NativeLowered
    );
    for intrinsic in [
        CoreIntrinsicId::VmProcessSpawn(CoreType::Int),
        CoreIntrinsicId::VmProcessLink(CoreType::Int),
        CoreIntrinsicId::VmProcessMonitor(CoreType::Int),
        CoreIntrinsicId::VmProcessAcquireResource(CoreType::Int),
        CoreIntrinsicId::VmProcessCancel(CoreType::Int),
    ] {
        assert_eq!(
            intrinsic_coverage(&intrinsic).disposition,
            LoweringDisposition::NativeLowered,
            "{intrinsic:?}"
        );
    }
}

/// Verifies parameter patterns and structured destructuring have explicit
/// native dispositions.
#[test]
fn pattern_families_have_explicit_lowering_dispositions() {
    assert_eq!(
        pattern_coverage(&CorePattern::Var("value".to_string())).disposition,
        LoweringDisposition::NativeLowered
    );
    assert_eq!(
        pattern_coverage(&CorePattern::Tuple(vec![CorePattern::Wildcard])).disposition,
        LoweringDisposition::NativeLowered
    );
}

/// Verifies unknown effect labels fail closed rather than inheriting a pure or
/// VM-transition disposition.
#[test]
fn unknown_effects_are_rejected() {
    assert_eq!(
        effect_coverage("ambient_network").diagnostic,
        Some("native_ir.effect.unknown")
    );
}
