//! Owner-local wake-driven capability dispatch for protocol task actors.

use std::collections::BTreeMap;
use std::future::Future;
use std::num::NonZeroU64;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use crate::runtime::vm::capability_worker::{
    VmCapabilityRequestContext, VmCapabilityWorkerClient, VmCapabilityWorkerEventPump,
    VmCapabilityWorkerEventPumpEvent, VmCapabilityWorkerGeneration, VmCapabilityWorkerId,
    VmCapabilityWorkerIdentity, VmCapabilityWorkerParkedRequest, VmCapabilityWorkerPool,
    VmCapabilityWorkerPoolSlot,
};
use crate::runtime::vm::package_native_helper::{VmOwnedNativeDispatcher, VmPackageNativeHelpers};
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::protocol_task_executor::{
    protocol_sleep_until, with_current_protocol_resource, with_existing_current_protocol_resource,
    VmProtocolSleep,
};
use crate::runtime::vm::pure_native::{repl_value_to_boundary_term, PureNativeCapabilityWait};
use crate::runtime::vm::scheduler_topology::{VmFixedActorRoute, VmSchedulerId};
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;

use super::shard_owner::capability_dispatch::{
    capability_worker_path, capability_worker_policy, GENERATED_CAPABILITY_CREDITS,
};

struct PendingProtocolCapability {
    route: VmFixedActorRoute,
    owner: VmProcessId,
    expected: VmCapabilityRequestContext,
}

pub(super) struct ProtocolCapabilityDispatcher {
    scheduler: VmSchedulerId,
    pump: Option<VmCapabilityWorkerEventPump<PendingProtocolCapability>>,
    assignments: BTreeMap<NonZeroU64, VmCapabilityWorkerParkedRequest>,
    completed: BTreeMap<NonZeroU64, NativeBoundaryReplyTerm>,
    trusted_helpers: VmPackageNativeHelpers,
    native: VmOwnedNativeDispatcher,
    native_pending: BTreeMap<NonZeroU64, VmProcessId>,
    resource_owners: BTreeMap<NonZeroU64, VmProcessId>,
    native_wakers: BTreeMap<NonZeroU64, Waker>,
}

impl ProtocolCapabilityDispatcher {
    pub(super) fn new(scheduler: VmSchedulerId) -> Result<Self, String> {
        Ok(Self {
            scheduler,
            pump: None,
            assignments: BTreeMap::new(),
            completed: BTreeMap::new(),
            trusted_helpers: VmPackageNativeHelpers::default(),
            native: VmOwnedNativeDispatcher::default(),
            native_pending: BTreeMap::new(),
            resource_owners: BTreeMap::new(),
            native_wakers: BTreeMap::new(),
        })
    }

    fn dispatch_trusted(
        &mut self,
        owner: VmProcessId,
        wait: &PureNativeCapabilityWait,
    ) -> Result<NativeBoundaryReplyTerm, String> {
        if wait.request().capability == "package-native" {
            let value = self
                .trusted_helpers
                .call(owner.as_u64(), wait.request(), wait.admitted_atoms())
                .map_err(String::from)?;
            return repl_value_to_boundary_term(value)
                .map(NativeBoundaryReplyTerm::Ok)
                .map_err(String::from);
        }
        crate::runtime::vm::package_native_helper::dispatch_vm_capability_with_program_arguments(
            wait.request(),
            &[],
        )
        .map_err(String::from)
    }

    pub(super) fn submit(
        &mut self,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        wait: &PureNativeCapabilityWait,
    ) -> Result<(), String> {
        if self.assignments.contains_key(&route.actor_id())
            || self.native_pending.contains_key(&route.actor_id())
        {
            return Err(
                "error[serve.aot.capability_route]: protocol route already has a worker assignment"
                    .to_string(),
            );
        }
        if wait.request().operation.starts_with("std.db.postgres.")
            && !trusted_host_capability(wait)
        {
            return Err("error[serve.aot.capability_denied]: database access requires trusted host capabilities".into());
        }
        if (wait.request().operation.starts_with("std.db.postgres.")
            || crate::std_native_packages::resource_operation(&wait.request().operation).is_some())
            && self
                .native
                .submit(
                    &mut self.trusted_helpers,
                    owner,
                    wait,
                    capability_worker_path().map_err(|error| error.to_string())?,
                )
                .map_err(String::from)?
        {
            self.native_pending.insert(route.actor_id(), owner);
            self.resource_owners.insert(route.actor_id(), owner);
            return Ok(());
        }
        let expected = wait.worker_context()?;
        let request = wait.request();
        let operation = request.operation.to_string();
        let arguments = request.boundary_arguments()?.into_owned();
        let pump = self.ensure_pump()?;
        let assignment = pump
            .submit(
                owner,
                expected.clone(),
                operation,
                arguments,
                PendingProtocolCapability {
                    route,
                    owner,
                    expected,
                },
            )
            .map_err(|(error, _)| error)?;
        self.assignments.insert(route.actor_id(), assignment);
        Ok(())
    }

