//! Linear transfer of aliases owned by one migrating actor.

use std::fmt;

use super::{VmProcessAlias, VmProcessAliasRoute, VmProcessAliasTable, VmProcessId};

/// Exact aliases and capabilities detached for one actor owner.
#[derive(Debug)]
pub(crate) struct VmProcessAliasTransfer {
    owner: VmProcessId,
    aliases: Vec<(VmProcessAlias, VmProcessAliasRoute)>,
    identity_watermark: u64,
}

impl VmProcessAliasTransfer {
    /// Returns the actor resolved by every transferred alias.
    pub(crate) const fn owner(&self) -> VmProcessId {
        self.owner
    }

    /// Returns the number of exact aliases retained by this transfer.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.aliases.len()
    }
}

/// Failed alias import retaining all identities and capabilities.
#[derive(Debug)]
pub(crate) struct VmProcessAliasImportFailure {
    reason: String,
    transfer: VmProcessAliasTransfer,
}

impl VmProcessAliasImportFailure {
    /// Returns the stable destination rejection.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns every alias for source restoration.
    pub(crate) fn into_transfer(self) -> VmProcessAliasTransfer {
        self.transfer
    }
}

impl fmt::Display for VmProcessAliasImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for VmProcessAliasImportFailure {}

impl VmProcessAliasTable {
    /// Detaches every alias currently resolving to one actor owner.
    pub(crate) fn detach_owner_aliases(&mut self, owner: VmProcessId) -> VmProcessAliasTransfer {
        let identities = self.aliases_for_process(owner);
        let aliases = identities
            .into_iter()
            .map(|alias| {
                let route = self
                    .aliases
                    .remove(&alias)
                    .expect("inventoried owner alias remains present");
                (alias, route)
            })
            .collect();
        VmProcessAliasTransfer {
            owner,
            aliases,
            identity_watermark: self.next_alias,
        }
    }

    /// Validates alias identity and owner admission before mutation.
    pub(crate) fn validate_alias_import(
        &self,
        transfer: &VmProcessAliasTransfer,
    ) -> Result<(), String> {
        if transfer.owner.as_u64() == 0 {
            return Err("alias transfer owner identity must be nonzero".to_string());
        }
        for (alias, route) in &transfer.aliases {
            if route.owner != transfer.owner {
                return Err("alias transfer contains a cross-actor route".to_string());
            }
            if self.aliases.contains_key(alias) {
                return Err(format!(
                    "alias transfer destination already contains alias {}",
                    alias.as_u64()
                ));
            }
        }
        Ok(())
    }

    /// Imports aliases or returns them unchanged for source rollback.
    pub(crate) fn import_alias_transfer(
        &mut self,
        transfer: VmProcessAliasTransfer,
    ) -> Result<(), VmProcessAliasImportFailure> {
        if let Err(reason) = self.validate_alias_import(&transfer) {
            return Err(VmProcessAliasImportFailure { reason, transfer });
        }
        self.next_alias = self.next_alias.max(transfer.identity_watermark);
        for (alias, route) in transfer.aliases {
            self.aliases.insert(alias, route);
        }
        Ok(())
    }
}
