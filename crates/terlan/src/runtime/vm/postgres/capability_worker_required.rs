//! Serve-runtime Postgres boundary.
//!
//! The compiler-free AOT VM deliberately has no in-process `libpq` driver.
//! Requests receive an explicit terminal failure until the asynchronous
//! capability-worker transport owns their execution.

use std::{
    collections::{BTreeSet, VecDeque},
    time::Duration,
};

use crate::{
    terlan_native::postgres::{self, libpq::DriverReadinessPoller},
    terlan_native_boundary::request::RequestId,
};

use super::{
    VmPostgresDriverCompletion, VmPostgresDriverControl, VmPostgresDriverRequest, VmPostgresFailure,
};

const CAPABILITY_REQUIRED_CODE: &str = "postgres.capability_worker.required";
const CAPABILITY_REQUIRED_MESSAGE: &str = "The AOT serve runtime cannot execute Postgres \
in-process; route this operation through the asynchronous capability-worker protocol.";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmPostgresIoInterest {
    Drive,
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmPostgresDriverWait {
    pub(crate) request_id: RequestId,
    pub(crate) socket: i64,
    pub(crate) interest: VmPostgresIoInterest,
}

#[derive(Debug, Default)]
pub(crate) struct VmPostgresLibpqWorker {
    queued: VecDeque<VmPostgresDriverRequest>,
}

impl VmPostgresLibpqWorker {
    pub(crate) fn submit(&mut self, request: VmPostgresDriverRequest) {
        self.queued.push_back(request);
    }

    pub(crate) fn wait(&self) -> Option<VmPostgresDriverWait> {
        None
    }

    pub(crate) fn waits(&self) -> Vec<VmPostgresDriverWait> {
        Vec::new()
    }

    pub(crate) fn drive_once(&mut self) -> Option<(RequestId, VmPostgresDriverCompletion)> {
        self.reject_next()
    }

    pub(crate) fn drive_socket_ready(
        &mut self,
        _ready: &BTreeSet<u64>,
    ) -> Option<(RequestId, VmPostgresDriverCompletion)> {
        self.reject_next()
    }

    pub(crate) fn wait_ready(
        &self,
        _poller: &DriverReadinessPoller,
        _timeout: Option<Duration>,
    ) -> Result<BTreeSet<u64>, postgres::PostgresError> {
        Err(capability_required_error())
    }

    pub(crate) fn apply_control(
        &mut self,
        _control: VmPostgresDriverControl,
    ) -> Result<(), VmPostgresFailure> {
        Err(capability_required_failure())
    }

    fn reject_next(&mut self) -> Option<(RequestId, VmPostgresDriverCompletion)> {
        self.queued.pop_front().map(|request| {
            (
                request.request_id,
                VmPostgresDriverCompletion::Failed(capability_required_failure()),
            )
        })
    }
}

fn capability_required_error() -> postgres::PostgresError {
    postgres::PostgresError::new(CAPABILITY_REQUIRED_CODE, CAPABILITY_REQUIRED_MESSAGE)
}

fn capability_required_failure() -> VmPostgresFailure {
    VmPostgresFailure::new(CAPABILITY_REQUIRED_CODE, CAPABILITY_REQUIRED_MESSAGE)
}

#[cfg(test)]
#[path = "capability_worker_required_test.rs"]
mod tests;
