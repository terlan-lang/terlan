//! Linear transfer of resource records that retain one actor owner.

use std::fmt;

use super::{VmProcessId, VmResourceRecord, VmResourceTable};

/// Complete owner-scoped resource state detached for actor migration.
#[derive(Debug)]
pub(crate) struct VmResourceTransfer {
    owner: VmProcessId,
    records: Vec<VmResourceRecord>,
    identity_watermark: u64,
}

impl VmResourceTransfer {
    /// Returns the actor that owns every transferred resource.
    pub(crate) const fn owner(&self) -> VmProcessId {
        self.owner
    }

    /// Returns the number of exact resource records in this transfer.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns exact resource identities for memory-ownership validation.
    pub(crate) fn resource_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.records.iter().map(|record| record.id.as_u64())
    }
}

/// Failed resource import that preserves every record for rollback.
#[derive(Debug)]
pub(crate) struct VmResourceImportFailure {
    reason: String,
    transfer: VmResourceTransfer,
}

impl VmResourceImportFailure {
    /// Returns the stable destination rejection.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns complete resource ownership for source restoration.
    pub(crate) fn into_transfer(self) -> VmResourceTransfer {
        self.transfer
    }
}

impl fmt::Display for VmResourceImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for VmResourceImportFailure {}

impl VmResourceTable {
    /// Detaches every resource record owned by one unchanged actor identity.
    pub(crate) fn detach_owner_resources(&mut self, owner: VmProcessId) -> VmResourceTransfer {
        let identities = self
            .resources
            .iter()
            .filter_map(|(identity, record)| (record.owner == owner).then_some(*identity))
            .collect::<Vec<_>>();
        let records = identities
            .into_iter()
            .map(|identity| {
                self.resources
                    .remove(&identity)
                    .expect("inventoried owner resource remains present")
            })
            .collect();
        VmResourceTransfer {
            owner,
            records,
            identity_watermark: self.next_id,
        }
    }

    /// Validates exact owner and identity admission before table mutation.
    pub(crate) fn validate_resource_import(
        &self,
        transfer: &VmResourceTransfer,
    ) -> Result<(), String> {
        if transfer.owner.as_u64() == 0 {
            return Err("resource transfer owner identity must be nonzero".to_string());
        }
        for record in &transfer.records {
            if record.owner != transfer.owner {
                return Err("resource transfer contains a cross-actor record".to_string());
            }
            if self.resources.contains_key(&record.id) {
                return Err(format!(
                    "resource transfer destination already contains resource {}",
                    record.id.as_u64()
                ));
            }
        }
        Ok(())
    }

    /// Imports owner resources or returns every record unchanged for rollback.
    pub(crate) fn import_resource_transfer(
        &mut self,
        transfer: VmResourceTransfer,
    ) -> Result<(), VmResourceImportFailure> {
        if let Err(reason) = self.validate_resource_import(&transfer) {
            return Err(VmResourceImportFailure { reason, transfer });
        }
        self.next_id = self.next_id.max(transfer.identity_watermark);
        for record in transfer.records {
            self.resources.insert(record.id, record);
        }
        Ok(())
    }
}
