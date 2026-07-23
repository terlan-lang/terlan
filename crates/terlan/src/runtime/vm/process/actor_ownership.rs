use super::super::actor_directory::{
    VmActorDirectoryError, VmActorLifecycle, VmActorMutatorToken, VmActorTransitionEvent,
};
use super::{VmExitReason, VmProcess, VmProcessId, VmProcessState, VmProcessTable};

const VM_CONTROL_PLANE_OWNER: u64 = (1_u64 << 20) - 1;

impl VmProcessTable {
    /// Integrates all complete MPSC fragments under receiver ownership.
    pub(crate) fn integrate_process_mailbox(&mut self, pid: VmProcessId) -> Result<usize, String> {
        let token = self
            .processes
            .acquire_control_mutator(pid, VM_CONTROL_PLANE_OWNER)
            .map_err(actor_directory_error)?;
        let result = self.integrate_actor_mailbox(&token);
        self.processes
            .release_control_mutator(token)
            .map_err(actor_directory_error)?;
        result
    }

    /// Integrates immediately when unowned or leaves publication for its owner.
    pub(crate) fn integrate_process_mailbox_if_unowned(
        &mut self,
        pid: VmProcessId,
    ) -> Result<bool, String> {
        let token = match self
            .processes
            .acquire_control_mutator(pid, VM_CONTROL_PLANE_OWNER)
        {
            Ok(token) => token,
            Err(VmActorDirectoryError::AlreadyOwned { .. }) => return Ok(false),
            Err(error) => return Err(actor_directory_error(error)),
        };
        let result = self.integrate_actor_mailbox(&token);
        self.processes
            .release_control_mutator(token)
            .map_err(actor_directory_error)?;
        result?;
        Ok(true)
    }

    /// Integrates complete fragments using an existing scheduler mutator.
    pub(crate) fn integrate_actor_mailbox(
        &mut self,
        token: &VmActorMutatorToken,
    ) -> Result<usize, String> {
        self.processes
            .drain_publications(token, |process, publication, mut message| {
                message.publication_sequence = publication.sequence;
                process.integrate_message(message);
            })
            .map_err(actor_directory_error)
    }

    /// Removes one exited tombstone after its diagnostics have been captured.
    pub(crate) fn reap_exited(&mut self, pid: VmProcessId) -> Result<(), String> {
        let process = self
            .processes
            .get(pid)
            .ok_or_else(|| format!("missing process {}", pid.as_u64()))?;
        if !matches!(process.state, VmProcessState::Exited(_)) {
            return Err(format!("cannot reap live process {}", pid.as_u64()));
        }
        self.processes.reclaim(pid).map_err(actor_directory_error)?;
        Ok(())
    }

    /// Exits a process and returns resources that must be cleaned up.
    pub(crate) fn exit_process(
        &mut self,
        pid: VmProcessId,
        reason: VmExitReason,
    ) -> Result<Vec<String>, String> {
        if !self.processes.contains(pid) {
            return Err(format!("missing process {}", pid.as_u64()));
        }
        self.processes
            .mark_exiting(pid)
            .map_err(actor_directory_error)?;
        let cleanup = self.with_process_control_mutator(pid, |process| process.exit(reason))?;
        self.processes
            .mark_retired(pid)
            .map_err(actor_directory_error)?;
        self.remove_registered_names(pid);
        Ok(cleanup)
    }

    /// Marks one runnable process as represented in the scheduler queue.
    pub(crate) fn mark_actor_queued(&self, pid: VmProcessId) -> Result<(), String> {
        self.processes
            .mark_queued(pid)
            .map_err(actor_directory_error)
    }

    /// Returns actor lifecycle for focused scheduler ownership tests.
    #[cfg(test)]
    pub(crate) fn actor_lifecycle(&self, pid: VmProcessId) -> Result<VmActorLifecycle, String> {
        self.processes.lifecycle(pid).map_err(actor_directory_error)
    }

    /// Acquires exclusive scheduler ownership of one queued actor.
    pub(crate) fn acquire_actor_mutator(
        &mut self,
        pid: VmProcessId,
        owner: u64,
    ) -> Result<VmActorMutatorToken, String> {
        self.processes
            .acquire_mutator(pid, owner)
            .map_err(actor_directory_error)
    }

    /// Executes one operation against actor state protected by a mutator token.
    pub(crate) fn with_actor_mutator<R>(
        &mut self,
        token: &VmActorMutatorToken,
        mutate: impl FnOnce(&mut VmProcess) -> R,
    ) -> Result<R, String> {
        self.processes
            .with_mutator(token, mutate)
            .map_err(actor_directory_error)
    }

    /// Runs one control-plane mutation under a fresh ownership generation.
    pub(crate) fn with_process_control_mutator<R>(
        &mut self,
        pid: VmProcessId,
        mutate: impl FnOnce(&mut VmProcess) -> R,
    ) -> Result<R, String> {
        let token = self
            .processes
            .acquire_control_mutator(pid, VM_CONTROL_PLANE_OWNER)
            .map_err(actor_directory_error)?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.processes.with_mutator(&token, mutate)
        }));
        match result {
            Ok(result) => {
                let result = result.map_err(actor_directory_error)?;
                self.processes
                    .release_control_mutator(token)
                    .map_err(actor_directory_error)?;
                Ok(result)
            }
            Err(payload) => {
                self.processes
                    .release_control_mutator(token)
                    .expect("panicking control mutation must release its current owner");
                std::panic::resume_unwind(payload)
            }
        }
    }

    /// Releases scheduler ownership into a stable actor lifecycle state.
    pub(crate) fn release_actor_mutator(
        &mut self,
        token: VmActorMutatorToken,
        lifecycle: VmActorLifecycle,
    ) -> Result<VmActorLifecycle, String> {
        self.processes
            .release_mutator(token, lifecycle)
            .map_err(actor_directory_error)
    }

    /// Returns stable actor ownership events for diagnostics and replay.
    pub(crate) fn actor_transition_events(&self) -> Vec<VmActorTransitionEvent> {
        self.processes.transition_events()
    }
}

/// Preserves a stable process-table string boundary around typed ownership errors.
fn actor_directory_error(error: VmActorDirectoryError) -> String {
    format!("actor directory ownership error: {error:?}")
}
