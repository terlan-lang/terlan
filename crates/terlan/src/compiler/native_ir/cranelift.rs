mod call_then;
mod callables;
mod dispatch;
mod error;
mod float;
mod function;
mod image_entry;
mod indirect;
mod managed;
#[cfg(all(test, unix))]
#[path = "cranelift/managed_callback_test.rs"]
mod managed_callback_test;
#[cfg(test)]
#[path = "cranelift/managed_stack_map_test.rs"]
mod managed_stack_map_test;
#[cfg(test)]
#[path = "managed_type_test.rs"]
mod managed_type_test;
mod setup;
mod signature;
mod suspension;
mod transition;
mod try_expr;
mod units;

use cranelift_codegen::ir::{
    condcodes::IntCC, types, Block, BlockArg, InstBuilder, MemFlagsData, Value,
};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;

use super::symbol::native_symbol;
use super::{status, NativeBinaryOperator, NativeExpr, NativeModule};
use callables::validate_callable_shapes;
use dispatch::define_dispatch;
use error::{branch_if_error, branch_on_flag};
use float::emit_float_binary;
use function::define_native_function;
use image_entry::define_image_entry;
use managed::ManagedLayouts;
use setup::{
    application_functions, declare_image_func_in_func, flattened_application, object_module,
};
use signature::native_signature;
use suspension::{is_suspending, suspension_profile, suspension_value_count};
use transition::{transition_flags, transition_status};

const RUNTIME_ARGUMENT_COUNT: usize = 3;

pub(crate) use suspension::suspension_profile as native_suspension_profile;
pub(crate) use units::{
    emit_native_application_dispatch_object_with_policy, emit_native_module_object_with_policy,
    native_application_abi_fingerprint,
};

#[cfg(test)]
pub(crate) fn emit_native_application_object(
    application: &str,
    natives: &[NativeModule],
) -> Result<Vec<u8>, String> {
    emit_native_application_object_with_policy(
        application,
        natives,
        super::NativeCodegenPolicy::Development,
    )
}

