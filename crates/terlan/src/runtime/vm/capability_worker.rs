//! VM-owned asynchronous transport for external capability workers.

#[path = "capability_worker/sandbox.rs"]
mod sandbox;
#[path = "capability_worker/parked.rs"]
mod parked;
#[path = "capability_worker/event_pump.rs"]
mod event_pump;
#[allow(dead_code)] // MC-6 pool surface is staged before its runtime consumer.
#[path = "capability_worker/pool.rs"]
mod pool;

#[allow(unused_imports)] // MC-6 pool surface is staged before its runtime consumer.
pub(crate) use pool::{
    VmCapabilityWorkerParkedRequest, VmCapabilityWorkerPool, VmCapabilityWorkerPoolRequest,
    VmCapabilityWorkerPoolSlot,
};
pub(crate) use event_pump::{
    VmCapabilityWorkerEventPump, VmCapabilityWorkerEventPumpEvent,
};

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};

use crate::runtime::vm::execution_shard_epoch::{VmShardEpochOperation, VmShardOperationKind};
use crate::runtime::vm::native_boundary::deadline::{
    VmNativeBoundaryDeadlineCompletion, VmNativeBoundaryDeadlineQueue,
    VmScheduledNativeBoundaryRequest,
};
use crate::runtime::vm::process::{VmProcessId, VmProcessTable};
use crate::runtime::vm::scheduler::VmScheduler;
use crate::runtime::vm::timer::{VmTimerEvent, VmTimerId, VmTimerTable};
use crate::terlan_native_boundary::capability_sandbox::CapabilitySandboxProfile;
use crate::terlan_native_boundary::capability_wire::{
    read_json_frame, validate_capability_term_budget, validate_protocol_version, write_json_frame,
    CapabilityHandle, CapabilityRequest, CapabilityResponse, CapabilityValue,
    CAPABILITY_PROTOCOL_VERSION,
};
use crate::terlan_native_boundary::metadata::NativeBoundaryExecutionProfile;
use crate::terlan_native_boundary::request::{next_request_id, RequestId};
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

/// Default maximum bytes admitted by one capability-worker frame.
const DEFAULT_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Default number of calls admitted during one worker lifetime.
const DEFAULT_MAX_REQUESTS: u64 = 1_024;

/// Default number of requests that may be parked concurrently.
const DEFAULT_CREDIT_LIMIT: u64 = 64;

/// Stable logical identity of one capability-worker slot.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmCapabilityWorkerId(
    /// Validated non-empty logical worker name.
    String,
);

impl VmCapabilityWorkerId {
    /// Creates a non-empty worker identity.
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(
                "error[capability_worker.identity]: worker identity must not be empty".into(),
            );
        }
        Ok(Self(value))
    }

    /// Returns the stable worker identity.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Monotonic process generation for one capability-worker slot.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmCapabilityWorkerGeneration(
    /// Validated nonzero worker process generation.
    u64,
);

impl VmCapabilityWorkerGeneration {
    /// Creates a nonzero worker generation.
    pub(crate) const fn new(value: u64) -> Result<Self, &'static str> {
        if value == 0 {
            return Err("error[capability_worker.identity]: worker generation must be nonzero");
        }
        Ok(Self(value))
    }

    /// Returns the numeric worker generation.
    pub(crate) const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Exact worker process identity used for completion and crash attribution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmCapabilityWorkerIdentity {
    /// Stable logical worker slot.
    pub(crate) id: VmCapabilityWorkerId,
    /// Current process generation within the slot.
    pub(crate) generation: VmCapabilityWorkerGeneration,
}

impl VmCapabilityWorkerIdentity {
    /// Creates one exact worker process identity.
    pub(crate) const fn new(
        id: VmCapabilityWorkerId,
        generation: VmCapabilityWorkerGeneration,
    ) -> Self {
        Self { id, generation }
    }
}

/// Explicit capability identity carried by every worker operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmCapabilityId(
    /// Validated non-empty capability name.
    String,
);

