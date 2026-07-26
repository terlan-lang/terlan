use super::super::resource::{VmResourceDescriptor, VmResourceEvent, VmResourceTransferPolicy};
use super::{ReplValue, VmActorRuntime, ACTOR_OPERATION_REDUCTIONS};
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::process::{
    VmExitReason, VmMessage, VmProcessId, VmProcessSource, VmProcessState,
};
use crate::runtime::vm::scheduler::VmSchedulerClass;
use crate::runtime::vm::timer::{VmTimerEvent, VmTimerId, VmTimerKind};

/// Exact VM timer lease held while one native continuation remains parked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmNativeTimerWait {
    pub(crate) timer_id: VmTimerId,
    pub(crate) deadline_tick: u64,
}

/// Observable result of one VM-owned actor hibernation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmActorHibernateOutcome {
    pub(crate) released_heap_bytes: usize,
    pub(crate) retained_mailbox_bytes: usize,
    pub(crate) awakened_immediately: bool,
}

impl VmActorRuntime {
    /// Compacts and parks an actor until a queued or future VM wake event.
    pub(crate) fn hibernate(
        &mut self,
        pid: VmProcessId,
    ) -> Result<VmActorHibernateOutcome, String> {
        let process = self
            .processes
            .get(pid)
            .ok_or_else(|| format!("cannot hibernate missing process {}", pid.as_u64()))?;
        if matches!(process.state, VmProcessState::Exited(_)) {
            return Err(format!("cannot hibernate exited process {}", pid.as_u64()));
        }
        let retained_mailbox_bytes = process.mailbox_accounted_bytes()?;
        let released_heap_bytes = process
            .heap_bytes
            .checked_sub(retained_mailbox_bytes)
            .ok_or_else(|| {
                format!(
                    "error[vm.hibernate_accounting]: process {} retains {} mailbox bytes but only {} heap bytes",
                    pid.as_u64(),
                    retained_mailbox_bytes,
                    process.heap_bytes
                )
            })?;
        let awakened_immediately = process.mailbox_len() != 0;

        self.scheduler.hibernate_process(&mut self.processes, pid)?;
        self.memory
            .release_heap(&mut self.processes, pid, released_heap_bytes)?;
        if awakened_immediately {
            self.scheduler.wake_process(&mut self.processes, pid)?;
        }
        self.charge_actor_reductions(pid, ACTOR_OPERATION_REDUCTIONS);
        Ok(VmActorHibernateOutcome {
            released_heap_bytes,
            retained_mailbox_bytes,
            awakened_immediately,
        })
    }

    /// Suspends an actor without discarding queued mailbox work.
    pub(crate) fn suspend(&mut self, pid: VmProcessId) -> Result<(), String> {
        let native_pending = self.native_continuations_by_owner.contains_key(&pid);
        self.scheduler.suspend_process(&mut self.processes, pid)?;
        if native_pending {
            self.explicit_native_suspensions.insert(pid);
        }
        self.charge_actor_reductions(pid, ACTOR_OPERATION_REDUCTIONS);
        Ok(())
    }

    /// Resumes an actor through the scheduler-owned runnable queue.
    pub(crate) fn resume(&mut self, pid: VmProcessId) -> Result<(), String> {
        let explicit_native_suspension = self.explicit_native_suspensions.remove(&pid);
        if let Some((request_id, continuation_id)) =
            self.native_continuations_by_owner.get(&pid).copied()
        {
            if !explicit_native_suspension {
                return Err(format!(
                    "cannot resume process {} while native continuation {request_id}/{continuation_id} is pending",
                    pid.as_u64()
                ));
            }
            self.charge_actor_reductions(pid, ACTOR_OPERATION_REDUCTIONS);
            return Ok(());
        }
        if let Err(error) = self.scheduler.resume_process(&mut self.processes, pid) {
            if explicit_native_suspension {
                self.explicit_native_suspensions.insert(pid);
            }
            return Err(error);
        }
        self.charge_actor_reductions(pid, ACTOR_OPERATION_REDUCTIONS);
        Ok(())
    }

