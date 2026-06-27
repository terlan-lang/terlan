use super::*;

/// Builds a request id used by lifecycle tests.
///
/// Inputs:
/// - `value`: numeric request id value.
///
/// Output:
/// - A `RequestId` carrying `value`.
///
/// Transformation:
/// - Wraps the raw integer in the SafeNative request id type.
fn request_id(value: u64) -> RequestId {
    RequestId { value }
}

/// Verifies request ids do not wrap on overflow.
///
/// Inputs:
/// - The maximum `u64` request id and a normal request id.
///
/// Output:
/// - Test passes when overflow returns `None` and normal increment
///   succeeds.
///
/// Transformation:
/// - Exercises checked arithmetic for request-id allocation.
#[test]
fn next_request_id_rejects_overflow() {
    assert_eq!(next_request_id(request_id(u64::MAX)), None);
    assert_eq!(next_request_id(request_id(41)), Some(request_id(42)));
}

/// Verifies a request can start only from idle state.
///
/// Inputs:
/// - Idle, pending, and completed states.
///
/// Output:
/// - Test passes when only idle state transitions to pending.
///
/// Transformation:
/// - Exercises the lifecycle rule that a slot cannot be reused while it is
///   already occupied.
#[test]
fn start_request_accepts_only_idle_state() {
    let id = request_id(10);

    assert_eq!(
        start_request(RequestState::Idle, id),
        Some(RequestState::Pending(id))
    );
    assert_eq!(
        start_request(RequestState::Pending(id), request_id(11)),
        None
    );
    assert_eq!(
        start_request(RequestState::Completed(id), request_id(11)),
        None
    );
}

/// Verifies pending checks are id-specific.
///
/// Inputs:
/// - A pending state and matching/mismatched request ids.
///
/// Output:
/// - Test passes when only the matching id reports pending.
///
/// Transformation:
/// - Exercises the command/reply matching predicate.
#[test]
fn is_pending_matches_request_id() {
    let id = request_id(10);

    assert!(is_pending(RequestState::Pending(id), id));
    assert!(!is_pending(RequestState::Pending(id), request_id(11)));
    assert!(!is_pending(RequestState::Idle, id));
}

/// Verifies only a matching pending request can complete.
///
/// Inputs:
/// - Pending states with matching and mismatched reply ids.
///
/// Output:
/// - Test passes when matching replies complete and mismatched replies fail.
///
/// Transformation:
/// - Exercises stale or crossed reply rejection.
#[test]
fn complete_request_requires_matching_pending_id() {
    let id = request_id(10);

    assert_eq!(
        complete_request(RequestState::Pending(id), id),
        Some(RequestState::Completed(id))
    );
    assert_eq!(
        complete_request(RequestState::Pending(id), request_id(11)),
        None
    );
    assert_eq!(complete_request(RequestState::Idle, id), None);
    assert_eq!(complete_request(RequestState::Completed(id), id), None);
}

/// Verifies a completed request no longer counts as pending.
///
/// Inputs:
/// - A pending state and matching request id.
///
/// Output:
/// - Test passes when successful completion produces a non-pending state.
///
/// Transformation:
/// - Checks the postcondition required before a slot can be retired or
///   prepared for a later generation.
#[test]
fn completed_request_is_not_pending() {
    let id = request_id(10);
    let completed = RequestState::Completed(id);

    assert_eq!(
        complete_request(RequestState::Pending(id), id),
        Some(completed)
    );
    assert!(!is_pending(completed, id));
}
