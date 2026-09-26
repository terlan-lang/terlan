//! Deterministic Cloud release archives and source revision metadata.

use super::*;

pub(super) fn create_release_archive(release_root: &Path, archive: &Path) -> Result<(), String> {
    let checksums = read_json(&release_root.join("checksums.json"), "release checksums")?;
    require_schema(
        &checksums,
        "terlan-cloud-release-checksums-v1",
        "release checksums",
    )?;
    let files = checksums
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| "error[cloud_release_checksums]: files are missing".to_string())?;
    let mut paths = Vec::with_capacity(files.len() + 1);
    for file in files {
        let path = required_json_string(file, "path", "release checksums")?;
        paths.push(PathBuf::from(path));
    }
    paths.push(PathBuf::from("checksums.json"));
    if let Some(parent) = archive.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create Cloud output directory: {error}"))?;
    }
    if archive.exists() {
        fs::remove_file(archive)
            .map_err(|error| format!("cannot replace release archive: {error}"))?;
    }
    terlan_archive::create_tar_zstd_files_with_limits(
        release_root,
        &paths,
        archive,
        terlan_archive::TarZstdCreateLimits {
            max_files: 16_384,
            max_path_bytes: 240,
            max_unpacked_bytes: MAX_RELEASE_BYTES,
            compression_level: RELEASE_COMPRESSION_LEVEL,
        },
    )
    .map_err(|error| format!("error[cloud_release_archive]: {error}"))?;
    Ok(())
}

pub(super) fn file_sha256(path: &Path) -> Result<(String, u64), terlan_runtime_abi::BoundaryError> {
    use terlan_runtime_abi::{BoundaryError, ErrorDomain};

    let mut file = File::open(path).map_err(|error| {
        BoundaryError::sourced(
            ErrorDomain::CommandExecution,
            "cloud_release_hash",
            "read Cloud release archive",
            format!("cannot read {}: {error}", path.display()),
            error,
        )
    })?;
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| {
            BoundaryError::sourced(
                ErrorDomain::CommandExecution,
                "cloud_release_hash",
                "hash Cloud release archive",
                format!("cannot hash {}: {error}", path.display()),
                error,
            )
        })?;
        if count == 0 {
            break;
        }
        bytes = bytes.checked_add(count as u64).ok_or_else(|| {
            BoundaryError::message(
                ErrorDomain::CommandExecution,
                "hash Cloud release archive",
                "release archive byte count overflow",
            )
        })?;
        digest.update(&buffer[..count]);
    }
    let sha256 = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((sha256, bytes))
}

pub(super) fn git_revision(project_dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", project_dir.to_str()?, "rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let revision = String::from_utf8(output.stdout).ok()?;
    Some(revision.trim().to_string()).filter(|value| !value.is_empty())
}
