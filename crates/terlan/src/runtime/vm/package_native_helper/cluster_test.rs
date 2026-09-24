//! Direct profile operations must remain actor-owned and outside worker RPC.

use super::*;
use crate::runtime::native_image::TvmBoundaryType;

impl VmClusterRuntime {
    fn call(
        &mut self,
        owner: u64,
        request: &PureNativeCapabilityRequest,
    ) -> VmRuntimeResult<ReplValue> {
        self.call_with_atoms(owner, request, &[])
    }
}

fn request(operation: &str, args: Vec<ReplValue>) -> PureNativeCapabilityRequest {
    PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: format!("std.vm.cluster.{operation}"),
        arguments: Vec::new(),
        package_arguments: Some(args),
        result_type: TvmBoundaryType::Int,
    }
}

fn create(owner: u64, runtime: &mut VmClusterRuntime) -> ReplValue {
    runtime
        .call(
            owner,
            &request(
                "profile",
                ["app", "vm", "node", "cluster", "1", "message"]
                    .into_iter()
                    .map(|s| ReplValue::String(s.into()))
                    .collect(),
            ),
        )
        .unwrap()
}

#[test]
fn profile_updates_preserve_the_original_identity_and_epoch() {
    let mut runtime = VmClusterRuntime::default();
    let original = create(17, &mut runtime);
    let advanced = runtime
        .call(17, &request("profile_next_epoch", vec![original.clone()]))
        .unwrap();
    assert_eq!(
        runtime
            .call(17, &request("profile_epoch", vec![original]))
            .unwrap(),
        ReplValue::Int(1)
    );
    assert_eq!(
        runtime
            .call(17, &request("profile_epoch", vec![advanced.clone()]))
            .unwrap(),
        ReplValue::Int(2)
    );
    assert_eq!(
        runtime
            .call(17, &request("profile_node_id", vec![advanced]))
            .unwrap(),
        ReplValue::String("node".into())
    );
}

#[test]
fn forged_owner_and_generation_fail_without_exposing_the_profile() {
    let mut runtime = VmClusterRuntime::default();
    let original = create(17, &mut runtime);
    assert!(runtime
        .call(18, &request("profile_epoch", vec![original.clone()]))
        .is_err());
    for (field, replacement, owner) in [
        ("$native_owner", ReplValue::String("18".into()), 18),
        ("$native_generation", ReplValue::Int(2), 17),
        (
            "$native_type",
            ReplValue::String("std.data.Json.Json".into()),
            17,
        ),
    ] {
        let mut forged = original.clone();
        let ReplValue::Record { fields, .. } = &mut forged else {
            panic!("opaque profile record")
        };
        fields.iter_mut().find(|(name, _)| name == field).unwrap().1 = replacement;
        assert!(runtime
            .call(owner, &request("profile_epoch", vec![forged]))
            .is_err());
    }
    assert_eq!(
        runtime
            .call(17, &request("profile_epoch", vec![original]))
            .unwrap(),
        ReplValue::Int(1)
    );
}

#[test]
fn owner_cleanup_preserves_other_actors_and_rejects_replays() {
    let mut runtime = VmClusterRuntime::default();
    let first = create(17, &mut runtime);
    let second = create(18, &mut runtime);
    runtime.close_owner(17);
    assert!(runtime
        .call(17, &request("profile_epoch", vec![first]))
        .is_err());
    assert_eq!(
        runtime
            .call(18, &request("profile_epoch", vec![second]))
            .unwrap(),
        ReplValue::Int(1)
    );
}

#[test]
fn unknown_operations_and_bad_arguments_do_not_start_a_worker() {
    let mut helpers = super::super::VmPackageNativeHelpers::default();
    for request in [
        request("missing", vec![]),
        request("profile", vec![]),
        request("profile_epoch", vec![ReplValue::Int(1)]),
    ] {
        assert!(helpers.call(17, &request, &[]).is_err());
        assert!(helpers.helpers.is_empty());
    }
}

#[test]
fn epoch_overflow_is_rejected_without_invalidating_the_original() {
    let mut runtime = VmClusterRuntime::default();
    let profile = VmCoordinationProfile::new(
        "app",
        "vm",
        "node",
        "cluster",
        i64::MAX as u64,
        "1",
        ["message"],
    )
    .unwrap();
    let original = runtime.insert(17, profile).unwrap();
    assert!(runtime
        .call(17, &request("profile_next_epoch", vec![original.clone()]))
        .is_err());
    assert_eq!(
        runtime
            .call(17, &request("profile_epoch", vec![original]))
            .unwrap(),
        ReplValue::Int(i64::MAX)
    );
}

#[test]
fn invalid_profiles_preserve_the_underlying_identity_validation() {
    let mut runtime = VmClusterRuntime::default();
    let args = ["app", "vm", "", "cluster", "1", "message"]
        .into_iter()
        .map(|s| ReplValue::String(s.into()))
        .collect();
    assert!(runtime.call(17, &request("profile", args)).is_err());
}

fn membership(runtime: &mut VmClusterRuntime, owner: u64) -> ReplValue {
    let profile = create(owner, runtime);
    runtime
        .call(
            owner,
            &request("membership", vec![profile, ReplValue::Int(5)]),
        )
        .unwrap()
}

fn peer(runtime: &mut VmClusterRuntime, owner: u64) -> ReplValue {
    runtime
        .call(
            owner,
            &request(
                "profile",
                ["app", "peer-vm", "peer", "cluster", "1", "message"]
                    .into_iter()
                    .map(|s| ReplValue::String(s.into()))
                    .collect(),
            ),
        )
        .unwrap()
}

