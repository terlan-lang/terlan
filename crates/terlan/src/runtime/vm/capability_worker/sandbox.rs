//! Fail-closed platform sandbox planning for external capability workers.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicU64, Ordering};

use crate::terlan_native_boundary::capability_sandbox::CapabilitySandboxProfile;
#[cfg(target_os = "linux")]
use crate::terlan_native_boundary::capability_sandbox::{
    CapabilitySandboxLimits, SANDBOX_LOCALE, SANDBOX_TEMP_DIR, SANDBOX_WORK_DIR,
};

#[cfg(target_os = "linux")]
const BUBBLEWRAP_PATH: &str = "/usr/bin/bwrap";
#[cfg(target_os = "linux")]
const PRLIMIT_PATH: &str = "/usr/bin/prlimit";
#[cfg(target_os = "linux")]
const BASH_PATH: &str = "/bin/bash";
#[cfg(target_os = "linux")]
const SANDBOX_WORKER_PATH: &str = "/run/terlan/worker";
#[cfg(target_os = "linux")]
const CLOSE_INHERITED_DESCRIPTORS: &str = concat!(
    "for fd_path in /proc/self/fd/*; do ",
    "fd=${fd_path##*/}; ",
    "case \"$fd\" in 0|1|2) ;; *[!0-9]*) exit 125 ;; *) eval \"exec ${fd}>&-\" ;; esac; ",
    "done; exec \"$@\"",
);
#[cfg(target_os = "linux")]
const MAX_SANDBOX_DIR_ATTEMPTS: u64 = 64;
#[cfg(target_os = "linux")]
static NEXT_SANDBOX_DIR: AtomicU64 = AtomicU64::new(1);

/// Private host directory mounted as the worker's only writable persistent path.
pub(super) struct VmCapabilityWorkerSandboxDir {
    path: PathBuf,
}

impl VmCapabilityWorkerSandboxDir {
    /// Allocates a private empty directory for one worker process.
    pub(super) fn create() -> Result<Self, String> {
        #[cfg(not(target_os = "linux"))]
        {
            return Err(
                "error[capability_worker.sandbox]: no supported sandbox backend for this platform"
                    .to_string(),
            );
        }
        #[cfg(target_os = "linux")]
        {
            let root = std::env::temp_dir();
            for _ in 0..MAX_SANDBOX_DIR_ATTEMPTS {
                let sequence = NEXT_SANDBOX_DIR.fetch_add(1, Ordering::Relaxed);
                let path = root.join(format!(
                    "terlan-capability-worker-{}-{sequence}",
                    std::process::id()
                ));
                let mut builder = std::fs::DirBuilder::new();
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
                match builder.create(&path) {
                    Ok(()) => return Ok(Self { path }),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => {
                        return Err(format!(
                            "error[capability_worker.sandbox]: cannot create private work directory: {error}"
                        ));
                    }
                }
            }
            Err(
                "error[capability_worker.sandbox]: private work directory allocation exhausted"
                    .to_string(),
            )
        }
    }

    /// Returns the private host path mounted into the worker sandbox.
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for VmCapabilityWorkerSandboxDir {
    /// Removes the private worker directory after process and I/O teardown.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Constructs the command for one already-admitted host sandbox profile.
pub(super) fn worker_command(
    profile: CapabilitySandboxProfile,
    executable: &Path,
    capabilities: &[String],
    work_dir: &Path,
) -> Result<Command, String> {
    match profile {
        CapabilitySandboxProfile::LinuxBwrapV1 => {
            linux_worker_command(executable, capabilities, work_dir)
        }
    }
}

/// Keeps untrusted worker diagnostics out of the production process while
/// exposing sandbox-launch failures to the real-process integration tests.
pub(super) fn worker_stderr() -> Stdio {
    #[cfg(test)]
    {
        Stdio::inherit()
    }
    #[cfg(not(test))]
    {
        Stdio::null()
    }
}

/// Constructs the fixed Linux `bubblewrap -> prlimit -> worker` command.
pub(super) fn linux_worker_command(
    executable: &Path,
    capabilities: &[String],
    work_dir: &Path,
) -> Result<Command, String> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (executable, capabilities, work_dir);
        return Err(
            "error[capability_worker.sandbox]: no supported sandbox backend for this platform"
                .to_string(),
        );
    }
    #[cfg(target_os = "linux")]
    {
        require_regular_file(Path::new(BASH_PATH), "Bash descriptor sanitizer")?;
        require_regular_file(Path::new(BUBBLEWRAP_PATH), "bubblewrap")?;
        require_regular_file(Path::new(PRLIMIT_PATH), "prlimit")?;
        require_regular_file(executable, "capability worker")?;
        if !work_dir.is_absolute() || !work_dir.is_dir() {
            return Err(
                "error[capability_worker.sandbox]: work directory must be an existing absolute directory"
                    .to_string(),
            );
        }
        let executable = executable.canonicalize().map_err(|error| {
            format!(
                "error[capability_worker.sandbox]: cannot canonicalize worker executable: {error}"
            )
        })?;
        let limits = CapabilitySandboxLimits::linux_default();
        let mut command = Command::new(BASH_PATH);
        command
            .arg("-c")
            .arg(CLOSE_INHERITED_DESCRIPTORS)
            .arg("terlan-capability-worker-fd-sanitizer")
            .arg(BUBBLEWRAP_PATH);
        append_namespace_policy(&mut command, capabilities);
        append_filesystem_policy(&mut command, &executable, work_dir);
        command.arg("--").arg(PRLIMIT_PATH);
        append_resource_limits(&mut command, limits);
        command.arg("--").arg(SANDBOX_WORKER_PATH);
        Ok(command)
    }
}

