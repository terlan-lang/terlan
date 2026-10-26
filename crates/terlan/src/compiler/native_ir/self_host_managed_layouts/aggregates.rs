use super::*;

/// Lowers a string-keyed map field projection using ABI-1.3 operand evidence.
pub(in crate::compiler::native_ir) fn map_field_expr(
    instruction: &IrInstruction,
    source: NativeExpr,
    source_type: NativeType,
    result_type: NativeType,
) -> Result<(NativeExpr, NativeType), NativeIrError> {
    let [source_name, field_name] = instruction.operands.as_slice() else {
        return Err(
            ("error[self_host.native.map_field]: get-map-elements requires source and field"
                .to_string())
            .into(),
        );
    };
    if source_name.is_empty() || field_name.is_empty() {
        return Err(
            ("error[self_host.native.map_field]: map source and field must not be empty"
                .to_string())
            .into(),
        );
    }
    let declared_source = instruction
        .operand_types
        .first()
        .and_then(Option::as_deref)
        .ok_or_else(|| {
            "error[self_host.native.map_field]: map source has no ABI type evidence".to_string()
        })?;
    let map = parse_type(declared_source)?;
    if !is_map_type(&map) {
        return Err((format!(
            "error[self_host.native.map_field]: structural field source `{declared_source}` is not Map[String, T]"
        )).into());
    }
    let key = &map.arguments[0];
    if !(key.constructor == "String" || key.constructor.ends_with(".String")) {
        return Err(
            ("error[self_host.native.map_field]: field projection requires a String-keyed Map"
                .to_string())
            .into(),
        );
    }
    if native_type(declared_source)? != Some(source_type) {
        return Err(("error[self_host.native.map_field]: map type evidence disagrees with its SSA producer"
                .to_string()).into());
    }
    let value_type = native_type_of(&map.arguments[1])?.ok_or_else(|| {
        "error[self_host.native.map_field]: map value type is not native".to_string()
    })?;
    if value_type != result_type {
        return Err(
            ("error[self_host.native.map_field]: map field result disagrees with its value type"
                .to_string())
            .into(),
        );
    }
    let semantic = SemanticTypeId::from_canonical(&type_contract(&map, "")?)
        .map_err(|error| format!("error[self_host.native.map_field_abi]: {error}"))?;
    let encoded_key = encode_string_literal(field_name)
        .map_err(|error| format!("error[self_host.native.map_field_key]: {error}"))?;
    Ok((
        NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_map_get_operation(
                semantic,
                result_type.is_managed_reference(),
            )),
            args: vec![
                source,
                NativeExpr::ManagedLiteral {
                    encoded: Arc::from(encoded_key),
                },
            ],
        },
        result_type,
    ))
}

/// Lowers a named struct field after the application catalog resolves its index.
pub(in crate::compiler::native_ir) fn aggregate_field_expr(
    instruction: &IrInstruction,
    source: NativeExpr,
    source_type: NativeType,
    result_type: NativeType,
    field: usize,
) -> Result<(NativeExpr, NativeType), NativeIrError> {
    let declared_source = instruction
        .operand_types
        .first()
        .and_then(Option::as_deref)
        .ok_or_else(|| {
            "error[self_host.native.struct_field]: struct source has no ABI type evidence"
                .to_string()
        })?;
    let aggregate = parse_type(declared_source)?;
    if native_type(declared_source)? != Some(source_type) {
        return Err(("error[self_host.native.struct_field]: struct type evidence disagrees with its SSA producer"
                .to_string()).into());
    }
    let semantic = SemanticTypeId::from_canonical(&type_contract(&aggregate, "")?)
        .map_err(|error| format!("error[self_host.native.struct_field_abi]: {error}"))?;
    let encoded = if result_type.is_managed_reference() {
        encode_aggregate_field_operation(semantic, field)
    } else {
        encode_aggregate_scalar_field_operation(semantic, field)
    }
    .map_err(|error| format!("error[self_host.native.struct_field_abi]: {error}"))?;
    Ok((
        NativeExpr::ManagedOperation {
            encoded: Arc::from(encoded),
            args: vec![source],
        },
        result_type,
    ))
}

