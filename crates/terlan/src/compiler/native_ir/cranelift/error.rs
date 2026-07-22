//! Shared error-edge emission for native functions.

use cranelift_codegen::ir::{condcodes::IntCC, types, Block, BlockArg, InstBuilder, Value};
use cranelift_frontend::FunctionBuilder;

use super::super::status;

/// Branches to the shared error block when a checked operation failed.
pub(super) fn branch_on_flag(
    builder: &mut FunctionBuilder<'_>,
    flag: Value,
    error_status: i32,
    error_block: Block,
) {
    let next = builder.create_block();
    let status_value = builder.ins().iconst(types::I32, i64::from(error_status));
    let error_args = [BlockArg::Value(status_value)];
    builder
        .ins()
        .brif(flag, error_block, &error_args, next, &[]);
    builder.switch_to_block(next);
}

/// Propagates a non-success status from one generated native call.
pub(super) fn branch_if_error(
    builder: &mut FunctionBuilder<'_>,
    call_status: Value,
    error_block: Block,
) {
    let failed = builder
        .ins()
        .icmp_imm(IntCC::NotEqual, call_status, i64::from(status::OK));
    let next = builder.create_block();
    let error_args = [BlockArg::Value(call_status)];
    builder
        .ins()
        .brif(failed, error_block, &error_args, next, &[]);
    builder.switch_to_block(next);
}
