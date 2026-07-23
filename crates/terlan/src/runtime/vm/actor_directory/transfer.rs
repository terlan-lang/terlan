//! Linear extraction and import of unowned actor-directory values.

use std::sync::atomic::Ordering;

use super::{
    next_generation, VmActorCell, VmActorDirectory, VmActorDirectoryError, VmActorHandle,
    VmActorLifecycle, VmActorSlot,
};
use crate::runtime::vm::process::VmProcessId;

impl<T, P> VmActorDirectory<T, P> {
    /// Detaches one fully published, unowned value at a migration boundary.
    pub(crate) fn detach_for_transfer(
        &mut self,
        pid: VmProcessId,
    ) -> Result<T, VmActorDirectoryError> {
        let state = self.cell(pid)?.state()?;
        if state.owner != 0 {
            return Err(VmActorDirectoryError::AlreadyOwned {
                owner: state.owner,
                owner_generation: state.owner_generation,
            });
        }
        if !matches!(
            state.lifecycle,
            VmActorLifecycle::Queued
                | VmActorLifecycle::Yielding
                | VmActorLifecycle::Parked
                | VmActorLifecycle::Migrating
        ) {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: state.lifecycle,
                to: VmActorLifecycle::Migrating,
            });
        }
        let pins = self.cell(pid)?.lookup_pins.load(Ordering::Acquire);
        if pins != 0 {
            return Err(VmActorDirectoryError::LookupPinned(pins));
        }
        let pending = self.cell(pid)?.mailbox.len();
        if pending != 0 {
            return Err(VmActorDirectoryError::TransferMailboxNotDrained { pending });
        }
        if state.lifecycle != VmActorLifecycle::Migrating {
            self.transition_unowned(
                pid,
                &[
                    VmActorLifecycle::Queued,
                    VmActorLifecycle::Yielding,
                    VmActorLifecycle::Parked,
                ],
                VmActorLifecycle::Migrating,
            )?;
        }
        let slot_index = self
            .actor_slots
            .remove(&pid)
            .expect("validated actor retains its directory slot");
        let slot = self
            .slots
            .get_mut(slot_index as usize)
            .expect("validated actor slot remains allocated");
        let cell = slot
            .cell
            .take()
            .expect("validated actor cell remains allocated");
        self.free_slots.push(slot_index);
        Ok(cell.value)
    }

    /// Imports a detached value or returns it unchanged with the rejection.
    pub(crate) fn import_transferred(
        &mut self,
        pid: VmProcessId,
        value: T,
    ) -> Result<VmActorHandle, (VmActorDirectoryError, T)> {
        if self.actor_slots.contains_key(&pid) {
            let lifecycle = self
                .lifecycle(pid)
                .expect("indexed destination actor has a readable lifecycle");
            return Err((
                VmActorDirectoryError::InvalidTransition {
                    from: lifecycle,
                    to: VmActorLifecycle::Yielding,
                },
                value,
            ));
        }
        let slot_index = match self.free_slots.pop() {
            Some(slot) => slot,
            None => {
                let Ok(slot) = u32::try_from(self.slots.len()) else {
                    return Err((VmActorDirectoryError::SlotCapacityExceeded, value));
                };
                self.slots.push(VmActorSlot {
                    generation: 0,
                    cell: None,
                });
                slot
            }
        };
        let slot = self
            .slots
            .get_mut(slot_index as usize)
            .expect("reserved destination actor slot exists");
        slot.generation = next_generation(slot.generation);
        let handle = VmActorHandle {
            pid,
            slot: slot_index,
            actor_generation: slot.generation,
        };
        slot.cell = Some(VmActorCell::new(handle, value));
        self.actor_slots.insert(pid, slot_index);
        Ok(handle)
    }
}
