use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::call_count::{VmCallCountRegistry, VmCallCountSnapshot, VmCallCountState};
use super::call_memory::{VmCallMemoryRegistry, VmCallMemorySnapshot, VmCallMemoryState};
use super::call_time::{VmCallTimeRegistry, VmCallTimeSnapshot, VmCallTimeState};
use super::code_server::VmCodeServer;
use super::dynamic_module::VmDynamicModuleRegistry;
use super::failure::{VmFailureProcessSnapshot, VmFailureRuntime};
use super::fatal_diagnostics::VmFatalDiagnosticBundle;
use super::local_trace::VmLocalTraceRegistry;
use super::memory::{
    VmAccountedMessageSend, VmMemoryAccountant, VmMemoryLimits, VmMemoryPressureOutcome,
    VmProcessMemoryMetrics,
};
use super::meta_trace::VmMetaTraceRegistry;
use super::postgres::{
    VmPostgresDriverControl, VmPostgresInspectionSnapshot, VmPostgresLibpqWorker, VmPostgresRuntime,
};
use super::process::{
    VmExitReason, VmMessage, VmProcessId, VmProcessRegistryError, VmProcessSnapshot,
    VmProcessSource, VmProcessTable,
};
use super::process_alias::{VmProcessAlias, VmProcessAliasError, VmProcessAliasTable};
use super::process_environment::{VmRuntimeEnvironmentProfile, VmRuntimeEnvironmentSnapshot};
use super::reference::VmReferenceAllocator;
use super::resource::{VmResourceSnapshot, VmResourceTable};
use super::scheduler::{
    VmScheduler, VmSchedulerDecision, VmSchedulerMetrics, VmSchedulerRun, VmSchedulerSlice,
};
use super::statistics::VmRuntimeStatisticsDelta;
use super::system_information::VmSystemInformationSnapshot;
use super::system_profile::{VmSystemProfileCursor, VmSystemProfileSnapshot};
use super::timer::{VmTimerId, VmTimerMetrics, VmTimerSnapshot, VmTimerTable};
use super::ReplValue;

use self::actor_timer::VmDelayedActorMessage;

const ACTOR_OPERATION_REDUCTIONS: u64 = 1;

/// Result of an actor receive operation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmActorReceive {
    Message(VmMessage),
    Blocked,
    Timeout,
}
/// Opaque execution identity for one live actor invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmActorContext {
    process_id: VmProcessId,
}

/// Correlated read-only state from one actor-runtime inspection boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmActorObservationSnapshot {
    pub(crate) environment: VmRuntimeEnvironmentSnapshot,
    pub(crate) processes: Vec<VmProcessSnapshot>,
    pub(crate) scheduler: VmSchedulerMetrics,
    pub(crate) timers: Vec<VmTimerSnapshot>,
    pub(crate) timer_metrics: VmTimerMetrics,
    pub(crate) postgres: VmPostgresInspectionSnapshot,
}

impl VmActorContext {
    /// Returns the current actor process identity.
    pub(crate) fn process_id(self) -> VmProcessId {
        self.process_id
    }
}

