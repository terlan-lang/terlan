//! Freeze producer environments and bind reusable receipts to their input bytes.
//!
//! Fingerprint all inherited variables, not just a hand-maintained flag list:
//! Cargo configuration and build scripts can consume arbitrary environment keys.
//! Only documented shell/Make scheduling and display bookkeeping is excluded.
//! Values (which can include credentials) are never emitted in diagnostics.
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::process::Command;

pub(super) struct Snapshot {
    entries: BTreeMap<OsString, OsString>,
    digest: String,
}

impl Snapshot {
    pub(super) fn capture() -> io::Result<Self> {
        Self::from_entries(std::env::vars_os())
    }

    fn from_entries(entries: impl IntoIterator<Item = (OsString, OsString)>) -> io::Result<Self> {
        let mut admitted = BTreeMap::new();
        let mut bytes = 0;
        for (index, (key, value)) in entries.into_iter().enumerate() {
            bytes += key.as_encoded_bytes().len() + value.as_encoded_bytes().len();
            if index >= 4096 || bytes > 1024 * 1024 {
                return Err(io::Error::other("owner environment exceeds budget"));
            }
            if key.is_empty()
                || key.as_encoded_bytes().contains(&b'=')
                || key.as_encoded_bytes().contains(&0)
                || value.as_encoded_bytes().contains(&0)
                || admitted.insert(key, value).is_some()
            {
                return Err(io::Error::other("invalid owner environment entry"));
            }
        }
        let mut digest = Sha256::new();
        digest.update(b"terlan.build-owner.environment.v1\0");
        for (key, value) in &admitted {
            if bookkeeping(key) {
                continue;
            }
            for field in [key, value] {
                let bytes = field.as_encoded_bytes();
                digest.update((bytes.len() as u64).to_le_bytes());
                digest.update(bytes);
            }
        }
        Ok(Self {
            entries: admitted,
            digest: digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        })
    }

    pub(super) fn digest(&self) -> &str {
        &self.digest
    }

    pub(super) fn configure(&self, command: &mut Command) {
        command.env_clear().envs(&self.entries);
    }
}

fn bookkeeping(key: &std::ffi::OsStr) -> bool {
    matches!(
        key.to_str(),
        Some(
            "_" | "SHLVL"
                | "MAKEFLAGS"
                | "MFLAGS"
                | "MAKELEVEL"
                | "MAKEOVERRIDES"
                | "GNUMAKEFLAGS"
                | "CARGO_MAKEFLAGS"
                | "CARGO_BUILD_JOBS"
                | "CARGO_TERM_COLOR"
        )
    )
}

#[cfg(test)]
#[path = "owner_environment_test.rs"]
mod tests;
