//! Bounded owner-thread execution of scheduler work-stealing decisions.

#![cfg_attr(not(test), allow(dead_code))]

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use super::{VmSchedulerWorkSnapshot, VmWorkDirective, VmWorkStealingConfig, VmWorkStealingPolicy};
use crate::runtime::vm::process::VmProcessTable;
use crate::runtime::vm::scheduler::{
    VmScheduler, VmSchedulerClass, VmSchedulerStealBatch, VmSchedulerStealImportFailure,
};
use crate::runtime::vm::scheduler_topology::VmSchedulerId;

/// Maximum outstanding control commands retained by one scheduler owner.
const OWNER_COMMAND_CAPACITY: usize = 64;

/// One observable result from a scheduler's policy cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmWorkStealingCycle {
    directive: VmWorkDirective,
    transferred: usize,
}

impl VmWorkStealingCycle {
    /// Returns the policy action executed by this cycle.
    pub(crate) const fn directive(self) -> VmWorkDirective {
        self.directive
    }

    /// Returns the number of actors published on the destination owner.
    pub(crate) const fn transferred(self) -> usize {
        self.transferred
    }
}

/// Destination owner failure retaining every claim available for rollback.
#[derive(Debug)]
struct OwnerImportFailure {
    reason: String,
    pending: Option<VmSchedulerStealBatch>,
    imported: usize,
}

impl OwnerImportFailure {
    /// Converts a scheduler admission failure into an owner-protocol failure.
    fn from_scheduler(failure: VmSchedulerStealImportFailure) -> Self {
        let reason = failure.reason().to_string();
        let imported = failure.imported();
        let pending = failure.into_pending();
        Self {
            reason,
            pending: Some(pending),
            imported,
        }
    }
}

/// Commands are the only mutable access to one scheduler's runnable queues.
enum OwnerCommand {
    Snapshot {
        reply: SyncSender<Result<VmSchedulerWorkSnapshot, String>>,
    },
    Claim {
        class: VmSchedulerClass,
        maximum: usize,
        reply: SyncSender<Result<VmSchedulerStealBatch, String>>,
    },
    Import {
        batch: VmSchedulerStealBatch,
        reply: SyncSender<Result<usize, VmSchedulerStealImportFailure>>,
    },
    Abort {
        batch: VmSchedulerStealBatch,
        reply: SyncSender<Result<(), String>>,
    },
    Wake {
        reply: SyncSender<()>,
    },
    Shutdown {
        reply: SyncSender<()>,
    },
}

/// Immutable command address for one thread-owned scheduler queue set.
struct SchedulerQueueOwner {
    scheduler: VmSchedulerId,
    inbox: SyncSender<OwnerCommand>,
    join: Option<JoinHandle<()>>,
}

impl SchedulerQueueOwner {
    /// Starts one bounded command loop around an already configured scheduler.
    fn spawn(
        scheduler_id: VmSchedulerId,
        scheduler: VmScheduler,
        processes: Arc<VmProcessTable>,
    ) -> Result<Self, String> {
        let (inbox, commands) = mpsc::sync_channel(OWNER_COMMAND_CAPACITY);
        let join = thread::Builder::new()
            .name(format!("terlan-vm-scheduler-{}", scheduler_id.index()))
            .spawn(move || owner_loop(scheduler_id, scheduler, processes, commands))
            .map_err(|error| format!("error[vm.work_stealing.owner_start]: {error}"))?;
        Ok(Self {
            scheduler: scheduler_id,
            inbox,
            join: Some(join),
        })
    }

    /// Requests one live snapshot from the queue owner.
    fn snapshot(&self) -> Result<VmSchedulerWorkSnapshot, String> {
        let (reply, response) = mpsc::sync_channel(1);
        self.send(OwnerCommand::Snapshot { reply }, "snapshot")?;
        receive(response, self.scheduler, "snapshot")?
    }

    /// Claims a bounded batch on the victim thread.
    fn claim(
        &self,
        class: VmSchedulerClass,
        maximum: usize,
    ) -> Result<VmSchedulerStealBatch, String> {
        let (reply, response) = mpsc::sync_channel(1);
        self.send(
            OwnerCommand::Claim {
                class,
                maximum,
                reply,
            },
            "claim",
        )?;
        receive(response, self.scheduler, "claim")?
    }