/// Local VM actor runtime facade.
///
/// Inputs:
/// - Process spawn requests, name registrations, sends, receives, and
///   scheduler polls.
///
/// Output:
/// - Actor/process effects expressed through Terlan-owned VM primitives.
///
/// Transformation:
/// - Composes the process table and cooperative scheduler into the first
///   higher-level actor surface without depending on OTP process machinery.
#[derive(Debug)]
pub(crate) struct VmActorRuntime {
    processes: VmProcessTable,
    aliases: VmProcessAliasTable,
    failures: VmFailureRuntime,
    references: VmReferenceAllocator,
    scheduler: VmScheduler,
    memory: VmMemoryAccountant,
    resources: VmResourceTable,
    code_server: VmCodeServer,
    dynamic_modules: VmDynamicModuleRegistry,
    timers: VmTimerTable,
    delayed_messages: BTreeMap<VmTimerId, VmDelayedActorMessage>,
    native_continuations: BTreeMap<(u64, u64), VmProcessId>,
    native_continuations_by_owner: BTreeMap<VmProcessId, (u64, u64)>,
    explicit_native_suspensions: BTreeSet<VmProcessId>,
    postgres: VmPostgresRuntime,
    postgres_driver: VmPostgresLibpqWorker,
    postgres_controls: VecDeque<VmPostgresDriverControl>,
    call_counts: VmCallCountRegistry,
    call_memory: VmCallMemoryRegistry,
    call_time: VmCallTimeRegistry,
    local_trace: VmLocalTraceRegistry,
    meta_trace: VmMetaTraceRegistry,
    latest_fatal_diagnostic: Option<VmFatalDiagnosticBundle>,
    native_image_diagnostics:
        Option<crate::runtime::vm::native_image_diagnostics::VmNativeImageDiagnosticMetadata>,
}

impl Default for VmActorRuntime {
    fn default() -> Self {
        Self::with_memory_limits(
            VmMemoryLimits::new(64 * 1024 * 1024, 256 * 1024 * 1024)
                .expect("actor runtime memory limits are valid"),
        )
    }
}

impl VmActorRuntime {
    /// Creates an actor runtime with explicit validated VM memory limits.
    pub(crate) fn with_memory_limits(limits: VmMemoryLimits) -> Self {
        Self::with_runtime_identity(limits, "local", 1)
            .expect("default actor runtime identity is valid")
    }

    /// Creates an actor runtime with explicit memory and reference identity.
    pub(crate) fn with_runtime_identity(
        limits: VmMemoryLimits,
        node_id: impl Into<String>,
        epoch: u64,
    ) -> Result<Self, String> {
        Ok(Self {
            processes: VmProcessTable::default(),
            aliases: VmProcessAliasTable::default(),
            failures: VmFailureRuntime::default(),
            references: VmReferenceAllocator::new(node_id, epoch)?,
            scheduler: VmScheduler::default(),
            memory: VmMemoryAccountant::new(limits),
            resources: VmResourceTable::default(),
            code_server: VmCodeServer::default(),
            dynamic_modules: VmDynamicModuleRegistry::default(),
            timers: VmTimerTable::default(),
            delayed_messages: BTreeMap::new(),
            native_continuations: BTreeMap::new(),
            native_continuations_by_owner: BTreeMap::new(),
            explicit_native_suspensions: BTreeSet::new(),
            postgres: VmPostgresRuntime::new(1_024),
            postgres_driver: VmPostgresLibpqWorker::default(),
            postgres_controls: VecDeque::new(),
            call_counts: VmCallCountRegistry::default(),
            call_memory: VmCallMemoryRegistry::default(),
            call_time: VmCallTimeRegistry::default(),
            local_trace: VmLocalTraceRegistry::default(),
            meta_trace: VmMetaTraceRegistry::default(),
            latest_fatal_diagnostic: None,
            native_image_diagnostics: None,
        })
    }

    /// Spawns and schedules a root actor.
    pub(crate) fn spawn_root(&mut self, source: VmProcessSource) -> VmProcessId {
        let pid = self.processes.spawn_root(source);
        self.scheduler
            .enqueue_runnable(&self.processes, pid)
            .expect("fresh root process must be runnable");
        pid
    }

    /// Spawns and schedules a child actor.
    pub(crate) fn spawn_child(
        &mut self,
        parent: VmProcessId,
        source: VmProcessSource,
    ) -> Result<VmProcessId, String> {
        self.spawn_child_with_options(parent, source, VmActorSpawnOptions::default())
            .map(|spawned| spawned.pid)
            .map_err(|error| {
                if error
                    == format!(
                        "cannot spawn child from missing process {}",
                        parent.as_u64()
                    )
                {
                    format!("missing parent process {}", parent.as_u64())
                } else {
                    error
                }
            })
    }

