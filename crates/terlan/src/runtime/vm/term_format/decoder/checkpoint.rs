//! Strict decoding of portable logical checkpoint entries.

use super::Decoder;
use crate::runtime::vm::distributed_state::{
    VmDistributedStateEntry as Entry, VmDistributedStatePolicy as Policy,
    VmDistributedStateScope as Scope, VmDistributedStateVersion as Version,
};
use crate::runtime::vm::term_format::checkpoint::{
    validate_entry, MAX_ENTRIES, MAX_VALUES, MIN_ENTRY_BYTES,
};
use crate::runtime::vm::VmRuntimeResult;

/// Restores only canonical, bounded logical values admitted by the current image.
///
/// Database integrity is verified by the storage engine before calling this
/// decoder. A valid digest does not bypass schema, atom, or resource checks.
pub(crate) fn decode_tetf_checkpoint(
    bytes: &[u8],
    declared_atoms: &[String],
) -> VmRuntimeResult<Vec<Entry>> {
    if bytes.len() > terlan_storage::MAX_CHECKPOINT_BYTES {
        return Err("error[tetf_size]: checkpoint payload exceeds storage limit".into());
    }
    let mut decoder = Decoder::new(bytes, declared_atoms);
    decoder.remaining_value_slots = MAX_VALUES;
    decoder.read_header(super::super::PROFILE_LOGICAL_CHECKPOINT)?;
    let count = decoder.read_len()?;
    if count > MAX_ENTRIES {
        return Err("error[tetf_checkpoint.count]: too many state entries".into());
    }
    decoder.require_remaining_items(count, MIN_ENTRY_BYTES)?;
    decoder.reserve_value_slots(count)?;
    let mut entries: Vec<Entry> = Vec::with_capacity(count);
    for _ in 0..count {
        let scope = Scope::new(decoder.read_text()?, decoder.read_text()?)?;
        if entries
            .last()
            .is_some_and(|previous| previous.scope >= scope)
        {
            return Err(
                "error[tetf_checkpoint.scope]: state scopes are not strictly ordered".into(),
            );
        }
        let owner_node_id = decoder.read_text()?;
        let version = Version::new(decoder.read_u64()?, decoder.read_text()?)?;
        let policy = match decoder.read_u8()? {
            1 => Policy::WinnerTakesAll,
            2 => Policy::LastWriterWins,
            3 => Policy::Merge,
            4 => Policy::ExplicitUserResolution,
            _ => return Err("error[tetf_checkpoint.policy]: invalid conflict policy".into()),
        };
        let entry = Entry {
            scope,
            owner_node_id,
            version,
            policy,
            value: decoder.decode_value(0)?,
        };
        validate_entry(&entry)?;
        entries.push(entry);
    }
    decoder.finish()?;
    Ok(entries)
}
