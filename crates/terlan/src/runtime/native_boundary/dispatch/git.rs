//! Byte-exact Git source-tree identity for typed repository tooling.

use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::Command;

use sha2::Digest;

use super::{DispatchError, NativeBoundaryValue};

const DOMAIN: &[u8] = b"terlan.source-tree.v1\0";

pub(super) fn source_tree_identity(
    operation: &str,
    root: &str,
) -> Result<NativeBoundaryValue, DispatchError> {
    let revision_bytes = git(operation, root, &["rev-parse", "HEAD"])?;
    let revision = std::str::from_utf8(&revision_bytes)
        .map_err(|_| error(operation, "Git revision is not UTF-8"))?
        .trim()
        .to_string();
    if revision.len() != 40
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error(
            operation,
            "checked-out source revision is not a full Git identity",
        ));
    }

    let tracked = git(
        operation,
        root,
        &[
            "diff",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
            &revision,
            "--",
        ],
    )?;
    let untracked_output = git(
        operation,
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    let mut untracked = untracked_output
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            std::str::from_utf8(path)
                .map(str::to_string)
                .map_err(|_| error(operation, "untracked source path is not valid UTF-8"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    untracked.sort();

    let mut digest = sha2::Sha256::new();
    digest.update(DOMAIN);
    update_framed(&mut digest, revision.as_bytes())?;
    update_framed(&mut digest, &tracked)?;
    digest.update(u64_length(untracked.len())?.to_be_bytes());
    for relative in &untracked {
        update_framed(&mut digest, relative.as_bytes())?;
        let path = Path::new(root).join(relative);
        let metadata = std::fs::symlink_metadata(&path).map_err(|failure| {
            error(
                operation,
                format!("failed to inspect untracked source `{relative}`: {failure}"),
            )
        })?;
        if metadata.file_type().is_symlink() {
            digest.update(b"L");
            let target = std::fs::read_link(&path).map_err(|failure| {
                error(
                    operation,
                    format!("failed to read untracked symlink `{relative}`: {failure}"),
                )
            })?;
            update_framed(&mut digest, os_bytes(target.as_os_str()))?;
        } else if metadata.is_file() {
            digest.update(b"F");
            update_framed_file(&mut digest, operation, relative, &path, metadata.len())?;
        } else {
            return Err(error(
                operation,
                format!("unsupported untracked source entry `{relative}`"),
            ));
        }
    }
    let sha256 = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(NativeBoundaryValue::Record {
        name: "SourceTreeIdentity".to_string(),
        fields: vec![
            ("revision".to_string(), NativeBoundaryValue::Text(revision)),
            (
                "clean".to_string(),
                NativeBoundaryValue::Bool(tracked.is_empty() && untracked.is_empty()),
            ),
            ("sha256".to_string(), NativeBoundaryValue::Text(sha256)),
        ],
    })
}

fn git(operation: &str, root: &str, arguments: &[&str]) -> Result<Vec<u8>, DispatchError> {
    let output = Command::new("git")
        .current_dir(root)
        .args(arguments)
        .output()
        .map_err(|failure| error(operation, format!("failed to launch Git: {failure}")))?;
    if !output.status.success() {
        return Err(error(
            operation,
            format!(
                "Git command failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    Ok(output.stdout)
}

fn update_framed(digest: &mut sha2::Sha256, value: &[u8]) -> Result<(), DispatchError> {
    digest.update(u64_length(value.len())?.to_be_bytes());
    digest.update(value);
    Ok(())
}

fn update_framed_file(
    digest: &mut sha2::Sha256,
    operation: &str,
    relative: &str,
    path: &Path,
    length: u64,
) -> Result<(), DispatchError> {
    digest.update(length.to_be_bytes());
    let mut source = std::fs::File::open(path).map_err(|failure| {
        error(
            operation,
            format!("failed to read untracked source `{relative}`: {failure}"),
        )
    })?;
    let mut buffer = [0u8; 65_536];
    loop {
        let count = source.read(&mut buffer).map_err(|failure| {
            error(
                operation,
                format!("failed to read untracked source `{relative}`: {failure}"),
            )
        })?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(())
}

fn u64_length(length: usize) -> Result<u64, DispatchError> {
    u64::try_from(length).map_err(|_| {
        error(
            "std.vcs.git.source_tree_identity",
            "source-tree field is too large",
        )
    })
}

fn error(operation: &str, message: impl Into<String>) -> DispatchError {
    let _ = operation;
    DispatchError::new("source_tree_identity_failed", message, 0)
}

#[cfg(unix)]
fn os_bytes(value: &OsStr) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes()
}

#[cfg(not(unix))]
fn os_bytes(value: &OsStr) -> &[u8] {
    value.as_encoded_bytes()
}
