//! Iterative retained-size accounting for runtime boundary values.

#[cfg(test)]
use super::accounting_support::opaque_value;
use super::accounting_support::{add_logical_string, add_sequence_storage, checked_add_size};
use super::{ReplValue, VmValueSizeError};

pub(super) const LOGICAL_VALUE_SLOT_BYTES: usize = 8;
pub(super) const LOGICAL_SEQUENCE_HEADER_BYTES: usize = 16;
pub(super) const LOGICAL_STRING_HEADER_BYTES: usize = 16;

/// Computes a deterministic retained-size estimate without recursive host calls.
pub(crate) fn logical_value_bytes(value: &ReplValue) -> Result<usize, VmValueSizeError> {
    let mut total = 0usize;
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            ReplValue::Closure(value) => {
                let descriptor = &value.descriptor;
                checked_add_size(&mut total, 40)?;
                checked_add_size(
                    &mut total,
                    24 * (descriptor.parameters().len()
                        + descriptor.results().len()
                        + descriptor.captures().len()),
                )?;
                add_sequence_storage(&mut total, value.captures.len())?;
                pending.extend(value.captures.iter());
            }
            ReplValue::Unit => {}
            ReplValue::Bool(_) => checked_add_size(&mut total, 1)?,
            ReplValue::Int(_) => checked_add_size(&mut total, 8)?,
            ReplValue::Float(value) | ReplValue::String(value) | ReplValue::Atom(value) => {
                add_logical_string(&mut total, value)?
            }
            #[cfg(test)]
            ReplValue::Type(value) => add_logical_string(&mut total, value)?,
            ReplValue::StringBytes(value) => {
                checked_add_size(&mut total, LOGICAL_STRING_HEADER_BYTES)?;
                checked_add_size(&mut total, value.len())?;
            }
            ReplValue::Bytes(value) => {
                checked_add_size(&mut total, LOGICAL_SEQUENCE_HEADER_BYTES)?;
                checked_add_size(&mut total, value.len())?;
            }
            ReplValue::BitString(value) => {
                checked_add_size(&mut total, LOGICAL_SEQUENCE_HEADER_BYTES)?;
                checked_add_size(&mut total, 8)?;
                checked_add_size(&mut total, value.byte_len())?;
            }
            ReplValue::Tuple(items) | ReplValue::List(items) | ReplValue::Set(items) => {
                add_sequence_storage(&mut total, items.len())?;
                pending.extend(items);
            }
            ReplValue::Record { name, fields } => {
                add_logical_string(&mut total, name)?;
                add_sequence_storage(&mut total, fields.len())?;
                for (field, value) in fields {
                    add_logical_string(&mut total, field)?;
                    pending.push(value);
                }
            }
            ReplValue::Map(entries) => {
                add_sequence_storage(
                    &mut total,
                    entries
                        .len()
                        .checked_mul(2)
                        .ok_or(VmValueSizeError::Overflow)?,
                )?;
                for (key, value) in entries {
                    pending.push(key);
                    pending.push(value);
                }
            }
            #[cfg(test)]
            ReplValue::MapIndexed(map) => {
                checked_add_size(&mut total, LOGICAL_SEQUENCE_HEADER_BYTES)?;
                let mut retained_error = None;
                map.visit_retained_entries(|key, value| {
                    if retained_error.is_some() {
                        return;
                    }
                    if let Err(error) = checked_add_size(&mut total, LOGICAL_VALUE_SLOT_BYTES * 2) {
                        retained_error = Some(error);
                        return;
                    }
                    pending.push(key);
                    if let Some(value) = value {
                        pending.push(value);
                    }
                });
                if let Some(error) = retained_error {
                    return Err(error);
                }
            }
            #[cfg(test)]
            ReplValue::Iterator { items, .. } => {
                add_sequence_storage(&mut total, items.len())?;
                checked_add_size(&mut total, 8)?;
                pending.extend(items);
            }
            #[cfg(test)]
            ReplValue::RandomGenerator(_) => return Err(opaque_value("RandomGenerator")),
        }
    }
    Ok(total)
}
