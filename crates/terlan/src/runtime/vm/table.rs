#[path = "table/atomic.rs"]
pub(crate) mod atomic;
#[path = "table/counter.rs"]
pub(crate) mod counter;

use super::process::{VmProcessId, VmProcessState, VmProcessTable};
use super::ReplValue;

/// VM-owned table identifier.
///
/// Inputs:
/// - Monotonic runtime allocation.
///
/// Output:
/// - Stable local table id used by VM storage primitives.
///
/// Transformation:
/// - Keeps Terlan table identity independent from ETS table ids, host maps, or
///   runtime-specific storage handles.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmTableId(u64);

impl VmTableId {
    /// Returns the numeric table id.
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }

    /// Creates a table id for adversarial VM runtime tests.
    #[cfg(test)]
    pub(crate) fn from_raw_for_test(value: u64) -> Self {
        Self(value)
    }
}

/// Access policy for a VM-owned table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmTableAccess {
    OwnerOnly,
    #[cfg(test)]
    PublicRead,
    #[cfg(test)]
    PublicReadWrite,
}

/// Stored key/value entry in a VM-owned table.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmTableEntry {
    pub(crate) key: ReplValue,
    pub(crate) value: ReplValue,
}

/// Live VM-owned table record.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmTableRecord {
    pub(crate) id: VmTableId,
    pub(crate) owner: VmProcessId,
    pub(crate) name: String,
    pub(crate) access: VmTableAccess,
    entries: Vec<VmTableEntry>,
}

/// Read-only table row for runtime inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(any(test, feature = "benchmark-tools"))]
pub(crate) struct VmTableSnapshot {
    pub(crate) id: VmTableId,
    pub(crate) owner: VmProcessId,
    pub(crate) name: String,
    pub(crate) access: VmTableAccess,
    pub(crate) len: usize,
}

/// Table lifecycle and mutation event.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmTableEvent {
    Created {
        id: VmTableId,
        owner: VmProcessId,
    },
    Inserted {
        id: VmTableId,
        key: ReplValue,
    },
    Replaced {
        id: VmTableId,
        key: ReplValue,
        old_value: ReplValue,
    },
    Deleted {
        id: VmTableId,
        key: ReplValue,
        old_value: ReplValue,
    },
    CleanedUpOnExit {
        id: VmTableId,
        owner: VmProcessId,
    },
}

/// VM-owned local key/value table collection.
///
/// Inputs:
/// - Live VM processes and table storage operations.
///
/// Output:
/// - Local key/value tables with owner identity, explicit access policy,
///   cleanup on process exit, stable diagnostics, and inspection rows.
///
/// Transformation:
/// - Provides Terlan-owned local table semantics without importing ETS table
///   names, OTP process ownership rules, or host-runtime storage handles.
#[derive(Debug, Default)]
pub(crate) struct VmTableStore {
    next_id: u64,
    tables: Vec<VmTableRecord>,
}

impl VmTableStore {
    /// Creates a table owned by a live process.
    pub(crate) fn create(
        &mut self,
        processes: &VmProcessTable,
        owner: VmProcessId,
        name: impl Into<String>,
        access: VmTableAccess,
    ) -> Result<VmTableEvent, String> {
        ensure_live_process(processes, owner, "owner")?;

        self.next_id = self.next_id.saturating_add(1);
        let id = VmTableId(self.next_id);
        self.tables.push(VmTableRecord {
            id,
            owner,
            name: name.into(),
            access,
            entries: Vec::new(),
        });
        Ok(VmTableEvent::Created { id, owner })
    }

    /// Inserts or replaces a key/value pair.
    pub(crate) fn insert(
        &mut self,
        processes: &VmProcessTable,
        requester: VmProcessId,
        table: VmTableId,
        key: ReplValue,
        value: ReplValue,
    ) -> Result<VmTableEvent, String> {
        ensure_live_process(processes, requester, "requester")?;
        let record = self.live_table_mut(table)?;
        ensure_write_access(record, requester)?;

        if let Some(entry) = record.entries.iter_mut().find(|entry| entry.key == key) {
            let old_value = std::mem::replace(&mut entry.value, value);
            return Ok(VmTableEvent::Replaced {
                id: table,
                key,
                old_value,
            });
        }

        record.entries.push(VmTableEntry {
            key: key.clone(),
            value,
        });
        Ok(VmTableEvent::Inserted { id: table, key })
    }

