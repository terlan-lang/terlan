//! Actor-owned entry and resume lifecycle inside a persistent handler shard.

use std::sync::Arc;

use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::protocol_task_executor::{
    current_protocol_scheduler, current_protocol_task_route, VmProtocolTaskRoute,
};
use crate::runtime::vm::pure_native::{
    PureNativeCapabilityRequest, PureNativeCapabilityWait, PureNativeIoWait, PureNativeIoWake,
    PureNativeSuspension,
};
use crate::runtime::vm::scheduler_topology::VmFixedActorRoute;
use crate::runtime::vm::ReplValue;
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;

use super::shard_owner::OwnedInvocationStep;
use super::{AotHandlerGeneration, AotHandlerRuntime};

/// One observable step from a request-owned native handler invocation.
#[derive(Debug)]
pub(in crate::commands::serve) enum AotHandlerInvocationStep {
    /// Generated handler returned and released all request-owned VM state.
    Complete(ReplValue),
    /// Generated handler is parked until its exact typed VM I/O wake arrives.
    Waiting(AotHandlerInvocation),
    /// Generated handler is parked on one external capability operation.
    CapabilityWaiting(AotHandlerCapabilityInvocation),
}

/// Linear ownership of one generated handler parked on a capability worker call.
#[derive(Debug)]
pub(in crate::commands::serve) struct AotHandlerCapabilityInvocation {
    generation: Arc<AotHandlerGeneration>,
    route: VmFixedActorRoute,
    owner: VmProcessId,
    suspension: Option<PureNativeSuspension>,
    wait: Option<PureNativeCapabilityWait>,
    active_route: bool,
}

impl AotHandlerCapabilityInvocation {
    /// Returns the decoded capability operation prepared by generated code.
    pub(in crate::commands::serve) fn request(
        &self,
    ) -> Result<&PureNativeCapabilityRequest, String> {
        self.wait
            .as_ref()
            .map(PureNativeCapabilityWait::request)
            .ok_or_else(|| {
                "error[serve.aot.capability]: capability invocation is no longer active".to_string()
            })
    }

    /// Publishes one worker reply through the fixed actor owner.
    #[allow(dead_code)] // Retained as a deterministic manual completion test seam.
    pub(in crate::commands::serve) fn resume(
        mut self,
        outcome: NativeBoundaryReplyTerm,
    ) -> Result<AotHandlerInvocationStep, String> {
        let generation = Arc::clone(&self.generation);
        let suspension = self.suspension.take().ok_or_else(|| {
            "error[serve.aot.capability]: capability invocation is no longer active".to_string()
        })?;
        let wait = self.wait.take().ok_or_else(|| {
            "error[serve.aot.capability]: capability wait is no longer active".to_string()
        })?;
        self.active_route = false;
        let step = match generation
            .shard(self.route.scheduler().index())
            .and_then(|shard| {
                shard.resume_capability(self.route, self.owner, suspension, wait, outcome)
            }) {
            Ok(step) => step,
            Err(error) => {
                generation.release_actor_route(self.route.scheduler().index());
                return Err(error);
            }
        };
        materialize_step(generation, step)
    }
}

impl Drop for AotHandlerCapabilityInvocation {
    fn drop(&mut self) {
        if self.suspension.take().is_some() {
            self.wait.take();
            if let Ok(shard) = self.generation.shard(self.route.scheduler().index()) {
                shard.cancel_detached(
                    self.route,
                    self.owner,
                    "native HTTP capability invocation dropped before completion".to_string(),
                );
            }
        }
        if self.active_route {
            self.generation
                .release_actor_route(self.route.scheduler().index());
            self.active_route = false;
        }
    }
}

/// Linear ownership of one generated HTTP handler while it is parked.
#[derive(Debug)]
pub(in crate::commands::serve) struct AotHandlerInvocation {
    /// Exact admitted generation and persistent shard executing this actor.
    generation: Arc<AotHandlerGeneration>,
    /// Stable shard routing identity; live actors never migrate.
    route: VmFixedActorRoute,
    /// Exact VM actor executing the generated handler.
    owner: VmProcessId,
    /// Owned generated continuation retained outside native stack memory.
    suspension: Option<PureNativeSuspension>,
    /// Typed external wait captured before the owner returns to its event loop.
    wait: PureNativeIoWait,
    /// Exact protocol connection allowed to publish this request's completion.
    protocol_origin: Option<VmProtocolTaskRoute>,
    /// Reservation in the generation's fixed scheduler routing table.
    active_route: bool,
}

impl AotHandlerRuntime {
    /// Enters one generated handler as a new actor in the persistent shard.
    pub(in crate::commands::serve) fn begin_request_invocation(
        &self,
        module: &str,
        function: &str,
        args: Vec<ReplValue>,
    ) -> Result<AotHandlerInvocationStep, String> {
        if module != self.module {
            return Err(format!(
                "error[serve.aot.module_missing]: native handler image `{}` does not own module `{module}`",
                self.module
            ));
        }
        let generation = Arc::clone(&self.generation);
        let route = match current_protocol_scheduler() {
            Some(scheduler) => generation.route_new_actor_on(scheduler)?,
            None => generation.route_new_actor()?,
        };
        let shard_index = route.scheduler().index();
        let step = match generation.shard(shard_index).and_then(|shard| {
            shard.begin(route, format!("{module}.{function}"), args, || {
                generation.rebalance_generated_queues();
            })
        }) {
            Ok(step) => step,
            Err(error) => {
                generation.release_actor_route(shard_index);
                return Err(error);
            }
        };
        materialize_step(generation, step)
    }
}

