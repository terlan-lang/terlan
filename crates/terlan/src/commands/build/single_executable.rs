//! Publishes one directly executable application with embedded native resources.

use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::runtime::executable::{append_application, ApplicationEntry, ArtifactEnvironment};
use terlan_process_owner::ProcessControl;

mod legacy;
pub(super) use legacy::LegacyOutputs;

/// Builds privately, then replaces the executable only after payload emission.
pub(super) fn write(
    destination: &Path,
    output_root: &Path,
    entry: ApplicationEntry,
    native_artifacts: &[(String, PathBuf)],
    incremental: bool,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or("application executable has no parent")?;
    fs::create_dir_all(parent).map_err(|cause| cause.to_string())?;
    let temporary = RuntimeCopy::new(parent)?;
    super::copy_bundled_terlan_vm_runner(&temporary.0)?;
    verify_native_runtime(&temporary.0)?;
    make_runtime_copy_writable(&temporary.0)?;
    let version = ProcessControl::new(Duration::from_secs(5))
        .capture_stdout(
            Command::new(&temporary.0).arg("--executable-format-version"),
            1024,
            |_| Ok(()),
        )
        .map_err(|cause| {
            format!("application runtime must support single executables: {cause:?}")
        })?;
    if version != b"1\n" && version != b"1\r\n" {
        return Err("application runtime must support executable format 1; install the compiler-paired terlan-vm".into());
    }
    let mut resources = Vec::new();
    for directory in ["vm", "web", "native"] {
        let path = output_root.join(directory);
        if path.is_dir() {
            collect_resources(output_root, &path, &mut resources)?;
        }
    }
    let mut environment = Vec::new();
    for (name, source) in native_artifacts {
        let filename = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("native artifact has no UTF-8 filename")?;
        let path = format!("native/artifacts/{name}/{filename}");
        resources.push((path.clone(), source.clone()));
        environment.push(ArtifactEnvironment {
            name: name.clone(),
            path,
        });
    }
    resources.sort_by(|left, right| left.0.cmp(&right.0));
    environment.sort_by(|left, right| left.name.cmp(&right.name));
    append_application(&temporary.0, entry, &resources, environment)?;
    if incremental && same_bytes(&temporary.0, destination)? {
        return Ok(());
    }
    fs::rename(&temporary.0, destination).map_err(|cause| {
        format!(
            "cannot publish application {}: {cause}",
            destination.display()
        )
    })
}

fn verify_native_runtime(path: &Path) -> Result<(), String> {
    let input = File::open(path).map_err(|cause| cause.to_string())?;
    let cache = object::ReadCache::new(input);
    let kind = object::FileKind::parse(&cache).map_err(|_| {
        "application runtime must be a native executable, not a script wrapper".to_string()
    })?;
    if !matches!(
        kind,
        object::FileKind::Elf32
            | object::FileKind::Elf64
            | object::FileKind::Pe32
            | object::FileKind::Pe64
            | object::FileKind::MachO32
            | object::FileKind::MachO64
            | object::FileKind::MachOFat32
            | object::FileKind::MachOFat64
    ) {
        return Err("application runtime must be an ELF, PE, or Mach-O executable".into());
    }
    Ok(())
}

fn make_runtime_copy_writable(path: &Path) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(|cause| cause.to_string())?
        .permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode((permissions.mode() & 0o777) | 0o200);
    }
    #[cfg(not(unix))]
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).map_err(|cause| cause.to_string())
}

#[cfg(test)]
#[path = "single_executable_test.rs"]
mod tests;

fn collect_resources(
    root: &Path,
    directory: &Path,
    resources: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|cause| cause.to_string())? {
        let entry = entry.map_err(|cause| cause.to_string())?;
        let kind = entry.file_type().map_err(|cause| cause.to_string())?;
        let path = entry.path();
        if kind.is_dir() {
            collect_resources(root, &path, resources)?;
        } else if kind.is_file() {
            let relative = path.strip_prefix(root).map_err(|cause| cause.to_string())?;
            resources.push((super::path_to_manifest_string(relative), path));
        } else {
            return Err(format!(
                "application resource is not a regular file: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

struct RuntimeCopy(PathBuf);

impl RuntimeCopy {
    fn new(directory: &Path) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|cause| cause.to_string())?
            .as_nanos();
        let name = format!(
            ".terlan-application-{}-{nonce}{}",
            std::process::id(),
            if cfg!(windows) { ".exe" } else { "" }
        );
        let path = directory.join(name);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|cause| cause.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for RuntimeCopy {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn same_bytes(left: &Path, right: &Path) -> Result<bool, String> {
    let mut right = match File::open(right) {
        Ok(file) => file,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(cause) => return Err(cause.to_string()),
    };
    let mut left = File::open(left).map_err(|cause| cause.to_string())?;
    if left.metadata().map_err(|cause| cause.to_string())?.len()
        != right.metadata().map_err(|cause| cause.to_string())?.len()
    {
        return Ok(false);
    }
    let mut a = [0; 65_536];
    let mut b = [0; 65_536];
    loop {
        let length = left.read(&mut a).map_err(|cause| cause.to_string())?;
        if length == 0 {
            return Ok(true);
        }
        right
            .read_exact(&mut b[..length])
            .map_err(|cause| cause.to_string())?;
        if a[..length] != b[..length] {
            return Ok(false);
        }
    }
}
