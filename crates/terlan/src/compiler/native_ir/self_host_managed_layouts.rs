//! Managed aggregate metadata emitted from self-host ABI struct declarations.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_field_operation, encode_aggregate_layout,
    encode_aggregate_replace_field_operation, encode_aggregate_scalar_field_operation,
    encode_binary_literal, encode_collection_layout, encode_list_first_operation,
    encode_list_from_elements_operation, encode_list_is_empty_operation,
    encode_list_prepend_operation, encode_list_rest_operation, encode_managed_variant_is_operation,
    encode_map_contains_operation, encode_map_get_operation, encode_string_equal_operation,
    encode_string_literal, encode_string_pattern_extract_operation,
    encode_string_pattern_matches_operation, ManagedAggregateDescriptor,
    ManagedCollectionDescriptor, ManagedFieldType, ManagedStringCaptureKind as StringCaptureKind,
    ManagedStringPatternSegment as StringPatternSegment, SemanticTypeId,
};
use crate::self_host_ir::{BackendEnvelope, IrInstruction, IrStructField};

use super::{NativeBinaryOperator, NativeExpr, NativeType};

/// Lowers application-wide ABI struct declarations into NativeIR image metadata.
pub(super) fn encoded_struct_layouts(
    envelopes: &[BackendEnvelope],
) -> Result<Vec<Arc<[u8]>>, String> {
    let mut layouts = BTreeMap::<String, Arc<[u8]>>::new();
    for envelope in envelopes {
        for layout in &envelope.module.structs {
            let canonical = nominal_contract(&layout.name, &envelope.module.name)?;
            let fields = layout
                .fields
                .iter()
                .map(|field| lower_field(field, &envelope.module.name, &layout.name))
                .collect::<Result<Vec<_>, _>>()?;
            let descriptor =
                ManagedAggregateDescriptor::record(&canonical, fields).map_err(|error| {
                    format!(
                        "error[self_host.native.struct_layout]: layout {:?} is invalid: {error}",
                        layout.name
                    )
                })?;
            let encoded = encode_aggregate_layout(&descriptor).map_err(|error| {
                format!(
                    "error[self_host.native.struct_layout_abi]: layout {:?} cannot be encoded: {error}",
                    layout.name
                )
            })?;
            let encoded = Arc::<[u8]>::from(encoded);
            if let Some(previous) = layouts.insert(canonical.clone(), encoded.clone()) {
                if previous.as_ref() != encoded.as_ref() {
                    return Err(format!(
                        "error[self_host.native.conflicting_struct_layout]: semantic type {canonical:?} has multiple physical layouts"
                    ));
                }
            }
            for field in &layout.fields {
                if let Some(type_name) = field.type_name() {
                    inventory_tuple_layouts(
                        &parse_type(type_name)?,
                        &envelope.module.name,
                        &mut layouts,
                    )?;
                }
            }
        }
        for function in &envelope.module.functions {
            for parameter in &function.parameters {
                inventory_tuple_layouts(
                    &parse_type(&parameter.type_name)?,
                    &envelope.module.name,
                    &mut layouts,
                )?;
            }
            inventory_tuple_layouts(
                &parse_type(&function.return_type)?,
                &envelope.module.name,
                &mut layouts,
            )?;
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                inventory_tuple_layouts(
                    &parse_type(&instruction.type_name)?,
                    &envelope.module.name,
                    &mut layouts,
                )?;
                if instruction.opcode == "prepare-pattern" {
                    let (_, encoded) = pattern_layout(instruction)?;
                    let canonical = pattern_canonical(instruction)?;
                    if let Some(previous) = layouts.insert(canonical.clone(), encoded.clone()) {
                        if previous.as_ref() != encoded.as_ref() {
                            return Err(format!(
                                "error[self_host.native.conflicting_pattern_layout]: semantic type {canonical:?} has multiple physical layouts"
                            ));
                        }
                    }
                }
            }
        }
    }
    let mut layouts = layouts.into_values().collect::<Vec<_>>();
    layouts.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
    Ok(layouts)
}

#[cfg(test)]
mod empty_array_tests {
    use super::*;

    #[test]
    fn empty_array_lowers_without_fabricated_fields() {
        let expression = aggregate_expr("put-array", "Array[0, Int]", vec![])
            .expect("empty array lowering")
            .expect("array expression");
        let NativeExpr::Construct {
            descriptor, fields, ..
        } = expression
        else {
            panic!("empty fixed array must use aggregate construction");
        };
        assert!(fields.is_empty());
        assert!(descriptor.fields().is_empty());
        assert_eq!(descriptor.managed().size(), 1);
    }

    #[test]
    fn qualified_array_types_keep_nominal_layout_contracts() {
        let ty = parse_type("std.js.Array.Array[Int]").expect("nominal array type");
        assert_eq!(
            type_contract(&ty, "").expect("nominal array contract"),
            "Apply(std.js.Array.Array;Int)"
        );
        assert!(aggregate_expr("put-array", "std.js.Array.Array[2, Int]", vec![]).is_err());
    }
}