    /// Parks one actor behind an exact native request and continuation pair.
    pub(crate) fn park_native_continuation(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
    ) -> Result<(), String> {
        let owner = VmProcessId::from_native_owner(owner_id)?;
        validate_native_continuation_identity(request_id, continuation_id)?;
        if let Some((pending_request, pending_continuation)) =
            self.native_continuations_by_owner.get(&owner)
        {
            return Err(format!(
                "process {} already owns native continuation {pending_request}/{pending_continuation}",
                owner.as_u64()
            ));
        }
        if let Some(existing_owner) = self
            .native_continuations
            .get(&(request_id, continuation_id))
        {
            return Err(format!(
                "native continuation {request_id}/{continuation_id} is already owned by process {}",
                existing_owner.as_u64()
            ));
        }
        let process = self.processes.get(owner).ok_or_else(|| {
            format!(
                "cannot park native continuation for missing process {}",
                owner.as_u64()
            )
        })?;
        if process.state != VmProcessState::Runnable {
            return Err(format!(
                "cannot park native continuation for non-runnable process {}",
                owner.as_u64()
            ));
        }

        self.scheduler.suspend_process(&mut self.processes, owner)?;
        self.charge_actor_reductions(owner, ACTOR_OPERATION_REDUCTIONS);
        self.native_continuations
            .insert((request_id, continuation_id), owner);
        self.native_continuations_by_owner
            .insert(owner, (request_id, continuation_id));
        Ok(())
    }

