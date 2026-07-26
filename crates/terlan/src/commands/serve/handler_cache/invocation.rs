//! Actor-owned entry and resume lifecycle inside a persistent handler shard.

use std::sync::Arc;
use std::time::Instant;

use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::protocol_task_executor::{
    current_protocol_task_route, protocol_sleep_until, with_current_protocol_resource,
    with_existing_current_protocol_resource, VmProtocolTaskRoute,
};
use crate::runtime::vm::pure_native::{
    PureNativeCapabilityRequest, PureNativeCapabilityWait, PureNativeIoWait, PureNativeIoWake,
    PureNativeSuspension, PureNativeTimerWait,
};
use crate::runtime::vm::scheduler_topology::VmFixedActorRoute;
use crate::runtime::vm::ReplValue;
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;

use super::protocol_capability::ProtocolCapabilityCompletion;
use super::shard_owner::OwnedInvocationStep;
use super::{AotHandlerGeneration, AotHandlerRuntime, LocalImmediateShard};

/// Mutable execution authority retained by a parked actor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InvocationOwner {
    /// Actor state remains on the VM protocol loop that began the call.
    Protocol(VmProtocolTaskRoute),
    /// Non-protocol callers use the lazily started compatibility owner.
    Dedicated,
}

/// One observable step from a request-owned native handler invocation.
#[derive(Debug)]
pub(in crate::commands::serve) enum AotHandlerInvocationStep {
    /// Generated handler returned and released all request-owned VM state.
    Complete(ReplValue),
    /// Generated handler is parked until its exact typed VM I/O wake arrives.
    Waiting(AotHandlerInvocation),
    /// Generated handler is parked on one external capability operation.
    CapabilityWaiting(AotHandlerCapabilityInvocation),
    /// Generated handler is parked on a VM protocol-owner deadline.
    TimerWaiting(AotHandlerTimerInvocation),
}

/// Linear ownership of one generated handler parked on its protocol deadline.
#[derive(Debug)]
pub(in crate::commands::serve) struct AotHandlerTimerInvocation {
    generation: Arc<AotHandlerGeneration>,
    route: VmFixedActorRoute,
    owner: VmProcessId,
    suspension: Option<PureNativeSuspension>,
    wait: Option<PureNativeTimerWait>,
    due: Instant,
    execution_owner: InvocationOwner,
    active_route: bool,
}

impl AotHandlerTimerInvocation {
    /// Waits without blocking the owner loop, then resumes on the same task.
    pub(in crate::commands::serve) async fn resume_at_deadline(
        mut self,
    ) -> Result<AotHandlerInvocationStep, String> {
        let InvocationOwner::Protocol(expected) = self.execution_owner else {
            return Err(
                "error[serve.aot.protocol_timer]: timer invocation has no protocol owner"
                    .to_string(),
            );
        };
        protocol_sleep_until(self.due).await;
        expected.validate_completion_origin()?;
        if current_protocol_task_route() != Some(expected) {
            return Err(
                "error[vm.protocol_completion_owner]: timer completion came from a foreign protocol task"
                    .to_string(),
            );
        }
        let generation = Arc::clone(&self.generation);
        let suspension = self.suspension.take().ok_or_else(|| {
            "error[serve.aot.protocol_timer]: timer invocation is no longer active".to_string()
        })?;
        let wait = self.wait.take().ok_or_else(|| {
            "error[serve.aot.protocol_timer]: timer wait is no longer active".to_string()
        })?;
        self.active_route = false;
        let result = with_existing_current_protocol_resource::<LocalImmediateShard, _>(
            generation.identity,
            |shard| shard.resume_timer(self.route, self.owner, suspension, wait),
        );
        let step = match result {
            Ok(step) => step,
            Err(error) => {
                generation.release_actor_route(self.route.scheduler().index());
                return Err(error);
            }
        };
        materialize_step(generation, step, self.execution_owner)
    }

    /// Cancels the deadline and releases only its actor-owned state.
    pub(in crate::commands::serve) fn cancel(mut self, reason: String) -> Result<(), String> {
        self.suspension.take();
        self.wait.take();
        self.active_route = false;
        let result = match self.execution_owner {
            InvocationOwner::Protocol(_) => {
                with_existing_current_protocol_resource::<LocalImmediateShard, _>(
                    self.generation.identity,
                    |shard| shard.cancel(self.owner, reason),
                )
            }
            InvocationOwner::Dedicated => self
                .generation
                .shard(self.route.scheduler().index())
                .and_then(|shard| shard.cancel(self.route, self.owner, reason)),
        };
        self.generation
            .release_actor_route(self.route.scheduler().index());
        result
    }
}