    /// Returns the process table for inspection.
    pub(crate) fn processes(&self) -> &VmProcessTable {
        &self.processes
    }

    /// Returns the newest bounded snapshot captured before an abnormal exit.
    pub(crate) fn latest_fatal_diagnostic(&self) -> Option<&VmFatalDiagnosticBundle> {
        self.latest_fatal_diagnostic.as_ref()
    }

    /// Updates the admitted native generation attached to subsequent fatal diagnostics.
    pub(crate) fn set_native_image_diagnostics(
        &mut self,
        diagnostics: crate::runtime::vm::native_image_diagnostics::VmNativeImageDiagnosticMetadata,
    ) {
        self.native_image_diagnostics = Some(diagnostics);
    }

    /// Enables call accounting for one exact Terlan function identity.
    pub(crate) fn enable_function_call_count(&mut self, source: VmProcessSource) {
        self.call_counts.enable(source);
    }

    /// Removes call accounting and its retained count for one function.
    pub(crate) fn disable_function_call_count(&mut self, source: &VmProcessSource) -> bool {
        self.call_counts.disable(source)
    }

    /// Pauses accounting without resetting the retained count.
    pub(crate) fn pause_function_call_count(
        &mut self,
        source: &VmProcessSource,
    ) -> Result<(), String> {
        self.call_counts.pause(source)
    }

    /// Resets an enabled counter and resumes recording from zero.
    pub(crate) fn restart_function_call_count(
        &mut self,
        source: &VmProcessSource,
    ) -> Result<(), String> {
        self.call_counts.restart(source)
    }

    /// Records exact function entries from the VM dispatch boundary.
    pub(crate) fn record_function_entries(
        &mut self,
        source: &VmProcessSource,
        entries: u64,
    ) -> Result<bool, String> {
        self.call_counts.record_entries(source, entries)
    }

    /// Returns typed state for one exact function counter.
    pub(crate) fn function_call_count_state(&self, source: &VmProcessSource) -> VmCallCountState {
        self.call_counts.state(source)
    }

    /// Returns immutable source-ordered function call-count rows.
    pub(crate) fn function_call_count_snapshots(&self) -> Vec<VmCallCountSnapshot> {
        self.call_counts.snapshots()
    }

    /// Enables logical allocation attribution for one function identity.
    pub(crate) fn enable_function_call_memory(&mut self, source: VmProcessSource) {
        self.call_memory.enable(source);
    }

    /// Removes one function allocation profile and all retained process rows.
    pub(crate) fn disable_function_call_memory(&mut self, source: &VmProcessSource) -> bool {
        self.call_memory.disable(source)
    }

    /// Clears retained allocation rows while leaving attribution enabled.
    pub(crate) fn restart_function_call_memory(
        &mut self,
        source: &VmProcessSource,
    ) -> Result<(), String> {
        self.call_memory.restart(source)
    }

    /// Attributes validated logical allocation totals to a live process call.
    pub(crate) fn record_function_allocations(
        &mut self,
        source: &VmProcessSource,
        pid: VmProcessId,
        calls: u64,
        allocated_bytes: u64,
    ) -> Result<bool, String> {
        self.ensure_live_process(pid, "record function allocations for")?;
        self.call_memory
            .record_allocations(source, pid, calls, allocated_bytes)
    }

    /// Returns typed allocation state for one exact function identity.
    pub(crate) fn function_call_memory_state(&self, source: &VmProcessSource) -> VmCallMemoryState {
        self.call_memory.state(source)
    }

    /// Returns immutable function and process ordered allocation profiles.
    pub(crate) fn function_call_memory_snapshots(&self) -> Vec<VmCallMemorySnapshot> {
        self.call_memory.snapshots()
    }

    /// Enables exclusive logical execution-time attribution for one function.
    pub(crate) fn enable_function_call_time(&mut self, source: VmProcessSource) {
        self.call_time.enable(source);
    }

    /// Removes one execution-time profile and all retained process rows.
    pub(crate) fn disable_function_call_time(&mut self, source: &VmProcessSource) -> bool {
        self.call_time.disable(source)
    }

