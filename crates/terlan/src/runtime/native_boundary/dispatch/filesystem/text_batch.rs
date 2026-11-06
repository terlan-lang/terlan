//! Rollback-safe batch text publication.

use std::io::Write;

use super::{dispatch_file_error, plan_text_field, DispatchError, NativeBoundaryValue};

pub(super) fn write_text_files(args: &[NativeBoundaryValue]) -> Result<(), DispatchError> {
    use std::collections::HashSet;

    const OPERATION: &str = "std.io.file.write_text_many";
    const MAXIMUM_FILES: usize = 65_536;
    let Some(NativeBoundaryValue::List(values)) = args.first() else {
        return Err(DispatchError::new(
            "boundary.type",
            format!("{OPERATION} argument 0 must be List[TextWritePlan]"),
            0,
        ));
    };
    if values.len() > MAXIMUM_FILES {
        return Err(DispatchError::new(
            "file.invalid_path",
            format!("text write batch exceeds {MAXIMUM_FILES} files"),
            0,
        ));
    }

    let mut plans = Vec::with_capacity(values.len());
    let mut destinations = HashSet::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let NativeBoundaryValue::Record { name, fields } = value else {
            return Err(DispatchError::new(
                "boundary.type",
                format!("{OPERATION} argument 0 item {index} must be TextWritePlan"),
                0,
            ));
        };
        if name != "TextWritePlan" {
            return Err(DispatchError::new(
                "boundary.type",
                format!("{OPERATION} argument 0 item {index} must be TextWritePlan"),
                0,
            ));
        }
        let path = plan_text_field(OPERATION, fields, "path", index)?;
        let contents = plan_text_field(OPERATION, fields, "contents", index)?;
        if path.is_empty() {
            return Err(DispatchError::new(
                "file.invalid_path",
                "text write destination must not be empty",
                0,
            )
            .with_path(path));
        }
        let identity = lexical_destination_identity(path)?;
        if !destinations.insert(identity) {
            return Err(DispatchError::new(
                "file.invalid_path",
                "text write destinations must be unique",
                0,
            )
            .with_path(path));
        }
        match std::fs::symlink_metadata(path) {
            Ok(_) => {
                return Err(DispatchError::new(
                    "file.invalid_path",
                    "text write destination must not already exist",
                    0,
                )
                .with_path(path));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(dispatch_file_error(OPERATION, path, error)),
        }
        plans.push((path.to_string(), contents.to_string()));
    }

    let mut created_files = Vec::with_capacity(plans.len());
    let mut created_directories = Vec::new();
    for (path, contents) in &plans {
        let destination = std::path::Path::new(path);
        if let Some(parent) = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            if let Err(error) = create_missing_parents(parent, &mut created_directories) {
                rollback_text_writes(&created_files, &created_directories);
                return Err(dispatch_file_error(OPERATION, path, error));
            }
        }
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .and_then(|mut file| {
                file.write_all(contents.as_bytes())?;
                file.sync_all()
            });
        if let Err(error) = written {
            let _ = std::fs::remove_file(destination);
            rollback_text_writes(&created_files, &created_directories);
            return Err(dispatch_file_error(OPERATION, path, error));
        }
        created_files.push(destination.to_path_buf());
    }
    Ok(())
}

fn lexical_destination_identity(path: &str) -> Result<std::path::PathBuf, DispatchError> {
    let path = std::path::Path::new(path);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                dispatch_file_error(
                    "std.io.file.write_text_many",
                    path.to_string_lossy().as_ref(),
                    error,
                )
            })?
            .join(path)
    };
    let mut normalized = std::path::PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

fn create_missing_parents(
    parent: &std::path::Path,
    created: &mut Vec<std::path::PathBuf>,
) -> std::io::Result<()> {
    let mut missing = Vec::new();
    let mut current = parent;
    loop {
        match std::fs::symlink_metadata(current) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotADirectory,
                        "text write parent is not a directory",
                    ));
                }
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(current.to_path_buf());
                let Some(next) = current.parent() else {
                    break;
                };
                current = next;
            }
            Err(error) => return Err(error),
        }
    }
    missing.reverse();
    for directory in missing {
        std::fs::create_dir(&directory)?;
        created.push(directory);
    }
    Ok(())
}

fn rollback_text_writes(files: &[std::path::PathBuf], directories: &[std::path::PathBuf]) {
    for path in files.iter().rev() {
        let _ = std::fs::remove_file(path);
    }
    for path in directories.iter().rev() {
        let _ = std::fs::remove_dir(path);
    }
}
