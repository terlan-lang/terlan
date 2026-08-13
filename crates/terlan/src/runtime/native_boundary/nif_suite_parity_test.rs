//! Portable replacement coverage for the retired ERTS `nif_SUITE`.

use super::*;
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;
use crate::terlan_native_boundary::metadata::NativeBoundaryWorkerClass;
use crate::terlan_native_boundary::request::RequestId;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

const OWNER: u64 = 41;
const FOREIGN_OWNER: u64 = 43;

fn request_id(value: u64) -> RequestId {
    RequestId { value }
}

fn handle_from_reply(reply: &NativeBoundaryWorkerReply) -> Option<NativeBoundaryHandle> {
    let NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Handle { id, generation }) = &reply.result
    else {
        return None;
    };
    Some(NativeBoundaryHandle {
        id: *id,
        generation: *generation,
    })
}

fn handle_term(handle: NativeBoundaryHandle) -> NativeBoundaryTerm {
    NativeBoundaryTerm::Handle {
        id: handle.id,
        generation: handle.generation,
    }
}

fn error_code(reply: &NativeBoundaryReplyTerm) -> Option<&str> {
    let NativeBoundaryReplyTerm::Error { code, .. } = reply else {
        return None;
    };
    Some(code)
}

fn assert_credit_recovered(worker: &NativeBoundaryWorker) {
    assert_eq!(worker.reserved_credits(), 0);
    assert_eq!(worker.available_credits(), worker.credit_limit());
}

/// Replaces portable NIF value, exception, resource, and destructor outcomes.
///
/// The ERTS suite reached these outcomes through `erl_nif` entry points and
/// host pointers. Terlan reaches them through monotonic VM requests, owned
/// terms, process-qualified resource handles, and deterministic disposal.
#[test]
fn nif_suite_portable_calls_keep_values_resources_and_failures_vm_owned() {
    let mut worker = NativeBoundaryWorker::new(1);

    let primitive = worker.call(
        request_id(1),
        "std.encoding.base64.encode",
        &[NativeBoundaryTerm::Text(String::from("hello"))],
    );
    assert_eq!(primitive.request_id, request_id(1));
    assert_eq!(
        primitive.result,
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Text(String::from("aGVsbG8=")))
    );
    assert_credit_recovered(&worker);

    let created = worker.call_for_process_with_policy(
        crate::terlan_native_boundary::worker::NativeBoundaryWorkerCall {
            request_id: request_id(2),
            owner_process_id: OWNER,
            granted_capabilities: &[],
            admitted_worker_classes: &[] as &[NativeBoundaryWorkerClass],
            operation: "std.data.json.parse",
            args: &[NativeBoundaryTerm::Text(String::from("\"Ada\""))],
        },
    );
    let handle = handle_from_reply(&created);
    assert!(handle.is_some(), "JSON parse must return an opaque handle");
    let Some(handle) = handle else {
        return;
    };
    assert_credit_recovered(&worker);

    let foreign_read = worker.call_for_process_with_policy(
        crate::terlan_native_boundary::worker::NativeBoundaryWorkerCall {
            request_id: request_id(3),
            owner_process_id: FOREIGN_OWNER,
            granted_capabilities: &[],
            admitted_worker_classes: &[],
            operation: "std.data.json.as_string",
            args: &[handle_term(handle)],
        },
    );
    assert_eq!(error_code(&foreign_read.result), Some("resource.owner"));

    let foreign_dispose = worker.dispose_for_process(request_id(4), FOREIGN_OWNER, handle);
    assert_eq!(error_code(&foreign_dispose.result), Some("resource.owner"));

    let owner_read = worker.call_for_process_with_policy(
        crate::terlan_native_boundary::worker::NativeBoundaryWorkerCall {
            request_id: request_id(5),
            owner_process_id: OWNER,
            granted_capabilities: &[],
            admitted_worker_classes: &[],
            operation: "std.data.json.as_string",
            args: &[handle_term(handle)],
        },
    );
    assert_eq!(
        owner_read.result,
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Text(String::from("Ada")))
    );

    let disposed = worker.dispose_for_process(request_id(6), OWNER, handle);
    assert_eq!(
        disposed.result,
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Unit)
    );

    let stale = worker.call_for_process_with_policy(
        crate::terlan_native_boundary::worker::NativeBoundaryWorkerCall {
            request_id: request_id(7),
            owner_process_id: OWNER,
            granted_capabilities: &[],
            admitted_worker_classes: &[],
            operation: "std.data.json.as_string",
            args: &[handle_term(handle)],
        },
    );
    assert_eq!(error_code(&stale.result), Some("resource.stale_handle"));

    let unknown = worker.call(request_id(8), "erts.nif.compatibility", &[]);
    assert_eq!(
        error_code(&unknown.result),
        Some("dispatch.unknown_operation")
    );
    assert_credit_recovered(&worker);
}

/// Replaces NIF scheduler-thread and load-race outcomes with explicit worker
/// admission, cancellation, timeout, and stale-request rules.
#[test]
fn nif_suite_request_lifecycle_recovers_backpressure_without_id_reuse() {
    let mut worker = NativeBoundaryWorker::new(2);

    assert_eq!(worker.begin_request(request_id(10)), Ok(()));
    assert_eq!(worker.begin_request(request_id(11)), Ok(()));
    let saturated = worker.begin_request(request_id(12));
    assert!(saturated.is_err());
    assert_eq!(
        saturated.err().as_ref().and_then(error_code),
        Some("native_boundary.backpressure_limit")
    );
    assert_eq!(worker.reserved_credits(), 2);
    assert_eq!(worker.available_credits(), 0);

    assert_eq!(
        error_code(&worker.cancel_request(request_id(10))),
        Some("native_boundary.cancelled")
    );
    assert!(worker.finish_request(request_id(10)).is_err());
    assert_eq!(worker.begin_request(request_id(12)), Ok(()));
    assert_eq!(
        error_code(&worker.timeout_request(request_id(11))),
        Some("native_boundary.timeout")
    );
    assert_eq!(worker.finish_request(request_id(12)), Ok(()));
    assert!(worker.begin_request(request_id(12)).is_err());
    assert_credit_recovered(&worker);

    let outcomes = worker
        .events()
        .map(|event| event.outcome)
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        [
            NativeBoundaryWorkerOutcome::Accepted,
            NativeBoundaryWorkerOutcome::Accepted,
            NativeBoundaryWorkerOutcome::RejectedBackpressure,
            NativeBoundaryWorkerOutcome::Cancelled,
            NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
            NativeBoundaryWorkerOutcome::Accepted,
            NativeBoundaryWorkerOutcome::TimedOut,
            NativeBoundaryWorkerOutcome::Completed,
            NativeBoundaryWorkerOutcome::RejectedInvalidRequest,
        ]
    );
}
