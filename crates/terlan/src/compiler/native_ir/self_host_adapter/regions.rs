use super::*;

pub(super) fn region_has_continuation(region: &IrRegion) -> bool {
    region.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            instruction.continuation_ordinal.is_some()
                || instruction
                    .try_region
                    .as_ref()
                    .is_some_and(|nested| try_region_has_continuation(nested))
        })
    })
}

pub(super) fn try_region_has_continuation(region: &IrTryRegion) -> bool {
    region_has_continuation(&region.protected)
        || region_has_continuation(&region.success)
        || region_has_continuation(&region.failure)
        || region.cleanup.iter().any(region_has_continuation)
}

pub(super) fn validate_region_return_identity(region: &IrRegion) -> Result<(), String> {
    let mut returns = 0_usize;
    for block in &region.blocks {
        if block.terminator.opcode != "return" {
            continue;
        }
        returns += 1;
        match block.terminator.operands.as_slice() {
            [result] if result == &region.result => {}
            _ => {
                return Err(adapter_error(
                    "try_region",
                    format!(
                        "region `{}` must return its declared result `{}`",
                        region.label, region.result
                    ),
                ));
            }
        }
    }
    if returns == 0 {
        return Err(adapter_error(
            "try_region",
            format!("region `{}` has no returning block", region.label),
        ));
    }
    Ok(())
}

pub(super) fn region_values(
    region: &IrRegion,
    outer: &ValueCatalog,
    slots: &[usize],
) -> Result<ValueCatalog, String> {
    if region.parameters.len() != slots.len() {
        return Err(adapter_error(
            "try_region",
            format!(
                "region `{}` parameter count disagrees with its native slots",
                region.label
            ),
        ));
    }
    let mut values = outer.clone();
    for (parameter, slot) in region.parameters.iter().zip(slots.iter().copied()) {
        if values.contains_key(&parameter.name) {
            return Err(adapter_error(
                "try_region",
                format!(
                    "region `{}` parameter `{}` shadows an outer value",
                    region.label, parameter.name
                ),
            ));
        }
        values.insert(
            parameter.name.clone(),
            (slot, native_type(&parameter.type_name)?),
        );
    }
    Ok(values)
}

pub(super) fn lower_region(
    region: &IrRegion,
    slots: &[usize],
    expected_type: NativeType,
    module_name: &str,
    outer: &ValueCatalog,
    functions: &FunctionCatalog,
    atoms: &mut Vec<String>,
    expansions: &mut usize,
) -> Result<NativeExpr, String> {
    let declared_type = native_type(&region.type_name)?;
    if declared_type != expected_type {
        return Err(adapter_error(
            "try_region",
            format!(
                "region `{}` type disagrees with its structured role",
                region.label
            ),
        ));
    }
    validate_region_return_identity(region)?;
    let actor_sites: &[super::super::self_host_actor_adapter::NativeActorSuspensionSite] = &[];
    lower_block_path(
        "entry",
        None,
        &region.blocks,
        module_name,
        region_values(region, outer, slots)?,
        functions,
        actor_sites,
        expected_type,
        atoms,
        &HashSet::new(),
        expansions,
    )?
    .ok_or_else(|| {
        adapter_error(
            "try_region",
            format!(
                "region `{}` has no returning control-flow path",
                region.label
            ),
        )
    })
}

pub(super) fn one_parameter_type(region: &IrRegion, expected: &str) -> bool {
    matches!(region.parameters.as_slice(), [parameter] if parameter.type_name == expected)
}

pub(super) fn try_region_expression(
    instruction: &IrInstruction,
    module_name: &str,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
    atoms: &mut Vec<String>,
) -> Result<(NativeExpr, NativeType), String> {
    if !instruction.operands.is_empty() {
        return Err(adapter_error(
            "try_region",
            "try-region cannot carry flat operands",
        ));
    }
    let regions = instruction.try_region.as_ref().ok_or_else(|| {
        adapter_error("try_region", "try-region has no structured region payload")
    })?;
    if try_region_has_continuation(regions) {
        return Err(adapter_error(
            "try_region",
            "structured try regions cannot contain actor continuations",
        ));
    }
    if !regions.protected.parameters.is_empty() {
        return Err(adapter_error(
            "try_region",
            "protected region cannot declare parameters",
        ));
    }
    if !one_parameter_type(&regions.success, &regions.protected.type_name) {
        return Err(adapter_error(
            "try_region",
            "success region must accept exactly the protected result type",
        ));
    }
    if !one_parameter_type(&regions.failure, "Int") {
        return Err(adapter_error(
            "try_region",
            "failure region must accept exactly one Int status",
        ));
    }
    if regions
        .cleanup
        .iter()
        .any(|region| !region.parameters.is_empty() || region.type_name != "Unit")
    {
        return Err(adapter_error(
            "try_region",
            "cleanup regions must accept no parameters and return Unit",
        ));
    }

    let result_type = native_type(&instruction.type_name)?;
    let protected_type = native_type(&regions.protected.type_name)?;
    let value_slot = values
        .values()
        .map(|(slot, _)| *slot)
        .max()
        .map_or(0, |slot| slot + 1);
    let mut expansions = 0_usize;
    let protected = lower_region(
        &regions.protected,
        &[],
        protected_type,
        module_name,
        values,
        functions,
        atoms,
        &mut expansions,
    )?;
    let success = lower_region(
        &regions.success,
        &[value_slot],
        result_type,
        module_name,
        values,
        functions,
        atoms,
        &mut expansions,
    )?;
    let failure = lower_region(
        &regions.failure,
        &[value_slot],
        result_type,
        module_name,
        values,
        functions,
        atoms,
        &mut expansions,
    )?;
    let cleanup = regions
        .cleanup
        .iter()
        .map(|region| {
            lower_region(
                region,
                &[],
                NativeType::Unit,
                module_name,
                values,
                functions,
                atoms,
                &mut expansions,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((
        NativeExpr::Try {
            protected: Box::new(protected),
            success: Box::new(success),
            failure: Box::new(failure),
            cleanup,
        },
        result_type,
    ))
}
