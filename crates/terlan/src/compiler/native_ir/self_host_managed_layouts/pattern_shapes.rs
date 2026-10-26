use super::*;

pub(super) fn pattern_canonical(instruction: &IrInstruction) -> Result<String, NativeIrError> {
    let source_type = instruction
        .operand_types
        .first()
        .and_then(Option::as_deref)
        .ok_or_else(|| {
            "error[self_host.native.pattern_source_type]: prepare-pattern subject has no type"
                .to_string()
        })?;
    let source = type_contract(&parse_type(source_type)?, "")?;
    let mut canonical = format!("PatternMatch({}:{};", source.len(), source);
    for operand in &instruction.operands[1..] {
        canonical.push_str(&format!("{}:{};", operand.len(), operand));
    }
    canonical.push(')');
    Ok(canonical)
}

pub(super) fn pattern_layout(
    instruction: &IrInstruction,
) -> Result<(Arc<ManagedAggregateDescriptor>, Arc<[u8]>), NativeIrError> {
    let (_subject, pattern) =
        super::super::self_host_pattern_ir::decode_prepare_pattern(instruction)?;
    let source_type = instruction
        .operand_types
        .first()
        .and_then(Option::as_deref)
        .ok_or_else(|| {
            "error[self_host.native.pattern_source_type]: prepare-pattern subject has no type"
                .to_string()
        })?;
    let source = parse_type(source_type)?;
    if !super::super::self_host_pattern_ir::prepare_pattern_deferred(instruction) {
        validate_pattern_shape(&pattern, &source, source_type, None)?;
    }
    let mut fields = vec![("matched".into(), ManagedFieldType::Bool)];
    if super::super::self_host_pattern_ir::prepare_pattern_deferred(instruction) {
        fields.push(("subject".into(), managed_field_type_of(&source, "")?));
    } else {
        fields.extend(
            pattern
                .bindings()
                .into_iter()
                .enumerate()
                .map(|(index, binding)| {
                    let field_type = binding_type_at_pattern(
                        &pattern,
                        &source,
                        source_type,
                        &binding.path,
                        None,
                    )?;
                    Ok((
                        format!("binding.{index}.{}", binding.name),
                        managed_field_type_of(&field_type, "")?,
                    ))
                })
                .collect::<Result<Vec<_>, NativeIrError>>()?,
        );
    }
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::record(&pattern_canonical(instruction)?, fields)
            .map_err(|error| format!("error[self_host.native.pattern_layout]: {error}"))?,
    );
    let encoded = Arc::<[u8]>::from(
        encode_aggregate_layout(&descriptor)
            .map_err(|error| format!("error[self_host.native.pattern_layout_abi]: {error}"))?,
    );
    Ok((descriptor, encoded))
}

pub(super) fn validate_pattern_shape(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    source_name: &str,
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
) -> Result<(), NativeIrError> {
    validate_pattern_shape_with_catalogs(
        pattern,
        source,
        source_name,
        PatternCatalogs {
            variants,
            structs: None,
        },
    )
}

