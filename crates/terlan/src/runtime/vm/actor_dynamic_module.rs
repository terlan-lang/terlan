use super::super::dynamic_module::{
    VmDynamicModuleDescriptor, VmDynamicModuleLeaseId, VmDynamicModuleLoadOutcome,
    VmDynamicModuleReloadOutcome, VmDynamicModuleSnapshot, VmDynamicModuleUnloadOutcome,
};
use super::{VmActorRuntime, VmProcessId};

impl VmActorRuntime {
    /// Loads or references a validated dynamic module for one live actor.
    pub(crate) fn load_dynamic_module(
        &mut self,
        owner: VmProcessId,
        descriptor: VmDynamicModuleDescriptor,
    ) -> Result<VmDynamicModuleLoadOutcome, String> {
        self.dynamic_modules
            .load(&self.processes, owner, descriptor)
    }

    /// Opens a live actor-owned lease on a loaded dynamic module.
    pub(crate) fn open_dynamic_module_lease(
        &mut self,
        owner: VmProcessId,
        module_name: &str,
    ) -> Result<VmDynamicModuleLeaseId, String> {
        self.dynamic_modules
            .open_lease(&self.processes, owner, module_name)
    }

    /// Closes one dynamic-module lease and completes any drained transition.
    pub(crate) fn close_dynamic_module_lease(
        &mut self,
        lease: VmDynamicModuleLeaseId,
    ) -> Result<(), String> {
        self.dynamic_modules.close_lease(lease)
    }

    /// Releases an actor's module reference, optionally forcing lease drain.
    pub(crate) fn unload_dynamic_module(
        &mut self,
        owner: VmProcessId,
        module_name: &str,
        force: bool,
    ) -> Result<VmDynamicModuleUnloadOutcome, String> {
        self.dynamic_modules
            .request_unload(&self.processes, owner, module_name, force)
    }

    /// Requests an atomic module generation replacement for one owner.
    pub(crate) fn reload_dynamic_module(
        &mut self,
        owner: VmProcessId,
        replacement: VmDynamicModuleDescriptor,
    ) -> Result<VmDynamicModuleReloadOutcome, String> {
        self.dynamic_modules
            .request_reload(&self.processes, owner, replacement)
    }

    /// Returns deterministic live dynamic-module lifecycle rows.
    pub(crate) fn dynamic_module_snapshots(&self) -> Vec<VmDynamicModuleSnapshot> {
        self.dynamic_modules.snapshots()
    }
}

#[cfg(test)]
#[path = "actor_dynamic_module_test.rs"]
mod actor_dynamic_module_test;
