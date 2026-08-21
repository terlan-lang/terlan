//! Canonical semantic-version policy shared by Registry publication and selection.

use std::fmt;

use semver::{Version, VersionReq};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VersionError(String);

impl fmt::Display for VersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for VersionError {}

pub(crate) fn canonical_version(source: &str) -> Result<Version, VersionError> {
    let version = Version::parse(source).map_err(|error| {
        VersionError(format!(
            "error[registry_semver]: `{source}` is invalid: {error}"
        ))
    })?;
    if version.to_string() != source {
        return Err(VersionError(format!(
            "error[registry_semver]: `{source}` is not canonical; use `{version}`"
        )));
    }
    Ok(version)
}

pub(crate) fn parse_requirement(source: &str) -> Result<VersionReq, VersionError> {
    VersionReq::parse(source).map_err(|error| {
        VersionError(format!(
            "error[registry_requirement]: `{source}` is invalid: {error}"
        ))
    })
}

pub(crate) fn requirement_matches(requirement: &str, version: &str) -> Result<bool, VersionError> {
    let requirement = parse_requirement(requirement)?;
    Ok(requirement.matches(&canonical_version(version)?))
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) fn latest_stable<'a>(versions: impl Iterator<Item = &'a str>) -> Option<String> {
    versions
        .filter_map(|source| {
            canonical_version(source)
                .ok()
                .map(|version| (version, source))
        })
        .filter(|(version, _)| version.pre.is_empty())
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, source)| source.to_owned())
}

#[cfg(test)]
#[path = "version_test.rs"]
mod tests;
