use super::*;

pub(super) fn binding_type_at_pattern(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    source_name: &str,
    path: &[usize],
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
) -> Result<AbiType, NativeIrError> {
    binding_type_at_pattern_with_catalogs(
        pattern,
        source,
        source_name,
        path,
        PatternCatalogs {
            variants,
            structs: None,
        },
    )
}

pub(super) fn binding_type_at_pattern_with_catalogs(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    source_name: &str,
    path: &[usize],
    catalogs: PatternCatalogs<'_>,
) -> Result<AbiType, NativeIrError> {
    let Some((index, rest)) = path.split_first() else {
        return Ok(source.clone());
    };
    let child = pattern.children.get(*index).ok_or_else(|| {
        "error[self_host.native.pattern_binding_shape]: binding path exceeds pattern arity"
            .to_string()
    })?;
    if pattern.kind == "string-capture-pattern" {
        if child.kind != "string-capture-segment" {
            return Err(("error[self_host.native.pattern_binding_shape]: string binding path selects a literal segment"
                    .to_string()).into());
        }
        let (_, kind) = string_capture_kind(&child.text)?;
        let type_name = match kind {
            StringCaptureKind::String => "String",
            StringCaptureKind::Int => "Int",
            StringCaptureKind::Float => "Float",
            StringCaptureKind::Bool => "Bool",
        };
        let binding_type = parse_type(type_name)?;
        return binding_type_at_pattern_with_catalogs(
            child,
            &binding_type,
            type_name,
            rest,
            catalogs,
        );
    }
    if pattern.kind == "tuple-pattern" {
        let child_type = source.arguments.get(*index).ok_or_else(|| {
            "error[self_host.native.pattern_binding_shape]: binding path exceeds tuple arity"
                .to_string()
        })?;
        return binding_type_at_pattern_with_catalogs(
            child,
            child_type,
            &abi_type_source(child_type),
            rest,
            catalogs,
        );
    }
    if pattern.kind == "list-pattern" {
        let element = source.arguments.first().ok_or_else(|| {
            "error[self_host.native.pattern_binding_shape]: List source has no element type"
                .to_string()
        })?;
        return binding_type_at_pattern_with_catalogs(
            child,
            element,
            &abi_type_source(element),
            rest,
            catalogs,
        );
    }
    if pattern.kind == "list-cons-pattern" {
        let child_type = if *index == 0 {
            source.arguments.first().ok_or_else(|| {
                "error[self_host.native.pattern_binding_shape]: List source has no element type"
                    .to_string()
            })?
        } else if *index == 1 {
            source
        } else {
            return Err(("error[self_host.native.pattern_binding_shape]: list-cons binding path exceeds arity"
                    .to_string()).into());
        };
        return binding_type_at_pattern_with_catalogs(
            child,
            child_type,
            &abi_type_source(child_type),
            rest,
            catalogs,
        );
    }
    if matches!(
        pattern.kind.as_str(),
        "struct-pattern" | "keyed-container-pattern" | "keyed-pattern-fields"
    ) {
        return binding_type_at_pattern_with_catalogs(child, source, source_name, rest, catalogs);
    }
    if pattern.kind == "keyed-pattern-field" {
        if *index != 0 {
            return Err(("error[self_host.native.pattern_binding_shape]: keyed field binding path is invalid"
                    .to_string()).into());
        }
        if is_map_type(source) {
            let value_type = &source.arguments[1];
            return binding_type_at_pattern_with_catalogs(
                child,
                value_type,
                &abi_type_source(value_type),
                rest,
                catalogs,
            );
        }
        let layout = admitted_struct_layout(source, catalogs)?;
        let declared = layout.get(&pattern.text).ok_or_else(|| {
            format!(
                "error[self_host.native.pattern_binding_shape]: struct {:?} has no field {:?}",
                source.constructor, pattern.text
            )
        })?;
        let field_type = parse_type(&declared.type_name)?;
        return binding_type_at_pattern_with_catalogs(
            child,
            &field_type,
            &declared.type_name,
            rest,
            catalogs,
        );
    }
    if pattern.kind == "constructor-pattern" {
        let catalog = catalogs.variants.ok_or_else(|| {
            "error[self_host.native.variant_authority]: constructor binding has no admitted variant catalog"
                .to_string()
        })?;
        let plan = crate::self_host_variant_adapter::plan_pattern(
            catalog,
            &pattern.text,
            source_name,
            pattern.children.len(),
        )
        .map_err(|error| format!("error[self_host.native.variant_pattern]: {error}"))?;
        let projection = plan.projections.get(*index).ok_or_else(|| {
            "error[self_host.native.pattern_binding_shape]: binding path exceeds variant arity"
                .to_string()
        })?;
        let child_type = parse_type(&projection.type_name)?;
        return binding_type_at_pattern_with_catalogs(
            child,
            &child_type,
            &projection.type_name,
            rest,
            catalogs,
        );
    }
    Err(
        ("error[self_host.native.pattern_binding_shape]: binding path crosses a terminal pattern"
            .to_string())
        .into(),
    )
}

