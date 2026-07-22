use super::request_resources::VmHttpRequestResourceTracker;
use crate::runtime::vm::process::{VmProcessSource, VmProcessTable};

#[test]
fn request_resources_track_peaks_and_release_every_transient_class() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.Http", "handle", 1));
    let mut resources = VmHttpRequestResourceTracker::default();

    let request_id = resources.begin(owner, 128).expect("begin resources");
    let active = resources.metrics();
    assert_eq!(active.active_body_buffers, 1);
    assert_eq!(active.active_telemetry_spans, 1);
    assert_eq!(active.active_route_contexts, 1);
    assert_eq!(active.active_body_bytes, 128);
    assert_eq!(active.peak_body_bytes, 128);

    resources
        .finish(owner, request_id)
        .expect("finish resources");
    let released = resources.metrics();
    assert_eq!(released.active_body_buffers, 0);
    assert_eq!(released.active_telemetry_spans, 0);
    assert_eq!(released.active_route_contexts, 0);
    assert_eq!(released.active_body_bytes, 0);
    assert_eq!(released.completed_requests, 1);
    assert!(resources.leaks().is_empty());
}

#[test]
fn request_resources_reject_duplicate_stale_and_unknown_completion() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.Http", "handle", 1));
    let mut resources = VmHttpRequestResourceTracker::default();
    let request_id = resources.begin(owner, 0).expect("begin resources");

    assert_eq!(
        resources.begin(owner, 0).expect_err("duplicate owner"),
        format!(
            "VM HTTP process {} already owns request resources for request {request_id}",
            owner.as_u64()
        )
    );
    assert_eq!(
        resources
            .finish(owner, request_id + 1)
            .expect_err("stale request id"),
        format!(
            "VM HTTP process {} request resource mismatch: expected {request_id}, observed {}",
            owner.as_u64(),
            request_id + 1
        )
    );
    assert_eq!(resources.leaks().len(), 1);
    resources
        .finish(owner, request_id)
        .expect("finish resources");
    assert_eq!(
        resources
            .finish(owner, request_id)
            .expect_err("unknown completion"),
        format!(
            "VM HTTP process {} has no active request resources",
            owner.as_u64()
        )
    );
}
