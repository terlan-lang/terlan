//! Rollback-safe batch text and byte publication.

use std::io::Write;

use super::{
    dispatch_file_error, plan_bytes_field, plan_text_field, DispatchError, NativeBoundaryValue,
};

pub(super) fn write_text_files(args: &[NativeBoundaryValue]) -> Result<(), DispatchError> {
    write_files(args, BatchKind::Text)
}

pub(super) fn write_byte_files(args: &[NativeBoundaryValue]) -> Result<(), DispatchError> {
    write_files(args, BatchKind::Bytes)
}

#[derive(Clone, Copy)]
enum BatchKind {
    Text,
    Bytes,
}

impl BatchKind {
    fn operation(self) -> &'static str {
        match self {
            Self::Text => "std.io.file.write_text_many",
            Self::Bytes => "std.io.file.write_bytes_many",
        }
    }

    fn plan_name(self) -> &'static str {
        match self {
            Self::Text => "TextWritePlan",
            Self::Bytes => "BytesWritePlan",
        }
    }
}

fn write_files(args: &[NativeBoundaryValue], kind: BatchKind) -> Result<(), DispatchError> {
    use std::collections::HashSet;

    const MAXIMUM_FILES: usize = 65_536;
    let operation = kind.operation();
    let plan_name = kind.plan_name();
    let Some(NativeBoundaryValue::List(values)) = args.first() else {
        return Err(DispatchError::new(
            "boundary.type",
            format!("{operation} argument 0 must be List[{plan_name}]"),
            0,
        ));
    };
    if values.len() > MAXIMUM_FILES {
        return Err(DispatchError::new(
            "file.invalid_path",
            format!("file write batch exceeds {MAXIMUM_FILES} files"),
            0,
        ));
    }

    let mut plans = Vec::with_capacity(values.len());
    let mut destinations = HashSet::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let NativeBoundaryValue::Record { name, fields } = value else {
            return Err(DispatchError::new(
                "boundary.type",
                format!("{operation} argument 0 item {index} must be {plan_name}"),
                0,
            ));
        };
        if name != plan_name {
            return Err(DispatchError::new(
                "boundary.type",
                format!("{operation} argument 0 item {index} must be {plan_name}"),
                0,
            ));
        }
        let path = plan_text_field(operation, fields, "path", index)?;
        let contents = match kind {
            BatchKind::Text => plan_text_field(operation, fields, "contents", index)?
                .as_bytes()
                .to_vec(),
            BatchKind::Bytes => plan_bytes_field(operation, fields, "contents", index)?.to_vec(),
        };
        if path.is_empty() {
            return Err(DispatchError::new(
                "file.invalid_path",
                "file write destination must not be empty",
                0,
            )
            .with_path(path));
        }
        let identity = lexical_destination_identity(operation, path)?;
        if !destinations.insert(identity) {
            return Err(DispatchError::new(
                "file.invalid_path",
                "file write destinations must be unique",
                0,
            )
            .with_path(path));
        }
        match std::fs::symlink_metadata(path) {
            Ok(_) => {
                return Err(DispatchError::new(
                    "file.invalid_path",
                    "file write destination must not already exist",
                    0,
                )
                .with_path(path));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(dispatch_file_error(operation, path, error)),
        }
        plans.push((path.to_string(), contents));
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
                rollback_writes(&created_files, &created_directories);
                return Err(dispatch_file_error(operation, path, error));
            }
        }
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .and_then(|mut file| {
                file.write_all(contents)?;
                file.sync_all()
            });
        if let Err(error) = written {
            let _ = std::fs::remove_file(destination);
            rollback_writes(&created_files, &created_directories);
            return Err(dispatch_file_error(operation, path, error));
        }
        created_files.push(destination.to_path_buf());
    }
    Ok(())
}

fn lexical_destination_identity(
    operation: &str,
    path: &str,
) -> Result<std::path::PathBuf, DispatchError> {
    let path = std::path::Path::new(path);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                dispatch_file_error(operation, path.to_string_lossy().as_ref(), error)
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
                        "file write parent is not a directory",
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

fn rollback_writes(files: &[std::path::PathBuf], directories: &[std::path::PathBuf]) {
    for path in files.iter().rev() {
        let _ = std::fs::remove_file(path);
    }
    for path in directories.iter().rev() {
        let _ = std::fs::remove_dir(path);
    }
}
