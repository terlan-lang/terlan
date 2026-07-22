//! Internal native-call signature construction.

use cranelift_codegen::ir::{types, AbiParam, Signature};
use cranelift_codegen::isa::CallConv;

use super::RUNTIME_ARGUMENT_COUNT;

/// Builds one direct native function or continuation signature.
pub(super) fn native_signature(
    arity: usize,
    suspending: bool,
    transition_value_count: usize,
    pointer: cranelift_codegen::ir::Type,
) -> Signature {
    let mut params = vec![AbiParam::new(pointer); RUNTIME_ARGUMENT_COUNT];
    params.extend(std::iter::repeat_n(AbiParam::new(types::I64), arity));
    if transition_value_count > 0 {
        params.push(AbiParam::new(pointer));
    }
    if suspending {
        params.push(AbiParam::new(pointer));
    }
    Signature {
        params,
        returns: vec![AbiParam::new(types::I32), AbiParam::new(types::I64)],
        call_conv: CallConv::Fast,
    }
}
