//! Typed filesystem helpers shared by NativeBoundary operations.

use super::args::{expect_bool, expect_int, expect_text};
use super::{DispatchError, NativeBoundaryValue};

static TEMPORARY_DIRECTORY_SEQUENCE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[path = "filesystem/traversal.rs"]
mod traversal;
pub(super) use traversal::{
    expect_text_list, normalized_host_path, recursive_file_paths, text_files_recursive,
    text_files_recursive_matching,
};

/// Dispatches direct regular-file operations kept outside the root dispatcher.
pub(super) fn dispatch_direct_file_operation(
    operation: &str,
    args: &[NativeBoundaryValue],
) -> Result<NativeBoundaryValue, DispatchError> {
    if operation == "std.io.file.copy_many" {
        return copy_regular_files(args).map(|()| NativeBoundaryValue::Unit);
    }
    let path = expect_text(operation, args, 0)?;
    match operation {
        "std.io.file.exists" => Ok(NativeBoundaryValue::Bool(
            std::path::Path::new(path).exists(),
        )),
        "std.io.file.read_text" => std::fs::read_to_string(path)
            .map(NativeBoundaryValue::Text)
            .map_err(|error| dispatch_file_error(operation, path, error)),
        "std.io.file.read_bytes" => std::fs::read(path)
            .map(NativeBoundaryValue::Bytes)
            .map_err(|error| dispatch_file_error(operation, path, error)),
        "std.io.file.size" => file_size(path),
        "std.io.file.timestamps" => file_timestamps(path),
        "std.io.file.set_timestamps" => {
            let accessed = expect_int(operation, args, 1)?;
            let modified = expect_int(operation, args, 2)?;
            set_file_timestamps(path, accessed, modified).map(|()| NativeBoundaryValue::Unit)
        }
        "std.io.file.is_executable" => file_is_executable(path).map(NativeBoundaryValue::Bool),
        "std.io.file.set_executable" => {
            let executable = expect_bool(operation, args, 1)?;
            set_file_executable(path, executable).map(|()| NativeBoundaryValue::Unit)
        }
        "std.io.file.copy" => {
            let destination = expect_text(operation, args, 1)?;
            copy_regular_file(path, destination).map(|()| NativeBoundaryValue::Unit)
        }
        _ => Err(DispatchError::new(
            "dispatch.unknown_operation",
            format!("unknown direct file operation `{operation}`"),
            0,
        )),
    }
}