/// Lowers one immutable named-struct field replacement after catalog resolution.
pub(in crate::compiler::native_ir) fn aggregate_replace_field_expr(
    instruction: &IrInstruction,
    source: (NativeExpr, NativeType),
    replacement: (NativeExpr, NativeType),
    result_type: NativeType,
    field: usize,
    expected_field_type: &str,
) -> Result<(NativeExpr, NativeType), NativeIrError> {
    let (source, source_type) = source;
    let (replacement, replacement_type) = replacement;
    let declared_source = instruction
        .operand_types
        .first()
        .and_then(Option::as_deref)
        .ok_or_else(|| {
            "error[self_host.native.struct_update]: struct source has no ABI type evidence"
                .to_string()
        })?;
    let aggregate = parse_type(declared_source)?;
    if native_type(declared_source)? != Some(source_type) || result_type != source_type {
        return Err(("error[self_host.native.struct_update]: struct source, result, and ABI type evidence disagree"
                .to_string()).into());
    }
    let declared_replacement = instruction
        .operand_types
        .get(2)
        .and_then(Option::as_deref)
        .ok_or_else(|| {
            "error[self_host.native.struct_update]: replacement has no ABI type evidence"
                .to_string()
        })?;
    if native_type(declared_replacement)? != Some(replacement_type)
        || native_type(expected_field_type)? != Some(replacement_type)
    {
        return Err(("error[self_host.native.struct_update]: replacement type disagrees with the struct field"
                .to_string()).into());
    }
    let semantic = SemanticTypeId::from_canonical(&type_contract(&aggregate, "")?)
        .map_err(|error| format!("error[self_host.native.struct_update_abi]: {error}"))?;
    let encoded = encode_aggregate_replace_field_operation(semantic, field)
        .map_err(|error| format!("error[self_host.native.struct_update_abi]: {error}"))?;
    Ok((
        NativeExpr::ManagedOperation {
            encoded: Arc::from(encoded),
            args: vec![source, replacement],
        },
        result_type,
    ))
}

pub(super) fn bool_and(left: NativeExpr, right: NativeExpr) -> NativeExpr {
    NativeExpr::If {
        clauses: vec![
            (left, right),
            (NativeExpr::Bool(true), NativeExpr::Bool(false)),
        ],
    }
}

/// Lowers ABI list constructors after their SSA operands have been resolved.
pub(in crate::compiler::native_ir) fn collection_expr(
    opcode: &str,
    type_name: &str,
    args: Vec<NativeExpr>,
) -> Result<Option<NativeExpr>, NativeIrError> {
    if !matches!(opcode, "put-list" | "put-list-cons") {
        return Ok(None);
    }
    let ty = parse_type(type_name)?;
    let short = ty.constructor.rsplit('.').next().unwrap_or(&ty.constructor);
    if short != "List" || ty.arguments.len() != 1 {
        return Err((format!(
            "error[self_host.native.list_type]: {opcode} result must be a concrete List[T], found {type_name:?}"
        )).into());
    }
    if opcode == "put-list-cons" && args.len() != 2 {
        return Err((format!(
            "error[self_host.native.list_cons_arity]: put-list-cons expects head and tail, found {} operand(s)",
            args.len()
        )).into());
    }
    let semantic = SemanticTypeId::from_canonical(&type_contract(&ty, "")?)
        .map_err(|error| format!("error[self_host.native.list_semantic]: {error}"))?;
    let encoded = if opcode == "put-list" {
        encode_list_from_elements_operation(semantic)
    } else {
        encode_list_prepend_operation(semantic)
    };
    Ok(Some(NativeExpr::ManagedOperation {
        encoded: Arc::from(encoded),
        args,
    }))
}