impl AotHandlerInvocation {
    /// Returns the exact typed VM I/O wait currently owned by this request.
    pub(in crate::commands::serve) fn wait(&self) -> Result<PureNativeIoWait, String> {
        self.suspension.as_ref().ok_or_else(|| {
            "error[serve.aot.invocation]: native handler invocation is no longer active".to_string()
        })?;
        Ok(self.wait.clone())
    }

    /// Moves this parked invocation to one explicitly selected scheduler.
    #[allow(dead_code)] // Publicly hidden until explicit migration instrumentation lands.
    pub(in crate::commands::serve) fn migrate_to_scheduler(
        mut self,
        destination_index: usize,
    ) -> Result<Self, String> {
        self.suspension.as_ref().ok_or_else(|| {
            "error[serve.aot.invocation]: completed invocation cannot migrate".to_string()
        })?;
        let destination =
            self.generation
                .migrate_actor(self.route, self.owner, destination_index)?;
        let destination_owner = self.generation.shard(destination.scheduler().index())?;
        self.wait = self.wait.migrated_to(
            destination_owner.shard_identity().clone(),
            destination_owner.shard_epoch(),
        );
        self.route = destination;
        Ok(self)
    }

    /// Resumes generated code through execution-shard authority after one wake.
    pub(in crate::commands::serve) fn resume(
        mut self,
        wake: PureNativeIoWake,
    ) -> Result<AotHandlerInvocationStep, String> {
        self.validate_protocol_origin()?;
        let generation = Arc::clone(&self.generation);
        let suspension = self.suspension.take().ok_or_else(|| {
            "error[serve.aot.invocation]: native handler invocation is no longer active".to_string()
        })?;
        self.active_route = false;
        let step = match generation
            .shard(self.route.scheduler().index())
            .and_then(|shard| shard.resume(self.route, self.owner, suspension, wake))
        {
            Ok(step) => step,
            Err(error) => {
                generation.release_actor_route(self.route.scheduler().index());
                return Err(error);
            }
        };
        materialize_step(generation, step)
    }

    /// Rejects completion publication outside the connection that parked it.
    fn validate_protocol_origin(&self) -> Result<(), String> {
        let Some(expected) = self.protocol_origin else {
            return Ok(());
        };
        expected.validate_completion_origin()?;
        let actual = current_protocol_task_route().ok_or_else(|| {
            "error[vm.protocol_completion_owner]: protocol task origin is missing".to_string()
        })?;
        if actual != expected {
            return Err(format!(
                "error[vm.protocol_completion_owner]: process {} cannot complete process {} request",
                actual.process().as_u64(),
                expected.process().as_u64()
            ));
        }
        Ok(())
    }

    /// Cancels a parked request and releases only its actor-owned state.
    pub(in crate::commands::serve) fn cancel(mut self, reason: String) -> Result<(), String> {
        self.suspension.take();
        self.active_route = false;
        let result = self
            .generation
            .shard(self.route.scheduler().index())
            .and_then(|shard| shard.cancel(self.route, self.owner, reason));
        self.generation
            .release_actor_route(self.route.scheduler().index());
        result
    }
}

impl Drop for AotHandlerInvocation {
    fn drop(&mut self) {
        if self.suspension.take().is_some() {
            if let Ok(shard) = self.generation.shard(self.route.scheduler().index()) {
                shard.cancel_detached(
                    self.route,
                    self.owner,
                    "native HTTP invocation dropped before completion".to_string(),
                );
            }
        }
        if self.active_route {
            self.generation
                .release_actor_route(self.route.scheduler().index());
            self.active_route = false;
        }
    }
}

/// Converts a shard execution result into the request-owned invocation state.
fn materialize_step(
    generation: Arc<AotHandlerGeneration>,
    step: OwnedInvocationStep,
) -> Result<AotHandlerInvocationStep, String> {
    match step {
        OwnedInvocationStep::Complete { route, value } => {
            generation.release_actor_route(route.scheduler().index());
            Ok(AotHandlerInvocationStep::Complete(value))
        }
        OwnedInvocationStep::Waiting {
            route,
            owner,
            suspension,
            wait,
        } => Ok(AotHandlerInvocationStep::Waiting(AotHandlerInvocation {
            generation,
            route,
            owner,
            suspension: Some(suspension),
            wait,
            protocol_origin: current_protocol_task_route(),
            active_route: true,
        })),
        OwnedInvocationStep::CapabilityWaiting {
            route,
            owner,
            suspension,
            wait,
        } => Ok(AotHandlerInvocationStep::CapabilityWaiting(
            AotHandlerCapabilityInvocation {
                generation,
                route,
                owner,
                suspension: Some(suspension),
                wait: Some(wait),
                active_route: true,
            },
        )),
    }
}

#[cfg(test)]
#[path = "invocation_test.rs"]
mod invocation_test;

#[cfg(test)]
#[path = "invocation_protocol_test.rs"]
mod invocation_protocol_test;
