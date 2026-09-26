//! Versioned logical state encoding shared with the maintained durable backend.

use super::{encode_value, write_len, write_text, write_u64, EncodingBuffer};
use crate::runtime::vm::distributed_state::{
    VmDistributedStateEntry as Entry, VmDistributedStatePolicy as Policy,
};
use crate::runtime::vm::VmRuntimeResult;

/// Bounds metadata and decoded entry allocation independently of payload bytes.
pub(super) const MAX_ENTRIES: usize = 16_384;
/// Bounds aggregate allocation independently of compact on-disk term tags.
pub(super) const MAX_VALUES: usize = 65_536;
/// Four nonempty length-prefixed strings, a version, a policy, and a value tag.
pub(super) const MIN_ENTRY_BYTES: usize = 30;

/// Encodes sorted logical entries, never heaps, continuations, or native handles.
///
/// The caller's byte limit is also capped by the storage engine's payload limit.
/// Sorting borrows entries; payloads are encoded directly without cloning them.
pub(crate) fn encode_tetf_checkpoint(
    entries: &[Entry],
    declared_atoms: &[String],
    maximum: usize,
) -> VmRuntimeResult<Vec<u8>> {
    if entries.len() > MAX_ENTRIES {
        return Err("error[tetf_checkpoint.count]: too many state entries".into());
    }
    let mut remaining = MAX_VALUES;
    for entry in entries {
        validate_value_budget(&entry.value, &mut remaining, 0)?;
    }
    let mut bytes = EncodingBuffer::new(maximum.min(terlan_storage::MAX_CHECKPOINT_BYTES));
    bytes.extend_from_slice(super::MAGIC)?;
    bytes.push(super::VERSION)?;
    bytes.push(super::PROFILE_LOGICAL_CHECKPOINT)?;
    write_len(&mut bytes, entries.len())?;
    if entries.len() > bytes.remaining() / MIN_ENTRY_BYTES {
        return Err("error[tetf_size]: checkpoint entries exceed byte limit".into());
    }
    let mut ordered = entries.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.scope.cmp(&right.scope));
    if ordered
        .windows(2)
        .any(|pair| pair[0].scope == pair[1].scope)
    {
        return Err("error[tetf_checkpoint.scope]: duplicate state scope".into());
    }
    for entry in ordered {
        validate_entry(entry)?;
        write_text(&mut bytes, &entry.scope.namespace)?;
        write_text(&mut bytes, &entry.scope.key)?;
        write_text(&mut bytes, &entry.owner_node_id)?;
        write_u64(&mut bytes, entry.version.sequence)?;
        write_text(&mut bytes, &entry.version.node_id)?;
        bytes.push(match entry.policy {
            Policy::WinnerTakesAll => 1,
            Policy::LastWriterWins => 2,
            Policy::Merge => 3,
            Policy::ExplicitUserResolution => 4,
        })?;
        encode_value(&entry.value, declared_atoms, &mut bytes, 0)?;
    }
    Ok(bytes.into_vec())
}

/// Counts borrowed source values before allocating the encoded representation.
fn validate_value_budget(
    value: &super::ReplValue,
    remaining: &mut usize,
    depth: usize,
) -> VmRuntimeResult<()> {
    if depth >= super::MAX_NESTING_DEPTH {
        return Err("error[tetf_depth]: checkpoint value nesting exceeds limit".into());
    }
    *remaining = remaining
        .checked_sub(1)
        .ok_or("error[tetf_checkpoint.values]: too many logical values")?;
    use super::ReplValue;
    match value {
        ReplValue::Tuple(items) | ReplValue::List(items) | ReplValue::Set(items) => {
            for item in items {
                validate_value_budget(item, remaining, depth + 1)?;
            }
        }
        ReplValue::Map(entries) => {
            for (key, value) in entries {
                validate_value_budget(key, remaining, depth + 1)?;
                validate_value_budget(value, remaining, depth + 1)?;
            }
        }
        ReplValue::Record { fields, .. } => {
            for (_, value) in fields {
                validate_value_budget(value, remaining, depth + 1)?;
            }
        }
        #[cfg(test)]
        ReplValue::MapIndexed(_) => {
            return Err(
                "error[tetf_checkpoint.values]: test-only indexed map is not portable".into(),
            )
        }
        _ => {}
    }
    Ok(())
}

/// Applies the source-visible metadata contract to encoder and decoder alike.
pub(super) fn validate_entry(entry: &Entry) -> VmRuntimeResult<()> {
    if entry.scope.namespace.is_empty()
        || entry.scope.key.is_empty()
        || entry.owner_node_id.is_empty()
        || entry.version.node_id.is_empty()
        || entry.version.sequence == 0
        || entry.version.sequence > i64::MAX as u64
    {
        return Err("error[tetf_checkpoint.metadata]: invalid state identity or version".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "checkpoint_test.rs"]
mod tests;