pub(super) fn project_binding_path(
    mut value: NativeExpr,
    source: &AbiType,
    path: &[usize],
) -> Result<NativeExpr, NativeIrError> {
    let mut current = source;
    for index in path {
        if current.constructor != "$Tuple" {
            return Err(("error[self_host.native.pattern_binding_shape]: binding path crosses a non-tuple value"
                    .to_string()).into());
        }
        let child = current.arguments.get(*index).ok_or_else(|| {
            "error[self_host.native.pattern_binding_shape]: binding path exceeds tuple arity"
                .to_string()
        })?;
        let semantic = SemanticTypeId::from_canonical(&type_contract(current, "")?)
            .map_err(|error| format!("error[self_host.native.pattern_projection]: {error}"))?;
        let child_type = native_type_of(child)?.ok_or_else(|| {
            "error[self_host.native.pattern_projection]: binding type is not native".to_string()
        })?;
        let encoded = if child_type.is_managed_reference() {
            encode_aggregate_field_operation(semantic, *index)
        } else {
            encode_aggregate_scalar_field_operation(semantic, *index)
        }
        .map_err(|error| format!("error[self_host.native.pattern_projection]: {error}"))?;
        value = NativeExpr::ManagedOperation {
            encoded: Arc::from(encoded),
            args: vec![value],
        };
        current = child;
    }
    Ok(value)
}

pub(super) fn project_variant_field(
    value: NativeExpr,
    source: &AbiType,
    index: usize,
    child: &AbiType,
) -> Result<NativeExpr, NativeIrError> {
    let semantic = SemanticTypeId::from_canonical(&type_contract(source, "")?)
        .map_err(|error| format!("error[self_host.native.variant_projection]: {error}"))?;
    let child_type = native_type_of(child)?.ok_or_else(|| {
        "error[self_host.native.variant_projection]: payload type is not native".to_string()
    })?;
    let encoded = if child_type.is_managed_reference() {
        encode_aggregate_field_operation(semantic, index)
    } else {
        encode_aggregate_scalar_field_operation(semantic, index)
    }
    .map_err(|error| format!("error[self_host.native.variant_projection]: {error}"))?;
    Ok(NativeExpr::ManagedOperation {
        encoded: Arc::from(encoded),
        args: vec![value],
    })
}

