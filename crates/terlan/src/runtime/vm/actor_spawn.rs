use super::super::failure::VmMonitorRef;
use super::super::process::{VmProcessId, VmProcessSource};
use super::super::scheduler::VmSchedulerClass;
use super::{VmActorRuntime, ACTOR_OPERATION_REDUCTIONS};

/// Typed relationship and scheduling policy for one child actor spawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmActorSpawnOptions {
    pub(crate) scheduler_class: VmSchedulerClass,
    pub(crate) link_parent: bool,
    pub(crate) monitor_child: bool,
}

impl Default for VmActorSpawnOptions {
    fn default() -> Self {
        Self {
            scheduler_class: VmSchedulerClass::Normal,
            link_parent: false,
            monitor_child: false,
        }
    }
}

impl VmActorSpawnOptions {
    /// Assigns the child scheduler class.
    pub(crate) fn with_scheduler_class(mut self, scheduler_class: VmSchedulerClass) -> Self {
        self.scheduler_class = scheduler_class;
        self
    }

    /// Links the parent and child failure lifecycles.
    pub(crate) fn linked(mut self) -> Self {
        self.link_parent = true;
        self
    }

    /// Monitors the child from its parent.
    pub(crate) fn monitored(mut self) -> Self {
        self.monitor_child = true;
        self
    }
}

/// Stable identities produced by one actor child spawn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmActorSpawnResult {
    pub(crate) pid: VmProcessId,
    pub(crate) monitor_ref: Option<VmMonitorRef>,
}

impl VmActorRuntime {
    /// Spawns one child with typed scheduler and failure relationships.
    pub(crate) fn spawn_child_with_options(
        &mut self,
        parent: VmProcessId,
        source: VmProcessSource,
        options: VmActorSpawnOptions,
    ) -> Result<VmActorSpawnResult, String> {
        self.ensure_live_process(parent, "spawn child from")?;

        let pid = self.processes.spawn_child(parent, source)?;
        self.scheduler
            .enqueue_runnable_with_class(&self.processes, pid, options.scheduler_class)
            .expect("fresh child process must be runnable");
        if options.link_parent {
            self.failures
                .link(&self.processes, parent, pid)
                .expect("validated parent and fresh child must link");
        }
        let monitor_ref = if options.monitor_child {
            Some(
                self.failures
                    .monitor(&mut self.references, &self.processes, parent, pid)?,
            )
        } else {
            None
        };
        self.charge_actor_reductions(parent, ACTOR_OPERATION_REDUCTIONS);

        Ok(VmActorSpawnResult { pid, monitor_ref })
    }
}
