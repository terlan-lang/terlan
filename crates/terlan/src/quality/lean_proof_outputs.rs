//! Complete proof-track output routing; an owned producer never mixes staged/final files.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use crate::terlan_quality::QualityResult;

const VARIABLES: [&str; 3] = [
    "TERLAN_PROOF_REPRO_OUTPUT",
    "TERLAN_PROOF_TRACK_OUTPUT",
    "TERLAN_PROOF_BASELINE_OUTPUT",
];

/// The three reports published by one proof-track producer.
pub(super) struct ReportPaths {
    pub(super) replay: PathBuf,
    pub(super) track: PathBuf,
    pub(super) baseline: PathBuf,
}

impl ReportPaths {
    /// Ordinary development writes the same repository-local reports as before.
    pub(super) fn repository(root: &Path) -> Self {
        let directory = root.join("target/quality/proof-artifacts");
        Self {
            replay: directory.join("lean-proof-repro-report.json"),
            track: directory.join("lean-proof-track.json"),
            baseline: directory.join("lean-proof-baseline.tsv"),
        }
    }

    /// Admit the entire optional staging set before any proof or report execution.
    pub(super) fn capture(root: &Path) -> QualityResult<Self> {
        Self::from_lookup(root, |name| std::env::var_os(name))
    }

    fn from_lookup(
        root: &Path,
        lookup: impl FnMut(&str) -> Option<OsString>,
    ) -> QualityResult<Self> {
        let values = VARIABLES.map(lookup);
        if values.iter().all(Option::is_none) {
            return Ok(Self::repository(root));
        }
        let [Some(replay), Some(track), Some(baseline)] = values else {
            return Err("proof-track staging requires all three output variables".into());
        };
        let paths = [
            PathBuf::from(replay),
            PathBuf::from(track),
            PathBuf::from(baseline),
        ];
        validate_private_paths(root, &paths)?;
        let [replay, track, baseline] = paths;
        Ok(Self {
            replay,
            track,
            baseline,
        })
    }
}

/// Route a policy report to its ordinary path or an explicitly private output.
pub(crate) fn single(root: &Path, default: &str, variable: &str) -> QualityResult<PathBuf> {
    single_from_value(root, default, std::env::var_os(variable))
}

fn single_from_value(
    root: &Path,
    default: &str,
    value: Option<OsString>,
) -> QualityResult<PathBuf> {
    let Some(value) = value else {
        return Ok(root.join(default));
    };
    let path = PathBuf::from(value);
    validate_private_paths(root, std::slice::from_ref(&path))?;
    Ok(path)
}

fn validate_private_paths(root: &Path, paths: &[PathBuf]) -> QualityResult<()> {
    let root = std::fs::canonicalize(root).map_err(|error| error.to_string())?;
    let namespace = root.join("target/quality/preparation");
    let parent = paths[0]
        .parent()
        .ok_or("proof output has no staging directory")?;
    if !parent.starts_with(&namespace)
        || parent == namespace
        || !parent
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".work"))
        || std::fs::canonicalize(parent).map_err(|error| error.to_string())? != parent
    {
        return Err("proof outputs require a canonical private preparation workspace".into());
    }
    for (index, path) in paths.iter().enumerate() {
        if !path.is_absolute()
            || path.parent() != Some(parent)
            || path.file_name().is_none()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
            || paths[..index].contains(path)
        {
            return Err(
                "proof staging outputs must be distinct files in one private workspace".into(),
            );
        }
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
            Ok(_) => return Err("proof staging output already exists".into()),
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "lean_proof_outputs_test.rs"]
mod tests;
