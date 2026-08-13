use super::*;

#[derive(Clone)]
pub(in crate::compiler::native_ir) struct YieldRegion {
    pub(in crate::compiler::native_ir) prefix: Vec<CoreLetBinding>,
    pub(in crate::compiler::native_ir) operation: NativeTransitionOperation,
    pub(in crate::compiler::native_ir) arguments: Vec<CoreExpr>,
    pub(in crate::compiler::native_ir) result: Option<(String, NativeType)>,
    pub(in crate::compiler::native_ir) result_core_type: Option<CoreType>,
    pub(in crate::compiler::native_ir) resume: CoreExpr,
    pub(in crate::compiler::native_ir) source_span: Option<Span>,
}

pub(in crate::compiler::native_ir) struct LoweredYield {
    pub(in crate::compiler::native_ir) entry: NativeExpr,
    pub(in crate::compiler::native_ir) continuation_params: Vec<NativeType>,
    pub(in crate::compiler::native_ir) resume: CoreExpr,
    pub(in crate::compiler::native_ir) capture_names: Vec<String>,
    pub(in crate::compiler::native_ir) resume_names: Vec<String>,
    pub(in crate::compiler::native_ir) resume_vars: HashMap<String, usize>,
    pub(in crate::compiler::native_ir) resume_types: HashMap<String, NativeType>,
    pub(in crate::compiler::native_ir) resume_core_types: HashMap<String, CoreType>,
    pub(in crate::compiler::native_ir) source_span: Option<Span>,
}

pub(in crate::compiler::native_ir) fn yield_region(expr: &CoreExpr) -> Option<YieldRegion> {
    if let Some((operation, arguments, result_type)) = process_transition(expr) {
        let result = result_type.map(|ty| ("$native_transition_result".to_string(), ty));
        let resume = result.as_ref().map_or_else(
            || CoreExpr::Atom("Unit".to_string()),
            |(name, _)| CoreExpr::Var(name.clone()),
        );
        return Some(YieldRegion {
            prefix: Vec::new(),
            operation,
            arguments,
            result,
            result_core_type: transition_result_core_type(expr),
            resume,
            source_span: process_transition_span(expr),
        });
    }
    let mut prefix = Vec::new();
    let mut current = expr;
    loop {
        let CoreExpr::Let { bindings, body } = current else {
            return None;
        };
        for (index, binding) in bindings.iter().enumerate() {
            if let Some((operation, arguments, result_type)) = process_transition(&binding.value) {
                let result = result_type.and_then(|ty| match &binding.pattern {
                    CorePattern::Var(name) => Some((name.clone(), ty)),
                    _ => None,
                });
                if result_type.is_some() && result.is_none() {
                    return None;
                }
                let remaining = bindings[index + 1..].to_vec();
                let resume = if remaining.is_empty() {
                    body.as_ref().clone()
                } else {
                    CoreExpr::Let {
                        bindings: remaining,
                        body: body.clone(),
                    }
                };
                return Some(YieldRegion {
                    prefix,
                    operation,
                    arguments,
                    result,
                    result_core_type: transition_result_core_type(&binding.value),
                    resume,
                    source_span: process_transition_span(&binding.value),
                });
            }
            prefix.push(binding.clone());
        }
        current = body;
    }
}

/// Extracts a yield only when moving it to the condition boundary cannot move
/// an earlier scalar computation across the suspension point.
pub(in crate::compiler::native_ir) fn condition_yield_region(
    expr: &CoreExpr,
) -> Option<YieldRegion> {
    condition_yield_region_at_depth(expr, 0)
}

