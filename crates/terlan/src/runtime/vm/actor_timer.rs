#[cfg(test)]
use super::ACTOR_OPERATION_REDUCTIONS;
use super::{VmActorRuntime, VmProcessId};
#[cfg(test)]
use crate::runtime::vm::postgres::VmPostgresDriverControl;
#[cfg(test)]
use crate::runtime::vm::timer::VmTimerId;
use crate::runtime::vm::{timer::VmTimerEvent, ReplValue};

/// One delayed actor message after its destination has been resolved.
#[derive(Debug)]
pub(super) struct VmDelayedActorMessage {
    sender: VmProcessId,
    recipient: VmProcessId,
    payload: ReplValue,
}

/// Delivery result for one expired delayed actor message.
#[derive(Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmActorTimerDelivery {
    Delivered {
        timer_id: VmTimerId,
        message_id: u64,
    },
    Rejected {
        timer_id: VmTimerId,
        diagnostic: String,
    },
    OwnerExited {
        timer_id: VmTimerId,
    },
}

/// Timer events and delayed-message delivery outcomes from one clock advance.
#[derive(Debug)]
pub(crate) struct VmActorTimerAdvance {
    pub(crate) timer_events: Vec<VmTimerEvent>,
    #[cfg(test)]
    pub(crate) deliveries: Vec<VmActorTimerDelivery>,
    #[cfg(test)]
    pub(crate) postgres_controls: Vec<VmPostgresDriverControl>,
    #[cfg(test)]
    pub(crate) postgres_diagnostics: Vec<String>,
}

/// Successful cancellation evidence for one delayed actor message.
#[derive(Debug)]
#[cfg(test)]
pub(crate) struct VmActorTimerCancellation {
    pub(crate) remaining_ticks: u64,
    pub(crate) timer_event: VmTimerEvent,
}

impl VmActorRuntime {
    /// Schedules one typed actor payload for delivery after `delay_ticks`.
    #[cfg(test)]
    pub(crate) fn send_after(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
        now_tick: u64,
        delay_ticks: u64,
    ) -> Result<VmTimerId, String> {
        self.send_with_deadline(
            sender,
            recipient,
            payload,
            now_tick,
            super::actor_timer_options::VmActorTimerDeadline::Relative(delay_ticks),
        )
    }

    #[cfg(test)]
    pub(super) fn schedule_delayed_message_at(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
        deadline_tick: u64,
    ) -> Result<VmTimerId, String> {
        let timer_id = self
            .timers
            .start_one_shot(&self.processes, sender, deadline_tick)?;
        self.delayed_messages.insert(
            timer_id,
            VmDelayedActorMessage {
                sender,
                recipient,
                payload,
            },
        );
        self.charge_actor_reductions(sender, ACTOR_OPERATION_REDUCTIONS);
        Ok(timer_id)
    }

    /// Resolves a stable actor name and schedules a delayed send to that process.
    #[cfg(test)]
    pub(crate) fn send_named_after(
        &mut self,
        sender: VmProcessId,
        recipient_name: &str,
        payload: ReplValue,
        now_tick: u64,
        delay_ticks: u64,
    ) -> Result<VmTimerId, String> {
        self.processes.validate_sender(sender)?;
        let recipient = self
            .processes
            .lookup_name(recipient_name)
            .ok_or_else(|| format!("actor name `{recipient_name}` is not registered"))?;
        self.send_after(sender, recipient, payload, now_tick, delay_ticks)
    }

    /// Resolves an opaque actor alias and schedules a delayed send to that process.
    #[cfg(test)]
    pub(crate) fn send_alias_after(
        &mut self,
        sender: VmProcessId,
        recipient_alias: super::VmProcessAlias,
        payload: ReplValue,
        now_tick: u64,
        delay_ticks: u64,
    ) -> Result<VmTimerId, String> {
        self.processes.validate_sender(sender)?;
        let recipient = self.aliases.resolve(recipient_alias).ok_or_else(|| {
            super::actor_alias_error(super::VmProcessAliasError::MissingAlias(recipient_alias))
        })?;
        self.send_after(sender, recipient, payload, now_tick, delay_ticks)
    }

    /// Starts a correlated message timer for one resolved actor process.
    #[cfg(test)]
    pub(crate) fn start_message_timer(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
        now_tick: u64,
        delay_ticks: u64,
    ) -> Result<VmTimerId, String> {
        self.start_message_timer_with_deadline(
            sender,
            recipient,
            payload,
            now_tick,
            super::actor_timer_options::VmActorTimerDeadline::Relative(delay_ticks),
        )
    }

    #[cfg(test)]
    pub(super) fn schedule_message_timer_at(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
        deadline_tick: u64,
    ) -> Result<VmTimerId, String> {
        let timer_id = self
            .timers
            .start_one_shot(&self.processes, sender, deadline_tick)?;
        self.delayed_messages.insert(
            timer_id,
            VmDelayedActorMessage {
                sender,
                recipient,
                payload: timer_message(timer_id, payload),
            },
        );
        self.charge_actor_reductions(sender, ACTOR_OPERATION_REDUCTIONS);
        Ok(timer_id)
    }

