//! Safe deterministic creation and extraction for gzip-tar and ZIP artifacts.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use flate2::{read::GzDecoder, Compression, GzBuilder};
use zip::write::SimpleFileOptions;

/// Stable failure returned by deterministic archive operations.
#[derive(Debug)]
pub struct ArchiveError {
    code: &'static str,
    archive: String,
    destination: String,
    message: String,
}

impl ArchiveError {
    /// Returns the stable machine-readable failure code.
    pub const fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the archive path involved in the failure.
    pub fn archive(&self) -> &str {
        &self.archive
    }

    /// Returns the source or destination path involved in the failure.
    pub fn destination(&self) -> &str {
        &self.destination
    }

    /// Returns the backend-independent failure description.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} (archive `{}`, destination `{}`)",
            self.message, self.archive, self.destination
        )
    }
}

impl std::error::Error for ArchiveError {}

struct ArchiveEntry {
    absolute: PathBuf,
    relative: PathBuf,
    directory: bool,
    executable: bool,
}

/// Deterministic `.tar.zst` creation result for an explicit file set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TarZstdSummary {
    pub file_count: u32,
    pub unpacked_bytes: u64,
}

/// Creates a deterministic zstd-compressed tar from explicit relative files.
///
/// Paths must be UTF-8, relative, traversal-free regular files beneath
/// `source`. Symbolic links, duplicates, more than 4,096 files, paths longer
/// than 240 bytes, and more than 256 MiB of input are rejected.
pub fn create_tar_zstd_files(
    source: &Path,
    relative_files: &[PathBuf],
    archive: &Path,
) -> Result<TarZstdSummary, ArchiveError> {
    const MAX_FILES: usize = 4_096;
    const MAX_PATH_BYTES: usize = 240;
    const MAX_UNPACKED_BYTES: u64 = 256 * 1024 * 1024;

    let archive_name = archive.display().to_string();
    let source_name = source.display().to_string();
    if archive.exists() {
        return Err(error(
            "archive.destination_exists",
            &archive_name,
            &source_name,
            "archive output already exists",
        ));
    }
    let mut files = relative_files.to_vec();
    files.sort();
    files.dedup();
    if files.len() != relative_files.len() || files.len() > MAX_FILES {
        return Err(error(
            "archive.limit",
            &archive_name,
            &source_name,
            "archive contains duplicate paths or exceeds 4096 files",
        ));
    }

    let mut entries = Vec::with_capacity(files.len());
    let mut unpacked_bytes = 0_u64;
    for relative in files {
        let valid_components = relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
        let Some(relative_text) = relative.to_str() else {
            return Err(error(
                "archive.unsafe_entry",
                &archive_name,
                &source_name,
                "archive paths must be UTF-8",
            ));
        };
        if !valid_components
            || relative_text.is_empty()
            || relative_text.len() > MAX_PATH_BYTES
            || relative_text.contains('\\')
        {
            return Err(error(
                "archive.unsafe_entry",
                &archive_name,
                &source_name,
                "archive path is absolute, traversing, non-portable, or too long",
            ));
        }
        let absolute = source.join(&relative);
        let metadata = fs::symlink_metadata(&absolute)
            .map_err(|failure| io_error(&archive_name, &source_name, failure))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(error(
                "archive.unsafe_entry",
                &archive_name,
                &source_name,
                "archive inputs must be regular files and may not be links",
            ));
        }
        unpacked_bytes = unpacked_bytes.checked_add(metadata.len()).ok_or_else(|| {
            error(
                "archive.limit",
                &archive_name,
                &source_name,
                "archive unpacked byte count overflow",
            )
        })?;
        if unpacked_bytes > MAX_UNPACKED_BYTES {
            return Err(error(
                "archive.limit",
                &archive_name,
                &source_name,
                "archive exceeds 256 MiB unpacked",
            ));
        }
        entries.push(ArchiveEntry {
            absolute,
            relative,
            directory: false,
            executable: metadata_executable(&metadata),
        });
    }

    create_parent(archive, &archive_name, &source_name)?;
    let result = create_tar_zstd(archive, &entries, &archive_name, &source_name);
    if result.is_err() {
        let _ = fs::remove_file(archive);
    }
    result.map(|()| TarZstdSummary {
        file_count: entries.len() as u32,
        unpacked_bytes,
    })
}