/// Lowers an ABI tuple or array constructor into a checked allocation.
pub(in crate::compiler::native_ir) fn aggregate_expr(
    opcode: &str,
    type_name: &str,
    fields: Vec<NativeExpr>,
) -> Result<Option<NativeExpr>, NativeIrError> {
    if !matches!(opcode, "put-tuple" | "put-array") {
        return Ok(None);
    }
    let ty = parse_type(type_name)?;
    if opcode == "put-array" {
        let [length, element] = ty.arguments.as_slice() else {
            return Err((format!(
                "error[self_host.native.array_type]: fixed array result requires size and element type, found {type_name:?}"
            )).into());
        };
        if ty.constructor != "Array" {
            return Err((format!(
                "error[self_host.native.array_type]: put-array result must be Array[N, T], found {type_name:?}"
            )).into());
        }
        let length = length.constructor.parse::<usize>().map_err(|_| {
            format!(
                "error[self_host.native.array_size]: fixed array size is not concrete in {type_name:?}"
            )
        })?;
        if length != fields.len() {
            return Err((format!(
                "error[self_host.native.array_arity]: fixed array type has {length} field(s), instruction has {} operand(s)",
                fields.len()
            )).into());
        }
        let canonical = type_contract(&ty, "")?;
        let descriptor = Arc::new(
            ManagedAggregateDescriptor::array(
                &canonical,
                managed_field_type_of(element, "")?,
                length,
            )
            .map_err(|error| format!("error[self_host.native.array_layout]: {error}"))?,
        );
        let encoded_layout = Arc::<[u8]>::from(
            encode_aggregate_layout(&descriptor)
                .map_err(|error| format!("error[self_host.native.array_layout_abi]: {error}"))?,
        );
        return Ok(Some(NativeExpr::Construct {
            descriptor,
            encoded_layout,
            fields,
        }));
    }
    if ty.constructor != "$Tuple" {
        return Err((format!(
            "error[self_host.native.tuple_type]: put-tuple result must be a tuple, found {type_name:?}"
        )).into());
    }
    if ty.arguments.len() != fields.len() {
        return Err((format!(
            "error[self_host.native.tuple_arity]: tuple type has {} field(s), instruction has {} operand(s)",
            ty.arguments.len(),
            fields.len()
        )).into());
    }
    let canonical = type_contract(&ty, "")?;
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::tuple(
            &canonical,
            ty.arguments
                .iter()
                .map(|field| managed_field_type_of(field, ""))
                .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(|error| format!("error[self_host.native.tuple_layout]: {error}"))?,
    );
    let encoded_layout = Arc::<[u8]>::from(
        encode_aggregate_layout(&descriptor)
            .map_err(|error| format!("error[self_host.native.tuple_layout_abi]: {error}"))?,
    );
    Ok(Some(NativeExpr::Construct {
        descriptor,
        encoded_layout,
        fields,
    }))
}

/// Builds one nominal record after the adapter orders and validates its fields.
pub(in crate::compiler::native_ir) fn aggregate_record_expr(
    type_name: &str,
    fields: Vec<(String, String, NativeExpr)>,
) -> Result<NativeExpr, NativeIrError> {
    let ty = parse_type(type_name)?;
    let canonical = type_contract(&ty, "")?;
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::record(
            &canonical,
            fields
                .iter()
                .map(|(name, field_type, _)| {
                    Ok((
                        name.clone(),
                        managed_field_type_of(&parse_type(field_type)?, "")?,
                    ))
                })
                .collect::<Result<Vec<_>, NativeIrError>>()?,
        )
        .map_err(|error| format!("error[self_host.native.struct_construct_layout]: {error}"))?,
    );
    let encoded_layout =
        Arc::<[u8]>::from(encode_aggregate_layout(&descriptor).map_err(|error| {
            format!("error[self_host.native.struct_construct_layout_abi]: {error}")
        })?);
    Ok(NativeExpr::Construct {
        descriptor,
        encoded_layout,
        fields: fields.into_iter().map(|(_, _, value)| value).collect(),
    })
}

