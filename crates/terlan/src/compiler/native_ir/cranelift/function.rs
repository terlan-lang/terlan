//! Definition of one local native function with its closed suspension ABI.

use cranelift_codegen::ir::{
    types, BlockArg, Function, InstBuilder, Signature, StackSlotData, StackSlotKind, UserFuncName,
    Value,
};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Module};
use cranelift_object::ObjectModule;

use super::super::suspension::{has_reduction_yield, is_suspending, suspension_value_count};
use super::super::{status, NativeExpr, NativeFunction, NativeModule, NativeType};
use super::setup::declare_image_func_in_func;
use super::{
    emit_expr, emit_suspending_body, NativeFunctionCatalog, NativeTailFrame, NativeTransitionFrame,
};

/// Context plus the allocator, closure resolver, and dispatch lookup services.
pub(super) const RUNTIME_ARGUMENT_COUNT: usize = 4;
/// Scalar recursive edges admitted before the actor cooperatively yields.
pub(super) const SCALAR_REDUCTIONS_PER_NATIVE_SLICE: i64 = 4_000;

/// Body, ABI, and tail-loop identity of one function definition.
pub(super) struct NativeFunctionDefinition<'a> {
    pub(super) id: FuncId,
    pub(super) self_function: Option<usize>,
    pub(super) tail_component_bodies: Option<&'a [(usize, usize, &'a NativeExpr)]>,
    pub(super) signature: &'a Signature,
    pub(super) body: &'a NativeExpr,
    pub(super) managed_loop_slots: &'a [bool],
}

