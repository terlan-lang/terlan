//! Operation arity table for NativeBoundary dispatch.
//!
//! The dispatcher owns execution, while this module owns the compact arity
//! contract used before execution and by manifest coverage tests.

use super::DispatchError;

/// Rust-backed operation arities accepted by NativeBoundary dispatch.
///
/// Inputs:
/// - Static operation ids from `@compiler.native` declarations.
///
/// Output:
/// - Operation id to runtime argument count mapping.
///
/// Transformation:
/// - Keeps arity validation data compact and aligned with
///   `std/RUST_BACKED_MANIFEST.tsv` while leaving operation execution in the
///   explicit dispatch match.
const OPERATION_ARITIES: &[(&str, usize)] = &[
    ("std.data.toml.parse", 1),
    ("std.db.postgres.bool", 2),
    ("std.db.postgres.connect", 1),
    ("std.db.postgres.execute", 3),
    ("std.db.postgres.int", 2),
    ("std.db.postgres.json", 2),
    ("std.db.postgres.query", 3),
    ("std.db.postgres.query_one", 3),
    ("std.db.postgres.string", 2),
    ("std.db.postgres.transaction", 2),
    ("std.crypto.hash.sha256_file", 1),
    ("std.crypto.hash.verify_sha256_manifest", 2),
    ("std.crypto.hash.sha256_tree", 1),
    ("std.crypto.hash.sha256_selected_files", 2),
    ("std.crypto.hash.sha256_labeled_file_digests", 1),
    ("std.crypto.hash.sha256_labeled_file_contents", 1),
    ("std.crypto.hash.audit_labeled_files", 2),
    ("std.crypto.hash.audit_labeled_file_patterns", 3),
    ("std.io.archive.create", 2),
    ("std.io.archive.extract", 2),
    ("std.http.session.current", 1),
    ("std.http.session.get", 2),
    ("std.http.session.set", 3),
    ("std.http.session.delete", 2),
    ("std.http.session.rotate", 1),
    ("std.http.session.expire", 1),
    ("std.http.session.is_live", 1),
    ("std.io.path.extension", 1),
    ("std.io.path.file_name", 1),
    ("std.io.path.from_string", 1),
    ("std.io.path.is_absolute", 1),
    ("std.io.path.join", 2),
    ("std.io.path.normalize", 1),
    ("std.io.path.parent", 1),
    ("std.io.path.starts_with", 2),
    ("std.io.path.strip_prefix", 2),
    ("std.io.path.to_string", 1),
    ("std.io.console.println", 1),
    ("std.io.console.eprintln", 1),
    ("std.io.file.exists", 1),
    ("std.io.file.read_text", 1),
    ("std.io.file.read_bytes", 1),
    ("std.io.file.size", 1),
    ("std.io.file.timestamps", 1),
    ("std.io.file.set_timestamps", 3),
    ("std.io.file.is_executable", 1),
    ("std.io.file.set_executable", 2),
    ("std.io.file.copy", 2),
    ("std.io.file.copy_many", 1),
    ("std.io.file.read_text_many", 1),
    ("std.io.file.read_text_directory", 1),
    ("std.io.file.read_text_tree_excluding", 2),
    ("std.io.file.read_text_tree_matching", 6),
    ("std.io.file.write_text", 2),
    ("std.io.file.append_text", 2),
    ("std.io.file.delete", 1),
    ("std.system.environment.contains", 1),
    ("std.system.environment.get", 1),
    ("std.system.environment.current_directory", 0),
    ("std.system.process.limits", 0),
    ("std.system.process.run", 1),
    ("std.system.process.run_many", 1),
    ("std.system.process.run_length_framed", 1),
    ("std.system.platform.current", 0),
    ("std.system.platform.current_metrics", 0),
    ("std.vcs.git.source_tree_identity", 1),
    ("std.io.directory.entries", 1),
    ("std.io.directory.files_recursive", 1),
    ("std.io.directory.files_recursive_excluding", 2),
    ("std.io.directory.find_named_recursive_excluding", 3),
    ("std.io.directory.tree_usage", 1),
    ("std.io.directory.copy_tree_excluding", 3),
    ("std.io.directory.create_symbolic_link", 2),
    ("std.io.directory.create_all", 1),
    ("std.io.directory.create_temporary", 1),
    ("std.io.directory.remove_all", 1),
    ("std.package.registry.parse_publish_request", 1),
    ("std.package.registry.parse_yank_request", 1),
    ("std.package.registry.archive_inventory_valid", 3),
    ("std.package.registry.sign_resource", 2),
    ("std.package.registry.canonical_payload", 1),
    ("std.package.registry.root_payload", 2),
    ("std.package.registry.signing_seed_valid", 1),
    ("std.package.registry.build_signed_resource", 5),
    ("std.package.registry.dependency_candidates_valid", 3),
    ("std.random.random.bool", 1),
    ("std.random.random.bounded_int", 3),
    ("std.random.random.choice", 2),
    ("std.random.random.entropy", 0),
    ("std.random.random.float", 1),
    ("std.random.random.int", 1),
    ("std.random.random.sample", 3),
    ("std.random.random.seed", 1),
    ("std.random.random.shuffle", 2),
    ("std.regex.regex.capture", 3),
    ("std.regex.regex.compile", 1),
    ("std.regex.regex.escape", 1),
    ("std.regex.regex.find", 2),
    ("std.regex.regex.find_all", 2),
    ("std.regex.regex.is_match", 2),
    ("std.regex.regex.matching_line_numbers", 2),
    ("std.regex.regex.named_capture", 3),
    ("std.regex.regex.replace", 3),
    ("std.regex.regex.split", 2),
    ("std.native.collections.vector.new", 0),
    ("std.native.collections.vector.from_list", 1),
    ("std.native.collections.vector.length", 1),
    ("std.native.collections.vector.get", 2),
    ("std.native.collections.vector.get_at", 2),
    ("std.native.collections.vector.set_at", 3),
    ("std.native.collections.vector.swap", 3),
    ("std.native.collections.vector.push", 2),
    ("std.native.collections.vector.to_list", 1),
];