    fn poll(
        &mut self,
        route: VmFixedActorRoute,
        context: &Context<'_>,
    ) -> Result<Option<NativeBoundaryReplyTerm>, String> {
        if let Some(outcome) = self.completed.remove(&route.actor_id()) {
            return Ok(Some(outcome));
        }
        if let Some(owner) = self.native_pending.get(&route.actor_id()) {
            self.native.register_waker(*owner, context.waker());
            self.native_wakers
                .insert(route.actor_id(), context.waker().clone());
        }
        while let Some((owner, reply)) = self
            .native
            .poll(&mut self.trusted_helpers)
            .map_err(String::from)?
        {
            if let Some(actor) = self
                .native_pending
                .iter()
                .find_map(|(actor, candidate)| (*candidate == owner).then_some(*actor))
            {
                self.native_pending.remove(&actor);
                self.completed.insert(actor, reply);
                if let Some(waker) = self.native_wakers.remove(&actor) {
                    waker.wake();
                }
            }
        }
        if let Some(outcome) = self.completed.remove(&route.actor_id()) {
            return Ok(Some(outcome));
        }
        if self.native_pending.contains_key(&route.actor_id()) {
            return Ok(None);
        }
        let Some(pump) = self.pump.as_mut() else {
            return Err(
                "error[serve.aot.capability_pump]: protocol capability pump is missing".to_string(),
            );
        };
        pump.register_event_waker(context.waker());
        while let Some(event) = pump.poll()? {
            match event {
                VmCapabilityWorkerEventPumpEvent::Completed {
                    assignment,
                    context,
                    reply,
                    payload,
                } => {
                    self.assignments.remove(&payload.route.actor_id());
                    let outcome =
                        if assignment.owner == payload.owner && context == payload.expected {
                            reply
                        } else {
                            NativeBoundaryReplyTerm::Error {
                            code: "capability.worker_correlation".to_string(),
                            message:
                                "worker completion did not match its protocol-owned actor context"
                                    .to_string(),
                            offset: 0,
                        }
                        };
                    self.completed.insert(payload.route.actor_id(), outcome);
                }
                VmCapabilityWorkerEventPumpEvent::WorkerLost {
                    worker,
                    reason,
                    pending,
                } => {
                    for (_, payload) in pending {
                        self.assignments.remove(&payload.route.actor_id());
                        self.completed.insert(
                            payload.route.actor_id(),
                            NativeBoundaryReplyTerm::Error {
                                code: "capability.worker_lost".to_string(),
                                message: format!(
                                    "worker `{}` generation {} failed: {reason}",
                                    worker.id.as_str(),
                                    worker.generation.as_u64()
                                ),
                                offset: 0,
                            },
                        );
                    }
                }
                VmCapabilityWorkerEventPumpEvent::Ignored { .. } => {}
            }
        }
        Ok(self.completed.remove(&route.actor_id()))
    }

    pub(super) fn cancel(&mut self, route: VmFixedActorRoute) {
        self.completed.remove(&route.actor_id());
        self.native_pending.remove(&route.actor_id());
        self.native_wakers.remove(&route.actor_id());
        if let Some(owner) = self.resource_owners.remove(&route.actor_id()) {
            self.native.close_owner(&mut self.trusted_helpers, owner);
        }
        let Some(assignment) = self.assignments.remove(&route.actor_id()) else {
            return;
        };
        if let Some(pump) = self.pump.as_mut() {
            let _ = pump.cancel(&assignment);
        }
    }

    fn ensure_pump(
        &mut self,
    ) -> Result<&mut VmCapabilityWorkerEventPump<PendingProtocolCapability>, String> {
        if self.pump.is_none() {
            let policy = capability_worker_policy()?;
            let id = VmCapabilityWorkerId::new(format!("aot-protocol-{}", self.scheduler.index()))?;
            let generation =
                VmCapabilityWorkerGeneration::new(1).map_err(|error| error.to_string())?;
            let client = VmCapabilityWorkerClient::spawn(
                VmCapabilityWorkerIdentity::new(id, generation),
                policy,
            )?;
            let slot = VmCapabilityWorkerPoolSlot::new(client, GENERATED_CAPABILITY_CREDITS)?;
            self.pump = Some(VmCapabilityWorkerEventPump::new(
                VmCapabilityWorkerPool::new(vec![slot])?,
            ));
        }
        Ok(self.pump.as_mut().expect("protocol pump initialized"))
    }
}

