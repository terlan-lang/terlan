use std::fs;
use std::path::Path;

use super::super::BuildOneError;

/// Keeps deployable VM output to one native image for the current application.
pub(super) fn remove_stale_tvm_images(
    vm_dir: &Path,
    retained_name: Option<&str>,
) -> Result<(), BuildOneError> {
    let entries = fs::read_dir(vm_dir).map_err(|error| {
        BuildOneError::Message(format!(
            "failed to inspect VM artifact directory `{}`: {error}",
            vm_dir.display()
        ))
    })?;
    for entry in entries {
        let path = entry
            .map_err(|error| {
                BuildOneError::Message(format!(
                    "failed to inspect VM artifact directory `{}`: {error}",
                    vm_dir.display()
                ))
            })?
            .path();
        let file_name = path.file_name().and_then(|value| value.to_str());
        let is_tvm_image = path.extension().and_then(|value| value.to_str()) == Some("tvm");
        let is_legacy_sidecar = file_name
            .is_some_and(|name| name.ends_with(".tvm.json") || name.ends_with(".tvm.reuse"));
        let is_retained = retained_name.is_some_and(|name| file_name == Some(name));
        if (is_tvm_image && !is_retained) || is_legacy_sidecar {
            fs::remove_file(&path).map_err(|error| {
                BuildOneError::Message(format!(
                    "failed to remove stale TVM artifact `{}`: {error}",
                    path.display()
                ))
            })?;
        }
    }
    Ok(())
}
