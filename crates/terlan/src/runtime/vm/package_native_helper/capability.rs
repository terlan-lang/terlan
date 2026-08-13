use crate::runtime::vm::pure_native::PureNativeCapabilityRequest;
use crate::runtime::vm::VmRuntimeResult;
use crate::terlan_native_boundary::dispatch::{dispatch, NativeBoundaryValue};
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

pub(crate) fn dispatch_vm_capability(
    request: &PureNativeCapabilityRequest,
) -> VmRuntimeResult<NativeBoundaryReplyTerm> {
    dispatch_vm_capability_with_program_arguments(request, &[])
}

/// Dispatches one built-in capability against immutable VM application input.
pub(crate) fn dispatch_vm_capability_with_program_arguments(
    request: &PureNativeCapabilityRequest,
    program_arguments: &[String],
) -> VmRuntimeResult<NativeBoundaryReplyTerm> {
    match request.operation.as_str() {
        "std.system.arguments.count" => {
            let count = i64::try_from(program_arguments.len()).map_err(|_| {
                "error[system.arguments.overflow]: argument count exceeds Terlan Int".to_string()
            })?;
            return Ok(NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Int(count)));
        }
        "std.system.arguments.get" => {
            let Some(NativeBoundaryTerm::Int(index)) = request.arguments.first() else {
                return Err("error[system.arguments.type]: argument index must be Int".into());
            };
            let value = usize::try_from(*index)
                .ok()
                .and_then(|index| program_arguments.get(index))
                .cloned();
            return Ok(NativeBoundaryReplyTerm::Ok(
                NativeBoundaryTerm::OptionalText(value),
            ));
        }
        "std.system.environment.get" => {
            let Some(NativeBoundaryTerm::Text(key)) = request.arguments.first() else {
                return Err(
                    "error[system.environment.type]: environment key must be String".into(),
                );
            };
            return Ok(NativeBoundaryReplyTerm::Ok(
                NativeBoundaryTerm::OptionalText(std::env::var(key).ok()),
            ));
        }
        _ => {}
    }
    let arguments = request
        .arguments
        .iter()
        .map(|argument| boundary_term_to_value(&request.operation, argument))
        .collect::<Result<Vec<_>, _>>()?;
    if matches!(
        request.operation.as_str(),
        "std.io.file.read_text"
            | "std.io.file.read_bytes"
            | "std.io.file.size"
            | "std.io.file.timestamps"
            | "std.io.file.set_timestamps"
            | "std.io.file.is_executable"
            | "std.io.file.set_executable"
            | "std.io.file.copy"
            | "std.io.file.copy_many"
            | "std.io.file.read_text_many"
            | "std.io.file.read_text_directory"
            | "std.io.file.read_text_tree_excluding"
            | "std.io.file.read_text_tree_matching"
            | "std.io.file.write_text"
            | "std.io.file.append_text"
            | "std.io.file.delete"
            | "std.crypto.hash.sha256_file"
            | "std.crypto.hash.verify_sha256_manifest"
            | "std.crypto.hash.sha256_tree"
            | "std.crypto.hash.sha256_selected_files"
            | "std.crypto.hash.sha256_labeled_file_digests"
            | "std.crypto.hash.sha256_labeled_file_contents"
            | "std.crypto.hash.audit_labeled_files"
            | "std.crypto.hash.audit_labeled_file_patterns"
    ) {
        return Ok(NativeBoundaryReplyTerm::Ok(dispatch_file_result(
            &request.operation,
            &arguments,
        )?));
    }
    if matches!(
        request.operation.as_str(),
        "std.io.directory.entries"
            | "std.io.directory.files_recursive"
            | "std.io.directory.files_recursive_excluding"
            | "std.io.directory.find_named_recursive_excluding"
            | "std.io.directory.tree_usage"
            | "std.io.directory.copy_tree_excluding"
            | "std.io.directory.create_symbolic_link"
            | "std.io.directory.create_all"
            | "std.io.directory.create_temporary"
            | "std.io.directory.remove_all"
    ) {
        return Ok(NativeBoundaryReplyTerm::Ok(dispatch_directory_result(
            &request.operation,
            &arguments,
        )?));
    }
    if matches!(
        request.operation.as_str(),
        "std.io.archive.create" | "std.io.archive.extract"
    ) {
        return Ok(NativeBoundaryReplyTerm::Ok(dispatch_archive_result(
            &request.operation,
            &arguments,
        )?));
    }
    if request.operation == "std.vcs.git.source_tree_identity" {
        return Ok(NativeBoundaryReplyTerm::Ok(dispatch_git_result(
            &request.operation,
            &arguments,
        )?));
    }
    let value = dispatch(&request.operation, &arguments).map_err(|error| {
        format!(
            "error[capability.{}]: {} at byte {}",
            error.code(),
            error.message(),
            error.offset()
        )
    })?;
    let term = boundary_value_to_term(&request.operation, value)?;
    Ok(NativeBoundaryReplyTerm::Ok(term))
}

