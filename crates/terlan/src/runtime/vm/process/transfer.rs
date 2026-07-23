//! Linear transfer of one VM process record between local process tables.

use std::fmt;

use super::{VmProcess, VmProcessId, VmProcessTable};

/// Complete process-owned state detached at a published migration boundary.
#[derive(Debug)]
pub(crate) struct VmProcessTransfer {
    process: VmProcess,
    names: Vec<String>,
    process_identity_watermark: u64,
    message_identity_watermark: u64,
}

impl VmProcessTransfer {
    /// Returns the process identity carried by this transfer.
    pub(crate) const fn process_id(&self) -> VmProcessId {
        self.process.pid
    }

    /// Returns the actor heap bytes carried by the exact process record.
    pub(crate) const fn heap_bytes(&self) -> usize {
        self.process.heap_bytes
    }

    /// Returns the exact registered names carried with the process.
    #[cfg(test)]
    pub(crate) fn names(&self) -> &[String] {
        &self.names
    }
}

/// Failed destination admission that preserves exact rollback ownership.
#[derive(Debug)]
pub(crate) struct VmProcessImportFailure {
    reason: String,
    transfer: VmProcessTransfer,
}

impl VmProcessImportFailure {
    /// Returns the stable import rejection.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns the complete process transfer for source restoration.
    pub(crate) fn into_transfer(self) -> VmProcessTransfer {
        self.transfer
    }
}

impl fmt::Display for VmProcessImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for VmProcessImportFailure {}

impl VmProcessTable {
    /// Detaches a process after integrating every locally published message.
    pub(crate) fn detach_process_for_transfer(
        &mut self,
        pid: VmProcessId,
    ) -> Result<VmProcessTransfer, String> {
        self.integrate_process_mailbox(pid)?;
        let names = self.names_for_process(pid);
        let process = self
            .processes
            .detach_for_transfer(pid)
            .map_err(|error| format!("process transfer detach failed: {error:?}"))?;
        for name in &names {
            self.names.remove(name);
        }
        Ok(VmProcessTransfer {
            process,
            names,
            process_identity_watermark: self.next_pid,
            message_identity_watermark: self.next_message_id,
        })
    }

    /// Validates destination ownership before consuming a process transfer.
    pub(crate) fn validate_process_import(
        &self,
        transfer: &VmProcessTransfer,
    ) -> Result<(), String> {
        let pid = transfer.process_id();
        if pid.as_u64() == 0 {
            return Err("process transfer owner identity must be nonzero".to_string());
        }
        if self.processes.contains(pid) {
            return Err(format!(
                "process transfer destination already contains process {}",
                pid.as_u64()
            ));
        }
        for name in &transfer.names {
            if let Some(existing) = self.names.get(name) {
                return Err(format!(
                    "process transfer name `{name}` is owned by process {}",
                    existing.as_u64()
                ));
            }
        }
        Ok(())
    }

    /// Imports a detached process or returns it unchanged for exact rollback.
    pub(crate) fn import_process_transfer(
        &mut self,
        transfer: VmProcessTransfer,
    ) -> Result<(), VmProcessImportFailure> {
        if let Err(reason) = self.validate_process_import(&transfer) {
            return Err(VmProcessImportFailure { reason, transfer });
        }
        let VmProcessTransfer {
            process,
            names,
            process_identity_watermark,
            message_identity_watermark,
        } = transfer;
        let pid = process.pid;
        if let Err((error, process)) = self.processes.import_transferred(pid, process) {
            return Err(VmProcessImportFailure {
                reason: format!("process transfer import failed: {error:?}"),
                transfer: VmProcessTransfer {
                    process,
                    names,
                    process_identity_watermark,
                    message_identity_watermark,
                },
            });
        }
        self.next_pid = self
            .next_pid
            .max(process_identity_watermark)
            .max(pid.as_u64());
        self.next_message_id = self.next_message_id.max(message_identity_watermark);
        for name in names {
            self.names.insert(name, pid);
        }
        Ok(())
    }
}
