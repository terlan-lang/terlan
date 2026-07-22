//! Request/credit-aware NativeBoundary worker core.
//!
//! This module composes the stable term runtime with the pure request and
//! credit helpers. It is still transport-neutral: VM, thread, NIF, or process
//! bridges can wrap this worker without changing the request lifecycle,
//! backpressure, resource ownership, or reply shape.

use std::collections::{BTreeMap, VecDeque};

use serde::Serialize;

use crate::terlan_native_boundary::cancellation::NativeBoundaryCancellationToken;
use crate::terlan_native_boundary::credit::{normalize_limit, release_credit, reserve_credit};
use crate::terlan_native_boundary::error::{error_for, ErrorKind};
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;
use crate::terlan_native_boundary::request::{
    cancel_request, complete_request, start_request, timeout_request, RequestId, RequestState,
};
use crate::terlan_native_boundary::runtime::NativeBoundaryRuntime;
use crate::terlan_native_boundary::term::{
    NativeBoundaryCommandTerm, NativeBoundaryReplyTerm, NativeBoundaryTerm,
};

const EVENT_HISTORY_LIMIT: usize = 1_024;

/// Stable NativeBoundary request lifecycle outcome exposed to VM inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeBoundaryWorkerOutcome {
    /// Worker accepted the request and reserved one credit.
    Accepted,
    /// Worker completed the request and released its credit.
    Completed,
    /// VM cancellation won before request completion.
    Cancelled,
    /// VM timeout won before request completion.
    TimedOut,
    /// Worker rejected an unknown, stale, duplicate, or non-monotonic id.
    RejectedInvalidRequest,
    /// Worker rejected a request because no credit was available.
    RejectedBackpressure,
}

/// Bounded NativeBoundary request event retained for diagnostics and reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeBoundaryWorkerEvent {
    /// Stable request id supplied at the worker boundary.
    pub request_id: u64,
    /// Lifecycle outcome observed for this event.
    pub outcome: NativeBoundaryWorkerOutcome,
    /// Credits reserved immediately after the event.
    pub reserved_credits: u64,
    /// Credits available immediately after the event.
    pub available_credits: u64,
}

/// Stable reply envelope returned by the NativeBoundary worker contract.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeBoundaryWorkerReply {
    /// Request id associated with this reply.
    pub request_id: RequestId,
    /// Operation result encoded in stable term form.
    pub result: NativeBoundaryReplyTerm,
    /// Credits currently reserved by in-flight requests.
    pub reserved_credits: u64,
    /// Credits still available inside the normalized credit limit.
    pub available_credits: u64,
}

/// Transport-neutral NativeBoundary worker state.
#[derive(Debug)]
pub struct NativeBoundaryWorker {
    runtime: NativeBoundaryRuntime,
    credit_limit: u64,
    reserved_credits: u64,
    last_started_request_id: Option<u64>,
    requests: BTreeMap<u64, RequestState>,
    events: VecDeque<NativeBoundaryWorkerEvent>,
}

impl NativeBoundaryWorker {
    /// Builds a NativeBoundary worker with an empty runtime and credit window.
    ///
    /// Inputs:
    /// - `credit_limit`: maximum number of in-flight requests accepted by this
    ///   worker; zero is normalized to one.
    ///
    /// Output:
    /// - Worker with no resources, no in-flight requests, and normalized
    ///   backpressure state.
    ///
    /// Transformation:
    /// - Initializes the shared runtime and stores the caller-provided limit in
    ///   normalized form so later accounting never sees an unusable zero limit.
    pub fn new(credit_limit: u64) -> Self {
        Self {
            runtime: NativeBoundaryRuntime::new(),
            credit_limit: normalize_limit(credit_limit),
            reserved_credits: 0,
            last_started_request_id: None,
            requests: BTreeMap::new(),
            events: VecDeque::new(),
        }
    }

    /// Returns the normalized credit limit.
    ///
    /// Inputs:
    /// - The worker state.
    ///
    /// Output:
    /// - Maximum in-flight request count accepted by this worker.
    ///
    /// Transformation:
    /// - Reads the already-normalized credit limit without mutating state.
    pub fn credit_limit(&self) -> u64 {
        self.credit_limit
    }