fn dispatch_git_result(
    operation: &str,
    arguments: &[NativeBoundaryValue],
) -> VmRuntimeResult<NativeBoundaryTerm> {
    match dispatch(operation, arguments) {
        Ok(value) => Ok(NativeBoundaryTerm::Record {
            name: "Ok".to_string(),
            fields: vec![(
                "value".to_string(),
                boundary_value_to_term(operation, value)?,
            )],
        }),
        Err(error) => Ok(NativeBoundaryTerm::Record {
            name: "Err".to_string(),
            fields: vec![(
                "reason".to_string(),
                NativeBoundaryTerm::Record {
                    name: "GitError".to_string(),
                    fields: vec![
                        (
                            "code".to_string(),
                            NativeBoundaryTerm::Atom(error.code().to_string()),
                        ),
                        (
                            "message".to_string(),
                            NativeBoundaryTerm::Text(error.message().to_string()),
                        ),
                    ],
                },
            )],
        }),
    }
}

fn dispatch_archive_result(
    operation: &str,
    arguments: &[NativeBoundaryValue],
) -> VmRuntimeResult<NativeBoundaryTerm> {
    let (archive, destination) = match arguments {
        [NativeBoundaryValue::Text(first), NativeBoundaryValue::Text(second)] => {
            if operation == "std.io.archive.create" {
                (second.clone(), first.clone())
            } else {
                (first.clone(), second.clone())
            }
        }
        _ => {
            return Err(format!(
                "error[vm.capability_argument]: capability `{operation}` requires archive and destination String paths"
            ).into())
        }
    };
    match dispatch(operation, arguments) {
        Ok(value) => Ok(NativeBoundaryTerm::Record {
            name: "Ok".to_string(),
            fields: vec![(
                "value".to_string(),
                boundary_value_to_term(operation, value)?,
            )],
        }),
        Err(error) => {
            let code = match error.code() {
                "archive.unsupported_format" => "unsupported_format",
                "archive.invalid_archive" => "invalid_archive",
                "archive.unsafe_entry" => "unsafe_entry",
                "archive.destination_exists" => "destination_exists",
                _ => "io_failure",
            };
            Ok(NativeBoundaryTerm::Record {
                name: "Err".to_string(),
                fields: vec![(
                    "reason".to_string(),
                    NativeBoundaryTerm::Record {
                        name: "ArchiveError".to_string(),
                        fields: vec![
                            (
                                "code".to_string(),
                                NativeBoundaryTerm::Atom(code.to_string()),
                            ),
                            (
                                "message".to_string(),
                                NativeBoundaryTerm::Text(error.message().to_string()),
                            ),
                            ("archive".to_string(), NativeBoundaryTerm::Text(archive)),
                            (
                                "destination".to_string(),
                                NativeBoundaryTerm::Text(destination),
                            ),
                        ],
                    },
                )],
            })
        }
    }
}

fn dispatch_file_result(
    operation: &str,
    arguments: &[NativeBoundaryValue],
) -> VmRuntimeResult<NativeBoundaryTerm> {
    let path = primary_file_path(arguments.first()).ok_or_else(|| {
        format!(
            "error[vm.capability_argument]: capability `{operation}` requires a String path or typed file batch"
        )
    })?;
    match dispatch(operation, arguments) {
        Ok(value) => Ok(NativeBoundaryTerm::Record {
            name: "Ok".to_string(),
            fields: vec![(
                "value".to_string(),
                boundary_value_to_term(operation, value)?,
            )],
        }),
        Err(error) => {
            let error_path = error.path().map(str::to_owned).unwrap_or(path);
            let code = match error.code() {
                "file.not_found" => "not_found",
                "file.permission_denied" => "permission_denied",
                "file.invalid_path" => "invalid_path",
                _ => "unknown",
            };
            let message = match code {
                "not_found" => "file not found".to_string(),
                "permission_denied" => "permission denied".to_string(),
                "invalid_path" => "invalid path".to_string(),
                _ => error.message().to_string(),
            };
            Ok(NativeBoundaryTerm::Record {
                name: "Err".to_string(),
                fields: vec![(
                    "reason".to_string(),
                    NativeBoundaryTerm::Record {
                        name: "FileError".to_string(),
                        fields: vec![
                            (
                                "code".to_string(),
                                NativeBoundaryTerm::Atom(code.to_string()),
                            ),
                            ("message".to_string(), NativeBoundaryTerm::Text(message)),
                            ("path".to_string(), NativeBoundaryTerm::Text(error_path)),
                        ],
                    },
                )],
            })
        }
    }
}

fn primary_file_path(argument: Option<&NativeBoundaryValue>) -> Option<String> {
    match argument {
        Some(NativeBoundaryValue::Text(path)) => Some(path.clone()),
        Some(NativeBoundaryValue::List(values)) => values
            .first()
            .and_then(|value| match value {
                NativeBoundaryValue::Text(path) => Some(path.clone()),
                NativeBoundaryValue::Record { fields, .. } => {
                    fields
                        .iter()
                        .find_map(|(name, value)| match (name.as_str(), value) {
                            ("path" | "source", NativeBoundaryValue::Text(path)) => {
                                Some(path.clone())
                            }
                            _ => None,
                        })
                }
                _ => None,
            })
            .or_else(|| values.is_empty().then(String::new)),
        _ => None,
    }
}

