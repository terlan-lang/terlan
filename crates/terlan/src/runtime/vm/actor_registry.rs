use super::super::process_alias::{VmProcessAlias, VmProcessAliasOptions};
use super::{actor_alias_error, actor_registry_error, VmActorRuntime, ACTOR_OPERATION_REDUCTIONS};
use crate::runtime::vm::process::VmProcessId;

impl VmActorRuntime {
    /// Registers a stable actor name.
    pub(crate) fn register_name(
        &mut self,
        name: impl Into<String>,
        pid: VmProcessId,
    ) -> Result<(), String> {
        self.processes
            .register_name(name, pid)
            .map_err(actor_registry_error)?;
        self.charge_actor_reductions(pid, ACTOR_OPERATION_REDUCTIONS);
        Ok(())
    }

    /// Looks up an actor name.
    pub(crate) fn lookup_name(&self, name: &str) -> Option<VmProcessId> {
        self.processes.lookup_name(name)
    }

    /// Removes one stable actor name.
    pub(crate) fn unregister_name(&mut self, name: &str) -> Result<VmProcessId, String> {
        let pid = self
            .processes
            .unregister_name(name)
            .map_err(actor_registry_error)?;
        self.charge_actor_reductions(pid, ACTOR_OPERATION_REDUCTIONS);
        Ok(pid)
    }

    /// Returns all stable actor names in deterministic lexical order.
    pub(crate) fn registered_names(&self) -> Vec<String> {
        self.processes.registered_names()
    }

    /// Creates a fresh opaque alias for one live actor.
    pub(crate) fn create_alias(&mut self, pid: VmProcessId) -> Result<VmProcessAlias, String> {
        self.create_alias_with_options(pid, VmProcessAliasOptions::default())
    }

    /// Creates an alias with explicit VM-owned delivery capabilities.
    pub(crate) fn create_alias_with_options(
        &mut self,
        pid: VmProcessId,
        options: VmProcessAliasOptions,
    ) -> Result<VmProcessAlias, String> {
        let alias = self
            .aliases
            .create_with_options(&self.processes, pid, options)
            .map_err(actor_alias_error)?;
        self.charge_actor_reductions(pid, ACTOR_OPERATION_REDUCTIONS);
        Ok(alias)
    }

    /// Resolves one actor alias.
    pub(crate) fn resolve_alias(&self, alias: VmProcessAlias) -> Option<VmProcessId> {
        self.aliases.resolve(alias)
    }

    /// Removes one actor alias.
    pub(crate) fn remove_alias(&mut self, alias: VmProcessAlias) -> Result<VmProcessId, String> {
        let pid = self.aliases.remove(alias).map_err(actor_alias_error)?;
        self.charge_actor_reductions(pid, ACTOR_OPERATION_REDUCTIONS);
        Ok(pid)
    }

    /// Returns aliases owned by one actor in allocation order.
    pub(crate) fn aliases_for_process(&self, pid: VmProcessId) -> Vec<VmProcessAlias> {
        self.aliases.aliases_for_process(pid)
    }

    /// Returns the number of live actor aliases.
    pub(crate) fn alias_count(&self) -> usize {
        self.aliases.len()
    }
}
