use super::*;

/// Lowers one recursively validated pattern into a managed match environment.
#[cfg(test)]
pub(in crate::compiler::native_ir) fn prepare_pattern_expr(
    instruction: &IrInstruction,
    subject: NativeExpr,
    subject_type: NativeType,
) -> Result<(NativeExpr, NativeType), NativeIrError> {
    prepare_pattern_expr_with_catalogs(instruction, subject, subject_type, None, None)
}

pub(in crate::compiler::native_ir) fn prepare_pattern_expr_with_catalogs(
    instruction: &IrInstruction,
    subject: NativeExpr,
    subject_type: NativeType,
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
    structs: Option<&StructPatternCatalog>,
) -> Result<(NativeExpr, NativeType), NativeIrError> {
    let catalogs = PatternCatalogs { variants, structs };
    let (_subject_name, pattern) =
        super::super::self_host_pattern_ir::decode_prepare_pattern(instruction)?;
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
        return Err("error[self_host.native.pattern_source_type]: pattern type evidence disagrees with its SSA producer"
                .into());
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
    let fields = if super::super::self_host_pattern_ir::prepare_pattern_deferred(instruction) {
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
#[cfg(test)]
pub(in crate::compiler::native_ir) fn pattern_environment_expr(
    instruction: &IrInstruction,
    environment: NativeExpr,
    environment_type: NativeType,
) -> Result<Option<(NativeExpr, NativeType)>, NativeIrError> {
    pattern_environment_expr_with_catalogs(instruction, environment, environment_type, None, None)
}

pub(in crate::compiler::native_ir) fn pattern_environment_expr_with_catalogs(
    instruction: &IrInstruction,
    environment: NativeExpr,
    environment_type: NativeType,
    variants: Option<&crate::self_host_variant_ir::VariantCatalog>,
    structs: Option<&StructPatternCatalog>,
) -> Result<Option<(NativeExpr, NativeType)>, NativeIrError> {
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
            super::super::self_host_pattern_ir::decode_binding_pattern(instruction)?;
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
            return Err(("error[self_host.native.pattern_binding]: binding result type disagrees with pattern metadata"
                    .to_string()).into());
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