fn dispatch_directory_result(
    operation: &str,
    arguments: &[NativeBoundaryValue],
) -> VmRuntimeResult<NativeBoundaryTerm> {
    let path = match arguments.first() {
        Some(NativeBoundaryValue::Text(path)) => path.clone(),
        _ => {
            return Err(format!(
                "error[vm.capability_argument]: capability `{operation}` requires a String path"
            )
            .into())
        }
    };
    match dispatch(operation, arguments) {
        Ok(value) => Ok(NativeBoundaryTerm::Record {
            name: "Ok".to_string(),
            fields: vec![(
                "value".to_string(),
                boundary_value_to_term(operation, value)?,
            )],
        }),
        Err(error) => {
            let code = match error.code() {
                "directory.not_found" => "not_found",
                "directory.permission_denied" => "permission_denied",
                "directory.invalid_path" => "invalid_path",
                _ => "unknown",
            };
            let message = match code {
                "not_found" => "directory not found".to_string(),
                "permission_denied" => "permission denied".to_string(),
                "invalid_path" => "invalid path".to_string(),
                _ => error.message().to_string(),
            };
            Ok(NativeBoundaryTerm::Record {
                name: "Err".to_string(),
                fields: vec![(
                    "reason".to_string(),
                    NativeBoundaryTerm::Record {
                        name: "DirectoryError".to_string(),
                        fields: vec![
                            (
                                "code".to_string(),
                                NativeBoundaryTerm::Atom(code.to_string()),
                            ),
                            ("message".to_string(), NativeBoundaryTerm::Text(message)),
                            ("path".to_string(), NativeBoundaryTerm::Text(path)),
                        ],
                    },
                )],
            })
        }
    }
}

fn boundary_term_to_value(
    operation: &str,
    term: &NativeBoundaryTerm,
) -> VmRuntimeResult<NativeBoundaryValue> {
    match term {
        NativeBoundaryTerm::Unit => Ok(NativeBoundaryValue::Unit),
        NativeBoundaryTerm::Text(value) => Ok(NativeBoundaryValue::Text(value.clone())),
        NativeBoundaryTerm::Bytes(value) => Ok(NativeBoundaryValue::Bytes(value.clone())),
        NativeBoundaryTerm::Int(value) => Ok(NativeBoundaryValue::Int(*value)),
        NativeBoundaryTerm::Float(value) => Ok(NativeBoundaryValue::Float(*value)),
        NativeBoundaryTerm::Bool(value) => Ok(NativeBoundaryValue::Bool(*value)),
        NativeBoundaryTerm::Atom(value) => Ok(NativeBoundaryValue::Atom(value.clone())),
        NativeBoundaryTerm::Record { name, fields } => Ok(NativeBoundaryValue::Record {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(name, value)| {
                    boundary_term_to_value(operation, value).map(|value| (name.clone(), value))
                })
                .collect::<Result<Vec<_>, _>>()?,
        }),
        NativeBoundaryTerm::List(values) => Ok(NativeBoundaryValue::List(
            values
                .iter()
                .map(|value| boundary_term_to_value(operation, value))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        unsupported => Err(format!(
            "error[vm.capability_argument]: capability `{operation}` cannot dispatch argument `{unsupported:?}`"
        ).into()),
    }
}

fn boundary_value_to_term(
    operation: &str,
    value: NativeBoundaryValue,
) -> VmRuntimeResult<NativeBoundaryTerm> {
    match value {
        NativeBoundaryValue::Unit => Ok(NativeBoundaryTerm::Unit),
        NativeBoundaryValue::Text(value) => Ok(NativeBoundaryTerm::Text(value)),
        NativeBoundaryValue::Bytes(value) => Ok(NativeBoundaryTerm::Bytes(value)),
        NativeBoundaryValue::Int(value) => Ok(NativeBoundaryTerm::Int(value)),
        NativeBoundaryValue::Float(value) => Ok(NativeBoundaryTerm::Float(value)),
        NativeBoundaryValue::Bool(value) => Ok(NativeBoundaryTerm::Bool(value)),
        NativeBoundaryValue::Atom(value) => Ok(NativeBoundaryTerm::Atom(value)),
        NativeBoundaryValue::Record { name, fields } => Ok(NativeBoundaryTerm::Record {
            name,
            fields: fields
                .into_iter()
                .map(|(name, value)| {
                    boundary_value_to_term(operation, value).map(|value| (name, value))
                })
                .collect::<Result<Vec<_>, _>>()?,
        }),
        NativeBoundaryValue::List(values) => Ok(NativeBoundaryTerm::List(
            values
                .into_iter()
                .map(|value| boundary_value_to_term(operation, value))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        NativeBoundaryValue::OptionalText(value) => Ok(NativeBoundaryTerm::OptionalText(value)),
        unsupported => Err(format!(
            "error[vm.capability_result]: capability `{operation}` returned unsupported value `{unsupported:?}`"
        ).into()),
    }
}
