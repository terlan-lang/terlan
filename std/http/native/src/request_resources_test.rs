use super::*;

// Deliberately has no Default, Display, or VM-specific numeric conversion.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Owner(&'static str);

#[test]
fn owners_are_isolated_and_metrics_preserve_peaks() {
    let mut tracker = RequestResourceTracker::default();
    let first = tracker.begin(Owner("first"), 128).unwrap();
    let second = tracker.begin(Owner("second"), 256).unwrap();
    assert_eq!((first, second), (1, 2));
    let active = tracker.metrics();
    assert_eq!(active.active_body_buffers, 2);
    assert_eq!(active.active_telemetry_spans, 2);
    assert_eq!(active.active_route_contexts, 2);
    assert_eq!(active.active_body_bytes, 384);
    assert_eq!(
        tracker.leaks(),
        vec![
            RequestResourceLeak {
                owner: Owner("first"),
                request_id: first
            },
            RequestResourceLeak {
                owner: Owner("second"),
                request_id: second
            },
        ]
    );
    tracker.finish(Owner("first"), first).unwrap();
    assert_eq!(tracker.metrics().active_body_bytes, 256);
    tracker.finish(Owner("second"), second).unwrap();
    let done = tracker.metrics();
    assert_eq!(done.active_body_buffers, 0);
    assert_eq!(done.active_telemetry_spans, 0);
    assert_eq!(done.active_route_contexts, 0);
    assert_eq!(done.active_body_bytes, 0);
    assert_eq!(done.peak_body_buffers, 2);
    assert_eq!(done.peak_telemetry_spans, 2);
    assert_eq!(done.peak_route_contexts, 2);
    assert_eq!(done.peak_body_bytes, 384);
    assert_eq!(done.completed_requests, 2);
    assert_eq!(done.last_request_id, second);
    assert!(tracker.leaks().is_empty());
}

#[test]
fn rejected_operations_leave_metrics_and_ownership_unchanged() {
    let mut tracker = RequestResourceTracker::default();
    let owner = Owner("owner");
    let id = tracker.begin(owner, 42).unwrap();
    let metrics = tracker.metrics();
    let leaks = tracker.leaks();
    assert_eq!(
        tracker.begin(owner, usize::MAX),
        Err(RequestResourceError::AlreadyActive {
            owner,
            request_id: id
        })
    );
    assert_eq!(
        tracker.finish(Owner("stranger"), id),
        Err(RequestResourceError::UnknownOwner {
            owner: Owner("stranger")
        })
    );
    assert_eq!(
        tracker.finish(owner, id + 1),
        Err(RequestResourceError::StaleRequest {
            owner,
            expected: id,
            observed: id + 1
        })
    );
    assert_eq!(tracker.metrics(), metrics);
    assert_eq!(tracker.leaks(), leaks);
    tracker.finish(owner, id).unwrap();
    let next = tracker.begin(owner, 0).unwrap();
    assert_eq!(next, id + 1);
    assert_eq!(
        tracker.finish(owner, id),
        Err(RequestResourceError::StaleRequest {
            owner,
            expected: next,
            observed: id
        })
    );
    assert_eq!(tracker.leaks()[0].request_id, next);
    tracker.finish(owner, next).unwrap();
    assert_eq!(
        tracker.finish(owner, next),
        Err(RequestResourceError::UnknownOwner { owner })
    );
}

#[test]
fn counter_overflow_is_atomic_and_zero_byte_requests_still_fit() {
    let mut tracker = RequestResourceTracker::default();
    let full = tracker.begin(Owner("full"), usize::MAX).unwrap();
    let before = tracker.metrics();
    assert_eq!(
        tracker.begin(Owner("empty"), 1),
        Err(RequestResourceError::BodyBytesOverflow)
    );
    assert_eq!(tracker.metrics(), before);
    assert_eq!(tracker.leaks().len(), 1);
    let empty = tracker.begin(Owner("empty"), 0).unwrap();
    assert_eq!(empty, full + 1);
    tracker.finish(Owner("full"), full).unwrap();
    assert_eq!(tracker.metrics().active_body_bytes, 0);
    tracker.finish(Owner("empty"), empty).unwrap();

    tracker.next_request_id = u64::MAX;
    let before = tracker.metrics();
    assert_eq!(
        tracker.begin(Owner("overflow"), 0),
        Err(RequestResourceError::RequestIdOverflow)
    );
    assert_eq!(tracker.metrics(), before);
    assert!(tracker.leaks().is_empty());
}

#[test]
fn completed_telemetry_saturates_without_retaining_ownership() {
    let mut tracker = RequestResourceTracker::default();
    tracker.metrics.completed_requests = usize::MAX;
    let id = tracker.begin(Owner("saturated"), 2).unwrap();
    tracker.finish(Owner("saturated"), id).unwrap();
    assert_eq!(tracker.metrics().completed_requests, usize::MAX);
    assert!(tracker.leaks().is_empty());
}
