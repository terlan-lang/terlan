use super::{
    VmPostgresDriverWait, VmPostgresIoInterest, VmPostgresRuntime, VmPostgresTransactionState,
};
use crate::runtime::vm::process::VmProcessId;
#[cfg(test)]
use std::collections::BTreeMap;

/// Sanitized pending database operation exposed to VM inspection surfaces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmPostgresPendingRequestSnapshot {
    pub(crate) request_id: u64,
    pub(crate) owner: VmProcessId,
    pub(crate) operation: String,
    pub(crate) sql_fingerprint: Option<String>,
    pub(crate) deadline_tick: u64,
}

/// Per-process database ownership counts exposed without native handles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmPostgresOwnerSnapshot {
    pub(crate) owner: VmProcessId,
    pub(crate) pending_requests: usize,
    pub(crate) registered_pools: usize,
    pub(crate) open_pools: usize,
    pub(crate) registered_connections: usize,
    pub(crate) open_connections: usize,
    pub(crate) active_transactions: usize,
    pub(crate) terminal_transactions: usize,
    pub(crate) prepared_statements: usize,
    pub(crate) result_sets: usize,
    pub(crate) rows: usize,
}

/// Cumulative database cleanup decisions exposed to runtime tooling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmPostgresCleanupSnapshot {
    pub(crate) cancellations: u64,
    pub(crate) rollbacks: u64,
    pub(crate) releases: u64,
    pub(crate) closed_pools: u64,
    pub(crate) dropped_prepared_statements: u64,
    pub(crate) dropped_result_sets: u64,
    pub(crate) dropped_rows: u64,
}

/// Sanitized reactor wait state. Host socket descriptors never cross inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmPostgresDriverWaitSnapshot {
    pub(crate) request_id: u64,
    pub(crate) interest: VmPostgresIoInterest,
}

/// Correlated Postgres state included in the VM actor observation boundary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmPostgresInspectionSnapshot {
    pub(crate) pending_requests: Vec<VmPostgresPendingRequestSnapshot>,
    pub(crate) owners: Vec<VmPostgresOwnerSnapshot>,
    pub(crate) cleanup: VmPostgresCleanupSnapshot,
    pub(crate) driver_wait: Option<VmPostgresDriverWaitSnapshot>,
}

#[derive(Default)]
#[cfg(test)]
struct OwnerCounts {
    pending_requests: usize,
    registered_pools: usize,
    open_pools: usize,
    registered_connections: usize,
    open_connections: usize,
    active_transactions: usize,
    terminal_transactions: usize,
    prepared_statements: usize,
    result_sets: usize,
    rows: usize,
}

impl VmPostgresRuntime {
    /// Captures deterministic Postgres state without exposing SQL or native resources.
    #[cfg(test)]
    pub(crate) fn inspection_snapshot(
        &self,
        driver_wait: Option<VmPostgresDriverWait>,
    ) -> VmPostgresInspectionSnapshot {
        let mut owners = BTreeMap::<VmProcessId, OwnerCounts>::new();
        let pending_requests = self
            .pending
            .values()
            .map(|pending| {
                owners.entry(pending.owner).or_default().pending_requests += 1;
                VmPostgresPendingRequestSnapshot {
                    request_id: pending.scheduled.request_id.value,
                    owner: pending.owner,
                    operation: pending.operation.name().to_string(),
                    sql_fingerprint: pending.operation.sql_fingerprint(),
                    deadline_tick: pending.scheduled.deadline_tick,
                }
            })
            .collect();
        for state in self.pools.values() {
            let counts = owners.entry(state.owner).or_default();
            counts.registered_pools += 1;
            counts.open_pools += usize::from(state.open);
        }
        for state in self.connections.values() {
            let counts = owners.entry(state.owner).or_default();
            counts.registered_connections += 1;
            counts.open_connections += usize::from(state.open);
        }
        for state in self.transactions.values() {
            let counts = owners.entry(state.owner).or_default();
            if state.state == VmPostgresTransactionState::Active {
                counts.active_transactions += 1;
            } else {
                counts.terminal_transactions += 1;
            }
        }
        for state in self.prepared.values() {
            owners.entry(state.owner).or_default().prepared_statements += 1;
        }
        for owner in self.result_sets.values() {
            owners.entry(*owner).or_default().result_sets += 1;
        }
        for state in self.rows.values() {
            owners.entry(state.owner).or_default().rows += 1;
        }
        VmPostgresInspectionSnapshot {
            pending_requests,
            owners: owners
                .into_iter()
                .map(|(owner, counts)| VmPostgresOwnerSnapshot {
                    owner,
                    pending_requests: counts.pending_requests,
                    registered_pools: counts.registered_pools,
                    open_pools: counts.open_pools,
                    registered_connections: counts.registered_connections,
                    open_connections: counts.open_connections,
                    active_transactions: counts.active_transactions,
                    terminal_transactions: counts.terminal_transactions,
                    prepared_statements: counts.prepared_statements,
                    result_sets: counts.result_sets,
                    rows: counts.rows,
                })
                .collect(),
            cleanup: VmPostgresCleanupSnapshot {
                cancellations: self.cleanup.cancellations,
                rollbacks: self.cleanup.rollbacks,
                releases: self.cleanup.releases,
                closed_pools: self.cleanup.closed_pools,
                dropped_prepared_statements: self.cleanup.dropped_prepared_statements,
                dropped_result_sets: self.cleanup.dropped_result_sets,
                dropped_rows: self.cleanup.dropped_rows,
            },
            driver_wait: driver_wait.map(|wait| VmPostgresDriverWaitSnapshot {
                request_id: wait.request_id.value,
                interest: wait.interest,
            }),
        }
    }
}
