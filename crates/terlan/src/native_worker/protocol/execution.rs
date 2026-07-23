//! Concurrent admission and single-owner execution for capability requests.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::time::Duration;

use crate::terlan_native_boundary::cancellation::NativeBoundaryCancellationToken;
use crate::terlan_native_boundary::capability_wire::{
    read_json_frame, validate_capability_term_budget, validate_protocol_version, write_json_frame,
    CapabilityOutcome, CapabilityRequest, CapabilityResponse, CapabilityValue,
    CAPABILITY_PROTOCOL_VERSION,
};
use crate::terlan_native_boundary::dispatch::operation_arity;
use crate::terlan_native_boundary::error::{error_for, ErrorKind};
use crate::terlan_native_boundary::metadata::{
    postgres_worker_manifest, NativeBoundaryCancellationPolicy, NativeBoundaryWorkerClass,
};
use crate::terlan_native_boundary::request::RequestId;
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;
use crate::terlan_native_boundary::worker::{NativeBoundaryWorker, NativeBoundaryWorkerReply};

use super::{next_request_count, validate_request_identity, CapabilityWorkerConfig};

const COORDINATOR_POLL_INTERVAL: Duration = Duration::from_millis(2);

/// Resource-owning executor used behind the concurrent protocol coordinator.
pub(super) trait CapabilityExecutor: Send {
    /// Executes one admitted call and observes its cooperative token.
    fn call(
        &mut self,
        call: CapabilityCall,
        cancellation: &NativeBoundaryCancellationToken,
    ) -> NativeBoundaryWorkerReply;

    /// Disposes one admitted process-owned resource.
    fn dispose(&mut self, dispose: CapabilityDispose) -> NativeBoundaryWorkerReply;
}

/// Owned call admitted by the protocol coordinator.
pub(super) struct CapabilityCall {
    /// Monotonic request identity.
    pub(super) request_id: u64,
    /// VM process that owns resulting resources.
    pub(super) owner_id: u64,
    /// Capability identity asserted by the VM request.
    pub(super) capability: String,
    /// Manifest-declared operation identity.
    pub(super) operation: String,
    /// Owned operation arguments.
    pub(super) arguments: Vec<CapabilityValue>,
}

/// Owned disposal admitted by the protocol coordinator.
pub(super) struct CapabilityDispose {
    /// Monotonic request identity.
    pub(super) request_id: u64,
    /// VM process that owns the resource.
    pub(super) owner_id: u64,
    /// Capability identity asserted by the VM request.
    pub(super) capability: String,
    /// Opaque worker-local resource handle.
    pub(super) handle: crate::terlan_native_boundary::capability_wire::CapabilityHandle,
}

/// Production executor retaining one mutable NativeBoundary resource store.
struct NativeCapabilityExecutor {
    worker: NativeBoundaryWorker,
    capabilities: Vec<String>,
    classes: Vec<NativeBoundaryWorkerClass>,
}

impl NativeCapabilityExecutor {
    /// Builds an executor from one closed startup policy.
    fn new(config: &CapabilityWorkerConfig, classes: Vec<NativeBoundaryWorkerClass>) -> Self {
        Self {
            worker: NativeBoundaryWorker::new(config.credit_limit),
            capabilities: config.capabilities.iter().cloned().collect(),
            classes,
        }
    }
}

impl CapabilityExecutor for NativeCapabilityExecutor {
    fn call(
        &mut self,
        call: CapabilityCall,
        cancellation: &NativeBoundaryCancellationToken,
    ) -> NativeBoundaryWorkerReply {
        let capabilities = self
            .capabilities
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let terms = call
            .arguments
            .into_iter()
            .map(CapabilityValue::into_term)
            .collect::<Vec<_>>();
        self.worker.call_for_process_with_policy_and_cancellation(
            RequestId {
                value: call.request_id,
            },
            call.owner_id,
            &capabilities,
            &self.classes,
            &call.operation,
            &terms,
            cancellation,
        )
    }