impl VmCapabilityId {
    /// Creates a non-empty capability identity.
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(
                "error[capability_worker.identity]: capability identity must not be empty".into(),
            );
        }
        Ok(Self(value))
    }

    /// Returns the stable capability name.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Epoch and capability ownership retained for one asynchronous request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmCapabilityRequestContext {
    /// Capability explicitly selected by the compiled call site.
    pub(crate) capability: VmCapabilityId,
    /// Epoch-fenced completion operation retained across asynchronous work.
    pub(crate) completion: VmShardEpochOperation,
}

impl VmCapabilityRequestContext {
    /// Creates context only for a capability-completion operation.
    pub(crate) fn new(
        capability: VmCapabilityId,
        completion: VmShardEpochOperation,
    ) -> Result<Self, String> {
        if completion.kind != VmShardOperationKind::CapabilityCompletion {
            return Err(
                "error[capability_worker.identity]: request context must use a capability-completion operation"
                    .into(),
            );
        }
        Ok(Self {
            capability,
            completion,
        })
    }
}

/// Closed process policy used when the VM starts a capability worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmCapabilityWorkerPolicy {
    /// Absolute worker executable path.
    executable: PathBuf,
    /// Explicit reason this operation crosses a process boundary.
    execution_profile: NativeBoundaryExecutionProfile,
    /// Capabilities granted to the child process.
    capabilities: Vec<String>,
    /// Scheduler classes admitted for the child process.
    worker_classes: Vec<String>,
    /// Maximum bytes in one request or response frame.
    max_payload_bytes: usize,
    /// Maximum operations accepted during the child lifetime.
    max_requests: u64,
    /// Maximum concurrently parked requests.
    credit_limit: u64,
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
    pub(crate) fn with_max_payload_bytes(mut self, maximum: usize) -> Result<Self, String> {
        if maximum == 0 {
            return Err("capability-worker payload limit must be positive".to_string());
        }
        self.max_payload_bytes = maximum;
        Ok(self)
    }

    /// Replaces the default lifetime request limit with a positive bound.
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

/// Borrowed VM tables required to park and wake capability callers.
pub(crate) struct VmCapabilityWorkerRuntime<'a> {
    /// VM timer ownership table.
    pub(crate) timers: &'a mut VmTimerTable,
    /// VM process ownership table.
    pub(crate) processes: &'a mut VmProcessTable,
    /// Scheduler used for parking charges and wakeups.
    pub(crate) scheduler: &'a mut VmScheduler,
}

/// VM-visible result of consuming one worker transport event.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmCapabilityWorkerCompletion {
    /// A live parked request completed and its owner became runnable.
    Reply {
        /// Exact worker process that produced the reply.
        worker: VmCapabilityWorkerIdentity,
        /// Request completed by the worker.
        request_id: RequestId,
        /// Capability and shard-epoch ownership of the request.
        context: VmCapabilityRequestContext,
        /// Stable adapter outcome.
        reply: NativeBoundaryReplyTerm,
    },
    /// A completion arrived after timeout, cancellation, or prior completion.
    StaleReply {
        /// Exact worker process that emitted the late reply.
        worker: VmCapabilityWorkerIdentity,
        /// Suppressed request identity.
        request_id: RequestId,
    },
    /// The worker acknowledged a cooperative cancellation frame.
    CancelAcknowledged {
        /// Exact worker process that acknowledged cancellation.
        worker: VmCapabilityWorkerIdentity,
        /// Request named by the acknowledgement.
        request_id: RequestId,
        /// Whether the worker still owned pending work for the request.
        accepted: bool,
    },
    /// The worker accepted orderly shutdown.
    ShutdownAcknowledged {
        /// Exact worker process that accepted shutdown.
        worker: VmCapabilityWorkerIdentity,
    },
    /// The response stream reached EOF and pending calls were cancelled.
    TransportClosed {
        /// Exact worker process whose response stream closed.
        worker: VmCapabilityWorkerIdentity,
        /// VM deadline completions produced while draining pending calls.
        cancelled: Vec<VmNativeBoundaryDeadlineCompletion>,
    },
    /// Worker I/O failed and pending calls were cancelled.
    TransportFailed {
        /// Exact worker process attributed with the transport failure.
        worker: VmCapabilityWorkerIdentity,
        /// Typed transport failure.
        error: String,
        /// VM deadline completions produced while draining pending calls.
        cancelled: Vec<VmNativeBoundaryDeadlineCompletion>,
    },
}

