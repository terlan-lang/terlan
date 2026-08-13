//! Streaming filesystem-backed cryptographic operations.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use regex::bytes::RegexSet;
use sha2::Digest;

use super::filesystem::dispatch_file_error;
use super::{DispatchError, NativeBoundaryValue};

pub(super) fn field_too_large(operation: &str) -> DispatchError {
    DispatchError::new(
        "dispatch.hash_field_too_large",
        format!("SHA-256 framed field length does not fit u64 for `{operation}`"),
        0,
    )
}

const SHA256_HEX_LENGTH: usize = 64;
const MAXIMUM_LABELED_FILES: usize = 65_536;
const MAXIMUM_FORBIDDEN_FRAGMENTS: usize = 256;
const MAXIMUM_FORBIDDEN_FRAGMENT_BYTES: usize = 4_096;

pub(super) fn sha256_file(operation: &str, path: &str) -> Result<String, DispatchError> {
    let mut file =
        std::fs::File::open(path).map_err(|error| dispatch_file_error(operation, path, error))?;
    sha256_reader(operation, path, &mut file)
}

/// Verifies one canonical checksum manifest in a single filesystem capability.
///
/// The root and every selected payload are canonicalized before hashing. This
/// rejects lexical traversal and symlink escapes while allowing the adapter to
/// stream arbitrarily large payload sets without materializing them in an actor
/// heap or crossing the VM boundary once per file.
pub(super) fn verify_sha256_manifest(
    operation: &str,
    root: &str,
    manifest: &str,
) -> Result<bool, DispatchError> {
    let canonical_root =
        std::fs::canonicalize(root).map_err(|error| dispatch_file_error(operation, root, error))?;
    let source = std::fs::read_to_string(manifest)
        .map_err(|error| dispatch_file_error(operation, manifest, error))?;
    for line in source.lines() {
        if line.is_empty() {
            continue;
        }
        let Some((expected, relative)) = line.split_once("  ") else {
            return Ok(false);
        };
        if !valid_sha256(expected) || !safe_relative_path(relative) {
            return Ok(false);
        }
        let selected = canonical_root.join(relative);
        let canonical_selected = std::fs::canonicalize(&selected)
            .map_err(|error| dispatch_file_error(operation, &selected.to_string_lossy(), error))?;
        if !canonical_selected.starts_with(&canonical_root) || !canonical_selected.is_file() {
            return Ok(false);
        }
        let actual = sha256_file(operation, &canonical_selected.to_string_lossy())?;
        if actual != expected {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Streams a canonical, path-independent digest of one regular-file tree.
pub(super) fn sha256_tree(operation: &str, root: &str) -> Result<String, DispatchError> {
    let canonical_root =
        std::fs::canonicalize(root).map_err(|error| dispatch_file_error(operation, root, error))?;
    if !canonical_root.is_dir() {
        return Err(DispatchError::new(
            "file.invalid_path",
            "tree hash root must be a directory",
            0,
        )
        .with_path(root));
    }
    let canonical_text = super::filesystem::normalized_host_path(&canonical_root);
    let paths = super::filesystem::recursive_file_paths(&canonical_text, &[], operation)?;
    let mut digest = sha2::Sha256::new();
    for path in paths {
        let relative = Path::new(&path)
            .strip_prefix(&canonical_root)
            .map_err(|_| {
                DispatchError::new(
                    "file.invalid_path",
                    "tree hash payload escaped its canonical root",
                    0,
                )
                .with_path(&path)
            })?;
        let relative = super::filesystem::normalized_host_path(relative);
        let file_digest = sha256_file(operation, &path)?;
        update_framed_text(&mut digest, operation, &relative)?;
        update_framed_text(&mut digest, operation, &file_digest)?;
    }
    Ok(hex_digest(digest.finalize()))
}

/// Streams caller-selected files using the release surface framing contract.
pub(super) fn sha256_selected_files(
    operation: &str,
    root: &str,
    relative_paths: &[&str],
) -> Result<String, DispatchError> {
    let canonical_root =
        std::fs::canonicalize(root).map_err(|error| dispatch_file_error(operation, root, error))?;
    if !canonical_root.is_dir() {
        return Err(DispatchError::new(
            "file.invalid_path",
            "selected-file hash root must be a directory",
            0,
        )
        .with_path(root));
    }
    let mut digest = sha2::Sha256::new();
    let mut buffer = [0u8; 65_536];
    for relative in relative_paths {
        if !safe_relative_path(relative) {
            return Err(DispatchError::new(
                "file.invalid_path",
                "selected-file hash path must remain relative",
                0,
            )
            .with_path(*relative));
        }
        let selected = canonical_root.join(relative);
        let canonical_selected = std::fs::canonicalize(&selected)
            .map_err(|error| dispatch_file_error(operation, &selected.to_string_lossy(), error))?;
        if !canonical_selected.starts_with(&canonical_root) || !canonical_selected.is_file() {
            return Err(DispatchError::new(
                "file.invalid_path",
                "selected-file hash payload escaped its canonical root",
                0,
            )
            .with_path(*relative));
        }
        digest.update(relative.replace('\\', "/").as_bytes());
        digest.update([0]);
        let mut file = std::fs::File::open(&canonical_selected).map_err(|error| {
            dispatch_file_error(operation, &canonical_selected.to_string_lossy(), error)
        })?;
        loop {
            let count = file.read(&mut buffer).map_err(|error| {
                dispatch_file_error(operation, &canonical_selected.to_string_lossy(), error)
            })?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        digest.update([b'\n']);
    }
    Ok(hex_digest(digest.finalize()))
}

/// Streams the established artifact path/digest row contract in caller order.
pub(super) fn sha256_labeled_file_digests(
    operation: &str,
    args: &[NativeBoundaryValue],
) -> Result<String, DispatchError> {
    let values = labeled_file_values(operation, args)?;
    let mut digest = sha2::Sha256::new();
    for (index, value) in values.iter().enumerate() {
        let (path, label) = checked_labeled_file(operation, value, index)?;
        let file_digest = sha256_file(operation, path)?;
        update_labeled_digest(&mut digest, label, &file_digest);
    }
    Ok(hex_digest(digest.finalize()))
}

/// Streams the established compilation-input path/content framing contract.
pub(super) fn sha256_labeled_file_contents(
    operation: &str,
    args: &[NativeBoundaryValue],
) -> Result<String, DispatchError> {
    let values = labeled_file_values(operation, args)?;
    let mut digest = sha2::Sha256::new();
    let mut buffer = [0_u8; 65_536];
    for (index, value) in values.iter().enumerate() {
        let (path, label) = checked_labeled_file(operation, value, index)?;
        let label_length = u32::try_from(label.len()).map_err(|_| {
            DispatchError::new(
                "file.invalid_path",
                "labeled-file label exceeds the unsigned 32-bit framing limit",
                0,
            )
            .with_path(label)
        })?;
        let mut file = std::fs::File::open(path)
            .map_err(|error| dispatch_file_error(operation, path, error))?;
        let expected_length = file
            .metadata()
            .map_err(|error| dispatch_file_error(operation, path, error))?
            .len();
        digest.update(label_length.to_be_bytes());
        digest.update(label.as_bytes());
        digest.update(expected_length.to_be_bytes());
        let mut actual_length = 0_u64;
        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|error| dispatch_file_error(operation, path, error))?;
            if count == 0 {
                break;
            }
            actual_length = actual_length.checked_add(count as u64).ok_or_else(|| {
                DispatchError::new("file.io", "file length overflow", 0).with_path(path)
            })?;
            digest.update(&buffer[..count]);
        }
        if actual_length != expected_length {
            return Err(DispatchError::new(
                "file.io",
                "file changed while computing the compilation-input digest",
                0,
            )
            .with_path(path));
        }
    }
    Ok(hex_digest(digest.finalize()))
}

/// Streams one content-policy scan and the established labeled digest without
/// materializing file contents in the managed actor heap or reading them twice.
pub(super) fn audit_labeled_files(
    operation: &str,
    args: &[NativeBoundaryValue],
    forbidden_fragments: &[&str],
) -> Result<NativeBoundaryValue, DispatchError> {
    let values = labeled_file_values(operation, args)?;
    let patterns = checked_forbidden_fragments(operation, forbidden_fragments)?;
    let mut digest = sha2::Sha256::new();
    let mut portable = true;
    for (index, value) in values.iter().enumerate() {
        let (path, label) = checked_labeled_file(operation, value, index)?;
        let (file_digest, matched) = sha256_file_and_scan(operation, path, &patterns)?;
        portable &= !matched;
        update_labeled_digest(&mut digest, label, &file_digest);
    }
    labeled_file_audit_value(values.len(), digest, portable)
}

/// Expands the bounded generated-artifact pattern grammar and performs one
/// streaming audit without materializing per-file records in an actor heap.
pub(super) fn audit_labeled_file_patterns(
    operation: &str,
    root: &str,
    args: &[NativeBoundaryValue],
    forbidden_fragments: &[&str],
) -> Result<NativeBoundaryValue, DispatchError> {
    let canonical_root =
        std::fs::canonicalize(root).map_err(|error| dispatch_file_error(operation, root, error))?;
    if !canonical_root.is_dir() {
        return Err(DispatchError::new(
            "file.invalid_path",
            "labeled-file pattern root must be a directory",
            0,
        )
        .with_path(root));
    }
    let values = labeled_file_pattern_values(operation, args)?;
    let forbidden = checked_forbidden_fragments(operation, forbidden_fragments)?;
    let mut inventories = HashMap::<(PathBuf, PatternDepth), Vec<PathBuf>>::new();
    let mut selected = HashSet::<String>::new();
    let mut digest = sha2::Sha256::new();
    let mut portable = true;
    let mut file_count = 0_usize;

    for (index, value) in values.iter().enumerate() {
        let (id, pattern) = checked_labeled_file_pattern(operation, value, index)?;
        let matched =
            expand_labeled_file_pattern(operation, &canonical_root, pattern, &mut inventories)?;
        if matched.is_empty() {
            return Err(DispatchError::new(
                "file.not_found",
                "labeled-file inventory pattern matched no regular files",
                1,
            )
            .with_path(pattern));
        }
        for path in matched {
            let relative = path.strip_prefix(&canonical_root).map_err(|_| {
                DispatchError::new(
                    "file.invalid_path",
                    "labeled-file inventory match escaped its canonical root",
                    1,
                )
                .with_path(super::filesystem::normalized_host_path(&path))
            })?;
            let relative = super::filesystem::normalized_host_path(relative);
            if !selected.insert(relative.clone()) {
                return Err(DispatchError::new(
                    "file.invalid_path",
                    "labeled-file inventory patterns selected one path more than once",
                    1,
                )
                .with_path(&relative));
            }
            let path_text = super::filesystem::normalized_host_path(&path);
            let (file_digest, matched_forbidden) =
                sha256_file_and_scan(operation, &path_text, &forbidden)?;
            portable &= !matched_forbidden;
            update_labeled_digest(&mut digest, &format!("{id}:{relative}"), &file_digest);
            file_count = file_count.checked_add(1).ok_or_else(|| {
                DispatchError::new(
                    "file.invalid_path",
                    "labeled-file audit count overflowed",
                    1,
                )
            })?;
            if file_count > MAXIMUM_LABELED_FILES {
                return Err(DispatchError::new(
                    "file.invalid_path",
                    format!("labeled-file operation exceeds {MAXIMUM_LABELED_FILES} files"),
                    1,
                ));
            }
        }
    }

    labeled_file_audit_value(file_count, digest, portable)
}

fn labeled_file_audit_value(
    file_count: usize,
    digest: sha2::Sha256,
    portable: bool,
) -> Result<NativeBoundaryValue, DispatchError> {
    let file_count = i64::try_from(file_count).map_err(|_| {
        DispatchError::new(
            "file.invalid_path",
            "labeled-file audit count exceeds the Terlan Int range",
            0,
        )
    })?;
    Ok(NativeBoundaryValue::Record {
        name: "LabeledFileAudit".to_string(),
        fields: vec![
            (
                "file_count".to_string(),
                NativeBoundaryValue::Int(file_count),
            ),
            (
                "digest".to_string(),
                NativeBoundaryValue::Text(hex_digest(digest.finalize())),
            ),
            ("portable".to_string(), NativeBoundaryValue::Bool(portable)),
        ],
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PatternDepth {
    Immediate,
    Recursive,
}

enum LabeledFilePattern<'a> {
    Exact(&'a str),
    Suffix {
        directory: &'a str,
        suffix: &'a str,
        depth: PatternDepth,
    },
}

fn labeled_file_pattern_values<'a>(
    operation: &str,
    args: &'a [NativeBoundaryValue],
) -> Result<&'a [NativeBoundaryValue], DispatchError> {
    let Some(NativeBoundaryValue::List(values)) = args.get(1) else {
        return Err(DispatchError::new(
            "boundary.type",
            format!("{operation} argument 1 must be List[LabeledFilePattern]"),
            1,
        ));
    };
    if values.is_empty() || values.len() > MAXIMUM_LABELED_FILES {
        return Err(DispatchError::new(
            "file.invalid_path",
            format!("labeled-file pattern operation requires 1..={MAXIMUM_LABELED_FILES} patterns"),
            1,
        ));
    }
    Ok(values)
}

fn checked_labeled_file_pattern<'a>(
    operation: &str,
    value: &'a NativeBoundaryValue,
    index: usize,
) -> Result<(&'a str, &'a str), DispatchError> {
    let NativeBoundaryValue::Record { name, fields } = value else {
        return Err(labeled_file_pattern_type_error(operation, index));
    };
    if name != "LabeledFilePattern" {
        return Err(labeled_file_pattern_type_error(operation, index));
    }
    let id = labeled_file_pattern_field(operation, fields, "id", index)?;
    let pattern = labeled_file_pattern_field(operation, fields, "pattern", index)?;
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(DispatchError::new(
            "file.invalid_path",
            "labeled-file pattern id must use only ASCII letters, digits, '.', '_', or '-'",
            1,
        ));
    }
    parse_labeled_file_pattern(operation, pattern)?;
    Ok((id, pattern))
}

fn labeled_file_pattern_field<'a>(
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
                format!("{operation} argument 1 item {index} must contain String field `{field}`"),
                1,
            )
        })
}