    /// Resolves a stable actor name and starts a correlated message timer.
    #[cfg(test)]
    pub(crate) fn start_named_message_timer(
        &mut self,
        sender: VmProcessId,
        recipient_name: &str,
        payload: ReplValue,
        now_tick: u64,
        delay_ticks: u64,
    ) -> Result<VmTimerId, String> {
        self.processes.validate_sender(sender)?;
        let recipient = self
            .processes
            .lookup_name(recipient_name)
            .ok_or_else(|| format!("actor name `{recipient_name}` is not registered"))?;
        self.start_message_timer(sender, recipient, payload, now_tick, delay_ticks)
    }

    /// Returns the remaining ticks for one active delayed send.
    #[cfg(test)]
    pub(crate) fn read_delayed_send(
        &self,
        timer_id: VmTimerId,
        now_tick: u64,
    ) -> Result<u64, String> {
        self.timers.remaining_ticks(timer_id, now_tick)
    }

    /// Cancels one delayed send and returns its remaining time atomically.
    #[cfg(test)]
    pub(crate) fn cancel_delayed_send(
        &mut self,
        timer_id: VmTimerId,
        now_tick: u64,
    ) -> Result<VmActorTimerCancellation, String> {
        let remaining_ticks = self.timers.remaining_ticks(timer_id, now_tick)?;
        let timer_event = self.timers.cancel(timer_id)?;
        self.delayed_messages
            .remove(&timer_id)
            .expect("actor timer must retain its delayed payload until cancellation");
        self.charge_actor_reductions(timer_event.owner(), ACTOR_OPERATION_REDUCTIONS);
        Ok(VmActorTimerCancellation {
            remaining_ticks,
            timer_event,
        })
    }

    /// Advances actor-owned timers and delivers every expired payload once.
    pub(crate) fn advance_actor_timers(&mut self, now_tick: u64) -> VmActorTimerAdvance {
        let timer_events =
            self.timers
                .advance_clock(&mut self.processes, &mut self.scheduler, now_tick);
        #[cfg(test)]
        let mut deliveries = Vec::new();
        #[cfg(test)]
        let mut postgres_controls = Vec::new();
        #[cfg(test)]
        let mut postgres_diagnostics = Vec::new();
        for event in &timer_events {
            let timer_id = event.timer_id();
            let Some(delayed) = self.delayed_messages.remove(&timer_id) else {
                match self.consume_postgres_timer_event(event) {
                    Ok(Some(control)) => {
                        self.postgres_controls.push_back(control);
                        #[cfg(test)]
                        postgres_controls.push(control);
                    }
                    Ok(None) => {}
                    Err(diagnostic) => {
                        #[cfg(test)]
                        postgres_diagnostics.push(diagnostic);
                        #[cfg(not(test))]
                        drop(diagnostic);
                    }
                }
                continue;
            };
            if matches!(event, VmTimerEvent::OwnerExited { .. }) {
                #[cfg(test)]
                deliveries.push(VmActorTimerDelivery::OwnerExited { timer_id });
                continue;
            }
            match self.send(delayed.sender, delayed.recipient, delayed.payload) {
                Ok(message_id) => {
                    #[cfg(test)]
                    deliveries.push(VmActorTimerDelivery::Delivered {
                        timer_id,
                        message_id,
                    });
                    #[cfg(not(test))]
                    let _ = message_id;
                }
                Err(diagnostic) => {
                    #[cfg(test)]
                    deliveries.push(VmActorTimerDelivery::Rejected {
                        timer_id,
                        diagnostic,
                    });
                    #[cfg(not(test))]
                    drop(diagnostic);
                }
            }
        }
        VmActorTimerAdvance {
            timer_events,
            #[cfg(test)]
            deliveries,
            #[cfg(test)]
            postgres_controls,
            #[cfg(test)]
            postgres_diagnostics,
        }
    }

    /// Returns the number of active delayed actor messages.
    #[cfg(test)]
    pub(crate) fn delayed_send_count(&self) -> usize {
        self.delayed_messages.len()
    }

    pub(super) fn remove_delayed_messages_for_owner(
        &mut self,
        owner: VmProcessId,
    ) -> Vec<VmTimerEvent> {
        let events = self.timers.cancel_owner_timers(owner);
        for event in &events {
            self.delayed_messages.remove(&event.timer_id());
        }
        events
    }
}

#[cfg(test)]
fn timer_message(timer_id: VmTimerId, payload: ReplValue) -> ReplValue {
    ReplValue::Record {
        name: "TimerMessage".to_string(),
        fields: vec![
            (
                "timer_id".to_string(),
                ReplValue::String(timer_id.as_u64().to_string()),
            ),
            ("payload".to_string(), payload),
        ],
    }
}
