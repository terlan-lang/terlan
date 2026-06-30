use super::{core_runtime_capability_return_type, CoreRuntimeCapability, CoreType};

/// Builds the CoreIR `Result[success, std.io.File.FileError]` type.
///
/// Inputs:
/// - `success`: success payload type.
///
/// Output:
/// - CoreIR result application type.
///
/// Transformation:
/// - Reuses the same error payload expected by file runtime capabilities.
fn file_result(success: CoreType) -> CoreType {
    CoreType::Apply {
        constructor: "Result".to_owned(),
        args: vec![success, CoreType::Named("std.io.File.FileError".to_owned())],
    }
}

/// Verifies simple runtime capabilities expose stable CoreIR return types.
///
/// Inputs:
/// - Console and file-existence runtime capabilities.
///
/// Output:
/// - Unit and Bool CoreIR return types.
///
/// Transformation:
/// - Checks the runtime capability registry's output column for non-Result
///   capabilities.
#[test]
fn runtime_capability_return_type_maps_simple_capabilities() {
    assert_eq!(
        core_runtime_capability_return_type(&CoreRuntimeCapability::ConsolePrintln),
        CoreType::Named("Unit".to_owned())
    );
    assert_eq!(
        core_runtime_capability_return_type(&CoreRuntimeCapability::FileExists),
        CoreType::Bool
    );
}

/// Verifies text reads return `Result[String, FileError]`.
///
/// Inputs:
/// - File read runtime capability.
///
/// Output:
/// - CoreIR result type with `String` success payload.
///
/// Transformation:
/// - Guards the backend-neutral contract for file read lowering.
#[test]
fn runtime_capability_return_type_maps_file_read_text() {
    assert_eq!(
        core_runtime_capability_return_type(&CoreRuntimeCapability::FileReadText),
        file_result(CoreType::String)
    );
}

/// Verifies file mutation capabilities return `Result[Unit, FileError]`.
///
/// Inputs:
/// - File write, append, and delete runtime capabilities.
///
/// Output:
/// - CoreIR result type with `Unit` success payload.
///
/// Transformation:
/// - Ensures all file mutation operations share the same typed success/error
///   shape.
#[test]
fn runtime_capability_return_type_maps_file_mutations() {
    for capability in [
        CoreRuntimeCapability::FileWriteText,
        CoreRuntimeCapability::FileAppendText,
        CoreRuntimeCapability::FileDelete,
    ] {
        assert_eq!(
            core_runtime_capability_return_type(&capability),
            file_result(CoreType::Named("Unit".to_owned()))
        );
    }
}