pub(in crate::compiler::native_ir) fn condition_yield_region_at_depth(
    expr: &CoreExpr,
    depth: usize,
) -> Option<YieldRegion> {
    if let Some(region) = yield_region(expr) {
        return Some(region);
    }
    match expr {
        CoreExpr::Call { function, args } if !args.is_empty() => {
            let (region, args) = eager_argument_yield(args, depth)?;
            Some(YieldRegion {
                resume: CoreExpr::Call {
                    function: function.clone(),
                    args,
                },
                ..region
            })
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } if !args.is_empty() => {
            let (region, args) = eager_argument_yield(args, depth)?;
            Some(YieldRegion {
                resume: CoreExpr::ConstructorCall {
                    constructor: constructor.clone(),
                    constructor_identity: constructor_identity.clone(),
                    args,
                },
                ..region
            })
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) if !items.is_empty() => {
            let (region, resumed) = eager_argument_yield(items, depth)?;
            let resume = if matches!(expr, CoreExpr::Tuple(_)) {
                CoreExpr::Tuple(resumed)
            } else {
                CoreExpr::List(resumed)
            };
            Some(YieldRegion { resume, ..region })
        }
        CoreExpr::UnaryOp { operator, operand } => {
            condition_yield_region_at_depth(operand, depth.saturating_add(1)).map(|region| {
                YieldRegion {
                    prefix: region.prefix,
                    operation: region.operation,
                    arguments: region.arguments,
                    result: region.result,
                    result_core_type: region.result_core_type,
                    resume: CoreExpr::UnaryOp {
                        operator: operator.clone(),
                        operand: Box::new(region.resume),
                    },
                    source_span: region.source_span,
                }
            })
        }
        CoreExpr::Cast { expr, target_type } => {
            condition_yield_region_at_depth(expr, depth.saturating_add(1)).map(|region| {
                YieldRegion {
                    prefix: region.prefix,
                    operation: region.operation,
                    arguments: region.arguments,
                    result: region.result,
                    result_core_type: region.result_core_type,
                    resume: CoreExpr::Cast {
                        expr: Box::new(region.resume),
                        target_type: target_type.clone(),
                    },
                    source_span: region.source_span,
                }
            })
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            if let Some(region) = condition_yield_region_at_depth(left, depth.saturating_add(1)) {
                return Some(YieldRegion {
                    prefix: region.prefix,
                    operation: region.operation,
                    arguments: region.arguments,
                    result: region.result,
                    result_core_type: region.result_core_type,
                    resume: CoreExpr::BinaryOp {
                        operator: operator.clone(),
                        left: Box::new(region.resume),
                        right: right.clone(),
                    },
                    source_span: region.source_span,
                });
            }
            if matches!(operator.as_str(), "and" | "or") || !expr_is_scalar(left) {
                return None;
            }
            let mut region = condition_yield_region_at_depth(right, depth.saturating_add(1))?;
            let left_name = fresh_eager_left_name(expr, &region.prefix, region.source_span, depth);
            region.prefix.insert(
                0,
                CoreLetBinding {
                    pattern: CorePattern::Var(left_name.clone()),
                    value: left.as_ref().clone(),
                },
            );
            region.resume = CoreExpr::BinaryOp {
                operator: operator.clone(),
                left: Box::new(CoreExpr::Var(left_name)),
                right: Box::new(region.resume),
            };
            Some(region)
        }
        CoreExpr::Case { scrutinee, clauses } => {
            condition_yield_region_at_depth(scrutinee, depth.saturating_add(1)).map(|region| {
                YieldRegion {
                    prefix: region.prefix,
                    operation: region.operation,
                    arguments: region.arguments,
                    result: region.result,
                    result_core_type: region.result_core_type,
                    resume: CoreExpr::Case {
                        scrutinee: Box::new(region.resume),
                        clauses: clauses.clone(),
                    },
                    source_span: region.source_span,
                }
            })
        }
        _ => None,
    }
}

/// Selects a compiler-owned eager operand binding that cannot shadow a value
/// captured by an earlier suspension in the same resumed expression.
fn fresh_eager_left_name(
    expr: &CoreExpr,
    prefix: &[CoreLetBinding],
    source_span: Option<Span>,
    depth: usize,
) -> String {
    let mut occupied = free_variables(expr);
    occupied.extend(prefix.iter().filter_map(|binding| match &binding.pattern {
        CorePattern::Var(name) => Some(name.clone()),
        _ => None,
    }));
    let base = source_span.map_or_else(
        || format!("$native_eager_left_{depth}"),
        |span| format!("$native_eager_left_{depth}_{}_{}", span.start, span.end),
    );
    if !occupied.contains(&base) {
        return base;
    }
    (1_usize..)
        .map(|ordinal| format!("{base}_{ordinal}"))
        .find(|candidate| !occupied.contains(candidate))
        .expect("unbounded eager temporary namespace has a free name")
}
/// Yield site and stable continuation identity selected by control lowering.
pub(in crate::compiler::native_ir) struct YieldRegionRequest<'a> {
    pub(in crate::compiler::native_ir) region: &'a YieldRegion,
    pub(in crate::compiler::native_ir) param_names: &'a [String],
    /// Values required by a completion outside this immediate resume.
    ///
    /// A nested process transition must retain these values even when its own
    /// resume does not mention them. Otherwise the next composed call sees a
    /// completion contract that its transition-only scope cannot satisfy.
    pub(in crate::compiler::native_ir) required_captures: &'a [String],
    pub(in crate::compiler::native_ir) continuation_id: u64,
}