pub(super) fn define_native_function(
    module: &mut ObjectModule,
    definition: NativeFunctionDefinition<'_>,
    catalog: NativeFunctionCatalog<'_>,
) -> super::super::NativeIrResult<()> {
    let NativeFunctionDefinition {
        id: function_id,
        self_function,
        tail_component_bodies,
        signature,
        body,
        managed_loop_slots,
    } = definition;
    let NativeFunctionCatalog {
        parameter_types,
        suspending: function_suspending,
        transition_counts: function_transition_counts,
        ..
    } = catalog;
    let mut context = Context::new();
    context.func = Function::with_name_signature(
        UserFuncName::user(0, function_id.as_u32()),
        signature.clone(),
    );
    let mut frontend_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend_context);
        let entry = builder.create_block();
        let loop_header = builder.create_block();
        let error = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        if let Some(component) = tail_component_bodies {
            for parameter in signature.params.iter().take(RUNTIME_ARGUMENT_COUNT) {
                builder.append_block_param(loop_header, parameter.value_type);
            }
            let component_members = component
                .iter()
                .map(|(function, arity, _)| (*function, *arity))
                .collect::<Vec<_>>();
            let (managed_width, scalar_width) =
                component_lane_widths(&component_members, parameter_types)?;
            for _ in 0..managed_width.saturating_add(scalar_width) {
                builder.append_block_param(loop_header, types::I64);
            }
            let first = component[0].0;
            let self_arity = component
                .iter()
                .find(|(function, _, _)| Some(*function) == self_function)
                .map(|(_, arity, _)| *arity)
                .unwrap_or_default();
            let mut tail_parameter = RUNTIME_ARGUMENT_COUNT.saturating_add(self_arity);
            if function_transition_counts[first] > 0 {
                builder
                    .append_block_param(loop_header, signature.params[tail_parameter].value_type);
                tail_parameter = tail_parameter.saturating_add(1);
            }
            if function_suspending[first] {
                builder
                    .append_block_param(loop_header, signature.params[tail_parameter].value_type);
            }
            builder.append_block_param(loop_header, types::I64);
        } else {
            for parameter in &signature.params {
                builder.append_block_param(loop_header, parameter.value_type);
            }
        }
        builder.append_block_param(error, types::I32);
        builder.switch_to_block(entry);
        let owns_reduction_yield = tail_component_bodies.map_or_else(
            || has_reduction_yield(body),
            |component| {
                component
                    .iter()
                    .any(|(_, _, body)| has_reduction_yield(body))
            },
        );
        let reduction_budget_slot = owns_reduction_yield.then(|| {
            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let budget = builder
                .ins()
                .iconst(types::I64, SCALAR_REDUCTIONS_PER_NATIVE_SLICE);
            builder.ins().stack_store(budget, slot, 0);
            slot
        });
        let entry_params = builder.block_params(entry).to_vec();
        let managed_pressure = managed_loop_slots.iter().any(|managed| *managed);
        let mut loop_args = entry_params
            .iter()
            .copied()
            .map(BlockArg::Value)
            .collect::<Vec<_>>();
        if let Some(component) = tail_component_bodies {
            let component_members = component
                .iter()
                .map(|(function, arity, _)| (*function, *arity))
                .collect::<Vec<_>>();
            let (managed_width, scalar_width) =
                component_lane_widths(&component_members, parameter_types)?;
            let first = component[0].0;
            let entry_arity = component
                .iter()
                .find(|(function, _, _)| Some(*function) == self_function)
                .map(|(_, arity, _)| *arity)
                .unwrap_or_default();
            let runtime = loop_args[..RUNTIME_ARGUMENT_COUNT].to_vec();
            let entry_values = entry_params
                [RUNTIME_ARGUMENT_COUNT..RUNTIME_ARGUMENT_COUNT.saturating_add(entry_arity)]
                .to_vec();
            let entry_function = self_function.ok_or_else(|| {
                "error[cranelift.tail_component]: entry function is unavailable".to_string()
            })?;
            let entry_types = parameter_types.get(entry_function).ok_or_else(|| {
                format!("error[cranelift.tail_component]: parameter types for function {entry_function} are unavailable")
            })?;
            let packed = pack_component_values(
                &mut builder,
                entry_values,
                entry_types,
                managed_width,
                scalar_width,
            )?;
            loop_args = runtime;
            loop_args.extend(packed.into_iter().map(BlockArg::Value));
            let tail_start = RUNTIME_ARGUMENT_COUNT.saturating_add(entry_arity);
            let tail_count = usize::from(function_transition_counts[first] > 0)
                .saturating_add(usize::from(function_suspending[first]));
            loop_args.extend(
                entry_params[tail_start..tail_start.saturating_add(tail_count)]
                    .iter()
                    .copied()
                    .map(BlockArg::Value),
            );
            loop_args.push(BlockArg::Value(
                builder
                    .ins()
                    .iconst(types::I64, self_function.unwrap_or_default() as i64),
            ));
        }
        builder.ins().jump(loop_header, &loop_args);
        builder.switch_to_block(loop_header);
        let params = builder.block_params(loop_header).to_vec();
        declare_managed_tail_roots(&mut builder, &params, managed_loop_slots)?;
        if is_suspending(body, function_suspending) {
            if let Some(component) = tail_component_bodies {
                emit_suspending_component_dispatch(
                    &mut builder,
                    module,
                    component,
                    &params,
                    catalog,
                    NativeTailFrame {
                        self_function,
                        component: None,
                        loop_header,
                        reduction_budget_slot,
                        managed_pressure,
                        error_block: error,
                    },
                )?;
            } else {
                let transition_value_count =
                    suspension_value_count(body, function_transition_counts);
                let transition_len_pointer = params.last().copied();
                let source_end = params.len() - 1 - usize::from(transition_value_count > 0);
                let source_params = &params[..source_end];
                let transition_pointer = (transition_value_count > 0).then(|| params[source_end]);
                emit_suspending_body(
                    &mut builder,
                    module,
                    body,
                    source_params,
                    catalog,
                    NativeTailFrame {
                        self_function,
                        component: None,
                        loop_header,
                        reduction_budget_slot,
                        managed_pressure,
                        error_block: error,
                    },
                    NativeTransitionFrame {
                        pointer: transition_pointer,
                        len_pointer: transition_len_pointer,
                    },
                )?;
            }
        } else {
            if let Some(component) = tail_component_bodies {
                emit_pure_component_dispatch(
                    &mut builder,
                    module,
                    component,
                    &params,
                    catalog,
                    NativeTailFrame {
                        self_function,
                        component: None,
                        loop_header,
                        reduction_budget_slot,
                        managed_pressure,
                        error_block: error,
                    },
                )?;
            } else {
                emit_pure_tail_body(
                    &mut builder,
                    module,
                    body,
                    &params,
                    catalog,
                    NativeTailFrame {
                        self_function,
                        component: None,
                        loop_header,
                        reduction_budget_slot,
                        managed_pressure,
                        error_block: error,
                    },
                )?;
            }
        }
        builder.switch_to_block(error);
        let error_status = builder.block_params(error)[0];
        let zero = builder.ins().iconst(types::I64, 0);
        builder.ins().return_(&[error_status, zero]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    Ok(module
        .define_function(function_id, &mut context)
        .map_err(|error| {
            format!(
                "error[cranelift.define]: function {}: {error}: {error:?}",
                function_id.as_u32()
            )
        })?)
}

