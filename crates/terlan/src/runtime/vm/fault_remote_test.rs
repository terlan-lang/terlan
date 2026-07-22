use super::{evaluate_call, evaluate_receiver};
use crate::runtime::vm::ReplValue;

fn string(value: &str) -> ReplValue {
    ReplValue::String(value.to_string())
}

fn int(value: i64) -> ReplValue {
    ReplValue::Int(value)
}

fn policy() -> ReplValue {
    evaluate_call("policy", &[int(2), int(5), int(9)]).expect("valid policy")
}

fn monitor() -> ReplValue {
    evaluate_call(
        "monitor",
        &[
            ReplValue::List(vec![string("node-a"), string("node-b")]),
            policy(),
        ],
    )
    .expect("valid monitor")
}

fn receiver(function: &str, monitor: &ReplValue, args: &[ReplValue]) -> ReplValue {
    evaluate_receiver(function, monitor, args)
        .expect("fault monitor receiver operation")
        .expect("successful fault monitor operation")
}

#[test]
fn vm_distributed_fault_recovery() {
    let monitor = monitor();

    assert_eq!(
        receiver("record_heartbeat", &monitor, &[string("node-a"), int(1)]),
        string("recorded")
    );
    assert_eq!(
        receiver("record_heartbeat", &monitor, &[string("node-a"), int(1)]),
        string("duplicate_suppressed")
    );

    let suspected = receiver(
        "suspect",
        &monitor,
        &[string("node-a"), int(4), string("heartbeat missed")],
    );
    assert!(matches!(suspected, ReplValue::Tuple(_)));
    assert_eq!(
        receiver("state_name", &monitor, &[string("node-a")]),
        string("suspected")
    );

    receiver(
        "isolate",
        &monitor,
        &[string("node-a"), int(7), string("partition confirmed")],
    );
    receiver(
        "begin_recovery",
        &monitor,
        &[string("node-a"), int(8), string("peer rejoined")],
    );
    let recovered = receiver(
        "complete",
        &monitor,
        &[string("node-a"), int(9), string("state caught up")],
    );
    assert_eq!(
        evaluate_call("transition_next_state", &[recovered]).expect("transition state"),
        string("recovered")
    );
    assert_eq!(
        receiver("state_name", &monitor, &[string("node-a")]),
        string("recovered")
    );

    let transitions = receiver("transitions_after", &monitor, &[int(0)]);
    let failures = receiver("failures_after", &monitor, &[int(0)]);
    assert!(matches!(transitions, ReplValue::List(items) if items.len() == 4));
    assert!(matches!(failures, ReplValue::List(items) if items.len() == 2));
}

#[test]
fn source_fault_monitor_bounds_recovery_and_replays_duplicate_transitions() {
    let monitor = monitor();
    let suspect_args = [string("node-b"), int(3), string("heartbeat missed")];
    let first = receiver("suspect", &monitor, &suspect_args);
    let replay = receiver("suspect", &monitor, &suspect_args);
    assert_eq!(first, replay);

    receiver(
        "isolate",
        &monitor,
        &[string("node-b"), int(6), string("partition confirmed")],
    );
    receiver(
        "begin_recovery",
        &monitor,
        &[string("node-b"), int(7), string("peer rejoined")],
    );
    let before_window = receiver(
        "expire",
        &monitor,
        &[string("node-b"), int(16), string("recovery expired")],
    );
    assert_eq!(before_window, ReplValue::Atom("none".to_string()));
    receiver(
        "expire",
        &monitor,
        &[string("node-b"), int(17), string("recovery expired")],
    );
    assert_eq!(
        receiver("state_name", &monitor, &[string("node-b")]),
        string("isolated")
    );

    let failures = receiver("failures_after", &monitor, &[int(0)]);
    assert!(matches!(failures, ReplValue::List(items) if items.len() == 3));
}

#[test]
fn source_fault_monitor_rejects_invalid_policy_events_and_closed_handles() {
    assert!(evaluate_call("policy", &[int(5), int(2), int(4)])
        .expect_err("unordered policy must fail")
        .contains("isolation threshold"));
    assert!(evaluate_call(
        "monitor",
        &[
            ReplValue::List(vec![string("node-a"), string("node-a")]),
            policy(),
        ],
    )
    .expect_err("duplicate node must fail")
    .contains("duplicate node id"));

    let monitor = monitor();
    receiver("record_heartbeat", &monitor, &[string("node-a"), int(2)]);
    let stale = evaluate_receiver("record_heartbeat", &monitor, &[string("node-a"), int(1)])
        .expect("recognized receiver")
        .expect_err("stale heartbeat must fail");
    assert!(stale.contains("stale heartbeat tick"));

    receiver("close", &monitor, &[]);
    let closed = evaluate_receiver("state_name", &monitor, &[string("node-a")])
        .expect("recognized receiver")
        .expect_err("closed monitor must fail");
    assert!(closed.contains("closed or unknown"));
}

#[test]
fn source_fault_failure_accessors_expose_stable_migration_diagnostics() {
    let rollback = evaluate_call(
        "migration_timeout",
        &[
            string("actor-a"),
            int(3),
            string("node-a"),
            string("node-b"),
            string("transferring"),
            int(15),
            string("migration timed out"),
        ],
    )
    .expect("rollback descriptor");
    let failure = evaluate_call("failure", &[rollback]).expect("failure envelope");
    assert_eq!(
        evaluate_call("failure_kind", &[failure.clone()]).expect("failure kind"),
        string("migration_timeout")
    );
    assert_eq!(
        evaluate_call("failure_node_id", &[failure.clone()]).expect("failure node"),
        string("node-b")
    );
    assert_eq!(
        evaluate_call("failure_tick", &[failure.clone()]).expect("failure tick"),
        int(15)
    );
    assert_eq!(
        evaluate_call("failure_reason", &[failure]).expect("failure reason"),
        string("migration timed out")
    );
}

#[test]
fn source_fault_diagnostics_and_compatibility_use_stable_machine_labels() {
    assert_eq!(
        evaluate_call(
            "compatibility",
            &[ReplValue::Bool(true), ReplValue::Bool(false)]
        )
        .expect("supported"),
        string("supported")
    );
    assert_eq!(
        evaluate_call(
            "compatibility",
            &[ReplValue::Bool(false), ReplValue::Bool(true)]
        )
        .expect("fallback"),
        string("fallback_local_only")
    );
    assert_eq!(
        evaluate_call(
            "compatibility",
            &[ReplValue::Bool(false), ReplValue::Bool(false)]
        )
        .expect("unsupported"),
        string("feature_unsupported")
    );

    let transition = evaluate_call(
        "classify_heartbeat",
        &[policy(), string("node-a"), int(4), int(4), string("missed")],
    )
    .expect("transition");
    assert_eq!(
        evaluate_call("transition_diagnostic_kind", &[transition]).expect("diagnostic"),
        string("partition_onset")
    );
    let failure = evaluate_call(
        "stale_placement_update",
        &[
            string("actor-a"),
            string("node-b"),
            int(4),
            int(5),
            int(12),
            string("stale"),
        ],
    )
    .expect("stale failure");
    assert_eq!(
        evaluate_call("failure_diagnostic_kind", &[failure]).expect("diagnostic"),
        string("stale_placement_rejection")
    );
}
