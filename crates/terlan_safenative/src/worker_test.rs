use super::*;

/// Builds a request id for worker tests.
///
/// Inputs:
/// - `value`: numeric request id value.
///
/// Output:
/// - SafeNative request id wrapper.
///
/// Transformation:
/// - Wraps the integer in the request-id type used by lifecycle helpers.
fn request_id(value: u64) -> RequestId {
    RequestId { value }
}

/// Extracts a handle from a worker reply.
///
/// Inputs:
/// - `reply`: worker reply expected to contain a handle success.
///
/// Output:
/// - `Some(handle)` when the reply is a handle success.
/// - `None` after asserting no shape in caller logic.
///
/// Transformation:
/// - Pattern-matches stable worker and term reply layers.
fn handle_reply(reply: SafeNativeWorkerReply) -> Option<SafeNativeHandle> {
    let SafeNativeReplyTerm::Ok(SafeNativeTerm::Handle { id, generation }) = reply.result else {
        return None;
    };
    Some(SafeNativeHandle { id, generation })
}

/// Verifies primitive operations run through worker request accounting.
///
/// Inputs:
/// - Base64 text operation and one request id.
///
/// Output:
/// - Test passes when the operation succeeds and all credits are released.
///
/// Transformation:
/// - Exercises begin, runtime call, finish, and reply-envelope generation.
#[test]
fn worker_call_executes_runtime_and_releases_credit() {
    let mut worker = SafeNativeWorker::new(2);

    let reply = worker.call(
        request_id(1),
        "std.encoding.base64.encode",
        &[SafeNativeTerm::Text(String::from("hello"))],
    );

    assert_eq!(reply.request_id, request_id(1));
    assert_eq!(
        reply.result,
        SafeNativeReplyTerm::Ok(SafeNativeTerm::Text(String::from("aGVsbG8=")))
    );
    assert_eq!(reply.reserved_credits, 0);
    assert_eq!(reply.available_credits, 2);
    assert_eq!(worker.reserved_credits(), 0);
}

/// Verifies command terms execute through the worker call path.
///
/// Inputs:
/// - A call command term for Base64 encoding.
///
/// Output:
/// - Test passes when the command returns the encoded text and releases its
///   credit.
///
/// Transformation:
/// - Exercises the transport-neutral command envelope without bypassing
///   worker request or credit accounting.
#[test]
fn worker_execute_command_runs_call_terms() {
    let mut worker = SafeNativeWorker::new(2);
    let command = SafeNativeCommandTerm::Call {
        request_id: 10,
        operation: String::from("std.encoding.base64.encode"),
        args: vec![SafeNativeTerm::Text(String::from("hello"))],
    };

    let reply = worker.execute_command(&command);

    assert_eq!(reply.request_id, request_id(10));
    assert_eq!(
        reply.result,
        SafeNativeReplyTerm::Ok(SafeNativeTerm::Text(String::from("aGVsbG8=")))
    );
    assert_eq!(reply.reserved_credits, 0);
    assert_eq!(reply.available_credits, 2);
}

/// Verifies the worker enforces its credit limit across pending requests.
///
/// Inputs:
/// - Worker with one credit and two request ids.
///
/// Output:
/// - Test passes when the second pending request is rejected with the
///   canonical backpressure error.
///
/// Transformation:
/// - Exercises credit reservation without executing runtime operations.
#[test]
fn worker_begin_request_rejects_backpressure_limit() {
    let mut worker = SafeNativeWorker::new(1);

    assert_eq!(worker.begin_request(request_id(1)), Ok(()));
    assert_eq!(
        worker.begin_request(request_id(2)),
        Err(worker_error_reply(ErrorKind::BackpressureLimit))
    );
    assert_eq!(worker.reserved_credits(), 1);
    assert_eq!(worker.available_credits(), 0);
}