/// Marks typed source-argument slots as precise relocating roots in the loop frame.
pub(super) fn declare_managed_tail_roots(
    builder: &mut FunctionBuilder<'_>,
    loop_params: &[Value],
    managed_slots: &[bool],
) -> super::super::NativeIrResult<()> {
    if loop_params.len() < RUNTIME_ARGUMENT_COUNT.saturating_add(managed_slots.len()) {
        return Err(format!(
            "error[cranelift.tail_roots]: loop exposes {} parameter(s), but {} typed argument slot(s) require root classification",
            loop_params.len(),
            managed_slots.len()
        ).into());
    }
    for (slot, managed) in managed_slots.iter().copied().enumerate() {
        if managed {
            builder.declare_value_needs_stack_map(loop_params[RUNTIME_ARGUMENT_COUNT + slot]);
        }
    }
    Ok(())
}

/// Classifies the source argument slots shared by one generated tail loop.
pub(super) fn managed_tail_loop_slots(
    own_params: &[NativeType],
    component: Option<&[usize]>,
    functions: &[(&NativeModule, &NativeFunction)],
) -> Vec<bool> {
    let Some(component) = component else {
        return own_params
            .iter()
            .map(|parameter| parameter.is_managed_reference())
            .collect();
    };
    let managed_width = component
        .iter()
        .map(|member| {
            functions[*member]
                .1
                .params
                .iter()
                .filter(|parameter| parameter.is_managed_reference())
                .count()
        })
        .max()
        .unwrap_or_default();
    let scalar_width = component
        .iter()
        .map(|member| {
            functions[*member]
                .1
                .params
                .iter()
                .filter(|parameter| !parameter.is_managed_reference())
                .count()
        })
        .max()
        .unwrap_or_default();
    std::iter::repeat_n(true, managed_width)
        .chain(std::iter::repeat_n(false, scalar_width))
        .collect()
}

pub(super) fn component_lane_widths(
    component: &[(usize, usize)],
    parameter_types: &[Vec<NativeType>],
) -> super::super::NativeIrResult<(usize, usize)> {
    let mut managed_width = 0;
    let mut scalar_width = 0;
    for (function, arity) in component {
        let types = parameter_types.get(*function).ok_or_else(|| {
            format!("error[cranelift.tail_component]: parameter types for function {function} are unavailable")
        })?;
        if types.len() != *arity {
            return Err(format!(
                "error[cranelift.tail_component_arity]: function {function} declares {arity} argument(s), found {} parameter type(s)",
                types.len()
            ).into());
        }
        managed_width = managed_width.max(
            types
                .iter()
                .filter(|parameter| parameter.is_managed_reference())
                .count(),
        );
        scalar_width = scalar_width.max(
            types
                .iter()
                .filter(|parameter| !parameter.is_managed_reference())
                .count(),
        );
    }
    Ok((managed_width, scalar_width))
}

pub(super) fn pack_component_values(
    builder: &mut FunctionBuilder<'_>,
    values: Vec<Value>,
    parameter_types: &[NativeType],
    managed_width: usize,
    scalar_width: usize,
) -> super::super::NativeIrResult<Vec<Value>> {
    if values.len() != parameter_types.len() {
        return Err(format!(
            "error[cranelift.tail_component_arity]: found {} value(s), expected {}",
            values.len(),
            parameter_types.len()
        )
        .into());
    }
    let mut managed = Vec::with_capacity(managed_width);
    let mut scalar = Vec::with_capacity(scalar_width);
    for (value, parameter) in values.into_iter().zip(parameter_types) {
        if parameter.is_managed_reference() {
            managed.push(value);
        } else {
            scalar.push(value);
        }
    }
    managed.resize_with(managed_width, || builder.ins().iconst(types::I64, 0));
    scalar.resize_with(scalar_width, || builder.ins().iconst(types::I64, 0));
    managed.extend(scalar);
    Ok(managed)
}

