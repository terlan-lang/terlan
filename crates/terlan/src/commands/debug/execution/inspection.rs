//! Process, supervisor, timer, and resource debugger projections.

use super::NativeDebuggerRuntime;

impl NativeDebuggerRuntime<'_> {
    pub(super) fn capture_processes(&mut self) {
        self.report.process_snapshots = self
            .shard
            .debugger_process_snapshots()
            .into_iter()
            .map(|snapshot| format!("{snapshot:?}"))
            .collect();
        let supervisor_history = self.shard.debugger_supervisor_history();
        self.report.events.extend(
            supervisor_history
                .iter()
                .map(|entry| format!("supervisor_restart:{entry}")),
        );
        if self.trace_filters.contains("supervisors") {
            self.report.events.extend(
                supervisor_history
                    .iter()
                    .map(|entry| format!("trace:supervisor:{entry}")),
            );
        }
        self.report.events.push(format!(
            "processes:{}:supervisor_restarts={}",
            self.report.process_snapshots.len(),
            supervisor_history.len()
        ));
    }

    pub(super) fn capture_resources(&mut self) {
        let resources = self.shard.debugger_resource_snapshots();
        for resource in &resources {
            if crate::commands::debug::tracing::resource_enabled(
                &self.trace_filters,
                &resource.kind,
            ) {
                self.report.events.push(format!(
                    "trace:resource:{}:{}:{}",
                    resource.owner.as_u64(),
                    resource.kind,
                    resource.label
                ));
            }
        }
        self.report.resource_snapshots = resources
            .into_iter()
            .map(|snapshot| format!("{snapshot:?}"))
            .collect();
        self.report.timer_snapshots = self
            .shard
            .debugger_timer_snapshots()
            .into_iter()
            .map(|snapshot| format!("{snapshot:?}"))
            .collect();
        self.report.events.push(format!(
            "resources:{}:timers:{}",
            self.report.resource_snapshots.len(),
            self.report.timer_snapshots.len()
        ));
    }
}
