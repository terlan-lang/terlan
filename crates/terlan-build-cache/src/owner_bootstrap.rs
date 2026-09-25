//! Finish the first support build, when its receipt owner did not exist yet.
//!
//! The Make bootstrap holds the same exclusive lease across Cargo and this
//! finalizer, checks Cargo's exit status, and rechecks clean source identity.
//! This is local reuse metadata, not an attestation for untrusted build output.
use super::{output_hashes, owned_file_path, Options, MAX_RECEIPT_BYTES};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub(super) fn validate(options: &Options) -> io::Result<String> {
    let log = options
        .completed_cargo_log
        .as_ref()
        .ok_or_else(|| io::Error::other("missing bootstrap Cargo observation"))?;
    let expected: BTreeSet<_> = [
        PathBuf::from("target/debug/terlan-build-cache"),
        PathBuf::from("target/debug/terlan-test-orchestrator"),
    ]
    .into_iter()
    .collect();
    if options.outputs.len() != expected.len()
        || options.outputs.iter().cloned().collect::<BTreeSet<_>>() != expected
        || log.is_absolute()
    {
        return Err(io::Error::other("invalid support bootstrap outputs or log"));
    }
    let path = options.root.join(log);
    owned_file_path(&options.root, &path)?;
    let mut source = Vec::new();
    File::open(path)?
        .take(MAX_RECEIPT_BYTES + 1)
        .read_to_end(&mut source)?;
    if source.len() as u64 > MAX_RECEIPT_BYTES {
        return Err(io::Error::other(
            "bootstrap Cargo observation exceeds limit",
        ));
    }
    let mut artifacts = BTreeSet::new();
    let mut finished = false;
    for line in source
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        if finished {
            return Err(io::Error::other("data after Cargo build-finished"));
        }
        let row: Value = serde_json::from_slice(line)?;
        match row["reason"].as_str() {
            Some("build-finished") if row["success"] == true => finished = true,
            Some("compiler-artifact") => {
                if let Some(executable) = row["executable"].as_str() {
                    let absolute = Path::new(executable);
                    let relative = absolute.strip_prefix(&options.root).map_err(|_| {
                        io::Error::other("bootstrap artifact outside selected repository")
                    })?;
                    if !expected.contains(relative)
                        || row["target"]["kind"] != serde_json::json!(["bin"])
                        || row["target"]["name"].as_str()
                            != relative.file_name().and_then(|name| name.to_str())
                        || row["profile"]["test"] != false
                        || !artifacts.insert(relative.to_path_buf())
                    {
                        return Err(io::Error::other("invalid bootstrap Cargo artifact"));
                    }
                }
            }
            Some("compiler-message" | "build-script-executed") => {}
            _ => return Err(io::Error::other("invalid bootstrap Cargo completion")),
        }
    }
    if !finished || artifacts != expected {
        return Err(io::Error::other("incomplete bootstrap Cargo observation"));
    }
    let hashes = output_hashes(&options.root, std::slice::from_ref(log))?;
    hashes
        .into_values()
        .next()
        .ok_or_else(|| io::Error::other("missing bootstrap observation hash"))
}

#[cfg(test)]
#[path = "owner_bootstrap_test.rs"]
mod tests;