pub(super) fn validate_pattern_shape_with_catalogs(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    source_name: &str,
    catalogs: PatternCatalogs<'_>,
) -> Result<(), NativeIrError> {
    if pattern.kind == "string-capture-pattern" {
        return validate_string_capture_pattern(pattern, source);
    }
    if pattern.kind == "struct-pattern" {
        return validate_struct_pattern_shape(pattern, source, catalogs);
    }
    if pattern.kind == "keyed-container-pattern" {
        return validate_keyed_pattern_shape(pattern, source, catalogs);
    }
    if matches!(pattern.kind.as_str(), "list-pattern" | "list-cons-pattern") {
        if !is_list_type(source) {
            return Err(
                ("error[self_host.native.pattern_shape]: list pattern requires a List source"
                    .to_string())
                .into(),
            );
        }
        if pattern.kind == "list-cons-pattern" && pattern.children.len() != 2 {
            return Err(
                ("error[self_host.native.pattern_shape]: list-cons pattern requires head and tail"
                    .to_string())
                .into(),
            );
        }
        let element = source.arguments.first().ok_or_else(|| {
            "error[self_host.native.pattern_shape]: List source has no element type".to_string()
        })?;
        for (index, child) in pattern.children.iter().enumerate() {
            let child_source = if pattern.kind == "list-cons-pattern" && index == 1 {
                source
            } else {
                element
            };
            validate_pattern_shape_with_catalogs(
                child,
                child_source,
                &abi_type_source(child_source),
                catalogs,
            )?;
        }
        return Ok(());
    }
    if pattern.irrefutable() {
        return pattern.children.is_empty().then_some(()).ok_or_else(|| {
            NativeIrError::from("error[self_host.native.pattern_shape]: leaf pattern has children")
        });
    }
    if pattern.kind == "literal-pattern" {
        return pattern.children.is_empty().then_some(()).ok_or_else(|| {
            NativeIrError::from(
                "error[self_host.native.pattern_shape]: literal pattern has children",
            )
        });
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
        for (child, projection) in pattern.children.iter().zip(plan.projections.iter()) {
            let child_type = parse_type(&projection.type_name)?;
            validate_pattern_shape_with_catalogs(
                child,
                &child_type,
                &projection.type_name,
                catalogs,
            )?;
        }
        return Ok(());
    }
    if pattern.kind != "tuple-pattern" || source.constructor != "$Tuple" {
        return Err((format!(
            "error[self_host.native.pattern_kind]: pattern kind {:?} has no direct native lowering for this subject",
            pattern.kind
        )).into());
    }
    if pattern.children.len() != source.arguments.len() {
        return Err((format!(
            "error[self_host.native.pattern_shape]: tuple pattern has {} child(ren), expected {}",
            pattern.children.len(),
            source.arguments.len()
        ))
        .into());
    }
    for (child, child_type) in pattern.children.iter().zip(&source.arguments) {
        validate_pattern_shape_with_catalogs(
            child,
            child_type,
            &abi_type_source(child_type),
            catalogs,
        )?;
        if !structurally_supported(child) {
            return Err("error[self_host.native.pattern_kind]: nested refutable tuple patterns are not yet lowered"
                    .into());
        }
    }
    Ok(())
}

pub(super) fn structurally_supported(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
) -> bool {
    pattern.irrefutable()
        || pattern.kind == "literal-pattern"
        || (pattern.kind == "tuple-pattern" && pattern.children.iter().all(structurally_supported))
        || (pattern.kind == "constructor-pattern"
            && pattern.children.iter().all(structurally_supported))
        || (matches!(pattern.kind.as_str(), "list-pattern" | "list-cons-pattern")
            && pattern.children.iter().all(structurally_supported))
        || (matches!(
            pattern.kind.as_str(),
            "keyed-container-pattern"
                | "struct-pattern"
                | "keyed-pattern-fields"
                | "keyed-pattern-field"
        ) && pattern.children.iter().all(structurally_supported))
        || (matches!(
            pattern.kind.as_str(),
            "string-capture-pattern" | "string-literal-segment" | "string-capture-segment"
        ) && pattern.children.iter().all(structurally_supported))
}

pub(super) fn is_list_type(source: &AbiType) -> bool {
    source.arguments.len() == 1
        && (source.constructor == "List" || source.constructor.ends_with(".List"))
}

pub(super) fn is_map_type(source: &AbiType) -> bool {
    source.arguments.len() == 2
        && (source.constructor == "Map" || source.constructor.ends_with(".Map"))
}

