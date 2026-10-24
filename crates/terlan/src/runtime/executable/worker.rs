//! Lazily snapshots the running native code without depending on its deployed path.

use std::fs::{self, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::sync::OnceLock;

use super::*;

struct WorkerRuntime {
    file: File,
    prefix_len: u64,
    root: PathBuf,
    snapshot: OnceLock<Result<PathBuf, String>>,
}

static RUNTIME: OnceLock<WorkerRuntime> = OnceLock::new();

pub(super) fn pin(application: &EmbeddedApplication, root: &Path) -> Result<(), String> {
    RUNTIME
        .set(WorkerRuntime {
            file: application.file.try_clone().map_err(io_error)?,
            prefix_len: application.payload_start,
            root: root.to_path_buf(),
            snapshot: OnceLock::new(),
        })
        .map_err(|_| error("worker", "application runtime is already initialized"))
}

pub(super) fn path() -> Result<Option<PathBuf>, String> {
    RUNTIME
        .get()
        .map(|runtime| runtime.snapshot.get_or_init(|| snapshot(runtime)).clone())
        .transpose()
}

fn snapshot(runtime: &WorkerRuntime) -> Result<PathBuf, String> {
    let directory = runtime.root.join(".runtime");
    fs::create_dir(&directory).map_err(io_error)?;
    let path = directory.join("worker");
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(&path)
        .map_err(io_error)?;
    let mut input = runtime.file.try_clone().map_err(io_error)?;
    input.seek(SeekFrom::Start(0)).map_err(io_error)?;
    let copied =
        std::io::copy(&mut input.take(runtime.prefix_len), &mut output).map_err(io_error)?;
    if copied != runtime.prefix_len {
        return Err(error("worker", "running native runtime was truncated"));
    }
    finish_payload(
        &mut output,
        &Manifest {
            entry: ApplicationEntry::Worker,
            files: Vec::new(),
            environment: Vec::new(),
        },
        0,
        Sha256::new(),
    )?;
    output.sync_all().map_err(io_error)?;
    drop(output);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o500)).map_err(io_error)?;
    Ok(path)
}