    /// Imports claims on the destination thread without losing send failures.
    fn import(&self, batch: VmSchedulerStealBatch) -> Result<usize, OwnerImportFailure> {
        let (reply, response) = mpsc::sync_channel(1);
        if let Err(error) = self.inbox.send(OwnerCommand::Import { batch, reply }) {
            let OwnerCommand::Import { batch, .. } = error.0 else {
                unreachable!("failed import send returns its import command")
            };
            return Err(OwnerImportFailure {
                reason: owner_stopped(self.scheduler, "import"),
                pending: Some(batch),
                imported: 0,
            });
        }
        match response.recv() {
            Ok(Ok(imported)) => Ok(imported),
            Ok(Err(failure)) => Err(OwnerImportFailure::from_scheduler(failure)),
            Err(_) => Err(OwnerImportFailure {
                reason: owner_stopped(self.scheduler, "import reply"),
                pending: None,
                imported: 0,
            }),
        }
    }

    /// Restores rejected claims on their victim thread.
    fn abort(&self, batch: VmSchedulerStealBatch) -> Result<(), String> {
        if batch.is_empty() {
            return Ok(());
        }
        let (reply, response) = mpsc::sync_channel(1);
        self.send(OwnerCommand::Abort { batch, reply }, "abort")?;
        receive(response, self.scheduler, "abort")?
    }

    /// Wakes an owner blocked on its command channel exactly once per request.
    fn wake(&self) -> Result<(), String> {
        let (reply, response) = mpsc::sync_channel(1);
        self.send(OwnerCommand::Wake { reply }, "wake")?;
        receive(response, self.scheduler, "wake")
    }

    /// Stops and joins one scheduler owner.
    fn shutdown(&mut self) -> Result<(), String> {
        let Some(join) = self.join.take() else {
            return Ok(());
        };
        let (reply, response) = mpsc::sync_channel(1);
        let command = self
            .send(OwnerCommand::Shutdown { reply }, "shutdown")
            .and_then(|()| receive(response, self.scheduler, "shutdown"));
        let joined = join.join().map_err(|_| {
            format!(
                "error[vm.work_stealing.owner_panic]: scheduler {} panicked",
                self.scheduler.index()
            )
        });
        command.and(joined)
    }

    /// Sends one command with stable owner-failure attribution.
    fn send(&self, command: OwnerCommand, operation: &str) -> Result<(), String> {
        self.inbox
            .send(command)
            .map_err(|_| owner_stopped(self.scheduler, operation))
    }
}

/// Shard-level policy and command owners for bounded queue rebalancing.
pub(crate) struct VmWorkStealingRuntime {
    owners: Vec<SchedulerQueueOwner>,
    policy: VmWorkStealingPolicy,
    stopped: bool,
}

impl VmWorkStealingRuntime {
    /// Starts one queue owner for each scheduler in stable index order.
    pub(crate) fn new(
        processes: VmProcessTable,
        schedulers: Vec<(VmSchedulerId, VmScheduler)>,
        config: VmWorkStealingConfig,
    ) -> Result<Self, String> {
        if schedulers.is_empty() {
            return Err("error[vm.work_stealing.runtime]: scheduler set is empty".to_string());
        }
        for (index, (scheduler, _)) in schedulers.iter().enumerate() {
            if scheduler.index() != index {
                return Err(format!(
                    "error[vm.work_stealing.runtime]: scheduler {} occupies slot {index}",
                    scheduler.index()
                ));
            }
        }
        let width = schedulers.len();
        let processes = Arc::new(processes);
        let mut owners = Vec::with_capacity(width);
        for (scheduler_id, scheduler) in schedulers {
            match SchedulerQueueOwner::spawn(scheduler_id, scheduler, Arc::clone(&processes)) {
                Ok(owner) => owners.push(owner),
                Err(error) => {
                    for owner in &mut owners {
                        let _ = owner.shutdown();
                    }
                    return Err(error);
                }
            }
        }
        Ok(Self {
            owners,
            policy: VmWorkStealingPolicy::new(width, config)?,
            stopped: false,
        })
    }

    /// Executes one policy decision through exact victim and thief owners.
    pub(crate) fn rebalance(
        &mut self,
        thief: VmSchedulerId,
    ) -> Result<VmWorkStealingCycle, String> {
        self.require_running()?;
        let snapshots = self.snapshots()?;
        let directive = self.policy.decide(thief, &snapshots)?;
        let transferred = match directive {
            VmWorkDirective::Steal(plan) => {
                let victim = self.owner(plan.victim())?;
                let batch = victim.claim(plan.class(), plan.maximum_actors())?;
                if batch.is_empty() {
                    self.policy.record_steal_result(thief, 0)?;
                    0
                } else {
                    match self.owner(plan.thief())?.import(batch) {
                        Ok(imported) => {
                            self.policy.record_steal_result(thief, imported)?;
                            imported
                        }
                        Err(failure) => {
                            let reason = failure.reason;
                            let imported = failure.imported;
                            let Some(pending) = failure.pending else {
                                self.stopped = true;
                                let stopped = self.stop_owners();
                                return Err(match stopped {
                                    Ok(()) => format!(
                                        "{reason}; error[vm.work_stealing.ownership_unknown]: shard stopped"
                                    ),
                                    Err(stop) => format!("{reason}; {stop}"),
                                });
                            };
                            self.owner(plan.victim())?.abort(pending)?;
                            self.policy.record_steal_result(thief, imported)?;
                            return Err(reason);
                        }
                    }
                }
            }
            VmWorkDirective::ServeLocal(_)
            | VmWorkDirective::Backoff(_)
            | VmWorkDirective::Sleep
            | VmWorkDirective::Stopped => 0,
        };
        Ok(VmWorkStealingCycle {
            directive,
            transferred,
        })
    }

