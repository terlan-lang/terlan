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
    let Some(CoreType::Union(variants)) = subject.core_type else {
        return Ok(None);
    };
    let Some((index, CoreType::Struct { fields, .. })) = variants.iter().enumerate().find(|(_, variant)| {
        matches!(variant, CoreType::Struct { name: identity, .. } if identity == name)
    }) else {
        return Err(format!("error[native_ir.record_identity]: `{name}` is not a declared record variant"));
    };
    let semantic = managed_semantic(subject.native_type)?;
    let discriminant = u32::try_from(index)
        .map_err(|_| "error[native_ir.union_pattern_arity]: union exceeds discriminant capacity")?;
    let mut plans = vec![PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_managed_variant_is_operation(semantic, discriminant)),
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
            project(subject.value.clone(), semantic, index, native)?,
            native,
            Some(&field.ty),
            constructors,
            depth + 1,
        )?);
    }
    merge(plans).map(Some)
}
