use super::*;

pub(super) fn wrap_bindings(bindings: Vec<NativeExpr>, body: NativeExpr) -> NativeExpr {
    if bindings.is_empty() {
        body
    } else {
        NativeExpr::Let {
            bindings,
            body: Box::new(body),
        }
    }
}

pub(super) fn phi_operand<'a>(
    instruction: &'a IrInstruction,
    predecessor: &str,
) -> Result<&'a str, String> {
    if instruction.operands.len() % 2 != 0 {
        return Err(adapter_error(
            "phi",
            "phi operands must alternate predecessor labels and values",
        ));
    }
    instruction
        .operands
        .chunks_exact(2)
        .find(|pair| pair[0] == predecessor)
        .map(|pair| pair[1].as_str())
        .ok_or_else(|| {
            adapter_error(
                "phi",
                format!("phi has no value for predecessor `{predecessor}`"),
            )
        })
}

pub(super) fn result_name<'a>(instruction: &'a IrInstruction) -> Result<&'a str, String> {
    instruction.result.as_deref().ok_or_else(|| {
        adapter_error(
            "instruction",
            format!("opcode `{}` has no result", instruction.opcode),
        )
    })
}

pub(super) fn bind_block_instructions(
    block: &IrBlock,
    predecessor: Option<&str>,
    module_name: &str,
    values: &mut ValueCatalog,
    functions: &FunctionCatalog,
    actor_sites: &[super::super::self_host_actor_adapter::NativeActorSuspensionSite],
    atoms: &mut Vec<String>,
) -> Result<Vec<NativeExpr>, String> {
    let mut bindings = Vec::with_capacity(block.instructions.len());
    let mut saw_non_phi = false;
    for instruction in &block.instructions {
        let result = result_name(instruction)?;
        if values.contains_key(result) {
            return Err(adapter_error(
                "instruction",
                format!("result `{result}` already has a producer"),
            ));
        }
        let (mut expression, result_type) = if instruction.opcode == "phi" {
            if saw_non_phi {
                return Err(adapter_error(
                    "phi",
                    "phi instructions must lead their block",
                ));
            }
            let predecessor = predecessor.ok_or_else(|| {
                adapter_error("phi", "entry block cannot contain a phi instruction")
            })?;
            let source = phi_operand(instruction, predecessor)?;
            let (expression, source_type) = value_operand(source, values)?;
            let result_type = native_type(&instruction.type_name)?;
            if source_type != result_type {
                return Err(adapter_error(
                    "phi",
                    format!("phi result `{result}` changes its incoming value type"),
                ));
            }
            (expression, result_type)
        } else {
            saw_non_phi = true;
            instruction_expression(instruction, module_name, values, functions, atoms)?
        };
        let matching_sites = actor_sites
            .iter()
            .filter(|site| site.block_label == block.label && site.result_name == result)
            .collect::<Vec<_>>();
        if matching_sites.len() > 1 {
            return Err(adapter_error(
                "actor-site",
                format!(
                    "block `{}` result `{result}` has duplicate suspension sites",
                    block.label
                ),
            ));
        }
        if let Some(site) = matching_sites.first() {
            let captures = site
                .value_names
                .iter()
                .map(|name| value_operand(name, values).map(|(value, _)| value))
                .collect::<Result<Vec<_>, _>>()?;
            expression = super::super::self_host_actor_adapter::apply_suspension_site(
                expression, site, captures,
            )?;
        }
        let index = values.len();
        values.insert(result.to_owned(), (index, result_type));
        bindings.push(expression);
    }
    Ok(bindings)
}

pub(super) fn block_named<'a>(blocks: &'a [IrBlock], label: &str) -> Result<&'a IrBlock, String> {
    blocks
        .iter()
        .find(|block| block.label == label)
        .ok_or_else(|| adapter_error("control-flow", format!("block `{label}` does not exist")))
}

