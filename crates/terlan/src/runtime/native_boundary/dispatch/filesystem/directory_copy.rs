//! Directory copy, link, and error helpers.

use super::{normalized_host_path, DispatchError};

/// Copies one regular-file directory tree without following symbolic links.
///
/// The destination must not exist. Exact excluded basenames prune both files
/// and directories. Any failure after destination creation removes the partial
/// tree before the typed error crosses the capability boundary.
pub(in crate::runtime::native_boundary::dispatch) fn copy_directory_tree_excluding(
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
pub(in crate::runtime::native_boundary::dispatch) fn create_directory_symbolic_link(
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

pub(in crate::runtime::native_boundary::dispatch) fn dispatch_directory_error(
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
