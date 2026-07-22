//! Finite Float emission for the direct-AOT scalar profile.

use cranelift_codegen::ir::{
    condcodes::{FloatCC, IntCC},
    types, Block, InstBuilder, MemFlagsData, Value,
};
use cranelift_frontend::FunctionBuilder;

use super::super::{status, NativeBinaryOperator};
use super::branch_on_flag;

/// Emits one checked Float arithmetic or ordered-comparison expression.
pub(super) fn emit_float_binary(
    builder: &mut FunctionBuilder<'_>,
    operator: NativeBinaryOperator,
    left_bits: Value,
    right_bits: Value,
    error_block: Block,
) -> Result<Value, String> {
    let left = builder
        .ins()
        .bitcast(types::F64, MemFlagsData::new(), left_bits);
    let right = builder
        .ins()
        .bitcast(types::F64, MemFlagsData::new(), right_bits);
    match operator {
        NativeBinaryOperator::Add
        | NativeBinaryOperator::Subtract
        | NativeBinaryOperator::Multiply
        | NativeBinaryOperator::Divide => {
            if operator == NativeBinaryOperator::Divide {
                let magnitude_mask = builder.ins().iconst(types::I64, i64::MAX);
                let magnitude = builder.ins().band(right_bits, magnitude_mask);
                let zero = builder.ins().iconst(types::I64, 0);
                let is_zero = builder.ins().icmp(IntCC::Equal, magnitude, zero);
                branch_on_flag(
                    builder,
                    is_zero,
                    status::FLOAT_DIVISION_BY_ZERO,
                    error_block,
                );
            }
            let value = match operator {
                NativeBinaryOperator::Add => builder.ins().fadd(left, right),
                NativeBinaryOperator::Subtract => builder.ins().fsub(left, right),
                NativeBinaryOperator::Multiply => builder.ins().fmul(left, right),
                NativeBinaryOperator::Divide => builder.ins().fdiv(left, right),
                _ => unreachable!("float arithmetic operators matched above"),
            };
            let bits = builder
                .ins()
                .bitcast(types::I64, MemFlagsData::new(), value);
            let exponent_mask = builder
                .ins()
                .iconst(types::I64, 0x7ff0_0000_0000_0000_u64 as i64);
            let exponent = builder.ins().band(bits, exponent_mask);
            let non_finite = builder.ins().icmp(IntCC::Equal, exponent, exponent_mask);
            branch_on_flag(builder, non_finite, status::FLOAT_OVERFLOW, error_block);
            Ok(bits)
        }
        NativeBinaryOperator::Equal
        | NativeBinaryOperator::NotEqual
        | NativeBinaryOperator::LessThan
        | NativeBinaryOperator::LessThanOrEqual
        | NativeBinaryOperator::GreaterThan
        | NativeBinaryOperator::GreaterThanOrEqual => {
            let condition = match operator {
                NativeBinaryOperator::Equal => FloatCC::Equal,
                NativeBinaryOperator::NotEqual => FloatCC::NotEqual,
                NativeBinaryOperator::LessThan => FloatCC::LessThan,
                NativeBinaryOperator::LessThanOrEqual => FloatCC::LessThanOrEqual,
                NativeBinaryOperator::GreaterThan => FloatCC::GreaterThan,
                NativeBinaryOperator::GreaterThanOrEqual => FloatCC::GreaterThanOrEqual,
                _ => unreachable!("float comparison operators matched above"),
            };
            let comparison = builder.ins().fcmp(condition, left, right);
            Ok(builder.ins().uextend(types::I64, comparison))
        }
        NativeBinaryOperator::Remainder => Err(
            "error[cranelift.float_operator]: unsupported Float operator reached code generation"
                .to_string(),
        ),
    }
}