    /// Looks up a key in a table when read policy allows it.
    pub(crate) fn lookup(
        &self,
        processes: &VmProcessTable,
        requester: VmProcessId,
        table: VmTableId,
        key: &ReplValue,
    ) -> Result<Option<ReplValue>, String> {
        let record = self.readable_table(processes, requester, table)?;
        Ok(record
            .entries
            .iter()
            .find(|entry| &entry.key == key)
            .map(|entry| entry.value.clone()))
    }

    /// Exports table entries when read policy allows it.
    #[cfg(test)]
    pub(crate) fn entries(
        &self,
        processes: &VmProcessTable,
        requester: VmProcessId,
        table: VmTableId,
    ) -> Result<Vec<VmTableEntry>, String> {
        let record = self.readable_table(processes, requester, table)?;
        Ok(record.entries.clone())
    }

    /// Returns the first entry in deterministic insertion order.
    #[cfg(test)]
    pub(crate) fn first_entry(
        &self,
        processes: &VmProcessTable,
        requester: VmProcessId,
        table: VmTableId,
    ) -> Result<Option<VmTableEntry>, String> {
        let record = self.readable_table(processes, requester, table)?;
        Ok(record.entries.first().cloned())
    }

    /// Returns the last entry in deterministic insertion order.
    #[cfg(test)]
    pub(crate) fn last_entry(
        &self,
        processes: &VmProcessTable,
        requester: VmProcessId,
        table: VmTableId,
    ) -> Result<Option<VmTableEntry>, String> {
        let record = self.readable_table(processes, requester, table)?;
        Ok(record.entries.last().cloned())
    }

    /// Returns the entry after an existing key in insertion order.
    #[cfg(test)]
    pub(crate) fn next_entry(
        &self,
        processes: &VmProcessTable,
        requester: VmProcessId,
        table: VmTableId,
        key: &ReplValue,
    ) -> Result<Option<VmTableEntry>, String> {
        let record = self.readable_table(processes, requester, table)?;
        let position = table_entry_position(record, table, key)?;
        Ok(record.entries.get(position.saturating_add(1)).cloned())
    }

    /// Returns the entry before an existing key in insertion order.
    #[cfg(test)]
    pub(crate) fn previous_entry(
        &self,
        processes: &VmProcessTable,
        requester: VmProcessId,
        table: VmTableId,
        key: &ReplValue,
    ) -> Result<Option<VmTableEntry>, String> {
        let record = self.readable_table(processes, requester, table)?;
        let position = table_entry_position(record, table, key)?;
        Ok(position
            .checked_sub(1)
            .and_then(|previous| record.entries.get(previous))
            .cloned())
    }

    /// Deletes a key from a table when write policy allows it.
    pub(crate) fn delete(
        &mut self,
        processes: &VmProcessTable,
        requester: VmProcessId,
        table: VmTableId,
        key: &ReplValue,
    ) -> Result<Option<VmTableEvent>, String> {
        ensure_live_process(processes, requester, "requester")?;
        let record = self.live_table_mut(table)?;
        ensure_write_access(record, requester)?;

        let Some(index) = record.entries.iter().position(|entry| &entry.key == key) else {
            return Ok(None);
        };
        let entry = record.entries.remove(index);
        Ok(Some(VmTableEvent::Deleted {
            id: table,
            key: entry.key,
            old_value: entry.value,
        }))
    }

