//! Structural admission of the source reconnect policy's result.

use super::*;

/// Admits Result[Option[{String, Int}], String] without interpreting request URLs.
pub fn restoration_identity<V: DescriptorValue>(value: &V) -> Result<Option<(String, i64)>> {
    let (name, fields) = variant(value, &[("Ok", &["value"]), ("Err", &["reason"])])?;
    if name == "Err" {
        return Err(error(text(fields[0])?));
    }
    let option = fields[0];
    if matches!(
        option.descriptor_view(),
        DescriptorView::Atom("none") | DescriptorView::Record("None", [])
    ) {
        return Ok(None);
    }
    let [identity] = record(option, "Some", ["value"])?;
    let DescriptorView::Tuple(parts) = identity.descriptor_view() else {
        return Err(error("expected reconnect {room, role} tuple"));
    };
    let [room, role] = parts else {
        return Err(error("expected reconnect {room, role} tuple"));
    };
    let room = text(room)?;
    if room.is_empty() {
        return Err(error("reconnect room must not be empty"));
    }
    let DescriptorView::Int(role @ 1..=2) = role.descriptor_view() else {
        return Err(error("reconnect role must be 1 or 2"));
    };
    Ok(Some((room, role)))
}

#[cfg(test)]
#[path = "restoration_test.rs"]
mod tests;
