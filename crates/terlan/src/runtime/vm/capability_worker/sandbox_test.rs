use std::os::unix::fs::PermissionsExt;

use super::*;

/// Builds a deterministic namespace, filesystem, and resource-limit command.
#[test]
fn linux_sandbox_command_is_closed_and_bounded() {
    let work_dir = VmCapabilityWorkerSandboxDir::create().expect("private sandbox directory");
    let executable = std::env::current_exe().expect("test executable path");
    let command =
        linux_worker_command(&executable, &[], work_dir.path()).expect("Linux sandbox command");
    let program = command.get_program().to_string_lossy();
    let arguments = command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert_eq!(program, POSIX_SHELL_PATH);
    assert!(arguments.contains(&CLOSE_INHERITED_DESCRIPTORS.to_string()));
    assert!(arguments.contains(&BUBBLEWRAP_PATH.to_string()));
    assert!(arguments.contains(&"--as=536870912:536870912".to_string()));
    assert!(arguments.contains(&"--cpu=60:60".to_string()));
    assert!(arguments.contains(&"--fsize=16777216:16777216".to_string()));
    assert!(arguments.contains(&"--nofile=64:64".to_string()));
    assert!(arguments.contains(&"--nproc=512:512".to_string()));
    assert!(arguments.contains(&PRLIMIT_PATH.to_string()));
    assert!(arguments.contains(&"--unshare-net".to_string()));
    assert!(arguments.contains(&"--clearenv".to_string()));
    assert!(arguments.contains(&"--cap-drop".to_string()));
    assert!(arguments.contains(&SANDBOX_WORKER_PATH.to_string()));
    assert_eq!(
        std::fs::metadata(work_dir.path())
            .expect("sandbox directory metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

/// Preserves host networking only for an explicitly networked capability.
#[test]
fn linux_sandbox_network_authority_follows_capability_allowlist() {
    let work_dir = VmCapabilityWorkerSandboxDir::create().expect("private sandbox directory");
    let executable = std::env::current_exe().expect("test executable path");
    let command = linux_worker_command(&executable, &["postgres".to_string()], work_dir.path())
        .expect("Postgres sandbox command");
    let arguments = command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(!arguments.contains(&"--unshare-net".to_string()));
}

/// Rejects absent worker executables before any wrapper process is started.
#[test]
fn linux_sandbox_rejects_missing_worker() {
    let work_dir = VmCapabilityWorkerSandboxDir::create().expect("private sandbox directory");
    let error = linux_worker_command(
        Path::new("/definitely/missing/terlan-native-worker"),
        &[],
        work_dir.path(),
    )
    .expect_err("missing worker");

    assert!(error.contains("required capability worker"));
}

/// Removes the private writable directory when sandbox ownership ends.
#[test]
fn linux_sandbox_directory_is_lifecycle_owned() {
    let work_dir = VmCapabilityWorkerSandboxDir::create().expect("private sandbox directory");
    let path = work_dir.path().to_path_buf();
    assert!(path.is_dir());

    drop(work_dir);

    assert!(!path.exists());
}
