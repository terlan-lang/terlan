//! Exclusively owned proof inputs and generated Lake state, outside source trees.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::terlan_quality::QualityResult;

const MAX_FILES: usize = 4096;
const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
static NEXT_WORKSPACE: AtomicU64 = AtomicU64::new(0);

/// Owns only an exclusively created replay directory, never a shared Lake cache.
pub(super) struct ProofWorkspace {
    path: Option<PathBuf>,
    #[cfg(target_os = "linux")]
    inputs: String,
}

impl ProofWorkspace {
    /// Snapshots the local Lean project and explicitly declared extra inputs.
    pub(super) fn create(root: &Path, extra: &[String]) -> QualityResult<Self> {
        let inputs = collect_inputs(root, extra)?;
        #[cfg(target_os = "linux")]
        let identity = input_identity(&inputs);
        let parent = workspace_parent(root)?;
        let mut workspace = None;
        for _ in 0..64 {
            let id = NEXT_WORKSPACE.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!("replay-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => {
                    workspace = Some(Self {
                        path: Some(path),
                        #[cfg(target_os = "linux")]
                        inputs: identity.clone(),
                    });
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("cannot reserve proof workspace: {error}")),
            }
        }
        let workspace = workspace.ok_or("proof workspace reservation exhausted")?;
        for (relative, bytes) in inputs {
            let destination = workspace.root().join(relative);
            fs::create_dir_all(destination.parent().ok_or("proof input has no parent")?)
                .map_err(|error| format!("cannot create private proof input directory: {error}"))?;
            fs::write(&destination, bytes)
                .map_err(|error| format!("cannot write private proof input: {error}"))?;
        }
        Ok(workspace)
    }

    /// Returns the immutable input snapshot's root for output normalization.
    pub(super) fn root(&self) -> &Path {
        self.path.as_deref().expect("live proof workspace")
    }

    /// Fingerprints the exact copied sources and declared dependencies.
    #[cfg(target_os = "linux")]
    pub(super) fn input_identity(&self) -> &str {
        &self.inputs
    }

    /// Cleans the private workspace after the process owner has reaped its child.
    pub(super) fn close(mut self) -> QualityResult<()> {
        fs::remove_dir_all(self.root())
            .map_err(|error| format!("cannot remove owned proof workspace: {error}"))?;
        self.path = None;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn input_identity(inputs: &BTreeMap<PathBuf, Vec<u8>>) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    for (path, bytes) in inputs {
        let path = path.as_os_str().as_encoded_bytes();
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path);
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    super::format_digest(&digest.finalize())
}

impl Drop for ProofWorkspace {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            if let Err(error) = fs::remove_dir_all(path) {
                eprintln!(
                    "cannot clean owned proof workspace {}: {error}",
                    path.display()
                );
            }
        }
    }
}

fn workspace_parent(root: &Path) -> QualityResult<PathBuf> {
    let mut path = root.to_path_buf();
    for component in ["target", "quality", "proof-artifacts", "workspaces"] {
        path.push(component);
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("cannot create proof workspace parent: {error}")),
        }
        require_directory(&path)?;
    }
    fs::canonicalize(&path)
        .map_err(|error| format!("cannot resolve proof workspace parent: {error}"))
}

fn require_directory(path: &Path) -> QualityResult<()> {
    if !fs::symlink_metadata(path)
        .map_err(|error| error.to_string())?
        .is_dir()
    {
        return Err(format!(
            "proof directory must not be a symlink: {}",
            path.display()
        ));
    }
    Ok(())
}

/// Only ordinary repository-relative components can enter a proof snapshot.
pub(super) fn relative_input(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn collect_inputs(root: &Path, extra: &[String]) -> QualityResult<BTreeMap<PathBuf, Vec<u8>>> {
    let mut paths = Vec::new();
    lean_paths(root, Path::new("proofs/lean"), &mut paths, &mut 0)?;
    paths.extend(
        [
            "proofs/lean/lakefile.lean",
            "proofs/lean/lake-manifest.json",
            "proofs/lean/lean-toolchain",
        ]
        .map(PathBuf::from),
    );
    for relative in extra {
        if !relative_input(relative) {
            return Err(format!("unsafe proof dependency `{relative}`"));
        }
        paths.push(PathBuf::from(relative));
    }
    paths.sort();
    paths.dedup();
    if paths.len() > MAX_FILES {
        return Err("proof input file budget exceeded".into());
    }
    let mut bytes = 0_u64;
    let mut inputs = BTreeMap::new();
    for relative in paths {
        let mut ancestor = root.to_path_buf();
        for component in relative
            .parent()
            .ok_or("proof input missing parent")?
            .components()
        {
            ancestor.push(component);
            require_directory(&ancestor)?;
        }
        let path = root.join(&relative);
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!(
                "proof input is not a regular file: {}",
                path.display()
            ));
        }
        bytes = bytes
            .checked_add(metadata.len())
            .ok_or("proof input size overflow")?;
        if bytes > MAX_INPUT_BYTES {
            return Err("proof input byte budget exceeded".into());
        }
        let mut contents = Vec::new();
        fs::File::open(&path)
            .map_err(|error| error.to_string())?
            .take(metadata.len() + 1)
            .read_to_end(&mut contents)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if contents.len() as u64 != metadata.len() {
            return Err("proof input changed during snapshot".into());
        }
        inputs.insert(relative, contents);
    }
    Ok(inputs)
}

fn lean_paths(
    root: &Path,
    relative: &Path,
    paths: &mut Vec<PathBuf>,
    visited: &mut usize,
) -> QualityResult<()> {
    if relative.components().count() > 64 {
        return Err("proof directory depth budget exceeded".into());
    }
    require_directory(&root.join(relative))?;
    for entry in fs::read_dir(root.join(relative)).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        *visited += 1;
        if *visited > 32768 {
            return Err("proof source scan budget exceeded".into());
        }
        let name = entry.file_name();
        if name == ".lake" || name == "artifacts" {
            continue;
        }
        let metadata = entry.file_type().map_err(|error| error.to_string())?;
        let child = relative.join(name);
        if metadata.is_symlink() {
            return Err(format!("symlink in proof sources: {}", child.display()));
        }
        if metadata.is_dir() {
            lean_paths(root, &child, paths, visited)?;
        } else if child
            .extension()
            .is_some_and(|extension| extension == "lean")
        {
            paths.push(child);
            if paths.len() > MAX_FILES {
                return Err("proof input file budget exceeded".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "lean_proof_workspace_test.rs"]
mod tests;
