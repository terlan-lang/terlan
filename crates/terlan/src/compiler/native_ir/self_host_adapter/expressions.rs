use super::*;

pub(super) fn map_type_arguments(type_name: &str) -> Result<(&str, &str), String> {
    let source = type_name.trim();
    let opening = source
        .find('[')
        .ok_or_else(|| adapter_error("map_construct", format!("`{source}` is not a Map type")))?;
    let constructor = source[..opening].trim();
    if constructor.rsplit('.').next() != Some("Map") || !source.ends_with(']') {
        return Err(adapter_error(
            "map_construct",
            format!("`{source}` is not a Map type"),
        ));
    }
    let arguments = &source[opening + 1..source.len() - 1];
    let mut depth = 0_usize;
    for (index, byte) in arguments.bytes().enumerate() {
        match byte {
            b'[' | b'{' | b'(' => depth = depth.saturating_add(1),
            b']' | b'}' | b')' => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    adapter_error("map_construct", "Map type has unbalanced delimiters")
                })?;
            }
            b',' if depth == 0 => {
                let key = arguments[..index].trim();
                let value = arguments[index + 1..].trim();
                if key.is_empty() || value.is_empty() {
                    break;
                }
                return Ok((key, value));
            }
            _ => {}
        }
    }
    Err(adapter_error(
        "map_construct",
        format!("Map type `{source}` must have key and value arguments"),
    ))
}

pub(in crate::compiler::native_ir) fn decoded_string_literal(text: &str) -> Result<String, String> {
    if !text.starts_with('"') {
        return Ok(text.to_owned());
    }
    let inner = text
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| adapter_error("literal", "unterminated String literal"))?;
    let mut output = String::new();
    let mut chars = inner.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let escaped = chars
            .next()
            .ok_or_else(|| adapter_error("literal", "unterminated String escape"))?;
        output.push(match escaped {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            other => other,
        });
    }
    Ok(output)
}

pub(super) fn map_construct_expr(
    instruction: &IrInstruction,
    values: &ValueCatalog,
) -> Result<(NativeExpr, NativeType), String> {
    if !instruction.operands.len().is_multiple_of(2) {
        return Err(adapter_error(
            "map_construct",
            "put-map requires alternating key/value operands",
        ));
    }
    if instruction.operand_types.len() != instruction.operands.len() {
        return Err(adapter_error(
            "map_construct",
            "put-map requires type evidence for every operand",
        ));
    }
    let result_type = native_type(&instruction.type_name)?;
    let NativeType::ManagedRef(semantic) = result_type else {
        return Err(adapter_error(
            "map_construct",
            "put-map result must have a managed Map type",
        ));
    };
    let (key_name, value_name) = map_type_arguments(&instruction.type_name)?;
    let key_type = native_type(key_name)?;
    let value_type = native_type(value_name)?;
    let mut args = Vec::with_capacity(instruction.operands.len());
    for (index, operand) in instruction.operands.iter().enumerate() {
        let (value, producer_type) = value_operand(operand, values)?;
        let declared_name = instruction.operand_types[index]
            .as_deref()
            .ok_or_else(|| adapter_error("map_construct", "map operand type is unresolved"))?;
        let declared_type = native_type(declared_name)?;
        let expected = if index.is_multiple_of(2) {
            key_type
        } else {
            value_type
        };
        if declared_type != expected || producer_type != expected {
            return Err(adapter_error(
                "map_construct",
                format!("map operand {index} disagrees with its declared Map type"),
            ));
        }
        args.push(value);
    }
    Ok((
        NativeExpr::ManagedOperation {
            encoded: Arc::from(
                crate::runtime::native_image::managed::encode_map_from_entries_operation(semantic),
            ),
            args,
        },
        result_type,
    ))
}

pub(super) fn value_operand(
    name: &str,
    values: &ValueCatalog,
) -> Result<(NativeExpr, NativeType), String> {
    values
        .get(name)
        .map(|(index, value_type)| (NativeExpr::Param(*index), *value_type))
        .ok_or_else(|| adapter_error("operand", format!("value `{name}` has no producer")))
}

pub(super) fn one_operand<'a>(instruction: &'a IrInstruction) -> Result<&'a str, String> {
    match instruction.operands.as_slice() {
        [operand] => Ok(operand),
        _ => Err(adapter_error(
            "instruction",
            format!(
                "opcode `{}` requires exactly one operand",
                instruction.opcode
            ),
        )),
    }
}

