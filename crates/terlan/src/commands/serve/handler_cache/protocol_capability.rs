//! Owner-local wake-driven capability dispatch for protocol task actors.

use std::collections::BTreeMap;
use std::future::Future;
use std::num::NonZeroU64;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::runtime::vm::capability_worker::{
    VmCapabilityRequestContext, VmCapabilityWorkerClient, VmCapabilityWorkerEventPump,
    VmCapabilityWorkerEventPumpEvent, VmCapabilityWorkerGeneration, VmCapabilityWorkerId,
    VmCapabilityWorkerIdentity, VmCapabilityWorkerParkedRequest, VmCapabilityWorkerPolicy,
    VmCapabilityWorkerPool, VmCapabilityWorkerPoolSlot,
};
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::protocol_task_executor::{
    with_current_protocol_resource, with_existing_current_protocol_resource,
};
use crate::runtime::vm::pure_native::PureNativeCapabilityWait;
use crate::runtime::vm::scheduler_topology::{VmFixedActorRoute, VmSchedulerId};
use crate::terlan_native_boundary::metadata::NativeBoundaryExecutionProfile;
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;

use super::shard_owner::capability_dispatch::{
    capability_worker_path, GENERATED_CAPABILITY_CREDITS,
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
}

impl ProtocolCapabilityDispatcher {
    pub(super) fn new(scheduler: VmSchedulerId) -> Result<Self, String> {
        Ok(Self {
            scheduler,
            pump: None,
            assignments: BTreeMap::new(),
            completed: BTreeMap::new(),
        })
    }

    pub(super) fn submit(
        &mut self,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        wait: &PureNativeCapabilityWait,
    ) -> Result<(), String> {
        if self.assignments.contains_key(&route.actor_id()) {
            return Err(
                "error[serve.aot.capability_route]: protocol route already has a worker assignment"
                    .to_string(),
            );
        }
        let expected = wait.worker_context()?;
        let request = wait.request();
        let operation = request.operation.to_string();
        let arguments = request.arguments.clone();
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
            let executable = capability_worker_path()?;
            let policy = VmCapabilityWorkerPolicy::new(
                executable,
                NativeBoundaryExecutionProfile::CrashIsolated,
            )?
            .allow("filesystem")
            .allow("stdio")
            .with_credit_limit(GENERATED_CAPABILITY_CREDITS)?;
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
}

impl ProtocolCapabilityCompletion {
    pub(super) fn submit(
        generation: u64,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        wait: &PureNativeCapabilityWait,
    ) -> Result<Self, String> {
        if trusted_host_capability(wait) {
            let outcome = crate::runtime::vm::package_native_helper::
                dispatch_vm_capability_with_program_arguments(wait.request(), &[])
                .map_err(String::from)?;
            return Ok(Self {
                generation,
                route,
                active: false,
                local_outcome: Some(outcome),
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
        })
    }
}

/// Allows the trusted native-service profile to use application-scoped host
/// resources. Untrusted edge workloads never enable this path and remain in
/// the sandboxed capability worker/Wasmtime boundary.
fn trusted_host_capability(wait: &PureNativeCapabilityWait) -> bool {
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