pub(super) fn returned_value(
    block: &IrBlock,
    values: &ValueCatalog,
    return_type: NativeType,
) -> Result<NativeExpr, String> {
    match block.terminator.operands.as_slice() {
        [] if return_type == NativeType::Unit => Ok(NativeExpr::Unit),
        [value] => {
            let (expression, value_type) = value_operand(value, values)?;
            if value_type != return_type {
                return Err(adapter_error(
                    "return",
                    format!("block `{}` returns a value of the wrong type", block.label),
                ));
            }
            Ok(expression)
        }
        _ => Err(adapter_error(
            "return",
            format!("block `{}` has an invalid return operand", block.label),
        )),
    }
}

pub(super) fn lower_block_path(
    label: &str,
    predecessor: Option<&str>,
    blocks: &[IrBlock],
    module_name: &str,
    mut values: ValueCatalog,
    functions: &FunctionCatalog,
    actor_sites: &[super::super::self_host_actor_adapter::NativeActorSuspensionSite],
    return_type: NativeType,
    atoms: &mut Vec<String>,
    path: &HashSet<String>,
    expansions: &mut usize,
) -> Result<Option<NativeExpr>, String> {
    *expansions = expansions
        .checked_add(1)
        .ok_or_else(|| adapter_error("resource", "CFG expansion count overflow"))?;
    if *expansions > MAX_CFG_EXPANSIONS {
        return Err(adapter_error(
            "resource",
            format!("CFG exceeds the {MAX_CFG_EXPANSIONS} expansion bound"),
        ));
    }
    if path.contains(label) {
        return Err(adapter_error(
            "control-flow",
            format!("cyclic CFG reaches block `{label}`; loop structuring is not implemented"),
        ));
    }
    let mut next_path = path.clone();
    next_path.insert(label.to_owned());
    let block = block_named(blocks, label)?;
    let bindings = bind_block_instructions(
        block,
        predecessor,
        module_name,
        &mut values,
        functions,
        actor_sites,
        atoms,
    )?;
    if actor_sites
        .iter()
        .any(|site| site.block_label == block.label)
    {
        let mut bindings = bindings;
        let terminal = bindings.pop().ok_or_else(|| {
            adapter_error(
                "actor-site",
                format!(
                    "split suspension block `{}` has no call binding",
                    block.label
                ),
            )
        })?;
        return Ok(Some(wrap_bindings(bindings, terminal)));
    }
    let body = match block.terminator.opcode.as_str() {
        "return" => Some(returned_value(block, &values, return_type)?),
        "unreachable" => None,
        "jump" => match block.terminator.targets.as_slice() {
            [target] => lower_block_path(
                target,
                Some(&block.label),
                blocks,
                module_name,
                values,
                functions,
                actor_sites,
                return_type,
                atoms,
                &next_path,
                expansions,
            )?,
            _ => {
                return Err(adapter_error(
                    "control-flow",
                    format!("jump in block `{}` does not have one target", block.label),
                ))
            }
        },
        "branch" => {
            let condition_name = match block.terminator.operands.as_slice() {
                [condition] => condition,
                _ => {
                    return Err(adapter_error(
                        "control-flow",
                        format!(
                            "branch in block `{}` does not have one condition",
                            block.label
                        ),
                    ))
                }
            };
            let (condition, condition_type) = value_operand(condition_name, &values)?;
            if condition_type != NativeType::Bool {
                return Err(adapter_error(
                    "control-flow",
                    format!(
                        "branch in block `{}` does not use a Bool condition",
                        block.label
                    ),
                ));
            }
            let (true_target, false_target) = match block.terminator.targets.as_slice() {
                [true_target, false_target] => (true_target, false_target),
                _ => {
                    return Err(adapter_error(
                        "control-flow",
                        format!(
                            "branch in block `{}` does not have two targets",
                            block.label
                        ),
                    ))
                }
            };
            let true_body = lower_block_path(
                true_target,
                Some(&block.label),
                blocks,
                module_name,
                values.clone(),
                functions,
                actor_sites,
                return_type,
                atoms,
                &next_path,
                expansions,
            )?;
            let false_body = lower_block_path(
                false_target,
                Some(&block.label),
                blocks,
                module_name,
                values,
                functions,
                actor_sites,
                return_type,
                atoms,
                &next_path,
                expansions,
            )?;
            match (true_body, false_body) {
                (Some(selected), Some(fallback)) => Some(NativeExpr::If {
                    clauses: vec![(condition, selected), (NativeExpr::Bool(true), fallback)],
                }),
                (Some(selected), None) => Some(NativeExpr::If {
                    clauses: vec![(condition, selected)],
                }),
                (None, Some(fallback)) => Some(NativeExpr::If {
                    clauses: vec![(NativeExpr::Not(Box::new(condition)), fallback)],
                }),
                (None, None) => None,
            }
        }
        opcode => {
            return Err(adapter_error(
                "control-flow",
                format!("terminator `{opcode}` is outside acyclic CFG lowering"),
            ))
        }
    };
    Ok(body.map(|expression| wrap_bindings(bindings, expression)))
}

