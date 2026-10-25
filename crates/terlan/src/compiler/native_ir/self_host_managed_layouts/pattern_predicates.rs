use super::*;

#[allow(dead_code)]
pub(super) fn pattern_predicate(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    value: NativeExpr,
    source: &AbiType,
    source_name: &str,
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
) -> Result<NativeExpr, String> {
    pattern_predicate_with_catalogs(
        pattern,
        value,
        source,
        source_name,
        PatternCatalogs {
            variants,
            structs: None,
        },
    )
}

pub(super) fn pattern_predicate_with_catalogs(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    value: NativeExpr,
    source: &AbiType,
    source_name: &str,
    catalogs: PatternCatalogs<'_>,
) -> Result<NativeExpr, String> {
    if pattern.kind == "string-capture-pattern" {
        let decoded = decoded_string_pattern_segments(pattern)?;
        let segments = borrowed_string_pattern_segments(&decoded);
        let encoded = encode_string_pattern_matches_operation(&segments)
            .map_err(|error| format!("error[self_host.native.string_pattern]: {error}"))?;
        return Ok(NativeExpr::ManagedOperation {
            encoded: Arc::from(encoded),
            args: vec![value],
        });
    }
    if pattern.irrefutable() {
        return Ok(NativeExpr::Bool(true));
    }
    if pattern.kind == "literal-pattern" {
        let source_type = native_type_of(source)?.ok_or_else(|| {
            "error[self_host.native.pattern_literal]: literal source type is not native".to_string()
        })?;
        if source_type == NativeType::StringRef {
            if !pattern.text.starts_with('"') || !pattern.text.ends_with('"') {
                return Err(
                    "error[self_host.native.pattern_literal]: String pattern metadata must be quoted"
                        .to_string(),
                );
            }
            let decoded = super::super::self_host_adapter::decoded_string_literal(&pattern.text)?;
            let encoded = encode_string_literal(&decoded)
                .map_err(|error| format!("error[self_host.native.pattern_literal]: {error}"))?;
            return Ok(NativeExpr::ManagedOperation {
                encoded: Arc::from(encode_string_equal_operation()),
                args: vec![
                    value,
                    NativeExpr::ManagedLiteral {
                        encoded: Arc::from(encoded),
                    },
                ],
            });
        }
        return Ok(NativeExpr::Binary {
            operator: NativeBinaryOperator::Equal,
            operand_type: source_type,
            left: Box::new(value),
            right: Box::new(pattern_literal(&pattern.text, source_type)?),
        });
    }
    if matches!(pattern.kind.as_str(), "list-pattern" | "list-cons-pattern") {
        return list_pattern_predicate(pattern, value, source, source_name, catalogs);
    }
    if pattern.kind == "keyed-container-pattern" {
        return keyed_pattern_predicate(pattern, value, source, catalogs);
    }
    if pattern.kind == "struct-pattern" {
        return struct_pattern_predicate(pattern, value, source, catalogs);
    }
    if pattern.kind == "constructor-pattern" {
        let catalog = catalogs.variants.ok_or_else(|| {
            "error[self_host.native.variant_authority]: constructor pattern has no admitted variant catalog"
                .to_string()
        })?;
        let plan = crate::self_host_variant_adapter::plan_pattern(
            catalog,
            &pattern.text,
            source_name,
            pattern.children.len(),
        )
        .map_err(|error| format!("error[self_host.native.variant_pattern]: {error}"))?;
        let semantic = SemanticTypeId::from_canonical(&type_contract(source, "")?)
            .map_err(|error| format!("error[self_host.native.variant_pattern]: {error}"))?;
        let mut predicate = NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_managed_variant_is_operation(
                semantic,
                plan.descriptor.discriminant,
            )),
            args: vec![value.clone()],
        };
        for (child, projection) in pattern.children.iter().zip(plan.projections.iter()) {
            let child_type = parse_type(&projection.type_name)?;
            let projected = project_variant_field(
                value.clone(),
                source,
                usize::try_from(projection.index).map_err(|_| {
                    "error[self_host.native.variant_pattern]: payload index overflows".to_string()
                })?,
                &child_type,
            )?;
            predicate = bool_and(
                predicate,
                pattern_predicate_with_catalogs(
                    child,
                    projected,
                    &child_type,
                    &projection.type_name,
                    catalogs,
                )?,
            );
        }
        return Ok(predicate);
    }
    if pattern.kind != "tuple-pattern" || source.constructor != "$Tuple" {
        return Err(format!(
            "error[self_host.native.pattern_kind]: unsupported recursive pattern {:?}",
            pattern.kind
        ));
    }
    let mut predicate = NativeExpr::Bool(true);
    for (index, (child, child_type)) in pattern
        .children
        .iter()
        .zip(source.arguments.iter())
        .enumerate()
    {
        let child_value = project_binding_path(value.clone(), source, &[index])?;
        predicate = bool_and(
            predicate,
            pattern_predicate_with_catalogs(
                child,
                child_value,
                child_type,
                &abi_type_source(child_type),
                catalogs,
            )?,
        );
    }
    Ok(predicate)
}