pub(super) fn two_operands<'a>(
    instruction: &'a IrInstruction,
) -> Result<(&'a str, &'a str), String> {
    match instruction.operands.as_slice() {
        [left, right] => Ok((left, right)),
        _ => Err(adapter_error(
            "instruction",
            format!(
                "opcode `{}` requires exactly two operands",
                instruction.opcode
            ),
        )),
    }
}

pub(super) fn literal(
    instruction: &IrInstruction,
    result_type: NativeType,
    atoms: &mut Vec<String>,
) -> Result<NativeExpr, String> {
    let text = one_operand(instruction)?;
    match result_type {
        NativeType::Unit if text == "Unit" || text == "unit" || text == "()" => {
            Ok(NativeExpr::Unit)
        }
        NativeType::Int => text
            .parse::<i64>()
            .map(NativeExpr::Int)
            .map_err(|_| adapter_error("literal", format!("invalid Int literal `{text}`"))),
        NativeType::Float => text
            .parse::<f64>()
            .map(|value| NativeExpr::Float(value.to_bits()))
            .map_err(|_| adapter_error("literal", format!("invalid Float literal `{text}`"))),
        NativeType::Bool if text == "true" => Ok(NativeExpr::Bool(true)),
        NativeType::Bool if text == "false" => Ok(NativeExpr::Bool(false)),
        NativeType::Atom => {
            if !atoms.iter().any(|atom| atom == text) {
                atoms.push(text.to_owned());
            }
            Ok(NativeExpr::AtomLiteral(Arc::<str>::from(text)))
        }
        NativeType::StringRef => Ok(NativeExpr::ManagedLiteral {
            encoded: Arc::from(
                crate::runtime::native_image::managed::encode_string_literal(
                    &decoded_string_literal(text)?,
                )
                .map_err(|error| adapter_error("literal", error.to_string()))?,
            ),
        }),
        NativeType::BytesRef | NativeType::BinaryRef => Err(adapter_error(
            "literal",
            "managed scalar literals require canonical managed encoding",
        )),
        _ => Err(adapter_error(
            "literal",
            format!(
                "literal `{text}` does not match `{}`",
                instruction.type_name
            ),
        )),
    }
}

pub(super) fn binary_operator(opcode: &str) -> Option<NativeBinaryOperator> {
    match opcode {
        "add-small" | "binary-+" => Some(NativeBinaryOperator::Add),
        "subtract-small" | "sub-small" | "binary--" => Some(NativeBinaryOperator::Subtract),
        "multiply-small" | "mul-small" | "binary-*" => Some(NativeBinaryOperator::Multiply),
        "divide-small" | "div-small" | "binary-/" => Some(NativeBinaryOperator::Divide),
        "remainder-small" | "rem-small" | "binary-%" => Some(NativeBinaryOperator::Remainder),
        "equal" | "equal-exact" | "binary-==" => Some(NativeBinaryOperator::Equal),
        "not-equal-exact" | "binary-!=" => Some(NativeBinaryOperator::NotEqual),
        "less-small" | "binary-<" => Some(NativeBinaryOperator::LessThan),
        "less-equal-small" | "binary-<=" => Some(NativeBinaryOperator::LessThanOrEqual),
        "greater-small" | "binary->" => Some(NativeBinaryOperator::GreaterThan),
        "greater-equal-small" | "binary->=" => Some(NativeBinaryOperator::GreaterThanOrEqual),
        _ => None,
    }
}

pub(super) fn binary_expression(
    instruction: &IrInstruction,
    operator: NativeBinaryOperator,
    result_type: NativeType,
    values: &ValueCatalog,
) -> Result<NativeExpr, String> {
    let (left_name, right_name) = two_operands(instruction)?;
    let (left, left_type) = value_operand(left_name, values)?;
    let (right, right_type) = value_operand(right_name, values)?;
    if left_type != right_type {
        return Err(adapter_error(
            "binary",
            format!(
                "operands of `{}` have different native types",
                instruction.opcode
            ),
        ));
    }
    let comparison = matches!(
        operator,
        NativeBinaryOperator::Equal
            | NativeBinaryOperator::NotEqual
            | NativeBinaryOperator::LessThan
            | NativeBinaryOperator::LessThanOrEqual
            | NativeBinaryOperator::GreaterThan
            | NativeBinaryOperator::GreaterThanOrEqual
    );
    if comparison && result_type != NativeType::Bool {
        return Err(adapter_error(
            "binary",
            format!("comparison `{}` must produce Bool", instruction.opcode),
        ));
    }
    if !comparison && result_type != left_type {
        return Err(adapter_error(
            "binary",
            format!(
                "arithmetic `{}` changes its operand type",
                instruction.opcode
            ),
        ));
    }
    if !matches!(left_type, NativeType::Int | NativeType::Float) {
        return Err(adapter_error(
            "binary",
            format!(
                "opcode `{}` has unsupported native operands",
                instruction.opcode
            ),
        ));
    }
    Ok(NativeExpr::Binary {
        operator,
        operand_type: left_type,
        left: Box::new(left),
        right: Box::new(right),
    })
}

