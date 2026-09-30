//! Pure operation dispatcher for Rust-backed NativeBoundary std adapters.
//!
//! This module is the first shared execution surface between compiler-native
//! operation ids such as `std.data.json.parse` and concrete Rust adapter
//! functions. The VM/native worker layer can call this module after it has
//! decoded runtime terms into `NativeBoundaryValue`.

use crate::terlan_native::{json, path, postgres, regex, toml};

mod archive;
mod args;
mod arity;
mod error;
mod filesystem;
mod git;
#[path = "dispatch/hash.rs"]
mod hash;
#[path = "dispatch/json.rs"]
mod json_dispatch;
mod manifest;
mod package_registry;
mod panic_boundary;
mod platform_dispatch;
mod process;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use process::{
    capture_optional_tool_command, capture_tool_command, capture_tool_command_with_launch,
    ToolCommandError,
};
mod resources;
mod value;
mod value_packages;
pub use value::{NativeBoundaryBridgeValue, NativeBoundaryValue};

use args::{
    dispatch_path_error, dispatch_postgres_error, expect_int, expect_json_list, expect_path,
    expect_postgres_config, expect_postgres_pool, expect_postgres_row, expect_text,
    unknown_operation,
};
pub use arity::{operation_arity, validate_operation_arity};
use filesystem::{
    copy_directory_tree_excluding, create_directory_symbolic_link, create_temporary_directory,
    directory_entries, directory_files_recursive, directory_find_named_recursive_excluding,
    directory_tree_usage, dispatch_direct_file_operation, dispatch_directory_error,
    dispatch_file_error, expect_text_list, normalized_host_path, text_files_recursive,
    text_files_recursive_matching,
};
pub use resources::{
    dispatch_with_resources, dispatch_with_resources_for_process,
    dispatch_with_resources_for_process_with_capabilities,
    dispatch_with_resources_for_process_with_policy,
    dispatch_with_resources_for_process_with_policy_and_cancellation,
};

pub use error::DispatchError;

