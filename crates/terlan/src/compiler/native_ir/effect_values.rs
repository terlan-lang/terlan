//! Concrete existential storage for the standard deferred-effect descriptors.

use crate::terlan_typeck::{CoreExpr, CoreTupleTypeElem, CoreType};

/// Internal storage name; it is not a source-level Dynamic escape hatch.
pub(super) const ERASED_VALUE_TYPE: &str = "$aot.erased_value";

/// Replaces only the standard descriptors' declared existential fields.
/// Ordinary Dynamic values and unrelated aliases retain their existing checks.
pub(super) fn storage_type(canonical: &str, body: &CoreType) -> CoreType {
    let fields: &[&str] = match canonical {
        "std.core.Effect.Mapped" => &["effect", "mapper"],
        "std.core.Effect.FlatMap" => &["effect", "next"],
        "std.core.Effect.Failed" => &["error"],
        _ => return body.clone(),
    };
    let mut storage = body.clone();
    if let CoreType::Tuple(elements) = &mut storage {
        for element in elements {
            if let CoreTupleTypeElem::Field { name, ty } = element {
                if fields.contains(&name.as_str()) && *ty == CoreType::Dynamic {
                    *ty = CoreType::Named(ERASED_VALUE_TYPE.to_string());
                }
            }
        }
    }
    storage
}

/// Makes field erasure explicit so the ordinary checked-cast lowerer owns it.
pub(super) fn boxed_field(value: &CoreExpr, expected: &CoreType) -> Option<CoreExpr> {
    if !matches!(expected, CoreType::Named(name) if name == ERASED_VALUE_TYPE) {
        return None;
    }
    Some(CoreExpr::Cast {
        expr: Box::new(value.clone()),
        target_type: expected.clone(),
    })
}

#[cfg(test)]
#[path = "effect_values_test.rs"]
mod tests;
