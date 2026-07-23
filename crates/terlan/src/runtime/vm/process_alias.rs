#![allow(dead_code)]

use std::collections::BTreeMap;

use super::process::{VmProcessId, VmProcessState, VmProcessTable};

#[path = "process_alias/transfer.rs"]
mod transfer;

#[allow(unused_imports)] // Public to staged MC-5 tests before migration orchestration lands.
pub(crate) use transfer::{VmProcessAliasImportFailure, VmProcessAliasTransfer};

/// Opaque local capability that resolves to one live VM process.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmProcessAlias(u64);

impl VmProcessAlias {
    /// Returns the numeric identity for diagnostics and serialization layers.
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }

    /// Creates an alias for adversarial VM runtime tests.
    pub(crate) fn from_raw_for_test(value: u64) -> Self {
        Self(value)
    }
}

/// Stable process alias lifecycle error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmProcessAliasError {
    MissingProcess(VmProcessId),
    ExitedProcess(VmProcessId),
    MissingAlias(VmProcessAlias),
    PriorityNotEnabled(VmProcessAlias),
    AliasSpaceExhausted,
}

/// Capabilities attached to a newly allocated process alias.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmProcessAliasOptions {
    priority: bool,
    reply: bool,
}

impl VmProcessAliasOptions {
    /// Allows explicitly priority-tagged sends through the alias.
    pub(crate) fn priority(mut self) -> Self {
        self.priority = true;
        self
    }

    /// Revokes the alias after its first successfully delivered reply.
    pub(crate) fn reply(mut self) -> Self {
        self.reply = true;
        self
    }
}

/// Immutable delivery metadata resolved for one live alias.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmProcessAliasRoute {
    pub(crate) owner: VmProcessId,
    pub(crate) priority: bool,
    pub(crate) reply: bool,
}

/// VM-owned local process alias table.
#[derive(Debug, Default)]
pub(crate) struct VmProcessAliasTable {
    next_alias: u64,
    aliases: BTreeMap<VmProcessAlias, VmProcessAliasRoute>,
}

impl VmProcessAliasTable {
    /// Creates a fresh alias for one live process.
    pub(crate) fn create(
        &mut self,
        processes: &VmProcessTable,
        process: VmProcessId,
    ) -> Result<VmProcessAlias, VmProcessAliasError> {
        self.create_with_options(processes, process, VmProcessAliasOptions::default())
    }

    /// Creates an alias with explicit priority and reply capabilities.
    pub(crate) fn create_with_options(
        &mut self,
        processes: &VmProcessTable,
        process: VmProcessId,
        options: VmProcessAliasOptions,
    ) -> Result<VmProcessAlias, VmProcessAliasError> {
        let record = processes
            .get(process)
            .ok_or(VmProcessAliasError::MissingProcess(process))?;
        if matches!(record.state, VmProcessState::Exited(_)) {
            return Err(VmProcessAliasError::ExitedProcess(process));
        }
        let next = self
            .next_alias
            .checked_add(1)
            .ok_or(VmProcessAliasError::AliasSpaceExhausted)?;
        self.next_alias = next;
        let alias = VmProcessAlias(next);
        self.aliases.insert(
            alias,
            VmProcessAliasRoute {
                owner: process,
                priority: options.priority,
                reply: options.reply,
            },
        );
        Ok(alias)
    }

    /// Resolves one alias without exposing mutable alias-table state.
    pub(crate) fn resolve(&self, alias: VmProcessAlias) -> Option<VmProcessId> {
        self.route(alias).map(|route| route.owner)
    }

    /// Resolves delivery capabilities without exposing mutable table state.
    pub(crate) fn route(&self, alias: VmProcessAlias) -> Option<VmProcessAliasRoute> {
        self.aliases.get(&alias).copied()
    }

    /// Revokes a reply alias after a successful reply delivery.
    pub(crate) fn consume_reply(&mut self, alias: VmProcessAlias) {
        if self.aliases.get(&alias).is_some_and(|route| route.reply) {
            self.aliases.remove(&alias);
        }
    }

    /// Removes one alias and returns its process owner.
    pub(crate) fn remove(
        &mut self,
        alias: VmProcessAlias,
    ) -> Result<VmProcessId, VmProcessAliasError> {
        self.aliases
            .remove(&alias)
            .map(|route| route.owner)
            .ok_or(VmProcessAliasError::MissingAlias(alias))
    }

    /// Returns aliases owned by one process in allocation order.
    pub(crate) fn aliases_for_process(&self, process: VmProcessId) -> Vec<VmProcessAlias> {
        self.aliases
            .iter()
            .filter(|&(_, route)| route.owner == process)
            .map(|(alias, _)| *alias)
            .collect()
    }

    /// Reports whether a process owns any priority-capable alias.
    pub(crate) fn has_priority_alias(&self, process: VmProcessId) -> bool {
        self.aliases
            .values()
            .any(|route| route.owner == process && route.priority)
    }

    /// Removes every alias owned by one process in allocation order.
    pub(crate) fn remove_process(&mut self, process: VmProcessId) -> Vec<VmProcessAlias> {
        let aliases = self.aliases_for_process(process);
        for alias in &aliases {
            self.aliases.remove(alias);
        }
        aliases
    }

    /// Returns the number of live aliases.
    pub(crate) fn len(&self) -> usize {
        self.aliases.len()
    }

    pub(crate) fn exhaust_for_test(&mut self) {
        self.next_alias = u64::MAX;
    }
}

#[cfg(test)]
#[path = "process_alias_transfer_test.rs"]
mod process_alias_transfer_test;