fn labeled_file_pattern_type_error(operation: &str, index: usize) -> DispatchError {
    DispatchError::new(
        "boundary.type",
        format!("{operation} argument 1 item {index} must be LabeledFilePattern"),
        1,
    )
}

fn parse_labeled_file_pattern<'a>(
    operation: &str,
    pattern: &'a str,
) -> Result<LabeledFilePattern<'a>, DispatchError> {
    if let Some((directory, suffix)) = pattern.split_once("/**/*") {
        return checked_suffix_pattern(
            operation,
            pattern,
            directory,
            suffix,
            PatternDepth::Recursive,
        );
    }
    if let Some((directory, suffix)) = pattern.split_once("/*") {
        return checked_suffix_pattern(
            operation,
            pattern,
            directory,
            suffix,
            PatternDepth::Immediate,
        );
    }
    if safe_relative_path(pattern) && !pattern.contains('*') {
        return Ok(LabeledFilePattern::Exact(pattern));
    }
    Err(invalid_labeled_file_pattern(operation, pattern))
}

fn checked_suffix_pattern<'a>(
    operation: &str,
    pattern: &'a str,
    directory: &'a str,
    suffix: &'a str,
    depth: PatternDepth,
) -> Result<LabeledFilePattern<'a>, DispatchError> {
    if safe_relative_path(directory)
        && !directory.contains('*')
        && !suffix.is_empty()
        && !suffix.contains('*')
        && !suffix.contains('/')
        && !suffix.contains('\\')
    {
        Ok(LabeledFilePattern::Suffix {
            directory,
            suffix,
            depth,
        })
    } else {
        Err(invalid_labeled_file_pattern(operation, pattern))
    }
}