pub(super) fn string_capture_kind(text: &str) -> Result<(&str, StringCaptureKind), NativeIrError> {
    let (name, annotation) = text
        .split_once(':')
        .map_or((text.trim(), "String"), |(name, annotation)| {
            (name.trim(), annotation.trim())
        });
    let mut characters = name.chars();
    let valid_name = characters
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_lowercase())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric());
    if !valid_name {
        return Err(
            ("error[self_host.native.string_pattern]: capture name must be a lower-case binding"
                .to_string())
            .into(),
        );
    }
    let kind = match annotation {
        "String" => StringCaptureKind::String,
        "Int" => StringCaptureKind::Int,
        "Float" => StringCaptureKind::Float,
        "Bool" => StringCaptureKind::Bool,
        _ => {
            return Err((format!(
                "error[self_host.native.string_pattern]: no native capture converter for {annotation:?}"
            )).into())
        }
    };
    Ok((name, kind))
}

pub(super) fn validate_string_capture_pattern(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
) -> Result<(), NativeIrError> {
    if !(source.constructor == "String" || source.constructor.ends_with(".String"))
        || !source.arguments.is_empty()
    {
        return Err(
            ("error[self_host.native.string_pattern]: string capture requires a String source"
                .to_string())
            .into(),
        );
    }
    if pattern.children.is_empty() {
        return Err(
            ("error[self_host.native.string_pattern]: string capture has no segments".to_string())
                .into(),
        );
    }
    let mut previous_capture = false;
    let mut capture_count = 0_usize;
    let mut names = std::collections::HashSet::new();
    for segment in &pattern.children {
        if !segment.children.is_empty() {
            return Err(
                ("error[self_host.native.string_pattern]: string segments must be leaves"
                    .to_string())
                .into(),
            );
        }
        if segment.kind == "string-literal-segment" {
            if segment.text.is_empty() {
                return Err(
                    ("error[self_host.native.string_pattern]: literal segment must not be empty"
                        .to_string())
                    .into(),
                );
            }
            previous_capture = false;
            continue;
        }
        if segment.kind != "string-capture-segment" {
            return Err((format!(
                "error[self_host.native.string_pattern]: unsupported segment {:?}",
                segment.kind
            ))
            .into());
        }
        if previous_capture {
            return Err(("error[self_host.native.string_pattern]: adjacent captures require a literal separator"
                    .to_string()).into());
        }
        let (name, _) = string_capture_kind(&segment.text)?;
        if name != "_" && !names.insert(name) {
            return Err((format!(
                "error[self_host.native.string_pattern]: duplicate capture {name:?}"
            ))
            .into());
        }
        capture_count += 1;
        previous_capture = true;
    }
    if capture_count == 0 {
        return Err(
            ("error[self_host.native.string_pattern]: pattern has no capture segment".to_string())
                .into(),
        );
    }
    Ok(())
}

pub(super) fn decoded_string_pattern_segments(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
) -> Result<Vec<DecodedStringPatternSegment>, NativeIrError> {
    pattern
        .children
        .iter()
        .map(|segment| {
            if segment.kind == "string-literal-segment" {
                let quoted = format!("\"{}\"", segment.text);
                return super::super::self_host_adapter::decoded_string_literal(&quoted)
                    .map(DecodedStringPatternSegment::Literal)
                    .map_err(NativeIrError::from);
            }
            string_capture_kind(&segment.text)
                .map(|(_, kind)| DecodedStringPatternSegment::Capture(kind))
        })
        .collect()
}

pub(super) fn borrowed_string_pattern_segments(
    segments: &[DecodedStringPatternSegment],
) -> Vec<StringPatternSegment<'_>> {
    segments
        .iter()
        .map(|segment| match segment {
            DecodedStringPatternSegment::Literal(text) => StringPatternSegment::Literal(text),
            DecodedStringPatternSegment::Capture(kind) => StringPatternSegment::Capture(*kind),
        })
        .collect()
}

pub(super) fn keyed_pattern_fields(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
) -> Result<&[super::super::self_host_pattern_ir::PatternNode], NativeIrError> {
    let [fields] = pattern.children.as_slice() else {
        return Err(
            ("error[self_host.native.pattern_shape]: keyed container requires one field wrapper"
                .to_string())
            .into(),
        );
    };
    if fields.kind != "keyed-pattern-fields" || !fields.text.is_empty() {
        return Err(
            ("error[self_host.native.pattern_shape]: keyed container field wrapper is invalid"
                .to_string())
            .into(),
        );
    }
    Ok(&fields.children)
}