pub(super) fn native_function(
    module_name: &str,
    function: &IrFunction,
    params: Vec<NativeType>,
    return_type: NativeType,
    body: NativeExpr,
) -> NativeFunction {
    NativeFunction {
        export_id: stable_export_id(module_name, &function.name, function.arity),
        name: function.name.clone(),
        public: true,
        arity: function.arity,
        source_module: module_name.to_owned(),
        source_function: function.name.clone(),
        source_arity: function.arity,
        callable_captures: params[..function.capture_count].to_vec(),
        params,
        return_type,
        body,
    }
}

pub(super) fn lower_cfg_function(
    module_name: &str,
    function: &IrFunction,
    functions: &FunctionCatalog,
    actor_sites: &[super::super::self_host_actor_adapter::NativeActorSuspensionSite],
    atoms: &mut Vec<String>,
) -> Result<NativeFunction, String> {
    let params = function
        .parameters
        .iter()
        .map(|parameter| native_type(&parameter.type_name))
        .collect::<Result<Vec<_>, _>>()?;
    if params.len() != function.arity {
        return Err(adapter_error(
            "arity",
            format!(
                "`{}.{}` parameter count does not match arity",
                module_name, function.name
            ),
        ));
    }
    let values = function
        .parameters
        .iter()
        .zip(params.iter().copied())
        .enumerate()
        .map(|(index, (parameter, value_type))| (parameter.name.clone(), (index, value_type)))
        .collect::<ValueCatalog>();
    let return_type = native_type(&function.return_type)?;
    let mut expansions = 0;
    let body = lower_block_path(
        "entry",
        None,
        &function.blocks,
        module_name,
        values,
        functions,
        actor_sites,
        return_type,
        atoms,
        &HashSet::new(),
        &mut expansions,
    )?
    .ok_or_else(|| {
        adapter_error(
            "control-flow",
            format!(
                "`{}.{}` has no returning control-flow path",
                module_name, function.name
            ),
        )
    })?;
    Ok(native_function(
        module_name,
        function,
        params,
        return_type,
        body,
    ))
}

