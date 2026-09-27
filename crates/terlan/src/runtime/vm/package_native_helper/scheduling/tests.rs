use super::*;

fn request(operation: &str, args: Vec<ReplValue>) -> PureNativeCapabilityRequest {
    PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: format!("std.vm.{operation}"),
        arguments: Vec::new(),
        package_arguments: Some(args),
        result_type: crate::runtime::native_image::TvmBoundaryType::Unit,
    }
}

fn call(
    runtime: &mut VmSchedulingRuntime,
    owner: u64,
    operation: &str,
    args: Vec<ReplValue>,
) -> VmRuntimeResult<ReplValue> {
    runtime.call(owner, &request(operation, args))
}

fn int(value: i64) -> ReplValue {
    ReplValue::Int(value)
}

fn monitor(runtime: &mut VmSchedulingRuntime, owner: u64) -> ReplValue {
    let policy = call(runtime, owner, "fault.policy", vec![int(2), int(5), int(9)]).unwrap();
    call(
        runtime,
        owner,
        "fault.monitor",
        vec![ReplValue::List(vec![string("node-a")]), policy],
    )
    .unwrap()
}

#[test]
fn monitor_checks_ownership_kind_generation_and_cleanup() {
    let mut runtime = VmSchedulingRuntime::default();
    let monitor = monitor(&mut runtime, 7);
    assert!(call(
        &mut runtime,
        8,
        "fault.state_name",
        vec![monitor.clone(), string("node-a")]
    )
    .is_err());
    let policy = call(&mut runtime, 7, "scheduler.round_robin", vec![]).unwrap();
    assert!(call(
        &mut runtime,
        7,
        "fault.monitor",
        vec![ReplValue::List(vec![string("node-a")]), policy]
    )
    .is_err());
    let mut forged = monitor.clone();
    if let ReplValue::Record { name, .. } = &mut forged {
        *name = "Policy".into();
    }
    assert!(call(&mut runtime, 7, "fault.close", vec![forged]).is_err());
    assert_eq!(
        call(
            &mut runtime,
            7,
            "fault.state_name",
            vec![monitor.clone(), string("node-a")]
        )
        .unwrap(),
        string("recovered")
    );
    call(&mut runtime, 7, "fault.close", vec![monitor.clone()]).unwrap();
    assert!(call(
        &mut runtime,
        7,
        "fault.state_name",
        vec![monitor.clone(), string("node-a")]
    )
    .is_err());
    let replacement = self::monitor(&mut runtime, 7);
    assert_ne!(monitor, replacement);
    assert!(call(&mut runtime, 7, "fault.close", vec![monitor]).is_err());
    let other = self::monitor(&mut runtime, 8);
    runtime.close_owner(7);
    assert!(call(&mut runtime, 7, "fault.close", vec![replacement]).is_err());
    assert_eq!(
        call(
            &mut runtime,
            8,
            "fault.state_name",
            vec![other, string("node-a")]
        )
        .unwrap(),
        string("recovered")
    );
}

#[test]
fn replay_preserves_identity_and_rejected_events_preserve_monitor_state() {
    let mut runtime = VmSchedulingRuntime::default();
    let monitor = monitor(&mut runtime, 1);
    let args = vec![monitor.clone(), string("node-a"), int(3), string("missed")];
    let first = call(&mut runtime, 1, "fault.suspect", args.clone()).unwrap();
    assert_eq!(call(&mut runtime, 1, "fault.suspect", args).unwrap(), first);
    for (operation, tick, reason) in [
        ("fault.complete", 4, "early"),
        ("fault.degrade", 2, "stale"),
        ("fault.degrade", 4, ""),
    ] {
        assert!(call(
            &mut runtime,
            1,
            operation,
            vec![monitor.clone(), string("node-a"), int(tick), string(reason)]
        )
        .is_err());
    }
    assert_eq!(
        call(
            &mut runtime,
            1,
            "fault.state_name",
            vec![monitor.clone(), string("node-a")]
        )
        .unwrap(),
        string("suspected")
    );
    call(
        &mut runtime,
        1,
        "fault.isolate",
        vec![
            monitor.clone(),
            string("node-a"),
            int(6),
            string("partition"),
        ],
    )
    .unwrap();
    call(
        &mut runtime,
        1,
        "fault.begin_recovery",
        vec![monitor.clone(), string("node-a"), int(7), string("retry")],
    )
    .unwrap();
    assert_eq!(
        call(
            &mut runtime,
            1,
            "fault.expire",
            vec![
                monitor.clone(),
                string("node-a"),
                int(16),
                string("expired")
            ]
        )
        .unwrap(),
        ReplValue::Record {
            name: "None".into(),
            fields: Vec::new()
        }
    );
    let expired = call(
        &mut runtime,
        1,
        "fault.expire",
        vec![
            monitor.clone(),
            string("node-a"),
            int(17),
            string("expired"),
        ],
    )
    .unwrap();
    assert!(
        matches!(expired, ReplValue::Record { name, fields } if name == "Some" && fields.len() == 1 && fields[0].0 == "value")
    );
    assert_eq!(
        call(
            &mut runtime,
            1,
            "fault.state_name",
            vec![monitor, string("node-a")]
        )
        .unwrap(),
        string("isolated")
    );
}