/// Appends fixed hard resource bounds understood by `prlimit`.
#[cfg(target_os = "linux")]
fn append_resource_limits(command: &mut Command, limits: CapabilitySandboxLimits) {
    command
        .arg(limit_arg("as", limits.address_space_bytes))
        .arg(limit_arg("cpu", limits.cpu_seconds))
        .arg(limit_arg("fsize", limits.file_bytes))
        .arg(limit_arg("nofile", limits.open_files))
        .arg(limit_arg("nproc", limits.processes))
        .arg("--core=0:0");
}

/// Appends namespace, capability, session, and environment isolation.
#[cfg(target_os = "linux")]
fn append_namespace_policy(command: &mut Command, capabilities: &[String]) {
    command
        .arg("--unshare-user-try")
        .arg("--unshare-pid")
        .arg("--unshare-ipc")
        .arg("--unshare-uts")
        .arg("--unshare-cgroup-try")
        .arg("--new-session")
        .arg("--die-with-parent")
        .arg("--clearenv")
        .arg("--setenv")
        .arg("HOME")
        .arg(SANDBOX_WORK_DIR)
        .arg("--setenv")
        .arg("TMPDIR")
        .arg(SANDBOX_TEMP_DIR)
        .arg("--setenv")
        .arg("LANG")
        .arg(SANDBOX_LOCALE)
        .arg("--hostname")
        .arg("terlan-worker")
        .arg("--cap-drop")
        .arg("ALL");
    if !capabilities
        .iter()
        .any(|capability| capability == "postgres")
    {
        command.arg("--unshare-net");
    }
}

/// Appends a read-only system view and one private writable directory.
#[cfg(target_os = "linux")]
fn append_filesystem_policy(command: &mut Command, executable: &Path, work_dir: &Path) {
    command
        .arg("--ro-bind")
        .arg("/usr")
        .arg("/usr")
        .arg("--ro-bind")
        .arg("/lib")
        .arg("/lib")
        .arg("--ro-bind-try")
        .arg("/lib64")
        .arg("/lib64")
        .arg("--proc")
        .arg("/proc")
        .arg("--dev")
        .arg("/dev")
        .arg("--tmpfs")
        .arg(SANDBOX_TEMP_DIR)
        .arg("--dir")
        .arg("/etc")
        .arg("--dir")
        .arg("/etc/ssl")
        .arg("--ro-bind-try")
        .arg("/etc/resolv.conf")
        .arg("/etc/resolv.conf")
        .arg("--ro-bind-try")
        .arg("/etc/hosts")
        .arg("/etc/hosts")
        .arg("--ro-bind-try")
        .arg("/etc/nsswitch.conf")
        .arg("/etc/nsswitch.conf")
        .arg("--ro-bind-try")
        .arg("/etc/passwd")
        .arg("/etc/passwd")
        .arg("--ro-bind-try")
        .arg("/etc/group")
        .arg("/etc/group")
        .arg("--ro-bind-try")
        .arg("/etc/ssl/certs")
        .arg("/etc/ssl/certs")
        .arg("--dir")
        .arg("/run")
        .arg("--dir")
        .arg("/run/terlan")
        .arg("--ro-bind")
        .arg(executable)
        .arg(SANDBOX_WORKER_PATH)
        .arg("--bind")
        .arg(work_dir)
        .arg(SANDBOX_WORK_DIR)
        .arg("--chdir")
        .arg(SANDBOX_WORK_DIR);
}

/// Formats one equal soft/hard `prlimit` argument.
#[cfg(target_os = "linux")]
fn limit_arg(name: &str, value: u64) -> String {
    format!("--{name}={value}:{value}")
}

/// Rejects absent tooling and worker paths before process creation.
#[cfg(target_os = "linux")]
fn require_regular_file(path: &Path, label: &str) -> Result<(), String> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!(
            "error[capability_worker.sandbox]: required {label} `{}` is unavailable",
            path.display()
        ))
    }
}

#[cfg(all(test, target_os = "linux"))]
#[cfg(test)]
#[path = "sandbox_test.rs"]
#[cfg(test)]
mod sandbox_test;