impl Drop for AotHandlerTimerInvocation {
    fn drop(&mut self) {
        if self.suspension.take().is_some() {
            self.wait.take();
            let reason = "native HTTP timer invocation dropped before completion".to_string();
            match self.execution_owner {
                InvocationOwner::Protocol(_) => {
                    let _ = with_existing_current_protocol_resource::<LocalImmediateShard, _>(
                        self.generation.identity,
                        |shard| shard.cancel(self.owner, reason),
                    );
                }
                InvocationOwner::Dedicated => {
                    if let Ok(shard) = self.generation.shard(self.route.scheduler().index()) {
                        shard.cancel_detached(self.route, self.owner, reason);
                    }
                }
            }
        }
        if self.active_route {
            self.generation
                .release_actor_route(self.route.scheduler().index());
            self.active_route = false;
        }
    }
}

/// Linear ownership of one generated handler parked on a capability worker call.
#[derive(Debug)]
pub(in crate::commands::serve) struct AotHandlerCapabilityInvocation {
    generation: Arc<AotHandlerGeneration>,
    route: VmFixedActorRoute,
    owner: VmProcessId,
    suspension: Option<PureNativeSuspension>,
    wait: Option<PureNativeCapabilityWait>,
    execution_owner: InvocationOwner,
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

    /// Dispatches the external call without blocking its VM protocol owner.
    pub(in crate::commands::serve) async fn resume_from_worker(
        self,
    ) -> Result<AotHandlerInvocationStep, String> {
        let InvocationOwner::Protocol(expected) = self.execution_owner else {
            return Err(
                "error[serve.aot.capability_owner]: automatic worker dispatch requires a protocol owner"
                    .to_string(),
            );
        };
        expected.validate_completion_origin()?;
        let wait = self.wait.as_ref().ok_or_else(|| {
            "error[serve.aot.capability]: capability wait is no longer active".to_string()
        })?;
        let completion = ProtocolCapabilityCompletion::submit(
            self.generation.identity,
            self.route,
            self.owner,
            wait,
        )?;
        let outcome = completion.await?;
        self.resume(outcome)
    }

    /// Publishes one worker reply through the fixed actor owner.
    #[allow(dead_code)] // Retained as a deterministic manual completion test seam.
    pub(in crate::commands::serve) fn resume(
        mut self,
        outcome: NativeBoundaryReplyTerm,
    ) -> Result<AotHandlerInvocationStep, String> {
        if let InvocationOwner::Protocol(expected) = self.execution_owner {
            expected.validate_completion_origin()?;
            if current_protocol_task_route() != Some(expected) {
                return Err(
                    "error[vm.protocol_completion_owner]: capability completion came from a foreign protocol task"
                        .to_string(),
                );
            }
        }
        let generation = Arc::clone(&self.generation);
        let suspension = self.suspension.take().ok_or_else(|| {
            "error[serve.aot.capability]: capability invocation is no longer active".to_string()
        })?;
        let wait = self.wait.take().ok_or_else(|| {
            "error[serve.aot.capability]: capability wait is no longer active".to_string()
        })?;
        self.active_route = false;
        let result = match self.execution_owner {
            InvocationOwner::Protocol(_) => with_existing_current_protocol_resource::<
                LocalImmediateShard,
                _,
            >(generation.identity, |shard| {
                shard.resume_capability(self.route, self.owner, suspension, wait, outcome)
            }),
            InvocationOwner::Dedicated => generation
                .shard(self.route.scheduler().index())
                .and_then(|shard| {
                    shard.resume_capability(self.route, self.owner, suspension, wait, outcome)
                }),
        };
        let step = match result {
            Ok(step) => step,
            Err(error) => {
                generation.release_actor_route(self.route.scheduler().index());
                return Err(error);
            }
        };
        materialize_step(generation, step, self.execution_owner)
    }
}