pub(super) struct ProtocolCapabilityCompletion {
    generation: u64,
    route: VmFixedActorRoute,
    active: bool,
    local_outcome: Option<NativeBoundaryReplyTerm>,
    deadline: Option<VmProtocolSleep>,
}

impl ProtocolCapabilityCompletion {
    pub(super) fn submit(
        generation: u64,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        wait: &PureNativeCapabilityWait,
    ) -> Result<Self, String> {
        if let Some(outcome) = dispatch_trusted_capability(generation, owner, wait)? {
            return Ok(Self {
                generation,
                route,
                active: false,
                local_outcome: Some(outcome),
                deadline: None,
            });
        }
        with_current_protocol_resource(
            generation,
            ProtocolCapabilityDispatcher::new,
            |dispatcher: &mut ProtocolCapabilityDispatcher| dispatcher.submit(route, owner, wait),
        )?
        .ok_or_else(|| {
            "error[serve.aot.capability_owner]: capability call is outside a protocol owner"
                .to_string()
        })?;
        Ok(Self {
            generation,
            route,
            active: true,
            local_outcome: None,
            deadline: (wait.request().operation.starts_with("std.db.postgres.")
                || crate::std_native_packages::resource_operation(&wait.request().operation)
                    .is_some())
            .then(|| {
                protocol_sleep_until(std::time::Instant::now() + std::time::Duration::from_secs(30))
            }),
        })
    }
}

/// Executes a direct-safe trusted capability on the current protocol owner.
pub(super) fn dispatch_trusted_capability(
    generation: u64,
    owner: VmProcessId,
    wait: &PureNativeCapabilityWait,
) -> Result<Option<NativeBoundaryReplyTerm>, String> {
    if wait.request().operation.starts_with("std.db.postgres.") || !trusted_host_capability(wait) {
        return Ok(None);
    }
    with_current_protocol_resource(
        generation,
        ProtocolCapabilityDispatcher::new,
        |dispatcher: &mut ProtocolCapabilityDispatcher| dispatcher.dispatch_trusted(owner, wait),
    )?
    .map(Some)
    .ok_or_else(|| {
        "error[serve.aot.capability_owner]: trusted capability call is outside a protocol owner"
            .to_string()
    })
}

/// Allows the trusted native-service profile to use application-scoped host
/// resources. Untrusted edge workloads never enable this path and remain in
/// the sandboxed capability worker/Wasmtime boundary.
pub(super) fn trusted_host_capability(wait: &PureNativeCapabilityWait) -> bool {
    std::env::var("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES").as_deref() == Ok("1")
        && matches!(
            wait.request().capability.as_str(),
            "system.environment" | "filesystem" | "postgres" | "package-native"
        )
}

impl Future for ProtocolCapabilityCompletion {
    type Output = Result<NativeBoundaryReplyTerm, String>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(outcome) = self.local_outcome.take() {
            return Poll::Ready(Ok(outcome));
        }
        if let Some(deadline) = self.deadline.as_mut() {
            let _ = Pin::new(deadline).poll(context);
        }
        let outcome = with_existing_current_protocol_resource::<ProtocolCapabilityDispatcher, _>(
            self.generation,
            |dispatcher| dispatcher.poll(self.route, context),
        );
        match outcome {
            Ok(Some(outcome)) => {
                self.active = false;
                Poll::Ready(Ok(outcome))
            }
            Ok(None) => Poll::Pending,
            Err(error) => Poll::Ready(Err(error)),
        }
    }
}

impl Drop for ProtocolCapabilityCompletion {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let _ = with_existing_current_protocol_resource::<ProtocolCapabilityDispatcher, _>(
            self.generation,
            |dispatcher| {
                dispatcher.cancel(self.route);
                Ok(())
            },
        );
    }
}

/// Releases source capability resources when a protocol actor terminates.
pub(super) fn close_route(generation: u64, route: VmFixedActorRoute) {
    let _ = with_existing_current_protocol_resource::<ProtocolCapabilityDispatcher, _>(
        generation,
        |dispatcher| {
            dispatcher.cancel(route);
            Ok(())
        },
    );
}

#[cfg(test)]
#[path = "postgres_server_test.rs"]
mod postgres_server_test;

#[cfg(test)]
#[path = "resource_server_test.rs"]
mod resource_server_test;
