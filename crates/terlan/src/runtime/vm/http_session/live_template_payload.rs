use super::{normalize_live_template_subscriber_field, validate_live_template_source_location};
use crate::runtime::vm::ReplValue;

/// Validated source location for one actor-bound template interpolation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpSessionLiveTemplateSourceSpan {
    pub(crate) module: String,
    pub(crate) line: u32,
    pub(crate) column: u32,
}

impl VmHttpSessionLiveTemplateSourceSpan {
    pub(crate) fn new(module: impl Into<String>, line: u32, column: u32) -> Result<Self, String> {
        let module = module.into();
        let module =
            normalize_live_template_subscriber_field(&module, "HTTP live-template source module")?
                .to_string();
        validate_live_template_source_location(line, column)?;
        Ok(Self {
            module,
            line,
            column,
        })
    }
}

pub(super) fn validate_live_template_patch_payload(
    value: &ReplValue,
    source: &VmHttpSessionLiveTemplateSourceSpan,
) -> Result<(), String> {
    if let Some(kind) = unsupported_live_template_patch_value(value) {
        return Err(format!(
            "invalid_template_actor_return_type: {}:{}:{}: HTTP live-template actor return type {kind} cannot be serialized as a typed patch payload",
            source.module, source.line, source.column
        ));
    }
    Ok(())
}

fn unsupported_live_template_patch_value(value: &ReplValue) -> Option<&'static str> {
    match value {
        ReplValue::Bytes(_) => Some("Bytes"),
        ReplValue::BitString(_) => Some("BitString"),
        #[cfg(test)]
        ReplValue::RandomGenerator(_) => Some("RandomGenerator"),
        ReplValue::Type(_) => Some("Type"),
        #[cfg(test)]
        ReplValue::Iterator { .. } => Some("Iterator"),
        ReplValue::Tuple(values) | ReplValue::List(values) | ReplValue::Set(values) => values
            .iter()
            .find_map(unsupported_live_template_patch_value),
        ReplValue::Record { fields, .. } => fields
            .iter()
            .find_map(|(_, value)| unsupported_live_template_patch_value(value)),
        ReplValue::Map(entries) => entries.iter().find_map(|(key, value)| {
            unsupported_live_template_patch_value(key)
                .or_else(|| unsupported_live_template_patch_value(value))
        }),
        #[cfg(test)]
        ReplValue::MapIndexed(map) => map.to_entries().iter().find_map(|(key, value)| {
            unsupported_live_template_patch_value(key)
                .or_else(|| unsupported_live_template_patch_value(value))
        }),
        ReplValue::Unit
        | ReplValue::Int(_)
        | ReplValue::Float(_)
        | ReplValue::String(_)
        | ReplValue::Atom(_)
        | ReplValue::Bool(_) => None,
    }
}
