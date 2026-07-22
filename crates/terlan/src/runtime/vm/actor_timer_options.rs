use super::{VmActorRuntime, VmProcessId};
use crate::runtime::vm::{
    timer::{VmTimerEvent, VmTimerId},
    ReplValue,
};

/// Deadline interpretation for a delayed actor message.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum VmActorTimerDeadline {
    Relative(u64),
    Absolute(u64),
}

/// Reply policy for reading one delayed actor timer.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum VmActorTimerReadMode {
    Synchronous,
    Asynchronous,
}

/// Reply policy for cancelling one delayed actor timer.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum VmActorTimerCancelMode {
    Synchronous { include_information: bool },
    Asynchronous { include_information: bool },
}

/// Information returned for an active or stale delayed actor timer.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum VmActorTimerInformation {
    Remaining(u64),
    Missing,
}

/// Immediate result of one timer option operation.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum VmActorTimerOptionResult {
    Information(VmActorTimerInformation),
    Acknowledged,
}

/// Observable effects from one typed timer option operation.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct VmActorTimerOptionOutcome {
    pub(crate) result: VmActorTimerOptionResult,
    pub(crate) timer_event: Option<VmTimerEvent>,
    pub(crate) reply_message_id: Option<u64>,
}

impl VmActorTimerDeadline {
    fn resolve(self, now_tick: u64, operation: &str) -> Result<u64, String> {
        match self {
            Self::Relative(delay_ticks) => now_tick.checked_add(delay_ticks).ok_or_else(|| {
                format!("{operation} deadline overflow at tick {now_tick} with delay {delay_ticks}")
            }),
            Self::Absolute(deadline_tick) => Ok(deadline_tick),
        }
    }
}

impl VmActorRuntime {
    /// Schedules a delayed message using an explicit relative or absolute deadline.
    pub(crate) fn send_with_deadline(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
        now_tick: u64,
        deadline: VmActorTimerDeadline,
    ) -> Result<VmTimerId, String> {
        self.processes.validate_send(sender, recipient)?;
        let deadline_tick = deadline.resolve(now_tick, "delayed actor send")?;
        self.schedule_delayed_message_at(sender, recipient, payload, deadline_tick)
    }

    /// Starts a correlated message timer with a relative or absolute deadline.
    pub(crate) fn start_message_timer_with_deadline(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
        now_tick: u64,
        deadline: VmActorTimerDeadline,
    ) -> Result<VmTimerId, String> {
        self.processes.validate_send(sender, recipient)?;
        let deadline_tick = deadline.resolve(now_tick, "actor message timer")?;
        self.schedule_message_timer_at(sender, recipient, payload, deadline_tick)
    }

    /// Reads one delayed timer synchronously or queues a typed asynchronous reply.
    pub(crate) fn read_delayed_send_with_mode(
        &mut self,
        requester: VmProcessId,
        timer_id: VmTimerId,
        now_tick: u64,
        mode: VmActorTimerReadMode,
    ) -> Result<VmActorTimerOptionOutcome, String> {
        self.processes.validate_sender(requester)?;
        let information = self.delayed_timer_information(timer_id, now_tick);
        match mode {
            VmActorTimerReadMode::Synchronous => Ok(VmActorTimerOptionOutcome {
                result: VmActorTimerOptionResult::Information(information),
                timer_event: None,
                reply_message_id: None,
            }),
            VmActorTimerReadMode::Asynchronous => {
                let reply_message_id = self.send(
                    requester,
                    requester,
                    timer_option_reply("TimerReadReply", timer_id, &information),
                )?;
                Ok(VmActorTimerOptionOutcome {
                    result: VmActorTimerOptionResult::Acknowledged,
                    timer_event: None,
                    reply_message_id: Some(reply_message_id),
                })
            }
        }
    }

    /// Cancels one delayed timer with explicit synchronous/asynchronous information policy.
    pub(crate) fn cancel_delayed_send_with_mode(
        &mut self,
        requester: VmProcessId,
        timer_id: VmTimerId,
        now_tick: u64,
        mode: VmActorTimerCancelMode,
    ) -> Result<VmActorTimerOptionOutcome, String> {
        self.processes.validate_sender(requester)?;
        let information = self.delayed_timer_information(timer_id, now_tick);
        let include_information = match &mode {
            VmActorTimerCancelMode::Synchronous {
                include_information,
            }
            | VmActorTimerCancelMode::Asynchronous {
                include_information,
            } => *include_information,
        };
        let reply_message_id = if matches!(&mode, VmActorTimerCancelMode::Asynchronous { .. })
            && include_information
        {
            Some(self.send(
                requester,
                requester,
                timer_option_reply("TimerCancelReply", timer_id, &information),
            )?)
        } else {
            None
        };
        let timer_event = match &information {
            VmActorTimerInformation::Remaining(_) => {
                Some(self.cancel_delayed_send(timer_id, now_tick)?.timer_event)
            }
            VmActorTimerInformation::Missing => None,
        };
        let result = match mode {
            VmActorTimerCancelMode::Synchronous {
                include_information: true,
            } => VmActorTimerOptionResult::Information(information),
            VmActorTimerCancelMode::Synchronous {
                include_information: false,
            }
            | VmActorTimerCancelMode::Asynchronous { .. } => VmActorTimerOptionResult::Acknowledged,
        };
        Ok(VmActorTimerOptionOutcome {
            result,
            timer_event,
            reply_message_id,
        })
    }

    fn delayed_timer_information(
        &self,
        timer_id: VmTimerId,
        now_tick: u64,
    ) -> VmActorTimerInformation {
        match self.read_delayed_send(timer_id, now_tick) {
            Ok(remaining_ticks) => VmActorTimerInformation::Remaining(remaining_ticks),
            Err(_) => VmActorTimerInformation::Missing,
        }
    }
}

fn timer_option_reply(
    name: &str,
    timer_id: VmTimerId,
    information: &VmActorTimerInformation,
) -> ReplValue {
    let result = match information {
        VmActorTimerInformation::Remaining(remaining_ticks) => ReplValue::Record {
            name: "TimerRemaining".to_string(),
            fields: vec![(
                "ticks".to_string(),
                ReplValue::String(remaining_ticks.to_string()),
            )],
        },
        VmActorTimerInformation::Missing => ReplValue::Atom("missing".to_string()),
    };
    ReplValue::Record {
        name: name.to_string(),
        fields: vec![
            (
                "timer_id".to_string(),
                ReplValue::String(timer_id.as_u64().to_string()),
            ),
            ("result".to_string(), result),
        ],
    }
}
