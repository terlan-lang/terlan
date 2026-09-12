//! Bounded additive log scopes: a nested owner cannot detach its caller's log.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::PathBuf;

const PREFIX: &str = "terlan-process-scopes-v1:";
const MAX_SCOPES: usize = 8;
const MAX_SCOPE_BYTES: usize = 32 * 1024;

/// Resolve every path before the command changes its working directory.
pub(super) fn collect(values: impl IntoIterator<Item = OsString>) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for value in values {
        append(&value, 0, &mut paths)?;
    }
    Ok(paths)
}

fn append(value: &OsStr, depth: usize, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    if value.is_empty() || value.len() > MAX_SCOPE_BYTES || depth > MAX_SCOPES {
        return Err(io::Error::other("invalid process inventory scope budget"));
    }
    if let Some(encoded) = value.to_str().and_then(|text| text.strip_prefix(PREFIX)) {
        let values: Vec<String> = serde_json::from_str(encoded).map_err(io::Error::other)?;
        if values.is_empty() || values.len() > MAX_SCOPES {
            return Err(io::Error::other("invalid process inventory scope count"));
        }
        for value in values {
            append(OsStr::new(&value), depth + 1, paths)?;
        }
    } else {
        let path = std::path::absolute(value)?;
        if !paths.contains(&path) {
            if paths.len() == MAX_SCOPES {
                return Err(io::Error::other("too many process inventory scopes"));
            }
            paths.push(path);
        }
    }
    Ok(())
}

/// Flatten inherited scopes so ordinary descendants do not grow nesting depth.
pub(super) fn encode(paths: &[PathBuf]) -> io::Result<OsString> {
    if let [path] = paths {
        return Ok(path.as_os_str().to_owned());
    }
    let paths = paths
        .iter()
        .map(|path| {
            path.to_str()
                .ok_or_else(|| io::Error::other("nested process inventory paths require UTF-8"))
        })
        .collect::<io::Result<Vec<_>>>()?;
    let encoded = format!(
        "{PREFIX}{}",
        serde_json::to_string(&paths).map_err(io::Error::other)?
    );
    if encoded.len() > MAX_SCOPE_BYTES {
        return Err(io::Error::other(
            "process inventory scope exceeds byte budget",
        ));
    }
    Ok(encoded.into())
}
