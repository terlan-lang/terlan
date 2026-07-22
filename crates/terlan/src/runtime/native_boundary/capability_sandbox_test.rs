//! Capability-worker platform admission tests.

use super::*;

/// Proves only the implemented Linux backend is admitted in this release.
#[test]
fn platform_admission_has_no_weak_fallback() {
    assert_eq!(
        CapabilitySandboxProfile::for_host(CapabilitySandboxHost::Linux),
        Ok(CapabilitySandboxProfile::LinuxBwrapV1)
    );
    for host in [
        CapabilitySandboxHost::MacOs,
        CapabilitySandboxHost::Windows,
        CapabilitySandboxHost::Other,
    ] {
        let error = CapabilitySandboxProfile::for_host(host).expect_err("unsupported backend");
        assert!(error.contains("external capability workers are unavailable"));
        assert!(error.contains(host.name()));
    }
}

/// Proves profile identity and host ownership cannot drift independently.
#[test]
fn linux_profile_has_stable_identity_and_host() {
    let profile = CapabilitySandboxProfile::LinuxBwrapV1;

    assert_eq!(profile.name(), LINUX_BWRAP_PROFILE);
    assert_eq!(profile.host(), CapabilitySandboxHost::Linux);
}
