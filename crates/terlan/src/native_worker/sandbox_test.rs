use super::*;

/// Rejects a host family without a packaged sandbox backend.
#[test]
fn sandbox_policy_rejects_unsupported_host_profile() {
    let error = CapabilitySandboxProfile::for_host(CapabilitySandboxHost::MacOs)
        .expect_err("unsupported host profile");

    assert!(error.contains("external capability workers are unavailable"));
}

/// Parses exact soft and hard limits while rejecting drift and malformed data.
#[test]
fn sandbox_limit_attestation_is_exact() {
    let limits = concat!(
        "Limit                     Soft Limit           Hard Limit           Units\n",
        "Max cpu time              60                   60                   seconds\n",
        "Max open files            64                   64                   files\n"
    );

    assert!(require_limit(limits, "Max cpu time", 60).is_ok());
    assert!(require_limit(limits, "Max open files", 64).is_ok());
    assert!(require_limit(limits, "Max cpu time", 61).is_err());
    assert!(require_limit(limits, "Max address space", 1).is_err());
    assert!(require_limit(
        "Max cpu time unlimited unlimited seconds\n",
        "Max cpu time",
        60
    )
    .is_err());
}

/// Rejects every inherited descriptor while permitting the proc iterator itself.
#[test]
fn sandbox_file_descriptor_attestation_rejects_inherited_resources() {
    assert!(validate_file_descriptors(&BTreeSet::from([0, 1, 2])).is_ok());
    assert!(validate_file_descriptors(&BTreeSet::from([0, 1, 2, 3])).is_ok());
    assert!(validate_file_descriptors(&BTreeSet::from([0, 1, 2, 3, 9])).is_err());
    assert!(validate_file_descriptors(&BTreeSet::from([0, 1])).is_err());
}