    /// Returns the current number of reserved credits.
    ///
    /// Inputs:
    /// - The worker state.
    ///
    /// Output:
    /// - Number of in-flight requests that have reserved a credit.
    ///
    /// Transformation:
    /// - Reads accounting state without mutating worker ownership.
    pub fn reserved_credits(&self) -> u64 {
        self.reserved_credits
    }

    /// Returns credits still available for new requests.
    ///
    /// Inputs:
    /// - The worker state.
    ///
    /// Output:
    /// - Remaining request credits inside the normalized limit.
    ///
    /// Transformation:
    /// - Subtracts reserved credits from the normalized limit; malformed
    ///   internal state is clamped to zero instead of wrapping.
    pub fn available_credits(&self) -> u64 {
        self.credit_limit.saturating_sub(self.reserved_credits)
    }

    /// Returns bounded lifecycle events in oldest-to-newest order.
    pub fn events(&self) -> impl Iterator<Item = &NativeBoundaryWorkerEvent> {
        self.events.iter()
    }

    /// Writes the worker's observable NativeBoundary lifecycle report.
    pub fn write_report(&self, path: &std::path::Path) -> Result<(), String> {
        let resource_events = self.runtime.resource_events().collect::<Vec<_>>();
        let dispatch_events = self.runtime.dispatch_events().collect::<Vec<_>>();
        crate::terlan_native_boundary::worker_report::write_worker_report(
            path,
            self.credit_limit,
            self.reserved_credits,
            self.available_credits(),
            self.last_started_request_id,
            EVENT_HISTORY_LIMIT,
            &self.events,
            &resource_events,
            &dispatch_events,
        )
    }

    /// Starts tracking an in-flight request.
    ///
    /// Inputs:
    /// - `request_id`: request id supplied by the bridge caller.
    ///
    /// Output:
    /// - `Ok(())` when the request reserves a credit and enters pending state.
    /// - `Err(reply)` with stable error fields when the id is duplicated,
    ///   stale, non-monotonic, or the worker is out of credits.
    ///
    /// Transformation:
    /// - Rejects ids at or below the last accepted id so a late terminal event
    ///   cannot target a newer request through id reuse. It then reserves one
    ///   credit, creates a pending lifecycle state, and stores it by id.
    pub fn begin_request(&mut self, request_id: RequestId) -> Result<(), NativeBoundaryReplyTerm> {
        if self.requests.contains_key(&request_id.value)
            || self
                .last_started_request_id
                .is_some_and(|last| request_id.value <= last)
        {
            self.record_event(
                request_id,
                NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
            );
            return Err(worker_error_reply(ErrorKind::InvalidRequest));
        }

        let Some(next_reserved) = reserve_credit(self.reserved_credits, 1, self.credit_limit)
        else {
            self.record_event(
                request_id,
                NativeBoundaryWorkerOutcome::RejectedBackpressure,
            );
            return Err(worker_error_reply(ErrorKind::BackpressureLimit));
        };

        let Some(state) = start_request(RequestState::Idle, request_id) else {
            self.record_event(
                request_id,
                NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
            );
            return Err(worker_error_reply(ErrorKind::InvalidRequest));
        };

        self.reserved_credits = next_reserved;
        self.last_started_request_id = Some(request_id.value);
        self.requests.insert(request_id.value, state);
        self.record_event(request_id, NativeBoundaryWorkerOutcome::Accepted);
        Ok(())
    }