fn create_tar_zstd(
    archive_path: &Path,
    entries: &[ArchiveEntry],
    archive: &str,
    source: &str,
) -> Result<(), ArchiveError> {
    let output =
        fs::File::create(archive_path).map_err(|failure| io_error(archive, source, failure))?;
    let encoder = zstd::stream::write::Encoder::new(output, 19)
        .map_err(|failure| io_error(archive, source, failure))?;
    let mut builder = tar::Builder::new(encoder);
    builder.mode(tar::HeaderMode::Deterministic);
    for entry in entries {
        let mut input = fs::File::open(&entry.absolute)
            .map_err(|failure| io_error(archive, source, failure))?;
        let mut header = tar::Header::new_gnu();
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(
            input
                .metadata()
                .map_err(|failure| io_error(archive, source, failure))?
                .len(),
        );
        header.set_mode(if entry.executable { 0o755 } else { 0o644 });
        header.set_cksum();
        builder
            .append_data(&mut header, &entry.relative, &mut input)
            .map_err(|failure| io_error(archive, source, failure))?;
    }
    builder
        .finish()
        .map_err(|failure| io_error(archive, source, failure))?;
    let encoder = builder
        .into_inner()
        .map_err(|failure| io_error(archive, source, failure))?;
    encoder
        .finish()
        .map(|_| ())
        .map_err(|failure| io_error(archive, source, failure))
}

/// Creates a deterministic `.tar.gz` or `.zip` archive from a real directory.
///
/// Entries are ordered, unsafe symbolic-link inputs are rejected, and a failed
/// operation removes any partial archive output.
pub fn create(source: &str, archive: &str) -> Result<(), ArchiveError> {
    let source_path = Path::new(source);
    let archive_path = Path::new(archive);
    if archive_path.exists() {
        return Err(error(
            "archive.destination_exists",
            archive,
            source,
            "archive output already exists",
        ));
    }
    let metadata =
        fs::symlink_metadata(source_path).map_err(|failure| io_error(archive, source, failure))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(error(
            "archive.unsafe_entry",
            archive,
            source,
            "archive source must be a real directory",
        ));
    }
    let mut entries = Vec::new();
    collect_entries(source_path, source_path, archive, source, &mut entries)?;
    entries.sort_by(|left, right| left.relative.cmp(&right.relative));
    create_parent(archive_path, archive, source)?;
    let result = if archive.ends_with(".tar.gz") {
        create_tar_gzip(archive_path, &entries, archive, source)
    } else if archive.ends_with(".zip") {
        create_zip(archive_path, &entries, archive, source)
    } else {
        Err(error(
            "archive.unsupported_format",
            archive,
            source,
            "supported archive suffixes are .tar.gz and .zip",
        ))
    };
    if result.is_err() {
        let _ = fs::remove_file(archive_path);
    }
    result
}

fn collect_entries(
    root: &Path,
    directory: &Path,
    archive: &str,
    source: &str,
    entries: &mut Vec<ArchiveEntry>,
) -> Result<(), ArchiveError> {
    let mut children = fs::read_dir(directory)
        .map_err(|failure| io_error(archive, source, failure))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|failure| io_error(archive, source, failure))?;
    children.sort_by_key(fs::DirEntry::file_name);
    for child in children {
        let absolute = child.path();
        let metadata = fs::symlink_metadata(&absolute)
            .map_err(|failure| io_error(archive, source, failure))?;
        let relative = absolute
            .strip_prefix(root)
            .map(Path::to_path_buf)
            .map_err(|failure| {
                error("archive.unsafe_entry", archive, source, failure.to_string())
            })?;
        if metadata.file_type().is_symlink() {
            return Err(error(
                "archive.unsafe_entry",
                archive,
                source,
                "archive source links are not permitted",
            ));
        }
        if metadata.is_dir() {
            entries.push(ArchiveEntry {
                absolute: absolute.clone(),
                relative,
                directory: true,
                executable: false,
            });
            collect_entries(root, &absolute, archive, source, entries)?;
        } else if metadata.is_file() {
            entries.push(ArchiveEntry {
                absolute,
                relative,
                directory: false,
                executable: metadata_executable(&metadata),
            });
        } else {
            return Err(error(
                "archive.unsafe_entry",
                archive,
                source,
                "archive source special files are not permitted",
            ));
        }
    }
    Ok(())
}