fn invalid_labeled_file_pattern(operation: &str, pattern: &str) -> DispatchError {
    DispatchError::new(
        "file.invalid_path",
        format!("{operation} accepts only exact paths, directory/*suffix, or directory/**/*suffix"),
        1,
    )
    .with_path(pattern)
}

fn expand_labeled_file_pattern(
    operation: &str,
    canonical_root: &Path,
    pattern: &str,
    inventories: &mut HashMap<(PathBuf, PatternDepth), Vec<PathBuf>>,
) -> Result<Vec<PathBuf>, DispatchError> {
    match parse_labeled_file_pattern(operation, pattern)? {
        LabeledFilePattern::Exact(relative) => {
            let candidate = canonical_root.join(relative);
            let metadata = std::fs::symlink_metadata(&candidate).map_err(|error| {
                dispatch_file_error(operation, &candidate.to_string_lossy(), error)
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(DispatchError::new(
                    "file.invalid_path",
                    "labeled-file exact pattern must select a regular file, not a symbolic link",
                    1,
                )
                .with_path(pattern));
            }
            let canonical = std::fs::canonicalize(&candidate).map_err(|error| {
                dispatch_file_error(operation, &candidate.to_string_lossy(), error)
            })?;
            if !canonical.starts_with(canonical_root) {
                return Err(DispatchError::new(
                    "file.invalid_path",
                    "labeled-file exact pattern escaped its canonical root",
                    1,
                )
                .with_path(pattern));
            }
            Ok(vec![canonical])
        }
        LabeledFilePattern::Suffix {
            directory,
            suffix,
            depth,
        } => {
            let canonical_directory =
                checked_pattern_directory(operation, canonical_root, directory, pattern)?;
            let key = (canonical_directory.clone(), depth);
            if !inventories.contains_key(&key) {
                let inventory = directory_file_inventory(operation, &canonical_directory, depth)?;
                inventories.insert(key.clone(), inventory);
            }
            let inventory = inventories.get(&key).ok_or_else(|| {
                DispatchError::new(
                    "file.invalid_path",
                    "labeled-file directory inventory cache lost an inserted entry",
                    1,
                )
            })?;
            Ok(inventory
                .iter()
                .filter(|path| {
                    path.file_name()
                        .and_then(std::ffi::OsStr::to_str)
                        .is_some_and(|name| name.ends_with(suffix))
                })
                .cloned()
                .collect())
        }
    }
}

fn checked_pattern_directory(
    operation: &str,
    canonical_root: &Path,
    directory: &str,
    pattern: &str,
) -> Result<PathBuf, DispatchError> {
    let candidate = canonical_root.join(directory);
    let metadata = std::fs::symlink_metadata(&candidate)
        .map_err(|error| dispatch_file_error(operation, &candidate.to_string_lossy(), error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(DispatchError::new(
            "file.invalid_path",
            "labeled-file pattern directory must be a directory, not a symbolic link",
            1,
        )
        .with_path(pattern));
    }
    let canonical = std::fs::canonicalize(&candidate)
        .map_err(|error| dispatch_file_error(operation, &candidate.to_string_lossy(), error))?;
    if !canonical.starts_with(canonical_root) {
        return Err(DispatchError::new(
            "file.invalid_path",
            "labeled-file pattern directory escaped its canonical root",
            1,
        )
        .with_path(pattern));
    }
    Ok(canonical)
}

fn directory_file_inventory(
    operation: &str,
    directory: &Path,
    depth: PatternDepth,
) -> Result<Vec<PathBuf>, DispatchError> {
    if depth == PatternDepth::Recursive {
        let directory = super::filesystem::normalized_host_path(directory);
        return super::filesystem::recursive_file_paths(&directory, &[], operation)
            .map(|paths| paths.into_iter().map(PathBuf::from).collect());
    }
    let directory_text = super::filesystem::normalized_host_path(directory);
    let mut files = Vec::new();
    let entries = std::fs::read_dir(directory)
        .map_err(|error| dispatch_file_error(operation, &directory_text, error))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| dispatch_file_error(operation, &directory_text, error))?;
        let path = entry.path();
        let path_text = super::filesystem::normalized_host_path(&path);
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| dispatch_file_error(operation, &path_text, error))?;
        if !metadata.file_type().is_symlink() && metadata.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn labeled_file_values<'a>(
    operation: &str,
    args: &'a [NativeBoundaryValue],
) -> Result<&'a [NativeBoundaryValue], DispatchError> {
    let Some(NativeBoundaryValue::List(values)) = args.first() else {
        return Err(DispatchError::new(
            "boundary.type",
            format!("{operation} argument 0 must be List[LabeledFile]"),
            0,
        ));
    };
    if values.len() > MAXIMUM_LABELED_FILES {
        return Err(DispatchError::new(
            "file.invalid_path",
            format!("labeled-file operation exceeds {MAXIMUM_LABELED_FILES} files"),
            0,
        ));
    }
    Ok(values)
}

