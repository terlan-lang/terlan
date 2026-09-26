//! String captures through the same managed predicate/binding plan as other patterns.

#[cfg(test)]
#[path = "string_test.rs"]
mod tests;

use crate::runtime::native_image::managed::{
    encode_string_pattern_extract_operation, encode_string_pattern_matches_operation,
    ManagedStringCaptureKind as Kind, ManagedStringPatternSegment as Segment,
};
use crate::terlan_typeck::{CoreStringPatternSegment, CoreType};

use super::super::NativeIrResult;
use super::{NativeExpr, NativeType, PatternBinding, PatternPlan};

pub(super) fn string_plan(
    source: &[CoreStringPatternSegment],
    value: NativeExpr,
    value_type: NativeType,
) -> NativeIrResult<PatternPlan> {
    if value_type != NativeType::StringRef {
        return Err("error[native_ir.string_pattern_type]: captures require String".into());
    }
    let segments = source.iter().map(|segment| match segment {
        CoreStringPatternSegment::Literal(text) => Ok(Segment::Literal(text)),
        CoreStringPatternSegment::Capture(capture) => {
            let kind = match capture.type_annotation.as_deref().unwrap_or("String") {
                "String" => Kind::String,
                "Int" => Kind::Int,
                "Float" => Kind::Float,
                "Bool" => Kind::Bool,
                annotation => return Err(format!(
                    "error[native_ir.string_pattern_conversion]: no native capture converter for `{annotation}`"
                ).into()),
            };
            Ok(Segment::Capture(kind))
        }
    }).collect::<NativeIrResult<Vec<_>>>()?;
    let predicate = encode_string_pattern_matches_operation(&segments)
        .map_err(|error| format!("error[native_ir.string_pattern_layout]: {error}"))?;
    let mut bindings = Vec::new();
    for (index, (capture, kind)) in source
        .iter()
        .zip(&segments)
        .filter_map(|(source, segment)| match (source, segment) {
            (CoreStringPatternSegment::Capture(capture), Segment::Capture(kind)) => {
                Some((capture, kind))
            }
            _ => None,
        })
        .enumerate()
    {
        if capture.name == "_" {
            continue;
        }
        let encoded = encode_string_pattern_extract_operation(&segments, index)
            .map_err(|error| format!("error[native_ir.string_pattern_layout]: {error}"))?;
        let (ty, core_ty) = match kind {
            Kind::String => (NativeType::StringRef, CoreType::String),
            Kind::Int => (NativeType::Int, CoreType::Int),
            Kind::Float => (NativeType::Float, CoreType::Float),
            Kind::Bool => (NativeType::Bool, CoreType::Bool),
        };
        bindings.push(PatternBinding {
            name: capture.name.clone(),
            value: NativeExpr::ManagedOperation {
                encoded: encoded.into(),
                args: vec![value.clone()],
            },
            ty,
            core_ty: Some(core_ty),
        });
    }
    Ok(PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: predicate.into(),
            args: vec![value],
        },
        bindings,
    })
}
