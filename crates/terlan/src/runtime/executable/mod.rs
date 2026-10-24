//! Application images and resources embedded in a single runtime executable.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod materialize;
#[cfg(target_os = "linux")]
mod worker;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod write;

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use write::append_application;

const MAGIC: &[u8; 16] = b"TERLAN_APP_V1\0\0\0";
const FOOTER_LEN: u64 = 64;
const MAX_MANIFEST_LEN: u64 = 8 * 1024 * 1024;

/// The worker launcher selects this role inside the existing closed sandbox.
pub(crate) const ROLE_ENV: &str = "TERLAN_INTERNAL_EXECUTABLE_ROLE";
pub(crate) const WORKER_ROLE: &str = "native-worker";

/// Entry policy is embedded with the application, independent of its filename.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ApplicationEntry {
    Cli { image: String, function: String },
    Service { web_root: String },
    Worker,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    entry: ApplicationEntry,
    files: Vec<Resource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    environment: Vec<ArtifactEnvironment>,
}

/// One compiler-selected native artifact, addressed inside the executable.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactEnvironment {
    pub(crate) name: String,
    pub(crate) path: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Resource {
    path: String,
    offset: u64,
    length: u64,
    sha256: [u8; 32],
    executable: bool,
}

/// One open executable handle retains the exact admitted application snapshot.
pub(crate) struct EmbeddedApplication {
    file: File,
    payload_start: u64,
    manifest: Manifest,
}

impl EmbeddedApplication {
    /// Linux's process file identifies the running inode even during replacement.
    pub(crate) fn open_current() -> Result<Option<Self>, String> {
        #[cfg(target_os = "linux")]
        {
            Self::open(Path::new("/proc/self/exe"))
        }
        #[cfg(not(target_os = "linux"))]
        {
            Self::open(&std::env::current_exe().map_err(io_error)?)
        }
    }

    /// Returns `None` for a bare runtime; malformed packaged applications fail.
    pub(crate) fn open(path: &Path) -> Result<Option<Self>, String> {
        let mut file = File::open(path).map_err(io_error)?;
        let Some(layout) = Layout::read(&mut file)? else {
            return Ok(None);
        };
        file.seek(SeekFrom::Start(layout.payload_start))
            .map_err(io_error)?;
        let digest = hash_reader(&mut file.by_ref().take(layout.payload_len))?;
        if digest != layout.sha256 {
            return Err(error("digest", "embedded application bytes changed"));
        }
        let manifest_start = layout.payload_start + layout.payload_len - layout.manifest_len;
        file.seek(SeekFrom::Start(manifest_start))
            .map_err(io_error)?;
        let mut bytes = Vec::new();
        file.by_ref()
            .take(layout.manifest_len)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        let manifest: Manifest = serde_json::from_slice(&bytes)
            .map_err(|cause| error("manifest", &cause.to_string()))?;
        validate_manifest(&manifest, layout.payload_len - layout.manifest_len)?;
        Ok(Some(Self {
            file,
            payload_start: layout.payload_start,
            manifest,
        }))
    }

    pub(crate) fn entry(&self) -> &ApplicationEntry {
        &self.manifest.entry
    }

    pub(crate) fn environment(&self) -> &[ArtifactEnvironment] {
        &self.manifest.environment
    }
}

/// Returns a private snapshot of the admitted runtime for supervised workers.
pub(crate) fn pinned_worker_path() -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "linux")]
    {
        worker::path()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(None)
    }
}

impl EmbeddedApplication {
    pub(crate) fn pin_for_workers(
        &self,
        files: &materialize::ApplicationFiles,
    ) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            worker::pin(self, files.root())?;
        }
        #[cfg(not(target_os = "linux"))]
        let _ = files;
        Ok(())
    }
}

/// Worker selection reads only the bounded footer, without loading resources.
pub(crate) fn is_application(path: &Path) -> Result<bool, String> {
    let mut file = File::open(path).map_err(io_error)?;
    Ok(Layout::read(&mut file)?.is_some())
}

struct Layout {
    payload_start: u64,
    payload_len: u64,
    manifest_len: u64,
    sha256: [u8; 32],
}

impl Layout {
    fn read(file: &mut File) -> Result<Option<Self>, String> {
        let length = file.metadata().map_err(io_error)?.len();
        if length < FOOTER_LEN {
            return Ok(None);
        }
        file.seek(SeekFrom::End(-(FOOTER_LEN as i64)))
            .map_err(io_error)?;
        let mut footer = [0; FOOTER_LEN as usize];
        file.read_exact(&mut footer).map_err(io_error)?;
        if &footer[..16] != MAGIC {
            return Ok(None);
        }
        let payload_len = u64::from_le_bytes(footer[16..24].try_into().unwrap());
        let manifest_len = u64::from_le_bytes(footer[24..32].try_into().unwrap());
        let payload_start = length
            .checked_sub(FOOTER_LEN)
            .and_then(|n| n.checked_sub(payload_len))
            .filter(|start| *start > 0)
            .ok_or_else(|| error("bounds", "embedded application exceeds the executable"))?;
        if manifest_len == 0 || manifest_len > MAX_MANIFEST_LEN || manifest_len > payload_len {
            return Err(error("bounds", "invalid embedded manifest length"));
        }
        Ok(Some(Self {
            payload_start,
            payload_len,
            manifest_len,
            sha256: footer[32..64].try_into().unwrap(),
        }))
    }
}