    /// Finishes tracking an in-flight request.
    ///
    /// Inputs:
    /// - `request_id`: request id supplied by a completed bridge reply.
    ///
    /// Output:
    /// - `Ok(())` when the request existed, matched, and released one credit.
    /// - `Err(reply)` with stable error fields when the request id is unknown
    ///   or credit accounting would underflow.
    ///
    /// Transformation:
    /// - Removes the pending lifecycle state, completes it with the matching
    ///   request id, and releases one reserved credit.
    pub fn finish_request(&mut self, request_id: RequestId) -> Result<(), NativeBoundaryReplyTerm> {
        let Some(state) = self.requests.get(&request_id.value).copied() else {
            self.record_event(
                request_id,
                NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
            );
            return Err(worker_error_reply(ErrorKind::InvalidRequest));
        };

        if complete_request(state, request_id).is_none() {
            self.record_event(
                request_id,
                NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
            );
            return Err(worker_error_reply(ErrorKind::InvalidRequest));
        }

        let Some(next_reserved) = release_credit(self.reserved_credits, 1) else {
            self.record_event(
                request_id,
                NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
            );
            return Err(worker_error_reply(ErrorKind::InvalidRequest));
        };

        self.reserved_credits = next_reserved;
        self.requests.remove(&request_id.value);
        self.record_event(request_id, NativeBoundaryWorkerOutcome::Completed);
        Ok(())
    }

    /// Cancels an in-flight request and releases its reserved credit.
    ///
    /// Inputs:
    /// - `request_id`: pending request id to cancel.
    ///
    /// Output:
    /// - Stable cancellation error reply when the request was pending and is
    ///   now terminal-cancelled.
    /// - Stable invalid-request error reply when the request is unknown,
    ///   already terminal, or credit accounting would underflow.
    ///
    /// Transformation:
    /// - Validates the matching pending lifecycle transition, releases one
    ///   reserved credit, and removes the request. The monotonic id watermark
    ///   prevents late replies or id reuse from reviving it.
    pub fn cancel_request(&mut self, request_id: RequestId) -> NativeBoundaryReplyTerm {
        let Some(state) = self.requests.get(&request_id.value).copied() else {
            self.record_event(
                request_id,
                NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
            );
            return worker_error_reply(ErrorKind::InvalidRequest);
        };
        let Some(_cancelled) = cancel_request(state, request_id) else {
            self.record_event(
                request_id,
                NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
            );
            return worker_error_reply(ErrorKind::InvalidRequest);
        };
        self.finish_terminal_request(
            request_id,
            ErrorKind::Cancelled,
            NativeBoundaryWorkerOutcome::Cancelled,
        )
    }

    /// Times out an in-flight request and releases its reserved credit.
    ///
    /// Inputs:
    /// - `request_id`: pending request id to time out.
    ///
    /// Output:
    /// - Stable timeout error reply when the request was pending and is now
    ///   terminal-timed-out.
    /// - Stable invalid-request error reply when the request is unknown,
    ///   already terminal, or credit accounting would underflow.
    ///
    /// Transformation:
    /// - Validates the matching pending lifecycle transition, releases one
    ///   reserved credit, and removes the request. The monotonic id watermark
    ///   prevents late replies or id reuse from reviving it.
    pub fn timeout_request(&mut self, request_id: RequestId) -> NativeBoundaryReplyTerm {
        let Some(state) = self.requests.get(&request_id.value).copied() else {
            self.record_event(
                request_id,
                NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
            );
            return worker_error_reply(ErrorKind::InvalidRequest);
        };
        let Some(_timed_out) = timeout_request(state, request_id) else {
            self.record_event(
                request_id,
                NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
            );
            return worker_error_reply(ErrorKind::InvalidRequest);
        };
        self.finish_terminal_request(
            request_id,
            ErrorKind::Timeout,
            NativeBoundaryWorkerOutcome::TimedOut,
        )
    }

    /// Releases one terminal request and its reserved credit.
    fn finish_terminal_request(
        &mut self,
        request_id: RequestId,
        error_kind: ErrorKind,
        outcome: NativeBoundaryWorkerOutcome,
    ) -> NativeBoundaryReplyTerm {
        let Some(next_reserved) = release_credit(self.reserved_credits, 1) else {
            self.record_event(
                request_id,
                NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
            );
            return worker_error_reply(ErrorKind::InvalidRequest);
        };

        self.reserved_credits = next_reserved;
        self.requests.remove(&request_id.value);
        self.record_event(request_id, outcome);
        worker_error_reply(error_kind)
    }

    fn record_event(&mut self, request_id: RequestId, outcome: NativeBoundaryWorkerOutcome) {
        if self.events.len() == EVENT_HISTORY_LIMIT {
            self.events.pop_front();
        }
        self.events.push_back(NativeBoundaryWorkerEvent {
            request_id: request_id.value,
            outcome,
            reserved_credits: self.reserved_credits,
            available_credits: self.available_credits(),
        });
    }