pub(super) fn list_pattern_operation(
    source: &AbiType,
    value: NativeExpr,
    first: bool,
) -> Result<NativeExpr, NativeIrError> {
    let element = source.arguments.first().ok_or_else(|| {
        "error[self_host.native.pattern_binding_shape]: List source has no element type".to_string()
    })?;
    let element_native = native_type_of(element)?.ok_or_else(|| {
        "error[self_host.native.pattern_binding_shape]: List element type is not native".to_string()
    })?;
    let semantic = SemanticTypeId::from_canonical(&type_contract(source, "")?)
        .map_err(|error| format!("error[self_host.native.pattern_list_abi]: {error}"))?;
    let encoded = if first {
        encode_list_first_operation(semantic, element_native.is_managed_reference())
    } else {
        encode_list_rest_operation(semantic)
    };
    Ok(NativeExpr::ManagedOperation {
        encoded: Arc::from(encoded),
        args: vec![value],
    })
}

#[cfg(test)]
pub(super) fn project_pattern_binding_path(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    source_name: &str,
    value: NativeExpr,
    path: &[usize],
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
) -> Result<NativeExpr, NativeIrError> {
    project_pattern_binding_path_with_catalogs(
        pattern,
        source,
        source_name,
        value,
        path,
        PatternCatalogs {
            variants,
            structs: None,
        },
    )
}