fn validate_manifest(manifest: &Manifest, data_len: u64) -> Result<(), String> {
    if (manifest.files.is_empty() && manifest.entry != ApplicationEntry::Worker)
        || manifest.files.len() > 65_536
    {
        return Err(error("manifest", "invalid embedded resource count"));
    }
    let mut paths = BTreeSet::new();
    let mut expected_offset = 0;
    for resource in &manifest.files {
        validate_path(&resource.path)?;
        if !paths.insert(resource.path.to_ascii_lowercase()) {
            return Err(error("manifest", "duplicate embedded resource path"));
        }
        if resource.offset != expected_offset {
            return Err(error("bounds", "embedded resources must be contiguous"));
        }
        expected_offset = resource
            .offset
            .checked_add(resource.length)
            .filter(|end| *end <= data_len)
            .ok_or_else(|| error("bounds", "embedded resource exceeds its payload"))?;
    }
    if expected_offset != data_len {
        return Err(error("bounds", "unclaimed embedded resource bytes"));
    }
    let mut variables = BTreeSet::new();
    if manifest.environment.len() > 256 {
        return Err(error("environment", "too many native artifact bindings"));
    }
    for binding in &manifest.environment {
        let name = binding.name.as_bytes();
        if name.is_empty()
            || name.len() > 256
            || !(name[0].is_ascii_alphabetic() || name[0] == b'_')
            || !name
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            || binding.name.eq_ignore_ascii_case(ROLE_ENV)
            || !variables.insert(binding.name.to_ascii_uppercase())
        {
            return Err(error(
                "environment",
                "invalid or duplicate native artifact binding",
            ));
        }
        validate_path(&binding.path)?;
        if !manifest
            .files
            .iter()
            .any(|resource| resource.path == binding.path)
        {
            return Err(error(
                "environment",
                "native artifact is missing from the executable",
            ));
        }
    }
    match &manifest.entry {
        ApplicationEntry::Worker => {
            if !manifest.files.is_empty() || !manifest.environment.is_empty() {
                return Err(error(
                    "worker",
                    "worker runtime must not contain application resources",
                ));
            }
        }
        ApplicationEntry::Cli { image, function } => {
            validate_path(image)?;
            if function.is_empty() || !manifest.files.iter().any(|file| &file.path == image) {
                return Err(error("entry", "application entry image is missing"));
            }
        }
        ApplicationEntry::Service { web_root } => {
            validate_path(web_root)?;
            let prefix = format!("{web_root}/");
            if !manifest
                .files
                .iter()
                .any(|file| file.path.starts_with(&prefix))
            {
                return Err(error("entry", "application service resources are missing"));
            }
        }
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 4096
        || path.contains(['\\', ':', '\0'])
        || path.split('/').any(|part| {
            part.is_empty() || part == "." || part == ".." || part.ends_with(['.', ' '])
        })
        || Path::new(path).is_absolute()
    {
        return Err(error(
            "path",
            "embedded resource must have a canonical relative path",
        ));
    }
    for part in path.split('/') {
        let stem = part.split('.').next().unwrap().to_ascii_uppercase();
        if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(error("path", "embedded resource uses a reserved filename"));
        }
    }
    Ok(())
}

fn hash_reader(reader: &mut impl Read) -> Result<[u8; 32], String> {
    let mut digest = Sha256::new();
    let mut buffer = [0; 65_536];
    loop {
        let length = reader.read(&mut buffer).map_err(io_error)?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
    }
    Ok(digest.finalize().into())
}

fn error(code: &str, message: &str) -> String {
    format!("error[application.{code}]: {message}")
}

fn io_error(cause: std::io::Error) -> String {
    error("io", &cause.to_string())
}

#[cfg(test)]
mod executable_test;

#[cfg(any(
    target_os = "linux",
    test,
    not(feature = "serve-runtime-bin"),
    feature = "native-codegen"
))]
fn finish_payload(
    output: &mut impl std::io::Write,
    manifest: &Manifest,
    mut payload_len: u64,
    mut digest: Sha256,
) -> Result<(), String> {
    validate_manifest(manifest, payload_len)?;
    let encoded =
        serde_json::to_vec(manifest).map_err(|cause| error("manifest", &cause.to_string()))?;
    if encoded.len() as u64 > MAX_MANIFEST_LEN {
        return Err(error("bounds", "embedded manifest is too large"));
    }
    output.write_all(&encoded).map_err(io_error)?;
    digest.update(&encoded);
    payload_len = payload_len
        .checked_add(encoded.len() as u64)
        .ok_or_else(|| error("bounds", "application size overflow"))?;
    output.write_all(MAGIC).map_err(io_error)?;
    output
        .write_all(&payload_len.to_le_bytes())
        .map_err(io_error)?;
    output
        .write_all(&(encoded.len() as u64).to_le_bytes())
        .map_err(io_error)?;
    output.write_all(&digest.finalize()).map_err(io_error)?;
    Ok(())
}