/// Dispatches one compiler-native operation to a NativeBoundary adapter function.
///
/// Inputs:
/// - `operation`: compiler-native operation id from `@compiler.native`.
/// - `args`: neutral runtime values decoded by the native bridge.
///
/// Output:
/// - `Ok(NativeBoundaryValue)` with the adapter result.
/// - `Err(DispatchError)` for unknown operation ids, arity mismatches, type
///   mismatches, or adapter-specific stable errors.
///
/// Transformation:
/// - Validates the operation id and argument shapes, calls the corresponding
///   Rust adapter, and converts adapter-specific errors into one dispatch
///   error shape.
pub fn dispatch(
    operation: &str,
    args: &[NativeBoundaryValue],
) -> Result<NativeBoundaryValue, DispatchError> {
    validate_operation_arity(operation, args.len(), unknown_operation)?;
    if let Some(binding) = crate::std_native_packages::value_binding(operation) {
        return value_packages::dispatch(binding, args);
    }
    if json::operation(operation).is_some() {
        return json_dispatch::dispatch(operation, args);
    }
    if operation.starts_with("std.system.platform.") {
        return platform_dispatch::dispatch(operation);
    }
    if operation.starts_with("std.package.registry.") {
        return package_registry::dispatch(operation, args);
    }
    match operation {
        "std.data.toml.parse" => toml::parse(expect_text(operation, args, 0)?)
            .map(NativeBoundaryValue::Json)
            .map_err(args::dispatch_json_error),
        "std.regex.regex.compile" => {
            let pattern = expect_text(operation, args, 0)?;
            regex::compile(pattern)
                .map(NativeBoundaryValue::Regex)
                .map_err(args::dispatch_regex_error)
        }
        "std.regex.regex.is_match" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::Bool(regex::is_match(value, text)))
        }
        "std.regex.regex.matching_line_numbers" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::List(
                regex::matching_line_numbers(value, text)
                    .into_iter()
                    .map(NativeBoundaryValue::Int)
                    .collect(),
            ))
        }
        "std.regex.regex.find" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::OptionalText(regex::find(value, text)))
        }
        "std.regex.regex.find_all" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::List(
                regex::find_all(value, text)
                    .into_iter()
                    .map(NativeBoundaryValue::Text)
                    .collect(),
            ))
        }
        "std.regex.regex.capture" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            let index = usize::try_from(expect_int(operation, args, 2)?)
                .map_err(|_| args::type_error(operation, 2, "nonnegative Int"))?;
            Ok(NativeBoundaryValue::OptionalText(regex::capture(
                value, text, index,
            )))
        }
        "std.regex.regex.named_capture" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            let name = expect_text(operation, args, 2)?;
            Ok(NativeBoundaryValue::OptionalText(regex::named_capture(
                value, text, name,
            )))
        }
        "std.regex.regex.replace" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            let replacement = expect_text(operation, args, 2)?;
            Ok(NativeBoundaryValue::Text(regex::replace(
                value,
                text,
                replacement,
            )))
        }
        "std.regex.regex.split" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::List(
                regex::split(value, text)
                    .into_iter()
                    .map(NativeBoundaryValue::Text)
                    .collect(),
            ))
        }
        "std.regex.regex.escape" => {
            let text = expect_text(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(regex::escape(text)))
        }
        "std.crypto.hash.sha256_file" => {
            let path = expect_text(operation, args, 0)?;
            hash::sha256_file(operation, path).map(NativeBoundaryValue::Text)
        }
        "std.crypto.hash.verify_sha256_manifest" => {
            let root = expect_text(operation, args, 0)?;
            let manifest = expect_text(operation, args, 1)?;
            hash::verify_sha256_manifest(operation, root, manifest).map(NativeBoundaryValue::Bool)
        }
        "std.crypto.hash.sha256_tree" => {
            let root = expect_text(operation, args, 0)?;
            hash::sha256_tree(operation, root).map(NativeBoundaryValue::Text)
        }
        "std.crypto.hash.sha256_selected_files" => {
            let root = expect_text(operation, args, 0)?;
            let relative_paths = filesystem::expect_text_list(operation, args, 1)?;
            hash::sha256_selected_files(operation, root, &relative_paths)
                .map(NativeBoundaryValue::Text)
        }
        "std.crypto.hash.sha256_labeled_file_digests" => {
            hash::sha256_labeled_file_digests(operation, args).map(NativeBoundaryValue::Text)
        }
        "std.crypto.hash.sha256_labeled_file_contents" => {
            hash::sha256_labeled_file_contents(operation, args).map(NativeBoundaryValue::Text)
        }
        "std.crypto.hash.audit_labeled_files" => {
            let forbidden_fragments = filesystem::expect_text_list(operation, args, 1)?;
            hash::audit_labeled_files(operation, args, &forbidden_fragments)
        }
        "std.crypto.hash.audit_labeled_file_patterns" => {
            let root = expect_text(operation, args, 0)?;
            let forbidden_fragments = filesystem::expect_text_list(operation, args, 2)?;
            hash::audit_labeled_file_patterns(operation, root, args, &forbidden_fragments)
        }
        "std.vcs.git.source_tree_identity" => {
            let root = expect_text(operation, args, 0)?;
            git::source_tree_identity(operation, root)
        }
        operation @ ("std.io.archive.create" | "std.io.archive.extract") => {
            archive::dispatch(operation, args)
        }
        "std.io.console.println" => {
            let text = expect_text(operation, args, 0)?;
            println!("{text}");
            Ok(NativeBoundaryValue::Unit)
        }
        "std.io.console.eprintln" => {
            let text = expect_text(operation, args, 0)?;
            eprintln!("{text}");
            Ok(NativeBoundaryValue::Unit)
        }
        operation @ ("std.io.file.exists"
        | "std.io.file.read_text"
        | "std.io.file.read_bytes"
        | "std.io.file.size"
        | "std.io.file.timestamps"
        | "std.io.file.set_timestamps"
        | "std.io.file.is_executable"
        | "std.io.file.set_executable"
        | "std.io.file.copy"
        | "std.io.file.copy_many") => dispatch_direct_file_operation(operation, args),
        "std.io.file.read_text_many" => {
            let paths = expect_text_list(operation, args, 0)?;
            paths
                .into_iter()
                .map(|path| {
                    std::fs::read_to_string(path)
                        .map(|contents| NativeBoundaryValue::Record {
                            name: "TextFile".to_string(),
                            fields: vec![
                                (
                                    "path".to_string(),
                                    NativeBoundaryValue::Text(path.to_string()),
                                ),
                                ("contents".to_string(), NativeBoundaryValue::Text(contents)),
                            ],
                        })
                        .map_err(|error| dispatch_file_error(operation, path, error))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(NativeBoundaryValue::List)
        }
        "std.io.file.read_text_directory" => {
            let directory = expect_text(operation, args, 0)?;
            let mut paths = std::fs::read_dir(directory)
                .map_err(|error| dispatch_file_error(operation, directory, error))?
                .map(|entry| {
                    entry
                        .map(|entry| entry.path())
                        .map_err(|error| dispatch_file_error(operation, directory, error))
                })
                .collect::<Result<Vec<_>, _>>()?;
            paths.sort();
            let mut files = Vec::new();
            for path in paths {
                let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
                    dispatch_file_error(operation, &normalized_host_path(&path), error)
                })?;
                if !metadata.file_type().is_file() {
                    continue;
                }
                let normalized = normalized_host_path(&path);
                let contents = std::fs::read_to_string(&path)
                    .map_err(|error| dispatch_file_error(operation, &normalized, error))?;
                files.push(NativeBoundaryValue::Record {
                    name: "TextFile".to_string(),
                    fields: vec![
                        ("path".to_string(), NativeBoundaryValue::Text(normalized)),
                        ("contents".to_string(), NativeBoundaryValue::Text(contents)),
                    ],
                });
            }
            Ok(NativeBoundaryValue::List(files))
        }
        "std.io.file.read_text_tree_excluding" => {
            let path = expect_text(operation, args, 0)?;
            let exclusions = expect_text_list(operation, args, 1)?;
            text_files_recursive(path, &exclusions)
        }
        "std.io.file.read_text_tree_matching" => {
            let path = expect_text(operation, args, 0)?;
            let exclusions = expect_text_list(operation, args, 1)?;
            let suffixes = expect_text_list(operation, args, 2)?;
            let excluded_suffixes = expect_text_list(operation, args, 3)?;
            let offset = expect_int(operation, args, 4)?;
            let limit = expect_int(operation, args, 5)?;
            let offset = usize::try_from(offset).map_err(|_| {
                DispatchError::new("boundary.value", "offset must be nonnegative", 0)
            })?;
            let limit = usize::try_from(limit).map_err(|_| {
                DispatchError::new("boundary.value", "limit must be nonnegative", 0)
            })?;
            if limit == 0 {
                return Err(DispatchError::new(
                    "boundary.value",
                    "limit must be positive",
                    0,
                ));
            }
            text_files_recursive_matching(
                path,
                &exclusions,
                &suffixes,
                &excluded_suffixes,
                offset,
                limit,
            )
        }
        "std.io.file.write_text" => {
            let value = expect_text(operation, args, 0)?;
            let contents = expect_text(operation, args, 1)?;
            std::fs::write(value, contents)
                .map(|()| NativeBoundaryValue::Unit)
                .map_err(|error| dispatch_file_error(operation, value, error))
        }
        "std.io.file.append_text" => {
            use std::io::Write as _;
            let value = expect_text(operation, args, 0)?;
            let contents = expect_text(operation, args, 1)?;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(value)
                .and_then(|mut file| file.write_all(contents.as_bytes()))
                .map(|()| NativeBoundaryValue::Unit)
                .map_err(|error| dispatch_file_error(operation, value, error))
        }
        "std.io.file.delete" => {
            let value = expect_text(operation, args, 0)?;
            std::fs::remove_file(value)
                .map(|()| NativeBoundaryValue::Unit)
                .map_err(|error| dispatch_file_error(operation, value, error))
        }
        "std.system.environment.contains" => {
            let key = expect_text(operation, args, 0)?;
            Ok(NativeBoundaryValue::Bool(std::env::var(key).is_ok()))
        }
        "std.system.environment.get" => {
            let key = expect_text(operation, args, 0)?;
            Ok(NativeBoundaryValue::OptionalText(std::env::var(key).ok()))
        }
        "std.system.environment.current_directory" => std::env::current_dir()
            .map(|path| NativeBoundaryValue::Text(path.to_string_lossy().into_owned()))
            .map_err(|error| {
                DispatchError::new(
                    "system.environment.current_directory",
                    format!("Current directory is unavailable: {error}"),
                    0,
                )
            }),
        "std.system.process.limits" => Ok(process::process_limits()),
        "std.system.process.run" => process::run_process(args, None),
        "std.system.process.run_many" => process::run_process_many(args, None),
        "std.system.process.run_length_framed" => process::run_process_length_framed(args, None),
        "std.io.directory.entries" => {
            let path = expect_text(operation, args, 0)?;
            directory_entries(path).map(NativeBoundaryValue::List)
        }
        "std.io.directory.files_recursive" => {
            let path = expect_text(operation, args, 0)?;
            directory_files_recursive(path, &[]).map(NativeBoundaryValue::List)
        }
        "std.io.directory.files_recursive_excluding" => {
            let path = expect_text(operation, args, 0)?;
            let exclusions = expect_text_list(operation, args, 1)?;
            directory_files_recursive(path, &exclusions).map(NativeBoundaryValue::List)
        }
        "std.io.directory.find_named_recursive_excluding" => {
            let path = expect_text(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            let exclusions = expect_text_list(operation, args, 2)?;
            directory_find_named_recursive_excluding(path, name, &exclusions)
                .map(NativeBoundaryValue::List)
        }
        "std.io.directory.tree_usage" => {
            let path = expect_text(operation, args, 0)?;
            directory_tree_usage(path)
        }
        "std.io.directory.copy_tree_excluding" => {
            let source = expect_text(operation, args, 0)?;
            let destination = expect_text(operation, args, 1)?;
            let exclusions = expect_text_list(operation, args, 2)?;
            copy_directory_tree_excluding(source, destination, &exclusions)
                .map(|()| NativeBoundaryValue::Unit)
        }
        "std.io.directory.create_symbolic_link" => {
            let target = expect_text(operation, args, 0)?;
            let link_path = expect_text(operation, args, 1)?;
            create_directory_symbolic_link(target, link_path).map(|()| NativeBoundaryValue::Unit)
        }
        "std.io.directory.create_all" => {
            let path = expect_text(operation, args, 0)?;
            std::fs::create_dir_all(path)
                .map(|()| NativeBoundaryValue::Unit)
                .map_err(|error| dispatch_directory_error(operation, path, error))
        }
        "std.io.directory.create_temporary" => {
            let prefix = expect_text(operation, args, 0)?;
            create_temporary_directory(prefix).map(NativeBoundaryValue::Text)
        }
        "std.io.directory.remove_all" => {
            let path = expect_text(operation, args, 0)?;
            std::fs::remove_dir_all(path)
                .map(|()| NativeBoundaryValue::Unit)
                .map_err(|error| dispatch_directory_error(operation, path, error))
        }
        "std.io.path.from_string" => {
            let text = expect_text(operation, args, 0)?;
            path::from_string(text)
                .map(NativeBoundaryValue::Path)
                .map_err(dispatch_path_error)
        }
        "std.io.path.to_string" => {
            let value = expect_path(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(path::to_string(value)))
        }
        "std.io.path.join" => {
            let value = expect_path(operation, args, 0)?;
            let child = expect_text(operation, args, 1)?;
            path::join(value, child)
                .map(NativeBoundaryValue::Path)
                .map_err(dispatch_path_error)
        }
        "std.io.path.file_name" => {
            let value = expect_path(operation, args, 0)?;
            Ok(NativeBoundaryValue::OptionalText(path::file_name(value)))
        }
        "std.io.path.extension" => {
            let value = expect_path(operation, args, 0)?;
            Ok(NativeBoundaryValue::OptionalText(path::extension(value)))
        }
        "std.io.path.parent" => {
            let value = expect_path(operation, args, 0)?;
            Ok(NativeBoundaryValue::OptionalPath(path::parent(value)))
        }
        "std.io.path.is_absolute" => {
            let value = expect_path(operation, args, 0)?;
            Ok(NativeBoundaryValue::Bool(path::is_absolute(value)))
        }
        "std.io.path.normalize" => {
            let value = expect_path(operation, args, 0)?;
            Ok(NativeBoundaryValue::Path(path::normalize(value)))
        }
        "std.io.path.starts_with" => {
            let value = expect_path(operation, args, 0)?;
            let base = expect_path(operation, args, 1)?;
            Ok(NativeBoundaryValue::Bool(path::starts_with(value, base)))
        }
        "std.io.path.strip_prefix" => {
            let value = expect_path(operation, args, 0)?;
            let base = expect_path(operation, args, 1)?;
            Ok(NativeBoundaryValue::OptionalPath(path::strip_prefix(
                value, base,
            )))
        }
        "std.db.postgres.connect" => {
            let config = expect_postgres_config(operation, args, 0)?;
            postgres::connect(config)
                .map(NativeBoundaryValue::PostgresPool)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.query" => {
            let pool = expect_postgres_pool(operation, args, 0)?;
            let sql = expect_text(operation, args, 1)?;
            let params = expect_json_list(operation, args, 2)?;
            postgres::query(pool, sql, params)
                .map(NativeBoundaryValue::PostgresRows)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.query_one" => {
            let pool = expect_postgres_pool(operation, args, 0)?;
            let sql = expect_text(operation, args, 1)?;
            let params = expect_json_list(operation, args, 2)?;
            postgres::query_one(pool, sql, params)
                .map(NativeBoundaryValue::OptionalPostgresRow)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.execute" => {
            let pool = expect_postgres_pool(operation, args, 0)?;
            let sql = expect_text(operation, args, 1)?;
            let params = expect_json_list(operation, args, 2)?;
            postgres::execute(pool, sql, params)
                .map(NativeBoundaryValue::Int)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.transaction" => {
            let _pool = expect_postgres_pool(operation, args, 0)?;
            Err(DispatchError::new(
                "dispatch.callback_requires_runtime_bridge",
                "Postgres transaction callbacks require runtime bridge lowering.",
                0,
            ))
        }
        "std.db.postgres.string" => {
            let row = expect_postgres_row(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            postgres::string(row, name)
                .map(NativeBoundaryValue::Text)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.int" => {
            let row = expect_postgres_row(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            postgres::int(row, name)
                .map(NativeBoundaryValue::Int)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.bool" => {
            let row = expect_postgres_row(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            postgres::r#bool(row, name)
                .map(NativeBoundaryValue::Bool)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.json" => {
            let row = expect_postgres_row(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            postgres::json(row, name)
                .map(NativeBoundaryValue::Json)
                .map_err(dispatch_postgres_error)
        }
        _ => Err(unknown_operation(operation)),
    }
}

#[cfg(test)]
#[path = "dispatch_test.rs"]
#[cfg(test)]
mod dispatch_test;