pub(super) fn lower_function_with_actor_sites(
    module_name: &str,
    function: &IrFunction,
    functions: &FunctionCatalog,
    actor_sites: &[super::super::self_host_actor_adapter::NativeActorSuspensionSite],
    atoms: &mut Vec<String>,
) -> Result<NativeFunction, String> {
    validate_actor_sites(function, actor_sites)?;
    if function.receiver_command && !function.receiver_mutable {
        return Err(adapter_error(
            "receiver",
            format!(
                "`{}.{}` is a receiver command without a mutable receiver",
                module_name, function.name
            ),
        ));
    }
    if function.receiver_mutable && function.parameters.is_empty() {
        return Err(adapter_error(
            "receiver",
            format!(
                "`{}.{}` has no receiver parameter",
                module_name, function.name
            ),
        ));
    }
    if function.receiver_command
        && function
            .parameters
            .first()
            .map(|parameter| parameter.type_name.as_str())
            != Some(function.return_type.as_str())
    {
        return Err(adapter_error(
            "receiver",
            format!(
                "`{}.{}` command return does not match its receiver",
                module_name, function.name
            ),
        ));
    }
    if function.native_symbol.is_some() {
        return Err(adapter_error(
            "native-symbol",
            format!(
                "`{}.{}` delegates to a native symbol",
                module_name, function.name
            ),
        ));
    }
    if function.blocks.len() != 1 || !actor_sites.is_empty() {
        return lower_cfg_function(module_name, function, functions, actor_sites, atoms);
    }
    let [block] = function.blocks.as_slice() else {
        return Err(adapter_error(
            "control-flow",
            format!(
                "`{}.{}` is not a single-block function",
                module_name, function.name
            ),
        ));
    };
    if block.label != "entry" || block.terminator.opcode != "return" {
        return Err(adapter_error(
            "control-flow",
            format!(
                "`{}.{}` is not an entry/return scalar function",
                module_name, function.name
            ),
        ));
    }
    let params = function
        .parameters
        .iter()
        .map(|parameter| native_type(&parameter.type_name))
        .collect::<Result<Vec<_>, _>>()?;
    if params.len() != function.arity {
        return Err(adapter_error(
            "arity",
            format!(
                "`{}.{}` parameter count does not match arity",
                module_name, function.name
            ),
        ));
    }
    let mut values = function
        .parameters
        .iter()
        .zip(params.iter().copied())
        .enumerate()
        .map(|(index, (parameter, value_type))| (parameter.name.clone(), (index, value_type)))
        .collect::<ValueCatalog>();
    let mut bindings = Vec::with_capacity(block.instructions.len());
    for instruction in &block.instructions {
        let result = instruction.result.as_ref().ok_or_else(|| {
            adapter_error(
                "instruction",
                format!(
                    "opcode `{}` has no result in scalar lowering",
                    instruction.opcode
                ),
            )
        })?;
        if values.contains_key(result) {
            return Err(adapter_error(
                "instruction",
                format!("result `{result}` already has a producer"),
            ));
        }
        let (mut expression, result_type) =
            instruction_expression(instruction, module_name, &values, functions, atoms)?;
        let matching_sites = actor_sites
            .iter()
            .filter(|site| site.block_label == block.label && site.result_name == *result)
            .collect::<Vec<_>>();
        if matching_sites.len() > 1 {
            return Err(adapter_error(
                "actor-site",
                format!(
                    "block `{}` result `{result}` has duplicate suspension sites",
                    block.label
                ),
            ));
        }
        if let Some(site) = matching_sites.first() {
            let captures = site
                .value_names
                .iter()
                .map(|name| value_operand(name, &values).map(|(value, _)| value))
                .collect::<Result<Vec<_>, _>>()?;
            expression = super::super::self_host_actor_adapter::apply_suspension_site(
                expression, site, captures,
            )?;
        }
        let index = params.len() + bindings.len();
        values.insert(result.clone(), (index, result_type));
        bindings.push(expression);
    }
    let return_type = native_type(&function.return_type)?;
    let body = match block.terminator.operands.as_slice() {
        [] if function.return_type == "Unit" => NativeExpr::Unit,
        [value] => {
            let (expression, value_type) = value_operand(value, &values)?;
            if value_type != return_type {
                return Err(adapter_error(
                    "return",
                    format!(
                        "`{}.{}` returns a value of the wrong type",
                        module_name, function.name
                    ),
                ));
            }
            expression
        }
        _ => {
            return Err(adapter_error(
                "return",
                format!(
                    "`{}.{}` has an invalid return operand",
                    module_name, function.name
                ),
            ))
        }
    };
    let body = wrap_bindings(bindings, body);
    Ok(native_function(
        module_name,
        function,
        params,
        return_type,
        body,
    ))
}