    /// Pauses execution-time attribution without resetting retained rows.
    pub(crate) fn pause_function_call_time(
        &mut self,
        source: &VmProcessSource,
    ) -> Result<(), String> {
        self.call_time.pause(source)
    }

    /// Clears retained execution rows and resumes attribution.
    pub(crate) fn restart_function_call_time(
        &mut self,
        source: &VmProcessSource,
    ) -> Result<(), String> {
        self.call_time.restart(source)
    }

    /// Attributes exact calls and exclusive scheduler ticks to a live process.
    pub(crate) fn record_function_time(
        &mut self,
        source: &VmProcessSource,
        pid: VmProcessId,
        calls: u64,
        exclusive_ticks: u64,
    ) -> Result<bool, String> {
        self.ensure_live_process(pid, "record function time for")?;
        self.call_time
            .record_execution(source, pid, calls, exclusive_ticks)
    }

    /// Returns typed execution-time state for one exact function identity.
    pub(crate) fn function_call_time_state(&self, source: &VmProcessSource) -> VmCallTimeState {
        self.call_time.state(source)
    }

    /// Returns immutable function- and process-ordered execution profiles.
    pub(crate) fn function_call_time_snapshots(&self) -> Vec<VmCallTimeSnapshot> {
        self.call_time.snapshots()
    }

    /// Returns deterministic VM-owned native resource rows.
    pub(crate) fn resource_snapshots(&self) -> Vec<VmResourceSnapshot> {
        self.resources.snapshots()
    }

    /// Returns deterministic VM-owned active timer rows.
    pub(crate) fn timer_snapshots(&self) -> Vec<VmTimerSnapshot> {
        self.timers.snapshots()
    }

    /// Returns all live actor process ids in deterministic allocation order.
    pub(crate) fn live_process_ids(&self) -> Vec<VmProcessId> {
        self.processes.live_process_ids()
    }

    /// Returns whether an actor identity currently names a live process.
    pub(crate) fn is_alive(&self, pid: VmProcessId) -> bool {
        self.processes.is_alive(pid)
    }

    /// Returns live process information while hiding missing and exited
    /// identities, matching the actor-language process inspection boundary.
    pub(crate) fn process_info_snapshot(&self, pid: VmProcessId) -> Option<VmProcessSnapshot> {
        self.processes.live_snapshot(pid)
    }

    /// Creates an execution context for one live actor.
    pub(crate) fn context(&self, pid: VmProcessId) -> Result<VmActorContext, String> {
        self.ensure_live_process(pid, "create context for")?;
        Ok(VmActorContext { process_id: pid })
    }

    /// Returns VM-owned logical memory metrics for one actor process.
    pub(crate) fn memory_metrics(&self, pid: VmProcessId) -> Option<&VmProcessMemoryMetrics> {
        self.memory.process_metrics(pid)
    }

    /// Returns scheduler reductions attributed to VM memory work for one actor.
    pub(crate) fn memory_reductions(&self, pid: VmProcessId) -> u64 {
        self.scheduler.memory_reductions(pid)
    }

    /// Returns scheduler reductions attributed to memory across all actors.
    pub(crate) fn total_memory_reductions(&self) -> u64 {
        self.scheduler.total_memory_reductions()
    }

    /// Returns the number of scheduled actor processes.
    pub(crate) fn scheduled_len(&self) -> usize {
        self.scheduler.queued_len()
    }

    /// Captures one immutable actor-runtime environment snapshot.
    pub(crate) fn environment_snapshot(
        &self,
        profile: VmRuntimeEnvironmentProfile,
    ) -> Result<VmRuntimeEnvironmentSnapshot, String> {
        VmRuntimeEnvironmentSnapshot::capture(
            profile,
            &self.processes,
            &self.scheduler,
            &self.timers,
        )
    }

