//! Composed call-region construction and resume rewriting.

use super::*;

pub(in crate::compiler::native_ir) fn composed_call_region<F>(
    expr: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
    is_composable: &F,
    reserved: &HashSet<String>,
) -> Option<CallRegion>
where
    F: Fn(&str, usize) -> bool,
{
    let result_name = "$native_call_result".to_string();
    composed_call_region_at(expr, suspending, is_composable, &result_name, reserved)
}

fn composed_call_region_at<F>(
    expr: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
    is_composable: &F,
    result_name: &str,
    reserved: &HashSet<String>,
) -> Option<CallRegion>
where
    F: Fn(&str, usize) -> bool,
{
    match expr {
        CoreExpr::Call { function, args }
            if is_composable(function, args.len())
                && args.iter().all(|arg| {
                    !expr_calls_suspending(arg, suspending) && !contains_process_yield(arg)
                }) =>
        {
            Some(CallRegion {
                prefix: Vec::new(),
                target: CallTarget::Direct(function.clone()),
                args: args.clone(),
                resume: CoreExpr::Var(result_name.to_string()),
                result_name: result_name.to_string(),
                gates: Vec::new(),
                join: None,
            })
        }
        CoreExpr::FunctionCall { callee, args }
            if args.iter().all(|arg| {
                !expr_calls_suspending(arg, suspending) && !contains_process_yield(arg)
            }) =>
        {
            Some(CallRegion {
                prefix: Vec::new(),
                target: CallTarget::Dynamic(callee.clone()),
                args: args.clone(),
                resume: CoreExpr::Var(result_name.to_string()),
                result_name: result_name.to_string(),
                gates: Vec::new(),
                join: None,
            })
        }
        CoreExpr::Call { function, args }
            if (!suspending.contains(&(function.clone(), args.len()))
                || is_composable(function, args.len()))
                && !args.is_empty() =>
        {
            for (call_index, arg) in args.iter().enumerate() {
                let Some(mut region) =
                    composed_call_region_at(arg, suspending, is_composable, result_name, reserved)
                else {
                    if expr_calls_suspending(arg, suspending) || contains_process_yield(arg) {
                        return None;
                    }
                    continue;
                };
                let mut resumed_args = args.clone();
                let mut evaluated_prefix = Vec::with_capacity(call_index + region.prefix.len());
                for (index, earlier) in args[..call_index].iter().enumerate() {
                    let name = unique_prefix_name(
                        &format!("$native_call_arg_{index}"),
                        &region,
                        &evaluated_prefix,
                        reserved,
                    );
                    evaluated_prefix.push(CoreLetBinding {
                        pattern: CorePattern::Var(name.clone()),
                        value: earlier.clone(),
                    });
                    resumed_args[index] = CoreExpr::Var(name);
                }
                evaluated_prefix.append(&mut region.prefix);
                resumed_args[call_index] = region.resume.clone();
                region.prefix = evaluated_prefix;
                return Some(map_region_resumes(region, |resume| {
                    let mut args = resumed_args.clone();
                    args[call_index] = resume;
                    CoreExpr::Call {
                        function: function.clone(),
                        args,
                    }
                }));
            }
            None
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if !args.is_empty() => {
            for (call_index, arg) in args.iter().enumerate() {
                let Some(mut region) =
                    composed_call_region_at(arg, suspending, is_composable, result_name, reserved)
                else {
                    if expr_calls_suspending(arg, suspending) || contains_process_yield(arg) {
                        return None;
                    }
                    continue;
                };
                let mut resumed_args = args.clone();
                let mut evaluated_prefix = Vec::with_capacity(call_index + region.prefix.len());
                for (index, earlier) in args[..call_index].iter().enumerate() {
                    let name = unique_prefix_name(
                        &format!("$native_remote_arg_{index}"),
                        &region,
                        &evaluated_prefix,
                        reserved,
                    );
                    evaluated_prefix.push(CoreLetBinding {
                        pattern: CorePattern::Var(name.clone()),
                        value: earlier.clone(),
                    });
                    resumed_args[index] = CoreExpr::Var(name);
                }
                resumed_args[call_index] = region.resume.clone();
                evaluated_prefix.append(&mut region.prefix);
                region.prefix = evaluated_prefix;
                return Some(map_region_resumes(region, |resume| {
                    let mut args = resumed_args.clone();
                    args[call_index] = resume;
                    CoreExpr::RemoteCall {
                        module: module.clone(),
                        function: function.clone(),
                        args,
                    }
                }));
            }
            None
        }
        // Evaluate nested suspension-capable arguments before both ordinary
        // intrinsics and process transitions. The transition itself remains in
        // the resume expression and is lowered only after the argument call
        // completes, preserving left-to-right argument evaluation.
        CoreExpr::Intrinsic(call) if !call.args.is_empty() => {
            for (call_index, arg) in call.args.iter().enumerate() {
                let Some(mut region) =
                    composed_call_region_at(arg, suspending, is_composable, result_name, reserved)
                else {
                    if expr_calls_suspending(arg, suspending) || contains_process_yield(arg) {
                        return None;
                    }
                    continue;
                };
                let mut resumed_args = call.args.clone();
                let mut evaluated_prefix = Vec::with_capacity(call_index + region.prefix.len());
                for (index, earlier) in call.args[..call_index].iter().enumerate() {
                    let name = unique_prefix_name(
                        &format!("$native_intrinsic_arg_{index}"),
                        &region,
                        &evaluated_prefix,
                        reserved,
                    );
                    evaluated_prefix.push(CoreLetBinding {
                        pattern: CorePattern::Var(name.clone()),
                        value: earlier.clone(),
                    });
                    resumed_args[index] = CoreExpr::Var(name);
                }
                resumed_args[call_index] = region.resume.clone();
                evaluated_prefix.append(&mut region.prefix);
                region.prefix = evaluated_prefix;
                return Some(map_region_resumes(region, |resume| {
                    let mut resumed = call.clone();
                    let mut args = resumed_args.clone();
                    args[call_index] = resume;
                    resumed.args = args;
                    CoreExpr::Intrinsic(resumed)
                }));
            }
            None
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } if !args.is_empty() => {
            for (call_index, arg) in args.iter().enumerate() {
                let Some(mut region) =
                    composed_call_region_at(arg, suspending, is_composable, result_name, reserved)
                else {
                    if expr_calls_suspending(arg, suspending) || contains_process_yield(arg) {
                        return None;
                    }
                    continue;
                };
                let mut resumed_args = args.clone();
                let mut evaluated_prefix = Vec::with_capacity(call_index + region.prefix.len());
                for (index, earlier) in args[..call_index].iter().enumerate() {
                    let name = unique_prefix_name(
                        &format!("$native_constructor_arg_{index}"),
                        &region,
                        &evaluated_prefix,
                        reserved,
                    );
                    evaluated_prefix.push(CoreLetBinding {
                        pattern: CorePattern::Var(name.clone()),
                        value: earlier.clone(),
                    });
                    resumed_args[index] = CoreExpr::Var(name);
                }
                resumed_args[call_index] = region.resume.clone();
                evaluated_prefix.append(&mut region.prefix);
                region.prefix = evaluated_prefix;
                return Some(map_region_resumes(region, |resume| {
                    let mut args = resumed_args.clone();
                    args[call_index] = resume;
                    CoreExpr::ConstructorCall {
                        constructor: constructor.clone(),
                        constructor_identity: constructor_identity.clone(),
                        args,
                    }
                }));
            }
            None
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) if !items.is_empty() => {
            for (call_index, item) in items.iter().enumerate() {
                let Some(mut region) =
                    composed_call_region_at(item, suspending, is_composable, result_name, reserved)
                else {
                    if expr_calls_suspending(item, suspending) || contains_process_yield(item) {
                        return None;
                    }
                    continue;
                };
                let mut resumed_items = items.clone();
                let mut evaluated_prefix = Vec::with_capacity(call_index + region.prefix.len());
                for (index, earlier) in items[..call_index].iter().enumerate() {
                    let name = unique_prefix_name(
                        &format!("$native_sequence_item_{index}"),
                        &region,
                        &evaluated_prefix,
                        reserved,
                    );
                    evaluated_prefix.push(CoreLetBinding {
                        pattern: CorePattern::Var(name.clone()),
                        value: earlier.clone(),
                    });
                    resumed_items[index] = CoreExpr::Var(name);
                }
                resumed_items[call_index] = region.resume.clone();
                evaluated_prefix.append(&mut region.prefix);
                region.prefix = evaluated_prefix;
                let tuple = matches!(expr, CoreExpr::Tuple(_));
                return Some(map_region_resumes(region, |resume| {
                    let mut items = resumed_items.clone();
                    items[call_index] = resume;
                    if tuple {
                        CoreExpr::Tuple(items)
                    } else {
                        CoreExpr::List(items)
                    }
                }));
            }
            None
        }
        CoreExpr::RecordConstruct { name, fields } if !fields.is_empty() => {
            for (field_index, field) in fields.iter().enumerate() {
                let Some(mut region) = composed_call_region_at(
                    &field.value,
                    suspending,
                    is_composable,
                    result_name,
                    reserved,
                ) else {
                    if expr_calls_suspending(&field.value, suspending)
                        || contains_process_yield(&field.value)
                    {
                        return None;
                    }
                    continue;
                };
                let mut resumed_fields = fields.clone();
                let mut evaluated_prefix = Vec::with_capacity(field_index + region.prefix.len());
                for (index, earlier) in fields[..field_index].iter().enumerate() {
                    let local = unique_prefix_name(
                        &format!("$native_record_field_{index}"),
                        &region,
                        &evaluated_prefix,
                        reserved,
                    );
                    evaluated_prefix.push(CoreLetBinding {
                        pattern: CorePattern::Var(local.clone()),
                        value: earlier.value.clone(),
                    });
                    resumed_fields[index].value = CoreExpr::Var(local);
                }
                resumed_fields[field_index].value = region.resume.clone();
                evaluated_prefix.append(&mut region.prefix);
                region.prefix = evaluated_prefix;
                return Some(map_region_resumes(region, |resume| {
                    let mut fields = resumed_fields.clone();
                    fields[field_index].value = resume;
                    CoreExpr::RecordConstruct {
                        name: name.clone(),
                        fields,
                    }
                }));
            }
            None
        }
        CoreExpr::UnaryOp { operator, operand } => {
            let region =
                composed_call_region_at(operand, suspending, is_composable, result_name, reserved)?;
            Some(map_region_resumes(region, |resume| CoreExpr::UnaryOp {
                operator: operator.clone(),
                operand: Box::new(resume),
            }))
        }
        CoreExpr::Cast { expr, target_type } => {
            let region =
                composed_call_region_at(expr, suspending, is_composable, result_name, reserved)?;
            Some(map_region_resumes(region, |resume| CoreExpr::Cast {
                expr: Box::new(resume),
                target_type: target_type.clone(),
            }))
        }
        CoreExpr::ListCons { head, tail } => {
            if let Some(region) =
                composed_call_region_at(head, suspending, is_composable, result_name, reserved)
            {
                return Some(map_region_resumes(region, |resume| CoreExpr::ListCons {
                    head: Box::new(resume),
                    tail: tail.clone(),
                }));
            }
            if expr_calls_suspending(head, suspending) || contains_process_yield(head) {
                return None;
            }
            let mut region =
                composed_call_region_at(tail, suspending, is_composable, result_name, reserved)?;
            let head_name = unique_prefix_name("$native_list_head", &region, &[], reserved);
            region.prefix.insert(
                0,
                CoreLetBinding {
                    pattern: CorePattern::Var(head_name.clone()),
                    value: head.as_ref().clone(),
                },
            );
            Some(map_region_resumes(region, |resume| CoreExpr::ListCons {
                head: Box::new(CoreExpr::Var(head_name.clone())),
                tail: Box::new(resume),
            }))
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            if let Some(region) =
                composed_call_region_at(left, suspending, is_composable, result_name, reserved)
            {
                return Some(map_region_resumes(region, |resume| CoreExpr::BinaryOp {
                    operator: operator.clone(),
                    left: Box::new(resume),
                    right: right.clone(),
                }));
            }
            if expr_calls_suspending(left, suspending) || contains_process_yield(left) {
                return None;
            }
            let mut region =
                composed_call_region_at(right, suspending, is_composable, result_name, reserved)?;
            if matches!(operator.as_str(), "and" | "or") {
                let gated_prefix = std::mem::take(&mut region.prefix);
                let call_when_true = operator == "and";
                let bypass_resume =
                    CoreExpr::Atom(if operator == "or" { "true" } else { "false" }.to_string());
                if gated_prefix.is_empty()
                    && region.gates.first().is_some_and(|gate| {
                        gate.call_when_true == call_when_true && gate.bypass_resume == bypass_resume
                    })
                {
                    let gate = &mut region.gates[0];
                    gate.condition = CoreExpr::BinaryOp {
                        operator: operator.clone(),
                        left: left.clone(),
                        right: Box::new(gate.condition.clone()),
                    };
                    return Some(region);
                }
                region.gates.insert(
                    0,
                    CallGate {
                        condition: left.as_ref().clone(),
                        call_when_true,
                        prefix: gated_prefix,
                        bypass_resume,
                    },
                );
                return Some(region);
            }
            let left_name = unique_prefix_name("$native_call_left", &region, &[], reserved);
            let left_binding = CoreLetBinding {
                pattern: CorePattern::Var(left_name.clone()),
                value: left.as_ref().clone(),
            };
            region.prefix.insert(0, left_binding);
            Some(map_region_resumes(region, |resume| CoreExpr::BinaryOp {
                operator: operator.clone(),
                left: Box::new(CoreExpr::Var(left_name.clone())),
                right: Box::new(resume),
            }))
        }
        CoreExpr::If { clauses } if !clauses.is_empty() => {
            let (first, remaining) = clauses
                .split_first()
                .expect("nonempty conditional checked by match guard");
            if let Some(region) = composed_call_region_at(
                &first.condition,
                suspending,
                is_composable,
                result_name,
                reserved,
            ) {
                return Some(map_region_resumes(region, |resume| {
                    let mut clauses = clauses.clone();
                    clauses[0].condition = resume;
                    CoreExpr::If { clauses }
                }));
            }
            if expr_calls_suspending(&first.condition, suspending)
                || contains_process_yield(&first.condition)
            {
                return None;
            }
            if let Some(mut region) = composed_call_region_at(
                &first.body,
                suspending,
                is_composable,
                result_name,
                reserved,
            ) {
                if remaining.is_empty() {
                    if let Some(mut prefix) =
                        unconditional_true_prefix(&first.condition, suspending)
                    {
                        prefix.append(&mut region.prefix);
                        region.prefix = prefix;
                        return Some(region);
                    }
                }
                let gated_prefix = std::mem::take(&mut region.prefix);
                region.gates.insert(
                    0,
                    CallGate {
                        condition: first.condition.clone(),
                        call_when_true: true,
                        prefix: gated_prefix,
                        bypass_resume: CoreExpr::If {
                            clauses: remaining.to_vec(),
                        },
                    },
                );
                return Some(region);
            }
            if expr_calls_suspending(&first.body, suspending) || contains_process_yield(&first.body)
            {
                return None;
            }
            if remaining.is_empty() {
                return None;
            }
            let mut region = composed_call_region_at(
                &CoreExpr::If {
                    clauses: remaining.to_vec(),
                },
                suspending,
                is_composable,
                result_name,
                reserved,
            )?;
            let gated_prefix = std::mem::take(&mut region.prefix);
            region.gates.insert(
                0,
                CallGate {
                    condition: first.condition.clone(),
                    call_when_true: false,
                    prefix: gated_prefix,
                    bypass_resume: first.body.clone(),
                },
            );
            Some(region)
        }
        CoreExpr::Let { bindings, body } if !bindings.is_empty() => {
            for (binding_index, binding) in bindings.iter().enumerate() {
                let Some(mut region) = composed_call_region_at(
                    &binding.value,
                    suspending,
                    is_composable,
                    result_name,
                    reserved,
                ) else {
                    if expr_calls_suspending(&binding.value, suspending)
                        || contains_process_yield(&binding.value)
                    {
                        return None;
                    }
                    continue;
                };
                let mut evaluated_prefix = bindings[..binding_index].to_vec();
                evaluated_prefix.append(&mut region.prefix);
                let mut resumed_bindings = bindings[binding_index..].to_vec();
                resumed_bindings[0].value = region.resume.clone();
                region.prefix = evaluated_prefix;
                return Some(map_region_resumes(region, |resume| {
                    let mut bindings = resumed_bindings.clone();
                    bindings[0].value = resume;
                    CoreExpr::Let {
                        bindings,
                        body: body.clone(),
                    }
                }));
            }
            let mut region =
                composed_call_region_at(body, suspending, is_composable, result_name, reserved)?;
            let mut evaluated_prefix = bindings.clone();
            evaluated_prefix.append(&mut region.prefix);
            region.prefix = evaluated_prefix;
            Some(region)
        }
        CoreExpr::Case { scrutinee, clauses } => {
            let region = composed_call_region_at(
                scrutinee,
                suspending,
                is_composable,
                result_name,
                reserved,
            )?;
            Some(map_region_resumes(region, |resume| CoreExpr::Case {
                scrutinee: Box::new(resume),
                clauses: clauses.clone(),
            }))
        }
        _ => None,
    }
}

/// Applies one surrounding evaluation context to both the call result and its
/// short-circuit bypass result.
fn map_region_resumes(
    mut region: CallRegion,
    mut map: impl FnMut(CoreExpr) -> CoreExpr,
) -> CallRegion {
    if let Some(join) = &mut region.join {
        join.resume = map(join.resume.clone());
    } else if region.gates.is_empty() {
        region.resume = map(region.resume);
    } else {
        let result_name = unique_prefix_name("$native_gate_result", &region, &[], &HashSet::new());
        region.join = Some(CallJoin {
            result_name: result_name.clone(),
            resume: map(CoreExpr::Var(result_name)),
        });
    }
    region
}
