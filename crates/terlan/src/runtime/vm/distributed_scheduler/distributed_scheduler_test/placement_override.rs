use super::*;

#[test]
fn vm_distributed_scheduler_resolves_actor_group_route_and_default_policy_precedence() {
    let mut scheduler = two_node_scheduler();
    scheduler
        .declare_route_policy(
            "/rooms",
            VmPlacementPolicy::Pinned {
                node_id: "node-b".to_string(),
            },
        )
        .expect("route policy");
    scheduler
        .declare_actor_group_policy(
            "/rooms",
            "moderators",
            VmPlacementPolicy::Pinned {
                node_id: "node-a".to_string(),
            },
        )
        .expect("actor-group policy");

    let route = scheduler
        .place_for_route("actor-route", "/rooms", &VmPlacementPolicy::RoundRobin)
        .expect("route placement");
    assert_eq!(route.node_id, "node-b");
    assert_eq!(route.policy, "pinned");

    let group = scheduler
        .place_for_actor_group(
            "actor-group",
            "/rooms",
            "moderators",
            &VmPlacementPolicy::RoundRobin,
        )
        .expect("actor-group placement");
    assert_eq!(group.node_id, "node-a");
    assert_eq!(group.policy, "pinned");

    let inherited_route = scheduler
        .place_for_actor_group(
            "actor-member",
            "/rooms",
            "members",
            &VmPlacementPolicy::RoundRobin,
        )
        .expect("route-inherited placement");
    assert_eq!(inherited_route.node_id, "node-b");

    let default = scheduler
        .place_for_route("actor-default", "/health", &VmPlacementPolicy::RoundRobin)
        .expect("default placement");
    assert_eq!(default.node_id, "node-a");
    assert_eq!(default.policy, "round_robin");
}

#[test]
fn vm_distributed_scheduler_replays_identical_overrides_and_rejects_conflicts() {
    let mut scheduler = two_node_scheduler();
    let pinned = VmPlacementPolicy::Pinned {
        node_id: "node-a".to_string(),
    };
    scheduler
        .declare_route_policy("/jobs", pinned.clone())
        .expect("initial route policy");
    scheduler
        .declare_route_policy("/jobs", pinned)
        .expect("identical route policy replay");

    let error = scheduler
        .declare_route_policy("/jobs", VmPlacementPolicy::LeastConnections)
        .expect_err("conflicting route policy must fail");
    assert!(error.contains("conflicting route policy override"));

    scheduler
        .declare_actor_group_policy("/jobs", "workers", VmPlacementPolicy::LeastConnections)
        .expect("initial actor-group policy");
    let error = scheduler
        .declare_actor_group_policy("/jobs", "workers", VmPlacementPolicy::RoundRobin)
        .expect_err("conflicting actor-group policy must fail");
    assert!(error.contains("conflicting actor-group policy override"));
}

#[test]
fn vm_distributed_scheduler_rejects_invalid_override_scopes_and_policies() {
    let mut scheduler = two_node_scheduler();
    assert!(scheduler
        .declare_route_policy("", VmPlacementPolicy::RoundRobin)
        .expect_err("empty route must fail")
        .contains("route id must be non-empty"));
    assert!(scheduler
        .declare_actor_group_policy("/jobs", "", VmPlacementPolicy::RoundRobin)
        .expect_err("empty actor group must fail")
        .contains("actor group id must be non-empty"));
    assert!(scheduler
        .declare_route_policy(
            "/jobs",
            VmPlacementPolicy::Pinned {
                node_id: String::new(),
            },
        )
        .expect_err("empty pinned node must fail")
        .contains("pinned policy node id must be non-empty"));
    assert!(scheduler
        .place_for_route("actor-a", "", &VmPlacementPolicy::RoundRobin)
        .expect_err("empty placement route must fail")
        .contains("route id must be non-empty"));
}