    /// Returns stable live snapshots from every scheduler owner.
    pub(crate) fn snapshots(&self) -> Result<Vec<VmSchedulerWorkSnapshot>, String> {
        self.require_running()?;
        self.owners
            .iter()
            .map(SchedulerQueueOwner::snapshot)
            .collect()
    }

    /// Publishes runnable work and wakes a sleeping scheduler at most once.
    pub(crate) fn publish_runnable(&mut self, scheduler: VmSchedulerId) -> Result<bool, String> {
        self.require_running()?;
        let wake = self.policy.publish_runnable(scheduler)?;
        if wake {
            self.owner(scheduler)?.wake()?;
        }
        Ok(wake)
    }

    /// Stops every queue owner and rejects subsequent policy cycles.
    pub(crate) fn shutdown(&mut self) -> Result<(), String> {
        if self.stopped {
            return Ok(());
        }
        self.stopped = true;
        self.stop_owners()
    }

    /// Joins all owner threads after orderly shutdown or fail-stop attribution.
    fn stop_owners(&mut self) -> Result<(), String> {
        let mut failures = Vec::new();
        for owner in &mut self.owners {
            if let Err(error) = owner.shutdown() {
                failures.push(error);
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    /// Returns one scheduler owner after validating its bounded identity.
    fn owner(&self, scheduler: VmSchedulerId) -> Result<&SchedulerQueueOwner, String> {
        self.owners.get(scheduler.index()).ok_or_else(|| {
            format!(
                "error[vm.work_stealing.scheduler]: scheduler {} is outside width {}",
                scheduler.index(),
                self.owners.len()
            )
        })
    }

    /// Rejects work after orderly owner shutdown begins.
    fn require_running(&self) -> Result<(), String> {
        if self.stopped {
            Err("error[vm.work_stealing.stopped]: scheduler runtime is stopped".to_string())
        } else {
            Ok(())
        }
    }
}

impl Drop for VmWorkStealingRuntime {
    /// Best-effort owner shutdown for abandoned runtime handles.
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// Runs all queue mutations on one scheduler's dedicated thread.
fn owner_loop(
    scheduler_id: VmSchedulerId,
    mut scheduler: VmScheduler,
    processes: Arc<VmProcessTable>,
    commands: Receiver<OwnerCommand>,
) {
    while let Ok(command) = commands.recv() {
        match command {
            OwnerCommand::Snapshot { reply } => {
                let _ = reply.send(scheduler.work_snapshot(scheduler_id, true));
            }
            OwnerCommand::Claim {
                class,
                maximum,
                reply,
            } => {
                let _ = reply.send(scheduler.claim_stealable_batch(&processes, class, maximum));
            }
            OwnerCommand::Import { batch, reply } => {
                let _ = reply.send(scheduler.complete_steal_batch(&processes, batch));
            }
            OwnerCommand::Abort { batch, reply } => {
                let _ = reply.send(scheduler.abort_steal_batch(&processes, batch));
            }
            OwnerCommand::Wake { reply } => {
                let _ = reply.send(());
            }
            OwnerCommand::Shutdown { reply } => {
                let _ = reply.send(());
                return;
            }
        }
    }
}

/// Receives one bounded owner response with stable scheduler attribution.
fn receive<T>(
    response: Receiver<T>,
    scheduler: VmSchedulerId,
    operation: &str,
) -> Result<T, String> {
    response
        .recv()
        .map_err(|_| owner_stopped(scheduler, operation))
}

/// Renders one stable owner-channel failure.
fn owner_stopped(scheduler: VmSchedulerId, operation: &str) -> String {
    format!(
        "error[vm.work_stealing.owner_stopped]: scheduler {} stopped during {operation}",
        scheduler.index()
    )
}

#[cfg(test)]
#[path = "runtime_test.rs"]
mod runtime_test;