fn state_of(
    runtime: &mut VmClusterRuntime,
    owner: u64,
    value: &ReplValue,
    node: &str,
) -> ReplValue {
    runtime
        .call(
            owner,
            &request(
                "membership_state",
                vec![value.clone(), ReplValue::String(node.into())],
            ),
        )
        .unwrap()
}

fn singleton(name: &str) -> ReplValue {
    if name != "None" {
        return ReplValue::Atom(name.to_ascii_lowercase());
    }
    ReplValue::Record {
        name: name.into(),
        fields: Vec::new(),
    }
}

#[test]
fn membership_updates_preserve_prior_views_and_timeout_boundaries() {
    let mut runtime = VmClusterRuntime::default();
    let original = membership(&mut runtime, 17);
    let peer = peer(&mut runtime, 17);
    let joined = runtime
        .call(
            17,
            &request(
                "membership_join",
                vec![
                    original.clone(),
                    peer,
                    ReplValue::Int(1),
                    ReplValue::String("leader".into()),
                ],
            ),
        )
        .unwrap();
    let boundary = runtime
        .call(
            17,
            &request("membership_expire", vec![joined.clone(), ReplValue::Int(6)]),
        )
        .unwrap();
    let expired = runtime
        .call(
            17,
            &request("membership_expire", vec![joined.clone(), ReplValue::Int(7)]),
        )
        .unwrap();
    assert_eq!(
        state_of(&mut runtime, 17, &original, "peer"),
        singleton("Missing")
    );
    assert_eq!(
        state_of(&mut runtime, 17, &joined, "peer"),
        singleton("Active")
    );
    assert_eq!(
        state_of(&mut runtime, 17, &boundary, "peer"),
        singleton("Active")
    );
    assert_eq!(
        state_of(&mut runtime, 17, &expired, "peer"),
        singleton("Unreachable")
    );
    assert_eq!(
        state_of(&mut runtime, 17, &expired, "node"),
        singleton("Active")
    );
    assert_eq!(
        runtime
            .call(17, &request("membership_health", vec![joined]))
            .unwrap(),
        singleton("Healthy")
    );
    assert_eq!(
        runtime
            .call(17, &request("membership_health", vec![expired.clone()]))
            .unwrap(),
        singleton("Degraded")
    );
    assert_eq!(
        runtime
            .call(17, &request("membership_leader_hint", vec![expired]))
            .unwrap(),
        singleton("None")
    );
}

#[test]
fn cluster_kind_is_checked_against_storage_not_only_the_claimed_type() {
    let mut runtime = VmClusterRuntime::default();
    let profile = create(17, &mut runtime);
    let member = membership(&mut runtime, 17);
    for (mut value, type_name, short_name, operation) in [
        (profile, MEMBERSHIP, "Membership", "membership_view"),
        (member, PROFILE, "Profile", "profile_epoch"),
    ] {
        let ReplValue::Record { name, fields } = &mut value else {
            panic!("resource handle")
        };
        *name = short_name.into();
        fields
            .iter_mut()
            .find(|(name, _)| name == "$native_type")
            .unwrap()
            .1 = ReplValue::String(type_name.into());
        let error = runtime
            .call(17, &request(operation, vec![value]))
            .unwrap_err();
        assert!(error.to_string().contains("vm.cluster.kind"));
    }
}

#[test]
fn membership_rejects_foreign_handles_and_cleanup_preserves_other_owners() {
    let mut runtime = VmClusterRuntime::default();
    let first = membership(&mut runtime, 17);
    let second = membership(&mut runtime, 18);
    let mut forged = first.clone();
    let ReplValue::Record { fields, .. } = &mut forged else {
        panic!("membership handle")
    };
    fields
        .iter_mut()
        .find(|(name, _)| name == "$native_owner")
        .unwrap()
        .1 = ReplValue::String("18".into());
    assert!(runtime
        .call(18, &request("membership_view", vec![forged]))
        .is_err());
    let foreign_peer = peer(&mut runtime, 18);
    assert!(runtime
        .call(
            17,
            &request(
                "membership_join",
                vec![
                    first.clone(),
                    foreign_peer,
                    ReplValue::Int(1),
                    ReplValue::String("leader".into())
                ]
            )
        )
        .is_err());
    runtime.close_owner(17);
    assert!(runtime
        .call(17, &request("membership_view", vec![first]))
        .is_err());
    assert_eq!(
        state_of(&mut runtime, 18, &second, "node"),
        singleton("Active")
    );
}

#[test]
fn malformed_membership_calls_do_not_start_helpers_or_mutate_the_view() {
    let mut helpers = super::super::VmPackageNativeHelpers::default();
    let member = membership(&mut helpers.cluster, 17);
    for invalid in [
        request("membership_view", vec![]),
        request("membership_unknown", vec![member.clone()]),
        request(
            "membership_expire",
            vec![member.clone(), ReplValue::Int(-1)],
        ),
        request(
            "membership_prune",
            vec![member.clone(), ReplValue::Int(10), ReplValue::Int(0)],
        ),
        request(
            "membership_heartbeat",
            vec![
                member.clone(),
                ReplValue::String("node".into()),
                ReplValue::Bool(true),
            ],
        ),
    ] {
        assert!(helpers.call(17, &invalid, &[]).is_err());
        assert!(helpers.helpers.is_empty());
    }
    assert_eq!(
        state_of(&mut helpers.cluster, 17, &member, "node"),
        singleton("Active")
    );
}
