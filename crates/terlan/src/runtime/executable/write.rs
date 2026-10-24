//! Streaming executable payload emission; native code is already compiled.

use std::fs::OpenOptions;
use std::io::Write;

use super::*;

/// Appends resources and a bounded descriptor to a private runtime copy.
pub(crate) fn append_application(
    runtime_copy: &Path,
    entry: ApplicationEntry,
    sources: &[(String, PathBuf)],
    environment: Vec<ArtifactEnvironment>,
) -> Result<(), String> {
    if is_application(runtime_copy)? {
        return Err(error("runtime", "expected a bare application runtime"));
    }
    let mut output = OpenOptions::new()
        .append(true)
        .open(runtime_copy)
        .map_err(io_error)?;
    let mut digest = Sha256::new();
    let mut files = Vec::new();
    let mut payload_len = 0_u64;
    for (path, source) in sources {
        validate_path(path)?;
        let metadata = std::fs::symlink_metadata(source).map_err(io_error)?;
        if !metadata.file_type().is_file() {
            return Err(error(
                "resource",
                "embedded resources must be regular files",
            ));
        }
        let mut input = File::open(source).map_err(io_error)?;
        let mut resource_digest = Sha256::new();
        let mut length = 0_u64;
        let mut buffer = [0; 65_536];
        loop {
            let read = input.read(&mut buffer).map_err(io_error)?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read]).map_err(io_error)?;
            digest.update(&buffer[..read]);
            resource_digest.update(&buffer[..read]);
            length = length
                .checked_add(read as u64)
                .ok_or_else(|| error("bounds", "resource size overflow"))?;
        }
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = false;
        files.push(Resource {
            path: path.clone(),
            offset: payload_len,
            length,
            sha256: resource_digest.finalize().into(),
            executable,
        });
        payload_len = payload_len
            .checked_add(length)
            .ok_or_else(|| error("bounds", "application size overflow"))?;
    }
    let manifest = Manifest {
        entry,
        files,
        environment,
    };
    finish_payload(&mut output, &manifest, payload_len, digest)?;
    output.sync_all().map_err(io_error)
}
