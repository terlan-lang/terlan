use std::collections::BTreeSet;
use std::path::{Component, Path};

use super::model::{
    ProjectDeployHealth, ProjectDeployResources, ProjectDeployment, ProjectRollbackCompatibility,
};

/// Incremental parser state for `[deploy]`.
#[derive(Debug, Default)]
pub(super) struct ProjectDeploymentBuilder {
    pub(super) seen: bool,
    pub(super) environment: Option<Vec<String>>,
    pub(super) secrets: Option<Vec<String>>,
    pub(super) migrations: Option<Vec<String>>,
    pub(super) outbound_network: Option<Vec<String>>,
    pub(super) rollback: Option<ProjectRollbackCompatibility>,
}

impl ProjectDeploymentBuilder {
    pub(super) fn finish(
        self,
        health: ProjectDeployHealthBuilder,
        resources: ProjectDeployResourcesBuilder,
        path: &Path,
    ) -> Result<Option<ProjectDeployment>, String> {
        if !self.seen && !health.seen && !resources.seen {
            return Ok(None);
        }
        let environment = self.environment.unwrap_or_default();
        let secrets = self.secrets.unwrap_or_default();
        validate_names("environment", &environment, path)?;
        validate_names("secrets", &secrets, path)?;
        for secret in &secrets {
            if !environment.contains(secret) {
                return Err(format!(
                    "{}: project manifest [deploy] secret `{secret}` must also appear in environment",
                    path.display()
                ));
            }
        }
        let migrations = self.migrations.unwrap_or_default();
        validate_relative_paths("migrations", &migrations, path)?;
        let outbound_network = self.outbound_network.unwrap_or_default();
        validate_outbound_network(&outbound_network, path)?;
        Ok(Some(ProjectDeployment {
            environment,
            secrets,
            migrations,
            outbound_network,
            rollback: self
                .rollback
                .unwrap_or(ProjectRollbackCompatibility::Stateless),
            health: health.finish(path)?,
            resources: resources.finish(path)?,
        }))
    }
}

/// Incremental parser state for `[deploy.health]`.
#[derive(Debug, Default)]
pub(super) struct ProjectDeployHealthBuilder {
    pub(super) seen: bool,
    pub(super) path: Option<String>,
    pub(super) interval_secs: Option<u64>,
    pub(super) timeout_secs: Option<u64>,
}

impl ProjectDeployHealthBuilder {
    fn finish(self, manifest_path: &Path) -> Result<Option<ProjectDeployHealth>, String> {
        if !self.seen {
            return Ok(None);
        }
        let path = self.path.ok_or_else(|| {
            format!(
                "{}: project manifest [deploy.health] requires path",
                manifest_path.display()
            )
        })?;
        if !path.starts_with('/') || path.contains('?') || path.contains('#') {
            return Err(format!(
                "{}: project manifest [deploy.health] path must be an absolute URL path without query or fragment",
                manifest_path.display()
            ));
        }
        let interval_secs = require_positive(
            "[deploy.health] interval_secs",
            self.interval_secs.unwrap_or(10),
            manifest_path,
        )?;
        let timeout_secs = require_positive(
            "[deploy.health] timeout_secs",
            self.timeout_secs.unwrap_or(2),
            manifest_path,
        )?;
        if timeout_secs >= interval_secs {
            return Err(format!(
                "{}: project manifest [deploy.health] timeout_secs must be less than interval_secs",
                manifest_path.display()
            ));
        }
        Ok(Some(ProjectDeployHealth {
            path,
            interval_secs,
            timeout_secs,
        }))
    }
}

/// Incremental parser state for `[deploy.resources]`.
#[derive(Debug, Default)]
pub(super) struct ProjectDeployResourcesBuilder {
    pub(super) seen: bool,
    pub(super) cpu_millis: Option<u64>,
    pub(super) memory_mb: Option<u64>,
    pub(super) processes: Option<u64>,
}

impl ProjectDeployResourcesBuilder {
    fn finish(self, path: &Path) -> Result<Option<ProjectDeployResources>, String> {
        if !self.seen {
            return Ok(None);
        }
        Ok(Some(ProjectDeployResources {
            cpu_millis: require_positive(
                "[deploy.resources] cpu_millis",
                self.cpu_millis.unwrap_or(250),
                path,
            )?,
            memory_mb: require_positive(
                "[deploy.resources] memory_mb",
                self.memory_mb.unwrap_or(256),
                path,
            )?,
            processes: require_positive(
                "[deploy.resources] processes",
                self.processes.unwrap_or(1),
                path,
            )?,
        }))
    }
}

pub(super) fn parse_rollback_compatibility(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectRollbackCompatibility, String> {
    match super::strings::parse_string(value, path, line_no)?.as_str() {
        "stateless" => Ok(ProjectRollbackCompatibility::Stateless),
        "migration-compatible" => Ok(ProjectRollbackCompatibility::MigrationCompatible),
        "manual" => Ok(ProjectRollbackCompatibility::Manual),
        other => Err(format!(
            "{}:{line_no}: unsupported [deploy] rollback `{other}`; supported values: stateless, migration-compatible, manual",
            path.display()
        )),
    }
}

fn validate_names(label: &str, names: &[String], path: &Path) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for name in names {
        let mut chars = name.chars();
        if !chars
            .next()
            .is_some_and(|first| first.is_ascii_uppercase() || first == '_')
            || chars.any(|ch| !(ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'))
        {
            return Err(format!(
                "{}: project manifest [deploy] {label} name `{name}` must use POSIX environment-name syntax",
                path.display()
            ));
        }
        if !seen.insert(name) {
            return Err(format!(
                "{}: project manifest [deploy] {label} contains duplicate `{name}`",
                path.display()
            ));
        }
    }
    Ok(())
}

fn validate_relative_paths(label: &str, values: &[String], path: &Path) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for value in values {
        let candidate = Path::new(value);
        if value.trim().is_empty()
            || candidate.is_absolute()
            || candidate
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
        {
            return Err(format!(
                "{}: project manifest [deploy] {label} entry `{value}` must be a package-relative path without `..`",
                path.display()
            ));
        }
        if !seen.insert(value) {
            return Err(format!(
                "{}: project manifest [deploy] {label} contains duplicate `{value}`",
                path.display()
            ));
        }
    }
    Ok(())
}

fn validate_outbound_network(values: &[String], path: &Path) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for value in values {
        let valid = !value.is_empty()
            && !value.contains("//")
            && !value.contains('/')
            && !value.chars().any(char::is_whitespace)
            && value.rsplit_once(':').is_some_and(|(host, port)| {
                !host.is_empty() && port.parse::<u16>().is_ok_and(|parsed| parsed > 0)
            });
        if !valid {
            return Err(format!(
                "{}: project manifest [deploy] outbound_network entry `{value}` must use host:port without a URL scheme or path",
                path.display()
            ));
        }
        if !seen.insert(value) {
            return Err(format!(
                "{}: project manifest [deploy] outbound_network contains duplicate `{value}`",
                path.display()
            ));
        }
    }
    Ok(())
}

fn require_positive(label: &str, value: u64, path: &Path) -> Result<u64, String> {
    if value == 0 {
        return Err(format!(
            "{}: project manifest {label} must be greater than zero",
            path.display()
        ));
    }
    Ok(value)
}