/// Returns the expected arity for a supported operation.
///
/// Inputs:
/// - `operation`: compiler-native operation id.
///
/// Output:
/// - Expected runtime argument count, or `None` for an unknown operation.
///
/// Transformation:
/// - Looks up operation ids in `OPERATION_ARITIES` without allocating, giving
///   both pure dispatch and bridge dispatch one shared arity source.
pub fn operation_arity(operation: &str) -> Option<usize> {
    crate::std_native_packages::resource_operation(operation)
        .map(|contract| contract.arity)
        .or_else(|| {
            OPERATION_ARITIES
                .iter()
                .find_map(|(candidate, arity)| (*candidate == operation).then_some(*arity))
        })
}

/// Validates the supplied argument count for one operation.
///
/// Inputs:
/// - `operation`: compiler-native operation id.
/// - `actual`: supplied argument count.
/// - `unknown`: operation-specific unknown-operation diagnostic builder.
///
/// Output:
/// - `Ok(())` when arity matches.
/// - `Err(DispatchError)` for unknown operations or wrong arity.
///
/// Transformation:
/// - Compares the supplied count with `operation_arity` while allowing each
///   dispatch surface to keep its own unknown-operation error context.
pub fn validate_operation_arity(
    operation: &str,
    actual: usize,
    unknown: impl FnOnce(&str) -> DispatchError,
) -> Result<(), DispatchError> {
    match crate::std_native_packages::value_binding(operation)
        .map(|binding| binding.arity)
        .or_else(|| operation_arity(operation))
    {
        Some(expected) if expected == actual => Ok(()),
        Some(expected) => Err(DispatchError::new(
            "dispatch.arity",
            format!("Operation `{operation}` expects {expected} argument(s), got {actual}."),
            0,
        )),
        None => Err(unknown(operation)),
    }
}
