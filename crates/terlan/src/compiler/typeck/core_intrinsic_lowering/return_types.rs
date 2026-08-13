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
        CoreRuntimeCapability::ConsolePrintln | CoreRuntimeCapability::ConsoleEprintln => {
            CoreType::Named("Unit".to_string())
        }
        CoreRuntimeCapability::ClockUnixTimeNs | CoreRuntimeCapability::ClockMonotonicTimeNs => {
            CoreType::Int
        }
        CoreRuntimeCapability::FileExists => CoreType::Bool,
        CoreRuntimeCapability::FileReadText => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::String,
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreRuntimeCapability::FileReadBytes => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("std.vm.Bytes.Bytes".to_string()),
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreRuntimeCapability::FileSize => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Int,
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreRuntimeCapability::FileTimestamps => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("std.io.File.FileTimestamps".to_string()),
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreRuntimeCapability::FileReadTextMany
        | CoreRuntimeCapability::FileReadTextDirectory
        | CoreRuntimeCapability::FileReadTextTreeExcluding
        | CoreRuntimeCapability::FileReadTextTreeMatching => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::List(Box::new(CoreType::Named(
                    "std.io.File.TextFile".to_string(),
                ))),
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreRuntimeCapability::FileWriteText
        | CoreRuntimeCapability::FileAppendText
        | CoreRuntimeCapability::FileDelete
        | CoreRuntimeCapability::FileSetTimestamps
        | CoreRuntimeCapability::FileSetExecutable
        | CoreRuntimeCapability::FileCopy
        | CoreRuntimeCapability::FileCopyMany => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Unit".to_string()),
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreRuntimeCapability::FileIsExecutable => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Bool,
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreRuntimeCapability::SystemArgumentsCount => CoreType::Int,
        CoreRuntimeCapability::SystemArgumentsGet | CoreRuntimeCapability::SystemEnvironmentGet => {
            CoreType::Apply {
                constructor: "Option".to_string(),
                args: vec![CoreType::String],
            }
        }
        CoreRuntimeCapability::SystemEnvironmentCurrentDirectory => CoreType::String,
        CoreRuntimeCapability::SystemEnvironmentContains => CoreType::Bool,
        CoreRuntimeCapability::SystemPlatformCurrentMetrics => {
            CoreType::Named("std.system.Platform.HostMetrics".to_string())
        }
        CoreRuntimeCapability::SystemProcessLimits => {
            CoreType::Named("std.system.Process.Limits".to_string())
        }
        CoreRuntimeCapability::SystemProcessRun => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("std.system.Process.Output".to_string()),
                CoreType::Named("std.system.Process.ProcessError".to_string()),
            ],
        },
        CoreRuntimeCapability::SystemProcessRunMany => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("std.system.Process.BatchOutput".to_string()),
                CoreType::Named("std.system.Process.ProcessError".to_string()),
            ],
        },
        CoreRuntimeCapability::SystemProcessRunLengthFramed => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("std.system.Process.FramedOutput".to_string()),
                CoreType::Named("std.system.Process.ProcessError".to_string()),
            ],
        },
        CoreRuntimeCapability::DirectoryEntries
        | CoreRuntimeCapability::DirectoryFilesRecursive
        | CoreRuntimeCapability::DirectoryFilesRecursiveExcluding
        | CoreRuntimeCapability::DirectoryFindNamedRecursiveExcluding => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::List(Box::new(CoreType::String)),
                CoreType::Named("std.io.Directory.DirectoryError".to_string()),
            ],
        },
        CoreRuntimeCapability::DirectoryTreeUsage => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("std.io.Directory.TreeUsage".to_string()),
                CoreType::Named("std.io.Directory.DirectoryError".to_string()),
            ],
        },
        CoreRuntimeCapability::DirectoryCopyTreeExcluding
        | CoreRuntimeCapability::DirectoryCreateSymbolicLink
        | CoreRuntimeCapability::DirectoryCreateAll
        | CoreRuntimeCapability::DirectoryRemoveAll => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Unit".to_string()),
                CoreType::Named("std.io.Directory.DirectoryError".to_string()),
            ],
        },
        CoreRuntimeCapability::DirectoryCreateTemporary => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::String,
                CoreType::Named("std.io.Directory.DirectoryError".to_string()),
            ],
        },
        CoreRuntimeCapability::ArchiveCreate | CoreRuntimeCapability::ArchiveExtract => {
            CoreType::Apply {
                constructor: "Result".to_string(),
                args: vec![
                    CoreType::Named("Unit".to_string()),
                    CoreType::Named("std.io.Archive.ArchiveError".to_string()),
                ],
            }
        }
        CoreRuntimeCapability::HashSha256File
        | CoreRuntimeCapability::HashSha256Tree
        | CoreRuntimeCapability::HashSha256SelectedFiles
        | CoreRuntimeCapability::HashSha256LabeledFileDigests
        | CoreRuntimeCapability::HashSha256LabeledFileContents => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::String,
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreRuntimeCapability::HashAuditLabeledFiles
        | CoreRuntimeCapability::HashAuditLabeledFilePatterns => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("std.crypto.Hash.LabeledFileAudit".to_string()),
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreRuntimeCapability::HashVerifySha256Manifest => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Bool,
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreRuntimeCapability::GitSourceTreeIdentity => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("std.vcs.Git.SourceTreeIdentity".to_string()),
                CoreType::Named("std.vcs.Git.GitError".to_string()),
            ],
        },
    }
}

#[cfg(test)]
#[path = "return_types_test.rs"]
#[cfg(test)]
mod return_types_test;
