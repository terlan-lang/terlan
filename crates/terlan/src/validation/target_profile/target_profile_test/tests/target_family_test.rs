use super::*;

/// Verifies implemented target profiles report their coarse backend family.
///
/// Inputs:
/// - Representative BEAM, JS, and Core target-profile variants.
///
/// Output:
/// - Test assertion only; each profile must map to the expected family.
///
/// Transformation:
/// - Exercises the target-family classifier used by build dispatch gates.
#[test]
fn target_family_groups_supported_profiles() {
    assert_eq!(TargetProfile::Erlang.family(), TargetFamily::Beam);
    assert_eq!(TargetProfile::A021Erlang.family(), TargetFamily::Beam);
    assert_eq!(TargetProfile::JsShared.family(), TargetFamily::Js);
    assert_eq!(TargetProfile::JsBrowser.family(), TargetFamily::Js);
    assert_eq!(TargetProfile::JsWorker.family(), TargetFamily::Js);
    assert_eq!(TargetProfile::CoreV0.family(), TargetFamily::Core);
}

/// Verifies reserved Wasm and WASI CLI target names classify by family.
///
/// Inputs:
/// - Future Wasm and WASI build target spellings.
///
/// Output:
/// - Test assertion only; reserved target names must report their intended
///   runtime family before implementation exists.
///
/// Transformation:
/// - Locks the JS/Wasm boundary so reserved Wasm/WASI targets cannot be treated
///   as generic JS target aliases.
#[test]
fn target_family_classifies_reserved_wasm_wasi_targets() {
    for target in [
        "wasm",
        "wasm.core",
        "wasm.browser",
        "wasm.component",
        "wasm.worker",
    ] {
        assert_eq!(
            TargetFamily::reserved_target(target),
            Some(TargetFamily::Wasm)
        );
    }

    for target in ["wasi", "wasi.cli", "wasi.http", "wasi.worker"] {
        assert_eq!(
            TargetFamily::reserved_target(target),
            Some(TargetFamily::Wasi)
        );
    }

    assert_eq!(TargetFamily::reserved_target("js.browser"), None);
    assert_eq!(TargetFamily::reserved_target("erlang"), None);
}
