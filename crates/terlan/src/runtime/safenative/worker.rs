//! Request/credit-aware SafeNative worker core.
//!
//! This module composes the stable term runtime with the pure request and
//! credit helpers. It is still transport-neutral: BEAM, thread, NIF, or process
//! bridges can wrap this worker without changing the request lifecycle,
//! backpressure, resource ownership, or reply shape.

use std::collections::BTreeMap;

use crate::terlan_safenative::credit::{normalize_limit, release_credit, reserve_credit};
use crate::terlan_safenative::error::{error_for, ErrorKind};
use crate::terlan_safenative::handle::SafeNativeHandle;
use crate::terlan_safenative::request::{complete_request, start_request, RequestId, RequestState};
use crate::terlan_safenative::runtime::SafeNativeRuntime;
use crate::terlan_safenative::term::{SafeNativeCommandTerm, SafeNativeReplyTerm, SafeNativeTerm};

/// Stable reply envelope returned by the SafeNative worker contract.
#[derive(Clone, Debug, PartialEq)]
pub struct SafeNativeWorkerReply {
    /// Request id associated with this reply.
    pub request_id: RequestId,
    /// Operation result encoded in stable term form.
    pub result: SafeNativeReplyTerm,
    /// Credits currently reserved by in-flight requests.
    pub reserved_credits: u64,
    /// Credits still available inside the normalized credit limit.
    pub available_credits: u64,
}

/// Transport-neutral SafeNative worker state.
#[derive(Debug)]
pub struct SafeNativeWorker {
    runtime: SafeNativeRuntime,
    credit_limit: u64,
    reserved_credits: u64,
    requests: BTreeMap<u64, RequestState>,
}

impl SafeNativeWorker {
    /// Builds a SafeNative worker with an empty runtime and credit window.
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
            runtime: SafeNativeRuntime::new(),
            credit_limit: normalize_limit(credit_limit),
            reserved_credits: 0,
            requests: BTreeMap::new(),
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

    /// Starts tracking an in-flight request.
    ///
    /// Inputs:
    /// - `request_id`: request id supplied by the bridge caller.
    ///
    /// Output:
    /// - `Ok(())` when the request reserves a credit and enters pending state.
    /// - `Err(reply)` with stable error fields when the id is duplicated or the
    ///   worker is out of credits.
    ///
    /// Transformation:
    /// - Reserves one credit, creates a pending lifecycle state from `Idle`,
    ///   and stores it under the request id.
    pub fn begin_request(&mut self, request_id: RequestId) -> Result<(), SafeNativeReplyTerm> {
        if self.requests.contains_key(&request_id.value) {
            return Err(worker_error_reply(ErrorKind::InvalidRequest));
        }

        let Some(next_reserved) = reserve_credit(self.reserved_credits, 1, self.credit_limit)
        else {
            return Err(worker_error_reply(ErrorKind::BackpressureLimit));
        };

        let Some(state) = start_request(RequestState::Idle, request_id) else {
            return Err(worker_error_reply(ErrorKind::InvalidRequest));
        };

        self.reserved_credits = next_reserved;
        self.requests.insert(request_id.value, state);
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
    pub fn finish_request(&mut self, request_id: RequestId) -> Result<(), SafeNativeReplyTerm> {
        let Some(state) = self.requests.remove(&request_id.value) else {
            return Err(worker_error_reply(ErrorKind::InvalidRequest));
        };

        if complete_request(state, request_id).is_none() {
            self.requests.insert(request_id.value, state);
            return Err(worker_error_reply(ErrorKind::InvalidRequest));
        }

        let Some(next_reserved) = release_credit(self.reserved_credits, 1) else {
            return Err(worker_error_reply(ErrorKind::InvalidRequest));
        };

        self.reserved_credits = next_reserved;
        Ok(())
    }

    /// Calls one SafeNative operation through request and credit accounting.
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
        args: &[SafeNativeTerm],
    ) -> SafeNativeWorkerReply {
        if let Err(error) = self.begin_request(request_id) {
            return self.reply(request_id, error);
        }

        let result = self.runtime.call(operation, args);
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
        handle: SafeNativeHandle,
    ) -> SafeNativeWorkerReply {
        if let Err(error) = self.begin_request(request_id) {
            return self.reply(request_id, error);
        }

        let result = self.runtime.dispose(handle);
        match self.finish_request(request_id) {
            Ok(()) => self.reply(request_id, result),
            Err(error) => self.reply(request_id, error),
        }
    }

    /// Executes one stable SafeNative command term.
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
    pub fn execute_command(&mut self, command: &SafeNativeCommandTerm) -> SafeNativeWorkerReply {
        match command {
            SafeNativeCommandTerm::Call {
                request_id,
                operation,
                args,
            } => self.call(RequestId { value: *request_id }, operation, args),
            SafeNativeCommandTerm::Dispose { request_id, handle } => {
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
    fn reply(&self, request_id: RequestId, result: SafeNativeReplyTerm) -> SafeNativeWorkerReply {
        SafeNativeWorkerReply {
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
/// - Converts the canonical SafeNative error mapping into the term-level error
///   shape used by worker replies.
fn worker_error_reply(kind: ErrorKind) -> SafeNativeReplyTerm {
    let error = error_for(kind);
    SafeNativeReplyTerm::Error {
        code: error.code.to_string(),
        message: error.message.to_string(),
        offset: 0,
    }
}

#[cfg(test)]
#[path = "worker_test.rs"]
mod worker_test;