    fn dispose(&mut self, dispose: CapabilityDispose) -> NativeBoundaryWorkerReply {
        self.worker.dispose_for_process(
            RequestId {
                value: dispose.request_id,
            },
            dispose.owner_id,
            dispose.handle.into(),
        )
    }
}

/// Request admitted to the resource-owning executor.
enum ExecutionCommand {
    Call {
        call: CapabilityCall,
        cancellation: NativeBoundaryCancellationToken,
    },
    Dispose(CapabilityDispose),
    Shutdown,
}

/// Parsed input sent from the blocking reader to the coordinator.
enum InputEvent {
    Request(CapabilityRequest),
    End,
    Failed(String),
}

/// One protocol request currently consuming transport credit.
struct ActiveRequest {
    owner_id: u64,
    cancellation: Option<NativeBoundaryCancellationToken>,
}

/// Coordinator action requested by one validated input frame.
enum InputAction {
    Continue,
    Shutdown,
}

/// Runs the production capability executor behind bounded concurrent channels.
pub(super) fn run(
    config: CapabilityWorkerConfig,
    input: impl BufRead + Send,
    output: impl Write,
) -> Result<(), String> {
    match config.execution_profile {
        crate::terlan_native_boundary::metadata::NativeBoundaryExecutionProfile::ExternalAdapter
        | crate::terlan_native_boundary::metadata::NativeBoundaryExecutionProfile::CrashIsolated
        | crate::terlan_native_boundary::metadata::NativeBoundaryExecutionProfile::CrossBoundary => {}
    }
    let classes = config.admitted_worker_classes()?;
    let executor = NativeCapabilityExecutor::new(&config, classes);
    run_with_executor(config, input, output, executor)
}

/// Runs an injected executor for protocol and cancellation contract tests.
pub(super) fn run_with_executor(
    config: CapabilityWorkerConfig,
    mut input: impl BufRead + Send,
    mut output: impl Write,
    executor: impl CapabilityExecutor,
) -> Result<(), String> {
    let capacity = usize::try_from(config.credit_limit)
        .map_err(|_| "error[capability_worker.credit_limit]: platform capacity overflow")?;
    let (input_sender, input_receiver) = mpsc::sync_channel(capacity.saturating_add(2));
    let (command_sender, command_receiver) = mpsc::sync_channel(capacity);
    let (completion_sender, completion_receiver) = mpsc::sync_channel(capacity);

    std::thread::scope(|scope| {
        scope.spawn(|| read_requests(&mut input, config.max_payload_bytes, input_sender));
        scope.spawn(|| execute_requests(executor, command_receiver, completion_sender));
        let result = coordinate(
            &config,
            &input_receiver,
            &command_sender,
            &completion_receiver,
            &mut output,
        );
        drop(input_receiver);
        let _ = command_sender.send(ExecutionCommand::Shutdown);
        drop(command_sender);
        drop(completion_receiver);
        result
    })
}

/// Reads bounded frames independently of adapter execution.
fn read_requests(
    input: &mut impl BufRead,
    max_payload_bytes: usize,
    sender: SyncSender<InputEvent>,
) {
    loop {
        match read_json_frame(input, max_payload_bytes) {
            Ok(Some(request)) => {
                let shutdown = matches!(request, CapabilityRequest::Shutdown { .. });
                if sender.send(InputEvent::Request(request)).is_err() || shutdown {
                    return;
                }
            }
            Ok(None) => {
                let _ = sender.send(InputEvent::End);
                return;
            }
            Err(error) => {
                let _ = sender.send(InputEvent::Failed(error));
                return;
            }
        }
    }
}

/// Executes admitted requests serially so adapter resources retain one owner.
fn execute_requests(
    mut executor: impl CapabilityExecutor,
    receiver: Receiver<ExecutionCommand>,
    sender: SyncSender<NativeBoundaryWorkerReply>,
) {
    while let Ok(command) = receiver.recv() {
        let reply = match command {
            ExecutionCommand::Call { call, cancellation } => executor.call(call, &cancellation),
            ExecutionCommand::Dispose(dispose) => executor.dispose(dispose),
            ExecutionCommand::Shutdown => return,
        };
        if sender.send(reply).is_err() {
            return;
        }
    }
}