pub(super) fn validate_keyed_pattern_shape(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    catalogs: PatternCatalogs<'_>,
) -> Result<(), NativeIrError> {
    if !is_map_type(source) {
        return Err(
            ("error[self_host.native.pattern_shape]: keyed container requires a Map source"
                .to_string())
            .into(),
        );
    }
    let key = &source.arguments[0];
    if !(key.constructor == "String" || key.constructor.ends_with(".String")) {
        return Err(
            ("error[self_host.native.pattern_shape]: keyed container requires a String-keyed Map"
                .to_string())
            .into(),
        );
    }
    let value = &source.arguments[1];
    let mut names = std::collections::HashSet::new();
    for field in keyed_pattern_fields(pattern)? {
        if field.kind != "keyed-pattern-field" || field.text.is_empty() || field.children.len() != 1
        {
            return Err(("error[self_host.native.pattern_shape]: keyed pattern field requires one named value pattern"
                    .to_string()).into());
        }
        if !names.insert(field.text.as_str()) {
            return Err((format!(
                "error[self_host.native.pattern_shape]: duplicate keyed pattern field {:?}",
                field.text
            ))
            .into());
        }
        validate_pattern_shape_with_catalogs(
            &field.children[0],
            value,
            &abi_type_source(value),
            catalogs,
        )?;
    }
    Ok(())
}

pub(super) fn struct_pattern_fields<'a>(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    catalogs: PatternCatalogs<'a>,
) -> Result<
    &'a std::collections::HashMap<String, super::super::self_host_adapter::LocalStructField>,
    String,
> {
    if pattern.text.is_empty()
        || (pattern.text != source.constructor
            && !source.constructor.ends_with(&format!(".{}", pattern.text)))
    {
        return Err(format!(
            "error[self_host.native.pattern_shape]: struct pattern {:?} disagrees with source type {:?}",
            pattern.text, source.constructor
        ));
    }
    admitted_struct_layout(source, catalogs)
}

pub(super) fn admitted_struct_layout<'a>(
    source: &AbiType,
    catalogs: PatternCatalogs<'a>,
) -> Result<
    &'a std::collections::HashMap<String, super::super::self_host_adapter::LocalStructField>,
    String,
> {
    catalogs
        .structs
        .and_then(|structs| structs.get(&source.constructor))
        .ok_or_else(|| {
            format!(
                "error[self_host.native.pattern_shape]: struct {:?} has no admitted application layout",
                source.constructor
            )
        })
}

pub(super) fn validate_struct_pattern_shape(
    pattern: &super::super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    catalogs: PatternCatalogs<'_>,
) -> Result<(), NativeIrError> {
    let layout = struct_pattern_fields(pattern, source, catalogs)?;
    let mut names = std::collections::HashSet::new();
    for field in keyed_pattern_fields(pattern)? {
        if field.kind != "keyed-pattern-field" || field.text.is_empty() || field.children.len() != 1
        {
            return Err(("error[self_host.native.pattern_shape]: struct pattern field requires one named value pattern"
                    .to_string()).into());
        }
        if !names.insert(field.text.as_str()) {
            return Err((format!(
                "error[self_host.native.pattern_shape]: duplicate struct pattern field {:?}",
                field.text
            ))
            .into());
        }
        let declared = layout.get(&field.text).ok_or_else(|| {
            format!(
                "error[self_host.native.pattern_shape]: struct {:?} has no field {:?}",
                source.constructor, field.text
            )
        })?;
        let field_type = parse_type(&declared.type_name)?;
        validate_pattern_shape_with_catalogs(
            &field.children[0],
            &field_type,
            &declared.type_name,
            catalogs,
        )?;
    }
    Ok(())
}