/// Lowers one finite-sum constructor from authoritative variant metadata.
pub(in crate::compiler::native_ir) fn variant_expr(
    instruction: &IrInstruction,
    fields: Vec<NativeExpr>,
    catalog: &crate::self_host_variant_ir::VariantCatalog,
) -> Result<NativeExpr, NativeIrError> {
    let constructor = instruction.operands.first().ok_or_else(|| {
        "error[self_host.native.variant_constructor]: put-variant has no constructor identity"
            .to_string()
    })?;
    let payload_types = instruction.operand_types.get(1..).ok_or_else(|| {
        "error[self_host.native.variant_operand_types]: put-variant has no payload type slice"
            .to_string()
    })?;
    let plan = crate::self_host_variant_adapter::plan_construction(
        catalog,
        constructor,
        &instruction.type_name,
        payload_types,
    )
    .map_err(|error| format!("error[self_host.native.variant_layout]: {error}"))?;
    if fields.len() != plan.operand_types.len() {
        return Err((format!(
            "error[self_host.native.variant_arity]: constructor `{}` received {} values for {} payload slots",
            plan.descriptor.constructor,
            fields.len(),
            plan.operand_types.len()
        )).into());
    }
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::constructor(
            &type_contract(&parse_type(&instruction.type_name)?, "")?,
            &plan.descriptor.constructor,
            plan.descriptor.discriminant,
            plan.descriptor.variant_count,
            plan.operand_types
                .iter()
                .map(|type_name| {
                    parse_type(type_name)
                        .and_then(|field| managed_field_type_of(&field, ""))
                        .map(|field_type| (None, field_type))
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(|error| format!("error[self_host.native.variant_layout]: {error}"))?,
    );
    let encoded_layout = Arc::<[u8]>::from(
        encode_aggregate_layout(&descriptor)
            .map_err(|error| format!("error[self_host.native.variant_layout_abi]: {error}"))?,
    );
    Ok(NativeExpr::Construct {
        descriptor,
        encoded_layout,
        fields,
    })
}

/// Lowers one managed ABI instruction from resolved SSA values and type evidence.
#[cfg(test)]
pub(in crate::compiler::native_ir) fn managed_instruction_expr(
    instruction: &IrInstruction,
    args: Vec<NativeExpr>,
) -> Result<Option<NativeExpr>, NativeIrError> {
    managed_instruction_expr_with_variants(instruction, args, None)
}

/// Lowers managed ABI instructions with optional finite-sum authority.
pub(in crate::compiler::native_ir) fn managed_instruction_expr_with_variants(
    instruction: &IrInstruction,
    args: Vec<NativeExpr>,
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
) -> Result<Option<NativeExpr>, NativeIrError> {
    validate_managed_instruction_types(instruction)?;
    if instruction.opcode == "put-variant" {
        let variants = variants.ok_or_else(|| {
            "error[self_host.native.variant_authority]: put-variant has no admitted variant catalog"
                .to_string()
        })?;
        return variant_expr(instruction, args, variants).map(Some);
    }
    if let Some(expression) =
        collection_expr(&instruction.opcode, &instruction.type_name, args.clone())?
    {
        return Ok(Some(expression));
    }
    if let Some(expression) =
        aggregate_expr(&instruction.opcode, &instruction.type_name, args.clone())?
    {
        return Ok(Some(expression));
    }
    if instruction.opcode != "get-tuple-element" {
        return Ok(None);
    }
    if args.len() != 2 || instruction.operands.len() != 2 {
        return Err((format!(
            "error[self_host.native.tuple_projection_arity]: get-tuple-element expects tuple and index, found {} operand(s)",
            instruction.operands.len()
        )).into());
    }
    let source_type = instruction
        .operand_types
        .first()
        .and_then(Option::as_deref)
        .ok_or_else(|| {
            "error[self_host.native.tuple_projection_source_type]: tuple operand has no ABI-1.3 type"
                .to_string()
        })?;
    let source = parse_type(source_type)?;
    if source.constructor != "$Tuple" {
        return Err((format!(
            "error[self_host.native.tuple_projection_source_type]: expected tuple operand, found {source_type:?}"
        )).into());
    }
    let index = instruction.operands[1].parse::<usize>().map_err(|_| {
        format!(
            "error[self_host.native.tuple_projection_index]: tuple index {:?} is not a non-negative integer",
            instruction.operands[1]
        )
    })?;
    if index >= source.arguments.len() {
        return Err((format!(
            "error[self_host.native.tuple_projection_index]: tuple index {index} exceeds arity {}",
            source.arguments.len()
        ))
        .into());
    }
    let semantic = SemanticTypeId::from_canonical(&type_contract(&source, "")?)
        .map_err(|error| format!("error[self_host.native.tuple_projection_semantic]: {error}"))?;
    let result_type = native_type(&instruction.type_name)?.ok_or_else(|| {
        format!(
            "error[self_host.native.tuple_projection_result_type]: unsupported result type {:?}",
            instruction.type_name
        )
    })?;
    let encoded = if result_type.is_managed_reference() {
        encode_aggregate_field_operation(semantic, index)
    } else {
        encode_aggregate_scalar_field_operation(semantic, index)
    }
    .map_err(|error| format!("error[self_host.native.tuple_projection_abi]: {error}"))?;
    Ok(Some(NativeExpr::ManagedOperation {
        encoded: Arc::from(encoded),
        args: vec![args[0].clone()],
    }))
}

pub(super) fn declared_operand_type(
    instruction: &IrInstruction,
    index: usize,
) -> Result<NativeType, NativeIrError> {
    let type_name = instruction
        .operand_types
        .get(index)
        .and_then(Option::as_deref)
        .ok_or_else(|| {
            format!(
                "error[self_host.native.managed_operand_type]: opcode {:?} operand {index} has no type",
                instruction.opcode
            )
        })?;
    native_type(type_name)?.ok_or_else(|| {
        format!(
            "error[self_host.native.managed_operand_type]: opcode {:?} operand {index} has unsupported type {type_name:?}",
            instruction.opcode
        )
    }).map_err(NativeIrError::from)
}

pub(super) fn require_operand_type(
    instruction: &IrInstruction,
    index: usize,
    expected: NativeType,
) -> Result<(), NativeIrError> {
    let actual = declared_operand_type(instruction, index)?;
    (actual == expected).then_some(()).ok_or_else(|| {
        format!(
            "error[self_host.native.managed_operand_type]: opcode {:?} operand {index} does not match its aggregate contract",
            instruction.opcode
        )
    }).map_err(NativeIrError::from)
}

pub(super) fn validate_managed_instruction_types(
    instruction: &IrInstruction,
) -> Result<(), NativeIrError> {
    match instruction.opcode.as_str() {
        "put-list" | "put-list-cons" => {
            let list = parse_type(&instruction.type_name)?;
            let [element] = list.arguments.as_slice() else {
                return Err(
                    "error[self_host.native.list_type]: list instruction requires one element type"
                        .into(),
                );
            };
            let element = native_type_of(element)?.ok_or_else(|| {
                "error[self_host.native.list_type]: list element type is not native".to_string()
            })?;
            if instruction.opcode == "put-list" {
                for index in 0..instruction.operands.len() {
                    require_operand_type(instruction, index, element)?;
                }
            } else {
                require_operand_type(instruction, 0, element)?;
                require_operand_type(
                    instruction,
                    1,
                    native_type(&instruction.type_name)?.ok_or_else(|| {
                        "error[self_host.native.list_type]: list type is not native".to_string()
                    })?,
                )?;
            }
        }
        "put-tuple" => {
            let tuple = parse_type(&instruction.type_name)?;
            if tuple.constructor != "$Tuple" || tuple.arguments.len() != instruction.operands.len()
            {
                return Err(
                    "error[self_host.native.tuple_type]: tuple operands do not match tuple type"
                        .into(),
                );
            }
            for (index, field) in tuple.arguments.iter().enumerate() {
                let expected = native_type_of(field)?.ok_or_else(|| {
                    "error[self_host.native.tuple_type]: tuple field type is not native".to_string()
                })?;
                require_operand_type(instruction, index, expected)?;
            }
        }
        "put-array" => {
            let array = parse_type(&instruction.type_name)?;
            let [length, element] = array.arguments.as_slice() else {
                return Err("error[self_host.native.array_type]: fixed array requires size and element type"
                        .into());
            };
            if array.constructor != "Array" {
                return Err(
                    "error[self_host.native.array_type]: instruction result is not a fixed array"
                        .into(),
                );
            }
            let length = length.constructor.parse::<usize>().map_err(|_| {
                "error[self_host.native.array_size]: fixed array size is not concrete".to_string()
            })?;
            if length != instruction.operands.len() {
                return Err(
                    "error[self_host.native.array_arity]: operands do not match fixed array size"
                        .into(),
                );
            }
            let expected = native_type_of(element)?.ok_or_else(|| {
                "error[self_host.native.array_type]: element type is not native".to_string()
            })?;
            for index in 0..instruction.operands.len() {
                require_operand_type(instruction, index, expected)?;
            }
        }
        "get-tuple-element" => {
            require_operand_type(instruction, 1, NativeType::Int)?;
            let source_name = instruction
                .operand_types
                .first()
                .and_then(Option::as_deref)
                .ok_or_else(|| {
                    "error[self_host.native.tuple_projection_source_type]: missing tuple source type"
                        .to_string()
                })?;
            let source = parse_type(source_name)?;
            let index = instruction
                .operands
                .get(1)
                .ok_or_else(|| {
                    "error[self_host.native.tuple_projection_index]: missing tuple index"
                        .to_string()
                })?
                .parse::<usize>()
                .map_err(|_| {
                    "error[self_host.native.tuple_projection_index]: invalid tuple index"
                        .to_string()
                })?;
            let field = source.arguments.get(index).ok_or_else(|| {
                "error[self_host.native.tuple_projection_index]: tuple index is out of bounds"
                    .to_string()
            })?;
            let expected = native_type_of(field)?.ok_or_else(|| {
                "error[self_host.native.tuple_projection_result_type]: tuple field is not native"
                    .to_string()
            })?;
            let actual = native_type(&instruction.type_name)?.ok_or_else(|| {
                "error[self_host.native.tuple_projection_result_type]: result is not native"
                    .to_string()
            })?;
            if expected != actual {
                return Err("error[self_host.native.tuple_projection_result_type]: result does not match selected tuple field"
                        .into());
            }
        }
        _ => {}
    }
    Ok(())
}