    /// Captures current VM statistics and validates their cumulative delta
    /// from an earlier snapshot of the same runtime profile.
    pub(crate) fn statistics_since(
        &self,
        profile: VmRuntimeEnvironmentProfile,
        earlier: &VmRuntimeEnvironmentSnapshot,
    ) -> Result<(VmRuntimeEnvironmentSnapshot, VmRuntimeStatisticsDelta), String> {
        let current = self.environment_snapshot(profile)?;
        let delta = current.statistics_delta_since(earlier)?;
        Ok((current, delta))
    }

    /// Captures portable VM identity, capacity, and resource gauges without
    /// exposing host-runtime or allocator implementation details.
    pub(crate) fn system_information_snapshot(
        &self,
        profile: VmRuntimeEnvironmentProfile,
    ) -> Result<VmSystemInformationSnapshot, String> {
        let environment = self.environment_snapshot(profile)?;
        Ok(VmSystemInformationSnapshot::from_environment(&environment))
    }

    /// Returns a cursor at the current end of the deterministic scheduler
    /// transition stream without enabling mutable global profiling state.
    pub(crate) fn system_profile_cursor(&self) -> VmSystemProfileCursor {
        VmSystemProfileCursor::at(self.scheduler.metrics().queue_transitions.len())
    }

    /// Captures every scheduler transition since a previously obtained cursor.
    pub(crate) fn system_profile_since(
        &self,
        cursor: VmSystemProfileCursor,
    ) -> Result<VmSystemProfileSnapshot, String> {
        VmSystemProfileSnapshot::capture(
            self.scheduler.metrics(),
            &self.processes.snapshots(),
            cursor,
        )
    }

    /// Captures correlated actor, scheduler, and timer state without mutation.
    pub(crate) fn observation_snapshot(
        &self,
        profile: VmRuntimeEnvironmentProfile,
    ) -> Result<VmActorObservationSnapshot, String> {
        Ok(VmActorObservationSnapshot {
            environment: self.environment_snapshot(profile)?,
            processes: self.processes.snapshots(),
            scheduler: self.scheduler.metrics().clone(),
            timers: self.timers.snapshots(),
            timer_metrics: self.timers.metrics().clone(),
            postgres: self.postgres_inspection_snapshot(),
        })
    }

    /// Captures sanitized database state without requiring environment metadata.
    pub(crate) fn postgres_inspection_snapshot(&self) -> VmPostgresInspectionSnapshot {
        self.postgres
            .inspection_snapshot(self.postgres_driver.wait())
    }

    /// Returns VM-owned links and monitors for one actor process.
    pub(crate) fn failure_snapshot(
        &self,
        pid: VmProcessId,
    ) -> Result<VmFailureProcessSnapshot, String> {
        self.failures
            .snapshot(&self.processes, pid)
            .map_err(|_| format!("cannot inspect missing process {}", pid.as_u64()))
    }

    /// Sends a message to a process and schedules the recipient.
    pub(crate) fn send(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
    ) -> Result<u64, String> {
        let accounted =
            self.memory
                .send_value_message(&mut self.processes, sender, recipient, payload)?;
        self.finish_actor_send(sender, recipient, accounted)
    }

    /// Sends one exactly typed native value and schedules its recipient.
    pub(crate) fn send_typed(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
        boundary_type: crate::runtime::native_image::TvmBoundaryType,
    ) -> Result<u64, String> {
        let accounted = self.memory.send_typed_value_message(
            &mut self.processes,
            sender,
            recipient,
            payload,
            boundary_type,
        )?;
        self.finish_actor_send(sender, recipient, accounted)
    }

    /// Sends one receiver-owned managed graph and schedules its recipient.
    pub(crate) fn send_typed_managed(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        fragment: super::process::VmManagedMailboxToken,
        boundary_type: crate::runtime::native_image::TvmBoundaryType,
    ) -> Result<u64, String> {
        let accounted = self.memory.send_typed_managed_message(
            &mut self.processes,
            sender,
            recipient,
            fragment,
            boundary_type,
        )?;
        self.finish_actor_send(sender, recipient, accounted)
    }