fn copy_plan_field<'a>(
    operation: &str,
    fields: &'a [(String, NativeBoundaryValue)],
    field: &str,
    index: usize,
) -> Result<&'a str, DispatchError> {
    fields
        .iter()
        .find_map(|(name, value)| (name == field).then_some(value))
        .and_then(|value| match value {
            NativeBoundaryValue::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .ok_or_else(|| {
            DispatchError::new(
                "boundary.type",
                format!("{operation} argument 0 item {index} must contain String field `{field}`"),
                0,
            )
        })
}

fn copy_regular_files(args: &[NativeBoundaryValue]) -> Result<(), DispatchError> {
    use std::collections::HashSet;

    const OPERATION: &str = "std.io.file.copy_many";
    const MAXIMUM_FILES: usize = 65_536;
    let Some(NativeBoundaryValue::List(values)) = args.first() else {
        return Err(DispatchError::new(
            "boundary.type",
            format!("{OPERATION} argument 0 must be List[CopyPlan]"),
            0,
        ));
    };
    if values.len() > MAXIMUM_FILES {
        return Err(DispatchError::new(
            "file.invalid_path",
            format!("copy batch exceeds {MAXIMUM_FILES} files"),
            0,
        ));
    }

    let mut plans = Vec::with_capacity(values.len());
    let mut destinations = HashSet::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let NativeBoundaryValue::Record { name, fields } = value else {
            return Err(DispatchError::new(
                "boundary.type",
                format!("{OPERATION} argument 0 item {index} must be CopyPlan"),
                0,
            ));
        };
        if name != "CopyPlan" {
            return Err(DispatchError::new(
                "boundary.type",
                format!("{OPERATION} argument 0 item {index} must be CopyPlan"),
                0,
            ));
        }
        let source = copy_plan_field(OPERATION, fields, "source", index)?;
        let destination = copy_plan_field(OPERATION, fields, "destination", index)?;
        if source.is_empty() || destination.is_empty() {
            return Err(DispatchError::new(
                "file.invalid_path",
                "copy source and destination must not be empty",
                0,
            )
            .with_path(destination));
        }
        if !destinations.insert(destination.to_string()) {
            return Err(DispatchError::new(
                "file.invalid_path",
                "copy destinations must be unique",
                0,
            )
            .with_path(destination));
        }
        let metadata = std::fs::metadata(source)
            .map_err(|error| dispatch_file_error(OPERATION, source, error))?;
        if !metadata.is_file() {
            return Err(DispatchError::new(
                "file.invalid_path",
                "copy source must resolve to a regular file",
                0,
            )
            .with_path(source));
        }
        match std::fs::symlink_metadata(destination) {
            Ok(_) => {
                return Err(DispatchError::new(
                    "file.invalid_path",
                    "copy destination must not already exist",
                    0,
                )
                .with_path(destination));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(dispatch_file_error(OPERATION, destination, error)),
        }
        plans.push((source.to_string(), destination.to_string()));
    }

    let mut copied = Vec::with_capacity(plans.len());
    for (source, destination) in &plans {
        if let Some(parent) = std::path::Path::new(destination).parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    for path in copied.iter().rev() {
                        let _ = std::fs::remove_file(path);
                    }
                    return Err(dispatch_file_error(OPERATION, destination, error));
                }
            }
        }
        if let Err(error) = std::fs::copy(source, destination) {
            for path in copied.iter().rev() {
                let _ = std::fs::remove_file(path);
            }
            return Err(dispatch_file_error(OPERATION, destination, error));
        }
        copied.push(destination.clone());
    }
    Ok(())
}

fn copy_regular_file(source: &str, destination: &str) -> Result<(), DispatchError> {
    const OPERATION: &str = "std.io.file.copy";
    let metadata =
        std::fs::metadata(source).map_err(|error| dispatch_file_error(OPERATION, source, error))?;
    if !metadata.is_file() {
        return Err(DispatchError::new(
            "file.invalid_path",
            "copy source must resolve to a regular file",
            0,
        )
        .with_path(source));
    }
    std::fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| dispatch_file_error(OPERATION, destination, error))
}

fn file_size(path: &str) -> Result<NativeBoundaryValue, DispatchError> {
    const OPERATION: &str = "std.io.file.size";
    let metadata =
        std::fs::metadata(path).map_err(|error| dispatch_file_error(OPERATION, path, error))?;
    if !metadata.is_file() {
        return Err(DispatchError::new(
            "file.invalid_path",
            "file size path must resolve to a regular file",
            0,
        )
        .with_path(path));
    }
    i64::try_from(metadata.len())
        .map(NativeBoundaryValue::Int)
        .map_err(|_| {
            DispatchError::new("file.unknown", "file size exceeds the Terlan Int range", 0)
                .with_path(path)
        })
}

/// Atomically creates one unique, bounded-name workspace below the host
/// temporary directory without following or replacing an existing entry.
pub(super) fn create_temporary_directory(prefix: &str) -> Result<String, DispatchError> {
    let valid_prefix = !prefix.is_empty()
        && prefix.len() <= 64
        && prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if !valid_prefix || matches!(prefix, "." | "..") {
        return Err(DispatchError::new(
            "directory.invalid_path",
            "temporary directory prefix must be 1-64 ASCII letters, digits, '.', '-', or '_'",
            0,
        )
        .with_path(prefix));
    }
    let root = std::env::temp_dir();
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    for _ in 0..128 {
        let sequence =
            TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let candidate = root.join(format!(
            "{prefix}-{}-{epoch:x}-{sequence:x}",
            std::process::id()
        ));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(normalized_host_path(&candidate)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                let path = normalized_host_path(&candidate);
                return Err(dispatch_directory_error(
                    "std.io.directory.create_temporary",
                    &path,
                    error,
                ));
            }
        }
    }
    Err(DispatchError::new(
        "directory.unknown",
        "temporary directory identity was exhausted",
        0,
    )
    .with_path(prefix))
}

