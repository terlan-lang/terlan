use std::path::Path;

/// Computes the module name implied by a source-root-relative file path.
///
/// Inputs:
/// - `root`: source root used for directory compilation.
/// - `file`: implementation source path.
///
/// Output:
/// - Dotted module name implied by the relative `.terl` path.
/// - `Err(message)` when the path cannot be represented as canonical source
///   layout input.
///
/// Transformation:
/// - Removes the source root prefix, drops the `.terl` extension from the final
///   path segment, validates UTF-8 path segments, and joins all relative
///   segments with dots.
pub(crate) fn expected_module_name_for_source_path(
    root: &Path,
    file: &Path,
) -> Result<String, String> {
    let relative = file.strip_prefix(root).map_err(|_| {
        format!(
            "source file `{}` is not under source root `{}`",
            file.display(),
            root.display()
        )
    })?;
    let mut segments = Vec::new();
    for component in relative.components() {
        let value = component.as_os_str().to_str().ok_or_else(|| {
            format!(
                "source path `{}` contains a non-UTF-8 module segment",
                file.display()
            )
        })?;
        segments.push(value.to_string());
    }
    let last = segments
        .last_mut()
        .ok_or_else(|| format!("source path `{}` has no module file name", file.display()))?;
    if !last.ends_with(".terl") {
        return Err(format!(
            "source path `{}` is not a Terlan implementation source",
            file.display()
        ));
    }
    last.truncate(last.len() - ".terl".len());
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err(format!(
            "source path `{}` contains an empty module segment",
            file.display()
        ));
    }
    Ok(segments.join("."))
}