    /// Sends a message ahead of ordinary mailbox traffic while preserving
    /// FIFO order relative to other priority messages.
    pub(crate) fn send_priority(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
    ) -> Result<u64, String> {
        let accounted = self.memory.send_priority_value_message(
            &mut self.processes,
            sender,
            recipient,
            payload,
        )?;
        self.finish_actor_send(sender, recipient, accounted)
    }

    fn finish_actor_send(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        accounted: VmAccountedMessageSend,
    ) -> Result<u64, String> {
        self.scheduler
            .charge_memory_reductions(
                &mut self.processes,
                recipient,
                accounted.pressure.requested_bytes,
            )
            .expect("accounted actor recipient remains live while charging send reductions");
        if accounted.pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            debug_assert!(accounted.publication.is_none());
            return Err(format!(
                "actor process {} exceeded its VM mailbox memory hard limit",
                recipient.as_u64()
            ));
        }
        self.charge_actor_reductions(sender, ACTOR_OPERATION_REDUCTIONS);
        let publication = accounted
            .publication
            .expect("accepted accounted actor send must produce a publication receipt");
        debug_assert_eq!(publication.recipient(), recipient);
        debug_assert_eq!(
            publication.accounted_bytes(),
            accounted.pressure.requested_bytes
        );
        self.scheduler
            .wake_process(&mut self.processes, publication.recipient())
            .expect("recipient was checked by send before wake");
        Ok(publication.message_id())
    }

    /// Sends a message from the current actor to itself.
    pub(crate) fn send_self(
        &mut self,
        context: VmActorContext,
        payload: ReplValue,
    ) -> Result<u64, String> {
        self.send(context.process_id, context.process_id, payload)
    }

    /// Sends a message to a named process.
    pub(crate) fn send_named(
        &mut self,
        sender: VmProcessId,
        recipient_name: &str,
        payload: ReplValue,
    ) -> Result<u64, String> {
        self.processes.validate_sender(sender)?;
        let recipient = self
            .processes
            .lookup_name(recipient_name)
            .ok_or_else(|| format!("actor name `{recipient_name}` is not registered"))?;
        self.send(sender, recipient, payload)
    }

    /// Sends a message through one opaque process alias.
    pub(crate) fn send_alias(
        &mut self,
        sender: VmProcessId,
        recipient_alias: VmProcessAlias,
        payload: ReplValue,
    ) -> Result<u64, String> {
        self.processes.validate_sender(sender)?;
        let recipient = self
            .aliases
            .route(recipient_alias)
            .ok_or_else(|| actor_alias_error(VmProcessAliasError::MissingAlias(recipient_alias)))?;
        let message_id = self.send(sender, recipient.owner, payload)?;
        self.aliases.consume_reply(recipient_alias);
        Ok(message_id)
    }

    /// Sends through a priority-enabled alias using the priority mailbox lane.
    pub(crate) fn send_alias_priority(
        &mut self,
        sender: VmProcessId,
        recipient_alias: VmProcessAlias,
        payload: ReplValue,
    ) -> Result<u64, String> {
        self.processes.validate_sender(sender)?;
        let route = self
            .aliases
            .route(recipient_alias)
            .ok_or_else(|| actor_alias_error(VmProcessAliasError::MissingAlias(recipient_alias)))?;
        if !route.priority {
            return Err(actor_alias_error(VmProcessAliasError::PriorityNotEnabled(
                recipient_alias,
            )));
        }
        let message_id = self.send_priority(sender, route.owner, payload)?;
        self.aliases.consume_reply(recipient_alias);
        Ok(message_id)
    }

    /// Delivers an exit signal through an alias without consuming reply-mode
    /// aliases. Priority delivery requires a priority-enabled capability.
    pub(crate) fn send_alias_exit_signal(
        &mut self,
        sender: VmProcessId,
        recipient_alias: VmProcessAlias,
        reason: ReplValue,
        priority: bool,
    ) -> Result<u64, String> {
        self.processes.validate_sender(sender)?;
        let route = self
            .aliases
            .route(recipient_alias)
            .ok_or_else(|| actor_alias_error(VmProcessAliasError::MissingAlias(recipient_alias)))?;
        if priority && !route.priority {
            return Err(actor_alias_error(VmProcessAliasError::PriorityNotEnabled(
                recipient_alias,
            )));
        }
        let payload = ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(sender.as_u64() as i64),
            reason,
        ]);
        let message_id = if priority {
            self.processes
                .send_priority_system_message(sender, route.owner, payload)?
        } else {
            self.processes
                .send_system_message(sender, route.owner, payload)?
        };
        self.charge_actor_reductions(sender, ACTOR_OPERATION_REDUCTIONS);
        self.scheduler
            .wake_process(&mut self.processes, route.owner)
            .expect("alias signal recipient was validated before wake");
        Ok(message_id)
    }

    /// Receives the oldest message or blocks the actor when the mailbox is
    /// empty.
    pub(crate) fn receive_next_or_block(
        &mut self,
        pid: VmProcessId,
    ) -> Result<VmActorReceive, String> {
        self.ensure_live_process(pid, "receive")?;
        self.charge_receive_reduction(pid);
        if let Some(message) = self.memory.receive_message(&mut self.processes, pid)? {
            self.scheduler
                .charge_memory_reductions(&mut self.processes, pid, message.accounted_bytes)
                .expect("receiving actor remains live while charging mailbox reductions");
            Ok(VmActorReceive::Message(message))
        } else {
            self.processes
                .get_mut(pid)
                .expect("process was checked before receive")
                .block();
            Ok(VmActorReceive::Blocked)
        }
    }

    /// Receives the first selected message or blocks the actor.
    pub(crate) fn selective_receive_or_block(
        &mut self,
        pid: VmProcessId,
        predicate: impl FnMut(&VmMessage) -> bool,
    ) -> Result<VmActorReceive, String> {
        self.ensure_live_process(pid, "receive")?;
        let outcome =
            self.memory
                .selective_receive_message_with_scan(&mut self.processes, pid, predicate)?;
        let scan_reductions = u64::try_from(outcome.inspected_messages)
            .unwrap_or(u64::MAX)
            .max(ACTOR_OPERATION_REDUCTIONS);
        self.charge_receive_reductions(pid, scan_reductions);
        if let Some(message) = outcome.message {
            self.scheduler
                .charge_memory_reductions(&mut self.processes, pid, message.accounted_bytes)
                .expect("selective receiving actor remains live while charging mailbox reductions");
            Ok(VmActorReceive::Message(message))
        } else {
            self.processes
                .get_mut(pid)
                .expect("process was checked before receive")
                .block();
            Ok(VmActorReceive::Blocked)
        }
    }

    /// Receives a message or reports an immediate timeout.
    pub(crate) fn receive_with_timeout(
        &mut self,
        pid: VmProcessId,
        timeout_ticks: u64,
    ) -> Result<VmActorReceive, String> {
        self.ensure_live_process(pid, "receive")?;
        self.charge_receive_reduction(pid);
        if let Some(message) = self.memory.receive_message(&mut self.processes, pid)? {
            self.scheduler
                .charge_memory_reductions(&mut self.processes, pid, message.accounted_bytes)
                .expect("timeout receiving actor remains live while charging mailbox reductions");
            return Ok(VmActorReceive::Message(message));
        }
        if timeout_ticks == 0 {
            Ok(VmActorReceive::Timeout)
        } else {
            self.processes
                .get_mut(pid)
                .expect("process was checked before timeout receive")
                .block();
            Ok(VmActorReceive::Blocked)
        }
    }

    fn charge_receive_reduction(&mut self, pid: VmProcessId) {
        self.charge_receive_reductions(pid, ACTOR_OPERATION_REDUCTIONS);
    }

    fn charge_receive_reductions(&mut self, pid: VmProcessId, reductions: u64) {
        self.charge_actor_reductions(pid, reductions);
    }

    fn charge_actor_reductions(&mut self, pid: VmProcessId, reductions: u64) {
        self.scheduler
            .charge_runtime_reductions(&mut self.processes, pid, reductions)
            .expect("actor was validated before scheduler accounting");
    }

    /// Runs the next scheduled actor slice.
    pub(crate) fn run_next(
        &mut self,
        run_slice: impl FnMut(&mut super::process::VmProcess, VmSchedulerSlice) -> VmSchedulerDecision,
    ) -> Result<VmSchedulerRun, String> {
        self.scheduler.run_next(&mut self.processes, run_slice)
    }

    fn ensure_live_process(&self, pid: VmProcessId, action: &str) -> Result<(), String> {
        let process = self
            .processes
            .get(pid)
            .ok_or_else(|| format!("cannot {action} missing process {}", pid.as_u64()))?;
        if matches!(process.state, super::process::VmProcessState::Exited(_)) {
            return Err(format!("cannot {action} exited process {}", pid.as_u64()));
        }
        Ok(())
    }
}