pub(super) fn dispatch_file_error(
    operation: &str,
    path: &str,
    error: std::io::Error,
) -> DispatchError {
    let code = match error.kind() {
        std::io::ErrorKind::NotFound => "file.not_found",
        std::io::ErrorKind::PermissionDenied => "file.permission_denied",
        std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData => "file.invalid_path",
        _ => "file.unknown",
    };
    DispatchError::new(code, format!("{operation} failed for `{path}`: {error}"), 0).with_path(path)
}

fn system_time_unix_ns(
    operation: &str,
    path: &str,
    value: std::time::SystemTime,
) -> Result<i64, DispatchError> {
    let signed = match value.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => i128::try_from(duration.as_nanos()).unwrap_or(i128::MAX),
        Err(error) => -i128::try_from(error.duration().as_nanos()).unwrap_or(i128::MAX),
    };
    i64::try_from(signed).map_err(|_| {
        DispatchError::new(
            "file.unknown",
            format!("{operation} timestamp exceeds the Terlan Int range"),
            0,
        )
        .with_path(path)
    })
}

fn system_time_from_unix_ns(
    operation: &str,
    path: &str,
    value: i64,
) -> Result<std::time::SystemTime, DispatchError> {
    let magnitude = value.unsigned_abs();
    let duration = std::time::Duration::from_nanos(magnitude);
    let timestamp = if value >= 0 {
        std::time::UNIX_EPOCH.checked_add(duration)
    } else {
        std::time::UNIX_EPOCH.checked_sub(duration)
    };
    timestamp.ok_or_else(|| {
        DispatchError::new(
            "file.invalid_path",
            format!("{operation} timestamp is outside the platform range"),
            0,
        )
        .with_path(path)
    })
}

/// Reads portable nanosecond timestamps without materializing file contents.
pub(super) fn file_timestamps(path: &str) -> Result<NativeBoundaryValue, DispatchError> {
    const OPERATION: &str = "std.io.file.timestamps";
    let metadata =
        std::fs::metadata(path).map_err(|error| dispatch_file_error(OPERATION, path, error))?;
    if !metadata.is_file() {
        return Err(DispatchError::new(
            "file.invalid_path",
            "file timestamp path must resolve to a regular file",
            0,
        )
        .with_path(path));
    }
    let accessed = metadata
        .accessed()
        .map_err(|error| dispatch_file_error(OPERATION, path, error))?;
    let modified = metadata
        .modified()
        .map_err(|error| dispatch_file_error(OPERATION, path, error))?;
    Ok(NativeBoundaryValue::Record {
        name: "FileTimestamps".to_string(),
        fields: vec![
            (
                "accessed_unix_ns".to_string(),
                NativeBoundaryValue::Int(system_time_unix_ns(OPERATION, path, accessed)?),
            ),
            (
                "modified_unix_ns".to_string(),
                NativeBoundaryValue::Int(system_time_unix_ns(OPERATION, path, modified)?),
            ),
        ],
    })
}

/// Restores portable nanosecond timestamps without changing file contents.
pub(super) fn set_file_timestamps(
    path: &str,
    accessed_unix_ns: i64,
    modified_unix_ns: i64,
) -> Result<(), DispatchError> {
    const OPERATION: &str = "std.io.file.set_timestamps";
    let metadata =
        std::fs::metadata(path).map_err(|error| dispatch_file_error(OPERATION, path, error))?;
    if !metadata.is_file() {
        return Err(DispatchError::new(
            "file.invalid_path",
            "file timestamp path must resolve to a regular file",
            0,
        )
        .with_path(path));
    }
    let accessed = system_time_from_unix_ns(OPERATION, path, accessed_unix_ns)?;
    let modified = system_time_from_unix_ns(OPERATION, path, modified_unix_ns)?;
    let file =
        std::fs::File::open(path).map_err(|error| dispatch_file_error(OPERATION, path, error))?;
    let times = std::fs::FileTimes::new()
        .set_accessed(accessed)
        .set_modified(modified);
    file.set_times(times)
        .map_err(|error| dispatch_file_error(OPERATION, path, error))
}