pub(super) fn project_pattern_binding_path_with_catalogs(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    source_name: &str,
    value: NativeExpr,
    path: &[usize],
    catalogs: PatternCatalogs<'_>,
) -> Result<NativeExpr, NativeIrError> {
    let Some((index, rest)) = path.split_first() else {
        return Ok(value);
    };
    let child = pattern.children.get(*index).ok_or_else(|| {
        "error[self_host.native.pattern_binding_shape]: binding path exceeds pattern arity"
            .to_string()
    })?;
    if pattern.kind == "string-capture-pattern" {
        if child.kind != "string-capture-segment" {
            return Err(("error[self_host.native.pattern_binding_shape]: string binding path selects a literal segment"
                    .to_string()).into());
        }
        let selected = pattern.children[..*index]
            .iter()
            .filter(|segment| segment.kind == "string-capture-segment")
            .count();
        let (_, kind) = string_capture_kind(&child.text)?;
        let type_name = match kind {
            StringCaptureKind::String => "String",
            StringCaptureKind::Int => "Int",
            StringCaptureKind::Float => "Float",
            StringCaptureKind::Bool => "Bool",
        };
        let binding_type = parse_type(type_name)?;
        let decoded = decoded_string_pattern_segments(pattern)?;
        let segments = borrowed_string_pattern_segments(&decoded);
        let encoded = encode_string_pattern_extract_operation(&segments, selected)
            .map_err(|error| format!("error[self_host.native.string_pattern]: {error}"))?;
        return project_pattern_binding_path_with_catalogs(
            child,
            &binding_type,
            type_name,
            NativeExpr::ManagedOperation {
                encoded: Arc::from(encoded),
                args: vec![value],
            },
            rest,
            catalogs,
        );
    }
    if pattern.kind == "tuple-pattern" {
        let child_type = source.arguments.get(*index).ok_or_else(|| {
            "error[self_host.native.pattern_binding_shape]: binding path exceeds tuple arity"
                .to_string()
        })?;
        let projected = project_binding_path(value, source, &[*index])?;
        return project_pattern_binding_path_with_catalogs(
            child,
            child_type,
            &abi_type_source(child_type),
            projected,
            rest,
            catalogs,
        );
    }
    if pattern.kind == "list-pattern" {
        let element = source.arguments.first().ok_or_else(|| {
            "error[self_host.native.pattern_binding_shape]: List source has no element type"
                .to_string()
        })?;
        let mut current = value;
        for _ in 0..*index {
            current = list_pattern_operation(source, current, false)?;
        }
        let projected = list_pattern_operation(source, current, true)?;
        return project_pattern_binding_path_with_catalogs(
            child,
            element,
            &abi_type_source(element),
            projected,
            rest,
            catalogs,
        );
    }
    if pattern.kind == "list-cons-pattern" {
        let (child_type, projected) = if *index == 0 {
            let element = source.arguments.first().ok_or_else(|| {
                "error[self_host.native.pattern_binding_shape]: List source has no element type"
                    .to_string()
            })?;
            (element, list_pattern_operation(source, value, true)?)
        } else if *index == 1 {
            (source, list_pattern_operation(source, value, false)?)
        } else {
            return Err(("error[self_host.native.pattern_binding_shape]: list-cons binding path exceeds arity"
                    .to_string()).into());
        };
        return project_pattern_binding_path_with_catalogs(
            child,
            child_type,
            &abi_type_source(child_type),
            projected,
            rest,
            catalogs,
        );
    }
    if matches!(
        pattern.kind.as_str(),
        "struct-pattern" | "keyed-container-pattern" | "keyed-pattern-fields"
    ) {
        return project_pattern_binding_path_with_catalogs(
            child,
            source,
            source_name,
            value,
            rest,
            catalogs,
        );
    }
    if pattern.kind == "keyed-pattern-field" {
        if *index != 0 {
            return Err(("error[self_host.native.pattern_binding_shape]: keyed field binding path is invalid"
                    .to_string()).into());
        }
        if is_map_type(source) {
            let value_type = &source.arguments[1];
            let value_native = native_type_of(value_type)?.ok_or_else(|| {
                "error[self_host.native.pattern_binding_shape]: Map value type is not native"
                    .to_string()
            })?;
            let semantic = SemanticTypeId::from_canonical(&type_contract(source, "")?)
                .map_err(|error| format!("error[self_host.native.pattern_map_abi]: {error}"))?;
            let encoded_key = encode_string_literal(&pattern.text)
                .map_err(|error| format!("error[self_host.native.pattern_map_key]: {error}"))?;
            let projected = NativeExpr::ManagedOperation {
                encoded: Arc::from(encode_map_get_operation(
                    semantic,
                    value_native.is_managed_reference(),
                )),
                args: vec![
                    value,
                    NativeExpr::ManagedLiteral {
                        encoded: Arc::from(encoded_key),
                    },
                ],
            };
            return project_pattern_binding_path_with_catalogs(
                child,
                value_type,
                &abi_type_source(value_type),
                projected,
                rest,
                catalogs,
            );
        }
        let layout = admitted_struct_layout(source, catalogs)?;
        let declared = layout.get(&pattern.text).ok_or_else(|| {
            format!(
                "error[self_host.native.pattern_binding_shape]: struct {:?} has no field {:?}",
                source.constructor, pattern.text
            )
        })?;
        let field_type = parse_type(&declared.type_name)?;
        let field_native = native_type_of(&field_type)?.ok_or_else(|| {
            "error[self_host.native.pattern_binding_shape]: struct field type is not native"
                .to_string()
        })?;
        let semantic = SemanticTypeId::from_canonical(&type_contract(source, "")?)
            .map_err(|error| format!("error[self_host.native.pattern_struct_abi]: {error}"))?;
        let encoded = if field_native.is_managed_reference() {
            encode_aggregate_field_operation(semantic, declared.index)
        } else {
            encode_aggregate_scalar_field_operation(semantic, declared.index)
        }
        .map_err(|error| format!("error[self_host.native.pattern_struct_abi]: {error}"))?;
        return project_pattern_binding_path_with_catalogs(
            child,
            &field_type,
            &declared.type_name,
            NativeExpr::ManagedOperation {
                encoded: Arc::from(encoded),
                args: vec![value],
            },
            rest,
            catalogs,
        );
    }
    if pattern.kind == "constructor-pattern" {
        let catalog = catalogs.variants.ok_or_else(|| {
            "error[self_host.native.variant_authority]: constructor binding has no admitted variant catalog"
                .to_string()
        })?;
        let plan = crate::self_host_variant_adapter::plan_pattern(
            catalog,
            &pattern.text,
            source_name,
            pattern.children.len(),
        )
        .map_err(|error| format!("error[self_host.native.variant_pattern]: {error}"))?;
        let projection = plan.projections.get(*index).ok_or_else(|| {
            "error[self_host.native.pattern_binding_shape]: binding path exceeds variant arity"
                .to_string()
        })?;
        let child_type = parse_type(&projection.type_name)?;
        let projected = project_variant_field(
            value,
            source,
            usize::try_from(projection.index).map_err(|_| {
                "error[self_host.native.variant_projection]: payload index overflows".to_string()
            })?,
            &child_type,
        )?;
        return project_pattern_binding_path_with_catalogs(
            child,
            &child_type,
            &projection.type_name,
            projected,
            rest,
            catalogs,
        );
    }
    Err(
        ("error[self_host.native.pattern_binding_shape]: binding path crosses a terminal pattern"
            .to_string())
        .into(),
    )
}

