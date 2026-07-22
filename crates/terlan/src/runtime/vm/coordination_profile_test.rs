use super::VmCoordinationProfile;

fn profile(
    app_id: &str,
    vm_id: &str,
    node_id: &str,
    cluster_id: &str,
    epoch: u64,
    runtime_version: &str,
    capability: &str,
) -> Result<VmCoordinationProfile, String> {
    VmCoordinationProfile::new(
        app_id,
        vm_id,
        node_id,
        cluster_id,
        epoch,
        runtime_version,
        [capability],
    )
}

#[test]
fn coordination_profile_preserves_valid_identity_and_capabilities() {
    let profile = VmCoordinationProfile::new(
        "app",
        "vm-a",
        "node-a",
        "cluster-a",
        7,
        "0.0.7",
        ["message", "message", "storage"],
    )
    .expect("valid profile");

    assert_eq!(profile.app_id(), "app");
    assert_eq!(profile.vm_id(), "vm-a");
    assert_eq!(profile.node_id(), "node-a");
    assert_eq!(profile.epoch(), 7);
    assert!(profile.has_capabilities(["message", "storage"]));
    assert!(!profile.has_capabilities(["missing"]));
}

#[test]
fn coordination_profile_rejects_empty_identity_fields() {
    for (field, args) in [
        ("app_id", [" ", "vm", "node", "cluster", "0.0.7"]),
        ("vm_id", ["app", " ", "node", "cluster", "0.0.7"]),
        ("node_id", ["app", "vm", " ", "cluster", "0.0.7"]),
        ("cluster_id", ["app", "vm", "node", " ", "0.0.7"]),
        ("runtime_version", ["app", "vm", "node", "cluster", " "]),
    ] {
        assert_eq!(
            profile(args[0], args[1], args[2], args[3], 1, args[4], "message")
                .expect_err("empty identity must fail"),
            format!("error[vm_coordination_profile]: `{field}` must not be empty")
        );
    }
}

#[test]
fn coordination_profile_rejects_zero_epoch_and_empty_capability_names() {
    assert_eq!(
        profile("app", "vm", "node", "cluster", 0, "0.0.7", "message")
            .expect_err("zero epoch must fail"),
        "error[vm_coordination_profile]: `epoch` must be non-zero"
    );
    assert_eq!(
        profile("app", "vm", "node", "cluster", 1, "0.0.7", " ")
            .expect_err("empty capability must fail"),
        "error[vm_coordination_profile]: capability names must not be empty"
    );
}
