use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::bitstring::VmBitString;

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ReplValue {
    Unit,
    Bool(bool),
    Int(i64),
    Float(String),
    String(String),
    Bytes(Arc<[u8]>),
    BitString(VmBitString),
    Atom(String),
    Type(String),
    Tuple(Vec<ReplValue>),
    Record {
        name: String,
        fields: Vec<(String, ReplValue)>,
    },
    List(Vec<ReplValue>),
    Map(Vec<(ReplValue, ReplValue)>),
    Set(Vec<ReplValue>),
}

impl Hash for ReplValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Unit => {}
            Self::Bool(value) => value.hash(state),
            Self::Int(value) => value.hash(state),
            Self::Float(value) | Self::String(value) | Self::Atom(value) | Self::Type(value) => {
                value.hash(state)
            }
            Self::Bytes(value) => value.hash(state),
            Self::BitString(value) => value.hash(state),
            Self::Tuple(items) | Self::List(items) | Self::Set(items) => items.hash(state),
            Self::Record { name, fields } => {
                name.hash(state);
                fields.hash(state);
            }
            Self::Map(entries) => entries.hash(state),
        }
    }
}

impl ReplValue {
    pub(crate) fn render(&self) -> String {
        match self {
            Self::Unit => "Unit".to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.clone(),
            Self::String(value) => format!("\"{value}\""),
            Self::Bytes(value) => format!(
                "Bytes([{}])",
                value
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::BitString(value) => format!(
                "BitString(bits = {}, bytes = [{}])",
                value.bit_len(),
                value
                    .packed_bytes()
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Atom(value) => format!("Atom[\"{value}\"]"),
            Self::Type(value) => value.clone(),
            Self::Tuple(items) | Self::List(items) | Self::Set(items) => format!(
                "{{{}}}",
                items
                    .iter()
                    .map(Self::render)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Record { name, fields } => format!(
                "{name}({})",
                fields
                    .iter()
                    .map(|(field, value)| format!("{field} = {}", value.render()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Map(entries) => format!(
                "Map({})",
                entries
                    .iter()
                    .map(|(key, value)| format!("{{{}, {}}}", key.render(), value.render()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}