    /// Calls one NativeBoundary operation through request and credit accounting.
    ///
    /// Inputs:
    /// - `request_id`: request id supplied by the bridge caller.
    /// - `operation`: compiler-native operation id.
    /// - `args`: stable bridge terms supplied by the caller.
    ///
    /// Output:
    /// - Worker reply containing the request id, operation result, and current
    ///   credit counters.
    ///
    /// Transformation:
    /// - Begins request accounting, executes the term runtime, finishes request
    ///   accounting, and wraps the resulting term reply in a worker envelope.
    pub fn call(
        &mut self,
        request_id: RequestId,
        operation: &str,
        args: &[NativeBoundaryTerm],
    ) -> NativeBoundaryWorkerReply {
        if let Err(error) = self.begin_request(request_id) {
            return self.reply(request_id, error);
        }

        let result = self.runtime.call(operation, args);
        match self.finish_request(request_id) {
            Ok(()) => self.reply(request_id, result),
            Err(error) => self.reply(request_id, error),
        }
    }

    /// Calls one operation with explicit process, capability, and worker-class policy.
    ///
    /// Inputs:
    /// - `request_id`: monotonic request identity assigned by the VM.
    /// - `owner_process_id`: VM process that owns produced resources.
    /// - `granted_capabilities`: closed capability allowlist for this worker.
    /// - `admitted_worker_classes`: scheduler classes admitted for this worker.
    /// - `operation`: compiler-native operation identity.
    /// - `args`: stable owned request terms.
    ///
    /// Output:
    /// - A bounded worker reply after policy-aware dispatch.
    ///
    /// Transformation:
    /// - Reuses request-credit accounting while routing dispatch through the
    ///   manifest capability and scheduler admission boundary.
    pub fn call_for_process_with_policy(
        &mut self,
        request_id: RequestId,
        owner_process_id: u64,
        granted_capabilities: &[&str],
        admitted_worker_classes: &[crate::terlan_native_boundary::metadata::NativeBoundaryWorkerClass],
        operation: &str,
        args: &[NativeBoundaryTerm],
    ) -> NativeBoundaryWorkerReply {
        if let Err(error) = self.begin_request(request_id) {
            return self.reply(request_id, error);
        }

        let result = self.runtime.call_for_process_with_policy(
            owner_process_id,
            granted_capabilities,
            admitted_worker_classes,
            operation,
            args,
        );
        match self.finish_request(request_id) {
            Ok(()) => self.reply(request_id, result),
            Err(error) => self.reply(request_id, error),
        }
    }

    /// Calls one admitted operation with a request-scoped cancellation token.
    pub fn call_for_process_with_policy_and_cancellation(
        &mut self,
        request_id: RequestId,
        owner_process_id: u64,
        granted_capabilities: &[&str],
        admitted_worker_classes: &[crate::terlan_native_boundary::metadata::NativeBoundaryWorkerClass],
        operation: &str,
        args: &[NativeBoundaryTerm],
        cancellation: &NativeBoundaryCancellationToken,
    ) -> NativeBoundaryWorkerReply {
        if let Err(error) = self.begin_request(request_id) {
            return self.reply(request_id, error);
        }
        if cancellation.is_cancelled() {
            let result = self.cancel_request(request_id);
            return self.reply(request_id, result);
        }

        let result = self.runtime.call_for_process_with_policy_and_cancellation(
            owner_process_id,
            granted_capabilities,
            admitted_worker_classes,
            operation,
            args,
            cancellation,
        );
        if cancellation.is_cancelled() {
            let result = self.cancel_request(request_id);
            return self.reply(request_id, result);
        }
        match self.finish_request(request_id) {
            Ok(()) => self.reply(request_id, result),
            Err(error) => self.reply(request_id, error),
        }
    }