    /// Cleans up every table owned by an exiting process.
    pub(crate) fn cleanup_owner(&mut self, owner: VmProcessId) -> Vec<VmTableEvent> {
        let mut events = Vec::new();
        let mut retained = Vec::with_capacity(self.tables.len());
        for table in self.tables.drain(..) {
            if table.owner == owner {
                events.push(VmTableEvent::CleanedUpOnExit {
                    id: table.id,
                    owner,
                });
            } else {
                retained.push(table);
            }
        }
        self.tables = retained;
        events
    }

    /// Returns live table rows for runtime inspection.
    #[cfg(any(test, feature = "benchmark-tools"))]
    pub(crate) fn snapshots(&self) -> Vec<VmTableSnapshot> {
        self.tables
            .iter()
            .map(|table| VmTableSnapshot {
                id: table.id,
                owner: table.owner,
                name: table.name.clone(),
                access: table.access,
                len: table.entries.len(),
            })
            .collect()
    }

    fn live_table(&self, table: VmTableId) -> Result<&VmTableRecord, String> {
        self.tables
            .iter()
            .find(|record| record.id == table)
            .ok_or_else(|| stale_table_diagnostic(table))
    }

    fn live_table_mut(&mut self, table: VmTableId) -> Result<&mut VmTableRecord, String> {
        self.tables
            .iter_mut()
            .find(|record| record.id == table)
            .ok_or_else(|| stale_table_diagnostic(table))
    }

    fn readable_table<'table>(
        &'table self,
        processes: &VmProcessTable,
        requester: VmProcessId,
        table: VmTableId,
    ) -> Result<&'table VmTableRecord, String> {
        ensure_live_process(processes, requester, "requester")?;
        let record = self.live_table(table)?;
        ensure_read_access(record, requester)?;
        Ok(record)
    }
}

#[cfg(test)]
fn table_entry_position(
    record: &VmTableRecord,
    table: VmTableId,
    key: &ReplValue,
) -> Result<usize, String> {
    record
        .entries
        .iter()
        .position(|entry| &entry.key == key)
        .ok_or_else(|| format!("missing key in VM table {}", table.as_u64()))
}

fn ensure_live_process(
    processes: &VmProcessTable,
    pid: VmProcessId,
    role: &str,
) -> Result<(), String> {
    let process = processes
        .get(pid)
        .ok_or_else(|| format!("missing {role} process {}", pid.as_u64()))?;
    if matches!(process.state, VmProcessState::Exited(_)) {
        return Err(format!("{role} process {} has exited", pid.as_u64()));
    }
    Ok(())
}

fn ensure_read_access(record: &VmTableRecord, requester: VmProcessId) -> Result<(), String> {
    match record.access {
        VmTableAccess::OwnerOnly if requester != record.owner => Err(table_access_diagnostic(
            record.id,
            record.owner,
            requester,
            "read",
        )),
        _ => Ok(()),
    }
}

fn ensure_write_access(record: &VmTableRecord, requester: VmProcessId) -> Result<(), String> {
    match record.access {
        #[cfg(test)]
        VmTableAccess::PublicReadWrite => Ok(()),
        VmTableAccess::OwnerOnly if requester == record.owner => Ok(()),
        #[cfg(test)]
        VmTableAccess::PublicRead if requester == record.owner => Ok(()),
        VmTableAccess::OwnerOnly => Err(table_access_diagnostic(
            record.id,
            record.owner,
            requester,
            "write",
        )),
        #[cfg(test)]
        VmTableAccess::PublicRead => Err(table_access_diagnostic(
            record.id,
            record.owner,
            requester,
            "write",
        )),
    }
}

fn table_access_diagnostic(
    table: VmTableId,
    owner: VmProcessId,
    requester: VmProcessId,
    operation: &str,
) -> String {
    format!(
        "process {} cannot {operation} table {} owned by process {}",
        requester.as_u64(),
        table.as_u64(),
        owner.as_u64()
    )
}

fn stale_table_diagnostic(table: VmTableId) -> String {
    format!("stale VM table handle {}", table.as_u64())
}

#[cfg(test)]
#[path = "table_test.rs"]
#[cfg(test)]
mod table_test;
