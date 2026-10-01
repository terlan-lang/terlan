//! Ownership-preserving admission of paired callback output.

use super::*;
use terlan_runtime_abi::OwnedDescriptor;

/// Opaque next state and independently optional deliveries to the two peers.
pub type PairedTransition = (String, Option<String>, Option<String>);

/// Admits the source {state, first_payload, second_payload} result.
pub fn paired_transition<V: DescriptorValue>(value: V) -> Result<PairedTransition> {
    let OwnedDescriptor::Tuple(parts) = value.into_descriptor() else {
        return Err(error(
            "expected paired transition {String, Option[String], Option[String]}",
        ));
    };
    let [next, first, second]: [V; 3] = parts.try_into().map_err(|_| {
        error("expected paired transition {String, Option[String], Option[String]}")
    })?;
    let OwnedDescriptor::String(next) = next.into_descriptor() else {
        return Err(error("expected paired transition state String"));
    };
    Ok((next, delivery(first)?, delivery(second)?))
}

fn delivery<V: DescriptorValue>(value: V) -> Result<Option<String>> {
    match value.into_descriptor() {
        OwnedDescriptor::Atom(name) if name == "none" => Ok(None),
        OwnedDescriptor::Record(name, fields) if name == "None" && fields.is_empty() => Ok(None),
        OwnedDescriptor::Record(name, mut fields) if name == "Some" && fields.len() == 1 => {
            match fields.pop() {
                Some((field, value)) if field == "value" => match value.into_descriptor() {
                    OwnedDescriptor::String(value) => Ok(Some(value)),
                    _ => Err(error("expected Some(String) delivery")),
                },
                _ => Err(error("expected Some value field")),
            }
        }
        _ => Err(error("expected optional paired delivery")),
    }
}

#[cfg(test)]
#[path = "transition_test.rs"]
mod tests;
