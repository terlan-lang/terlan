//! Removes only bundle members recorded by an earlier build of this application.

use super::*;
use serde_json::Value;

#[derive(Default)]
pub(in crate::commands::build) struct LegacyOutputs {
    files: Vec<PathBuf>,
    web: bool,
}

impl LegacyOutputs {
    pub(in crate::commands::build) fn capture(root: &Path, package: &str, image: &Path) -> Self {
        let metadata = root.join(crate::commands::build::BUILD_PACKAGE_METADATA_FILE);
        let Some(metadata) = fs::read(metadata)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        else {
            return Self::default();
        };
        if metadata["schema"] != crate::commands::build::BUILD_PACKAGE_METADATA_SCHEMA
            || metadata["target"] != "terlan-vm"
            || metadata["artifact"] != "terlan-vm"
            || metadata["package"]["name"] != package
            || metadata["executable"]["embedded"] == true
        {
            return Self::default();
        }
        let legacy_name = if cfg!(windows) {
            format!("{package}.cmd")
        } else {
            package.to_string()
        };
        if Path::new(&legacy_name).components().count() != 1
            || matches!(legacy_name.as_str(), "." | "..")
        {
            return Self::default();
        }
        let old_executable = format!("bin/{legacy_name}");
        if metadata["executable"]["path"] != old_executable {
            return Self::default();
        }
        let runtime = format!("bin/terlan-vm{}", std::env::consts::EXE_SUFFIX);
        let worker = format!("bin/terlan-native-worker{}", std::env::consts::EXE_SUFFIX);
        let service = format!("bin/terlan-serve-runtime{}", std::env::consts::EXE_SUFFIX);
        let known = [&old_executable, &runtime, &worker, &service];
        let bin = root.join("bin");
        if fs::symlink_metadata(&bin).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Self::default();
        }
        let Ok(entries) = fs::read_dir(&bin) else {
            return Self::default();
        };
        for entry in entries {
            let Ok(entry) = entry else {
                return Self::default();
            };
            if !known.iter().any(|path| root.join(path) == entry.path()) {
                // Other programs may still depend on shared legacy runtimes.
                return Self::default();
            }
        }
        let mut files = vec![PathBuf::from(old_executable)];
        for (field, expected) in [
            ("runtime", runtime),
            ("native_worker", worker),
            ("service_runtime", service),
        ] {
            if metadata["executable"][field] == expected {
                files.push(PathBuf::from(expected));
            }
        }
        if metadata["executable"]["image"] == crate::commands::build::path_to_manifest_string(image)
            && !fs::symlink_metadata(root.join("vm"))
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            files.push(image.to_path_buf());
        }
        Self {
            files,
            web: metadata["executable"]["web_root"] == "web",
        }
    }

    pub(in crate::commands::build) fn remove(
        self,
        root: &Path,
        executable: &Path,
    ) -> Result<(), String> {
        if self.files.is_empty() && !self.web {
            return Ok(());
        }
        for relative in self.files {
            if relative == executable {
                continue;
            }
            let path = root.join(relative);
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => {
                    #[cfg(windows)]
                    if metadata.is_file() && metadata.permissions().readonly() {
                        let mut permissions = metadata.permissions();
                        permissions.set_readonly(false);
                        fs::set_permissions(&path, permissions)
                            .map_err(|cause| cause.to_string())?;
                    }
                    fs::remove_file(path).map_err(|cause| cause.to_string())?;
                }
                Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {}
                Ok(_) => {}
                Err(cause) => return Err(cause.to_string()),
            }
        }
        if self.web
            && root.join("web").is_dir()
            && !fs::symlink_metadata(root.join("web"))
                .map_err(|cause| cause.to_string())?
                .file_type()
                .is_symlink()
        {
            fs::remove_dir_all(root.join("web")).map_err(|cause| cause.to_string())?;
        }
        let vm = root.join("vm");
        if !fs::symlink_metadata(&vm).is_ok_and(|metadata| metadata.file_type().is_symlink())
            && fs::read_dir(&vm).is_ok_and(|mut entries| entries.next().is_none())
        {
            fs::remove_dir(vm).map_err(|cause| cause.to_string())?;
        }
        Ok(())
    }
}