#[path = "actor_checkpoint.rs"]
mod actor_checkpoint;
#[path = "actor_code.rs"]
mod actor_code;
#[path = "actor_dynamic_module.rs"]
mod actor_dynamic_module;
#[path = "actor_exit.rs"]
mod actor_exit;
#[path = "actor_heap_limit.rs"]
mod actor_heap_limit;
#[path = "actor_local_trace.rs"]
mod actor_local_trace;
#[path = "actor_meta_trace.rs"]
mod actor_meta_trace;
#[path = "actor_native_trace.rs"]
mod actor_native_trace;
#[path = "actor_postgres.rs"]
mod actor_postgres;
#[path = "actor_registry.rs"]
mod actor_registry;
#[path = "actor_relationship.rs"]
mod actor_relationship;
#[path = "actor_spawn.rs"]
mod actor_spawn;
#[path = "actor_suspension.rs"]
mod actor_suspension;
#[path = "actor_timer.rs"]
mod actor_timer;
#[path = "actor_timer_options.rs"]
mod actor_timer_options;
pub(crate) type VmActorDemonitorOptions = actor_relationship::VmActorDemonitorOptions;
pub(crate) type VmActorDemonitorResult = actor_relationship::VmActorDemonitorResult;
pub(crate) use actor_native_trace::VmNativeTraceCall;
pub(crate) use actor_spawn::VmActorSpawnOptions;

