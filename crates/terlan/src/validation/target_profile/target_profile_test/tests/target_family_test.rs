use super::*;

/// Verifies implemented target profiles report their coarse backend family.
///
/// Inputs:
/// - Representative VM, JS, and Core target-profile variants.
///
/// Output:
/// - Test assertion only; each profile must map to the expected family.
///
/// Transformation:
/// - Exercises the target-family classifier used by build dispatch gates.
#[test]
fn target_family_groups_supported_profiles() {
    assert_eq!(TargetProfile::Vm.family(), TargetFamily::Vm);
    assert_eq!(TargetProfile::A021Vm.family(), TargetFamily::Vm);
    assert_eq!(TargetProfile::JsShared.family(), TargetFamily::Js);
    assert_eq!(TargetProfile::JsBrowser.family(), TargetFamily::Js);
    assert_eq!(TargetProfile::JsWorker.family(), TargetFamily::Js);
    assert_eq!(TargetProfile::WasmCore.family(), TargetFamily::Wasm);
    assert_eq!(TargetProfile::CoreV0.family(), TargetFamily::Core);
}

/// Verifies reserved target names classify by their intended backend family.
///
/// Inputs:
/// - Future Wasm, WASI, mobile, and native constrained target spellings.
///
/// Output:
/// - Test assertion only; reserved target names must report their intended
///   runtime family before implementation exists.
///
/// Transformation:
/// - Locks the JS/Wasm/mobile/native boundary so reserved future targets cannot
///   be treated as generic JS target aliases.
#[test]
fn target_family_classifies_reserved_future_targets() {
    for target in ["wasm", "wasm.browser", "wasm.component", "wasm.worker"] {
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

    for target in ["mobile", "mobile.shell", "mobile.android", "mobile.ios"] {
        assert_eq!(
            TargetFamily::reserved_target(target),
            Some(TargetFamily::Mobile)
        );
    }

    for target in [
        "native.no-std",
        "native.bare-metal",
        "native.kernel",
        "native.rtos",
        "native.riscv",
        "native.arm",
    ] {
        assert_eq!(
            TargetFamily::reserved_target(target),
            Some(TargetFamily::NativeConstrained)
        );
    }

    assert_eq!(TargetFamily::reserved_target("js.browser"), None);
    assert_eq!(TargetFamily::reserved_target("wasm.core"), None);
    assert_eq!(TargetFamily::reserved_target("erlang"), None);
}