fn unpack_component_values(
    lane_values: &[Value],
    parameter_types: &[NativeType],
    managed_width: usize,
    scalar_width: usize,
) -> super::super::NativeIrResult<Vec<Value>> {
    if lane_values.len() < managed_width.saturating_add(scalar_width) {
        return Err(
            "error[cranelift.tail_component]: dispatcher lanes are truncated"
                .to_string()
                .into(),
        );
    }
    let mut managed = lane_values[..managed_width].iter().copied();
    let mut scalar = lane_values[managed_width..managed_width + scalar_width]
        .iter()
        .copied();
    parameter_types
        .iter()
        .map(|parameter| {
            if parameter.is_managed_reference() {
                managed.next()
            } else {
                scalar.next()
            }
            .ok_or_else(|| {
                "error[cranelift.tail_component]: dispatcher lane shape is invalid".to_string()
            })
        })
        .collect::<Result<Vec<_>, String>>()
        .map_err(Into::into)
}
fn emit_pure_tail_body(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    body: &NativeExpr,
    params: &[Value],
    catalog: NativeFunctionCatalog<'_>,
    tail: NativeTailFrame<'_>,
) -> super::super::NativeIrResult<()> {
    let NativeFunctionCatalog {
        ids: function_ids,
        parameter_types,
        suspending: function_suspending,
        managed_layouts,
        ..
    } = catalog;
    let NativeTailFrame {
        self_function,
        component: tail_component,
        loop_header,
        error_block,
        ..
    } = tail;
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
            emit_pure_tail_body(builder, module, body, &locals, catalog, tail)
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
                let is_true = builder.ins().icmp_imm(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                    condition,
                    0,
                );
                builder.ins().brif(is_true, selected, &[], next, &[]);
                builder.switch_to_block(selected);
                emit_pure_tail_body(builder, module, body, params, catalog, tail)?;
                builder.switch_to_block(next);
            }
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(status::NO_MATCHING_BRANCH));
            builder.ins().jump(error_block, &[BlockArg::Value(status)]);
            Ok(())
        }
        NativeExpr::TailCall { function, args, .. } => {
            let argument_values = args
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
            let mut values = argument_values.clone();
            values.splice(0..0, params[..RUNTIME_ARGUMENT_COUNT].iter().copied());
            let component_target = tail_component
                .and_then(|component| {
                    component
                        .iter()
                        .find(|(candidate, _)| candidate == function)
                })
                .copied();
            if let Some((_, target_arity)) = component_target {
                if args.len() != target_arity {
                    return Err(format!(
                        "error[cranelift.tail_component_arity]: function {function} expects {target_arity} argument(s), found {}",
                        args.len()
                    ).into());
                }
                let component = tail_component.unwrap_or_default();
                let (managed_width, scalar_width) =
                    component_lane_widths(component, parameter_types)?;
                let target_types = parameter_types.get(*function).ok_or_else(|| {
                    format!("error[cranelift.tail_component]: parameter types for function {function} are unavailable")
                })?;
                let packed = pack_component_values(
                    builder,
                    argument_values,
                    target_types,
                    managed_width,
                    scalar_width,
                )?;
                values.truncate(RUNTIME_ARGUMENT_COUNT);
                values.extend(packed);
                values.push(builder.ins().iconst(types::I64, *function as i64));
            }
            if component_target.is_some() || self_function == Some(*function) {
                if values.len() != builder.block_params(loop_header).len() {
                    return Err(format!(
                        "error[cranelift.tail_loop_arity]: self tail call has {} ABI argument(s), expected {}",
                        values.len(),
                        builder.block_params(loop_header).len()
                    ).into());
                }
                let values = values.into_iter().map(BlockArg::Value).collect::<Vec<_>>();
                builder.ins().jump(loop_header, &values);
                return Ok(());
            }
            if function_suspending.get(*function).copied().unwrap_or(false) {
                return Err(format!(
                    "error[cranelift.tail_call]: suspending function {function} entered pure lowering"
                ).into());
            }
            let function_id = function_ids.get(*function).copied().ok_or_else(|| {
                format!("error[cranelift.tail_call]: native function {function} is unavailable")
            })?;
            let function_ref = declare_image_func_in_func(module, function_id, builder.func);
            let call = builder.ins().call(function_ref, &values);
            let results = builder.inst_results(call).to_vec();
            builder.ins().return_(&results);
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
fn emit_pure_component_dispatch(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    component_bodies: &[(usize, usize, &NativeExpr)],
    loop_params: &[Value],
    catalog: NativeFunctionCatalog<'_>,
    tail: NativeTailFrame<'_>,
) -> super::super::NativeIrResult<()> {
    let NativeFunctionCatalog {
        parameter_types, ..
    } = catalog;
    let NativeTailFrame { error_block, .. } = tail;
    let (tag, params) = loop_params.split_last().ok_or_else(|| {
        "error[cranelift.tail_component]: dispatcher tag is unavailable".to_string()
    })?;
    let component = component_bodies
        .iter()
        .map(|(function, arity, _)| (*function, *arity))
        .collect::<Vec<_>>();
    let (managed_width, scalar_width) = component_lane_widths(&component, parameter_types)?;
    for (function, _arity, body) in component_bodies {
        let selected = builder.create_block();
        let next = builder.create_block();
        let matches = builder.ins().icmp_imm(
            cranelift_codegen::ir::condcodes::IntCC::Equal,
            *tag,
            *function as i64,
        );
        builder.ins().brif(matches, selected, &[], next, &[]);
        builder.switch_to_block(selected);
        let types = parameter_types.get(*function).ok_or_else(|| {
            format!("error[cranelift.tail_component]: parameter types for function {function} are unavailable")
        })?;
        let source = unpack_component_values(
            &params[RUNTIME_ARGUMENT_COUNT..],
            types,
            managed_width,
            scalar_width,
        )?;
        let mut body_params = params[..RUNTIME_ARGUMENT_COUNT].to_vec();
        body_params.extend(source);
        emit_pure_tail_body(
            builder,
            module,
            body,
            &body_params,
            catalog,
            NativeTailFrame {
                self_function: None,
                component: Some(&component),
                ..tail
            },
        )?;
        builder.switch_to_block(next);
    }
    let status = builder
        .ins()
        .iconst(types::I32, i64::from(status::NO_MATCHING_BRANCH));
    builder.ins().jump(error_block, &[BlockArg::Value(status)]);
    Ok(())
}
fn emit_suspending_component_dispatch(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    component_bodies: &[(usize, usize, &NativeExpr)],
    loop_params: &[Value],
    catalog: NativeFunctionCatalog<'_>,
    tail: NativeTailFrame<'_>,
) -> super::super::NativeIrResult<()> {
    let NativeFunctionCatalog {
        parameter_types,
        suspending: function_suspending,
        transition_counts: function_transition_counts,
        ..
    } = catalog;
    let NativeTailFrame { error_block, .. } = tail;
    let (tag, _) = loop_params.split_last().ok_or_else(|| {
        "error[cranelift.tail_component]: dispatcher tag is unavailable".to_string()
    })?;
    let component = component_bodies
        .iter()
        .map(|(function, arity, _)| (*function, *arity))
        .collect::<Vec<_>>();
    let (managed_width, scalar_width) = component_lane_widths(&component, parameter_types)?;
    let first = component[0].0;
    let transition_offset = RUNTIME_ARGUMENT_COUNT
        .saturating_add(managed_width)
        .saturating_add(scalar_width);
    let transition_pointer =
        (function_transition_counts[first] > 0).then(|| loop_params[transition_offset]);
    let transition_len_pointer = function_suspending[first].then(|| {
        loop_params
            [transition_offset.saturating_add(usize::from(function_transition_counts[first] > 0))]
    });
    for (function, _arity, body) in component_bodies {
        let selected = builder.create_block();
        let next = builder.create_block();
        let matches = builder.ins().icmp_imm(
            cranelift_codegen::ir::condcodes::IntCC::Equal,
            *tag,
            *function as i64,
        );
        builder.ins().brif(matches, selected, &[], next, &[]);
        builder.switch_to_block(selected);
        let types = parameter_types.get(*function).ok_or_else(|| {
            format!("error[cranelift.tail_component]: parameter types for function {function} are unavailable")
        })?;
        let source = unpack_component_values(
            &loop_params[RUNTIME_ARGUMENT_COUNT..],
            types,
            managed_width,
            scalar_width,
        )?;
        let mut body_params = loop_params[..RUNTIME_ARGUMENT_COUNT].to_vec();
        body_params.extend(source);
        super::emit_suspending_body(
            builder,
            module,
            body,
            &body_params,
            catalog,
            NativeTailFrame {
                self_function: None,
                component: Some(&component),
                ..tail
            },
            NativeTransitionFrame {
                pointer: transition_pointer,
                len_pointer: transition_len_pointer,
            },
        )?;
        builder.switch_to_block(next);
    }
    let status = builder
        .ins()
        .iconst(types::I32, i64::from(status::NO_MATCHING_BRANCH));
    builder.ins().jump(error_block, &[BlockArg::Value(status)]);
    Ok(())
}