fn create_tar_gzip(
    archive_path: &Path,
    entries: &[ArchiveEntry],
    archive: &str,
    source: &str,
) -> Result<(), ArchiveError> {
    let output =
        fs::File::create(archive_path).map_err(|failure| io_error(archive, source, failure))?;
    let encoder = GzBuilder::new()
        .mtime(0)
        .write(output, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.mode(tar::HeaderMode::Deterministic);
    for entry in entries {
        let mut header = tar::Header::new_gnu();
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        if entry.directory {
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, &entry.relative, io::empty())
                .map_err(|failure| io_error(archive, source, failure))?;
        } else {
            let mut input = fs::File::open(&entry.absolute)
                .map_err(|failure| io_error(archive, source, failure))?;
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(
                input
                    .metadata()
                    .map_err(|failure| io_error(archive, source, failure))?
                    .len(),
            );
            header.set_mode(if entry.executable { 0o755 } else { 0o644 });
            header.set_cksum();
            builder
                .append_data(&mut header, &entry.relative, &mut input)
                .map_err(|failure| io_error(archive, source, failure))?;
        }
    }
    builder
        .finish()
        .map_err(|failure| io_error(archive, source, failure))?;
    let encoder = builder
        .into_inner()
        .map_err(|failure| io_error(archive, source, failure))?;
    encoder
        .finish()
        .map(|_| ())
        .map_err(|failure| io_error(archive, source, failure))
}

fn create_zip(
    archive_path: &Path,
    entries: &[ArchiveEntry],
    archive: &str,
    source: &str,
) -> Result<(), ArchiveError> {
    let output =
        fs::File::create(archive_path).map_err(|failure| io_error(archive, source, failure))?;
    let mut writer = zip::ZipWriter::new(output);
    for entry in entries {
        let name = normalized_archive_name(&entry.relative, entry.directory, archive, source)?;
        let mode = if entry.directory || entry.executable {
            0o755
        } else {
            0o644
        };
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(mode);
        if entry.directory {
            writer
                .add_directory(name, options)
                .map_err(|failure| archive_format_error(archive, source, failure))?;
        } else {
            writer
                .start_file(name, options)
                .map_err(|failure| archive_format_error(archive, source, failure))?;
            let mut input = fs::File::open(&entry.absolute)
                .map_err(|failure| io_error(archive, source, failure))?;
            io::copy(&mut input, &mut writer)
                .map_err(|failure| io_error(archive, source, failure))?;
        }
    }
    writer
        .finish()
        .map(|_| ())
        .map_err(|failure| archive_format_error(archive, source, failure))
}

fn normalized_archive_name(
    relative: &Path,
    directory: bool,
    archive: &str,
    source: &str,
) -> Result<String, ArchiveError> {
    let mut name = relative
        .to_str()
        .ok_or_else(|| {
            error(
                "archive.unsafe_entry",
                archive,
                source,
                "archive paths must be UTF-8",
            )
        })?
        .replace('\\', "/");
    if directory {
        name.push('/');
    }
    Ok(name)
}

#[cfg(unix)]
fn metadata_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn metadata_executable(_metadata: &fs::Metadata) -> bool {
    false
}

fn archive_format_error(
    archive: &str,
    destination: &str,
    failure: zip::result::ZipError,
) -> ArchiveError {
    error(
        "archive.invalid_archive",
        archive,
        destination,
        failure.to_string(),
    )
}

/// Extracts a supported archive into a new destination directory.
///
/// Extraction rejects path traversal, symbolic links, existing destinations,
/// and unsupported formats; a failed operation removes its partial directory.
pub fn extract(archive: &str, destination: &str) -> Result<(), ArchiveError> {
    let archive_path = Path::new(archive);
    let destination_path = Path::new(destination);
    if destination_path.exists() {
        return Err(error(
            "archive.destination_exists",
            archive,
            destination,
            "archive destination already exists",
        ));
    }
    fs::create_dir_all(destination_path).map_err(|failure| {
        error(
            "archive.io_failure",
            archive,
            destination,
            failure.to_string(),
        )
    })?;
    let result = if archive.ends_with(".tar.gz") {
        extract_tar_gzip(archive_path, destination_path, archive, destination)
    } else if archive.ends_with(".zip") {
        extract_zip(archive_path, destination_path, archive, destination)
    } else {
        Err(error(
            "archive.unsupported_format",
            archive,
            destination,
            "supported archive suffixes are .tar.gz and .zip",
        ))
    };
    if result.is_err() {
        let _ = fs::remove_dir_all(destination_path);
    }
    result
}