/// Typed lexical scope and application lookup tables used by a yield region.
pub(in crate::compiler::native_ir) struct YieldRegionEnvironment<'a> {
    pub(in crate::compiler::native_ir) params: &'a HashMap<String, usize>,
    pub(in crate::compiler::native_ir) param_types: &'a HashMap<String, NativeType>,
    pub(in crate::compiler::native_ir) param_core_types: &'a HashMap<String, CoreType>,
    pub(in crate::compiler::native_ir) functions: &'a HashMap<(String, usize), usize>,
    pub(in crate::compiler::native_ir) function_types: &'a HashMap<(String, usize), NativeType>,
    pub(in crate::compiler::native_ir) function_core_types: &'a HashMap<(String, usize), CoreType>,
    pub(in crate::compiler::native_ir) constructors: &'a NativeConstructorLayouts,
}

pub(in crate::compiler::native_ir) fn lower_yield_region(
    request: YieldRegionRequest<'_>,
    environment: YieldRegionEnvironment<'_>,
) -> Result<LoweredYield, super::super::NativeIrError> {
    let YieldRegionRequest {
        region: requested_region,
        param_names,
        required_captures,
        continuation_id,
    } = request;
    let freshened_region = freshen_generated_prefix_names(requested_region, param_names);
    let region = &freshened_region;
    let YieldRegionEnvironment {
        params,
        param_types,
        param_core_types,
        functions,
        function_types,
        function_core_types,
        constructors,
    } = environment;
    let mut available_names = param_names.iter().cloned().collect::<HashSet<_>>();
    let mut prefix_names = Vec::with_capacity(region.prefix.len());
    for binding in &region.prefix {
        let CorePattern::Var(name) = &binding.pattern else {
            return Err(
                "error[native_ir.yield_pattern]: yield prefix requires scalar variable bindings"
                    .into(),
            );
        };
        if !available_names.insert(name.clone()) {
            return Err(format!(
                "error[native_ir.yield_shadow]: captured variable `{name}` shadows an existing scalar"
            )
            .into());
        }
        prefix_names.push(name.clone());
    }

    let capture_set = yield_capture_set(region, required_captures);
    let mut needed = capture_set.clone();
    for argument in &region.arguments {
        needed.extend(free_variables(argument));
    }
    let mut selected = vec![false; region.prefix.len()];
    for (index, binding) in region.prefix.iter().enumerate().rev() {
        let name = &prefix_names[index];
        if needed.contains(name) {
            selected[index] = true;
            needed.extend(free_variables(&binding.value));
        }
    }

    let mut entry_vars = params.clone();
    let mut entry_types = param_types.clone();
    let mut entry_core_types = param_core_types.clone();
    let mut entry_bindings = Vec::new();
    for (index, binding) in region.prefix.iter().enumerate() {
        if !selected[index] {
            continue;
        }
        let value_type = infer_native_type_with_constructors(
            &binding.value,
            &entry_types,
            function_types,
            constructors,
        )
        .or_else(|| {
            structured_case::core_expr_type(&binding.value, &entry_core_types, function_core_types)
                .and_then(|core_type| native_type(Some(&core_type), &core_type.contract_text()))
        })
        .ok_or_else(|| {
            format!(
                "error[native_ir.yield_type]: cannot infer captured scalar `{}`",
                prefix_names[index]
            )
        })?;
        let value = structured_case::lower_lexical_expr(
            &binding.value,
            &entry_vars,
            &entry_types,
            &entry_core_types,
            structured_case::StructuredCaseEnvironment {
                functions,
                function_types,
                function_core_types,
                constructors,
            },
        )?;
        entry_bindings.push(value);
        let value_index = params.len() + entry_bindings.len() - 1;
        entry_vars.insert(prefix_names[index].clone(), value_index);
        entry_types.insert(prefix_names[index].clone(), value_type);
        if let Some(core_type) =
            structured_case::core_expr_type(&binding.value, &entry_core_types, function_core_types)
        {
            entry_core_types.insert(prefix_names[index].clone(), core_type);
        }
    }

    let capture_names = param_names
        .iter()
        .chain(prefix_names.iter())
        .filter(|name| capture_set.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    if let Some(unknown) = capture_set
        .iter()
        .find(|name| !available_names.contains(*name))
    {
        return Err(format!(
            "error[native_ir.yield_capture]: resume references unavailable scalar `{unknown}`"
        )
        .into());
    }
    let values = capture_names
        .iter()
        .map(|name| {
            entry_vars
                .get(name)
                .copied()
                .map(NativeExpr::Param)
                .ok_or_else(|| {
                    format!("error[native_ir.yield_capture]: scalar `{name}` was not materialized")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut continuation_params = region.result.iter().map(|(_, ty)| *ty).collect::<Vec<_>>();
    continuation_params.extend(
        capture_names
            .iter()
            .map(|name| {
                entry_types.get(name).copied().ok_or_else(|| {
                    format!("error[native_ir.yield_type]: scalar `{name}` has no native type")
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    let result_offset = usize::from(region.result.is_some());
    let mut resume_vars = capture_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index + result_offset))
        .collect::<HashMap<_, _>>();
    if let Some((name, _)) = &region.result {
        resume_vars.insert(name.clone(), 0);
    }
    let operation_arguments = lower_transition_arguments(
        region,
        &entry_vars,
        &entry_types,
        &entry_core_types,
        structured_case::StructuredCaseEnvironment {
            functions,
            function_types,
            function_core_types,
            constructors,
        },
    )?;
    let suspend = NativeExpr::Suspend {
        operation: region.operation,
        arguments: operation_arguments,
        continuation_id,
        values,
    };
    let mut resume_types = capture_names
        .iter()
        .filter_map(|name| entry_types.get(name).copied().map(|ty| (name.clone(), ty)))
        .collect::<HashMap<_, _>>();
    if let Some((name, ty)) = &region.result {
        resume_types.insert(name.clone(), *ty);
    }
    let mut resume_core_types = capture_names
        .iter()
        .filter_map(|name| {
            entry_core_types
                .get(name)
                .cloned()
                .map(|ty| (name.clone(), ty))
        })
        .collect::<HashMap<_, _>>();
    if let (Some((name, _)), Some(core_type)) = (&region.result, &region.result_core_type) {
        resume_core_types.insert(name.clone(), core_type.clone());
    }
    let mut resume_names = region
        .result
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    resume_names.extend(capture_names.iter().cloned());
    Ok(LoweredYield {
        entry: if entry_bindings.is_empty() {
            suspend
        } else {
            NativeExpr::Let {
                bindings: entry_bindings,
                body: Box::new(suspend),
            }
        },
        continuation_params,
        resume: region.resume.clone(),
        capture_names,
        resume_names,
        resume_vars,
        resume_types,
        resume_core_types,
        source_span: region.source_span,
    })
}

/// Computes the complete lexical contract for a process-yield continuation.
/// Downstream completion frames are part of that contract even when the
/// immediate resume expression does not read their values itself.
pub(in crate::compiler::native_ir) fn yield_capture_set(
    region: &YieldRegion,
    required_captures: &[String],
) -> HashSet<String> {
    let mut captures = free_variables(&region.resume);
    if let Some((name, _)) = &region.result {
        captures.remove(name);
    }
    captures.extend(required_captures.iter().cloned());
    captures
}

/// Alpha-renames compiler-generated prefix locals that collide with captures
/// admitted by an earlier suspension in the same source function.
pub(super) fn freshen_generated_prefix_names(
    region: &YieldRegion,
    param_names: &[String],
) -> YieldRegion {
    let mut region = region.clone();
    let mut reserved = param_names.iter().cloned().collect::<HashSet<_>>();
    let mut renames = HashMap::new();
    for binding in &mut region.prefix {
        binding.value = super::super::static_callable::rename_free_variables(
            &binding.value,
            &renames,
            &mut HashSet::new(),
        );
        let CorePattern::Var(name) = &mut binding.pattern else {
            continue;
        };
        if reserved.contains(name) && name.starts_with("$native_") {
            let original = name.clone();
            let replacement = (1_usize..)
                .map(|ordinal| format!("{original}_{ordinal}"))
                .find(|candidate| !reserved.contains(candidate))
                .expect("unbounded compiler temporary namespace has a free name");
            *name = replacement.clone();
            renames.insert(original, replacement);
        }
        reserved.insert(name.clone());
    }
    region.arguments = region
        .arguments
        .iter()
        .map(|argument| {
            super::super::static_callable::rename_free_variables(
                argument,
                &renames,
                &mut HashSet::new(),
            )
        })
        .collect();
    region.resume = super::super::static_callable::rename_free_variables(
        &region.resume,
        &renames,
        &mut HashSet::new(),
    );
    region
}

fn transition_result_core_type(expr: &CoreExpr) -> Option<CoreType> {
    if let CoreExpr::SqlQuery {
        result_core_type, ..
    } = expr
    {
        return Some(result_core_type.clone());
    }
    let CoreExpr::Intrinsic(call) = expr else {
        return None;
    };
    match &call.return_type {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Message") && args.len() == 1 =>
        {
            args.first().cloned()
        }
        other => Some(other.clone()),
    }
}

/// Lowers fixed transition arguments and expands SQL parameters with their
/// exact boundary type metadata from the live lexical environment.
fn lower_transition_arguments(
    region: &YieldRegion,
    variables: &HashMap<String, usize>,
    variable_types: &HashMap<String, NativeType>,
    variable_core_types: &HashMap<String, CoreType>,
    environment: structured_case::StructuredCaseEnvironment<'_>,
) -> super::super::NativeIrResult<Vec<NativeExpr>> {
    let is_sql = region.operation == NativeTransitionOperation::Capability
        && matches!(region.arguments.first(), Some(CoreExpr::Int(tag)) if *tag == super::super::transitions::SQL_CAPABILITY_TAG);
    let lower = |argument: &CoreExpr| {
        structured_case::lower_lexical_expr(
            argument,
            variables,
            variable_types,
            variable_core_types,
            environment,
        )
    };
    if !is_sql {
        return region
            .arguments
            .iter()
            .map(lower)
            .collect::<Result<Vec<_>, String>>()
            .map_err(Into::into);
    }
    let prefix = super::super::transitions::SQL_CAPABILITY_PREFIX_WORDS;
    if region.arguments.len() < prefix {
        return Err(
            "error[native_ir.sql_capability_frame]: SQL transition prefix is truncated"
                .to_string()
                .into(),
        );
    }
    let parameter_count = region.arguments.len() - prefix;
    let declared_count = match &region.arguments[prefix - 1] {
        CoreExpr::Int(count) => usize::try_from(*count).ok(),
        _ => None,
    };
    if declared_count != Some(parameter_count) {
        return Err(
            "error[native_ir.sql_capability_frame]: SQL parameter count does not match payload"
                .to_string()
                .into(),
        );
    }
    let mut lowered = region.arguments[..prefix]
        .iter()
        .map(lower)
        .collect::<Result<Vec<_>, _>>()?;
    for (index, parameter) in region.arguments[prefix..].iter().enumerate() {
        let ty = infer_native_type_with_constructors(
            parameter,
            variable_types,
            environment.function_types,
            environment.constructors,
        )
        .or_else(|| {
            structured_case::core_expr_type(
                parameter,
                variable_core_types,
                environment.function_core_types,
            )
            .and_then(|core_type| native_type(Some(&core_type), &core_type.contract_text()))
        })
        .ok_or_else(|| {
            format!(
                "error[native_ir.sql_parameter_type]: cannot recover SQL parameter {} boundary type",
                index + 1
            )
        })?;
        lowered.extend(
            ty.boundary_type()
                .transition_words()
                .into_iter()
                .map(NativeExpr::Int),
        );
        lowered.push(lower(parameter)?);
    }
    Ok(lowered)
}

pub(super) fn process_transition_span(expr: &CoreExpr) -> Option<Span> {
    match expr {
        CoreExpr::Intrinsic(call) if call.span.start < call.span.end => Some(call.span),
        _ => None,
    }
}
