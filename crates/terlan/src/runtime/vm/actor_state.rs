//! Generic actor-owned state resources supplied to package services.

use std::sync::Arc;
use terlan_runtime_abi::{ActorStateStore, BoundaryError, ErrorDomain};

use super::actor::VmActorRuntime;
use super::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState};
use super::table::{VmTableAccess, VmTableEvent, VmTableId, VmTableStore};
use super::ReplValue;

#[derive(Debug, Default)]
pub(crate) struct VmActorStateStore {
    origin: Arc<()>,
    actors: VmActorRuntime,
    tables: VmTableStore,
}

#[derive(Clone, Debug)]
pub(crate) struct VmActorStateHandle {
    origin: Arc<()>,
    actor: VmProcessId,
    table: VmTableId,
}

impl VmActorStateStore {
    fn check_owner(&self, handle: &VmActorStateHandle) -> Result<(), BoundaryError> {
        if Arc::ptr_eq(&self.origin, &handle.origin) {
            Ok(())
        } else {
            Err(state_error(
                "error[vm.actor_state.foreign_handle]: state belongs to another host",
            ))
        }
    }
}

impl ActorStateStore<String> for VmActorStateStore {
    type Handle = VmActorStateHandle;

    fn create(&mut self, owner: &str, name: &str) -> Result<Self::Handle, BoundaryError> {
        let actor = self
            .actors
            .spawn_root(VmProcessSource::new(owner, "state", 0));
        let table = match self.tables.create(
            self.actors.processes(),
            actor,
            name,
            VmTableAccess::OwnerOnly,
        ) {
            Ok(VmTableEvent::Created { id, .. }) => id,
            result => {
                self.actors
                    .exit_actor(actor, VmExitReason::Normal)
                    .map_err(state_error)?;
                return Err(state_error(format!(
                    "actor state table creation failed: {result:?}"
                )));
            }
        };
        Ok(VmActorStateHandle {
            origin: Arc::clone(&self.origin),
            actor,
            table,
        })
    }

    fn check_live(&self, handle: &Self::Handle) -> Result<(), BoundaryError> {
        self.check_owner(handle)?;
        match self
            .actors
            .processes()
            .get(handle.actor)
            .map(|process| &process.state)
        {
            Some(VmProcessState::Exited(reason)) => Err(state_error(format!(
                "actor {} exited: {reason:?}",
                handle.actor.as_u64()
            ))),
            None => Err(state_error("actor state owner is missing")),
            Some(_) => Ok(()),
        }
    }

    fn release(&mut self, handle: &Self::Handle) -> Result<(), BoundaryError> {
        self.check_owner(handle)?;
        let result = if self.check_live(handle).is_ok() {
            self.actors
                .exit_actor(handle.actor, VmExitReason::Normal)
                .map(|_| ())
                .map_err(state_error)
        } else {
            Ok(())
        };
        self.tables.cleanup_owner(handle.actor);
        result
    }

    fn read(&self, handle: &Self::Handle, key: &str) -> Result<Option<String>, BoundaryError> {
        self.check_live(handle)?;
        self.tables
            .lookup(
                self.actors.processes(),
                handle.actor,
                handle.table,
                &ReplValue::String(key.into()),
            )
            .map_err(state_error)?
            .map(|value| match value {
                ReplValue::String(text) => Ok(text),
                _ => Err(state_error(
                    "actor state value does not match the String store contract",
                )),
            })
            .transpose()
    }

    fn write(
        &mut self,
        handle: &Self::Handle,
        key: &str,
        value: String,
    ) -> Result<(), BoundaryError> {
        self.check_live(handle)?;
        self.tables
            .insert(
                self.actors.processes(),
                handle.actor,
                handle.table,
                ReplValue::String(key.into()),
                ReplValue::String(value),
            )
            .map(|_| ())
            .map_err(state_error)
    }

    fn delete(&mut self, handle: &Self::Handle, key: &str) -> Result<(), BoundaryError> {
        self.check_live(handle)?;
        self.tables
            .delete(
                self.actors.processes(),
                handle.actor,
                handle.table,
                &ReplValue::String(key.into()),
            )
            .map(|_| ())
            .map_err(state_error)
    }
}

fn state_error(message: impl Into<String>) -> BoundaryError {
    BoundaryError::message(ErrorDomain::VmRuntime, "actor-owned state", message)
}

#[cfg(test)]
#[path = "actor_state_test.rs"]
mod tests;
