//! Nominal record patterns inside a closed, discriminated union.

use super::*;
use crate::terlan_typeck::CoreRecordPatternField;

/// Selects a declared nominal variant before emitting any field projections.
pub(super) fn union_record_plan(
    name: &str,
    patterns: &[CoreRecordPatternField],
    subject: PatternSubject<'_>,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<Option<PatternPlan>, String> {
    let Some(union @ CoreType::Union(variants)) = subject.core_type else {
        return Ok(None);
    };
    let Some((source_index, variant @ CoreType::Struct { fields, .. })) = variants.iter().enumerate().find(|(_, variant)| {
        matches!(variant, CoreType::Struct { name: identity, .. } if identity == name)
    }) else {
        return Err(format!("error[native_ir.record_identity]: `{name}` is not a declared record variant"));
    };
    let union_semantic = managed_semantic(subject.native_type)?;
    let (discriminant, record_value, record_semantic) =
        if super::super::union_values::is_boxed(union) {
            let record_native = native_core_type(variant)?;
            let (_, layout) = super::super::union_values::layouts(union)?
                .into_iter()
                .find(|(candidate, _)| *candidate == record_native)
                .ok_or_else(|| {
                    format!("error[native_ir.record_layout]: `{name}` has no boxed union layout")
                })?;
            let discriminant = layout.discriminant().ok_or_else(|| {
                format!("error[native_ir.record_layout]: `{name}` has no discriminant")
            })?;
            let value = project(subject.value.clone(), union_semantic, 0, record_native)?;
            (discriminant, value, managed_semantic(record_native)?)
        } else {
            let discriminant = u32::try_from(source_index).map_err(|_| {
                "error[native_ir.union_pattern_arity]: union exceeds discriminant capacity"
            })?;
            (discriminant, subject.value.clone(), union_semantic)
        };
    let mut plans = vec![PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_managed_variant_is_operation(
                union_semantic,
                discriminant,
            )),
            args: vec![subject.value.clone()],
        },
        bindings: Vec::new(),
    }];
    let mut seen = HashSet::new();
    for pattern in patterns {
        let Some((index, field)) = fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == pattern.key)
        else {
            return Err(format!(
                "error[native_ir.field_missing]: record `{name}` has no field `{}`",
                pattern.key
            ));
        };
        if !seen.insert(&pattern.key) {
            return Err(format!(
                "error[native_ir.record_field_duplicate]: record `{name}` repeats field `{}`",
                pattern.key
            ));
        }
        let native = native_core_type(&field.ty)?;
        plans.push(pattern_plan(
            &pattern.value,
            project(record_value.clone(), record_semantic, index, native)?,
            native,
            Some(&field.ty),
            constructors,
            depth + 1,
        )?);
    }
    merge(plans).map(Some)
}
