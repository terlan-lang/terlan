use super::{CoreRuntimeCapability, CoreType};

/// Returns the Core return type for a runtime capability.
///
/// Inputs:
/// - `capability`: compiler-owned runtime capability identity.
///
/// Output:
/// - Backend-neutral `CoreType` result expected from the capability call.
///
/// Transformation:
/// - Encodes the runtime capability registry's output column as CoreIR type
///   payloads so target lowering can validate effectful operation results
///   without re-reading source signatures.
pub(super) fn core_runtime_capability_return_type(capability: &CoreRuntimeCapability) -> CoreType {
    match capability {
        CoreRuntimeCapability::ConsolePrintln => CoreType::Named("Unit".to_string()),
        CoreRuntimeCapability::FileExists => CoreType::Bool,
        CoreRuntimeCapability::FileReadText => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::String,
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreRuntimeCapability::FileWriteText
        | CoreRuntimeCapability::FileAppendText
        | CoreRuntimeCapability::FileDelete => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Unit".to_string()),
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
    }
}