#[test]
fn invalid_policy_membership_and_descriptor_inputs_fail() {
    let mut runtime = VmSchedulingRuntime::default();
    for args in [
        vec![int(0), int(5), int(9)],
        vec![int(5), int(2), int(9)],
        vec![int(2), int(5), int(-1)],
    ] {
        assert!(call(&mut runtime, 1, "fault.policy", args).is_err());
    }
    let policy = call(
        &mut runtime,
        1,
        "fault.policy",
        vec![int(2), int(5), int(9)],
    )
    .unwrap();
    for nodes in [vec![], vec![string("")], vec![string("a"), string("a")]] {
        assert!(call(
            &mut runtime,
            1,
            "fault.monitor",
            vec![ReplValue::List(nodes), policy.clone()]
        )
        .is_err());
    }
    assert!(call(
        &mut runtime,
        1,
        "fault.classify_heartbeat",
        vec![policy.clone(), string("a"), int(2), int(2), string("early")]
    )
    .is_err());
    let recovery = call(
        &mut runtime,
        1,
        "fault.start_recovery",
        vec![string("a"), int(5), string("start")],
    )
    .unwrap();
    assert!(call(
        &mut runtime,
        1,
        "fault.complete_recovery",
        vec![recovery.clone(), int(4), string("stale")]
    )
    .is_err());
    assert!(call(
        &mut runtime,
        1,
        "fault.expire_recovery",
        vec![policy, recovery, int(14), string("early")]
    )
    .is_err());
    assert!(call(
        &mut runtime,
        1,
        "fault.stale_placement_update",
        vec![
            string("actor"),
            string("a"),
            int(5),
            int(5),
            int(10),
            string("equal")
        ]
    )
    .is_err());
}

#[test]
fn routing_owns_resources_until_actor_completion() {
    let mut helpers = super::super::VmPackageNativeHelpers::default();
    let policy = helpers
        .call(
            3,
            &request("fault.policy", vec![int(2), int(5), int(9)]),
            &[],
        )
        .unwrap();
    let monitor = helpers
        .call(
            3,
            &request(
                "fault.monitor",
                vec![ReplValue::List(vec![string("a")]), policy],
            ),
            &[],
        )
        .unwrap();
    helpers.close_owner(3);
    assert!(helpers
        .call(
            3,
            &request("fault.state_name", vec![monitor, string("a")]),
            &[]
        )
        .is_err());
}

#[test]
fn migration_rejects_stale_descriptors_without_changing_either_plan() {
    let mut runtime = VmSchedulingRuntime::default();
    let mut nodes = Vec::new();
    for id in ["a", "b"] {
        nodes.push(
            call(
                &mut runtime,
                1,
                "scheduler.node",
                vec![string(id), string("active")],
            )
            .unwrap(),
        );
    }
    let original = call(
        &mut runtime,
        1,
        "scheduler.new",
        vec![ReplValue::List(nodes)],
    )
    .unwrap();
    let result = call(
        &mut runtime,
        1,
        "scheduler.request_migration",
        vec![
            original.clone(),
            string("actor"),
            string("a"),
            string("b"),
            ReplValue::Bool(true),
        ],
    )
    .unwrap();
    let requested = call(&mut runtime, 1, "scheduler.migration", vec![result.clone()]).unwrap();
    let plan = call(
        &mut runtime,
        1,
        "scheduler.migration_scheduler",
        vec![result],
    )
    .unwrap();
    let result = call(
        &mut runtime,
        1,
        "scheduler.advance_migration",
        vec![plan.clone(), requested.clone(), string("snapshotting")],
    )
    .unwrap();
    let advanced = call(
        &mut runtime,
        1,
        "scheduler.migration_scheduler",
        vec![result],
    )
    .unwrap();
    assert!(call(
        &mut runtime,
        1,
        "scheduler.advance_migration",
        vec![advanced.clone(), requested, string("transferring")]
    )
    .is_err());
    for (plan, expected) in [(original, 0), (plan, 1), (advanced, 2)] {
        let events = call(
            &mut runtime,
            1,
            "scheduler.events_after",
            vec![plan, int(0)],
        )
        .unwrap();
        assert_eq!(list(&events).unwrap().len(), expected);
    }
}
