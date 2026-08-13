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
        core_runtime_capability_return_type(&CoreRuntimeCapability::ConsoleEprintln),
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

#[test]
fn runtime_capability_return_type_maps_file_size() {
    assert_eq!(
        core_runtime_capability_return_type(&CoreRuntimeCapability::FileSize),
        file_result(CoreType::Int)
    );
}

#[test]
fn runtime_capability_return_type_maps_file_timestamps() {
    assert_eq!(
        core_runtime_capability_return_type(&CoreRuntimeCapability::FileTimestamps),
        file_result(CoreType::Named("std.io.File.FileTimestamps".to_owned(),))
    );
}

#[test]
fn runtime_capability_return_type_maps_executable_query() {
    assert_eq!(
        core_runtime_capability_return_type(&CoreRuntimeCapability::FileIsExecutable),
        file_result(CoreType::Bool)
    );
}

/// Verifies batch reads return ordered typed file records.
#[test]
fn runtime_capability_return_type_maps_file_read_text_many() {
    for capability in [
        CoreRuntimeCapability::FileReadTextMany,
        CoreRuntimeCapability::FileReadTextDirectory,
        CoreRuntimeCapability::FileReadTextTreeExcluding,
        CoreRuntimeCapability::FileReadTextTreeMatching,
    ] {
        assert_eq!(
            core_runtime_capability_return_type(&capability),
            file_result(CoreType::List(Box::new(CoreType::Named(
                "std.io.File.TextFile".to_owned(),
            ))))
        );
    }
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
        CoreRuntimeCapability::FileSetTimestamps,
        CoreRuntimeCapability::FileSetExecutable,
        CoreRuntimeCapability::FileCopy,
        CoreRuntimeCapability::FileCopyMany,
    ] {
        assert_eq!(
            core_runtime_capability_return_type(&capability),
            file_result(CoreType::Named("Unit".to_owned()))
        );
    }
}

/// Verifies bounded process batches return one nominal ordered envelope.
#[test]
fn runtime_capability_return_type_maps_process_batch() {
    assert_eq!(
        core_runtime_capability_return_type(&CoreRuntimeCapability::SystemProcessRunMany),
        CoreType::Apply {
            constructor: "Result".to_owned(),
            args: vec![
                CoreType::Named("std.system.Process.BatchOutput".to_owned()),
                CoreType::Named("std.system.Process.ProcessError".to_owned()),
            ],
        }
    );
}

/// Verifies framed child sessions retain their nominal typed result envelope.
#[test]
fn runtime_capability_return_type_maps_framed_process_session() {
    assert_eq!(
        core_runtime_capability_return_type(&CoreRuntimeCapability::SystemProcessRunLengthFramed,),
        CoreType::Apply {
            constructor: "Result".to_owned(),
            args: vec![
                CoreType::Named("std.system.Process.FramedOutput".to_owned()),
                CoreType::Named("std.system.Process.ProcessError".to_owned()),
            ],
        }
    );
}

/// Verifies process limits retain their nominal public standard-library type.
#[test]
fn runtime_capability_return_type_maps_process_limits() {
    assert_eq!(
        core_runtime_capability_return_type(&CoreRuntimeCapability::SystemProcessLimits),
        CoreType::Named("std.system.Process.Limits".to_owned())
    );
}

/// Verifies dynamic host snapshots retain their nominal standard-library type.
#[test]
fn runtime_capability_return_type_maps_platform_metrics() {
    assert_eq!(
        core_runtime_capability_return_type(&CoreRuntimeCapability::SystemPlatformCurrentMetrics),
        CoreType::Named("std.system.Platform.HostMetrics".to_owned())
    );
}
