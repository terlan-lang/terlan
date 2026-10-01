use super::UpdateError;

use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolves the actual executable so updates follow symlinks to the installed bin.
pub(super) fn run(tag: &str) -> Result<(), UpdateError> {
    let executable = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|error| format!("could not locate the installed compiler: {error}"))?;
    let (bin, share) = destinations(&executable)?;
    println!("Installing Terlan {tag} in {}", bin.display());
    launch(tag, &bin, &share)
}

/// Preserves either supported installed share layout, using the prefix by default.
pub(super) fn destinations(executable: &Path) -> Result<(PathBuf, PathBuf), UpdateError> {
    let bin = executable
        .parent()
        .ok_or("compiler has no installation directory")?;
    let prefix = bin
        .parent()
        .ok_or("compiler installation has no parent directory")?;
    let share = if bin.join("share/terlan").is_dir() {
        bin.join("share/terlan")
    } else {
        prefix.join("share/terlan")
    };
    Ok((bin.to_path_buf(), share))
}

/// Pins installer inputs to the selected official release and current installation.
fn configure(command: &mut Command, tag: &str, bin: &Path, share: &Path) {
    command
        .env("TERLAN_VERSION", tag)
        .env("TERLAN_INSTALL_DIR", bin)
        .env("TERLAN_INSTALL_SHARE_DIR", share)
        .env(
            "TERLAN_RELEASE_BASE_URL",
            "https://github.com/terlan-lang/terlan/releases/download",
        )
        .env_remove("TERLAN_INSTALL_DRY_RUN")
        .env_remove("TERLAN_INSTALL_OS")
        .env_remove("TERLAN_INSTALL_ARCH");
}

/// Replaces this process with the bundled installer, releasing the executable first.
#[cfg(unix)]
fn launch(tag: &str, bin: &Path, share: &Path) -> Result<(), UpdateError> {
    use std::os::unix::process::CommandExt;
    let mut command = unix_command(tag, bin, share);
    // exec preserves the installer's exit status and terminal (including sudo).
    // The old terlc image is no longer running when the installer replaces it.
    Err(format!("could not start the installer: {}", command.exec()).into())
}

/// Builds a shell invocation without interpolating paths or versions into code.
#[cfg(unix)]
pub(super) fn unix_command(tag: &str, bin: &Path, share: &Path) -> Command {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(include_str!("../../../../../install.sh"))
        .arg("terlc-self-update");
    configure(&mut command, tag, bin, share);
    command.env(
        "TERLAN_INSTALL_OS",
        if cfg!(target_os = "macos") {
            "Darwin"
        } else {
            "Linux"
        },
    );
    command.env("TERLAN_INSTALL_ARCH", std::env::consts::ARCH);
    command
}

/// Starts a logged Windows installer that waits for this process to release terlc.exe.
#[cfg(windows)]
fn launch(tag: &str, bin: &Path, share: &Path) -> Result<(), UpdateError> {
    use std::fs::OpenOptions;
    use std::process::Stdio;
    let log_path = std::env::temp_dir().join(format!(
        "terlc-self-update-{}-{}.log",
        std::process::id(),
        rand::random::<u64>()
    ));
    let log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&log_path)
        .map_err(|error| format!("could not create update log: {error}"))?;
    let error_log = log.try_clone().map_err(|error| error.to_string())?;
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        r#"
        $ErrorActionPreference = 'Stop'
        Wait-Process -Id ([int]$env:TERLAN_SELF_UPDATE_PARENT) -ErrorAction SilentlyContinue
        try {
            & ([ScriptBlock]::Create($env:TERLAN_SELF_UPDATE_SCRIPT))
            Write-Output 'Terlan self-update completed.'
        } catch {
            Write-Error $_
            exit 1
        }
    "#,
    ]);
    configure(&mut command, tag, bin, share);
    command
        .env("TERLAN_INSTALL_ARCH", std::env::consts::ARCH)
        .env("TERLAN_SELF_UPDATE_PARENT", std::process::id().to_string())
        .env(
            "TERLAN_SELF_UPDATE_SCRIPT",
            include_str!("../../../../../install.ps1"),
        )
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(error_log);
    command
        .spawn()
        .map_err(|error| format!("could not start PowerShell installer: {error}"))?;
    println!(
        "Update starts after terlc exits. Check {} for completion or errors.",
        log_path.display()
    );
    Ok(())
}

/// Reports unsupported platforms without attempting to alter the installation.
#[cfg(not(any(unix, windows)))]
fn launch(_tag: &str, _bin: &Path, _share: &Path) -> Result<(), UpdateError> {
    Err("self-update is supported on Linux, macOS, and Windows".into())
}