fn checked_labeled_file<'a>(
    operation: &str,
    value: &'a NativeBoundaryValue,
    index: usize,
) -> Result<(&'a str, &'a str), DispatchError> {
    let NativeBoundaryValue::Record { name, fields } = value else {
        return Err(labeled_file_type_error(operation, index));
    };
    if name != "LabeledFile" {
        return Err(labeled_file_type_error(operation, index));
    }
    let path = labeled_file_field(operation, fields, "path", index)?;
    let label = labeled_file_field(operation, fields, "label", index)?;
    if !safe_relative_path(label) {
        return Err(DispatchError::new(
            "file.invalid_path",
            "labeled-file digest labels must be safe relative paths",
            0,
        )
        .with_path(label));
    }
    let metadata =
        std::fs::metadata(path).map_err(|error| dispatch_file_error(operation, path, error))?;
    if !metadata.is_file() {
        return Err(DispatchError::new(
            "file.invalid_path",
            "labeled-file digest source must be a regular file",
            0,
        )
        .with_path(path));
    }
    Ok((path, label))
}

struct ForbiddenMatcher {
    patterns: RegexSet,
    overlap: usize,
}

fn checked_forbidden_fragments(
    operation: &str,
    values: &[&str],
) -> Result<Option<ForbiddenMatcher>, DispatchError> {
    if values.len() > MAXIMUM_FORBIDDEN_FRAGMENTS {
        return Err(DispatchError::new(
            "file.invalid_path",
            format!("{operation} exceeds {MAXIMUM_FORBIDDEN_FRAGMENTS} forbidden fragments"),
            1,
        ));
    }
    let patterns = values
        .iter()
        .map(|value| {
            if value.is_empty() || value.len() > MAXIMUM_FORBIDDEN_FRAGMENT_BYTES {
                Err(DispatchError::new(
                    "file.invalid_path",
                    format!(
                        "{operation} forbidden fragments must contain 1..={MAXIMUM_FORBIDDEN_FRAGMENT_BYTES} bytes"
                    ),
                    1,
                ))
            } else {
                Ok(regex::escape(value))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if patterns.is_empty() {
        return Ok(None);
    }
    let overlap = patterns
        .iter()
        .map(|pattern| pattern.len().saturating_sub(1))
        .max()
        .unwrap_or_default();
    let patterns = RegexSet::new(patterns).map_err(|error| {
        DispatchError::new(
            "file.invalid_path",
            format!("{operation} cannot compile forbidden fragments: {error}"),
            1,
        )
    })?;
    Ok(Some(ForbiddenMatcher { patterns, overlap }))
}

fn sha256_file_and_scan(
    operation: &str,
    path: &str,
    matcher: &Option<ForbiddenMatcher>,
) -> Result<(String, bool), DispatchError> {
    let mut file =
        std::fs::File::open(path).map_err(|error| dispatch_file_error(operation, path, error))?;
    let mut digest = sha2::Sha256::new();
    let mut buffer = [0_u8; 65_536];
    let overlap = matcher.as_ref().map_or(0, |value| value.overlap);
    let mut tail = Vec::with_capacity(overlap);
    let mut window = Vec::new();
    let mut matched = false;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| dispatch_file_error(operation, path, error))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        if !matched {
            if let Some(matcher) = matcher {
                if tail.is_empty() {
                    matched = matcher.patterns.is_match(&buffer[..count]);
                } else {
                    window.clear();
                    window.reserve(tail.len().saturating_add(count));
                    window.extend_from_slice(&tail);
                    window.extend_from_slice(&buffer[..count]);
                    matched = matcher.patterns.is_match(&window);
                }
                tail.clear();
                let retained = overlap.min(count);
                tail.extend_from_slice(&buffer[count.saturating_sub(retained)..count]);
            }
        }
    }
    Ok((hex_digest(digest.finalize()), matched))
}

fn update_labeled_digest(digest: &mut sha2::Sha256, label: &str, file_digest: &str) {
    digest.update(label.replace('\\', "/").as_bytes());
    digest.update([0]);
    digest.update(file_digest.as_bytes());
    digest.update([b'\n']);
}

fn labeled_file_field<'a>(
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

fn labeled_file_type_error(operation: &str, index: usize) -> DispatchError {
    DispatchError::new(
        "boundary.type",
        format!("{operation} argument 0 item {index} must be LabeledFile"),
        0,
    )
}

fn valid_sha256(value: &str) -> bool {
    value.len() == SHA256_HEX_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn sha256_reader(
    operation: &str,
    path: &str,
    reader: &mut impl Read,
) -> Result<String, DispatchError> {
    let mut digest = sha2::Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| dispatch_file_error(operation, path, error))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex_digest(digest.finalize()))
}

fn update_framed_text(
    digest: &mut sha2::Sha256,
    operation: &str,
    value: &str,
) -> Result<(), DispatchError> {
    let bytes = value.as_bytes();
    let length = u64::try_from(bytes.len()).map_err(|_| {
        DispatchError::new(
            "dispatch.hash_field_too_large",
            format!("{operation} field length does not fit u64"),
            0,
        )
    })?;
    digest.update(length.to_be_bytes());
    digest.update(bytes);
    Ok(())
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
#[path = "hash_test.rs"]
mod test;
