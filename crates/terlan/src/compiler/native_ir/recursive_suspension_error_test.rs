//! Typed recursive-admission failures retain exact structural identities.

use super::*;
use crate::compiler::native_ir::NativeType;

fn function(body: NativeExpr) -> NativeFunction {
    NativeFunction {
        export_id: 1,
        name: "entry".into(),
        public: true,
        arity: 0,
        source_module: "fixture".into(),
        source_function: "entry".into(),
        source_arity: 0,
        callable_captures: Vec::new(),
        params: Vec::new(),
        return_type: NativeType::Int,
        body,
    }
}

fn module(body: NativeExpr) -> NativeModule {
    NativeModule {
        name: "fixture".into(),
        functions: vec![function(body)],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

fn returning_call() -> NativeExpr {
    NativeExpr::CallThen {
        function: 0,
        args: Vec::new(),
        resumes: Vec::new(),
        completion_continuation_id: 7,
        completion_function: None,
        values: Vec::new(),
    }
}

#[test]
fn invalid_call_target_is_a_structural_error_before_rewriting() {
    let mut modules = vec![module(NativeExpr::Call {
        function: 1,
        args: Vec::new(),
    })];
    let before = modules.clone();
    assert_eq!(
        defer_recursive_calls(&mut modules),
        Err(RecursiveSuspensionError::UnavailableCallTarget)
    );
    assert_eq!(modules, before);
}

#[test]
fn generated_identity_collision_is_rejected_before_rewriting() {
    let mut input = module(returning_call());
    let mut collision = function(NativeExpr::Int(0));
    collision.export_id = stable_export_id(MODULE, "$resume_1", 0);
    input.functions.push(collision);
    let mut modules = vec![input];
    let before = modules.clone();
    assert_eq!(
        defer_recursive_calls(&mut modules),
        Err(RecursiveSuspensionError::IdentityCollision)
    );
    assert_eq!(modules, before);
}

#[test]
fn unsupported_return_context_is_not_silently_admitted() {
    let mut modules = vec![module(NativeExpr::Neg(Box::new(returning_call())))];
    assert_eq!(
        defer_recursive_calls(&mut modules),
        Err(RecursiveSuspensionError::UnsupportedReturnContext)
    );
}

#[test]
fn missing_capture_layout_retains_its_continuation_identity() {
    let error = refresh_returning_calls(
        &mut returning_call(),
        &[HashSet::from([99])],
        &HashMap::new(),
        &HashMap::new(),
    )
    .unwrap_err();
    assert_eq!(error, RecursiveSuspensionError::MissingCaptureLayout(99));
    assert_eq!(
        error.to_string(),
        "error[native_ir.recursive_suspend_entry]: continuation 99 has no capture layout"
    );
}

#[test]
fn unavailable_callback_retains_its_export_identity() {
    let mut expression = NativeExpr::InvokeClosureThen {
        callee: Box::new(NativeExpr::Int(0)),
        args: Vec::new(),
        parameter_types: Vec::new(),
        result_type: NativeType::Int,
        resumes: vec![NativeDynamicCallResume {
            callee_export_id: 11,
            callee_continuation_id: 42,
            callee_capture_count: 0,
            continuation_id: 7,
        }],
        completion_continuation_id: 7,
        completion_function: None,
        values: Vec::new(),
    };
    let error = refresh_returning_calls(&mut expression, &[], &HashMap::new(), &HashMap::new())
        .unwrap_err();
    assert_eq!(error, RecursiveSuspensionError::UnavailableCallback(11));
    assert_eq!(
        error.to_string(),
        "error[native_ir.recursive_suspend_target]: unavailable callback 11"
    );
}
