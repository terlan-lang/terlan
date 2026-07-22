use std::collections::BTreeMap;

use super::process::{VmProcessLocation, VmProcessSnapshot};
use super::scheduler::{VmSchedulerClass, VmSchedulerMetrics};

#[cfg(test)]
#[path = "system_profile_test.rs"]
mod system_profile_test;

/// Immutable position in the scheduler transition stream.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmSystemProfileCursor {
    transition_index: usize,
}

impl VmSystemProfileCursor {
    pub(super) fn at(transition_index: usize) -> Self {
        Self { transition_index }
    }
}

/// Portable actor scheduling activity retained for deterministic profiling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmSystemProfileActivity {
    Runnable,
    Inactive,
}

/// One replay-stable scheduler profile event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSystemProfileEvent {
    pub(crate) sequence: usize,
    pub(crate) tick: u64,
    pub(crate) pid: u64,
    pub(crate) activity: VmSystemProfileActivity,
    pub(crate) transition: &'static str,
    pub(crate) scheduler_class: VmSchedulerClass,
    pub(crate) run_queue_length: usize,
    pub(crate) location: VmProcessLocation,
}

/// Immutable system profile events and scheduler totals captured at one point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSystemProfileSnapshot {
    pub(crate) events: Vec<VmSystemProfileEvent>,
    pub(crate) next_cursor: VmSystemProfileCursor,
    pub(crate) total_reductions: u64,
    pub(crate) total_slices: u64,
    pub(crate) total_preemptions: u64,
}

impl VmSystemProfileSnapshot {
    pub(super) fn capture(
        scheduler: &VmSchedulerMetrics,
        processes: &[VmProcessSnapshot],
        cursor: VmSystemProfileCursor,
    ) -> Result<Self, String> {
        let transition_count = scheduler.queue_transitions.len();
        if cursor.transition_index > transition_count {
            return Err(format!(
                "VM system profile cursor {} exceeds transition count {transition_count}",
                cursor.transition_index
            ));
        }
        let process_by_id = processes
            .iter()
            .map(|process| (process.pid.as_u64(), process))
            .collect::<BTreeMap<_, _>>();
        let events = scheduler.queue_transitions[cursor.transition_index..]
            .iter()
            .enumerate()
            .map(|(offset, transition)| {
                let process = process_by_id.get(&transition.pid).ok_or_else(|| {
                    format!(
                        "VM system profile transition references missing process {}",
                        transition.pid
                    )
                })?;
                Ok(VmSystemProfileEvent {
                    sequence: cursor.transition_index + offset,
                    tick: transition.tick,
                    pid: transition.pid,
                    activity: if transition.action == "enqueue" {
                        VmSystemProfileActivity::Runnable
                    } else {
                        VmSystemProfileActivity::Inactive
                    },
                    transition: transition.action,
                    scheduler_class: transition.class,
                    run_queue_length: transition.queue_len,
                    location: process.current_location.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            events,
            next_cursor: VmSystemProfileCursor::at(transition_count),
            total_reductions: scheduler.total_reductions,
            total_slices: scheduler.total_slices,
            total_preemptions: scheduler.preemptions,
        })
    }
}