/// Reports whether one regular file has host executable semantics.
pub(super) fn file_is_executable(path: &str) -> Result<bool, DispatchError> {
    const OPERATION: &str = "std.io.file.is_executable";
    let metadata =
        std::fs::metadata(path).map_err(|error| dispatch_file_error(OPERATION, path, error))?;
    if !metadata.is_file() {
        return Err(DispatchError::new(
            "file.invalid_path",
            "executable path must resolve to a regular file",
            0,
        )
        .with_path(path));
    }
    Ok(host_file_is_executable(path, &metadata))
}

/// Changes one regular file's executable marker without changing its contents.
pub(super) fn set_file_executable(path: &str, executable: bool) -> Result<(), DispatchError> {
    const OPERATION: &str = "std.io.file.set_executable";
    let metadata =
        std::fs::metadata(path).map_err(|error| dispatch_file_error(OPERATION, path, error))?;
    if !metadata.is_file() {
        return Err(DispatchError::new(
            "file.invalid_path",
            "executable path must resolve to a regular file",
            0,
        )
        .with_path(path));
    }
    set_host_file_executable(path, metadata, executable)
}

#[cfg(unix)]
fn host_file_is_executable(_path: &str, metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(unix)]
fn set_host_file_executable(
    path: &str,
    metadata: std::fs::Metadata,
    executable: bool,
) -> Result<(), DispatchError> {
    use std::os::unix::fs::PermissionsExt;

    const OPERATION: &str = "std.io.file.set_executable";
    let permissions = metadata.permissions();
    let mode = permissions.mode();
    let next_mode = if executable {
        mode | 0o111
    } else {
        mode & !0o111
    };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(next_mode))
        .map_err(|error| dispatch_file_error(OPERATION, path, error))
}

#[cfg(windows)]
fn host_file_is_executable(path: &str, _metadata: &std::fs::Metadata) -> bool {
    Path::new(path)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "exe" | "com" | "bat" | "cmd"
            )
        })
}

#[cfg(windows)]
fn set_host_file_executable(
    path: &str,
    metadata: std::fs::Metadata,
    executable: bool,
) -> Result<(), DispatchError> {
    if host_file_is_executable(path, &metadata) == executable {
        Ok(())
    } else {
        Err(DispatchError::new(
            "file.invalid_path",
            "Windows executable state is determined by the file extension",
            0,
        )
        .with_path(path))
    }
}

