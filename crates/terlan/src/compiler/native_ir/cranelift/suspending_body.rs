//! Emission of suspending native function bodies.

use super::*;

pub(super) fn emit_suspending_body(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    body: &NativeExpr,
    params: &[Value],
    catalog: NativeFunctionCatalog<'_>,
    tail: NativeTailFrame<'_>,
    transition: NativeTransitionFrame,
) -> Result<(), String> {
    let NativeFunctionCatalog {
        ids: function_ids,
        suspending: function_suspending,
        transition_counts: function_transition_counts,
        managed_returns: function_managed_returns,
        managed_layouts,
        ..
    } = catalog;
    let error_block = tail.error_block;
    let NativeTransitionFrame {
        pointer: transition_pointer,
        len_pointer: transition_len_pointer,
    } = transition;
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
            emit_suspending_body(builder, module, body, &locals, catalog, tail, transition)
        }
        NativeExpr::Suspend {
            operation,
            arguments,
            continuation_id,
            values,
        } => {
            let transition_value_count = arguments.len().saturating_add(values.len());
            if transition_value_count > TVM_COMPLETION_TRANSITION_WORD_CAPACITY {
                return Err(format!(
                    "error[cranelift.transition_capacity]: suspension requires {transition_value_count} words, exceeding the ABI-1 completion limit of {TVM_COMPLETION_TRANSITION_WORD_CAPACITY}"
                ));
            }
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
            resumes,
            completion_function,
            values,
            ..
        } => {
            let callee_suspending = function_suspending.get(*function).copied().unwrap_or(false);
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
            call_args.splice(0..0, params[..RUNTIME_ARGUMENT_COUNT].iter().copied());
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
            if callee_suspending {
                call_args.push(len_pointer);
            }
            let function_ref = declare_image_func_in_func(module, function_id, builder.func);
            let call = builder.ins().call(function_ref, &call_args);
            let results = builder.inst_results(call).to_vec();
            let call_status = results[0];
            let call_value = results[1];
            if !callee_suspending {
                if function_managed_returns
                    .get(*function)
                    .copied()
                    .unwrap_or(false)
                {
                    builder.declare_value_needs_stack_map(call_value);
                }
                let completed = builder.create_block();
                let succeeded =
                    builder
                        .ins()
                        .icmp_imm_s(IntCC::Equal, call_status, i64::from(status::OK));
                let error_args = [BlockArg::Value(call_status)];
                builder
                    .ins()
                    .brif(succeeded, completed, &[], error_block, &error_args);
                builder.switch_to_block(completed);
                return call_then::return_from_synchronous_completion(
                    builder,
                    module,
                    call_then::SynchronousCompletion {
                        function: *completion_function,
                        values,
                        call_value,
                    },
                    params,
                    NativeTransitionFrame {
                        pointer: transition_pointer,
                        len_pointer: Some(len_pointer),
                    },
                    catalog,
                    call_then::SynchronousControl { tail, error_block },
                );
            }
            let completed = builder.create_block();
            let inspect_yield = builder.create_block();
            let succeeded =
                builder
                    .ins()
                    .icmp_imm_s(IntCC::Equal, call_status, i64::from(status::OK));
            builder
                .ins()
                .brif(succeeded, completed, &[], inspect_yield, &[]);

            builder.switch_to_block(completed);
            call_then::return_from_synchronous_completion(
                builder,
                module,
                call_then::SynchronousCompletion {
                    function: *completion_function,
                    values,
                    call_value,
                },
                params,
                NativeTransitionFrame {
                    pointer: transition_pointer,
                    len_pointer: Some(len_pointer),
                },
                catalog,
                call_then::SynchronousControl { tail, error_block },
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
            if resumes.is_empty() {
                // An empty closed profile proves that this specialization has
                // no legal parked continuation. The shared recursive ABI may
                // still classify its callee as suspension-capable because a
                // different component entry can park. Trap an impossible
                // transition loudly without manufacturing a compatibility
                // resume identity.
                let failure = builder.ins().iconst(types::I32, i64::from(status::FAILURE));
                let error_args = [BlockArg::Value(failure)];
                builder.ins().jump(error_block, &error_args);
                return Ok(());
            }
            for (index, resume) in resumes.iter().enumerate() {
                let matched = builder.create_block();
                let last = index.saturating_add(1) == resumes.len();
                let expected_id = builder
                    .ins()
                    .iconst(types::I64, resume.callee_continuation_id as i64);
                let matches = builder.ins().icmp(IntCC::Equal, call_value, expected_id);
                if last {
                    let unknown = builder.ins().ireduce(types::I32, call_value);
                    let error_args = [BlockArg::Value(unknown)];
                    builder
                        .ins()
                        .brif(matches, matched, &[], error_block, &error_args);
                } else {
                    let next = builder.create_block();
                    builder.ins().brif(matches, matched, &[], next, &[]);
                    builder.switch_to_block(matched);
                    emit_wrapped_call_yield(
                        builder,
                        module,
                        WrappedCallYield {
                            call_status,
                            call_value,
                            transition: NativeTransitionFrame {
                                pointer: transition_pointer,
                                len_pointer: Some(len_pointer),
                            },
                            flags: &flags,
                            callee_capture_count: resume.callee_capture_count,
                            continuation_id: resume.continuation_id,
                            forward_callee_frame: resume.continuation_id
                                == resume.callee_continuation_id,
                            caller_value_start: resume.caller_value_start,
                            values,
                            params,
                            function_ids,
                            managed_layouts,
                            error_block,
                        },
                    )?;
                    builder.switch_to_block(next);
                    continue;
                }
                builder.switch_to_block(matched);
                emit_wrapped_call_yield(
                    builder,
                    module,
                    WrappedCallYield {
                        call_status,
                        call_value,
                        transition: NativeTransitionFrame {
                            pointer: transition_pointer,
                            len_pointer: Some(len_pointer),
                        },
                        flags: &flags,
                        callee_capture_count: resume.callee_capture_count,
                        continuation_id: resume.continuation_id,
                        forward_callee_frame: resume.continuation_id
                            == resume.callee_continuation_id,
                        caller_value_start: resume.caller_value_start,
                        values,
                        params,
                        function_ids,
                        managed_layouts,
                        error_block,
                    },
                )?;
            }
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
            let (status, value, _) = indirect::emit_suspending_invoke_closure(
                builder,
                module,
                indirect::IndirectRuntimeValues {
                    context: params[0],
                    allocator: params[1],
                    resolver: params[2],
                    lookup: params[3],
                    callable_recorder: params[4],
                },
                indirect::IndirectInvocation {
                    closure: callee,
                    arguments: &args,
                    parameter_types,
                    result_type: *result_type,
                },
                indirect::IndirectTransition {
                    pointer: Some(transition_pointer),
                    len_pointer: Some(transition_len_pointer),
                },
                error_block,
            )?;
            builder.ins().return_(&[status, value]);
            Ok(())
        }
        NativeExpr::InvokeClosureThen {
            callee,
            args,
            parameter_types,
            result_type,
            resumes,
            completion_function,
            values,
            ..
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
                "error[cranelift.closure_call_then]: transition buffer is unavailable".to_string()
            })?;
            let len_pointer = transition_len_pointer.ok_or_else(|| {
                "error[cranelift.closure_call_then]: transition length output is unavailable"
                    .to_string()
            })?;
            let (call_status, call_value, call_target) = indirect::emit_suspending_invoke_closure(
                builder,
                module,
                indirect::IndirectRuntimeValues {
                    context: params[0],
                    allocator: params[1],
                    resolver: params[2],
                    lookup: params[3],
                    callable_recorder: params[4],
                },
                indirect::IndirectInvocation {
                    closure: callee,
                    arguments: &args,
                    parameter_types,
                    result_type: *result_type,
                },
                indirect::IndirectTransition {
                    pointer: Some(transition_pointer),
                    len_pointer: Some(len_pointer),
                },
                error_block,
            )?;
            let completed = builder.create_block();
            let inspect_yield = builder.create_block();
            let succeeded =
                builder
                    .ins()
                    .icmp_imm_s(IntCC::Equal, call_status, i64::from(status::OK));
            builder
                .ins()
                .brif(succeeded, completed, &[], inspect_yield, &[]);

            builder.switch_to_block(completed);
            call_then::return_from_synchronous_completion(
                builder,
                module,
                call_then::SynchronousCompletion {
                    function: *completion_function,
                    values,
                    call_value,
                },
                params,
                NativeTransitionFrame {
                    pointer: Some(transition_pointer),
                    len_pointer: Some(len_pointer),
                },
                catalog,
                call_then::SynchronousControl { tail, error_block },
            )?;

            builder.switch_to_block(inspect_yield);
            let wrap_yield = builder.create_block();
            let return_error = builder.create_block();
            let flags = transition_flags(builder, call_status);
            builder
                .ins()
                .brif(flags.transitioned, wrap_yield, &[], return_error, &[]);
            builder.switch_to_block(return_error);
            builder
                .ins()
                .jump(error_block, &[BlockArg::Value(call_status)]);

            builder.switch_to_block(wrap_yield);
            if resumes.is_empty() {
                let unknown = builder
                    .ins()
                    .iconst(types::I32, i64::from(status::UNKNOWN_EXPORT));
                builder.ins().jump(error_block, &[BlockArg::Value(unknown)]);
                return Ok(());
            }
            for (index, resume) in resumes.iter().enumerate() {
                let matched = builder.create_block();
                let last = index.saturating_add(1) == resumes.len();
                let expected_id = builder
                    .ins()
                    .iconst(types::I64, resume.callee_continuation_id as i64);
                let continuation_matches =
                    builder.ins().icmp(IntCC::Equal, call_value, expected_id);
                let expected_target = builder
                    .ins()
                    .iconst(types::I64, resume.callee_export_id as i64);
                let target_matches = builder
                    .ins()
                    .icmp(IntCC::Equal, call_target, expected_target);
                let matches = builder.ins().band(continuation_matches, target_matches);
                if last {
                    let unknown = builder.ins().ireduce(types::I32, call_value);
                    builder.ins().brif(
                        matches,
                        matched,
                        &[],
                        error_block,
                        &[BlockArg::Value(unknown)],
                    );
                } else {
                    let next = builder.create_block();
                    builder.ins().brif(matches, matched, &[], next, &[]);
                    builder.switch_to_block(matched);
                    emit_wrapped_call_yield(
                        builder,
                        module,
                        WrappedCallYield {
                            call_status,
                            call_value,
                            transition: NativeTransitionFrame {
                                pointer: Some(transition_pointer),
                                len_pointer: Some(len_pointer),
                            },
                            flags: &flags,
                            callee_capture_count: resume.callee_capture_count,
                            continuation_id: resume.continuation_id,
                            forward_callee_frame: resume.continuation_id
                                == resume.callee_continuation_id,
                            caller_value_start: 0,
                            values,
                            params,
                            function_ids,
                            managed_layouts,
                            error_block,
                        },
                    )?;
                    builder.switch_to_block(next);
                    continue;
                }
                builder.switch_to_block(matched);
                emit_wrapped_call_yield(
                    builder,
                    module,
                    WrappedCallYield {
                        call_status,
                        call_value,
                        transition: NativeTransitionFrame {
                            pointer: Some(transition_pointer),
                            len_pointer: Some(len_pointer),
                        },
                        flags: &flags,
                        callee_capture_count: resume.callee_capture_count,
                        continuation_id: resume.continuation_id,
                        forward_callee_frame: resume.continuation_id
                            == resume.callee_continuation_id,
                        caller_value_start: 0,
                        values,
                        params,
                        function_ids,
                        managed_layouts,
                        error_block,
                    },
                )?;
            }
            Ok(())
        }
        NativeExpr::TailCall {
            function,
            args,
            yield_continuation_id,
        } => tail_call::emit_suspending_tail_call(
            builder,
            module,
            tail_call::SuspendingTailCall {
                function: *function,
                arguments: args,
                yield_continuation_id: *yield_continuation_id,
            },
            params,
            catalog,
            tail,
            transition,
        ),
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
                let is_true = builder.ins().icmp_imm_s(IntCC::NotEqual, condition, 0);
                builder.ins().brif(is_true, selected, &[], next, &[]);
                builder.switch_to_block(selected);
                emit_suspending_body(builder, module, body, params, catalog, tail, transition)?;
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
