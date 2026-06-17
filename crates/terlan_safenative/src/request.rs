//! Request lifecycle accounting for SafeNative command/reply bridges.
//!
//! Native workers need a small, deterministic command/reply contract before
//! they can safely cross process, thread, or runtime boundaries. This module
//! models the pure part of that contract: request identifiers do not wrap, a
//! new request can start only from an idle slot, and a reply can complete only
//! the matching pending request.

/// Identifier assigned to one SafeNative command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestId {
    /// Monotonic request id value.
    pub value: u64,
}

/// Pure lifecycle state for one command/reply slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestState {
    /// No request currently occupies the slot.
    Idle,
    /// A request is in flight and must be completed by the matching id.
    Pending(RequestId),
    /// A request completed with the recorded id.
    Completed(RequestId),
}

/// Computes the next request id.
///
/// Inputs:
/// - `current`: current request id.
///
/// Output:
/// - `Some(next)` when the id can be incremented.
/// - `None` when incrementing would overflow the machine integer.
///
/// Transformation:
/// - Uses checked addition so request ids never wrap and accidentally match an
///   older in-flight request.
pub fn next_request_id(current: RequestId) -> Option<RequestId> {
    current
        .value
        .checked_add(1)
        .map(|value| RequestId { value })
}

/// Returns whether a state is pending for a specific request id.
///
/// Inputs:
/// - `state`: current lifecycle state.
/// - `request_id`: request id to test.
///
/// Output:
/// - `true` only when `state` is `Pending(request_id)`.
///
/// Transformation:
/// - Performs a pure structural match with no mutation or backend effects.
pub fn is_pending(state: RequestState, request_id: RequestId) -> bool {
    matches!(state, RequestState::Pending(pending_id) if pending_id == request_id)
}

/// Starts a request in an idle lifecycle slot.
///
/// Inputs:
/// - `state`: current lifecycle state.
/// - `request_id`: request id to place into the slot.
///
/// Output:
/// - `Some(Pending(request_id))` when `state` is idle.
/// - `None` when a request is already pending or completed in the slot.
///
/// Transformation:
/// - Converts only `Idle` into `Pending(request_id)`.
pub fn start_request(state: RequestState, request_id: RequestId) -> Option<RequestState> {
    match state {
        RequestState::Idle => Some(RequestState::Pending(request_id)),
        RequestState::Pending(_) | RequestState::Completed(_) => None,
    }
}

/// Completes a pending request with a matching request id.
///
/// Inputs:
/// - `state`: current lifecycle state.
/// - `request_id`: reply request id.
///
/// Output:
/// - `Some(Completed(request_id))` when the state is pending for that id.
/// - `None` when the state is idle, completed, or pending for another id.
///
/// Transformation:
/// - Converts only a matching `Pending(request_id)` into
///   `Completed(request_id)`.
pub fn complete_request(state: RequestState, request_id: RequestId) -> Option<RequestState> {
    match state {
        RequestState::Pending(pending_id) if pending_id == request_id => {
            Some(RequestState::Completed(request_id))
        }
        RequestState::Idle | RequestState::Pending(_) | RequestState::Completed(_) => None,
    }
}

#[cfg(test)]
mod tests {
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
}
