use std::hash::{Hash, Hasher};
use std::sync::Arc;

use super::bitstring::VmBitString;
#[cfg(test)]
use super::map_value::VmMapValue;
#[cfg(test)]
use crate::terlan_native::random as native_random;

#[path = "value_hash.rs"]
mod hash;

/// Transitional scalar and runtime-service value used at native boundaries.
///
/// Inputs:
/// - Constructed by native image calls and reusable runtime services.
///
/// Output:
/// - A backend-neutral value that can be rendered for the public REPL.
///
/// Transformation:
/// - Does not retain compiler IR or executable function bodies.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ReplValue {
    Unit,
    Int(i64),
    Float(String),
    String(String),
    Bytes(Arc<[u8]>),
    BitString(VmBitString),
    Atom(String),
    Bool(bool),
    #[cfg(test)]
    RandomGenerator(native_random::Generator),
    Type(String),
    Tuple(Vec<ReplValue>),
    Record {
        name: String,
        fields: Vec<(String, ReplValue)>,
    },
    List(Vec<ReplValue>),
    Map(Vec<(ReplValue, ReplValue)>),
    #[cfg(test)]
    MapIndexed(VmMapValue<ReplValue, ReplValue>),
    Set(Vec<ReplValue>),
    #[cfg(test)]
    Iterator {
        items: Vec<ReplValue>,
        index: usize,
    },
}

impl Hash for ReplValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        #[cfg(not(test))]
        {
            let hash = match self.stable_hash() {
                Ok(hash) => hash,
                Err(error) => match error {},
            };
            hash.hash(state);
            return;
        }

        #[cfg(test)]
        if let Ok(hash) = self.stable_hash() {
            hash.hash(state);
            return;
        }

        #[cfg(test)]
        std::mem::discriminant(self).hash(state);
        #[cfg(test)]
        match self {
            Self::Tuple(items) | Self::List(items) | Self::Set(items) => items.hash(state),
            Self::Record { name, fields } => {
                name.hash(state);
                fields.hash(state);
            }
            Self::Map(entries) => entries.hash(state),
            #[cfg(test)]
            Self::MapIndexed(map) => map.to_entries().hash(state),
            #[cfg(test)]
            Self::Iterator { items, index } => {
                items.hash(state);
                index.hash(state);
            }
            _ => unreachable!("portable VM values must have a stable hash"),
        }
    }
}

impl ReplValue {
    /// Renders a VM value with Terlan source-facing spelling.
    ///
    /// Inputs:
    /// - `self`: evaluated backend-neutral value.
    ///
    /// Output:
    /// - Stable text shown in text-mode REPL result events.
    ///
    /// Transformation:
    /// - Converts primitive and aggregate values to Terlan-facing syntax,
    ///   keeping `Unit`, `true`, and `false` distinct from backend atoms.
    pub(crate) fn render(&self) -> String {
        match self {
            Self::Unit => "Unit".to_string(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.clone(),
            Self::String(value) => format!("\"{}\"", escape_string(value)),
            Self::Bytes(value) => {
                let rendered = value
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Bytes([{rendered}])")
            }
            Self::BitString(value) => {
                let rendered = value
                    .packed_bytes()
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "BitString(bits = {}, bytes = [{rendered}])",
                    value.bit_len()
                )
            }
            Self::Atom(value) => format!("Atom[\"{}\"]", escape_string(value)),
            Self::Bool(value) => value.to_string(),
            #[cfg(test)]
            Self::RandomGenerator(_) => "<random-generator>".to_string(),
            Self::Type(value) => value.clone(),
            Self::Tuple(items) => {
                let rendered = items
                    .iter()
                    .map(Self::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{rendered}}}")
            }
            Self::Record { name, fields } => {
                let rendered = fields
                    .iter()
                    .map(|(field, value)| format!("{field} = {}", value.render()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name}({rendered})")
            }
            Self::List(items) => {
                let rendered = items
                    .iter()
                    .map(Self::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{rendered}]")
            }
            Self::Map(entries) => {
                if entries.is_empty() {
                    return "Map()".to_string();
                }
                let rendered = entries
                    .iter()
                    .map(|(key, value)| format!("{{{}, {}}}", key.render(), value.render()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Map({rendered})")
            }
            #[cfg(test)]
            Self::MapIndexed(map) => render_map_entries(&map.to_entries()),
            Self::Set(items) => {
                let rendered = items
                    .iter()
                    .map(Self::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Set({rendered})")
            }
            #[cfg(test)]
            Self::Iterator { items, index } => {
                format!(
                    "<iterator index={index} remaining={}>",
                    items.len().saturating_sub(*index)
                )
            }
        }
    }
}

#[cfg(test)]
fn render_map_entries(entries: &[(ReplValue, ReplValue)]) -> String {
    if entries.is_empty() {
        return "Map()".to_string();
    }
    let rendered = entries
        .iter()
        .map(|(key, value)| format!("{{{}, {}}}", key.render(), value.render()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("Map({rendered})")
}

impl ReplValue {
    /// Returns map entries in stable insertion order.
    pub(crate) fn map_entries_owned(&self) -> Option<Vec<(ReplValue, ReplValue)>> {
        match self {
            Self::Map(entries) => Some(entries.clone()),
            #[cfg(test)]
            Self::MapIndexed(map) => Some(map.to_entries()),
            _ => None,
        }
    }
}

/// Escapes a string for REPL source-style rendering.
///
/// Inputs:
/// - `value`: runtime string.
///
/// Output:
/// - Escaped string payload without surrounding quotes.
///
/// Transformation:
/// - Escapes the small set of characters needed by REPL display tests.
fn escape_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