/// Extracts a zstd-compressed tar artifact with path and entry validation.
///
/// Regular files and directories are admitted. Symbolic links, special files,
/// traversal, oversized paths, excessive entries, and archive bombs are
/// rejected before their entry is unpacked.
pub fn extract_tar_zstd(archive: &Path, destination: &Path) -> Result<(), ArchiveError> {
    const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
    const MAX_UNPACKED_BYTES: u64 = 256 * 1024 * 1024;
    const MAX_ENTRIES: u32 = 4_096;
    const MAX_PATH_BYTES: usize = 240;

    let archive_text = archive.to_string_lossy();
    let destination_text = destination.to_string_lossy();
    if destination.exists() {
        return Err(error(
            "archive.destination_exists",
            &archive_text,
            &destination_text,
            "archive destination already exists",
        ));
    }
    let compressed_bytes = fs::metadata(archive)
        .map_err(|failure| io_error(&archive_text, &destination_text, failure))?
        .len();
    if compressed_bytes > MAX_ARCHIVE_BYTES {
        return Err(error(
            "archive.limit",
            &archive_text,
            &destination_text,
            "archive exceeds 64 MiB compressed",
        ));
    }
    fs::create_dir_all(destination)
        .map_err(|failure| io_error(&archive_text, &destination_text, failure))?;
    let result = extract_tar_zstd_bounded(
        archive,
        destination,
        &archive_text,
        &destination_text,
        MAX_UNPACKED_BYTES,
        MAX_ENTRIES,
        MAX_PATH_BYTES,
    );
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

fn extract_tar_zstd_bounded(
    archive: &Path,
    destination: &Path,
    archive_text: &str,
    destination_text: &str,
    max_unpacked_bytes: u64,
    max_entries: u32,
    max_path_bytes: usize,
) -> Result<(), ArchiveError> {
    let file = fs::File::open(archive)
        .map_err(|failure| io_error(archive_text, destination_text, failure))?;
    let decoder = zstd::stream::read::Decoder::new(file).map_err(|failure| {
        error(
            "archive.invalid_archive",
            archive_text,
            destination_text,
            format!("invalid zstd stream: {failure}"),
        )
    })?;
    let mut archive_reader = tar::Archive::new(decoder);
    let entries = archive_reader.entries().map_err(|failure| {
        error(
            "archive.invalid_archive",
            archive_text,
            destination_text,
            format!("invalid tar stream: {failure}"),
        )
    })?;
    let mut entry_count = 0_u32;
    let mut unpacked_bytes = 0_u64;
    for entry in entries {
        entry_count = entry_count.checked_add(1).ok_or_else(|| {
            error(
                "archive.limit",
                archive_text,
                destination_text,
                "archive entry count overflow",
            )
        })?;
        if entry_count > max_entries {
            return Err(error(
                "archive.limit",
                archive_text,
                destination_text,
                "archive exceeds 4096 entries",
            ));
        }
        let mut entry = entry.map_err(|failure| {
            error(
                "archive.invalid_archive",
                archive_text,
                destination_text,
                format!("invalid tar entry: {failure}"),
            )
        })?;
        let path = entry
            .path()
            .map_err(|failure| {
                error(
                    "archive.unsafe_entry",
                    archive_text,
                    destination_text,
                    format!("invalid archive path: {failure}"),
                )
            })?
            .into_owned();
        if !safe_relative_path(&path) {
            return Err(error(
                "archive.unsafe_entry",
                archive_text,
                destination_text,
                format!("archive entry escapes destination: {}", path.display()),
            ));
        }
        let path_text = path.to_str().ok_or_else(|| {
            error(
                "archive.unsafe_entry",
                archive_text,
                destination_text,
                "archive path must be UTF-8",
            )
        })?;
        if path_text.len() > max_path_bytes || path_text.contains('\\') {
            return Err(error(
                "archive.limit",
                archive_text,
                destination_text,
                "archive path exceeds 240 bytes or is non-portable",
            ));
        }
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir()) {
            return Err(error(
                "archive.unsafe_entry",
                archive_text,
                destination_text,
                format!(
                    "archive links and special entries are forbidden: {}",
                    path.display()
                ),
            ));
        }
        if entry_type.is_file() {
            unpacked_bytes = unpacked_bytes.checked_add(entry.size()).ok_or_else(|| {
                error(
                    "archive.limit",
                    archive_text,
                    destination_text,
                    "archive unpacked byte count overflow",
                )
            })?;
            if unpacked_bytes > max_unpacked_bytes {
                return Err(error(
                    "archive.limit",
                    archive_text,
                    destination_text,
                    "archive exceeds 256 MiB unpacked",
                ));
            }
        }
        if !entry.unpack_in(destination).map_err(|failure| {
            error(
                "archive.invalid_archive",
                archive_text,
                destination_text,
                format!("cannot extract archive entry: {failure}"),
            )
        })? {
            return Err(error(
                "archive.unsafe_entry",
                archive_text,
                destination_text,
                format!("archive entry escapes destination: {}", path.display()),
            ));
        }
    }
    Ok(())
}

fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn extract_tar_gzip(
    archive_path: &Path,
    destination_path: &Path,
    archive: &str,
    destination: &str,
) -> Result<(), ArchiveError> {
    let file =
        fs::File::open(archive_path).map_err(|failure| io_error(archive, destination, failure))?;
    let decoder = GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    let entries = tar.entries().map_err(|failure| {
        error(
            "archive.invalid_archive",
            archive,
            destination,
            failure.to_string(),
        )
    })?;
    for entry in entries {
        let mut entry = entry.map_err(|failure| {
            error(
                "archive.invalid_archive",
                archive,
                destination,
                failure.to_string(),
            )
        })?;
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir()) {
            return Err(error(
                "archive.unsafe_entry",
                archive,
                destination,
                "archive links and special entries are not permitted",
            ));
        }
        let unpacked = entry.unpack_in(destination_path).map_err(|failure| {
            error(
                "archive.invalid_archive",
                archive,
                destination,
                failure.to_string(),
            )
        })?;
        if !unpacked {
            return Err(error(
                "archive.unsafe_entry",
                archive,
                destination,
                "archive entry escapes the destination",
            ));
        }
    }
    Ok(())
}

fn extract_zip(
    archive_path: &Path,
    destination_path: &Path,
    archive: &str,
    destination: &str,
) -> Result<(), ArchiveError> {
    let file =
        fs::File::open(archive_path).map_err(|failure| io_error(archive, destination, failure))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|failure| {
        error(
            "archive.invalid_archive",
            archive,
            destination,
            failure.to_string(),
        )
    })?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|failure| {
            error(
                "archive.invalid_archive",
                archive,
                destination,
                failure.to_string(),
            )
        })?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(error(
                "archive.unsafe_entry",
                archive,
                destination,
                "archive links are not permitted",
            ));
        }
        let relative = entry.enclosed_name().ok_or_else(|| {
            error(
                "archive.unsafe_entry",
                archive,
                destination,
                "archive entry escapes the destination",
            )
        })?;
        let output = destination_path.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output)
                .map_err(|failure| io_error(archive, destination, failure))?;
            continue;
        }
        create_parent(&output, archive, destination)?;
        let mut output_file =
            fs::File::create(&output).map_err(|failure| io_error(archive, destination, failure))?;
        io::copy(&mut entry, &mut output_file)
            .map_err(|failure| io_error(archive, destination, failure))?;
        apply_zip_mode(&output, entry.unix_mode(), archive, destination)?;
    }
    Ok(())
}

#[cfg(unix)]
fn apply_zip_mode(
    output: &Path,
    mode: Option<u32>,
    archive: &str,
    destination: &str,
) -> Result<(), ArchiveError> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(mode) = mode {
        fs::set_permissions(output, fs::Permissions::from_mode(mode & 0o777))
            .map_err(|failure| io_error(archive, destination, failure))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn apply_zip_mode(
    _output: &Path,
    _mode: Option<u32>,
    _archive: &str,
    _destination: &str,
) -> Result<(), ArchiveError> {
    Ok(())
}

fn create_parent(output: &Path, archive: &str, destination: &str) -> Result<(), ArchiveError> {
    let parent: PathBuf = output
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(destination));
    fs::create_dir_all(parent).map_err(|failure| io_error(archive, destination, failure))
}

fn io_error(archive: &str, destination: &str, failure: io::Error) -> ArchiveError {
    error(
        "archive.io_failure",
        archive,
        destination,
        failure.to_string(),
    )
}

fn error(
    code: &'static str,
    archive: &str,
    destination: &str,
    message: impl Into<String>,
) -> ArchiveError {
    ArchiveError {
        code,
        archive: archive.to_string(),
        destination: destination.to_string(),
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
