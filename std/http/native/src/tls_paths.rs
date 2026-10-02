//! Local deployment-file admission shared by package checks and HTTPS startup.

use crate::tls_config::{Config, Mode};
use std::path::{Component, Path};

/// Checks existing manual cert/key/CA paths, including symlink containment.
/// These are admission checks, not a race-free directory capability: the project
/// directory must remain host-controlled between validation and material loading.
pub fn validate_manual_tls_file_references(
    root: &Path,
    tls: &Config,
) -> Result<(), crate::ServiceError> {
    if tls.mode != Mode::Manual {
        return Ok(());
    }
    for (field, value) in [
        ("cert", tls.cert.as_deref()),
        ("key", tls.key.as_deref()),
        ("ca", tls.ca.as_deref()),
    ] {
        if let Some(value) = value {
            validate_manual_tls_file_reference(root, field, value)?;
        }
    }
    Ok(())
}

fn validate_manual_tls_file_reference(
    root: &Path,
    field: &str,
    value: &str,
) -> Result<(), crate::ServiceError> {
    let relative = Path::new(value);
    let escaped = || {
        format!(
        "error[serve_package]: [server.tls] manual {field} path `{value}` must be project-relative and stay inside the project"
    )
    };
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(escaped().into());
    }
    let full_path = root.join(relative);
    if !full_path.is_file() {
        return Err(format!(
            "error[serve_package]: [server.tls] manual {field} file `{}` does not exist",
            full_path.display()
        )
        .into());
    }
    let resolve = |path: &Path| {
        path.canonicalize().map_err(|err| {
            format!(
                "error[serve_package]: failed to resolve manual TLS path `{}`: {err}",
                path.display()
            )
        })
    };
    if !resolve(&full_path)?.starts_with(resolve(root)?) {
        return Err(escaped().into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "tls_paths_test.rs"]
mod tests;
