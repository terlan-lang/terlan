use std::{collections::BTreeMap, path::Path};

use serde::Serialize;

use super::{VmPostgresRuntime, VmPostgresRuntimeEvent, VmPostgresTransactionState};

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryLifecycleSummary {
    dispatched: u64,
    succeeded: u64,
    failed: u64,
    cancelled: u64,
    timed_out: u64,
    owner_exited: u64,
}

impl QueryLifecycleSummary {
    fn record(&mut self, event: &VmPostgresRuntimeEvent) {
        match event.outcome.as_str() {
            "dispatched" => self.dispatched += 1,
            "cancelled" => self.cancelled += 1,
            "timed_out" => self.timed_out += 1,
            "owner_exited" => self.owner_exited += 1,
            _ if event.error_code.is_some() => self.failed += 1,
            _ => self.succeeded += 1,
        }
    }
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransactionOutcomeSummary {
    committed: u64,
    rolled_back: u64,
    failed: u64,
    cancelled: u64,
    timed_out: u64,
    owner_exited: u64,
}

impl TransactionOutcomeSummary {
    fn record(&mut self, event: &VmPostgresRuntimeEvent) {
        if event.outcome == "dispatched" {
            return;
        }
        match event.outcome.as_str() {
            "cancelled" => self.cancelled += 1,
            "timed_out" => self.timed_out += 1,
            "owner_exited" => self.owner_exited += 1,
            _ if event.error_code.is_some() => self.failed += 1,
            _ if event.operation == "commit" => self.committed += 1,
            _ if event.operation == "rollback" => self.rolled_back += 1,
            _ => self.failed += 1,
        }
    }
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct CancellationDecisionSummary {
    explicit: u64,
    timed_out: u64,
    owner_exited: u64,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct RowDecodeFailureSummary {
    count: u64,
    error_codes: BTreeMap<String, u64>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeEvidenceSummary {
    query_lifecycle: QueryLifecycleSummary,
    transaction_outcomes: TransactionOutcomeSummary,
    cancellation_decisions: CancellationDecisionSummary,
    row_decode_failures: RowDecodeFailureSummary,
}

impl RuntimeEvidenceSummary {
    fn from_events(events: &std::collections::VecDeque<VmPostgresRuntimeEvent>) -> Self {
        let mut summary = Self::default();
        for event in events {
            if matches!(event.operation.as_str(), "query" | "query_one" | "execute") {
                summary.query_lifecycle.record(event);
            }
            if matches!(event.operation.as_str(), "commit" | "rollback") {
                summary.transaction_outcomes.record(event);
            }
            match event.outcome.as_str() {
                "cancelled" => summary.cancellation_decisions.explicit += 1,
                "timed_out" => summary.cancellation_decisions.timed_out += 1,
                "owner_exited" => summary.cancellation_decisions.owner_exited += 1,
                _ => {}
            }
            if event.operation == "decode" {
                if let Some(error_code) = &event.error_code {
                    summary.row_decode_failures.count += 1;
                    *summary
                        .row_decode_failures
                        .error_codes
                        .entry(error_code.clone())
                        .or_default() += 1;
                }
            }
        }
        summary
    }
}

impl VmPostgresRuntime {
    pub(crate) fn write_report(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create Postgres report directory: {error}"))?;
        }
        let terminal_transactions = self
            .transactions
            .values()
            .filter(|state| state.state != VmPostgresTransactionState::Active)
            .count();
        let configured_pool_capacity = self
            .pools
            .values()
            .map(|pool| pool.max_connections)
            .sum::<usize>();
        let active_transactions = self
            .transactions
            .values()
            .filter(|state| state.state == VmPostgresTransactionState::Active)
            .count();
        let evidence = RuntimeEvidenceSummary::from_events(&self.events);
        let no_pending_requests = self.pending.is_empty();
        let no_reserved_credits = self.deadlines.reserved_credits() == 0;
        let no_live_resources = self.pools.values().all(|state| !state.open)
            && self.connections.values().all(|state| !state.open)
            && active_transactions == 0
            && self.prepared.is_empty()
            && self.result_sets.is_empty()
            && self.rows.is_empty();
        let driver = crate::terlan_native::postgres::driver_provenance();
        let report = serde_json::json!({
            "schema": "terlan-vm-postgres-runtime-report-v1",
            "driver": {
                "crate": driver.client_crate,
                "version": driver.client_version,
                "runtime": driver.runtime,
            },
            "pool": {"crate": driver.pool_crate, "version": driver.pool_version},
            "runtimeOwner": "terlan-vm",
            "scheduler": {"parkedRequests": self.pending.len(), "reservedCredits": self.deadlines.reserved_credits()},
            "poolConfiguration": {
                "poolCount": self.pools.len(),
                "configuredConnectionCapacity": configured_pool_capacity,
            },
            "resources": {
                "pools": self.pools.len(),
                "connections": self.connections.len(),
                "transactions": self.transactions.len(),
                "preparedStatements": self.prepared.len(),
                "resultSets": self.result_sets.len(),
                "rows": self.rows.len(),
                "terminalTransactions": terminal_transactions,
            },
            "events": self.events,
            "evidence": evidence,
            "cleanup": {
                "cancellations": self.cleanup.cancellations,
                "rollbacks": self.cleanup.rollbacks,
                "releases": self.cleanup.releases,
                "closedPools": self.cleanup.closed_pools,
                "droppedPreparedStatements": self.cleanup.dropped_prepared_statements,
                "droppedResultSets": self.cleanup.dropped_result_sets,
                "droppedRows": self.cleanup.dropped_rows,
            },
            "resourcePolicy": {
                "ownership": "process_scoped",
                "transfer": "forbidden",
                "checkpoint": "forbidden",
                "debug": "opaque",
                "drop": "owner_cleanup",
            },
            "security": {"rawDriverErrors": false, "rawHandles": false, "credentialsReported": false, "sqlTextReported": false},
            "cleanupProof": {
                "typedTerminalStates": active_transactions == 0,
                "noPendingRequests": no_pending_requests,
                "noReservedCredits": no_reserved_credits,
                "noLiveResources": no_live_resources,
                "ownerCleanupComplete": no_pending_requests && no_reserved_credits && no_live_resources,
                "ownerCleanupControls": ["cancel", "rollback", "release", "close_pool"],
            },
        });
        let json = serde_json::to_string_pretty(&report)
            .map_err(|error| format!("failed to serialize Postgres runtime report: {error}"))?;
        std::fs::write(path, format!("{json}\n"))
            .map_err(|error| format!("failed to write Postgres runtime report: {error}"))
    }
}