/// Terminal VM deadline result and cancellation-delivery status.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmCapabilityWorkerTerminal {
    /// Exactly-once VM deadline completion.
    pub(crate) completion: VmNativeBoundaryDeadlineCompletion,
    /// Transport error when cooperative cancellation could not be queued.
    pub(crate) cancellation_error: Option<String>,
}

/// VM-owned capability-worker client and request lifecycle state.
pub(crate) struct VmCapabilityWorkerClient {
    /// Exact logical slot and process generation.
    identity: VmCapabilityWorkerIdentity,
    /// Bounded nonblocking scheduler-to-process transport.
    transport: VmCapabilityWorkerTransport,
    /// VM-owned parking, deadline, and single-completion state.
    deadlines: VmNativeBoundaryDeadlineQueue,
    /// Capability and epoch ownership for each live request.
    pending_contexts: BTreeMap<u64, VmCapabilityRequestContext>,
    /// Generated continuations already parked by their execution shard.
    parked_contexts: BTreeMap<u64, (VmProcessId, VmCapabilityRequestContext)>,
    /// Closed capability names granted to this worker process.
    capabilities: BTreeSet<String>,
    /// Last locally allocated request identity.
    last_request_id: RequestId,
    /// Credit total attested by every worker reply.
    remote_credit_limit: u64,
}

impl VmCapabilityWorkerClient {
    /// Returns the exact logical slot and process generation of this client.
    pub(crate) fn identity(&self) -> &VmCapabilityWorkerIdentity {
        &self.identity
    }

    /// Returns the maximum number of requests admitted by this worker process.
    pub(crate) const fn credit_limit(&self) -> u64 {
        self.remote_credit_limit
    }

    /// Returns whether startup policy granted one capability to this worker.
    pub(crate) fn admits_capability(&self, capability: &VmCapabilityId) -> bool {
        self.capabilities.contains(capability.as_str())
    }