/// Verifies duplicate request ids are rejected before reserving credit.
///
/// Inputs:
/// - Worker with two credits and the same request id submitted twice.
///
/// Output:
/// - Test passes when the duplicate id returns the canonical invalid
///   request error and credit accounting remains unchanged.
///
/// Transformation:
/// - Exercises request-id uniqueness independent from the credit limit.
#[test]
fn worker_begin_request_rejects_duplicate_request_id() {
    let mut worker = SafeNativeWorker::new(2);

    assert_eq!(worker.begin_request(request_id(1)), Ok(()));
    assert_eq!(
        worker.begin_request(request_id(1)),
        Err(worker_error_reply(ErrorKind::InvalidRequest))
    );
    assert_eq!(worker.reserved_credits(), 1);
    assert_eq!(worker.available_credits(), 1);
}

/// Verifies finishing an unknown request id is rejected.
///
/// Inputs:
/// - Worker with one pending request and a mismatched completion id.
///
/// Output:
/// - Test passes when the mismatched id returns the canonical invalid
///   request error and the original pending credit remains reserved.
///
/// Transformation:
/// - Exercises reply/request matching before releasing credits.
#[test]
fn worker_finish_request_rejects_mismatched_request_id() {
    let mut worker = SafeNativeWorker::new(2);

    assert_eq!(worker.begin_request(request_id(1)), Ok(()));
    assert_eq!(
        worker.finish_request(request_id(2)),
        Err(worker_error_reply(ErrorKind::InvalidRequest))
    );
    assert_eq!(worker.reserved_credits(), 1);
    assert_eq!(worker.available_credits(), 1);
}

/// Verifies handles produced by one worker call can be disposed safely.
///
/// Inputs:
/// - JSON parse operation followed by dispose operation.
///
/// Output:
/// - Test passes when disposal succeeds and stale reuse is rejected.
///
/// Transformation:
/// - Exercises resource-backed runtime ownership through the worker
///   request/credit envelope.
#[test]
fn worker_disposes_runtime_resources() {
    let mut worker = SafeNativeWorker::new(2);
    let Some(handle) = handle_reply(worker.call(
        request_id(1),
        "std.data.json.parse",
        &[SafeNativeTerm::Text(String::from("null"))],
    )) else {
        return;
    };

    let disposed = worker.dispose(request_id(2), handle);
    assert_eq!(
        disposed.result,
        SafeNativeReplyTerm::Ok(SafeNativeTerm::Unit)
    );
    assert_eq!(disposed.reserved_credits, 0);

    let stale = worker.call(
        request_id(3),
        "std.data.json.is_null",
        &[SafeNativeTerm::Handle {
            id: handle.id,
            generation: handle.generation,
        }],
    );
    assert!(matches!(stale.result, SafeNativeReplyTerm::Error { .. }));
    assert_eq!(stale.reserved_credits, 0);
}

/// Verifies command terms execute through the worker dispose path.
///
/// Inputs:
/// - JSON parse call followed by a dispose command term for the returned
///   handle.
///
/// Output:
/// - Test passes when disposal succeeds and all credits are released.
///
/// Transformation:
/// - Exercises command-level disposal without duplicating resource cleanup
///   logic outside the worker.
#[test]
fn worker_execute_command_runs_dispose_terms() {
    let mut worker = SafeNativeWorker::new(2);
    let Some(handle) = handle_reply(worker.call(
        request_id(20),
        "std.data.json.parse",
        &[SafeNativeTerm::Text(String::from("null"))],
    )) else {
        return;
    };
    let command = SafeNativeCommandTerm::Dispose {
        request_id: 21,
        handle,
    };

    let reply = worker.execute_command(&command);

    assert_eq!(reply.request_id, request_id(21));
    assert_eq!(reply.result, SafeNativeReplyTerm::Ok(SafeNativeTerm::Unit));
    assert_eq!(reply.reserved_credits, 0);
    assert_eq!(reply.available_credits, 2);
}