fn actor_registry_error(error: VmProcessRegistryError) -> String {
    match error {
        VmProcessRegistryError::EmptyName => "actor name cannot be empty".to_string(),
        VmProcessRegistryError::NameNotRegistered(name) => {
            format!("actor name `{name}` is not registered")
        }
        VmProcessRegistryError::MissingProcess(pid) => {
            format!("cannot register missing process {}", pid.as_u64())
        }
        VmProcessRegistryError::ExitedProcess(pid) => {
            format!("cannot register exited process {}", pid.as_u64())
        }
        VmProcessRegistryError::Conflict { name, existing } => format!(
            "actor name `{name}` is already registered to process {}",
            existing.as_u64()
        ),
    }
}

fn actor_alias_error(error: VmProcessAliasError) -> String {
    match error {
        VmProcessAliasError::MissingProcess(pid) => {
            format!("cannot alias missing process {}", pid.as_u64())
        }
        VmProcessAliasError::ExitedProcess(pid) => {
            format!("cannot alias exited process {}", pid.as_u64())
        }
        VmProcessAliasError::MissingAlias(alias) => {
            format!("process alias {} is not registered", alias.as_u64())
        }
        VmProcessAliasError::PriorityNotEnabled(alias) => {
            format!("process alias {} is not priority-enabled", alias.as_u64())
        }
        VmProcessAliasError::AliasSpaceExhausted => {
            "process alias identity space is exhausted".to_string()
        }
    }
}
