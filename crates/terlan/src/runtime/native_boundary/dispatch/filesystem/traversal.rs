use super::{dispatch_directory_error, dispatch_file_error};
use crate::terlan_native_boundary::dispatch::{DispatchError, NativeBoundaryValue};

pub(crate) fn text_files_recursive(
    path: &str,
    excluded_directory_names: &[&str],
) -> Result<NativeBoundaryValue, DispatchError> {
    let operation = "std.io.file.read_text_tree_excluding";
    let paths = recursive_file_paths_with(
        path,
        excluded_directory_names,
        operation,
        dispatch_file_error,
    )?;
    paths
        .into_iter()
        .map(|path| {
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| dispatch_file_error(operation, &path, error))?;
            Ok(NativeBoundaryValue::Record {
                name: "TextFile".to_string(),
                fields: vec![
                    ("path".to_string(), NativeBoundaryValue::Text(path)),
                    ("contents".to_string(), NativeBoundaryValue::Text(contents)),
                ],
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(NativeBoundaryValue::List)
}

pub(crate) fn text_files_recursive_matching(
    path: &str,
    excluded_directory_names: &[&str],
    included_suffixes: &[&str],
    excluded_suffixes: &[&str],
    offset: usize,
    limit: usize,
) -> Result<NativeBoundaryValue, DispatchError> {
    let operation = "std.io.file.read_text_tree_matching";
    let paths = recursive_file_paths_with(
        path,
        excluded_directory_names,
        operation,
        dispatch_file_error,
    )?;
    paths
        .into_iter()
        .filter(|path| {
            included_suffixes
                .iter()
                .any(|suffix| path.ends_with(suffix))
                && !excluded_suffixes
                    .iter()
                    .any(|suffix| path.ends_with(suffix))
        })
        .skip(offset)
        .take(limit)
        .map(|path| {
            let contents = std::fs::read_to_string(&path)
                .map_err(|error| dispatch_file_error(operation, &path, error))?;
            Ok(NativeBoundaryValue::Record {
                name: "TextFile".to_string(),
                fields: vec![
                    ("path".to_string(), NativeBoundaryValue::Text(path)),
                    ("contents".to_string(), NativeBoundaryValue::Text(contents)),
                ],
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(NativeBoundaryValue::List)
}

pub(crate) fn expect_text_list<'a>(
    operation: &str,
    args: &'a [NativeBoundaryValue],
    index: usize,
) -> Result<Vec<&'a str>, DispatchError> {
    let Some(NativeBoundaryValue::List(values)) = args.get(index) else {
        return Err(DispatchError::new(
            "boundary.type",
            format!("{operation} argument {index} must be List[String]"),
            0,
        ));
    };
    values
        .iter()
        .map(|value| match value {
            NativeBoundaryValue::Text(value) => Ok(value.as_str()),
            _ => Err(DispatchError::new(
                "boundary.type",
                format!("{operation} argument {index} must contain only String values"),
                0,
            )),
        })
        .collect()
}

pub(crate) fn normalized_host_path(path: &std::path::Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_string()
}

pub(crate) fn recursive_file_paths(
    path: &str,
    exclusions: &[&str],
    operation: &str,
) -> Result<Vec<String>, DispatchError> {
    recursive_file_paths_with(path, exclusions, operation, dispatch_directory_error)
}

fn recursive_file_paths_with(
    path: &str,
    exclusions: &[&str],
    operation: &str,
    error_mapper: fn(&str, &str, std::io::Error) -> DispatchError,
) -> Result<Vec<String>, DispatchError> {
    let mut pending = vec![std::path::PathBuf::from(path)];
    let mut files = Vec::new();
    while let Some(current) = pending.pop() {
        let normalized = normalized_host_path(&current);
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|error| error_mapper(operation, &normalized, error))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            files.push(normalized);
            continue;
        }
        if metadata.is_dir() {
            if current != std::path::Path::new(path)
                && current
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .is_some_and(|name| exclusions.contains(&name))
            {
                continue;
            }
            let mut children = std::fs::read_dir(&current)
                .map_err(|error| error_mapper(operation, &normalized, error))?
                .map(|entry| {
                    entry
                        .map(|entry| entry.path())
                        .map_err(|error| error_mapper(operation, &normalized, error))
                })
                .collect::<Result<Vec<_>, _>>()?;
            children.sort();
            children.reverse();
            pending.extend(children);
        }
    }
    files.sort();
    Ok(files)
}
