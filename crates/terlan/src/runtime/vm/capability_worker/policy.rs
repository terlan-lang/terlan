//! Closed supervisor startup policy for capability workers.

use std::path::PathBuf;

use super::{DEFAULT_CREDIT_LIMIT, DEFAULT_MAX_PAYLOAD_BYTES, DEFAULT_MAX_REQUESTS};
use crate::terlan_native_boundary::metadata::NativeBoundaryExecutionProfile;

/// Closed process policy used when the VM starts a capability worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmCapabilityWorkerPolicy {
    /// Absolute worker executable path.
    pub(super) executable: PathBuf,
    /// Supervisor-selected durable directory; never removed with worker scratch data.
    pub(crate) storage_directory: Option<PathBuf>,
    /// Explicit reason this operation crosses a process boundary.
    pub(super) execution_profile: NativeBoundaryExecutionProfile,
    /// Capabilities granted to the child process.
    pub(super) capabilities: Vec<String>,
    /// Scheduler classes admitted for the child process.
    pub(super) worker_classes: Vec<String>,
    /// Maximum bytes in one request or response frame.
    pub(super) max_payload_bytes: usize,
    /// Maximum operations accepted during the child lifetime.
    pub(super) max_requests: u64,
    /// Maximum concurrently parked requests.
    pub(super) credit_limit: u64,
}

impl VmCapabilityWorkerPolicy {
    /// Creates an empty-authority policy for one explicit worker-only profile.
    pub(crate) fn new(
        executable: impl Into<PathBuf>,
        execution_profile: NativeBoundaryExecutionProfile,
    ) -> Result<Self, String> {
        let executable = executable.into();
        if !executable.is_absolute() {
            return Err("capability-worker executable path must be absolute".to_string());
        }
        Ok(Self {
            executable,
            storage_directory: None,
            execution_profile,
            capabilities: Vec::new(),
            worker_classes: Vec::new(),
            max_payload_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
            max_requests: DEFAULT_MAX_REQUESTS,
            credit_limit: DEFAULT_CREDIT_LIMIT,
        })
    }

    /// Adds one explicit capability grant to the worker policy.
    pub(crate) fn allow(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.push(capability.into());
        self
    }

    /// Adds one explicit scheduler-class grant to the worker policy.
    pub(crate) fn admit_worker_class(mut self, worker_class: impl Into<String>) -> Self {
        self.worker_classes.push(worker_class.into());
        self
    }

    /// Replaces the default frame-size limit with a positive bound.
    #[cfg(test)]
    pub(crate) fn with_max_payload_bytes(mut self, maximum: usize) -> Result<Self, String> {
        if maximum == 0 {
            return Err("capability-worker payload limit must be positive".to_string());
        }
        self.max_payload_bytes = maximum;
        Ok(self)
    }

    /// Replaces the default lifetime request limit with a positive bound.
    #[cfg(test)]
    pub(crate) fn with_max_requests(mut self, maximum: u64) -> Result<Self, String> {
        if maximum == 0 {
            return Err("capability-worker request limit must be positive".to_string());
        }
        self.max_requests = maximum;
        Ok(self)
    }

    /// Replaces the default concurrent request-credit limit.
    pub(crate) fn with_credit_limit(mut self, limit: u64) -> Result<Self, String> {
        if limit == 0 {
            return Err("capability-worker credit limit must be positive".to_string());
        }
        self.credit_limit = limit;
        Ok(self)
    }
}
