use super::owner_loop::{
    reject_duplicate_route, validate_live_route, validate_scheduler_route, RUNNABLE_QUEUE_CAPACITY,
};
use super::*;
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;
use std::collections::BTreeMap;

fn route(actor: u64) -> VmFixedActorRoute {
    VmSchedulerTopology::new(2)
        .expect("topology")
        .route(std::num::NonZeroU64::new(actor).expect("actor route"))
}

#[test]
fn shard_inbox_is_bounded() {
    assert_eq!(SHARD_INBOX_CAPACITY, 1_024);
    assert_eq!(RUNNABLE_QUEUE_CAPACITY, SHARD_INBOX_CAPACITY);
}

#[test]
fn live_route_validation_rejects_missing_and_wrong_local_owner() {
    let route = route(1);
    let expected = VmProcessId::from_raw_for_test(7);
    let actual = VmProcessId::from_raw_for_test(8);
    let mut routes = BTreeMap::new();
    assert!(validate_live_route(&routes, route, expected).is_err());
    routes.insert(route.actor_id(), expected);
    assert!(validate_live_route(&routes, route, actual)
        .expect_err("wrong owner")
        .contains("owns process"));
    validate_live_route(&routes, route, expected).expect("exact owner");
}

#[test]
fn scheduler_route_validation_rejects_wrong_thread() {
    let error = validate_scheduler_route(route(2), &thread::current())
        .expect_err("test thread is not scheduler one");
    assert!(error.contains("wrong scheduler"), "{error}");
}

#[test]
fn duplicate_route_is_rejected_before_new_actor_allocation() {
    let route = route(1);
    let mut routes = BTreeMap::new();
    routes.insert(route.actor_id(), VmProcessId::from_raw_for_test(1));
    assert!(reject_duplicate_route(&routes, route)
        .expect_err("duplicate route")
        .contains("already live"));
}

/// Panic diagnostics are bounded without splitting UTF-8 code points.
#[test]
fn panic_detail_is_bounded_and_stable_for_all_payload_classes() {
    let unicode = "é".repeat(MAX_SCHEDULER_PANIC_DETAIL_BYTES);
    let detail = panic_detail(Box::new(unicode));
    assert_eq!(detail.len(), MAX_SCHEDULER_PANIC_DETAIL_BYTES);
    assert!(detail.is_char_boundary(detail.len()));
    assert_eq!(
        panic_detail(Box::new(17_u64)),
        "non-string panic payload".to_string()
    );
}