fn pattern_canonical(instruction: &IrInstruction) -> Result<String, String> {
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

fn pattern_layout(
    instruction: &IrInstruction,
) -> Result<(Arc<ManagedAggregateDescriptor>, Arc<[u8]>), String> {
    let (_subject, pattern) = super::self_host_pattern_ir::decode_prepare_pattern(instruction)?;
    let source_type = instruction
        .operand_types
        .first()
        .and_then(Option::as_deref)
        .ok_or_else(|| {
            "error[self_host.native.pattern_source_type]: prepare-pattern subject has no type"
                .to_string()
        })?;
    let source = parse_type(source_type)?;
    if !super::self_host_pattern_ir::prepare_pattern_deferred(instruction) {
        validate_pattern_shape(&pattern, &source, source_type, None)?;
    }
    let mut fields = vec![("matched".into(), ManagedFieldType::Bool)];
    if super::self_host_pattern_ir::prepare_pattern_deferred(instruction) {
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
                .collect::<Result<Vec<_>, String>>()?,
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

pub(super) type StructPatternCatalog = std::collections::HashMap<
    String,
    std::collections::HashMap<String, super::self_host_adapter::LocalStructField>,
>;

#[derive(Clone, Copy)]
struct PatternCatalogs<'a> {
    variants: Option<&'a crate::self_host_variant_ir::VariantCatalog>,
    structs: Option<&'a StructPatternCatalog>,
}

fn validate_pattern_shape(
    pattern: &super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    source_name: &str,
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
) -> Result<(), String> {
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

fn validate_pattern_shape_with_catalogs(
    pattern: &super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    source_name: &str,
    catalogs: PatternCatalogs<'_>,
) -> Result<(), String> {
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
                "error[self_host.native.pattern_shape]: list pattern requires a List source"
                    .to_string(),
            );
        }
        if pattern.kind == "list-cons-pattern" && pattern.children.len() != 2 {
            return Err(
                "error[self_host.native.pattern_shape]: list-cons pattern requires head and tail"
                    .to_string(),
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
            "error[self_host.native.pattern_shape]: leaf pattern has children".to_string()
        });
    }
    if pattern.kind == "literal-pattern" {
        return pattern.children.is_empty().then_some(()).ok_or_else(|| {
            "error[self_host.native.pattern_shape]: literal pattern has children".to_string()
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
        return Err(format!(
            "error[self_host.native.pattern_kind]: pattern kind {:?} has no direct native lowering for this subject",
            pattern.kind
        ));
    }
    if pattern.children.len() != source.arguments.len() {
        return Err(format!(
            "error[self_host.native.pattern_shape]: tuple pattern has {} child(ren), expected {}",
            pattern.children.len(),
            source.arguments.len()
        ));
    }
    for (child, child_type) in pattern.children.iter().zip(&source.arguments) {
        validate_pattern_shape_with_catalogs(
            child,
            child_type,
            &abi_type_source(child_type),
            catalogs,
        )?;
        if !structurally_supported(child) {
            return Err(
                "error[self_host.native.pattern_kind]: nested refutable tuple patterns are not yet lowered"
                    .into(),
            );
        }
    }
    Ok(())
}

fn structurally_supported(pattern: &super::self_host_pattern_ir::PatternNode) -> bool {
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

fn is_list_type(source: &AbiType) -> bool {
    source.arguments.len() == 1
        && (source.constructor == "List" || source.constructor.ends_with(".List"))
}

fn is_map_type(source: &AbiType) -> bool {
    source.arguments.len() == 2
        && (source.constructor == "Map" || source.constructor.ends_with(".Map"))
}

fn string_capture_kind(text: &str) -> Result<(&str, StringCaptureKind), String> {
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
            "error[self_host.native.string_pattern]: capture name must be a lower-case binding"
                .to_string(),
        );
    }
    let kind = match annotation {
        "String" => StringCaptureKind::String,
        "Int" => StringCaptureKind::Int,
        "Float" => StringCaptureKind::Float,
        "Bool" => StringCaptureKind::Bool,
        _ => {
            return Err(format!(
                "error[self_host.native.string_pattern]: no native capture converter for {annotation:?}"
            ))
        }
    };
    Ok((name, kind))
}

fn validate_string_capture_pattern(
    pattern: &super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
) -> Result<(), String> {
    if !(source.constructor == "String" || source.constructor.ends_with(".String"))
        || !source.arguments.is_empty()
    {
        return Err(
            "error[self_host.native.string_pattern]: string capture requires a String source"
                .to_string(),
        );
    }
    if pattern.children.is_empty() {
        return Err(
            "error[self_host.native.string_pattern]: string capture has no segments".to_string(),
        );
    }
    let mut previous_capture = false;
    let mut capture_count = 0_usize;
    let mut names = std::collections::HashSet::new();
    for segment in &pattern.children {
        if !segment.children.is_empty() {
            return Err(
                "error[self_host.native.string_pattern]: string segments must be leaves"
                    .to_string(),
            );
        }
        if segment.kind == "string-literal-segment" {
            if segment.text.is_empty() {
                return Err(
                    "error[self_host.native.string_pattern]: literal segment must not be empty"
                        .to_string(),
                );
            }
            previous_capture = false;
            continue;
        }
        if segment.kind != "string-capture-segment" {
            return Err(format!(
                "error[self_host.native.string_pattern]: unsupported segment {:?}",
                segment.kind
            ));
        }
        if previous_capture {
            return Err(
                "error[self_host.native.string_pattern]: adjacent captures require a literal separator"
                    .to_string(),
            );
        }
        let (name, _) = string_capture_kind(&segment.text)?;
        if name != "_" && !names.insert(name) {
            return Err(format!(
                "error[self_host.native.string_pattern]: duplicate capture {name:?}"
            ));
        }
        capture_count += 1;
        previous_capture = true;
    }
    if capture_count == 0 {
        return Err(
            "error[self_host.native.string_pattern]: pattern has no capture segment".to_string(),
        );
    }
    Ok(())
}

enum DecodedStringPatternSegment {
    Literal(String),
    Capture(StringCaptureKind),
}

fn decoded_string_pattern_segments(
    pattern: &super::self_host_pattern_ir::PatternNode,
) -> Result<Vec<DecodedStringPatternSegment>, String> {
    pattern
        .children
        .iter()
        .map(|segment| {
            if segment.kind == "string-literal-segment" {
                let quoted = format!("\"{}\"", segment.text);
                return super::self_host_adapter::decoded_string_literal(&quoted)
                    .map(DecodedStringPatternSegment::Literal);
            }
            string_capture_kind(&segment.text)
                .map(|(_, kind)| DecodedStringPatternSegment::Capture(kind))
        })
        .collect()
}

fn borrowed_string_pattern_segments(
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

fn keyed_pattern_fields(
    pattern: &super::self_host_pattern_ir::PatternNode,
) -> Result<&[super::self_host_pattern_ir::PatternNode], String> {
    let [fields] = pattern.children.as_slice() else {
        return Err(
            "error[self_host.native.pattern_shape]: keyed container requires one field wrapper"
                .to_string(),
        );
    };
    if fields.kind != "keyed-pattern-fields" || !fields.text.is_empty() {
        return Err(
            "error[self_host.native.pattern_shape]: keyed container field wrapper is invalid"
                .to_string(),
        );
    }
    Ok(&fields.children)
}

fn validate_keyed_pattern_shape(
    pattern: &super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    catalogs: PatternCatalogs<'_>,
) -> Result<(), String> {
    if !is_map_type(source) {
        return Err(
            "error[self_host.native.pattern_shape]: keyed container requires a Map source"
                .to_string(),
        );
    }
    let key = &source.arguments[0];
    if !(key.constructor == "String" || key.constructor.ends_with(".String")) {
        return Err(
            "error[self_host.native.pattern_shape]: keyed container requires a String-keyed Map"
                .to_string(),
        );
    }
    let value = &source.arguments[1];
    let mut names = std::collections::HashSet::new();
    for field in keyed_pattern_fields(pattern)? {
        if field.kind != "keyed-pattern-field" || field.text.is_empty() || field.children.len() != 1
        {
            return Err(
                "error[self_host.native.pattern_shape]: keyed pattern field requires one named value pattern"
                    .to_string(),
            );
        }
        if !names.insert(field.text.as_str()) {
            return Err(format!(
                "error[self_host.native.pattern_shape]: duplicate keyed pattern field {:?}",
                field.text
            ));
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

fn struct_pattern_fields<'a>(
    pattern: &super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    catalogs: PatternCatalogs<'a>,
) -> Result<&'a std::collections::HashMap<String, super::self_host_adapter::LocalStructField>, String>
{
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

fn admitted_struct_layout<'a>(
    source: &AbiType,
    catalogs: PatternCatalogs<'a>,
) -> Result<&'a std::collections::HashMap<String, super::self_host_adapter::LocalStructField>, String>
{
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

fn validate_struct_pattern_shape(
    pattern: &super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    catalogs: PatternCatalogs<'_>,
) -> Result<(), String> {
    let layout = struct_pattern_fields(pattern, source, catalogs)?;
    let mut names = std::collections::HashSet::new();
    for field in keyed_pattern_fields(pattern)? {
        if field.kind != "keyed-pattern-field" || field.text.is_empty() || field.children.len() != 1
        {
            return Err(
                "error[self_host.native.pattern_shape]: struct pattern field requires one named value pattern"
                    .to_string(),
            );
        }
        if !names.insert(field.text.as_str()) {
            return Err(format!(
                "error[self_host.native.pattern_shape]: duplicate struct pattern field {:?}",
                field.text
            ));
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

/// Lowers a string-keyed map field projection using ABI-1.3 operand evidence.
pub(super) fn map_field_expr(
    instruction: &IrInstruction,
    source: NativeExpr,
    source_type: NativeType,
    result_type: NativeType,
) -> Result<(NativeExpr, NativeType), String> {
    let [source_name, field_name] = instruction.operands.as_slice() else {
        return Err(
            "error[self_host.native.map_field]: get-map-elements requires source and field"
                .to_string(),
        );
    };
    if source_name.is_empty() || field_name.is_empty() {
        return Err(
            "error[self_host.native.map_field]: map source and field must not be empty".to_string(),
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
        return Err(format!(
            "error[self_host.native.map_field]: structural field source `{declared_source}` is not Map[String, T]"
        ));
    }
    let key = &map.arguments[0];
    if !(key.constructor == "String" || key.constructor.ends_with(".String")) {
        return Err(
            "error[self_host.native.map_field]: field projection requires a String-keyed Map"
                .to_string(),
        );
    }
    if native_type(declared_source)? != Some(source_type) {
        return Err(
            "error[self_host.native.map_field]: map type evidence disagrees with its SSA producer"
                .to_string(),
        );
    }
    let value_type = native_type_of(&map.arguments[1])?.ok_or_else(|| {
        "error[self_host.native.map_field]: map value type is not native".to_string()
    })?;
    if value_type != result_type {
        return Err(
            "error[self_host.native.map_field]: map field result disagrees with its value type"
                .to_string(),
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
pub(super) fn aggregate_field_expr(
    instruction: &IrInstruction,
    source: NativeExpr,
    source_type: NativeType,
    result_type: NativeType,
    field: usize,
) -> Result<(NativeExpr, NativeType), String> {
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
        return Err(
            "error[self_host.native.struct_field]: struct type evidence disagrees with its SSA producer"
                .to_string(),
        );
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
pub(super) fn aggregate_replace_field_expr(
    instruction: &IrInstruction,
    source: NativeExpr,
    source_type: NativeType,
    replacement: NativeExpr,
    replacement_type: NativeType,
    result_type: NativeType,
    field: usize,
    expected_field_type: &str,
) -> Result<(NativeExpr, NativeType), String> {
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
        return Err(
            "error[self_host.native.struct_update]: struct source, result, and ABI type evidence disagree"
                .to_string(),
        );
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
        return Err(
            "error[self_host.native.struct_update]: replacement type disagrees with the struct field"
                .to_string(),
        );
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

fn bool_and(left: NativeExpr, right: NativeExpr) -> NativeExpr {
    NativeExpr::If {
        clauses: vec![
            (left, right),
            (NativeExpr::Bool(true), NativeExpr::Bool(false)),
        ],
    }
}

#[cfg(test)]
#[path = "self_host_managed_layouts_keyed_pattern_test.rs"]
mod keyed_pattern_tests;

#[cfg(test)]
#[path = "self_host_managed_layouts_string_pattern_test.rs"]
mod string_pattern_tests;

#[cfg(test)]
#[path = "self_host_managed_layouts_struct_pattern_test.rs"]
mod struct_pattern_tests;

#[cfg(test)]
#[path = "self_host_managed_layouts_string_capture_test.rs"]
mod string_capture_tests;

#[allow(dead_code)]
fn pattern_predicate(
    pattern: &super::self_host_pattern_ir::PatternNode,
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

fn pattern_predicate_with_catalogs(
    pattern: &super::self_host_pattern_ir::PatternNode,
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
            let decoded = super::self_host_adapter::decoded_string_literal(&pattern.text)?;
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

fn keyed_pattern_predicate(
    pattern: &super::self_host_pattern_ir::PatternNode,
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

fn struct_pattern_predicate(
    pattern: &super::self_host_pattern_ir::PatternNode,
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

fn list_pattern_predicate(
    pattern: &super::self_host_pattern_ir::PatternNode,
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

fn binding_type_at_pattern(
    pattern: &super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    source_name: &str,
    path: &[usize],
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
) -> Result<AbiType, String> {
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

fn binding_type_at_pattern_with_catalogs(
    pattern: &super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    source_name: &str,
    path: &[usize],
    catalogs: PatternCatalogs<'_>,
) -> Result<AbiType, String> {
    let Some((index, rest)) = path.split_first() else {
        return Ok(source.clone());
    };
    let child = pattern.children.get(*index).ok_or_else(|| {
        "error[self_host.native.pattern_binding_shape]: binding path exceeds pattern arity"
            .to_string()
    })?;
    if pattern.kind == "string-capture-pattern" {
        if child.kind != "string-capture-segment" {
            return Err(
                "error[self_host.native.pattern_binding_shape]: string binding path selects a literal segment"
                    .to_string(),
            );
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
            return Err(
                "error[self_host.native.pattern_binding_shape]: list-cons binding path exceeds arity"
                    .to_string(),
            );
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
            return Err(
                "error[self_host.native.pattern_binding_shape]: keyed field binding path is invalid"
                    .to_string(),
            );
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
        "error[self_host.native.pattern_binding_shape]: binding path crosses a terminal pattern"
            .to_string(),
    )
}

fn project_binding_path(
    mut value: NativeExpr,
    source: &AbiType,
    path: &[usize],
) -> Result<NativeExpr, String> {
    let mut current = source;
    for index in path {
        if current.constructor != "$Tuple" {
            return Err(
                "error[self_host.native.pattern_binding_shape]: binding path crosses a non-tuple value"
                    .to_string(),
            );
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

fn project_variant_field(
    value: NativeExpr,
    source: &AbiType,
    index: usize,
    child: &AbiType,
) -> Result<NativeExpr, String> {
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

fn list_pattern_operation(
    source: &AbiType,
    value: NativeExpr,
    first: bool,
) -> Result<NativeExpr, String> {
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

#[allow(dead_code)]
fn project_pattern_binding_path(
    pattern: &super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    source_name: &str,
    value: NativeExpr,
    path: &[usize],
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
) -> Result<NativeExpr, String> {
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

fn project_pattern_binding_path_with_catalogs(
    pattern: &super::self_host_pattern_ir::PatternNode,
    source: &AbiType,
    source_name: &str,
    value: NativeExpr,
    path: &[usize],
    catalogs: PatternCatalogs<'_>,
) -> Result<NativeExpr, String> {
    let Some((index, rest)) = path.split_first() else {
        return Ok(value);
    };
    let child = pattern.children.get(*index).ok_or_else(|| {
        "error[self_host.native.pattern_binding_shape]: binding path exceeds pattern arity"
            .to_string()
    })?;
    if pattern.kind == "string-capture-pattern" {
        if child.kind != "string-capture-segment" {
            return Err(
                "error[self_host.native.pattern_binding_shape]: string binding path selects a literal segment"
                    .to_string(),
            );
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
            return Err(
                "error[self_host.native.pattern_binding_shape]: list-cons binding path exceeds arity"
                    .to_string(),
            );
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
            return Err(
                "error[self_host.native.pattern_binding_shape]: keyed field binding path is invalid"
                    .to_string(),
            );
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
        "error[self_host.native.pattern_binding_shape]: binding path crosses a terminal pattern"
            .to_string(),
    )
}

fn binding_path_crosses_list(
    pattern: &super::self_host_pattern_ir::PatternNode,
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

fn guarded_binding(
    matched: NativeExpr,
    projected: NativeExpr,
    binding_type: NativeType,
    binding_abi: &AbiType,
) -> Result<NativeExpr, String> {
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
            return Err(
                "error[self_host.native.pattern_binding_shape]: managed binding has no typed deferred default"
                    .to_string(),
            )
        }
    };
    Ok(NativeExpr::If {
        clauses: vec![(matched, projected), (NativeExpr::Bool(true), fallback)],
    })
}

fn pattern_literal(text: &str, source_type: NativeType) -> Result<NativeExpr, String> {
    match source_type {
        NativeType::Int => text
            .parse::<i64>()
            .map(NativeExpr::Int)
            .map_err(|_| format!("error[self_host.native.pattern_literal]: invalid Int {text:?}")),
        NativeType::Float => text
            .parse::<f64>()
            .map(|value| NativeExpr::Float(value.to_bits()))
            .map_err(|_| {
                format!("error[self_host.native.pattern_literal]: invalid Float {text:?}")
            }),
        NativeType::Bool if text == "true" => Ok(NativeExpr::Bool(true)),
        NativeType::Bool if text == "false" => Ok(NativeExpr::Bool(false)),
        NativeType::Unit if matches!(text, "Unit" | "unit" | "()") => Ok(NativeExpr::Unit),
        _ => Err(format!(
            "error[self_host.native.pattern_literal]: literal {text:?} is unsupported for {source_type:?}"
        )),
    }
}

/// Lowers one recursively validated pattern into a managed match environment.
#[allow(dead_code)]
pub(super) fn prepare_pattern_expr(
    instruction: &IrInstruction,
    subject: NativeExpr,
    subject_type: NativeType,
) -> Result<(NativeExpr, NativeType), String> {
    prepare_pattern_expr_with_catalogs(instruction, subject, subject_type, None, None)
}

#[allow(dead_code)]
pub(super) fn prepare_pattern_expr_with_variants(
    instruction: &IrInstruction,
    subject: NativeExpr,
    subject_type: NativeType,
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
) -> Result<(NativeExpr, NativeType), String> {
    prepare_pattern_expr_with_catalogs(instruction, subject, subject_type, variants, None)
}

pub(super) fn prepare_pattern_expr_with_catalogs(
    instruction: &IrInstruction,
    subject: NativeExpr,
    subject_type: NativeType,
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
    structs: Option<&StructPatternCatalog>,
) -> Result<(NativeExpr, NativeType), String> {
    let catalogs = PatternCatalogs { variants, structs };
    let (_subject_name, pattern) =
        super::self_host_pattern_ir::decode_prepare_pattern(instruction)?;
    let declared_source = instruction
        .operand_types
        .first()
        .and_then(Option::as_deref)
        .ok_or_else(|| {
            "error[self_host.native.pattern_source_type]: prepare-pattern subject has no type"
                .to_string()
        })?;
    let source = parse_type(declared_source)?;
    validate_pattern_shape_with_catalogs(&pattern, &source, declared_source, catalogs)?;
    if native_type(declared_source)? != Some(subject_type) {
        return Err(
            "error[self_host.native.pattern_source_type]: pattern type evidence disagrees with its SSA producer"
                .into(),
        );
    }
    let matched = pattern_predicate_with_catalogs(
        &pattern,
        subject.clone(),
        &source,
        declared_source,
        catalogs,
    )?;
    let (descriptor, encoded_layout) = pattern_layout(instruction)?;
    let semantic = descriptor.managed().semantic_id();
    let fields = if super::self_host_pattern_ir::prepare_pattern_deferred(instruction) {
        vec![matched, subject]
    } else {
        let binding_guard = matched.clone();
        let mut fields = vec![matched];
        for binding in pattern.bindings() {
            let binding_type = binding_type_at_pattern_with_catalogs(
                &pattern,
                &source,
                declared_source,
                &binding.path,
                catalogs,
            )?;
            let native_binding_type = native_type_of(&binding_type)?.ok_or_else(|| {
                "error[self_host.native.pattern_binding_shape]: binding type is not native"
                    .to_string()
            })?;
            let projected = project_pattern_binding_path_with_catalogs(
                &pattern,
                &source,
                declared_source,
                subject.clone(),
                &binding.path,
                catalogs,
            )?;
            fields.push(if binding_path_crosses_list(&pattern, &binding.path) {
                guarded_binding(
                    binding_guard.clone(),
                    projected,
                    native_binding_type,
                    &binding_type,
                )?
            } else {
                projected
            });
        }
        fields
    };
    Ok((
        NativeExpr::Construct {
            descriptor,
            encoded_layout,
            fields,
        },
        NativeType::ManagedRef(semantic),
    ))
}

/// Projects match status or the root binding from a managed pattern environment.
#[allow(dead_code)]
pub(super) fn pattern_environment_expr(
    instruction: &IrInstruction,
    environment: NativeExpr,
    environment_type: NativeType,
) -> Result<Option<(NativeExpr, NativeType)>, String> {
    pattern_environment_expr_with_catalogs(instruction, environment, environment_type, None, None)
}

#[allow(dead_code)]
pub(super) fn pattern_environment_expr_with_variants(
    instruction: &IrInstruction,
    environment: NativeExpr,
    environment_type: NativeType,
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
) -> Result<Option<(NativeExpr, NativeType)>, String> {
    pattern_environment_expr_with_catalogs(
        instruction,
        environment,
        environment_type,
        variants,
        None,
    )
}

pub(super) fn pattern_environment_expr_with_catalogs(
    instruction: &IrInstruction,
    environment: NativeExpr,
    environment_type: NativeType,
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
    structs: Option<&StructPatternCatalog>,
) -> Result<Option<(NativeExpr, NativeType)>, String> {
    let catalogs = PatternCatalogs { variants, structs };
    if !matches!(
        instruction.opcode.as_str(),
        "pattern-matched" | "get-pattern-binding"
    ) {
        return Ok(None);
    }
    let NativeType::ManagedRef(semantic) = environment_type else {
        return Err(
            "error[self_host.native.pattern_environment]: pattern environment is not managed"
                .into(),
        );
    };
    if instruction.opcode == "get-pattern-binding" && instruction.operands.len() > 2 {
        let (selector, source_name, pattern) =
            super::self_host_pattern_ir::decode_binding_pattern(instruction)?;
        let source = parse_type(source_name)?;
        let source_type = native_type_of(&source)?.ok_or_else(|| {
            "error[self_host.native.pattern_binding]: binding source type is unsupported"
                .to_string()
        })?;
        let binding = pattern.bindings().into_iter().nth(selector).ok_or_else(|| {
            format!(
                "error[self_host.native.pattern_binding]: binding selector {selector} is out of range"
            )
        })?;
        let binding_type = binding_type_at_pattern_with_catalogs(
            &pattern,
            &source,
            source_name,
            &binding.path,
            catalogs,
        )?;
        let result_type = native_type_of(&binding_type)?.ok_or_else(|| {
            "error[self_host.native.pattern_binding]: binding result type is unsupported"
                .to_string()
        })?;
        if native_type(&instruction.type_name)? != Some(result_type) {
            return Err(
                "error[self_host.native.pattern_binding]: binding result type disagrees with pattern metadata"
                    .to_string(),
            );
        }
        let encoded = if source_type.is_managed_reference() {
            encode_aggregate_field_operation(semantic, 1)
        } else {
            encode_aggregate_scalar_field_operation(semantic, 1)
        }
        .map_err(|error| format!("error[self_host.native.pattern_projection_abi]: {error}"))?;
        let subject = NativeExpr::ManagedOperation {
            encoded: Arc::from(encoded),
            args: vec![environment],
        };
        return Ok(Some((
            project_pattern_binding_path_with_catalogs(
                &pattern,
                &source,
                source_name,
                subject,
                &binding.path,
                catalogs,
            )?,
            result_type,
        )));
    }
    let (field, result_type) = if instruction.opcode == "pattern-matched" {
        (0_usize, NativeType::Bool)
    } else {
        let selector = instruction.operands.get(1).ok_or_else(|| {
            "error[self_host.native.pattern_binding]: missing binding selector".to_string()
        })?;
        let selector = selector.parse::<usize>().map_err(|_| {
            format!(
                "error[self_host.native.pattern_binding]: binding selector {selector:?} is not an index"
            )
        })?;
        let result_type = native_type(&instruction.type_name)?.ok_or_else(|| {
            "error[self_host.native.pattern_binding]: binding result type is unsupported"
                .to_string()
        })?;
        (
            selector.checked_add(1).ok_or_else(|| {
                "error[self_host.native.pattern_binding]: binding selector overflows".to_string()
            })?,
            result_type,
        )
    };
    let encoded = if result_type.is_managed_reference() {
        encode_aggregate_field_operation(semantic, field)
    } else {
        encode_aggregate_scalar_field_operation(semantic, field)
    }
    .map_err(|error| format!("error[self_host.native.pattern_projection_abi]: {error}"))?;
    Ok(Some((
        NativeExpr::ManagedOperation {
            encoded: Arc::from(encoded),
            args: vec![environment],
        },
        result_type,
    )))
}

fn inventory_tuple_layouts(
    ty: &AbiType,
    module_name: &str,
    layouts: &mut BTreeMap<String, Arc<[u8]>>,
) -> Result<(), String> {
    for argument in &ty.arguments {
        inventory_tuple_layouts(argument, module_name, layouts)?;
    }
    if ty.constructor != "$Tuple" {
        return Ok(());
    }
    let canonical = type_contract(ty, module_name)?;
    let descriptor = ManagedAggregateDescriptor::tuple(
        &canonical,
        ty.arguments
            .iter()
            .map(|field| managed_field_type_of(field, module_name))
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(|error| format!("error[self_host.native.tuple_layout]: {error}"))?;
    let encoded = Arc::<[u8]>::from(
        encode_aggregate_layout(&descriptor)
            .map_err(|error| format!("error[self_host.native.tuple_layout_abi]: {error}"))?,
    );
    if let Some(previous) = layouts.insert(canonical.clone(), encoded.clone()) {
        if previous.as_ref() != encoded.as_ref() {
            return Err(format!(
                "error[self_host.native.conflicting_tuple_layout]: semantic type {canonical:?} has multiple physical layouts"
            ));
        }
    }
    Ok(())
}

/// Inventories every concrete collection schema reachable from ABI type positions.
pub(super) fn encoded_collection_layouts(
    envelopes: &[BackendEnvelope],
) -> Result<Vec<Arc<[u8]>>, String> {
    let mut layouts = BTreeMap::<String, Arc<[u8]>>::new();
    for envelope in envelopes {
        let module_name = envelope.module.name.as_str();
        for layout in &envelope.module.structs {
            for field in &layout.fields {
                if let Some(type_name) = field.type_name() {
                    inventory_collections(&parse_type(type_name)?, module_name, &mut layouts)?;
                }
            }
        }
        for function in &envelope.module.functions {
            for parameter in &function.parameters {
                inventory_collections(
                    &parse_type(&parameter.type_name)?,
                    module_name,
                    &mut layouts,
                )?;
            }
            inventory_collections(
                &parse_type(&function.return_type)?,
                module_name,
                &mut layouts,
            )?;
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                inventory_collections(
                    &parse_type(&instruction.type_name)?,
                    module_name,
                    &mut layouts,
                )?;
            }
        }
    }
    let mut layouts = layouts.into_values().collect::<Vec<_>>();
    layouts.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
    Ok(layouts)
}

/// Resolves one closed ABI type into its direct NativeIR value kind.
///
/// Bare non-primitive names deliberately return `None`: their defining module
/// must be supplied by authoritative type resolution before a semantic ID can
/// be assigned safely.
pub(super) fn native_type(type_name: &str) -> Result<Option<NativeType>, String> {
    let ty = parse_type(type_name)?;
    native_type_of(&ty)
}

/// Lowers ABI list constructors after their SSA operands have been resolved.
pub(super) fn collection_expr(
    opcode: &str,
    type_name: &str,
    args: Vec<NativeExpr>,
) -> Result<Option<NativeExpr>, String> {
    if !matches!(opcode, "put-list" | "put-list-cons") {
        return Ok(None);
    }
    let ty = parse_type(type_name)?;
    let short = ty.constructor.rsplit('.').next().unwrap_or(&ty.constructor);
    if short != "List" || ty.arguments.len() != 1 {
        return Err(format!(
            "error[self_host.native.list_type]: {opcode} result must be a concrete List[T], found {type_name:?}"
        ));
    }
    if opcode == "put-list-cons" && args.len() != 2 {
        return Err(format!(
            "error[self_host.native.list_cons_arity]: put-list-cons expects head and tail, found {} operand(s)",
            args.len()
        ));
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
pub(super) fn aggregate_expr(
    opcode: &str,
    type_name: &str,
    fields: Vec<NativeExpr>,
) -> Result<Option<NativeExpr>, String> {
    if !matches!(opcode, "put-tuple" | "put-array") {
        return Ok(None);
    }
    let ty = parse_type(type_name)?;
    if opcode == "put-array" {
        let [length, element] = ty.arguments.as_slice() else {
            return Err(format!(
                "error[self_host.native.array_type]: fixed array result requires size and element type, found {type_name:?}"
            ));
        };
        if ty.constructor != "Array" {
            return Err(format!(
                "error[self_host.native.array_type]: put-array result must be Array[N, T], found {type_name:?}"
            ));
        }
        let length = length.constructor.parse::<usize>().map_err(|_| {
            format!(
                "error[self_host.native.array_size]: fixed array size is not concrete in {type_name:?}"
            )
        })?;
        if length != fields.len() {
            return Err(format!(
                "error[self_host.native.array_arity]: fixed array type has {length} field(s), instruction has {} operand(s)",
                fields.len()
            ));
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
        return Err(format!(
            "error[self_host.native.tuple_type]: put-tuple result must be a tuple, found {type_name:?}"
        ));
    }
    if ty.arguments.len() != fields.len() {
        return Err(format!(
            "error[self_host.native.tuple_arity]: tuple type has {} field(s), instruction has {} operand(s)",
            ty.arguments.len(),
            fields.len()
        ));
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
pub(super) fn aggregate_record_expr(
    type_name: &str,
    fields: Vec<(String, String, NativeExpr)>,
) -> Result<NativeExpr, String> {
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
                .collect::<Result<Vec<_>, String>>()?,
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
pub(super) fn variant_expr(
    instruction: &IrInstruction,
    fields: Vec<NativeExpr>,
    catalog: &crate::self_host_variant_ir::VariantCatalog,
) -> Result<NativeExpr, String> {
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
        return Err(format!(
            "error[self_host.native.variant_arity]: constructor `{}` received {} values for {} payload slots",
            plan.descriptor.constructor,
            fields.len(),
            plan.operand_types.len()
        ));
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
#[allow(dead_code)]
pub(super) fn managed_instruction_expr(
    instruction: &IrInstruction,
    args: Vec<NativeExpr>,
) -> Result<Option<NativeExpr>, String> {
    managed_instruction_expr_with_variants(instruction, args, None)
}

/// Lowers managed ABI instructions with optional finite-sum authority.
pub(super) fn managed_instruction_expr_with_variants(
    instruction: &IrInstruction,
    args: Vec<NativeExpr>,
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
) -> Result<Option<NativeExpr>, String> {
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
        return Err(format!(
            "error[self_host.native.tuple_projection_arity]: get-tuple-element expects tuple and index, found {} operand(s)",
            instruction.operands.len()
        ));
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
        return Err(format!(
            "error[self_host.native.tuple_projection_source_type]: expected tuple operand, found {source_type:?}"
        ));
    }
    let index = instruction.operands[1].parse::<usize>().map_err(|_| {
        format!(
            "error[self_host.native.tuple_projection_index]: tuple index {:?} is not a non-negative integer",
            instruction.operands[1]
        )
    })?;
    if index >= source.arguments.len() {
        return Err(format!(
            "error[self_host.native.tuple_projection_index]: tuple index {index} exceeds arity {}",
            source.arguments.len()
        ));
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

fn declared_operand_type(instruction: &IrInstruction, index: usize) -> Result<NativeType, String> {
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
    })
}

fn require_operand_type(
    instruction: &IrInstruction,
    index: usize,
    expected: NativeType,
) -> Result<(), String> {
    let actual = declared_operand_type(instruction, index)?;
    (actual == expected).then_some(()).ok_or_else(|| {
        format!(
            "error[self_host.native.managed_operand_type]: opcode {:?} operand {index} does not match its aggregate contract",
            instruction.opcode
        )
    })
}

fn validate_managed_instruction_types(instruction: &IrInstruction) -> Result<(), String> {
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
                return Err(
                    "error[self_host.native.array_type]: fixed array requires size and element type"
                        .into(),
                );
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
                return Err(
                    "error[self_host.native.tuple_projection_result_type]: result does not match selected tuple field"
                        .into(),
                );
            }
        }
        _ => {}
    }
    Ok(())
}

fn native_type_of(ty: &AbiType) -> Result<Option<NativeType>, String> {
    if ty.constructor == "$Tuple" {
        return Ok(Some(NativeType::ManagedRef(
            SemanticTypeId::from_canonical(&type_contract(ty, "")?)
                .map_err(|error| error.to_string())?,
        )));
    }
    let short = ty.constructor.rsplit('.').next().unwrap_or(&ty.constructor);
    if ty.arguments.is_empty() {
        return Ok(match short {
            "Unit" => Some(NativeType::Unit),
            "Bool" => Some(NativeType::Bool),
            "Int" => Some(NativeType::Int),
            "Float" => Some(NativeType::Float),
            "Atom" => Some(NativeType::Atom),
            "String" => Some(NativeType::StringRef),
            "Bytes" => Some(NativeType::BytesRef),
            "Binary" | "BitString" => Some(NativeType::BinaryRef),
            "Dynamic" | "Term" => None,
            _ if ty.constructor.contains('.') => Some(NativeType::ManagedRef(
                SemanticTypeId::from_canonical(&nominal_contract(&ty.constructor, "")?)
                    .map_err(|error| error.to_string())?,
            )),
            _ => None,
        });
    }
    match (short, ty.arguments.as_slice()) {
        ("Process" | "Entry" | "Monitor" | "ResourceKind" | "Resource", [_]) => {
            Ok(Some(NativeType::Int))
        }
        ("Message", [payload]) => native_type_of(payload),
        _ if !context_free_closed(ty) => Ok(None),
        _ => Ok(Some(NativeType::ManagedRef(
            SemanticTypeId::from_canonical(&type_contract(&ty, "")?)
                .map_err(|error| error.to_string())?,
        ))),
    }
}

fn context_free_closed(ty: &AbiType) -> bool {
    let short = ty.constructor.rsplit('.').next().unwrap_or(&ty.constructor);
    let constructor_is_closed = ty.constructor == "$Tuple"
        || ty.constructor.contains('.')
        || matches!(
            short,
            "Unit"
                | "Bool"
                | "Int"
                | "Float"
                | "Atom"
                | "String"
                | "Bytes"
                | "Binary"
                | "BitString"
                | "Never"
                | "List"
                | "Map"
                | "Set"
                | "Option"
                | "Result"
                | "Iterator"
                | "Process"
                | "Message"
                | "Entry"
                | "Monitor"
                | "ResourceKind"
                | "Resource"
        );
    constructor_is_closed && ty.arguments.iter().all(context_free_closed)
}

fn lower_field(
    field: &IrStructField,
    module_name: &str,
    layout_name: &str,
) -> Result<(String, ManagedFieldType), String> {
    let IrStructField::Typed { name, type_name } = field else {
        return Err(format!(
            "error[self_host.native.legacy_struct_layout]: layout {layout_name:?} field {:?} has no ABI-1.2 type",
            field.name()
        ));
    };
    Ok((
        name.clone(),
        managed_field_type(type_name, module_name).map_err(|error| {
            format!(
                "error[self_host.native.struct_field_type]: layout {layout_name:?} field {name:?}: {error}"
            )
        })?,
    ))
}

fn managed_field_type(type_name: &str, module_name: &str) -> Result<ManagedFieldType, String> {
    managed_field_type_of(&parse_type(type_name)?, module_name)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AbiType {
    constructor: String,
    arguments: Vec<AbiType>,
}

fn parse_type(source: &str) -> Result<AbiType, String> {
    if source.len() > 4_096 {
        return Err("type contract exceeds the 4096-byte ABI bound".into());
    }
    parse_type_at(source.trim(), 0)
}

fn abi_type_source(value: &AbiType) -> String {
    if value.constructor == "$Tuple" {
        return format!(
            "({})",
            value
                .arguments
                .iter()
                .map(abi_type_source)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if value.arguments.is_empty() {
        return value.constructor.clone();
    }
    format!(
        "{}[{}]",
        value.constructor,
        value
            .arguments
            .iter()
            .map(abi_type_source)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn parse_type_at(source: &str, depth: usize) -> Result<AbiType, String> {
    if depth >= 64 {
        return Err("type contract exceeds the 64-level nesting bound".into());
    }
    if source.is_empty() {
        return Err("empty type contract".into());
    }
    if source.starts_with('(') && source.ends_with(')') {
        let inner = source[1..source.len() - 1].trim();
        let arguments = if inner.is_empty() {
            Vec::new()
        } else {
            split_type_arguments(inner)?
                .into_iter()
                .map(|argument| parse_type_at(argument, depth + 1))
                .collect::<Result<Vec<_>, _>>()?
        };
        return Ok(AbiType {
            constructor: "$Tuple".into(),
            arguments,
        });
    }
    let Some(open) = source.find('[') else {
        if is_nominal_identity(source) {
            return Ok(AbiType {
                constructor: source.into(),
                arguments: Vec::new(),
            });
        }
        return Err(format!("unsupported type contract {source:?}"));
    };
    if !source.ends_with(']') || !is_nominal_identity(source[..open].trim()) {
        return Err(format!("malformed type application {source:?}"));
    }
    let constructor = source[..open].trim();
    let arguments = split_type_arguments(&source[open + 1..source.len() - 1])?
        .into_iter()
        .enumerate()
        .map(|(index, argument)| {
            if constructor == "Array"
                && index == 0
                && !argument.is_empty()
                && argument.bytes().all(|byte| byte.is_ascii_digit())
            {
                Ok(AbiType {
                    constructor: argument.into(),
                    arguments: Vec::new(),
                })
            } else {
                parse_type_at(argument, depth + 1)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if arguments.is_empty() || arguments.len() > 256 {
        return Err(format!("invalid type application arity in {source:?}"));
    }
    Ok(AbiType {
        constructor: constructor.into(),
        arguments,
    })
}

fn split_type_arguments(source: &str) -> Result<Vec<&str>, String> {
    let mut brackets = 0_usize;
    let mut parentheses = 0_usize;
    let mut braces = 0_usize;
    let mut start = 0_usize;
    let mut arguments = Vec::new();
    for (index, character) in source.char_indices() {
        match character {
            '[' => brackets = brackets.saturating_add(1),
            ']' => {
                brackets = brackets
                    .checked_sub(1)
                    .ok_or_else(|| "unbalanced type application".to_string())?;
            }
            '(' => parentheses = parentheses.saturating_add(1),
            ')' => {
                parentheses = parentheses
                    .checked_sub(1)
                    .ok_or_else(|| "unbalanced tuple type".to_string())?;
            }
            '{' => braces = braces.saturating_add(1),
            '}' => {
                braces = braces
                    .checked_sub(1)
                    .ok_or_else(|| "unbalanced record type".to_string())?;
            }
            ',' if brackets == 0 && parentheses == 0 && braces == 0 => {
                arguments.push(source[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    if brackets != 0 || parentheses != 0 || braces != 0 {
        return Err("unbalanced type application".into());
    }
    arguments.push(source[start..].trim());
    if arguments.iter().any(|argument| argument.is_empty()) {
        return Err("empty type application argument".into());
    }
    Ok(arguments)
}

fn managed_field_type_of(ty: &AbiType, module_name: &str) -> Result<ManagedFieldType, String> {
    let short = ty.constructor.rsplit('.').next().unwrap_or(&ty.constructor);
    if ty.arguments.is_empty() {
        return Ok(match short {
            "Unit" => ManagedFieldType::Unit,
            "Bool" => ManagedFieldType::Bool,
            "Int" => ManagedFieldType::Int,
            "Float" => ManagedFieldType::Float,
            "Atom" => ManagedFieldType::Atom,
            "String" => reference("std.core.String")?,
            "Bytes" => reference("std.binary.Bytes")?,
            "Binary" | "BitString" => reference("std.binary.Binary")?,
            "Dynamic" | "Term" => {
                return Err(format!(
                    "open type {:?} has no closed managed layout",
                    ty.constructor
                ));
            }
            _ => reference(&type_contract(ty, module_name)?)?,
        });
    }
    match (short, ty.arguments.as_slice()) {
        ("Process" | "Entry" | "Monitor" | "ResourceKind" | "Resource", [_]) => {
            Ok(ManagedFieldType::Int)
        }
        ("Message", [payload]) => managed_field_type_of(payload, module_name),
        _ => reference(&type_contract(ty, module_name)?),
    }
}

fn type_contract(ty: &AbiType, module_name: &str) -> Result<String, String> {
    if ty.constructor == "$Tuple" {
        return Ok(format!(
            "Tuple({})",
            ty.arguments
                .iter()
                .map(|argument| type_contract(argument, module_name))
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        ));
    }
    if ty.arguments.is_empty() {
        return match ty.constructor.rsplit('.').next().unwrap_or(&ty.constructor) {
            primitive @ ("Unit" | "Bool" | "Int" | "Float" | "Atom" | "String" | "Binary"
            | "Bytes" | "Never") => Ok(primitive.into()),
            _ => nominal_contract(&ty.constructor, module_name),
        };
    }
    if ty.constructor == "Array" {
        let [length, element] = ty.arguments.as_slice() else {
            return Err("Array requires a size and element type".into());
        };
        length
            .constructor
            .parse::<usize>()
            .map_err(|_| "Array size must be a concrete non-negative integer".to_string())?;
        if !length.arguments.is_empty() {
            return Err("Array size must not have type arguments".into());
        }
        return Ok(format!(
            "Apply({};{},{})",
            ty.constructor,
            length.constructor,
            type_contract(element, module_name)?
        ));
    }
    let arguments = ty
        .arguments
        .iter()
        .map(|argument| type_contract(argument, module_name))
        .collect::<Result<Vec<_>, _>>()?
        .join(",");
    Ok(format!("Apply({};{arguments})", ty.constructor))
}

fn inventory_collections(
    ty: &AbiType,
    module_name: &str,
    layouts: &mut BTreeMap<String, Arc<[u8]>>,
) -> Result<(), String> {
    for argument in &ty.arguments {
        inventory_collections(argument, module_name, layouts)?;
    }
    let short = ty.constructor.rsplit('.').next().unwrap_or(&ty.constructor);
    let descriptor = match (short, ty.arguments.as_slice()) {
        ("List", [element]) => Some(ManagedCollectionDescriptor::list(
            &type_contract(ty, module_name)?,
            managed_field_type_of(element, module_name)?,
        )),
        ("Map", [key, value]) => Some(ManagedCollectionDescriptor::map(
            &type_contract(ty, module_name)?,
            managed_field_type_of(key, module_name)?,
            managed_field_type_of(value, module_name)?,
        )),
        ("Set", [element]) => Some(ManagedCollectionDescriptor::set(
            &type_contract(ty, module_name)?,
            managed_field_type_of(element, module_name)?,
        )),
        ("List" | "Map" | "Set", _) => {
            return Err(format!(
                "error[self_host.native.collection_arity]: invalid collection type {:?}",
                ty.constructor
            ));
        }
        _ => None,
    };
    let Some(descriptor) = descriptor else {
        return Ok(());
    };
    let descriptor = descriptor
        .map_err(|error| format!("error[self_host.native.collection_layout]: {error}"))?;
    let canonical = descriptor.canonical_type().to_owned();
    let encoded = Arc::<[u8]>::from(
        encode_collection_layout(&descriptor)
            .map_err(|error| format!("error[self_host.native.collection_layout]: {error}"))?,
    );
    if let Some(previous) = layouts.insert(canonical.clone(), encoded.clone()) {
        if previous.as_ref() != encoded.as_ref() {
            return Err(format!(
                "error[self_host.native.conflicting_collection_layout]: semantic type {canonical:?} has multiple physical layouts"
            ));
        }
    }
    Ok(())
}

fn reference(canonical: &str) -> Result<ManagedFieldType, String> {
    SemanticTypeId::from_canonical(canonical)
        .map(ManagedFieldType::Reference)
        .map_err(|error| error.to_string())
}

fn nominal_contract(identity: &str, module_name: &str) -> Result<String, String> {
    let identity = identity.trim();
    if !is_nominal_identity(identity) {
        return Err(format!("invalid nominal type identity {identity:?}"));
    }
    let qualified = if identity.contains('.') || module_name.is_empty() {
        identity.to_owned()
    } else {
        format!("{module_name}.{identity}")
    };
    Ok(format!("Named({qualified})"))
}

fn is_nominal_identity(identity: &str) -> bool {
    !identity.is_empty()
        && identity.split('.').all(|part| {
            let mut characters = part.chars();
            characters
                .next()
                .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
                && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::self_host_ir::{IrModule, IrStructLayout};

    fn envelope(field: IrStructField) -> BackendEnvelope {
        BackendEnvelope {
            abi_major: 1,
            abi_minor: 2,
            target: "native".into(),
            module: IrModule {
                name: "sample.geometry".into(),
                structs: vec![IrStructLayout {
                    name: "sample.geometry.Point".into(),
                    fields: vec![field],
                }],
                functions: Vec::new(),
            },
        }
    }

    #[test]
    fn typed_layout_lowers_to_decodable_managed_metadata() {
        let layouts = encoded_struct_layouts(&[envelope(IrStructField::Typed {
            name: "x".into(),
            type_name: "Int".into(),
        })])
        .expect("typed struct layout");

        let descriptor =
            crate::runtime::native_image::managed::decode_aggregate_layout(&layouts[0])
                .expect("decode struct layout");
        assert_eq!(descriptor.canonical_type(), "Named(sample.geometry.Point)");
        assert_eq!(descriptor.fields()[0].name(), Some("x"));
    }

    #[test]
    fn legacy_layout_is_rejected_by_native_lowering() {
        let error = encoded_struct_layouts(&[envelope(IrStructField::Legacy("x".into()))])
            .expect_err("legacy layout must not reach current native lowering");

        assert!(error.contains("legacy_struct_layout"), "{error}");
    }

    #[test]
    fn conflicting_application_layouts_are_rejected() {
        let left = envelope(IrStructField::Typed {
            name: "x".into(),
            type_name: "Int".into(),
        });
        let right = envelope(IrStructField::Typed {
            name: "x".into(),
            type_name: "Float".into(),
        });

        let error = encoded_struct_layouts(&[left, right])
            .expect_err("conflicting semantic layouts must not be admitted");

        assert!(error.contains("conflicting_struct_layout"), "{error}");
    }

    #[test]
    fn nested_collection_types_emit_each_required_schema() {
        let mut source = envelope(IrStructField::Typed {
            name: "index".into(),
            type_name: "Map[String, List[Int]]".into(),
        });
        source.module.structs.clear();
        source
            .module
            .functions
            .push(crate::self_host_ir::IrFunction {
                module_name: "sample.geometry".into(),
                name: "index".into(),
                arity: 1,
                capture_count: 0,
                receiver_mutable: false,
                receiver_command: false,
                parameters: vec![crate::self_host_ir::IrParameter {
                    name: "value".into(),
                    type_name: "Map[String, List[Int]]".into(),
                }],
                return_type: "Map[String, List[Int]]".into(),
                blocks: Vec::new(),
                native_symbol: Some("sample_index".into()),
            });

        let layouts = encoded_collection_layouts(&[source]).expect("collection layouts");
        let mut canonical = layouts
            .iter()
            .map(|layout| {
                crate::runtime::native_image::managed::decode_collection_layout(layout)
                    .expect("decode collection")
                    .canonical_type()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        canonical.sort();

        assert_eq!(
            canonical,
            vec![
                "Apply(List;Int)".to_string(),
                "Apply(Map;String,Apply(List;Int))".to_string(),
            ]
        );
    }

    #[test]
    fn closed_managed_abi_types_map_to_native_references() {
        let list = native_type("List[Int]")
            .expect("closed list type")
            .expect("native list type");
        let nominal = native_type("sample.geometry.Point")
            .expect("qualified nominal type")
            .expect("native nominal type");

        assert!(matches!(list, NativeType::ManagedRef(_)));
        assert!(matches!(nominal, NativeType::ManagedRef(_)));
        assert_eq!(native_type("Point").expect("bare type syntax"), None);
        assert_eq!(
            native_type("List[Point]").expect("generic bare type syntax"),
            None
        );
    }

    #[test]
    fn abi_list_constructors_lower_to_managed_operations() {
        let list = collection_expr(
            "put-list",
            "List[Int]",
            vec![NativeExpr::Int(1), NativeExpr::Int(2)],
        )
        .expect("list lowering")
        .expect("list operation");
        let cons = collection_expr(
            "put-list-cons",
            "List[Int]",
            vec![NativeExpr::Int(0), list.clone()],
        )
        .expect("list-cons lowering")
        .expect("list-cons operation");

        assert!(matches!(list, NativeExpr::ManagedOperation { .. }));
        assert!(matches!(cons, NativeExpr::ManagedOperation { .. }));
    }

    #[test]
    fn abi_list_cons_requires_head_and_tail() {
        let error = collection_expr("put-list-cons", "List[Int]", vec![NativeExpr::Int(1)])
            .expect_err("invalid list-cons arity");

        assert!(error.contains("list_cons_arity"), "{error}");
    }

    #[test]
    fn abi_tuple_constructor_lowers_to_managed_allocation() {
        let tuple = aggregate_expr(
            "put-tuple",
            "(Int, Int)",
            vec![NativeExpr::Int(1), NativeExpr::Int(2)],
        )
        .expect("tuple lowering")
        .expect("tuple expression");

        assert!(matches!(tuple, NativeExpr::Construct { fields, .. } if fields.len() == 2));
    }

    #[test]
    fn tuple_types_are_admitted_to_native_image_metadata() {
        let mut source = envelope(IrStructField::Typed {
            name: "pair".into(),
            type_name: "(Int, String)".into(),
        });
        source.module.structs[0].name = "sample.geometry.Box".into();

        let layouts = encoded_struct_layouts(&[source]).expect("aggregate layouts");
        let canonical = layouts
            .iter()
            .map(|layout| {
                crate::runtime::native_image::managed::decode_aggregate_layout(layout)
                    .expect("decode aggregate layout")
                    .canonical_type()
                    .to_owned()
            })
            .collect::<Vec<_>>();

        assert!(canonical.contains(&"Tuple(Int,String)".to_string()));
    }

    #[test]
    fn typed_tuple_projection_selects_scalar_managed_operation() {
        let instruction = IrInstruction {
            opcode: "get-tuple-element".into(),
            result: Some("value".into()),
            operands: vec!["pair".into(), "1".into()],
            operand_types: vec![Some("(String, Int)".into()), Some("Int".into())],
            type_name: "Int".into(),
            continuation_ordinal: None,
            try_region: None,
        };

        let expression =
            managed_instruction_expr(&instruction, vec![NativeExpr::Param(0), NativeExpr::Int(1)])
                .expect("tuple projection")
                .expect("managed projection");

        assert!(
            matches!(expression, NativeExpr::ManagedOperation { args, .. } if args == vec![NativeExpr::Param(0)])
        );
    }

    #[test]
    fn root_binding_pattern_uses_managed_environment_projections() {
        let prepare = IrInstruction {
            opcode: "prepare-pattern".into(),
            result: Some("environment".into()),
            operands: vec![
                "subject".into(),
                "binding-pattern".into(),
                "value".into(),
                "0".into(),
            ],
            operand_types: vec![Some("Int".into()), None, None, Some("Int".into())],
            type_name: "PatternMatch".into(),
            continuation_ordinal: None,
            try_region: None,
        };
        let (environment, environment_type) =
            prepare_pattern_expr(&prepare, NativeExpr::Param(0), NativeType::Int)
                .expect("prepare root binding");
        assert!(matches!(environment, NativeExpr::Construct { .. }));

        let matched = IrInstruction {
            opcode: "pattern-matched".into(),
            result: Some("matched".into()),
            operands: vec!["environment".into()],
            operand_types: vec![Some("PatternMatch".into())],
            type_name: "Bool".into(),
            continuation_ordinal: None,
            try_region: None,
        };
        let (_, matched_type) =
            pattern_environment_expr(&matched, NativeExpr::Param(1), environment_type)
                .expect("matched projection")
                .expect("matched expression");
        assert_eq!(matched_type, NativeType::Bool);

        let binding = IrInstruction {
            opcode: "get-pattern-binding".into(),
            result: Some("value".into()),
            operands: vec!["environment".into(), "0".into()],
            operand_types: vec![Some("PatternMatch".into()), Some("Int".into())],
            type_name: "Int".into(),
            continuation_ordinal: None,
            try_region: None,
        };
        let (_, binding_type) =
            pattern_environment_expr(&binding, NativeExpr::Param(1), environment_type)
                .expect("binding projection")
                .expect("binding expression");
        assert_eq!(binding_type, NativeType::Int);
    }

    #[test]
    fn v2_list_pattern_defers_aggregate_binding_projection() {
        let operands = vec![
            "subject".into(),
            "pattern-v2".into(),
            "list-cons-pattern".into(),
            "|".into(),
            "2".into(),
            "binding-pattern".into(),
            "head".into(),
            "0".into(),
            "wildcard-pattern".into(),
            "_".into(),
            "0".into(),
        ];
        let prepare = IrInstruction {
            opcode: "prepare-pattern".into(),
            result: Some("environment".into()),
            operand_types: (0..operands.len())
                .map(|index| (index == 0).then(|| "List[app.Widget]".into()))
                .collect(),
            operands,
            type_name: "PatternMatch".into(),
            continuation_ordinal: None,
            try_region: None,
        };
        let source_type = native_type("List[app.Widget]")
            .expect("list source type")
            .expect("native list source type");
        let (environment, environment_type) =
            prepare_pattern_expr(&prepare, NativeExpr::Param(0), source_type)
                .expect("deferred aggregate pattern environment");
        let NativeExpr::Construct { fields, .. } = environment else {
            panic!("v2 pattern environment must be managed");
        };
        assert_eq!(fields.len(), 2, "matched flag plus original subject");

        let binding = IrInstruction {
            opcode: "get-pattern-binding".into(),
            result: Some("head".into()),
            operands: vec![
                "environment".into(),
                "0".into(),
                "List[app.Widget]".into(),
                "list-cons-pattern".into(),
                "|".into(),
                "2".into(),
                "binding-pattern".into(),
                "head".into(),
                "0".into(),
                "wildcard-pattern".into(),
                "_".into(),
                "0".into(),
            ],
            operand_types: Vec::new(),
            type_name: "app.Widget".into(),
            continuation_ordinal: None,
            try_region: None,
        };
        let (_, binding_type) =
            pattern_environment_expr(&binding, NativeExpr::Param(1), environment_type)
                .expect("deferred aggregate binding projection")
                .expect("binding expression");
        assert!(matches!(binding_type, NativeType::ManagedRef(_)));
    }
}