pub(super) fn directory_entries(path: &str) -> Result<Vec<NativeBoundaryValue>, DispatchError> {
    let mut entries = std::fs::read_dir(path)
        .map_err(|error| dispatch_directory_error("std.io.directory.entries", path, error))?
        .map(|entry| {
            entry
                .map(|entry| normalized_host_path(&entry.path()))
                .map_err(|error| dispatch_directory_error("std.io.directory.entries", path, error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    Ok(entries.into_iter().map(NativeBoundaryValue::Text).collect())
}

pub(super) fn directory_files_recursive(
    path: &str,
    excluded_directory_names: &[&str],
) -> Result<Vec<NativeBoundaryValue>, DispatchError> {
    recursive_file_paths(
        path,
        excluded_directory_names,
        "std.io.directory.files_recursive",
    )
    .map(|files| files.into_iter().map(NativeBoundaryValue::Text).collect())
}

/// Finds exact directory basenames without descending into matched trees.
pub(super) fn directory_find_named_recursive_excluding(
    path: &str,
    name: &str,
    excluded_directory_names: &[&str],
) -> Result<Vec<NativeBoundaryValue>, DispatchError> {
    const OPERATION: &str = "std.io.directory.find_named_recursive_excluding";
    if name.is_empty() || matches!(name, "." | "..") || name.contains('/') || name.contains('\\') {
        return Err(DispatchError::new(
            "directory.invalid_path",
            "directory search name must be one nonempty basename",
            0,
        )
        .with_path(path));
    }
    let root = std::path::PathBuf::from(path);
    let metadata = std::fs::symlink_metadata(&root)
        .map_err(|error| dispatch_directory_error(OPERATION, path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(DispatchError::new(
            "directory.invalid_path",
            "directory search root must be a directory and not a symbolic link",
            0,
        )
        .with_path(path));
    }

    let mut pending = vec![root.clone()];
    let mut matches = Vec::new();
    while let Some(current) = pending.pop() {
        let current_text = normalized_host_path(&current);
        let mut entries = std::fs::read_dir(&current)
            .map_err(|error| dispatch_directory_error(OPERATION, &current_text, error))?
            .map(|entry| {
                entry.map_err(|error| dispatch_directory_error(OPERATION, &current_text, error))
            })
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries.into_iter().rev() {
            let entry_name = entry.file_name();
            let Some(entry_name_text) = entry_name.to_str() else {
                return Err(DispatchError::new(
                    "directory.invalid_path",
                    "directory search encountered a non-UTF-8 basename",
                    0,
                )
                .with_path(normalized_host_path(&entry.path())));
            };
            if excluded_directory_names.contains(&entry_name_text) {
                continue;
            }
            let entry_path = entry.path();
            let metadata = std::fs::symlink_metadata(&entry_path).map_err(|error| {
                dispatch_directory_error(OPERATION, &normalized_host_path(&entry_path), error)
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                continue;
            }
            if entry_name_text == name {
                let relative = entry_path.strip_prefix(&root).map_err(|_| {
                    DispatchError::new(
                        "directory.invalid_path",
                        "directory search result escaped its root",
                        0,
                    )
                    .with_path(normalized_host_path(&entry_path))
                })?;
                matches.push(normalized_host_path(relative));
            } else {
                pending.push(entry_path);
            }
        }
    }
    matches.sort();
    Ok(matches.into_iter().map(NativeBoundaryValue::Text).collect())
}

#[derive(Default)]
struct DirectoryTreeUsage {
    logical_file_bytes: u64,
    allocated_bytes: u64,
    regular_file_count: u64,
    directory_count: u64,
    symbolic_link_count: u64,
}

/// Measures one directory tree without following symbolic links.
pub(super) fn directory_tree_usage(path: &str) -> Result<NativeBoundaryValue, DispatchError> {
    const OPERATION: &str = "std.io.directory.tree_usage";
    let root = std::path::PathBuf::from(path);
    let root_metadata = std::fs::symlink_metadata(&root)
        .map_err(|error| dispatch_directory_error(OPERATION, path, error))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(invalid_tree_usage_path(path));
    }

    let mut pending = vec![root];
    let mut usage = DirectoryTreeUsage::default();
    while let Some(current) = pending.pop() {
        let current_text = normalized_host_path(&current);
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|error| dispatch_directory_error(OPERATION, &current_text, error))?;
        add_usage(
            &mut usage.allocated_bytes,
            metadata_allocated_bytes(&metadata),
            path,
        )?;
        if metadata.file_type().is_symlink() {
            add_usage(&mut usage.symbolic_link_count, 1, path)?;
            continue;
        }
        if metadata.is_file() {
            add_usage(&mut usage.logical_file_bytes, metadata.len(), path)?;
            add_usage(&mut usage.regular_file_count, 1, path)?;
            continue;
        }
        if metadata.is_dir() {
            add_usage(&mut usage.directory_count, 1, path)?;
            let entries = std::fs::read_dir(&current)
                .map_err(|error| dispatch_directory_error(OPERATION, &current_text, error))?;
            for entry in entries {
                let entry = entry
                    .map_err(|error| dispatch_directory_error(OPERATION, &current_text, error))?;
                pending.push(entry.path());
            }
        }
    }

    Ok(NativeBoundaryValue::Record {
        name: "TreeUsage".to_string(),
        fields: vec![
            (
                "logical_file_bytes".to_string(),
                NativeBoundaryValue::Int(usage_int(usage.logical_file_bytes, path)?),
            ),
            (
                "allocated_bytes".to_string(),
                NativeBoundaryValue::Int(usage_int(usage.allocated_bytes, path)?),
            ),
            (
                "regular_file_count".to_string(),
                NativeBoundaryValue::Int(usage_int(usage.regular_file_count, path)?),
            ),
            (
                "directory_count".to_string(),
                NativeBoundaryValue::Int(usage_int(usage.directory_count, path)?),
            ),
            (
                "symbolic_link_count".to_string(),
                NativeBoundaryValue::Int(usage_int(usage.symbolic_link_count, path)?),
            ),
        ],
    })
}

fn add_usage(total: &mut u64, value: u64, path: &str) -> Result<(), DispatchError> {
    *total = total
        .checked_add(value)
        .ok_or_else(|| usage_overflow(path))?;
    Ok(())
}

fn usage_int(value: u64, path: &str) -> Result<i64, DispatchError> {
    i64::try_from(value).map_err(|_| usage_overflow(path))
}

fn usage_overflow(path: &str) -> DispatchError {
    DispatchError::new(
        "directory.unknown",
        "directory tree usage exceeds the Terlan Int range",
        0,
    )
    .with_path(path)
}

fn invalid_tree_usage_path(path: &str) -> DispatchError {
    DispatchError::new(
        "directory.invalid_path",
        "tree usage root must be a directory and must not be a symbolic link",
        0,
    )
    .with_path(path)
}

#[cfg(unix)]
fn metadata_allocated_bytes(metadata: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;

    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn metadata_allocated_bytes(metadata: &std::fs::Metadata) -> u64 {
    if metadata.is_file() {
        metadata.len()
    } else {
        0
    }
}

/// Copies one regular-file directory tree without following symbolic links.
///
/// The destination must not exist. Exact excluded basenames prune both files
/// and directories. Any failure after destination creation removes the partial
/// tree before the typed error crosses the capability boundary.
pub(super) fn copy_directory_tree_excluding(
    source: &str,
    destination: &str,
    excluded_entry_names: &[&str],
) -> Result<(), DispatchError> {
    const OPERATION: &str = "std.io.directory.copy_tree_excluding";
    let source_path = std::path::Path::new(source);
    let destination_path = std::path::Path::new(destination);
    let source_metadata = std::fs::symlink_metadata(source_path)
        .map_err(|error| dispatch_directory_error(OPERATION, source, error))?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_dir() {
        return Err(invalid_copy_path(
            source,
            "copy source must be a directory and must not be a symbolic link",
        ));
    }
    match std::fs::symlink_metadata(destination_path) {
        Ok(_) => {
            return Err(invalid_copy_path(
                destination,
                "copy destination must not already exist",
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(dispatch_directory_error(OPERATION, destination, error)),
    }
    reject_nested_copy_destination(source_path, destination_path, source, destination)?;
    std::fs::create_dir(destination_path)
        .map_err(|error| dispatch_directory_error(OPERATION, destination, error))?;
    let result = copy_directory_contents(
        source_path,
        destination_path,
        excluded_entry_names,
        OPERATION,
    );
    if result.is_err() {
        let _ = std::fs::remove_dir_all(destination_path);
    }
    result
}

/// Creates one directory symbolic link without replacing an existing entry.
pub(super) fn create_directory_symbolic_link(
    target_directory: &str,
    link_path: &str,
) -> Result<(), DispatchError> {
    const OPERATION: &str = "std.io.directory.create_symbolic_link";
    let target = std::fs::canonicalize(target_directory)
        .map_err(|error| dispatch_directory_error(OPERATION, target_directory, error))?;
    let metadata = std::fs::metadata(&target)
        .map_err(|error| dispatch_directory_error(OPERATION, target_directory, error))?;
    if !metadata.is_dir() {
        return Err(DispatchError::new(
            "directory.invalid_path",
            "symbolic-link target must be an existing directory",
            0,
        )
        .with_path(target_directory));
    }
    match std::fs::symlink_metadata(link_path) {
        Ok(_) => {
            return Err(DispatchError::new(
                "directory.invalid_path",
                "symbolic-link destination must not already exist",
                0,
            )
            .with_path(link_path));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(dispatch_directory_error(OPERATION, link_path, error)),
    }
    create_platform_directory_symbolic_link(&target, std::path::Path::new(link_path))
        .map_err(|error| dispatch_directory_error(OPERATION, link_path, error))
}

#[cfg(unix)]
fn create_platform_directory_symbolic_link(
    target_directory: &std::path::Path,
    link_path: &std::path::Path,
) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target_directory, link_path)
}

#[cfg(windows)]
fn create_platform_directory_symbolic_link(
    target_directory: &std::path::Path,
    link_path: &std::path::Path,
) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target_directory, link_path)
}

#[cfg(not(any(unix, windows)))]
fn create_platform_directory_symbolic_link(
    _target_directory: &std::path::Path,
    _link_path: &std::path::Path,
) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "directory symbolic links are unsupported on this platform",
    ))
}

fn reject_nested_copy_destination(
    source: &std::path::Path,
    destination: &std::path::Path,
    source_text: &str,
    destination_text: &str,
) -> Result<(), DispatchError> {
    let operation = "std.io.directory.copy_tree_excluding";
    let source = std::fs::canonicalize(source)
        .map_err(|error| dispatch_directory_error(operation, source_text, error))?;
    let parent = destination
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let parent = std::fs::canonicalize(parent)
        .map_err(|error| dispatch_directory_error(operation, destination_text, error))?;
    let Some(name) = destination.file_name() else {
        return Err(invalid_copy_path(
            destination_text,
            "copy destination must have a final path component",
        ));
    };
    if parent.join(name).starts_with(&source) {
        return Err(invalid_copy_path(
            destination_text,
            "copy destination must not be inside the source tree",
        ));
    }
    Ok(())
}

fn copy_directory_contents(
    source: &std::path::Path,
    destination: &std::path::Path,
    exclusions: &[&str],
    operation: &str,
) -> Result<(), DispatchError> {
    let mut entries = std::fs::read_dir(source)
        .map_err(|error| dispatch_directory_error(operation, &normalized_host_path(source), error))?
        .map(|entry| {
            entry.map_err(|error| {
                dispatch_directory_error(operation, &normalized_host_path(source), error)
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name();
        if name.to_str().is_some_and(|name| exclusions.contains(&name)) {
            continue;
        }
        let source_entry = entry.path();
        let destination_entry = destination.join(&name);
        let source_text = normalized_host_path(&source_entry);
        let destination_text = normalized_host_path(&destination_entry);
        let metadata = std::fs::symlink_metadata(&source_entry)
            .map_err(|error| dispatch_directory_error(operation, &source_text, error))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            std::fs::create_dir(&destination_entry)
                .map_err(|error| dispatch_directory_error(operation, &destination_text, error))?;
            copy_directory_contents(&source_entry, &destination_entry, exclusions, operation)?;
            continue;
        }
        if metadata.is_file() {
            std::fs::copy(&source_entry, &destination_entry)
                .map(|_| ())
                .map_err(|error| dispatch_directory_error(operation, &destination_text, error))?;
            continue;
        }
        return Err(invalid_copy_path(
            &source_text,
            "copy source contains an unsupported filesystem entry",
        ));
    }
    Ok(())
}

fn invalid_copy_path(path: &str, message: &str) -> DispatchError {
    DispatchError::new("directory.invalid_path", message, 0).with_path(path)
}

pub(super) fn dispatch_directory_error(
    operation: &str,
    path: &str,
    error: std::io::Error,
) -> DispatchError {
    let code = match error.kind() {
        std::io::ErrorKind::NotFound => "directory.not_found",
        std::io::ErrorKind::PermissionDenied => "directory.permission_denied",
        std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData => {
            "directory.invalid_path"
        }
        _ => "directory.unknown",
    };
    DispatchError::new(code, format!("{operation} failed for `{path}`: {error}"), 0).with_path(path)
}
