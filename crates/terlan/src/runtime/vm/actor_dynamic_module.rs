use super::*;

impl VmActorRuntime {
    /// Loads or references a validated dynamic module for one live actor.
    #[cfg(test)]
    pub(crate) fn load_dynamic_module(
        &mut self,
        owner: VmProcessId,
        descriptor: VmDynamicModuleDescriptor,
    ) -> Result<VmDynamicModuleLoadOutcome, String> {
        self.dynamic_modules
            .load(&self.processes, owner, descriptor)
    }

    /// Opens a live actor-owned lease on a loaded dynamic module.
    #[cfg(test)]
    pub(crate) fn open_dynamic_module_lease(
        &mut self,
        owner: VmProcessId,
        module_name: &str,
    ) -> Result<VmDynamicModuleLeaseId, String> {
        self.dynamic_modules
            .open_lease(&self.processes, owner, module_name)
    }

    /// Releases an actor's module reference, optionally forcing lease drain.
    #[cfg(test)]
    pub(crate) fn unload_dynamic_module(
        &mut self,
        owner: VmProcessId,
        module_name: &str,
        force: bool,
    ) -> Result<VmDynamicModuleUnloadOutcome, String> {
        self.dynamic_modules
            .request_unload(&self.processes, owner, module_name, force)
    }

    /// Returns deterministic live dynamic-module lifecycle rows.
    #[cfg(test)]
    pub(crate) fn dynamic_module_snapshots(&self) -> Vec<VmDynamicModuleSnapshot> {
        self.dynamic_modules.snapshots()
    }
}

#[cfg(test)]
#[path = "actor_dynamic_module_test.rs"]
#[cfg(test)]
mod actor_dynamic_module_test;