/// Coordinates admission, cancellation, credit accounting, and ordered shutdown.
fn coordinate(
    config: &CapabilityWorkerConfig,
    input: &Receiver<InputEvent>,
    commands: &SyncSender<ExecutionCommand>,
    completions: &Receiver<NativeBoundaryWorkerReply>,
    output: &mut impl Write,
) -> Result<(), String> {
    let mut active = BTreeMap::<u64, ActiveRequest>::new();
    let mut requests = 0_u64;
    let mut last_request_id = None;
    let mut closing = false;
    let mut acknowledge_shutdown = false;
    let mut terminal_error = None;

    loop {
        while let Ok(reply) = completions.try_recv() {
            if let Err(error) = complete_request(config, output, &mut active, reply) {
                terminal_error.get_or_insert(error);
                closing = true;
                cancel_active(&active);
            }
        }
        if closing && active.is_empty() {
            commands
                .send(ExecutionCommand::Shutdown)
                .map_err(|_| "error[capability_worker.executor]: executor closed")?;
            if acknowledge_shutdown {
                write_json_frame(
                    output,
                    &CapabilityResponse::ShutdownAck {
                        version: CAPABILITY_PROTOCOL_VERSION,
                    },
                    config.max_payload_bytes,
                )?;
            }
            return terminal_error.map_or(Ok(()), Err);
        }

        match input.recv_timeout(COORDINATOR_POLL_INTERVAL) {
            Ok(InputEvent::Request(request)) if !closing => {
                match handle_input_request(
                    config,
                    output,
                    commands,
                    &mut active,
                    &mut requests,
                    &mut last_request_id,
                    request,
                ) {
                    Ok(InputAction::Continue) => {}
                    Ok(InputAction::Shutdown) => {
                        closing = true;
                        acknowledge_shutdown = true;
                        cancel_active(&active);
                    }
                    Err(error) => {
                        closing = true;
                        terminal_error.get_or_insert(error);
                        cancel_active(&active);
                    }
                }
            }
            Ok(InputEvent::Request(_)) => {}
            Ok(InputEvent::End) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                closing = true;
                cancel_active(&active);
            }
            Ok(InputEvent::Failed(error)) => {
                closing = true;
                terminal_error = Some(error);
                cancel_active(&active);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

/// Validates and applies one input frame without bypassing terminal cleanup.
fn handle_input_request(
    config: &CapabilityWorkerConfig,
    output: &mut impl Write,
    commands: &SyncSender<ExecutionCommand>,
    active: &mut BTreeMap<u64, ActiveRequest>,
    requests: &mut u64,
    last_request_id: &mut Option<u64>,
    request: CapabilityRequest,
) -> Result<InputAction, String> {
    match request {
        CapabilityRequest::Call {
            version,
            request_id,
            owner_id,
            capability,
            operation,
            arguments,
        } => {
            validate_protocol_version(version)?;
            validate_request_identity(request_id, owner_id)?;
            *requests = next_request_count(*requests, config.max_requests)?;
            validate_monotonic_id(request_id, last_request_id)?;
            validate_capability_term_budget(&arguments)?;
            admit_call(
                config,
                output,
                commands,
                active,
                CapabilityCall {
                    request_id,
                    owner_id,
                    capability,
                    operation,
                    arguments,
                },
            )?;
            Ok(InputAction::Continue)
        }
        CapabilityRequest::Dispose {
            version,
            request_id,
            owner_id,
            capability,
            handle,
        } => {
            validate_protocol_version(version)?;
            validate_request_identity(request_id, owner_id)?;
            *requests = next_request_count(*requests, config.max_requests)?;
            validate_monotonic_id(request_id, last_request_id)?;
            admit_dispose(
                config,
                output,
                commands,
                active,
                CapabilityDispose {
                    request_id,
                    owner_id,
                    capability,
                    handle,
                },
            )?;
            Ok(InputAction::Continue)
        }
        CapabilityRequest::Cancel {
            version,
            request_id,
            owner_id,
        } => {
            validate_protocol_version(version)?;
            validate_request_identity(request_id, owner_id)?;
            let accepted = active.get(&request_id).is_some_and(|request| {
                request.owner_id == owner_id
                    && request.cancellation.as_ref().is_some_and(|token| {
                        token.cancel();
                        true
                    })
            });
            write_json_frame(
                output,
                &CapabilityResponse::CancelAck {
                    version: CAPABILITY_PROTOCOL_VERSION,
                    request_id,
                    accepted,
                },
                config.max_payload_bytes,
            )?;
            Ok(InputAction::Continue)
        }
        CapabilityRequest::Shutdown { version } => {
            validate_protocol_version(version)?;
            Ok(InputAction::Shutdown)
        }
    }
}

/// Admits one manifest call or emits an immediate typed rejection.
fn admit_call(
    config: &CapabilityWorkerConfig,
    output: &mut impl Write,
    commands: &SyncSender<ExecutionCommand>,
    active: &mut BTreeMap<u64, ActiveRequest>,
    call: CapabilityCall,
) -> Result<(), String> {
    let Some((required_capability, cancellation_policy)) = operation_admission(&call.operation)
    else {
        return write_rejection(
            config,
            output,
            active.len(),
            call.request_id,
            "capability_worker.operation_denied",
            "operation is not declared by this capability worker",
        );
    };
    if call.capability != required_capability {
        return write_rejection(
            config,
            output,
            active.len(),
            call.request_id,
            "capability_worker.capability_mismatch",
            "request capability does not own the declared operation",
        );
    }
    if active.len() >= usize::try_from(config.credit_limit).unwrap_or(usize::MAX) {
        let error = error_for(ErrorKind::BackpressureLimit);
        return write_rejection(
            config,
            output,
            active.len(),
            call.request_id,
            error.code,
            error.message,
        );
    }

    let cancellation = (cancellation_policy == NativeBoundaryCancellationPolicy::Cooperative)
        .then(NativeBoundaryCancellationToken::new);
    let command_token = cancellation
        .clone()
        .unwrap_or_else(NativeBoundaryCancellationToken::new);
    let request_id = call.request_id;
    let owner_id = call.owner_id;
    active.insert(
        request_id,
        ActiveRequest {
            owner_id,
            cancellation,
        },
    );
    send_command(
        commands,
        ExecutionCommand::Call {
            call,
            cancellation: command_token,
        },
        active,
        request_id,
    )
}

/// Resolves the closed operation family admitted by this worker executable.
fn operation_admission(
    operation: &str,
) -> Option<(&'static str, NativeBoundaryCancellationPolicy)> {
    if let Some(export) = postgres_worker_manifest().export_for_operation(operation) {
        return Some((export.required_capability, export.cancellation));
    }
    if operation_arity(operation).is_none() {
        return None;
    }
    if operation.starts_with("std.io.file.") {
        return Some((
            "filesystem",
            NativeBoundaryCancellationPolicy::NotCancellable,
        ));
    }
    if operation.starts_with("std.io.console.") {
        return Some(("stdio", NativeBoundaryCancellationPolicy::NotCancellable));
    }
    None
}

/// Admits one non-cancellable disposal request.
fn admit_dispose(
    config: &CapabilityWorkerConfig,
    output: &mut impl Write,
    commands: &SyncSender<ExecutionCommand>,
    active: &mut BTreeMap<u64, ActiveRequest>,
    dispose: CapabilityDispose,
) -> Result<(), String> {
    if !config.capabilities.contains(&dispose.capability) {
        return write_rejection(
            config,
            output,
            active.len(),
            dispose.request_id,
            "capability_worker.capability_denied",
            "resource capability is not granted to this worker",
        );
    }
    if active.len() >= usize::try_from(config.credit_limit).unwrap_or(usize::MAX) {
        let error = error_for(ErrorKind::BackpressureLimit);
        return write_rejection(
            config,
            output,
            active.len(),
            dispose.request_id,
            error.code,
            error.message,
        );
    }
    let request_id = dispose.request_id;
    active.insert(
        request_id,
        ActiveRequest {
            owner_id: dispose.owner_id,
            cancellation: None,
        },
    );
    send_command(
        commands,
        ExecutionCommand::Dispose(dispose),
        active,
        request_id,
    )
}

/// Sends one admitted command without allowing queue growth past credit.
fn send_command(
    commands: &SyncSender<ExecutionCommand>,
    command: ExecutionCommand,
    active: &mut BTreeMap<u64, ActiveRequest>,
    request_id: u64,
) -> Result<(), String> {
    match commands.try_send(command) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) => {
            active.remove(&request_id);
            Err("error[capability_worker.queue]: admitted executor queue is full".to_string())
        }
        Err(TrySendError::Disconnected(_)) => {
            active.remove(&request_id);
            Err("error[capability_worker.executor]: executor closed".to_string())
        }
    }
}

/// Publishes one completion with coordinator-owned credit telemetry.
fn complete_request(
    config: &CapabilityWorkerConfig,
    output: &mut impl Write,
    active: &mut BTreeMap<u64, ActiveRequest>,
    reply: NativeBoundaryWorkerReply,
) -> Result<(), String> {
    if active.remove(&reply.request_id.value).is_none() {
        return Err(format!(
            "error[capability_worker.completion]: unknown request {}",
            reply.request_id.value
        ));
    }
    let reserved = u64::try_from(active.len())
        .map_err(|_| "error[capability_worker.credit_limit]: active count overflow")?;
    let response = response_from_worker_reply(
        reply,
        reserved,
        config.credit_limit.saturating_sub(reserved),
    );
    write_json_frame(output, &response, config.max_payload_bytes)
}

/// Writes an immediate rejection without consuming transport credit.
fn write_rejection(
    config: &CapabilityWorkerConfig,
    output: &mut impl Write,
    active: usize,
    request_id: u64,
    code: &str,
    message: &str,
) -> Result<(), String> {
    let reserved = u64::try_from(active)
        .map_err(|_| "error[capability_worker.credit_limit]: active count overflow")?;
    write_json_frame(
        output,
        &CapabilityResponse::Reply {
            version: CAPABILITY_PROTOCOL_VERSION,
            request_id,
            reserved_credits: reserved,
            available_credits: config.credit_limit.saturating_sub(reserved),
            outcome: CapabilityOutcome::Error {
                code: code.to_owned(),
                message: message.to_owned(),
                offset: 0,
            },
        },
        config.max_payload_bytes,
    )
}

/// Converts one internal worker completion into transport-owned telemetry.
fn response_from_worker_reply(
    reply: NativeBoundaryWorkerReply,
    reserved_credits: u64,
    available_credits: u64,
) -> CapabilityResponse {
    let outcome = match reply.result {
        NativeBoundaryReplyTerm::Ok(value) => CapabilityOutcome::Ok {
            value: CapabilityValue::from_term(value),
        },
        NativeBoundaryReplyTerm::Error {
            code,
            message,
            offset,
        } => CapabilityOutcome::Error {
            code,
            message,
            offset,
        },
    };
    CapabilityResponse::Reply {
        version: CAPABILITY_PROTOCOL_VERSION,
        request_id: reply.request_id.value,
        reserved_credits,
        available_credits,
        outcome,
    }
}

/// Cancels every cooperative request during transport shutdown.
fn cancel_active(active: &BTreeMap<u64, ActiveRequest>) {
    for request in active.values() {
        if let Some(cancellation) = &request.cancellation {
            cancellation.cancel();
        }
    }
}

/// Enforces one monotonic identity domain before asynchronous admission.
fn validate_monotonic_id(request_id: u64, last: &mut Option<u64>) -> Result<(), String> {
    if last.is_some_and(|last| request_id <= last) {
        return Err(format!(
            "error[capability_worker.identity]: request id {request_id} is not monotonic"
        ));
    }
    *last = Some(request_id);
    Ok(())
}