pub(super) fn keyed_pattern_predicate(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    value: NativeExpr,
    source: &AbiType,
    catalogs: PatternCatalogs<'_>,
) -> Result<NativeExpr, String> {
    let value_type = source.arguments.get(1).ok_or_else(|| {
        "error[self_host.native.pattern_shape]: Map source has no value type".to_string()
    })?;
    let value_native = native_type_of(value_type)?.ok_or_else(|| {
        "error[self_host.native.pattern_shape]: Map value type is not native".to_string()
    })?;
    let semantic = SemanticTypeId::from_canonical(&type_contract(source, "")?)
        .map_err(|error| format!("error[self_host.native.pattern_map_abi]: {error}"))?;
    let mut predicate = NativeExpr::Bool(true);
    for field in keyed_pattern_fields(pattern)? {
        let encoded_key = encode_string_literal(&field.text)
            .map_err(|error| format!("error[self_host.native.pattern_map_key]: {error}"))?;
        let key = NativeExpr::ManagedLiteral {
            encoded: Arc::from(encoded_key),
        };
        let present = NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_map_contains_operation(semantic)),
            args: vec![value.clone(), key.clone()],
        };
        let projected = NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_map_get_operation(
                semantic,
                value_native.is_managed_reference(),
            )),
            args: vec![value.clone(), key],
        };
        let child = pattern_predicate_with_catalogs(
            &field.children[0],
            projected,
            value_type,
            &abi_type_source(value_type),
            catalogs,
        )?;
        predicate = bool_and(predicate, bool_and(present, child));
    }
    Ok(predicate)
}

pub(super) fn struct_pattern_predicate(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    value: NativeExpr,
    source: &AbiType,
    catalogs: PatternCatalogs<'_>,
) -> Result<NativeExpr, String> {
    let layout = struct_pattern_fields(pattern, source, catalogs)?;
    let semantic = SemanticTypeId::from_canonical(&type_contract(source, "")?)
        .map_err(|error| format!("error[self_host.native.pattern_struct_abi]: {error}"))?;
    let mut predicate = NativeExpr::Bool(true);
    for field in keyed_pattern_fields(pattern)? {
        let declared = layout.get(&field.text).ok_or_else(|| {
            format!(
                "error[self_host.native.pattern_shape]: struct {:?} has no field {:?}",
                source.constructor, field.text
            )
        })?;
        let field_type = parse_type(&declared.type_name)?;
        let native = native_type_of(&field_type)?.ok_or_else(|| {
            "error[self_host.native.pattern_shape]: struct field type is not native".to_string()
        })?;
        let encoded = if native.is_managed_reference() {
            encode_aggregate_field_operation(semantic, declared.index)
        } else {
            encode_aggregate_scalar_field_operation(semantic, declared.index)
        }
        .map_err(|error| format!("error[self_host.native.pattern_struct_abi]: {error}"))?;
        let projected = NativeExpr::ManagedOperation {
            encoded: Arc::from(encoded),
            args: vec![value.clone()],
        };
        predicate = bool_and(
            predicate,
            pattern_predicate_with_catalogs(
                &field.children[0],
                projected,
                &field_type,
                &declared.type_name,
                catalogs,
            )?,
        );
    }
    Ok(predicate)
}

pub(super) fn list_pattern_predicate(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    value: NativeExpr,
    source: &AbiType,
    _source_name: &str,
    catalogs: PatternCatalogs<'_>,
) -> Result<NativeExpr, String> {
    if !is_list_type(source) {
        return Err(
            "error[self_host.native.pattern_shape]: list pattern requires a List source"
                .to_string(),
        );
    }
    let element = source.arguments.first().ok_or_else(|| {
        "error[self_host.native.pattern_shape]: List source has no element type".to_string()
    })?;
    let element_native = native_type_of(element)?.ok_or_else(|| {
        "error[self_host.native.pattern_shape]: List element type is not native".to_string()
    })?;
    let semantic = SemanticTypeId::from_canonical(&type_contract(source, "")?)
        .map_err(|error| format!("error[self_host.native.pattern_list_abi]: {error}"))?;
    let empty = |list: NativeExpr| NativeExpr::ManagedOperation {
        encoded: Arc::from(encode_list_is_empty_operation(semantic)),
        args: vec![list],
    };
    let first = |list: NativeExpr| NativeExpr::ManagedOperation {
        encoded: Arc::from(encode_list_first_operation(
            semantic,
            element_native.is_managed_reference(),
        )),
        args: vec![list],
    };
    let rest = |list: NativeExpr| NativeExpr::ManagedOperation {
        encoded: Arc::from(encode_list_rest_operation(semantic)),
        args: vec![list],
    };

    if pattern.kind == "list-cons-pattern" {
        let nonempty = NativeExpr::Not(Box::new(empty(value.clone())));
        let head = pattern_predicate_with_catalogs(
            &pattern.children[0],
            first(value.clone()),
            element,
            &abi_type_source(element),
            catalogs,
        )?;
        let tail = pattern_predicate_with_catalogs(
            &pattern.children[1],
            rest(value),
            source,
            &abi_type_source(source),
            catalogs,
        )?;
        return Ok(bool_and(nonempty, bool_and(head, tail)));
    }

    let mut current = value;
    let mut predicate = NativeExpr::Bool(true);
    for child in &pattern.children {
        predicate = bool_and(predicate, NativeExpr::Not(Box::new(empty(current.clone()))));
        predicate = bool_and(
            predicate,
            pattern_predicate_with_catalogs(
                child,
                first(current.clone()),
                element,
                &abi_type_source(element),
                catalogs,
            )?,
        );
        current = rest(current);
    }
    Ok(bool_and(predicate, empty(current)))
}
