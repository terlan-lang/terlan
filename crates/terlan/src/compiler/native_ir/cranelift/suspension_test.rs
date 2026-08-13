use super::*;
use crate::compiler::native_ir::{NativeCallResume, NativeDynamicCallResume, NativeType};

#[test]
fn call_then_reserves_the_direct_callee_transition_frame() {
    let body = NativeExpr::CallThen {
        function: 1,
        args: Vec::new(),
        resumes: vec![NativeCallResume {
            callee_continuation_id: 1,
            callee_capture_count: 2,
            continuation_id: 2,
            caller_value_start: 0,
        }],
        completion_continuation_id: 3,
        completion_function: Some(2),
        values: vec![NativeExpr::Unit],
    };

    assert_eq!(suspension_value_count(&body, &[0, 9, 4]), 9 + 1 + 2);
}

#[test]
fn closure_call_then_reserves_the_indirect_transition_frame() {
    let body = NativeExpr::InvokeClosureThen {
        callee: Box::new(NativeExpr::Unit),
        args: Vec::new(),
        parameter_types: Vec::new(),
        result_type: NativeType::Unit,
        resumes: vec![NativeDynamicCallResume {
            callee_export_id: 1,
            callee_continuation_id: 2,
            callee_capture_count: 3,
            continuation_id: 4,
        }],
        completion_continuation_id: 5,
        completion_function: None,
        values: vec![NativeExpr::Unit],
    };

    assert_eq!(
        suspension_value_count(&body, &[]),
        TVM_INDIRECT_TRANSITION_WORD_CAPACITY
    );
}
