use super::*;

pub(super) fn emit_expr(
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
        NativeExpr::ManagedLiteral { encoded } => managed::emit_managed_allocation(
            builder,
            module,
            managed_layouts,
            managed::ManagedAllocation {
                encoded_layout: encoded,
                fields: &[],
            },
            managed::ManagedAllocationRuntime {
                context: params[0],
                allocator: params[1],
            },
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
                managed::ManagedAllocation {
                    encoded_layout: encoded,
                    fields: &args,
                },
                managed::ManagedAllocationRuntime {
                    context: params[0],
                    allocator: params[1],
                },
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
                managed::ManagedAllocation {
                    encoded_layout: encoded,
                    fields: &captures,
                },
                managed::ManagedAllocationRuntime {
                    context: params[0],
                    allocator: params[1],
                },
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
                managed::ManagedAllocation {
                    encoded_layout,
                    fields: &fields,
                },
                managed::ManagedAllocationRuntime {
                    context: params[0],
                    allocator: params[1],
                },
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
            args.splice(
                0..0,
                params[..RUNTIME_ARGUMENT_COUNT].iter().copied(),
            );
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
                indirect::IndirectRuntimeValues {
                    context: params[0],
                    allocator: params[1],
                    resolver: params[2],
                    lookup: params[3],
                },
                indirect::IndirectInvocation {
                    closure: callee,
                    arguments: &args,
                    parameter_types,
                    result_type: *result_type,
                },
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
            if *operand_type == super::super::NativeType::Float {
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
                    Ok(emit_integer_comparison(builder, *operator, left, right))
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
            try_expr::TryExpressions {
                protected,
                success,
                failure,
                cleanup,
            },
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
        NativeExpr::InvokeClosureThen { .. } => Err(
            "error[cranelift.closure_call_then]: suspending closure continuation must terminate a native entry"
                .to_string(),
        ),
        NativeExpr::ContinuationTailCall { .. } => Err(
            "error[cranelift.continuation_sharing]: unresolved continuation call".to_string(),
        ),
    }
}
