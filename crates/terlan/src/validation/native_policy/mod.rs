/// Native-code policy selected by CLI commands.
///
/// The policy controls whether source files may contain native declarations and
/// whether safe-native support is optional or required for the command.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum NativePolicy {
    Pure,
    #[default]
    SafeNativeOptional,
    SafeNativeRequired,
}

impl NativePolicy {
    /// Returns the CLI/API spelling for this native policy.
    ///
    /// Inputs:
    /// - `self`: the native policy selected by parsed CLI state.
    ///
    /// Output:
    /// - Static string spelling used in JSON metadata and CLI flag parsing.
    ///
    /// Transformation:
    /// - Maps the enum variant to the stable snake-case policy identifier.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            NativePolicy::Pure => "pure",
            NativePolicy::SafeNativeOptional => "safe_native_optional",
            NativePolicy::SafeNativeRequired => "safe_native_required",
        }
    }
}

/// Validates native declarations against the selected policy.
///
/// Inputs:
/// - `source`: Terlan source text to scan.
/// - `policy`: native policy selected for the command.
///
/// Output:
/// - `Ok(())` when native usage is permitted.
/// - `Err(String)` with a CLI-ready policy error when native usage is forbidden.
///
/// Transformation:
/// - Scans source text for safe and unsafe native markers, then compares the
///   discovered usage against the requested policy.
pub(crate) fn validate_native_policy(source: &str, policy: NativePolicy) -> Result<(), String> {
    if source_contains_unsafe_native(source) {
        return Err("unsafe native declarations require an explicit unsafe mode".to_string());
    }
    if source_uses_native(source) && policy == NativePolicy::Pure {
        return Err(
            "native declarations require --native-policy safe_native_optional or safe_native_required"
                .to_string(),
        );
    }
    Ok(())
}

/// Detects whether source text uses safe-native declarations.
///
/// Inputs:
/// - `source`: Terlan source text to scan.
///
/// Output:
/// - `true` when the source declares safe-native target support or a native
///   declaration block.
/// - `false` when no native marker is found.
///
/// Transformation:
/// - Performs a lightweight textual scan that is suitable for early CLI policy
///   checks before deeper compiler phases run.
pub(crate) fn source_uses_native(source: &str) -> bool {
    source.contains("target erlang with safe_native")
        || source.contains("@compiler.native")
        || source
            .lines()
            .any(|line| line.trim_start().starts_with("native "))
}

/// Detects whether source text contains unsafe-native markers.
///
/// Inputs:
/// - `source`: Terlan source text to scan.
///
/// Output:
/// - `true` when unsafe native spelling appears in the source.
/// - `false` when the source does not contain unsafe-native markers.
///
/// Transformation:
/// - Performs a conservative textual scan for the unsafe-native spellings that
///   should not pass the safe-native policy gate.
pub(crate) fn source_contains_unsafe_native(source: &str) -> bool {
    source.contains("unsafe_native")
        || source.contains("unsafe native")
        || source.contains("native unsafe")
}

#[cfg(test)]
#[path = "native_policy_test.rs"]
mod native_policy_test;