    /// Resumes only the actor that owns the exact native continuation identity.
    pub(crate) fn resume_native_continuation(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
    ) -> Result<(), String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        let identity = (request_id, continuation_id);
        self.consume_native_continuation(owner, identity)
    }

    /// Delivers one native `Send` transition and resumes its exact owner.
    ///
    /// The continuation lease is validated before mailbox mutation and is
    /// retained when delivery fails, so a stale, foreign, or invalid send can
    /// neither enqueue a message nor wake native code.
    pub(crate) fn service_native_send(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        recipient_id: u64,
        payload: ReplValue,
    ) -> Result<u64, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        let recipient = VmProcessId::from_native_recipient(recipient_id)?;
        let message_id = self.send(owner, recipient, payload)?;
        self.consume_native_continuation(owner, (request_id, continuation_id))?;
        Ok(message_id)
    }

    /// Delivers one exactly typed native value and resumes its continuation.
    pub(crate) fn service_native_send_typed(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        recipient_id: u64,
        payload: ReplValue,
        boundary_type: TvmBoundaryType,
    ) -> Result<u64, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        let recipient = VmProcessId::from_native_recipient(recipient_id)?;
        let message_id = self.send_typed(owner, recipient, payload, boundary_type)?;
        self.consume_native_continuation(owner, (request_id, continuation_id))?;
        Ok(message_id)
    }

    /// Delivers one receiver-owned managed graph and resumes its continuation.
    pub(crate) fn service_native_send_managed(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        recipient_id: u64,
        fragment: crate::runtime::vm::process::VmManagedMailboxToken,
        boundary_type: TvmBoundaryType,
    ) -> Result<u64, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        let recipient = VmProcessId::from_native_recipient(recipient_id)?;
        let message_id = self.send_typed_managed(owner, recipient, fragment, boundary_type)?;
        self.consume_native_continuation(owner, (request_id, continuation_id))?;
        Ok(message_id)
    }

    /// Delivers one native message through the VM-owned actor registry.
    ///
    /// Exact continuation authority is validated before name lookup. A missing
    /// or stale registration leaves the mailbox, scheduler, and continuation
    /// lease unchanged so native adapters cannot manufacture delivery races.
    pub(crate) fn service_native_named_send(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        recipient_name: &str,
        payload: ReplValue,
    ) -> Result<u64, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        let message_id = self.send_named(owner, recipient_name, payload)?;
        self.consume_native_continuation(owner, (request_id, continuation_id))?;
        Ok(message_id)
    }

    /// Receives one queued Int for an exact native continuation owner.
    ///
    /// Non-Int messages retain their FIFO positions. An empty matching mailbox
    /// leaves the continuation parked so delivery can be retried without
    /// manufacturing a resume value.
    pub(crate) fn service_native_receive_int(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
    ) -> Result<Option<i64>, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        let outcome = self.memory.selective_receive_message_with_scan(
            &mut self.processes,
            owner,
            |message| matches!(message.payload, ReplValue::Int(_)),
        )?;
        let reductions = u64::try_from(outcome.inspected_messages)
            .unwrap_or(u64::MAX)
            .max(ACTOR_OPERATION_REDUCTIONS);
        self.charge_receive_reductions(owner, reductions);
        let Some(message) = outcome.message else {
            return Ok(None);
        };
        self.scheduler
            .charge_memory_reductions(&mut self.processes, owner, message.accounted_bytes)
            .expect("native receive owner remains live while charging mailbox reductions");
        let ReplValue::Int(payload) = message.payload else {
            unreachable!("native Int receive selected only an Int payload")
        };
        self.consume_native_continuation(owner, (request_id, continuation_id))?;
        Ok(Some(payload))
    }

    /// Receives and receiver-encodes one exact typed native mailbox value.
    ///
    /// Conversion occurs while the message remains queued. A failed allocation
    /// or type conversion therefore leaves mailbox order, accounting, and the
    /// continuation lease unchanged.
    pub(crate) fn service_native_receive_typed(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        boundary_type: &TvmBoundaryType,
        mut encode: impl FnMut(&ReplValue) -> Result<i64, String>,
    ) -> Result<Option<i64>, String> {
        self.service_native_receive_typed_message(
            owner_id,
            request_id,
            continuation_id,
            boundary_type,
            |message| encode(&message.payload),
        )
    }

    /// Receives and encodes one exact typed message including native graph metadata.
    pub(crate) fn service_native_receive_typed_message(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        boundary_type: &TvmBoundaryType,
        mut encode: impl FnMut(&VmMessage) -> Result<i64, String>,
    ) -> Result<Option<i64>, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        let mut encoded = None;
        let mut conversion_error = None;
        let outcome = self.memory.selective_receive_message_with_scan(
            &mut self.processes,
            owner,
            |message| {
                if conversion_error.is_some()
                    || message.boundary_type.as_ref() != Some(boundary_type)
                {
                    return false;
                }
                match encode(message) {
                    Ok(value) => {
                        encoded = Some(value);
                        true
                    }
                    Err(error) => {
                        conversion_error = Some(error);
                        false
                    }
                }
            },
        )?;
        let reductions = u64::try_from(outcome.inspected_messages)
            .unwrap_or(u64::MAX)
            .max(ACTOR_OPERATION_REDUCTIONS);
        self.charge_receive_reductions(owner, reductions);
        if let Some(error) = conversion_error {
            return Err(error);
        }
        let Some(message) = outcome.message else {
            return Ok(None);
        };
        self.scheduler
            .charge_memory_reductions(&mut self.processes, owner, message.accounted_bytes)
            .expect("typed native receive owner remains live while charging mailbox reductions");
        let encoded = encoded.expect("typed receive removes only an encoded message");
        self.consume_native_continuation(owner, (request_id, continuation_id))?;
        Ok(Some(encoded))
    }

    /// Spawns one scheduled native child and resumes its exact parent owner.
    pub(crate) fn service_native_spawn(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        entry_id: u64,
    ) -> Result<u64, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        if entry_id == 0 {
            return Err("native spawn entry identity must be nonzero".to_string());
        }
        let child = self.spawn_child(
            owner,
            VmProcessSource::new("native.Image", format!("entry_{entry_id}"), 0),
        )?;
        self.consume_native_continuation(owner, (request_id, continuation_id))?;
        Ok(child.as_u64())
    }

    /// Starts a VM-owned one-shot timer without consuming the native lease.
    pub(crate) fn begin_native_timer(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        delay_ticks: u64,
    ) -> Result<VmNativeTimerWait, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        if delay_ticks == 0 {
            return Err("native timer delay must be positive".to_string());
        }
        let deadline_tick = self
            .timers
            .current_tick()
            .checked_add(delay_ticks)
            .ok_or_else(|| format!("native timer deadline overflow for process {owner_id}"))?;
        let timer_id = self
            .timers
            .start_one_shot(&self.processes, owner, deadline_tick)?;
        Ok(VmNativeTimerWait {
            timer_id,
            deadline_tick,
        })
    }

    /// Starts a VM-owned one-shot timer against an absolute scheduler clock.
    pub(crate) fn begin_native_timer_at(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        observed_tick: u64,
        delay_ticks: u64,
    ) -> Result<VmNativeTimerWait, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        if delay_ticks == 0 {
            return Err("native timer delay must be positive".to_string());
        }
        if observed_tick < self.timers.current_tick() {
            return Err(format!(
                "native timer clock moved backwards from {} to {observed_tick}",
                self.timers.current_tick()
            ));
        }
        let deadline_tick = observed_tick
            .checked_add(delay_ticks)
            .ok_or_else(|| format!("native timer deadline overflow for process {owner_id}"))?;
        let timer_id = self
            .timers
            .start_one_shot(&self.processes, owner, deadline_tick)?;
        Ok(VmNativeTimerWait {
            timer_id,
            deadline_tick,
        })
    }

    /// Consumes one native continuation after its exact timer event was delivered.
    pub(crate) fn complete_delivered_native_timer(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        wait: VmNativeTimerWait,
        advance: &super::actor_timer::VmActorTimerAdvance,
    ) -> Result<(), String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        let delivered = advance.timer_events.iter().any(|event| {
            matches!(
                event,
                VmTimerEvent::Fired {
                    timer_id,
                    owner: event_owner,
                    kind: VmTimerKind::OneShot,
                } | VmTimerEvent::DeadlineMissed {
                    timer_id,
                    owner: event_owner,
                    kind: VmTimerKind::OneShot,
                    ..
                } if *timer_id == wait.timer_id && *event_owner == owner
            )
        });
        if !delivered {
            return Err(format!(
                "native timer {} did not produce its scheduler delivery for continuation {request_id}/{continuation_id}",
                wait.timer_id.as_u64()
            ));
        }
        self.consume_native_continuation(owner, (request_id, continuation_id))
    }

    /// Fires an exact native timer before consuming and resuming its lease.
    pub(crate) fn complete_native_timer(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        wait: VmNativeTimerWait,
    ) -> Result<(), String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        let snapshot = self
            .timers
            .snapshots()
            .into_iter()
            .find(|timer| timer.id == wait.timer_id)
            .ok_or_else(|| format!("missing native timer {}", wait.timer_id.as_u64()))?;
        if snapshot.owner != owner
            || snapshot.kind != VmTimerKind::OneShot
            || snapshot.deadline_tick != wait.deadline_tick
        {
            return Err(format!(
                "native timer {} does not match continuation {request_id}/{continuation_id}",
                wait.timer_id.as_u64()
            ));
        }
        let before_deadline = wait.deadline_tick - 1;
        if self.timers.current_tick() > before_deadline {
            return Err(format!(
                "native timer {} deadline already passed",
                wait.timer_id.as_u64()
            ));
        }
        let early = self.advance_actor_timers(before_deadline);
        if early
            .timer_events
            .iter()
            .any(|event| event.timer_id() == wait.timer_id)
        {
            return Err(format!(
                "native timer {} fired before its deadline",
                wait.timer_id.as_u64()
            ));
        }
        let due = self.advance_actor_timers(wait.deadline_tick);
        if !due.timer_events.iter().any(|event| {
            matches!(
                event,
                VmTimerEvent::Fired {
                    timer_id,
                    owner: event_owner,
                    kind: VmTimerKind::OneShot,
                } if *timer_id == wait.timer_id && *event_owner == owner
            )
        }) {
            return Err(format!(
                "native timer {} did not fire at its deadline",
                wait.timer_id.as_u64()
            ));
        }
        self.consume_native_continuation(owner, (request_id, continuation_id))
    }

    /// Waits on a VM-owned timer and resumes the exact native continuation.
    pub(crate) fn service_native_timer(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        delay_ticks: u64,
    ) -> Result<(), String> {
        let wait = self.begin_native_timer(owner_id, request_id, continuation_id, delay_ticks)?;
        self.complete_native_timer(owner_id, request_id, continuation_id, wait)
    }

    /// Creates one VM-owned symmetric link before resuming its native owner.
    pub(crate) fn service_native_link(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        peer_id: u64,
    ) -> Result<bool, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        let peer = VmProcessId::from_native_recipient(peer_id)?;
        let created = self.link_actors(owner, peer)?;
        self.consume_native_continuation(owner, (request_id, continuation_id))?;
        Ok(created)
    }

    /// Creates one owner-bound monitor before resuming its native watcher.
    pub(crate) fn service_native_monitor(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        target_id: u64,
    ) -> Result<u64, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        let target = VmProcessId::from_native_recipient(target_id)?;
        let monitor_ref = self.monitor_actor(owner, target)?;
        self.consume_native_continuation(owner, (request_id, continuation_id))?;
        Ok(monitor_ref.as_u64())
    }

    /// Registers one opaque VM resource before resuming its native owner.
    pub(crate) fn service_native_resource(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        kind_tag: u64,
    ) -> Result<u64, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        if kind_tag == 0 {
            return Err("native resource kind tag must be positive".to_string());
        }
        let event = self.resources.register(
            &mut self.processes,
            owner,
            VmResourceDescriptor::new("native.scalar", format!("tag_{kind_tag}")),
            VmResourceTransferPolicy::OwnerOnly,
        )?;
        let VmResourceEvent::Registered { id, .. } = event else {
            unreachable!("native resource registration returns a registered event")
        };
        self.consume_native_continuation(owner, (request_id, continuation_id))?;
        Ok(id.as_u64())
    }

    /// Records one scheduler-owned cancellation before resuming its native requester.
    ///
    /// Exact continuation authority is validated before the target is mutated.
    /// The scheduler remains responsible for applying the killed exit and all
    /// associated failure/resource cleanup at the target's next boundary.
    pub(crate) fn service_native_cancellation(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        target_id: u64,
    ) -> Result<(), String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        let target = VmProcessId::from_native_recipient(target_id)?;
        self.scheduler
            .request_cancellation(&mut self.processes, target)?;
        self.consume_native_continuation(owner, (request_id, continuation_id))
    }

    /// Applies cancellation of a native owner before a resume frame can run it.
    pub(crate) fn enforce_native_cancellation_boundary(
        &mut self,
        owner_id: u64,
    ) -> Result<bool, String> {
        let owner = VmProcessId::from_native_owner(owner_id)?;
        let process = self
            .processes
            .get(owner)
            .ok_or_else(|| format!("missing native process {owner_id}"))?;
        if !process.cancellation_requested {
            return Ok(false);
        }
        if !matches!(process.state, VmProcessState::Exited(_)) {
            self.exit_actor(owner, VmExitReason::Killed)?;
        }
        Ok(true)
    }

    /// Terminates an exact native continuation owner through the VM failure layer.
    ///
    /// The continuation lease and positive scalar code are validated before
    /// abnormal-exit propagation mutates the owner, links, monitors, or resources.
    pub(crate) fn service_native_failure(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        failure_code: u64,
    ) -> Result<Vec<String>, String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        if failure_code == 0 {
            return Err("native failure code must be positive".to_string());
        }
        self.exit_actor(
            owner,
            VmExitReason::Error(format!("native_failure:{failure_code}")),
        )
    }

    /// Reclassifies an exact native owner before resuming it through the scheduler.
    pub(crate) fn service_native_scheduling(
        &mut self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
        class: VmSchedulerClass,
    ) -> Result<(), String> {
        let owner =
            self.validate_native_continuation_owner(owner_id, request_id, continuation_id)?;
        self.scheduler
            .set_process_class(&mut self.processes, owner, class)?;
        self.consume_native_continuation(owner, (request_id, continuation_id))
    }

    /// Attaches a resolved native export identity to a spawned child record.
    pub(crate) fn attach_native_spawn_entry(
        &mut self,
        child_id: u64,
        source: VmProcessSource,
    ) -> Result<(), String> {
        let child = VmProcessId::from_native_recipient(child_id)?;
        if self.processes.get(child).is_none() {
            return Err(format!("missing spawned native process {child_id}"));
        }
        self.processes
            .with_process_control_mutator(child, |process| {
                if process.state != VmProcessState::Runnable {
                    return Err(format!("spawned native process {child_id} is not runnable"));
                }
                process.source = source.clone();
                process.set_current_location(source, 0);
                Ok(())
            })?
    }

    pub(super) fn remove_native_continuation_for_owner(&mut self, owner: VmProcessId) {
        if let Some(identity) = self.native_continuations_by_owner.remove(&owner) {
            self.native_continuations.remove(&identity);
        }
        self.explicit_native_suspensions.remove(&owner);
    }

    pub(crate) fn pending_native_continuation_count(&self) -> usize {
        self.native_continuations.len()
    }

    fn validate_native_continuation_owner(
        &self,
        owner_id: u64,
        request_id: u64,
        continuation_id: u64,
    ) -> Result<VmProcessId, String> {
        let owner = VmProcessId::from_native_owner(owner_id)?;
        validate_native_continuation_identity(request_id, continuation_id)?;
        let identity = (request_id, continuation_id);
        let actual_owner = self
            .native_continuations
            .get(&identity)
            .ok_or_else(|| format!("stale native continuation {request_id}/{continuation_id}"))?;
        if *actual_owner != owner {
            return Err(format!(
                "native continuation {request_id}/{continuation_id} is owned by process {}, not process {}",
                actual_owner.as_u64(),
                owner.as_u64()
            ));
        }
        if self.native_continuations_by_owner.get(&owner) != Some(&identity) {
            return Err(format!(
                "native continuation ownership index mismatch for process {}",
                owner.as_u64()
            ));
        }
        Ok(owner)
    }

    fn consume_native_continuation(
        &mut self,
        owner: VmProcessId,
        identity: (u64, u64),
    ) -> Result<(), String> {
        if self.explicit_native_suspensions.contains(&owner) {
            self.charge_actor_reductions(owner, ACTOR_OPERATION_REDUCTIONS);
        } else {
            self.scheduler.resume_process(&mut self.processes, owner)?;
            self.charge_actor_reductions(owner, ACTOR_OPERATION_REDUCTIONS);
        }
        self.native_continuations.remove(&identity);
        self.native_continuations_by_owner.remove(&owner);
        Ok(())
    }
}

fn validate_native_continuation_identity(
    request_id: u64,
    continuation_id: u64,
) -> Result<(), String> {
    if request_id == 0 {
        return Err("native continuation request identity must be nonzero".to_string());
    }
    if continuation_id == 0 {
        return Err("native continuation identity must be nonzero".to_string());
    }
    Ok(())
}