pub(super) fn binding_path_crosses_list(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    path: &[usize],
) -> bool {
    let Some((index, rest)) = path.split_first() else {
        return false;
    };
    if matches!(
        pattern.kind.as_str(),
        "list-pattern"
            | "list-cons-pattern"
            | "constructor-pattern"
            | "keyed-container-pattern"
            | "string-capture-pattern"
    ) {
        return true;
    }
    pattern
        .children
        .get(*index)
        .is_some_and(|child| binding_path_crosses_list(child, rest))
}

pub(super) fn guarded_binding(
    matched: NativeExpr,
    projected: NativeExpr,
    binding_type: NativeType,
    binding_abi: &AbiType,
) -> Result<NativeExpr, NativeIrError> {
    let fallback = match binding_type {
        NativeType::Unit => NativeExpr::Unit,
        NativeType::Int => NativeExpr::Int(0),
        NativeType::Float => NativeExpr::Float(0_f64.to_bits()),
        NativeType::Bool => NativeExpr::Bool(false),
        NativeType::Atom => NativeExpr::AtomLiteral(Arc::from("$pattern-unmatched")),
        NativeType::StringRef => NativeExpr::ManagedLiteral {
            encoded: Arc::from(
                encode_string_literal("")
                    .map_err(|error| format!("error[self_host.native.pattern_default]: {error}"))?,
            ),
        },
        NativeType::BinaryRef => NativeExpr::ManagedLiteral {
            encoded: Arc::from(
                encode_binary_literal(&[])
                    .map_err(|error| format!("error[self_host.native.pattern_default]: {error}"))?,
            ),
        },
        NativeType::ManagedRef(semantic) if is_list_type(binding_abi) => {
            NativeExpr::ManagedOperation {
                encoded: Arc::from(encode_list_from_elements_operation(semantic)),
                args: Vec::new(),
            }
        }
        _ => {
            return Err(("error[self_host.native.pattern_binding_shape]: managed binding has no typed deferred default"
                    .to_string()).into())
        }
    };
    Ok(NativeExpr::If {
        clauses: vec![(matched, projected), (NativeExpr::Bool(true), fallback)],
    })
}

pub(super) fn pattern_literal(
    text: &str,
    source_type: NativeType,
) -> Result<NativeExpr, NativeIrError> {
    match source_type {
        NativeType::Int => text
            .parse::<i64>()
            .map(NativeExpr::Int)
            .map_err(|_| NativeIrError::from(format!("error[self_host.native.pattern_literal]: invalid Int {text:?}"))),
        NativeType::Float => text
            .parse::<f64>()
            .map(|value| NativeExpr::Float(value.to_bits()))
            .map_err(|_| {
                NativeIrError::from(format!("error[self_host.native.pattern_literal]: invalid Float {text:?}"))
            }),
        NativeType::Bool if text == "true" => Ok(NativeExpr::Bool(true)),
        NativeType::Bool if text == "false" => Ok(NativeExpr::Bool(false)),
        NativeType::Unit if matches!(text, "Unit" | "unit" | "()") => Ok(NativeExpr::Unit),
        _ => Err((format!(
            "error[self_host.native.pattern_literal]: literal {text:?} is unsupported for {source_type:?}"
        )).into()),
    }
}
