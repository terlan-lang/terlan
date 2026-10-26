//! Project manifest section parsing and quoted comments.

use super::*;

/// Removes unquoted line comments from one manifest line.
///
/// Inputs:
/// - `line`: one raw source line.
///
/// Output:
/// - Slice before the first unquoted `#`, or the whole line when there is no
///   comment.
///
/// Transformation:
/// - Scans quote and escape state so `#` inside strings is preserved.
pub(super) fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

/// Parses a manifest section header.
///
/// Inputs:
/// - `line`: trimmed source line beginning with `[`.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Supported `ProjectManifestSection`.
///
/// Transformation:
/// - Accepts exact `[package]`, `[build]`, `[dependencies]`,
///   `[web.assets]`, `[scripts]`, `[server]`, `[serve]`, `[server.tls]`, and
///   `[target.<name>.dependencies]`
///   section headers.
pub(super) fn parse_section(
    line: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectManifestSection, String> {
    let section = line
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .ok_or_else(|| {
            format!(
                "{}:{}: malformed project manifest section",
                path.display(),
                line_no
            )
        })?;
    match section.trim() {
        "package" => Ok(ProjectManifestSection::Package),
        "build" => Ok(ProjectManifestSection::Build),
        "web.assets" => Ok(ProjectManifestSection::WebAssets),
        "scripts" => Ok(ProjectManifestSection::Scripts),
        "server" => Ok(ProjectManifestSection::Server),
        "serve" => Ok(ProjectManifestSection::Serve),
        "server.tls" => Ok(ProjectManifestSection::ServerTls),
        "native.rust" => Ok(ProjectManifestSection::NativeRust),
        "accelerator" => Ok(ProjectManifestSection::Accelerator),
        "deploy" => Ok(ProjectManifestSection::Deploy),
        "deploy.health" => Ok(ProjectManifestSection::DeployHealth),
        "deploy.resources" => Ok(ProjectManifestSection::DeployResources),
        "dependencies" => Ok(ProjectManifestSection::Dependencies),
        "target.wasm" => Ok(ProjectManifestSection::TargetWasm),
        "target.wasi" => Ok(ProjectManifestSection::TargetWasi),
        other => {
            if other.starts_with("integration.") && other["integration.".len()..].trim().is_empty()
            {
                Err(format!(
                    "{}:{}: unsupported project manifest section `{}`",
                    path.display(),
                    line_no,
                    other
                ))
            } else if other.starts_with("integration.") {
                Ok(ProjectManifestSection::IntegrationFlow)
            } else if let Some(target) = parse_target_dependency_section(other) {
                Ok(ProjectManifestSection::TargetDependencies(target))
            } else {
                Err(format!(
                    "{}:{}: unsupported project manifest section `{}`",
                    path.display(),
                    line_no,
                    other
                ))
            }
        }
    }
}
