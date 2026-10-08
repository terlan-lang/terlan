//! Ownership-preserving admission of paired callback output.

use super::*;
use terlan_runtime_abi::OwnedDescriptor;

/// Opaque next state and independently optional deliveries to the two peers.
pub type PairedTransition = (String, Option<String>, Option<String>);

/// State and peer metadata are absent until a second peer has joined.
pub type PairContext = (String, i64, String, String);

/// Admits the source policy's explicit success or failure before publishing output.
pub fn paired_callback_transition<V: DescriptorValue>(value: V) -> Result<PairedTransition> {
    let OwnedDescriptor::Record(name, fields) = value.into_descriptor() else {
        return Err(error("expected paired callback Result"));
    };
    let [(field, value)]: [(String, V); 1] = fields
        .try_into()
        .map_err(|_| error("expected paired callback Result payload"))?;
    match (name.as_str(), field.as_str()) {
        ("Ok", "value") => paired_transition(value),
        ("Err", "reason") => match value.into_descriptor() {
            OwnedDescriptor::String(message) => Err(error(message)),
            _ => Err(error("expected paired callback error String")),
        },
        _ => Err(error("expected paired callback Result payload")),
    }
}

/// Admits both source-prepared match payloads before either can be published.
pub fn paired_match<V: DescriptorValue>(value: V) -> Result<(String, String)> {
    let OwnedDescriptor::Tuple(parts) = value.into_descriptor() else {
        return Err(error("expected paired match {String, String}"));
    };
    let [first, second]: [V; 2] = parts
        .try_into()
        .map_err(|_| error("expected paired match {String, String}"))?;
    match (first.into_descriptor(), second.into_descriptor()) {
        (OwnedDescriptor::String(first), OwnedDescriptor::String(second)) => Ok((first, second)),
        _ => Err(error("expected paired match payload Strings")),
    }
}

/// Admits the source {state, first_payload, second_payload} result.
fn paired_transition<V: DescriptorValue>(value: V) -> Result<PairedTransition> {
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