    /// Starts a capability worker with no inherited environment variables.
    pub(crate) fn spawn(
        identity: VmCapabilityWorkerIdentity,
        policy: VmCapabilityWorkerPolicy,
    ) -> Result<Self, String> {
        let sandbox_profile = CapabilitySandboxProfile::current()?;
        let sandbox_dir = sandbox::VmCapabilityWorkerSandboxDir::create()?;
        let mut command = sandbox::worker_command(
            sandbox_profile,
            &policy.executable,
            &policy.capabilities,
            sandbox_dir.path(),
        )?;
        command
            .env_clear()
            .current_dir(sandbox_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for capability in &policy.capabilities {
            command.arg("--allow").arg(capability);
        }
        for worker_class in &policy.worker_classes {
            command.arg("--worker-class").arg(worker_class);
        }
        command
            .arg("--execution-profile")
            .arg(policy.execution_profile.protocol_name())
            .arg("--sandbox-profile")
            .arg(sandbox_profile.name())
            .arg("--max-payload-bytes")
            .arg(policy.max_payload_bytes.to_string())
            .arg("--max-requests")
            .arg(policy.max_requests.to_string())
            .arg("--credit-limit")
            .arg(policy.credit_limit.to_string());
        let mut child = command.spawn().map_err(|error| {
            format!(
                "failed to start capability worker `{}`: {error}",
                policy.executable.display()
            )
        })?;
        let input = child.stdin.take().ok_or_else(|| {
            terminate_child(&mut child);
            "capability worker did not expose stdin".to_string()
        })?;
        let output = child.stdout.take().ok_or_else(|| {
            terminate_child(&mut child);
            "capability worker did not expose stdout".to_string()
        })?;
        let transport = VmCapabilityWorkerTransport::from_streams(
            input,
            output,
            policy.max_payload_bytes,
            policy.credit_limit,
            Some(child),
            Some(sandbox_dir),
        )?;
        Ok(Self {
            identity,
            transport,
            deadlines: VmNativeBoundaryDeadlineQueue::new(policy.credit_limit),
            pending_contexts: BTreeMap::new(),
            parked_contexts: BTreeMap::new(),
            capabilities: policy.capabilities.into_iter().collect(),
            last_request_id: RequestId { value: 0 },
            remote_credit_limit: policy.credit_limit,
        })
    }

    /// Starts one operation and parks its owner without blocking the scheduler.
    pub(crate) fn start_call(
        &mut self,
        runtime: &mut VmCapabilityWorkerRuntime<'_>,
        owner: VmProcessId,
        context: VmCapabilityRequestContext,
        operation: impl Into<String>,
        arguments: Vec<NativeBoundaryTerm>,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<VmScheduledNativeBoundaryRequest, String> {
        self.require_capability(&context.capability)?;
        let arguments = arguments
            .into_iter()
            .map(CapabilityValue::from_term)
            .collect::<Vec<_>>();
        validate_capability_term_budget(&arguments)?;
        let request_id = self.allocate_request_id()?;
        self.start_request(
            runtime,
            owner,
            request_id,
            context.clone(),
            now_tick,
            timeout_ticks,
            CapabilityRequest::Call {
                version: CAPABILITY_PROTOCOL_VERSION,
                request_id: request_id.value,
                owner_id: owner.as_u64(),
                capability: context.capability.as_str().to_string(),
                operation: operation.into(),
                arguments,
            },
        )
    }

    /// Starts disposal of one process-owned external resource.
    pub(crate) fn start_dispose(
        &mut self,
        runtime: &mut VmCapabilityWorkerRuntime<'_>,
        owner: VmProcessId,
        context: VmCapabilityRequestContext,
        handle: CapabilityHandle,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<VmScheduledNativeBoundaryRequest, String> {
        self.require_capability(&context.capability)?;
        let request_id = self.allocate_request_id()?;
        self.start_request(
            runtime,
            owner,
            request_id,
            context.clone(),
            now_tick,
            timeout_ticks,
            CapabilityRequest::Dispose {
                version: CAPABILITY_PROTOCOL_VERSION,
                request_id: request_id.value,
                owner_id: owner.as_u64(),
                capability: context.capability.as_str().to_string(),
                handle,
            },
        )
    }

    /// Polls at most one transport event and applies live replies to the VM.
    pub(crate) fn poll(
        &mut self,
        runtime: &mut VmCapabilityWorkerRuntime<'_>,
    ) -> Result<Option<VmCapabilityWorkerCompletion>, String> {
        let Some(event) = self.transport.try_event()? else {
            return Ok(None);
        };
        match event {
            VmCapabilityWorkerTransportEvent::Response(response) => {
                match self.apply_response(runtime, response) {
                    Ok(completion) => Ok(Some(completion)),
                    Err(error) => {
                        self.transport.close();
                        let cancelled = self.cancel_all_pending(runtime)?;
                        Ok(Some(VmCapabilityWorkerCompletion::TransportFailed {
                            worker: self.identity.clone(),
                            error,
                            cancelled,
                        }))
                    }
                }
            }
            VmCapabilityWorkerTransportEvent::Closed => {
                self.transport.close();
                let cancelled = self.cancel_all_pending(runtime)?;
                Ok(Some(VmCapabilityWorkerCompletion::TransportClosed {
                    worker: self.identity.clone(),
                    cancelled,
                }))
            }
            VmCapabilityWorkerTransportEvent::Failed(error) => {
                self.transport.close();
                let cancelled = self.cancel_all_pending(runtime)?;
                Ok(Some(VmCapabilityWorkerCompletion::TransportFailed {
                    worker: self.identity.clone(),
                    error,
                    cancelled,
                }))
            }
        }
    }

    /// Cancels one parked call and delivers cooperative cancellation.
    pub(crate) fn cancel(
        &mut self,
        runtime: &mut VmCapabilityWorkerRuntime<'_>,
        timer_id: VmTimerId,
    ) -> Result<VmCapabilityWorkerTerminal, String> {
        let identity = self.deadlines.request_for_timer(timer_id);
        let completion = self.deadlines.cancel(
            runtime.timers,
            runtime.processes,
            runtime.scheduler,
            timer_id,
        )?;
        Ok(self.terminal_with_cancellation(identity, completion))
    }

    /// Applies timeout or owner-exit events and delivers worker cancellation.
    pub(crate) fn handle_timer_event(
        &mut self,
        runtime: &mut VmCapabilityWorkerRuntime<'_>,
        event: &VmTimerEvent,
    ) -> Result<Option<VmCapabilityWorkerTerminal>, String> {
        let identity = self.deadlines.request_for_timer(event.timer_id());
        let Some(completion) =
            self.deadlines
                .handle_timer_event(runtime.processes, runtime.scheduler, event)?
        else {
            return Ok(None);
        };
        Ok(Some(self.terminal_with_cancellation(identity, completion)))
    }

    /// Queues an orderly worker shutdown without waiting on process I/O.
    pub(crate) fn shutdown(&self) -> Result<(), String> {
        self.transport.try_send(CapabilityRequest::Shutdown {
            version: CAPABILITY_PROTOCOL_VERSION,
        })
    }

    /// Returns the number of VM requests currently parked on this worker.
    pub(crate) fn pending_len(&self) -> usize {
        self.deadlines.pending_len() + self.parked_contexts.len()
    }

    /// Starts VM deadline ownership before publishing the request to transport.
    fn start_request(
        &mut self,
        runtime: &mut VmCapabilityWorkerRuntime<'_>,
        owner: VmProcessId,
        request_id: RequestId,
        context: VmCapabilityRequestContext,
        now_tick: u64,
        timeout_ticks: u64,
        request: CapabilityRequest,
    ) -> Result<VmScheduledNativeBoundaryRequest, String> {
        let scheduled = self.deadlines.start(
            runtime.timers,
            runtime.processes,
            runtime.scheduler,
            owner,
            request_id,
            now_tick,
            timeout_ticks,
        )?;
        self.pending_contexts.insert(request_id.value, context);
        if let Err(error) = self.transport.try_send(request) {
            self.pending_contexts.remove(&request_id.value);
            let _ = self.deadlines.cancel(
                runtime.timers,
                runtime.processes,
                runtime.scheduler,
                scheduled.timer_id,
            );
            return Err(error);
        }
        Ok(scheduled)
    }

    /// Allocates a monotonic nonzero request identity without wrapping.
    fn allocate_request_id(&mut self) -> Result<RequestId, String> {
        let next = next_request_id(self.last_request_id)
            .ok_or_else(|| "capability-worker request identity exhausted".to_string())?;
        self.last_request_id = next;
        Ok(next)
    }

    /// Requires capability admission before request identity or actor parking.
    fn require_capability(&self, capability: &VmCapabilityId) -> Result<(), String> {
        if self.capabilities.contains(capability.as_str()) {
            Ok(())
        } else {
            Err(format!(
                "error[capability_worker.capability_denied]: capability `{}` is not granted to worker `{}`",
                capability.as_str(),
                self.identity.id.as_str()
            ))
        }
    }

    /// Correlates one typed response with the VM-owned deadline queue.
    fn apply_response(
        &mut self,
        runtime: &mut VmCapabilityWorkerRuntime<'_>,
        response: CapabilityResponse,
    ) -> Result<VmCapabilityWorkerCompletion, String> {
        match response {
            CapabilityResponse::Reply {
                version,
                request_id,
                reserved_credits,
                available_credits,
                outcome,
            } => {
                validate_protocol_version(version)?;
                self.validate_remote_credits(reserved_credits, available_credits)?;
                let request_id = RequestId { value: request_id };
                let Some(timer_id) = self.deadlines.timer_for_request(request_id) else {
                    return Ok(VmCapabilityWorkerCompletion::StaleReply {
                        worker: self.identity.clone(),
                        request_id,
                    });
                };
                let context = self
                    .pending_contexts
                    .get(&request_id.value)
                    .cloned()
                    .ok_or_else(|| {
                        format!(
                            "error[capability_worker.identity]: request {} has no capability context",
                            request_id.value
                        )
                    })?;
                self.deadlines.complete(
                    runtime.timers,
                    runtime.processes,
                    runtime.scheduler,
                    timer_id,
                )?;
                self.pending_contexts.remove(&request_id.value);
                Ok(VmCapabilityWorkerCompletion::Reply {
                    worker: self.identity.clone(),
                    request_id,
                    context,
                    reply: outcome.into_reply(),
                })
            }
            CapabilityResponse::CancelAck {
                version,
                request_id,
                accepted,
            } => {
                validate_protocol_version(version)?;
                Ok(VmCapabilityWorkerCompletion::CancelAcknowledged {
                    worker: self.identity.clone(),
                    request_id: RequestId { value: request_id },
                    accepted,
                })
            }
            CapabilityResponse::ShutdownAck { version } => {
                validate_protocol_version(version)?;
                Ok(VmCapabilityWorkerCompletion::ShutdownAcknowledged {
                    worker: self.identity.clone(),
                })
            }
        }
    }

    /// Checks worker credit telemetry for overflow or protocol drift.
    fn validate_remote_credits(&self, reserved: u64, available: u64) -> Result<(), String> {
        let observed = reserved.checked_add(available).ok_or_else(|| {
            "error[capability_worker.credit]: remote credit accounting overflow".to_string()
        })?;
        if observed != self.remote_credit_limit {
            return Err(format!(
                "error[capability_worker.credit]: expected {}, received {observed}",
                self.remote_credit_limit
            ));
        }
        Ok(())
    }

    /// Publishes cancellation after the VM has made the request terminal.
    fn terminal_with_cancellation(
        &mut self,
        identity: Option<(VmProcessId, RequestId)>,
        completion: VmNativeBoundaryDeadlineCompletion,
    ) -> VmCapabilityWorkerTerminal {
        let cancellation_error = identity.and_then(|(owner, request_id)| {
            self.pending_contexts.remove(&request_id.value);
            self.transport
                .try_send(CapabilityRequest::Cancel {
                    version: CAPABILITY_PROTOCOL_VERSION,
                    request_id: request_id.value,
                    owner_id: owner.as_u64(),
                })
                .err()
        });
        VmCapabilityWorkerTerminal {
            completion,
            cancellation_error,
        }
    }

    /// Cancels every parked request after terminal transport failure.
    fn cancel_all_pending(
        &mut self,
        runtime: &mut VmCapabilityWorkerRuntime<'_>,
    ) -> Result<Vec<VmNativeBoundaryDeadlineCompletion>, String> {
        let mut completions = Vec::new();
        for timer_id in self.deadlines.pending_timer_ids() {
            if let Some((_, request_id)) = self.deadlines.request_for_timer(timer_id) {
                self.pending_contexts.remove(&request_id.value);
            }
            completions.push(self.deadlines.cancel(
                runtime.timers,
                runtime.processes,
                runtime.scheduler,
                timer_id,
            )?);
        }
        Ok(completions)
    }
}

/// Internal event emitted by background capability-worker I/O.
enum VmCapabilityWorkerTransportEvent {
    /// Decoded response frame.
    Response(CapabilityResponse),
    /// Worker response stream reached EOF.
    Closed,
    /// Reader or writer failed.
    Failed(String),
}

/// Bounded process I/O transport isolated from VM scheduler threads.
struct VmCapabilityWorkerTransport {
    /// Bounded request queue sender, removed when transport closes.
    requests: Option<SyncSender<CapabilityRequest>>,
    /// Bounded response queue receiver, removed before joining I/O threads.
    events: Option<Receiver<VmCapabilityWorkerTransportEvent>>,
    /// Attached sandboxed child process when this is a production transport.
    child: Option<Child>,
    /// Temporary sandbox directory retained for the child lifetime.
    _sandbox_dir: Option<sandbox::VmCapabilityWorkerSandboxDir>,
    /// Request serialization thread.
    writer: Option<JoinHandle<()>>,
    /// Response decoding thread.
    reader: Option<JoinHandle<()>>,
}

impl VmCapabilityWorkerTransport {
    /// Starts bounded reader and writer loops around owned streams.
    fn from_streams(
        input: impl Write + Send + 'static,
        output: impl Read + Send + 'static,
        max_payload_bytes: usize,
        credit_limit: u64,
        child: Option<Child>,
        sandbox_dir: Option<sandbox::VmCapabilityWorkerSandboxDir>,
    ) -> Result<Self, String> {
        let queue_limit = credit_limit
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| "capability-worker transport queue limit overflow".to_string())?;
        let (request_tx, request_rx) = mpsc::sync_channel(queue_limit);
        let (event_tx, event_rx) = mpsc::sync_channel(queue_limit);
        let writer_events = event_tx.clone();
        let writer = thread::Builder::new()
            .name("terlan-capability-writer".to_string())
            .spawn(move || run_writer(input, request_rx, writer_events, max_payload_bytes))
            .map_err(|error| format!("failed to start capability-worker writer: {error}"))?;
        let reader = match thread::Builder::new()
            .name("terlan-capability-reader".to_string())
            .spawn(move || run_reader(output, event_tx, max_payload_bytes))
        {
            Ok(reader) => reader,
            Err(error) => {
                drop(request_tx);
                let _ = writer.join();
                return Err(format!("failed to start capability-worker reader: {error}"));
            }
        };
        Ok(Self {
            requests: Some(request_tx),
            events: Some(event_rx),
            child,
            _sandbox_dir: sandbox_dir,
            writer: Some(writer),
            reader: Some(reader),
        })
    }

    /// Queues one bounded request without waiting for child-process I/O.
    fn try_send(&self, request: CapabilityRequest) -> Result<(), String> {
        let sender = self.requests.as_ref().ok_or_else(|| {
            "error[capability_worker.transport]: request transport is closed".to_string()
        })?;
        sender.try_send(request).map_err(|error| match error {
            TrySendError::Full(_) => {
                "error[capability_worker.backpressure]: transport queue is full".to_string()
            }
            TrySendError::Disconnected(_) => {
                "error[capability_worker.transport]: request transport disconnected".to_string()
            }
        })
    }

    /// Receives at most one worker event without blocking the VM scheduler.
    fn try_event(&self) -> Result<Option<VmCapabilityWorkerTransportEvent>, String> {
        let events = self.events.as_ref().ok_or_else(|| {
            "error[capability_worker.transport]: response transport is closed".to_string()
        })?;
        match events.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(
                "error[capability_worker.transport]: response transport disconnected".to_string(),
            ),
        }
    }

    /// Prevents new requests and terminates an attached child process.
    fn close(&mut self) {
        self.requests.take();
        if let Some(child) = self.child.as_mut() {
            terminate_child(child);
        }
    }
}

impl Drop for VmCapabilityWorkerTransport {
    /// Closes queues, terminates the child, and joins both I/O loops.
    fn drop(&mut self) {
        self.close();
        self.events.take();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

/// Serializes queued requests outside VM scheduler threads.
fn run_writer(
    mut input: impl Write,
    requests: Receiver<CapabilityRequest>,
    events: SyncSender<VmCapabilityWorkerTransportEvent>,
    max_payload_bytes: usize,
) {
    while let Ok(request) = requests.recv() {
        if let Err(error) = write_json_frame(&mut input, &request, max_payload_bytes) {
            let _ = events.send(VmCapabilityWorkerTransportEvent::Failed(error));
            break;
        }
    }
}

/// Decodes worker replies outside VM scheduler threads.
fn run_reader(
    output: impl Read,
    events: SyncSender<VmCapabilityWorkerTransportEvent>,
    max_payload_bytes: usize,
) {
    let mut output = BufReader::new(output);
    loop {
        match read_json_frame(&mut output, max_payload_bytes) {
            Ok(Some(response)) => {
                if events
                    .send(VmCapabilityWorkerTransportEvent::Response(response))
                    .is_err()
                {
                    break;
                }
            }
            Ok(None) => {
                let _ = events.send(VmCapabilityWorkerTransportEvent::Closed);
                break;
            }
            Err(error) => {
                let _ = events.send(VmCapabilityWorkerTransportEvent::Failed(error));
                break;
            }
        }
    }
}

/// Terminates and reaps one child without panicking during cleanup.
fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
#[path = "capability_worker_test.rs"]
mod capability_worker_test;
