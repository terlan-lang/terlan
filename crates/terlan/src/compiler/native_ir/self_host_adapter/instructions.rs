use super::*;

pub(super) fn instruction_expression(
    instruction: &IrInstruction,
    module_name: &str,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
    atoms: &mut Vec<String>,
) -> Result<(NativeExpr, NativeType), String> {
    if instruction.opcode == "try-region" {
        return try_region_expression(instruction, module_name, values, functions, atoms);
    }
    if instruction.opcode == "prepare-pattern" {
        let (subject_name, _pattern) =
            super::super::self_host_pattern_ir::decode_prepare_pattern(instruction)?;
        let (subject, subject_type) = value_operand(subject_name, values)?;
        return super::super::self_host_managed_layouts::prepare_pattern_expr_with_catalogs(
            instruction,
            subject,
            subject_type,
            functions.variants.as_ref(),
            Some(&functions.structs),
        );
    }
    if matches!(
        instruction.opcode.as_str(),
        "pattern-matched" | "get-pattern-binding"
    ) {
        let environment_name = instruction.operands.first().ok_or_else(|| {
            adapter_error(
                "pattern_environment",
                format!("opcode `{}` has no environment operand", instruction.opcode),
            )
        })?;
        let (environment, environment_type) = value_operand(environment_name, values)?;
        return super::super::self_host_managed_layouts::pattern_environment_expr_with_catalogs(
            instruction,
            environment,
            environment_type,
            functions.variants.as_ref(),
            Some(&functions.structs),
        )?
        .ok_or_else(|| {
            adapter_error(
                "pattern_environment",
                format!("opcode `{}` has no pattern lowering", instruction.opcode),
            )
        });
    }
    if instruction.opcode == "make-closure" {
        return make_closure(instruction, module_name, values, functions);
    }
    let result_type = native_type(&instruction.type_name)?;
    if instruction.opcode == "get-map-elements" {
        let source_name = instruction
            .operands
            .first()
            .ok_or_else(|| adapter_error("map_field", "get-map-elements has no source operand"))?;
        let (source, source_type) = value_operand(source_name, values)?;
        let declared_source = instruction
            .operand_types
            .first()
            .and_then(Option::as_deref)
            .ok_or_else(|| adapter_error("struct_field", "field source has no type evidence"))?;
        let constructor = declared_source
            .split_once('[')
            .map_or(declared_source, |(name, _)| name)
            .trim();
        if let Some(fields) = functions.structs.get(constructor) {
            let field_name = instruction.operands.get(1).ok_or_else(|| {
                adapter_error("struct_field", "field projection has no field identity")
            })?;
            let field = fields.get(field_name).ok_or_else(|| {
                adapter_error(
                    "struct_field",
                    format!("struct `{constructor}` has no field `{field_name}`"),
                )
            })?;
            let field_type = native_type(&field.type_name)?;
            if field_type != result_type {
                return Err(adapter_error(
                    "struct_field",
                    "field result type disagrees with its ABI layout",
                ));
            }
            return super::super::self_host_managed_layouts::aggregate_field_expr(
                instruction,
                source,
                source_type,
                result_type,
                field.index,
            );
        }
        return super::super::self_host_managed_layouts::map_field_expr(
            instruction,
            source,
            source_type,
            result_type,
        );
    }
    if instruction.opcode == "update-struct-field" {
        let [source_name, field_name, replacement_name] = instruction.operands.as_slice() else {
            return Err(adapter_error(
                "struct_update",
                "update-struct-field requires source, field, and replacement operands",
            ));
        };
        let (source, source_type) = value_operand(source_name, values)?;
        let (replacement, replacement_type) = value_operand(replacement_name, values)?;
        let declared_source = instruction
            .operand_types
            .first()
            .and_then(Option::as_deref)
            .ok_or_else(|| adapter_error("struct_update", "field source has no type evidence"))?;
        let constructor = declared_source
            .split_once('[')
            .map_or(declared_source, |(name, _)| name)
            .trim();
        let fields = functions.structs.get(constructor).ok_or_else(|| {
            adapter_error(
                "struct_update",
                format!("struct `{constructor}` has no admitted ABI layout"),
            )
        })?;
        let field = fields.get(field_name).ok_or_else(|| {
            adapter_error(
                "struct_update",
                format!("struct `{constructor}` has no field `{field_name}`"),
            )
        })?;
        return super::super::self_host_managed_layouts::aggregate_replace_field_expr(
            instruction,
            source,
            source_type,
            replacement,
            replacement_type,
            result_type,
            field.index,
            &field.type_name,
        );
    }
    if instruction.opcode == "put-map" {
        return map_construct_expr(instruction, values);
    }
    if instruction.opcode == "put-struct" {
        let constructor = instruction
            .type_name
            .split_once('[')
            .map_or(instruction.type_name.as_str(), |(name, _)| name)
            .trim();
        if constructor
            .strip_prefix(module_name)
            .and_then(|suffix| suffix.strip_prefix('.'))
            .is_none()
        {
            return Err(adapter_error(
                "struct_construct_visibility",
                format!("cannot raw-construct non-local struct `{constructor}`"),
            ));
        }
        let declared = functions.structs.get(constructor).ok_or_else(|| {
            adapter_error(
                "struct_construct",
                format!("struct `{constructor}` has no admitted ABI layout"),
            )
        })?;
        if instruction.operands.len() != instruction.operand_types.len()
            || !instruction.operands.len().is_multiple_of(2)
        {
            return Err(adapter_error(
                "struct_construct",
                "put-struct requires aligned field/value operand pairs",
            ));
        }
        let mut provided = HashMap::new();
        for (pair_index, pair) in instruction.operands.chunks_exact(2).enumerate() {
            let name = &pair[0];
            let (value, value_type) = value_operand(&pair[1], values)?;
            let declared_name_type = instruction
                .operand_types
                .get(pair_index * 2)
                .and_then(Option::as_deref)
                .ok_or_else(|| {
                    adapter_error("struct_construct", "field identity has no type evidence")
                })?;
            if native_type(declared_name_type)? != native_type("String")? {
                return Err(adapter_error(
                    "struct_construct",
                    "field identity must carry String type evidence",
                ));
            }
            let declared_value_type = instruction
                .operand_types
                .get(pair_index * 2 + 1)
                .and_then(Option::as_deref)
                .ok_or_else(|| {
                    adapter_error("struct_construct", "field value has no type evidence")
                })?;
            if native_type(declared_value_type)? != value_type {
                return Err(adapter_error(
                    "struct_construct",
                    "field value type evidence disagrees with its SSA producer",
                ));
            }
            if provided.insert(name.clone(), (value, value_type)).is_some() {
                return Err(adapter_error(
                    "struct_construct",
                    format!("struct `{constructor}` repeats field `{name}`"),
                ));
            }
        }
        if provided.len() != declared.len() {
            return Err(adapter_error(
                "struct_construct",
                format!("struct `{constructor}` requires every declared field"),
            ));
        }
        let mut ordered = declared.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|(_, field)| field.index);
        let mut fields = Vec::with_capacity(ordered.len());
        for (name, field) in ordered {
            let (value, actual) = provided.remove(name).ok_or_else(|| {
                adapter_error(
                    "struct_construct",
                    format!("struct `{constructor}` is missing field `{name}`"),
                )
            })?;
            let expected = native_type(&field.type_name)?;
            if actual != expected {
                return Err(adapter_error(
                    "struct_construct",
                    format!("struct `{constructor}` field `{name}` has the wrong type"),
                ));
            }
            fields.push((name.clone(), field.type_name.clone(), value));
        }
        return Ok((
            super::super::self_host_managed_layouts::aggregate_record_expr(
                &instruction.type_name,
                fields,
            )?,
            result_type,
        ));
    }
    if let Some(args) = managed_instruction_operands(instruction, values)? {
        let expression =
            super::super::self_host_managed_layouts::managed_instruction_expr_with_variants(
                instruction,
                args,
                functions.variants.as_ref(),
            )?
            .ok_or_else(|| {
                adapter_error(
                    "managed_instruction",
                    format!("opcode `{}` has no managed lowering", instruction.opcode),
                )
            })?;
        return Ok((expression, result_type));
    }
    let expression = match instruction.opcode.as_str() {
        "load-literal" => literal(instruction, result_type, atoms)?,
        "load-atom" if result_type == NativeType::Atom => {
            literal(instruction, NativeType::Atom, atoms)?
        }
        "load-atom" => {
            return Err(adapter_error("literal", "load-atom must produce Atom"));
        }
        "move" => {
            let (value, value_type) = value_operand(one_operand(instruction)?, values)?;
            if value_type != result_type {
                return Err(adapter_error("move", "move changes the source value type"));
            }
            value
        }
        "neg-small" => {
            let (value, value_type) = value_operand(one_operand(instruction)?, values)?;
            if value_type != result_type {
                return Err(adapter_error(
                    "negation",
                    "negation changes the operand type",
                ));
            }
            match value_type {
                NativeType::Int => NativeExpr::Neg(Box::new(value)),
                NativeType::Float => NativeExpr::FloatNeg(Box::new(value)),
                _ => {
                    return Err(adapter_error(
                        "negation",
                        "negation requires an Int or Float operand",
                    ))
                }
            }
        }
        "unary-not" | "unary-!" => {
            let (value, value_type) = value_operand(one_operand(instruction)?, values)?;
            if value_type != NativeType::Bool || result_type != NativeType::Bool {
                return Err(adapter_error(
                    "not",
                    "logical negation requires a Bool operand",
                ));
            }
            NativeExpr::Not(Box::new(value))
        }
        "call-local" => local_call(instruction, module_name, result_type, values, functions)?,
        "call-import" => imported_call(instruction, result_type, values, functions)?,
        "call-local-mut-receiver" => {
            local_mutable_call(instruction, module_name, result_type, values, functions)?
        }
        "call-import-mut-receiver" => {
            imported_mutable_call(instruction, result_type, values, functions)?
        }
        "call-value" => call_value(instruction, result_type, values)?,
        opcode => match binary_operator(opcode) {
            Some(operator) => binary_expression(instruction, operator, result_type, values)?,
            None => {
                return Err(adapter_error(
                    "opcode",
                    format!("opcode `{opcode}` is outside the direct-native scalar slice"),
                ))
            }
        },
    };
    Ok((expression, result_type))
}