impl Drop for AotHandlerCapabilityInvocation {
    fn drop(&mut self) {
        if self.suspension.take().is_some() {
            self.wait.take();
            let reason = "native HTTP capability invocation dropped before completion".to_string();
            match self.execution_owner {
                InvocationOwner::Protocol(_) => {
                    let _ = with_existing_current_protocol_resource::<LocalImmediateShard, _>(
                        self.generation.identity,
                        |shard| shard.cancel(self.owner, reason),
                    );
                }
                InvocationOwner::Dedicated => {
                    if let Ok(shard) = self.generation.shard(self.route.scheduler().index()) {
                        shard.cancel_detached(self.route, self.owner, reason);
                    }
                }
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
    execution_owner: InvocationOwner,
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
        let protocol_origin = current_protocol_task_route();
        let route = protocol_origin.map_or_else(
            || generation.route_new_actor(),
            |origin| generation.route_new_actor_on(origin.scheduler()),
        )?;
        let shard_index = route.scheduler().index();
        let export = format!("{module}.{function}");
        let arity = args.len();
        let execution_owner = if protocol_origin.is_some() {
            InvocationOwner::Protocol(protocol_origin.expect("checked protocol origin"))
        } else {
            InvocationOwner::Dedicated
        };
        let result = match execution_owner {
            InvocationOwner::Protocol(_) => with_current_protocol_resource(
                generation.identity,
                |scheduler| {
                    LocalImmediateShard::new(
                        generation.image.spawn_shard_on_scheduler(scheduler)?,
                        module,
                        function,
                        arity,
                    )
                },
                |shard: &mut LocalImmediateShard| shard.begin(route, export, args),
            )
            .and_then(|step| {
                step.ok_or_else(|| {
                    "error[serve.aot.protocol_owner]: protocol task lost its owner".to_string()
                })
            }),
            InvocationOwner::Dedicated => generation.shard(shard_index).and_then(|shard| {
                shard.begin(route, export, args, || {
                    generation.rebalance_generated_queues();
                })
            }),
        };
        let step = match result {
            Ok(step) => step,
            Err(error) => {
                generation.release_actor_route(shard_index);
                return Err(error);
            }
        };
        materialize_step(generation, step, execution_owner)
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
        if matches!(self.execution_owner, InvocationOwner::Protocol(_)) {
            return Err(
                "error[serve.aot.protocol_migration]: protocol-owned actors cannot migrate"
                    .to_string(),
            );
        }
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
        let result = match self.execution_owner {
            InvocationOwner::Protocol(_) => {
                with_existing_current_protocol_resource::<LocalImmediateShard, _>(
                    generation.identity,
                    |shard| shard.resume(self.route, self.owner, suspension, wake),
                )
            }
            InvocationOwner::Dedicated => generation
                .shard(self.route.scheduler().index())
                .and_then(|shard| shard.resume(self.route, self.owner, suspension, wake)),
        };
        let step = match result {
            Ok(step) => step,
            Err(error) => {
                generation.release_actor_route(self.route.scheduler().index());
                return Err(error);
            }
        };
        materialize_step(generation, step, self.execution_owner)
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
        let result = match self.execution_owner {
            InvocationOwner::Protocol(_) => {
                with_existing_current_protocol_resource::<LocalImmediateShard, _>(
                    self.generation.identity,
                    |shard| shard.cancel(self.owner, reason),
                )
            }
            InvocationOwner::Dedicated => self
                .generation
                .shard(self.route.scheduler().index())
                .and_then(|shard| shard.cancel(self.route, self.owner, reason)),
        };
        self.generation
            .release_actor_route(self.route.scheduler().index());
        result
    }
}

impl Drop for AotHandlerInvocation {
    fn drop(&mut self) {
        if self.suspension.take().is_some() {
            let reason = "native HTTP invocation dropped before completion".to_string();
            match self.execution_owner {
                InvocationOwner::Protocol(_) => {
                    let _ = with_existing_current_protocol_resource::<LocalImmediateShard, _>(
                        self.generation.identity,
                        |shard| shard.cancel(self.owner, reason),
                    );
                }
                InvocationOwner::Dedicated => {
                    if let Ok(shard) = self.generation.shard(self.route.scheduler().index()) {
                        shard.cancel_detached(self.route, self.owner, reason);
                    }
                }
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
    execution_owner: InvocationOwner,
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
            execution_owner,
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
                execution_owner,
                active_route: true,
            },
        )),
        OwnedInvocationStep::TimerWaiting {
            route,
            owner,
            suspension,
            wait,
            due,
        } => Ok(AotHandlerInvocationStep::TimerWaiting(
            AotHandlerTimerInvocation {
                generation,
                route,
                owner,
                suspension: Some(suspension),
                wait: Some(wait),
                due,
                execution_owner,
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