pub(super) fn local_call(
    instruction: &IrInstruction,
    module_name: &str,
    result_type: NativeType,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
) -> Result<NativeExpr, String> {
    let (callee, arguments) = instruction
        .operands
        .split_first()
        .ok_or_else(|| adapter_error("call", "local call has no callable identity"))?;
    invoke_call(
        module_name,
        callee,
        arguments,
        result_type,
        values,
        functions,
    )
}

pub(super) fn invoke_call(
    module_name: &str,
    callee: &str,
    arguments: &[String],
    result_type: NativeType,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
) -> Result<NativeExpr, String> {
    let target = functions
        .get(&(module_name.to_owned(), callee.to_owned(), arguments.len()))
        .ok_or_else(|| {
            adapter_error(
                "call",
                format!(
                    "local callable `{callee}/{}` is not in this ABI module",
                    arguments.len()
                ),
            )
        })?;
    if target.result != result_type {
        return Err(adapter_error(
            "call",
            format!(
                "local call `{callee}/{}` has the wrong result type",
                arguments.len()
            ),
        ));
    }
    let typed_args = arguments
        .iter()
        .map(|argument| value_operand(argument, values))
        .collect::<Result<Vec<_>, _>>()?;
    if typed_args
        .iter()
        .map(|(_, value_type)| *value_type)
        .ne(target.parameters.iter().copied())
    {
        return Err(adapter_error(
            "call",
            format!(
                "local call `{callee}/{}` has incompatible arguments",
                arguments.len()
            ),
        ));
    }
    Ok(NativeExpr::Call {
        function: target.index,
        args: typed_args
            .into_iter()
            .map(|(expression, _)| expression)
            .collect(),
    })
}

pub(super) fn imported_call(
    instruction: &IrInstruction,
    result_type: NativeType,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
) -> Result<NativeExpr, String> {
    match instruction.operands.as_slice() {
        [module_name, callee, arguments @ ..] => invoke_call(
            module_name,
            callee,
            arguments,
            result_type,
            values,
            functions,
        ),
        _ => Err(adapter_error(
            "call",
            "imported call has no module or callable identity",
        )),
    }
}

pub(super) fn local_mutable_call(
    instruction: &IrInstruction,
    module_name: &str,
    result_type: NativeType,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
) -> Result<NativeExpr, String> {
    match instruction.operands.as_slice() {
        [callee, _place, arguments @ ..] => invoke_call(
            module_name,
            callee,
            arguments,
            result_type,
            values,
            functions,
        ),
        _ => Err(adapter_error(
            "call",
            "mutable receiver call has no callable, place, or receiver operand",
        )),
    }
}

pub(super) fn imported_mutable_call(
    instruction: &IrInstruction,
    result_type: NativeType,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
) -> Result<NativeExpr, String> {
    match instruction.operands.as_slice() {
        [module_name, callee, _place, arguments @ ..] => invoke_call(
            module_name,
            callee,
            arguments,
            result_type,
            values,
            functions,
        ),
        _ => Err(adapter_error(
            "call",
            "imported mutable call has no module, callable, place, or receiver operand",
        )),
    }
}

