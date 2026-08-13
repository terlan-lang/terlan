pub(super) use super::super::{
    process::{VmExitReason, VmProcessTable},
    resource::VmResourceTable,
};
pub(super) use super::{
    VmMemoryAccountant, VmMemoryLimits, VmMemoryPressureOutcome, VmSharedAllocationId,
    VmSharedAllocationKind,
};
pub(super) use std::{path::PathBuf, time::Instant};

#[cfg(test)]
#[path = "memory_test/limits_and_accounting.rs"]
mod limits_and_accounting;
use limits_and_accounting::*;
#[cfg(test)]
#[path = "memory_test/shared_ownership_and_soak.rs"]
mod shared_ownership_and_soak;
