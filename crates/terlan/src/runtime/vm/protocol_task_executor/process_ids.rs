//! Block-reserved process identities consumed without cross-owner contention.

use std::sync::atomic::{AtomicU64, Ordering};

use super::VmProtocolTaskRoute;
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::scheduler_topology::VmSchedulerId;

const PROCESS_ID_RESERVATION_SIZE: u64 = 1 << 20;

static NEXT_PROTOCOL_PROCESS_BLOCK: AtomicU64 = AtomicU64::new(1);

/// One fixed protocol owner's private range of globally unique process IDs.
pub(super) struct VmProtocolProcessIds {
    next: u64,
    remaining: u64,
}

impl VmProtocolProcessIds {
    pub(super) fn new() -> Result<Self, String> {
        let next = reserve_block()?;
        Ok(Self {
            next,
            remaining: PROCESS_ID_RESERVATION_SIZE,
        })
    }

    pub(super) fn next_route(
        &mut self,
        scheduler: VmSchedulerId,
    ) -> Result<VmProtocolTaskRoute, String> {
        if self.remaining == 0 {
            self.next = reserve_block()?;
            self.remaining = PROCESS_ID_RESERVATION_SIZE;
        }
        let identity = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| "error[vm.protocol_process]: identity exhausted".to_string())?;
        self.remaining -= 1;
        Ok(VmProtocolTaskRoute {
            process: VmProcessId::from_native_owner(identity)?,
            scheduler,
        })
    }
}

fn reserve_block() -> Result<u64, String> {
    NEXT_PROTOCOL_PROCESS_BLOCK
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(PROCESS_ID_RESERVATION_SIZE)
        })
        .map_err(|_| "error[vm.protocol_process]: identity exhausted".to_string())
}