pub(super) fn managed_instruction_operands(
    instruction: &IrInstruction,
    values: &ValueCatalog,
) -> Result<Option<Vec<NativeExpr>>, String> {
    if !matches!(
        instruction.opcode.as_str(),
        "put-array"
            | "put-list"
            | "put-list-cons"
            | "put-tuple"
            | "put-variant"
            | "get-tuple-element"
    ) {
        return Ok(None);
    }
    if instruction.operand_types.len() != instruction.operands.len() {
        return Err(adapter_error(
            "managed_operand_types",
            format!(
                "opcode `{}` requires one ABI-1.3 type slot per operand",
                instruction.opcode
            ),
        ));
    }
    let mut args = Vec::with_capacity(instruction.operands.len());
    for (index, operand) in instruction.operands.iter().enumerate() {
        if instruction.opcode == "put-variant" && index == 0 {
            if instruction.operand_types[index].is_some() || operand.trim().is_empty() {
                return Err(adapter_error(
                    "variant_constructor",
                    "put-variant constructor metadata must be nonempty and untyped",
                ));
            }
            continue;
        }
        if instruction.opcode == "get-tuple-element" && index == 1 {
            let value = operand.parse::<i64>().map_err(|_| {
                adapter_error(
                    "tuple_projection_index",
                    format!("tuple index `{operand}` is not a non-negative integer"),
                )
            })?;
            if value < 0 {
                return Err(adapter_error(
                    "tuple_projection_index",
                    format!("tuple index `{operand}` is negative"),
                ));
            }
            args.push(NativeExpr::Int(value));
            continue;
        }
        let (value, actual_type) = value_operand(operand, values)?;
        let expected_type = instruction.operand_types[index]
            .as_deref()
            .ok_or_else(|| {
                adapter_error(
                    "managed_operand_type",
                    format!(
                        "opcode `{}` operand {index} has no resolved type",
                        instruction.opcode
                    ),
                )
            })
            .and_then(native_type)?;
        if actual_type != expected_type {
            return Err(adapter_error(
                "managed_operand_type",
                format!(
                    "opcode `{}` operand {index} type evidence disagrees with its SSA producer",
                    instruction.opcode
                ),
            ));
        }
        args.push(value);
    }
    Ok(Some(args))
}

pub(super) fn closure_type(
    parameters: &[NativeType],
    result: NativeType,
) -> Result<NativeType, String> {
    let parameters = parameters
        .iter()
        .copied()
        .map(NativeType::boundary_type)
        .collect::<Vec<_>>();
    ManagedClosureDescriptor::semantic_id_for_signature(&parameters, &[result.boundary_type()])
        .map(NativeType::ManagedRef)
        .map_err(|error| adapter_error("closure_signature", error.to_string()))
}

pub(super) fn make_closure(
    instruction: &IrInstruction,
    module_name: &str,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
) -> Result<(NativeExpr, NativeType), String> {
    let (callee, captures) = instruction
        .operands
        .split_first()
        .ok_or_else(|| adapter_error("closure", "make-closure has no callable identity"))?;
    let candidates = functions
        .iter()
        .filter(|((module, name, _arity), target)| {
            module == module_name && name == callee && target.capture_count == captures.len()
        })
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err(adapter_error(
            "closure_target",
            format!(
                "callable `{module_name}.{callee}` is absent or overloaded without an ABI signature"
            ),
        ));
    }
    let (identity, target) = candidates[0];
    let capture_values = captures
        .iter()
        .map(|capture| value_operand(capture, values))
        .collect::<Result<Vec<_>, _>>()?;
    if capture_values
        .iter()
        .map(|(_, capture_type)| *capture_type)
        .ne(target.parameters[..target.capture_count].iter().copied())
    {
        return Err(adapter_error(
            "closure_captures",
            "captured values disagree with the lifted callable prefix",
        ));
    }
    let result_type = closure_type(&target.parameters[target.capture_count..], target.result)?;
    let callable_id = stable_export_id(module_name, callee, identity.2);
    let encoded = encode_closure_allocation(callable_id)
        .map_err(|error| adapter_error("closure_allocation", error.to_string()))?;
    Ok((
        NativeExpr::MakeClosure {
            encoded: Arc::from(encoded),
            captures: capture_values
                .into_iter()
                .map(|(capture, _)| capture)
                .collect(),
        },
        result_type,
    ))
}

pub(super) fn call_value(
    instruction: &IrInstruction,
    result_type: NativeType,
    values: &ValueCatalog,
) -> Result<NativeExpr, String> {
    let (callee_name, arguments) = instruction
        .operands
        .split_first()
        .ok_or_else(|| adapter_error("closure_call", "call-value has no callee"))?;
    let (callee, callee_type) = value_operand(callee_name, values)?;
    let typed_args = arguments
        .iter()
        .map(|argument| value_operand(argument, values))
        .collect::<Result<Vec<_>, _>>()?;
    let parameter_types = typed_args
        .iter()
        .map(|(_, argument_type)| *argument_type)
        .collect::<Vec<_>>();
    let expected_closure = closure_type(&parameter_types, result_type)?;
    if callee_type != expected_closure {
        return Err(adapter_error(
            "closure_call",
            "call-value callee signature disagrees with its typed arguments or result",
        ));
    }
    Ok(NativeExpr::InvokeClosure {
        callee: Box::new(callee),
        args: typed_args
            .into_iter()
            .map(|(argument, _)| argument)
            .collect(),
        parameter_types,
        result_type,
    })
}