    /// Disposes one runtime resource through request and credit accounting.
    ///
    /// Inputs:
    /// - `request_id`: request id supplied by the bridge caller.
    /// - `handle`: opaque resource handle previously returned by `call`.
    ///
    /// Output:
    /// - Worker reply containing either `Ok(Unit)` or stable disposal error
    ///   fields plus current credit counters.
    ///
    /// Transformation:
    /// - Begins request accounting, delegates cleanup to the runtime, finishes
    ///   request accounting, and wraps the term reply in a worker envelope.
    pub fn dispose(
        &mut self,
        request_id: RequestId,
        handle: NativeBoundaryHandle,
    ) -> NativeBoundaryWorkerReply {
        if let Err(error) = self.begin_request(request_id) {
            return self.reply(request_id, error);
        }

        let result = self.runtime.dispose(handle);
        match self.finish_request(request_id) {
            Ok(()) => self.reply(request_id, result),
            Err(error) => self.reply(request_id, error),
        }
    }

    /// Disposes one process-owned resource through request accounting.
    ///
    /// Inputs:
    /// - `request_id`: monotonic request identity assigned by the VM.
    /// - `owner_process_id`: process expected to own the resource.
    /// - `handle`: opaque resource identity returned by this worker.
    ///
    /// Output:
    /// - A worker reply containing Unit or a typed ownership error.
    ///
    /// Transformation:
    /// - Preserves credit accounting while enforcing process ownership at
    ///   resource disposal.
    pub fn dispose_for_process(
        &mut self,
        request_id: RequestId,
        owner_process_id: u64,
        handle: NativeBoundaryHandle,
    ) -> NativeBoundaryWorkerReply {
        if let Err(error) = self.begin_request(request_id) {
            return self.reply(request_id, error);
        }

        let result = self.runtime.dispose_for_process(owner_process_id, handle);
        match self.finish_request(request_id) {
            Ok(()) => self.reply(request_id, result),
            Err(error) => self.reply(request_id, error),
        }
    }

    /// Executes one stable NativeBoundary command term.
    ///
    /// Inputs:
    /// - `command`: transport-neutral command envelope received by the worker.
    ///
    /// Output:
    /// - Worker reply containing the command request id, operation/disposal
    ///   result, and current credit counters.
    ///
    /// Transformation:
    /// - Converts term-level request ids into lifecycle request ids, then
    ///   delegates to the existing `call` or `dispose` path without duplicating
    ///   operation, resource, or credit logic.
    pub fn execute_command(
        &mut self,
        command: &NativeBoundaryCommandTerm,
    ) -> NativeBoundaryWorkerReply {
        match command {
            NativeBoundaryCommandTerm::Call {
                request_id,
                operation,
                args,
            } => self.call(RequestId { value: *request_id }, operation, args),
            NativeBoundaryCommandTerm::Dispose { request_id, handle } => {
                self.dispose(RequestId { value: *request_id }, *handle)
            }
        }
    }

    /// Wraps a term reply in the worker envelope.
    ///
    /// Inputs:
    /// - `request_id`: request id to echo to the bridge caller.
    /// - `result`: stable operation result term.
    ///
    /// Output:
    /// - Worker reply with result and current credit counters.
    ///
    /// Transformation:
    /// - Adds request and credit metadata without changing the operation result.
    fn reply(
        &self,
        request_id: RequestId,
        result: NativeBoundaryReplyTerm,
    ) -> NativeBoundaryWorkerReply {
        NativeBoundaryWorkerReply {
            request_id,
            result,
            reserved_credits: self.reserved_credits,
            available_credits: self.available_credits(),
        }
    }
}

/// Builds a stable worker-level error reply.
///
/// Inputs:
/// - `kind`: closed worker error category.
///
/// Output:
/// - Stable term error reply with code, message, and zero source offset.
///
/// Transformation:
/// - Converts the canonical NativeBoundary error mapping into the term-level error
///   shape used by worker replies.
fn worker_error_reply(kind: ErrorKind) -> NativeBoundaryReplyTerm {
    let error = error_for(kind);
    NativeBoundaryReplyTerm::Error {
        code: error.code.to_string(),
        message: error.message.to_string(),
        offset: 0,
    }
}

#[cfg(test)]
#[path = "worker_test.rs"]
mod worker_test;
