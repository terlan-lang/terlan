/// VM-owned process identifier.
///
/// Inputs:
/// - Monotonic runtime allocation.
///
/// Output:
/// - Stable process id value used by local VM tables.
///
/// Transformation:
/// - Keeps process identity independent from OTP pid syntax or any host
///   runtime handle.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmProcessId(u64);

impl VmProcessId {
    /// Creates one identity from the process table's monotonic allocator.
    pub(super) const fn from_allocated(value: u64) -> Self {
        Self(value)
    }

    /// Returns the reserved process id used by VM-owned runtime workers.
    pub(crate) fn system_runtime_worker() -> Self {
        Self(0)
    }

    /// Returns the numeric process id.
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }

    /// Resolves a nonzero native control-frame owner to VM process identity.
    pub(crate) fn from_native_owner(value: u64) -> Result<Self, String> {
        if value == 0 {
            return Err("native continuation owner identity must be nonzero".to_string());
        }
        Ok(Self(value))
    }

    /// Resolves a nonzero native transition argument to a VM process identity.
    pub(crate) fn from_native_recipient(value: u64) -> Result<Self, String> {
        if value == 0 {
            return Err("native send recipient identity must be nonzero".to_string());
        }
        Ok(Self(value))
    }

    /// Creates a process id for adversarial VM runtime tests.
    #[cfg(test)]
    pub(crate) fn from_raw_for_test(value: u64) -> Self {
        Self(value)
    }
}
