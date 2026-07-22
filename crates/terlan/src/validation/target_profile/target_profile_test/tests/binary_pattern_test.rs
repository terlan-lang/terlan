use super::*;

fn binary_pattern_module() -> CoreModule {
    lower(
        "\
module profile_test_binary_pattern.\n\
\n\
import std.vm.BitString.{BitString}.\n\
\n\
pub decode(value: BitString): Int ->\n\
    case value {\n\
        Binary[big] { port: UInt[16], body: Rest } -> port;\n\
        _ -> 0\n\
    }.\n\
",
        "src/profile_test_binary_pattern.terl",
    )
}

#[test]
fn target_profile_allows_binary_pattern_for_vm_profile() {
    let violations = target_profile_checks(&binary_pattern_module(), TargetProfile::Vm);

    assert!(
        violations.is_empty(),
        "VM profile should allow typed binary patterns: {violations:?}"
    );
}

#[test]
fn target_profile_rejects_binary_pattern_for_js_profiles() {
    for profile in [
        TargetProfile::JsShared,
        TargetProfile::JsBrowser,
        TargetProfile::JsWorker,
    ] {
        let violations = target_profile_checks(&binary_pattern_module(), profile);
        assert!(
            violations.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("BinaryLayout")
            }),
            "{profile:?} should reject VM-owned binary patterns: {violations:?}"
        );
    }
}