/// Emits one complete application object under explicit optimization policy.
pub(crate) fn emit_native_application_object_with_policy(
    application: &str,
    natives: &[NativeModule],
    policy: super::NativeCodegenPolicy,
) -> Result<Vec<u8>, String> {
    if natives.is_empty() {
        return Err("error[cranelift.application]: native application has no modules".to_string());
    }
    validate_callable_shapes(natives)?;
    let mut module = object_module(application, policy)?;
    let managed_layouts = ManagedLayouts::declare(&mut module, natives)?;

    let pointer = module.target_config().pointer_type();
    let application_native = flattened_application(application, natives);
    let (function_suspending, function_transition_counts) = suspension_profile(&application_native);
    let application_functions = application_functions(natives);
    let signatures = application_functions
        .iter()
        .enumerate()
        .map(|(index, (_, function))| {
            native_signature(
                function.arity,
                function_suspending[index],
                function_transition_counts[index],
                pointer,
            )
        })
        .collect::<Vec<_>>();
    let function_ids = application_functions
        .iter()
        .enumerate()
        .map(|(index, (native, function))| {
            module
                .declare_function(
                    &native_symbol(&native.name, &function.name, function.arity),
                    Linkage::Local,
                    &signatures[index],
                )
                .map_err(|error| format!("error[cranelift.declare]: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut dispatch_functions = Vec::new();
    for (index, (_, function)) in application_functions.iter().enumerate() {
        define_native_function(
            &mut module,
            function_ids[index],
            &signatures[index],
            &function.body,
            &function_ids,
            &function_suspending,
            &function_transition_counts,
            &managed_layouts,
        )
        .map_err(|error| {
            format!(
                "{error}; while defining `{}.{}` at application index {index}",
                application_functions[index].0.name, function.name
            )
        })?;
        dispatch_functions.push((
            function.export_id,
            function.arity,
            function_ids[index],
            function_transition_counts[index],
            function_suspending[index],
        ));
    }
    for native in natives {
        for (index, continuation) in native.continuations.iter().enumerate() {
            let transition_value_count =
                suspension_value_count(&continuation.body, &function_transition_counts);
            let continuation_suspending = is_suspending(&continuation.body, &function_suspending);
            let signature = native_signature(
                continuation.params.len(),
                continuation_suspending,
                transition_value_count,
                pointer,
            );
            let id = module
                .declare_function(
                    &format!("terlan_continuation_{}_{}", continuation.id, index),
                    Linkage::Local,
                    &signature,
                )
                .map_err(|error| format!("error[cranelift.continuation_declare]: {error}"))?;
            define_native_function(
                &mut module,
                id,
                &signature,
                &continuation.body,
                &function_ids,
                &function_suspending,
                &function_transition_counts,
                &managed_layouts,
            )?;
            dispatch_functions.push((
                continuation.id,
                continuation.params.len(),
                id,
                transition_value_count,
                continuation_suspending,
            ));
        }
    }
    define_dispatch(&mut module, &dispatch_functions)?;
    define_image_entry(&mut module)?;

    module
        .finish()
        .emit()
        .map_err(|error| format!("error[cranelift.emit]: {error}"))
}

#[allow(clippy::too_many_arguments)]
fn emit_suspending_body(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    body: &NativeExpr,
    params: &[Value],
    transition_pointer: Option<Value>,
    transition_len_pointer: Option<Value>,
    function_ids: &[FuncId],
    function_suspending: &[bool],
    function_transition_counts: &[usize],
    managed_layouts: &ManagedLayouts,
    error_block: Block,
) -> Result<(), String> {
    match body {
        NativeExpr::Let { bindings, body } => {
            let mut locals = params.to_vec();
            for binding in bindings {
                let value = emit_expr(
                    builder,
                    module,
                    binding,
                    &locals,
                    function_ids,
                    managed_layouts,
                    error_block,
                )?;
                locals.push(value);
            }
            emit_suspending_body(
                builder,
                module,
                body,
                &locals,
                transition_pointer,
                transition_len_pointer,
                function_ids,
                function_suspending,
                function_transition_counts,
                managed_layouts,
                error_block,
            )
        }
        NativeExpr::Suspend {
            operation,
            arguments,
            continuation_id,
            values,
        } => {
            let transition_value_count = arguments.len().saturating_add(values.len());
            if transition_value_count != 0 {
                let pointer = transition_pointer.ok_or_else(|| {
                    "error[cranelift.suspend]: transition buffer is unavailable".to_string()
                })?;
                for (index, value) in arguments.iter().chain(values).enumerate() {
                    let captured = emit_expr(
                        builder,
                        module,
                        value,
                        params,
                        function_ids,
                        managed_layouts,
                        error_block,
                    )?;
                    let offset = i32::try_from(index.saturating_mul(8)).map_err(|_| {
                        "error[cranelift.suspend]: transition offset exceeds i32".to_string()
                    })?;
                    builder
                        .ins()
                        .store(MemFlagsData::new(), captured, pointer, offset);
                }
            }
            let transition_status = transition_status(*operation);
            let yielded = builder
                .ins()
                .iconst(types::I32, i64::from(transition_status));
            let continuation = builder.ins().iconst(types::I64, *continuation_id as i64);
            let value_count = builder
                .ins()
                .iconst(types::I64, transition_value_count as i64);
            let len_pointer = transition_len_pointer.ok_or_else(|| {
                "error[cranelift.suspend]: transition length output is unavailable".to_string()
            })?;
            builder
                .ins()
                .store(MemFlagsData::new(), value_count, len_pointer, 0);
            builder.ins().return_(&[yielded, continuation]);
            Ok(())
        }
        NativeExpr::CallThen {
            function,
            args,
            callee_continuation_id,
            callee_capture_count,
            continuation_id,
            completion_function,
            values,
            ..
        } => {
            if !function_suspending.get(*function).copied().unwrap_or(false) {
                return Err(format!(
                    "error[cranelift.call_then]: native function {function} is not suspending"
                ));
            }
            let function_id = function_ids.get(*function).copied().ok_or_else(|| {
                format!("error[cranelift.call_then]: native function {function} is unavailable")
            })?;
            let mut call_args = args
                .iter()
                .map(|arg| {
                    emit_expr(
                        builder,
                        module,
                        arg,
                        params,
                        function_ids,
                        managed_layouts,
                        error_block,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            call_args.splice(0..0, [params[0], params[1], params[2]]);
            if function_transition_counts
                .get(*function)
                .copied()
                .unwrap_or(0)
                > 0
            {
                call_args.push(transition_pointer.ok_or_else(|| {
                    "error[cranelift.call_then]: transition buffer is unavailable".to_string()
                })?);
            }
            let len_pointer = transition_len_pointer.ok_or_else(|| {
                "error[cranelift.call_then]: transition length output is unavailable".to_string()
            })?;
            call_args.push(len_pointer);
            let function_ref = declare_image_func_in_func(module, function_id, builder.func);
            let call = builder.ins().call(function_ref, &call_args);
            let results = builder.inst_results(call).to_vec();
            let call_status = results[0];
            let call_value = results[1];
            let completed = builder.create_block();
            let inspect_yield = builder.create_block();
            let succeeded =
                builder
                    .ins()
                    .icmp_imm(IntCC::Equal, call_status, i64::from(status::OK));
            builder
                .ins()
                .brif(succeeded, completed, &[], inspect_yield, &[]);

            builder.switch_to_block(completed);
            call_then::return_from_synchronous_completion(
                builder,
                module,
                *completion_function,
                values,
                call_value,
                params,
                transition_pointer,
                len_pointer,
                function_ids,
                function_suspending,
                function_transition_counts,
                managed_layouts,
                error_block,
            )?;

            builder.switch_to_block(inspect_yield);
            let wrap_yield = builder.create_block();
            let return_error = builder.create_block();
            let flags = transition_flags(builder, call_status);
            builder
                .ins()
                .brif(flags.transitioned, wrap_yield, &[], return_error, &[]);
            builder.switch_to_block(return_error);
            let error_args = [BlockArg::Value(call_status)];
            builder.ins().jump(error_block, &error_args);

            builder.switch_to_block(wrap_yield);
            let expected_id = builder
                .ins()
                .iconst(types::I64, *callee_continuation_id as i64);
            let unexpected_id = builder.ins().icmp(IntCC::NotEqual, call_value, expected_id);
            branch_on_flag(builder, unexpected_id, status::UNKNOWN_EXPORT, error_block);
            let actual_count = builder
                .ins()
                .load(types::I64, MemFlagsData::new(), len_pointer, 0);
            let expected_count = transition::expected_value_count(
                builder,
                &flags,
                actual_count,
                *callee_capture_count,
            );
            let unexpected_count =
                builder
                    .ins()
                    .icmp(IntCC::NotEqual, actual_count, expected_count);
            branch_on_flag(
                builder,
                unexpected_count,
                status::TRANSITION_CAPACITY,
                error_block,
            );
            if !values.is_empty() {
                let pointer = transition_pointer.ok_or_else(|| {
                    "error[cranelift.call_then]: transition buffer is unavailable".to_string()
                })?;
                let byte_offset = builder.ins().imul_imm(actual_count, 8);
                let append_pointer = builder.ins().iadd(pointer, byte_offset);
                for (index, value) in values.iter().enumerate() {
                    let captured = emit_expr(
                        builder,
                        module,
                        value,
                        params,
                        function_ids,
                        managed_layouts,
                        error_block,
                    )?;
                    let offset = i32::try_from(index.saturating_mul(8)).map_err(|_| {
                        "error[cranelift.call_then]: transition offset exceeds i32".to_string()
                    })?;
                    builder
                        .ins()
                        .store(MemFlagsData::new(), captured, append_pointer, offset);
                }
            }
            let value_count = builder.ins().iadd_imm(actual_count, values.len() as i64);
            builder
                .ins()
                .store(MemFlagsData::new(), value_count, len_pointer, 0);
            let wrapper = builder.ins().iconst(types::I64, *continuation_id as i64);
            builder.ins().return_(&[call_status, wrapper]);
            Ok(())
        }
        NativeExpr::InvokeClosure {
            callee,
            args,
            parameter_types,
            result_type,
        } => {
            let (callee, args) = indirect::emit_operands(callee, args, |operand| {
                emit_expr(
                    builder,
                    module,
                    operand,
                    params,
                    function_ids,
                    managed_layouts,
                    error_block,
                )
            })?;
            let transition_pointer = transition_pointer.ok_or_else(|| {
                "error[cranelift.closure_dispatch]: transition buffer is unavailable".to_string()
            })?;
            let transition_len_pointer = transition_len_pointer.ok_or_else(|| {
                "error[cranelift.closure_dispatch]: transition length output is unavailable"
                    .to_string()
            })?;
            let (status, value) = indirect::emit_suspending_invoke_closure(
                builder,
                module,
                params[0],
                params[1],
                params[2],
                callee,
                &args,
                parameter_types,
                *result_type,
                transition_pointer,
                transition_len_pointer,
                error_block,
            )?;
            builder.ins().return_(&[status, value]);
            Ok(())
        }
        NativeExpr::TailCall { function, args } => {
            let function_id = function_ids.get(*function).copied().ok_or_else(|| {
                format!("error[cranelift.tail_call]: native function {function} is unavailable")
            })?;
            let mut args = args
                .iter()
                .map(|arg| {
                    emit_expr(
                        builder,
                        module,
                        arg,
                        params,
                        function_ids,
                        managed_layouts,
                        error_block,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            args.splice(0..0, [params[0], params[1], params[2]]);
            let transition_value_count = function_transition_counts
                .get(*function)
                .copied()
                .unwrap_or(0);
            if transition_value_count > 0 {
                args.push(transition_pointer.ok_or_else(|| {
                    "error[cranelift.tail_call]: transition buffer is unavailable".to_string()
                })?);
            }
            if function_suspending.get(*function).copied().unwrap_or(false) {
                args.push(transition_len_pointer.ok_or_else(|| {
                    "error[cranelift.tail_call]: transition length output is unavailable"
                        .to_string()
                })?);
            }
            let function_ref = declare_image_func_in_func(module, function_id, builder.func);
            let call = builder.ins().call(function_ref, &args);
            let results = builder.inst_results(call).to_vec();
            builder.ins().return_(&results);
            Ok(())
        }
        NativeExpr::If { clauses } => {
            for (condition, body) in clauses {
                let condition = emit_expr(
                    builder,
                    module,
                    condition,
                    params,
                    function_ids,
                    managed_layouts,
                    error_block,
                )?;
                let selected = builder.create_block();
                let next = builder.create_block();
                let is_true = builder.ins().icmp_imm(IntCC::NotEqual, condition, 0);
                builder.ins().brif(is_true, selected, &[], next, &[]);
                builder.switch_to_block(selected);
                emit_suspending_body(
                    builder,
                    module,
                    body,
                    params,
                    transition_pointer,
                    transition_len_pointer,
                    function_ids,
                    function_suspending,
                    function_transition_counts,
                    managed_layouts,
                    error_block,
                )?;
                builder.switch_to_block(next);
            }
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(status::NO_MATCHING_BRANCH));
            let error = [BlockArg::Value(status)];
            builder.ins().jump(error_block, &error);
            Ok(())
        }
        _ => {
            let value = emit_expr(
                builder,
                module,
                body,
                params,
                function_ids,
                managed_layouts,
                error_block,
            )?;
            let ok = builder.ins().iconst(types::I32, i64::from(status::OK));
            builder.ins().return_(&[ok, value]);
            Ok(())
        }
    }
}

fn emit_expr(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    expr: &NativeExpr,
    params: &[Value],
    function_ids: &[FuncId],
    managed_layouts: &ManagedLayouts,
    error_block: Block,
) -> Result<Value, String> {
    match expr {
        NativeExpr::Unit => Ok(builder.ins().iconst(types::I64, 0)),
        NativeExpr::Int(value) => Ok(builder.ins().iconst(types::I64, *value)),
        NativeExpr::Float(value) => Ok(builder.ins().iconst(types::I64, *value as i64)),
        NativeExpr::Bool(value) => Ok(builder.ins().iconst(types::I64, i64::from(*value))),
        NativeExpr::AtomLiteral(identity) => Ok(builder
            .ins()
            .iconst(types::I64, managed_layouts.atom_word(identity)?)),
        NativeExpr::StringLiteral { encoded } => managed::emit_managed_allocation(
            builder,
            module,
            managed_layouts,
            encoded,
            &[],
            params[0],
            params[1],
            error_block,
        ),
        NativeExpr::ManagedOperation { encoded, args } => {
            let args = args
                .iter()
                .map(|argument| {
                    emit_expr(
                        builder,
                        module,
                        argument,
                        params,
                        function_ids,
                        managed_layouts,
                        error_block,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            managed::emit_managed_allocation(
                builder,
                module,
                managed_layouts,
                encoded,
                &args,
                params[0],
                params[1],
                error_block,
            )
        }
        NativeExpr::MakeClosure { encoded, captures } => {
            let captures = captures
                .iter()
                .map(|capture| {
                    emit_expr(
                        builder,
                        module,
                        capture,
                        params,
                        function_ids,
                        managed_layouts,
                        error_block,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            managed::emit_managed_allocation(
                builder,
                module,
                managed_layouts,
                encoded,
                &captures,
                params[0],
                params[1],
                error_block,
            )
        }
        NativeExpr::Param(index) => params
            .get(index.saturating_add(RUNTIME_ARGUMENT_COUNT))
            .copied()
            .ok_or_else(|| {
            format!("error[cranelift.param]: native parameter {index} is unavailable")
        }),
        NativeExpr::Construct {
            encoded_layout,
            fields,
            ..
        } => {
            let fields = fields
                .iter()
                .map(|field| {
                    emit_expr(
                        builder,
                        module,
                        field,
                        params,
                        function_ids,
                        managed_layouts,
                        error_block,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            managed::emit_managed_allocation(
                builder,
                module,
                managed_layouts,
                encoded_layout,
                &fields,
                params[0],
                params[1],
                error_block,
            )
        }
        NativeExpr::Call { function, args } => {
            let function_id = function_ids.get(*function).copied().ok_or_else(|| {
                format!("error[cranelift.call]: native function {function} is unavailable")
            })?;
            let mut args = args
                .iter()
                .map(|arg| {
                    emit_expr(
                        builder,
                        module,
                        arg,
                        params,
                        function_ids,
                        managed_layouts,
                        error_block,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            args.splice(0..0, [params[0], params[1], params[2]]);
            let function_ref =
                declare_image_func_in_func(module, function_id, builder.func);
            let call = builder.ins().call(function_ref, &args);
            let results = builder.inst_results(call).to_vec();
            let call_status = results[0];
            let value = results[1];
            branch_if_error(builder, call_status, error_block);
            Ok(value)
        }
        NativeExpr::InvokeClosure {
            callee,
            args,
            parameter_types,
            result_type,
        } => {
            let (callee, args) = indirect::emit_operands(callee, args, |operand| {
                emit_expr(
                    builder,
                    module,
                    operand,
                    params,
                    function_ids,
                    managed_layouts,
                    error_block,
                )
            })?;
            indirect::emit_invoke_closure(
                builder,
                module,
                params[0],
                params[1],
                params[2],
                callee,
                &args,
                parameter_types,
                *result_type,
                error_block,
            )
        }
        NativeExpr::Neg(operand) => {
            let operand = emit_expr(
                builder,
                module,
                operand,
                params,
                function_ids,
                managed_layouts,
                error_block,
            )?;
            let zero = builder.ins().iconst(types::I64, 0);
            let (value, overflow) = builder.ins().ssub_overflow(zero, operand);
            branch_on_flag(builder, overflow, status::OVERFLOW, error_block);
            Ok(value)
        }
        NativeExpr::FloatNeg(operand) => {
            let operand = emit_expr(
                builder,
                module,
                operand,
                params,
                function_ids,
                managed_layouts,
                error_block,
            )?;
            let sign = builder.ins().iconst(types::I64, i64::MIN);
            Ok(builder.ins().bxor(operand, sign))
        }
        NativeExpr::FloatFloor(operand) | NativeExpr::FloatCeil(operand) => {
            let value = emit_expr(
                builder,
                module,
                operand,
                params,
                function_ids,
                managed_layouts,
                error_block,
            )?;
            let value = builder
                .ins()
                .bitcast(types::F64, MemFlagsData::new(), value);
            let rounded = if matches!(expr, NativeExpr::FloatFloor(_)) {
                builder.ins().floor(value)
            } else {
                builder.ins().ceil(value)
            };
            Ok(builder
                .ins()
                .bitcast(types::I64, MemFlagsData::new(), rounded))
        }
        NativeExpr::IntToFloat(operand) => {
            let operand = emit_expr(
                builder,
                module,
                operand,
                params,
                function_ids,
                managed_layouts,
                error_block,
            )?;
            let value = builder.ins().fcvt_from_sint(types::F64, operand);
            Ok(builder
                .ins()
                .bitcast(types::I64, MemFlagsData::new(), value))
        }
        NativeExpr::Not(operand) => {
            let value = emit_expr(
                builder,
                module,
                operand,
                params,
                function_ids,
                managed_layouts,
                error_block,
            )?;
            let zero = builder.ins().iconst(types::I64, 0);
            let is_false = builder.ins().icmp(IntCC::Equal, value, zero);
            Ok(builder.ins().uextend(types::I64, is_false))
        }
        NativeExpr::Binary {
            operator,
            operand_type,
            left,
            right,
        } => {
            let left = emit_expr(
                builder,
                module,
                left,
                params,
                function_ids,
                managed_layouts,
                error_block,
            )?;
            let right = emit_expr(
                builder,
                module,
                right,
                params,
                function_ids,
                managed_layouts,
                error_block,
            )?;
            if *operand_type == super::NativeType::Float {
                return emit_float_binary(builder, *operator, left, right, error_block);
            }
            match operator {
                NativeBinaryOperator::Add => {
                    let (value, overflow) = builder.ins().sadd_overflow(left, right);
                    branch_on_flag(builder, overflow, status::OVERFLOW, error_block);
                    Ok(value)
                }
                NativeBinaryOperator::Subtract => {
                    let (value, overflow) = builder.ins().ssub_overflow(left, right);
                    branch_on_flag(builder, overflow, status::OVERFLOW, error_block);
                    Ok(value)
                }
                NativeBinaryOperator::Multiply => {
                    let (value, overflow) = builder.ins().smul_overflow(left, right);
                    branch_on_flag(builder, overflow, status::OVERFLOW, error_block);
                    Ok(value)
                }
                NativeBinaryOperator::Divide | NativeBinaryOperator::Remainder => {
                    let zero = builder.ins().iconst(types::I64, 0);
                    let is_zero = builder.ins().icmp(IntCC::Equal, right, zero);
                    branch_on_flag(builder, is_zero, status::DIVISION_BY_ZERO, error_block);
                    let minimum = builder.ins().iconst(types::I64, i64::MIN);
                    let negative_one = builder.ins().iconst(types::I64, -1);
                    let left_is_minimum = builder.ins().icmp(IntCC::Equal, left, minimum);
                    let right_is_negative_one =
                        builder.ins().icmp(IntCC::Equal, right, negative_one);
                    let overflows = builder.ins().band(left_is_minimum, right_is_negative_one);
                    branch_on_flag(builder, overflows, status::OVERFLOW, error_block);
                    Ok(if *operator == NativeBinaryOperator::Divide {
                        builder.ins().sdiv(left, right)
                    } else {
                        builder.ins().srem(left, right)
                    })
                }
                NativeBinaryOperator::Equal
                | NativeBinaryOperator::NotEqual
                | NativeBinaryOperator::LessThan
                | NativeBinaryOperator::LessThanOrEqual
                | NativeBinaryOperator::GreaterThan
                | NativeBinaryOperator::GreaterThanOrEqual => {
                    let condition = match operator {
                        NativeBinaryOperator::Equal => IntCC::Equal,
                        NativeBinaryOperator::NotEqual => IntCC::NotEqual,
                        NativeBinaryOperator::LessThan => IntCC::SignedLessThan,
                        NativeBinaryOperator::LessThanOrEqual => IntCC::SignedLessThanOrEqual,
                        NativeBinaryOperator::GreaterThan => IntCC::SignedGreaterThan,
                        NativeBinaryOperator::GreaterThanOrEqual => IntCC::SignedGreaterThanOrEqual,
                        _ => unreachable!("comparison operators matched above"),
                    };
                    let comparison = builder.ins().icmp(condition, left, right);
                    Ok(builder.ins().uextend(types::I64, comparison))
                }
            }
        }
        NativeExpr::Let { bindings, body } => {
            let mut locals = params.to_vec();
            for binding in bindings {
                let value =
                    emit_expr(
                        builder,
                        module,
                        binding,
                        &locals,
                        function_ids,
                        managed_layouts,
                        error_block,
                    )?;
                locals.push(value);
            }
            emit_expr(
                builder,
                module,
                body,
                &locals,
                function_ids,
                managed_layouts,
                error_block,
            )
        }
        NativeExpr::If { clauses } => {
            let merge = builder.create_block();
            builder.append_block_param(merge, types::I64);
            for (condition, body) in clauses {
                let condition = emit_expr(
                    builder,
                    module,
                    condition,
                    params,
                    function_ids,
                    managed_layouts,
                    error_block,
                )?;
                let selected = builder.create_block();
                let next = builder.create_block();
                let is_true = builder.ins().icmp_imm(IntCC::NotEqual, condition, 0);
                builder.ins().brif(is_true, selected, &[], next, &[]);
                builder.switch_to_block(selected);
                let value = emit_expr(
                    builder,
                    module,
                    body,
                    params,
                    function_ids,
                    managed_layouts,
                    error_block,
                )?;
                let result = [BlockArg::Value(value)];
                builder.ins().jump(merge, &result);
                builder.switch_to_block(next);
            }
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(status::NO_MATCHING_BRANCH));
            let error = [BlockArg::Value(status)];
            builder.ins().jump(error_block, &error);
            builder.switch_to_block(merge);
            Ok(builder.block_params(merge)[0])
        }
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => try_expr::emit_try(
            builder,
            module,
            protected,
            success,
            failure,
            cleanup,
            params,
            function_ids,
            managed_layouts,
            error_block,
        ),
        NativeExpr::Suspend { .. } => {
            Err("error[cranelift.suspend]: suspension must terminate a native entry".to_string())
        }
        NativeExpr::TailCall { .. } => Err(
            "error[cranelift.tail_call]: suspending tail call must terminate a native entry"
                .to_string(),
        ),
        NativeExpr::CallThen { .. } => Err(
            "error[cranelift.call_then]: suspending call continuation must terminate a native entry"
                .to_string(),
        ),
        NativeExpr::ContinuationTailCall { .. } => Err(
            "error[cranelift.continuation_sharing]: unresolved continuation call".to_string(),
        ),
    }
}
