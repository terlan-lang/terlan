use std::fs;
use std::path::Path;

use crate::runtime::native_image::{host_tvm_target, inspect_tvm_image};

/// Returns the platform VM runtime executable filename.
pub(super) fn terlan_vm_runner_name() -> &'static str {
    if cfg!(windows) {
        "terlan-vm.exe"
    } else {
        "terlan-vm"
    }
}

/// Checks whether a native VM image exports the package's public `main/0`.
pub(super) fn vm_image_has_main_entrypoint(
    image_path: &Path,
    entry_module: &str,
) -> Result<bool, String> {
    let image = fs::read(image_path).map_err(|err| {
        format!(
            "failed to read native VM image `{}`: {err}",
            image_path.display()
        )
    })?;
    let target = host_tvm_target()?;
    let inspection = inspect_tvm_image(&image, &target.triple).map_err(|err| {
        format!(
            "failed to admit native VM image `{}` for executable entrypoint: {err}",
            image_path.display()
        )
    })?;
    let entry = format!("{entry_module}.main/0");
    Ok(inspection
        .descriptor
        .exports
        .iter()
        .any(|export| export.name == entry && export.parameters.is_empty()))
}
