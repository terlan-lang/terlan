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
#[path = "request_test.rs"]
mod request_test;
