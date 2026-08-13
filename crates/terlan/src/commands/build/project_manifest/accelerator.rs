use std::path::{Component, Path};

use super::model::ProjectAccelerator;

/// Incremental parser state for optional generic accelerator metadata.
#[derive(Debug, Default)]
pub(super) struct ProjectAcceleratorBuilder {
    pub(super) schema: Option<u64>,
    pub(super) descriptor: Option<String>,
}

impl ProjectAcceleratorBuilder {
    /// Finalizes the accelerator descriptor reference declared by a package.
    pub(super) fn finish(self, manifest_path: &Path) -> Result<Option<ProjectAccelerator>, String> {
        if self.schema.is_none() && self.descriptor.is_none() {
            return Ok(None);
        }
        let schema = self.schema.ok_or_else(|| {
            format!(
                "{}: [accelerator] requires `schema` when the section is present",
                manifest_path.display()
            )
        })?;
        if schema != 1 {
            return Err(format!(
                "{}: unsupported [accelerator] schema `{schema}`; supported schemas: 1",
                manifest_path.display()
            ));
        }
        let descriptor = self.descriptor.ok_or_else(|| {
            format!(
                "{}: [accelerator] requires `descriptor` when the section is present",
                manifest_path.display()
            )
        })?;
        validate_descriptor_path(&descriptor, manifest_path)?;
        Ok(Some(ProjectAccelerator {
            schema,
            descriptor,
            contract: None,
        }))
    }
}

fn validate_descriptor_path(value: &str, manifest_path: &Path) -> Result<(), String> {
    let path = Path::new(value);
    let unsafe_component = value.trim().is_empty()
        || value != value.trim()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir
                    | Component::CurDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        });
    if unsafe_component || path.extension().and_then(|value| value.to_str()) != Some("toml") {
        return Err(format!(
            "{}: [accelerator] descriptor `{value}` must be a package-relative .toml path without traversal",
            manifest_path.display()
        ));
    }
    Ok(())
}
